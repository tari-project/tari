// Copyright 2019. The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{
    convert::{TryFrom, TryInto},
    sync::{Arc, RwLock},
};

use chacha20poly1305::XChaCha20Poly1305;
use chrono::NaiveDateTime;
use derivative::Derivative;
use diesel::{
    prelude::*,
    r2d2::{ConnectionManager, PooledConnection},
    result::Error as DieselError,
    SqliteConnection,
};
use log::*;
pub use new_output_sql::NewOutputSql;
pub use output_sql::OutputSql;
use tari_common_types::{
    transaction::TxId,
    types::{Commitment, FixedHash, PrivateKey},
};
use tari_core::transactions::transaction_components::{OutputType, TransactionOutput};
use tari_crypto::tari_utilities::{hex::Hex, ByteArray};
use tari_script::{ExecutionStack, TariScript};
use tokio::time::Instant;

use crate::{
    output_manager_service::{
        error::OutputManagerStorageError,
        service::Balance,
        storage::{
            database::{DbKey, DbKeyValuePair, DbValue, OutputBackendQuery, OutputManagerBackend, WriteOperation},
            models::{DbUnblindedOutput, KnownOneSidedPaymentScript},
            OutputStatus,
        },
        UtxoSelectionCriteria,
    },
    schema::{known_one_sided_payment_scripts, outputs},
    storage::sqlite_utilities::wallet_db_connection::WalletDbConnection,
    util::{
        diesel_ext::ExpectedRowsExtension,
        encryption::{decrypt_bytes_integral_nonce, encrypt_bytes_integral_nonce, Encryptable},
    },
};
mod new_output_sql;
mod output_sql;
const LOG_TARGET: &str = "wallet::output_manager_service::database::wallet";

/// A Sqlite backend for the Output Manager Service. The Backend is accessed via a connection pool to the Sqlite file.
#[derive(Clone)]
pub struct OutputManagerSqliteDatabase {
    database_connection: WalletDbConnection,
    cipher: Arc<RwLock<Option<XChaCha20Poly1305>>>,
}

impl OutputManagerSqliteDatabase {
    pub fn new(database_connection: WalletDbConnection, cipher: Option<XChaCha20Poly1305>) -> Self {
        Self {
            database_connection,
            cipher: Arc::new(RwLock::new(cipher)),
        }
    }

    fn decrypt_if_necessary<T: Encryptable<XChaCha20Poly1305>>(
        &self,
        o: &mut T,
    ) -> Result<(), OutputManagerStorageError> {
        let cipher = acquire_read_lock!(self.cipher);
        if let Some(cipher) = cipher.as_ref() {
            o.decrypt(cipher)
                .map_err(|_| OutputManagerStorageError::AeadError("Decryption Error".to_string()))?;
        }
        Ok(())
    }

    fn encrypt_if_necessary<T: Encryptable<XChaCha20Poly1305>>(
        &self,
        o: &mut T,
    ) -> Result<(), OutputManagerStorageError> {
        let cipher = acquire_read_lock!(self.cipher);
        if let Some(cipher) = cipher.as_ref() {
            o.encrypt(cipher)
                .map_err(|_| OutputManagerStorageError::AeadError("Encryption Error".to_string()))?;
        }
        Ok(())
    }

    fn insert(&self, key_value_pair: DbKeyValuePair, conn: &SqliteConnection) -> Result<(), OutputManagerStorageError> {
        match key_value_pair {
            DbKeyValuePair::UnspentOutput(c, o) => {
                if OutputSql::find_by_commitment_and_cancelled(&c.to_vec(), false, conn).is_ok() {
                    return Err(OutputManagerStorageError::DuplicateOutput);
                }
                let mut new_output = NewOutputSql::new(*o, OutputStatus::Unspent, None, None)?;
                self.encrypt_if_necessary(&mut new_output)?;
                new_output.commit(conn)?
            },
            DbKeyValuePair::UnspentOutputWithTxId(c, (tx_id, o)) => {
                if OutputSql::find_by_commitment_and_cancelled(&c.to_vec(), false, conn).is_ok() {
                    return Err(OutputManagerStorageError::DuplicateOutput);
                }
                let mut new_output = NewOutputSql::new(*o, OutputStatus::Unspent, Some(tx_id), None)?;
                self.encrypt_if_necessary(&mut new_output)?;
                new_output.commit(conn)?
            },
            DbKeyValuePair::OutputToBeReceived(c, (tx_id, o, coinbase_block_height)) => {
                if OutputSql::find_by_commitment_and_cancelled(&c.to_vec(), false, conn).is_ok() {
                    return Err(OutputManagerStorageError::DuplicateOutput);
                }
                let mut new_output = NewOutputSql::new(
                    *o,
                    OutputStatus::EncumberedToBeReceived,
                    Some(tx_id),
                    coinbase_block_height,
                )?;
                self.encrypt_if_necessary(&mut new_output)?;
                new_output.commit(conn)?
            },

            DbKeyValuePair::KnownOneSidedPaymentScripts(script) => {
                let mut script_sql = KnownOneSidedPaymentScriptSql::from(script);
                if KnownOneSidedPaymentScriptSql::find(&script_sql.script_hash, conn).is_ok() {
                    return Err(OutputManagerStorageError::DuplicateScript);
                }
                self.encrypt_if_necessary(&mut script_sql)?;
                script_sql.commit(conn)?
            },
        }
        Ok(())
    }
}

impl OutputManagerBackend for OutputManagerSqliteDatabase {
    #[allow(clippy::cognitive_complexity)]
    #[allow(clippy::too_many_lines)]
    fn fetch(&self, key: &DbKey) -> Result<Option<DbValue>, OutputManagerStorageError> {
        let start = Instant::now();
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();

        let result = match key {
            DbKey::SpentOutput(k) => match OutputSql::find_status(&k.to_vec(), OutputStatus::Spent, &conn) {
                Ok(mut o) => {
                    self.decrypt_if_necessary(&mut o)?;
                    Some(DbValue::SpentOutput(Box::new(DbUnblindedOutput::try_from(o)?)))
                },
                Err(e) => {
                    match e {
                        OutputManagerStorageError::DieselError(DieselError::NotFound) => (),
                        e => return Err(e),
                    };
                    None
                },
            },
            DbKey::UnspentOutput(k) => match OutputSql::find_status(&k.to_vec(), OutputStatus::Unspent, &conn) {
                Ok(mut o) => {
                    self.decrypt_if_necessary(&mut o)?;
                    Some(DbValue::UnspentOutput(Box::new(DbUnblindedOutput::try_from(o)?)))
                },
                Err(e) => {
                    match e {
                        OutputManagerStorageError::DieselError(DieselError::NotFound) => (),
                        e => return Err(e),
                    };
                    None
                },
            },
            DbKey::UnspentOutputHash(hash) => {
                match OutputSql::find_by_hash(hash.as_slice(), OutputStatus::Unspent, &(*conn)) {
                    Ok(mut o) => {
                        self.decrypt_if_necessary(&mut o)?;
                        Some(DbValue::UnspentOutput(Box::new(DbUnblindedOutput::try_from(o)?)))
                    },
                    Err(e) => {
                        match e {
                            OutputManagerStorageError::DieselError(DieselError::NotFound) => (),
                            e => return Err(e),
                        };
                        None
                    },
                }
            },
            DbKey::AnyOutputByCommitment(commitment) => {
                match OutputSql::find_by_commitment(&commitment.to_vec(), &conn) {
                    Ok(mut o) => {
                        self.decrypt_if_necessary(&mut o)?;
                        Some(DbValue::AnyOutput(Box::new(DbUnblindedOutput::try_from(o)?)))
                    },
                    Err(e) => {
                        match e {
                            OutputManagerStorageError::DieselError(DieselError::NotFound) => (),
                            e => return Err(e),
                        };
                        None
                    },
                }
            },
            DbKey::OutputsByTxIdAndStatus(tx_id, status) => {
                let mut outputs = OutputSql::find_by_tx_id_and_status(*tx_id, *status, &conn)?;
                for o in &mut outputs {
                    self.decrypt_if_necessary(o)?;
                }
                Some(DbValue::AnyOutputs(
                    outputs
                        .iter()
                        .map(|o| DbUnblindedOutput::try_from(o.clone()))
                        .collect::<Result<Vec<_>, _>>()?,
                ))
            },
            DbKey::UnspentOutputs => {
                let mut outputs = OutputSql::index_status(OutputStatus::Unspent, &conn)?;
                for o in &mut outputs {
                    self.decrypt_if_necessary(o)?;
                }

                Some(DbValue::UnspentOutputs(
                    outputs
                        .iter()
                        .map(|o| DbUnblindedOutput::try_from(o.clone()))
                        .collect::<Result<Vec<_>, _>>()?,
                ))
            },
            DbKey::SpentOutputs => {
                let mut outputs = OutputSql::index_status(OutputStatus::Spent, &conn)?;
                for o in &mut outputs {
                    self.decrypt_if_necessary(o)?;
                }

                Some(DbValue::SpentOutputs(
                    outputs
                        .iter()
                        .map(|o| DbUnblindedOutput::try_from(o.clone()))
                        .collect::<Result<Vec<_>, _>>()?,
                ))
            },
            DbKey::TimeLockedUnspentOutputs(tip) => {
                let mut outputs = OutputSql::index_time_locked(*tip, &conn)?;
                for o in &mut outputs {
                    self.decrypt_if_necessary(o)?;
                }

                Some(DbValue::UnspentOutputs(
                    outputs
                        .iter()
                        .map(|o| DbUnblindedOutput::try_from(o.clone()))
                        .collect::<Result<Vec<_>, _>>()?,
                ))
            },
            DbKey::InvalidOutputs => {
                let mut outputs = OutputSql::index_status(OutputStatus::Invalid, &conn)?;
                for o in &mut outputs {
                    self.decrypt_if_necessary(o)?;
                }

                Some(DbValue::InvalidOutputs(
                    outputs
                        .iter()
                        .map(|o| DbUnblindedOutput::try_from(o.clone()))
                        .collect::<Result<Vec<_>, _>>()?,
                ))
            },
            DbKey::KnownOneSidedPaymentScripts => {
                let mut known_one_sided_payment_scripts = KnownOneSidedPaymentScriptSql::index(&conn)?;
                for script in &mut known_one_sided_payment_scripts {
                    self.decrypt_if_necessary(script)?;
                }

                Some(DbValue::KnownOneSidedPaymentScripts(
                    known_one_sided_payment_scripts
                        .iter()
                        .map(|script| KnownOneSidedPaymentScript::try_from(script.clone()))
                        .collect::<Result<Vec<_>, _>>()?,
                ))
            },
        };
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - fetch '{}': lock {} + db_op {} = {} ms",
                key,
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }

        Ok(result)
    }

    fn fetch_with_features(
        &self,
        output_type: OutputType,
    ) -> Result<Vec<DbUnblindedOutput>, OutputManagerStorageError> {
        let conn = self.database_connection.get_pooled_connection()?;
        let mut outputs = OutputSql::index_by_output_type(output_type, &conn)?;
        for o in &mut outputs {
            self.decrypt_if_necessary(o)?;
        }

        outputs
            .iter()
            .map(|o| DbUnblindedOutput::try_from(o.clone()))
            .collect::<Result<Vec<_>, _>>()
    }

    fn fetch_sorted_unspent_outputs(&self) -> Result<Vec<DbUnblindedOutput>, OutputManagerStorageError> {
        let conn = self.database_connection.get_pooled_connection()?;
        let mut outputs = OutputSql::index_unspent(&conn)?;
        for output in &mut outputs {
            self.decrypt_if_necessary(output)?;
        }

        outputs
            .into_iter()
            .map(DbUnblindedOutput::try_from)
            .collect::<Result<Vec<_>, _>>()
    }

    fn fetch_mined_unspent_outputs(&self) -> Result<Vec<DbUnblindedOutput>, OutputManagerStorageError> {
        let start = Instant::now();
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let mut outputs = OutputSql::index_marked_deleted_in_block_is_null(&conn)?;
        for output in &mut outputs {
            self.decrypt_if_necessary(output)?;
        }
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - fetch_mined_unspent_outputs: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }

        outputs
            .into_iter()
            .map(DbUnblindedOutput::try_from)
            .collect::<Result<Vec<_>, _>>()
    }

    fn fetch_unspent_mined_unconfirmed_outputs(&self) -> Result<Vec<DbUnblindedOutput>, OutputManagerStorageError> {
        let start = Instant::now();
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let mut outputs = OutputSql::index_unconfirmed(&conn)?;
        for output in &mut outputs {
            self.decrypt_if_necessary(output)?;
        }
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - fetch_unspent_mined_unconfirmed_outputs: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }

        outputs
            .into_iter()
            .map(DbUnblindedOutput::try_from)
            .collect::<Result<Vec<_>, _>>()
    }

    fn write(&self, op: WriteOperation) -> Result<Option<DbValue>, OutputManagerStorageError> {
        let start = Instant::now();
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();

        let mut msg = "".to_string();
        let result = match op {
            WriteOperation::Insert(kvp) => {
                msg.push_str("Insert");
                self.insert(kvp, &conn)?;
                Ok(None)
            },
            WriteOperation::Remove(k) => match k {
                DbKey::AnyOutputByCommitment(commitment) => {
                    conn.transaction::<_, _, _>(|| {
                        msg.push_str("Remove");
                        // Used by coinbase when mining.
                        match OutputSql::find_by_commitment(&commitment.to_vec(), &conn) {
                            Ok(mut o) => {
                                o.delete(&conn)?;
                                self.decrypt_if_necessary(&mut o)?;
                                Ok(Some(DbValue::AnyOutput(Box::new(DbUnblindedOutput::try_from(o)?))))
                            },
                            Err(e) => match e {
                                OutputManagerStorageError::DieselError(DieselError::NotFound) => Ok(None),
                                e => Err(e),
                            },
                        }
                    })
                },
                DbKey::SpentOutput(_s) => Err(OutputManagerStorageError::OperationNotSupported),
                DbKey::UnspentOutputHash(_h) => Err(OutputManagerStorageError::OperationNotSupported),
                DbKey::UnspentOutput(_k) => Err(OutputManagerStorageError::OperationNotSupported),
                DbKey::UnspentOutputs => Err(OutputManagerStorageError::OperationNotSupported),
                DbKey::SpentOutputs => Err(OutputManagerStorageError::OperationNotSupported),
                DbKey::InvalidOutputs => Err(OutputManagerStorageError::OperationNotSupported),
                DbKey::TimeLockedUnspentOutputs(_) => Err(OutputManagerStorageError::OperationNotSupported),
                DbKey::KnownOneSidedPaymentScripts => Err(OutputManagerStorageError::OperationNotSupported),
                DbKey::OutputsByTxIdAndStatus(_, _) => Err(OutputManagerStorageError::OperationNotSupported),
            },
        };
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - write {}: lock {} + db_op {} = {} ms",
                msg,
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }

        result
    }

    fn fetch_pending_incoming_outputs(&self) -> Result<Vec<DbUnblindedOutput>, OutputManagerStorageError> {
        let start = Instant::now();
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();

        let mut outputs = OutputSql::index_status(OutputStatus::EncumberedToBeReceived, &conn)?;
        outputs.extend(OutputSql::index_status(
            OutputStatus::ShortTermEncumberedToBeReceived,
            &conn,
        )?);
        outputs.extend(OutputSql::index_status(OutputStatus::UnspentMinedUnconfirmed, &conn)?);
        for o in &mut outputs {
            self.decrypt_if_necessary(o)?;
        }
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - fetch_pending_incoming_outputs: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }
        outputs
            .iter()
            .map(|o| DbUnblindedOutput::try_from(o.clone()))
            .collect::<Result<Vec<_>, _>>()
    }

    fn set_received_output_mined_height_and_status(
        &self,
        hash: FixedHash,
        mined_height: u64,
        mined_in_block: FixedHash,
        mmr_position: u64,
        confirmed: bool,
        mined_timestamp: u64,
    ) -> Result<(), OutputManagerStorageError> {
        let start = Instant::now();
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let status = if confirmed {
            OutputStatus::Unspent as i32
        } else {
            OutputStatus::UnspentMinedUnconfirmed as i32
        };
        debug!(
            target: LOG_TARGET,
            "`set_received_output_mined_height` status: {}", status
        );
        let hash = hash.to_vec();
        let mined_in_block = mined_in_block.to_vec();
        diesel::update(outputs::table.filter(outputs::hash.eq(hash)))
            .set((
                outputs::mined_height.eq(mined_height as i64),
                outputs::mined_in_block.eq(mined_in_block),
                outputs::mined_mmr_position.eq(mmr_position as i64),
                outputs::status.eq(status),
                outputs::mined_timestamp.eq(NaiveDateTime::from_timestamp(mined_timestamp as i64, 0)),
                outputs::marked_deleted_at_height.eq::<Option<i64>>(None),
                outputs::marked_deleted_in_block.eq::<Option<Vec<u8>>>(None),
            ))
            .execute(&conn)
            .num_rows_affected_or_not_found(1)?;
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - set_received_output_mined_height: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }

        Ok(())
    }

    fn set_output_to_unmined_and_invalid(&self, hash: FixedHash) -> Result<(), OutputManagerStorageError> {
        let start = Instant::now();
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let hash = hash.to_vec();
        diesel::update(outputs::table.filter(outputs::hash.eq(hash)))
            .set((
                outputs::mined_height.eq::<Option<i64>>(None),
                outputs::mined_in_block.eq::<Option<Vec<u8>>>(None),
                outputs::mined_mmr_position.eq::<Option<i64>>(None),
                outputs::status.eq(OutputStatus::Invalid as i32),
                outputs::mined_timestamp.eq::<Option<NaiveDateTime>>(None),
                outputs::marked_deleted_at_height.eq::<Option<i64>>(None),
                outputs::marked_deleted_in_block.eq::<Option<Vec<u8>>>(None),
            ))
            .execute(&conn)
            .num_rows_affected_or_not_found(1)?;
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - set_output_to_unmined: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }

        Ok(())
    }

    fn set_outputs_to_be_revalidated(&self) -> Result<(), OutputManagerStorageError> {
        let start = Instant::now();
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let result = diesel::update(outputs::table)
            .set((
                outputs::mined_height.eq::<Option<i64>>(None),
                outputs::mined_in_block.eq::<Option<Vec<u8>>>(None),
                outputs::mined_mmr_position.eq::<Option<i64>>(None),
                outputs::status.eq(OutputStatus::Invalid as i32),
                outputs::mined_timestamp.eq::<Option<NaiveDateTime>>(None),
                outputs::marked_deleted_at_height.eq::<Option<i64>>(None),
                outputs::marked_deleted_in_block.eq::<Option<Vec<u8>>>(None),
            ))
            .execute(&conn)?;

        trace!(target: LOG_TARGET, "rows updated: {:?}", result);
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - set_outputs_to_be_revalidated: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }

        Ok(())
    }

    fn mark_output_as_spent(
        &self,
        hash: FixedHash,
        mark_deleted_at_height: u64,
        mark_deleted_in_block: FixedHash,
        confirmed: bool,
    ) -> Result<(), OutputManagerStorageError> {
        let start = Instant::now();
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let hash = hash.to_vec();
        let mark_deleted_in_block = mark_deleted_in_block.to_vec();
        let status = if confirmed {
            OutputStatus::Spent as i32
        } else {
            OutputStatus::SpentMinedUnconfirmed as i32
        };
        diesel::update(outputs::table.filter(outputs::hash.eq(hash)))
            .set((
                outputs::marked_deleted_at_height.eq(mark_deleted_at_height as i64),
                outputs::marked_deleted_in_block.eq(mark_deleted_in_block),
                outputs::status.eq(status),
            ))
            .execute(&conn)
            .num_rows_affected_or_not_found(1)?;
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - mark_output_as_spent: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }

        Ok(())
    }

    fn mark_output_as_unspent(&self, hash: FixedHash) -> Result<(), OutputManagerStorageError> {
        let start = Instant::now();
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let hash = hash.to_vec();
        debug!(target: LOG_TARGET, "mark_output_as_unspent({})", hash.to_hex());
        diesel::update(outputs::table.filter(outputs::hash.eq(hash)))
            .set((
                outputs::marked_deleted_at_height.eq::<Option<i64>>(None),
                outputs::marked_deleted_in_block.eq::<Option<Vec<u8>>>(None),
                outputs::status.eq(OutputStatus::Unspent as i32),
            ))
            .execute(&conn)
            .num_rows_affected_or_not_found(1)?;
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - mark_output_as_unspent: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }

        Ok(())
    }

    fn short_term_encumber_outputs(
        &self,
        tx_id: TxId,
        outputs_to_send: &[DbUnblindedOutput],
        outputs_to_receive: &[DbUnblindedOutput],
    ) -> Result<(), OutputManagerStorageError> {
        let start = Instant::now();
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();

        let mut commitments = Vec::with_capacity(outputs_to_send.len());
        for output in outputs_to_send {
            commitments.push(output.commitment.as_bytes());
        }
        conn.transaction::<_, _, _>(|| {
            // Any output in the list without the `Unspent` status will invalidate the encumberance
            if !OutputSql::find_by_commitments_excluding_status(commitments.clone(), OutputStatus::Unspent, &conn)?
                .is_empty()
            {
                return Err(OutputManagerStorageError::OutputAlreadySpent);
            };

            let count = OutputSql::update_by_commitments(
                commitments,
                UpdateOutput {
                    status: Some(OutputStatus::ShortTermEncumberedToBeSpent),
                    spent_in_tx_id: Some(Some(tx_id)),
                    ..Default::default()
                },
                &conn,
            )?;
            if count != outputs_to_send.len() {
                let msg = format!(
                    "Inconsistent short term encumbering! Lengths do not match - {} vs {}",
                    count,
                    outputs_to_send.len()
                );
                error!(target: LOG_TARGET, "{}", msg,);
                return Err(OutputManagerStorageError::UnexpectedResult(msg));
            }

            Ok(())
        })?;

        for co in outputs_to_receive {
            let mut new_output = NewOutputSql::new(
                co.clone(),
                OutputStatus::ShortTermEncumberedToBeReceived,
                Some(tx_id),
                None,
            )?;
            self.encrypt_if_necessary(&mut new_output)?;
            new_output.commit(&conn)?;
        }
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - short_term_encumber_outputs (TxId: {}): lock {} + db_op {} = {} ms",
                tx_id,
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }

        Ok(())
    }

    fn confirm_encumbered_outputs(&self, tx_id: TxId) -> Result<(), OutputManagerStorageError> {
        let start = Instant::now();
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();

        conn.transaction::<_, _, _>(|| {
            update_outputs_with_tx_id_and_status_to_new_status(
                &conn,
                tx_id,
                OutputStatus::ShortTermEncumberedToBeReceived,
                OutputStatus::EncumberedToBeReceived,
            )?;

            update_outputs_with_tx_id_and_status_to_new_status(
                &conn,
                tx_id,
                OutputStatus::ShortTermEncumberedToBeSpent,
                OutputStatus::EncumberedToBeSpent,
            )
        })?;

        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - confirm_encumbered_outputs (TxId: {}): lock {} + db_op {} = {} ms",
                tx_id,
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }

        Ok(())
    }

    fn clear_short_term_encumberances(&self) -> Result<(), OutputManagerStorageError> {
        let start = Instant::now();
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();

        conn.transaction::<_, _, _>(|| {
            diesel::update(
                outputs::table.filter(outputs::status.eq(OutputStatus::ShortTermEncumberedToBeReceived as i32)),
            )
            .set((outputs::status.eq(OutputStatus::CancelledInbound as i32),))
            .execute(&conn)?;

            diesel::update(outputs::table.filter(outputs::status.eq(OutputStatus::ShortTermEncumberedToBeSpent as i32)))
                .set((outputs::status.eq(OutputStatus::Unspent as i32),))
                .execute(&conn)
        })?;

        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - clear_short_term_encumberances: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }

        Ok(())
    }

    fn get_last_mined_output(&self) -> Result<Option<DbUnblindedOutput>, OutputManagerStorageError> {
        let start = Instant::now();
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();

        let output = OutputSql::first_by_mined_height_desc(&conn)?;
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - get_last_mined_output: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }
        match output {
            Some(mut o) => {
                self.decrypt_if_necessary(&mut o)?;
                Ok(Some(o.try_into()?))
            },
            None => Ok(None),
        }
    }

    fn get_last_spent_output(&self) -> Result<Option<DbUnblindedOutput>, OutputManagerStorageError> {
        let start = Instant::now();
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();

        let output = OutputSql::first_by_marked_deleted_height_desc(&conn)?;
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - get_last_spent_output: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }
        match output {
            Some(mut o) => {
                self.decrypt_if_necessary(&mut o)?;
                Ok(Some(o.try_into()?))
            },
            None => Ok(None),
        }
    }

    fn get_balance(
        &self,
        current_tip_for_time_lock_calculation: Option<u64>,
    ) -> Result<Balance, OutputManagerStorageError> {
        let start = Instant::now();
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();

        let result = OutputSql::get_balance(current_tip_for_time_lock_calculation, &conn);
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - get_balance: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }
        result
    }

    fn cancel_pending_transaction(&self, tx_id: TxId) -> Result<(), OutputManagerStorageError> {
        let start = Instant::now();
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();

        conn.transaction::<_, _, _>(|| {
            let outputs = OutputSql::find_by_tx_id_and_encumbered(tx_id, &conn)?;

            if outputs.is_empty() {
                return Err(OutputManagerStorageError::ValueNotFound);
            }

            for output in &outputs {
                if output.received_in_tx_id == Some(tx_id.as_i64_wrapped()) {
                    info!(
                        target: LOG_TARGET,
                        "Cancelling pending inbound output with Commitment: {} - MMR Position: {:?} from TxId: {}",
                        output.commitment.as_ref().unwrap_or(&vec![]).to_hex(),
                        output.mined_mmr_position,
                        tx_id
                    );
                    output.update(
                        UpdateOutput {
                            status: Some(OutputStatus::CancelledInbound),
                            ..Default::default()
                        },
                        &conn,
                    )?;
                } else if output.spent_in_tx_id == Some(tx_id.as_i64_wrapped()) {
                    info!(
                        target: LOG_TARGET,
                        "Cancelling pending outbound output with Commitment: {} - MMR Position: {:?} from TxId: {}",
                        output.commitment.as_ref().unwrap_or(&vec![]).to_hex(),
                        output.mined_mmr_position,
                        tx_id
                    );
                    output.update(
                        UpdateOutput {
                            status: Some(OutputStatus::Unspent),
                            spent_in_tx_id: Some(None),
                            // We clear these so that the output will be revalidated the next time a validation is done.
                            mined_height: Some(None),
                            mined_in_block: Some(None),
                            ..Default::default()
                        },
                        &conn,
                    )?;
                } else {
                }
            }

            Ok(())
        })?;

        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - cancel_pending_transaction: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }

        Ok(())
    }

    // This is typically used by a receiver after the finalized transaction has been broadcast/returned by the sender
    // as the sender has to finalize the signature that was partially constructed by the receiver
    fn update_output_metadata_signature(&self, output: &TransactionOutput) -> Result<(), OutputManagerStorageError> {
        let start = Instant::now();
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();

        conn.transaction::<_, OutputManagerStorageError, _>(|| {
            let db_output = OutputSql::find_by_commitment_and_cancelled(&output.commitment.to_vec(), false, &conn)?;
            db_output.update(
                // Note: Only the `nonce` and `u` portion needs to be updated at this time as the `v` portion is
                // already correct
                UpdateOutput {
                    metadata_signature_nonce: Some(output.metadata_signature.public_nonce().to_vec()),
                    metadata_signature_u_key: Some(output.metadata_signature.u().to_vec()),
                    ..Default::default()
                },
                &conn,
            )?;

            Ok(())
        })?;
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - update_output_metadata_signature: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }

        Ok(())
    }

    fn revalidate_unspent_output(&self, commitment: &Commitment) -> Result<(), OutputManagerStorageError> {
        let start = Instant::now();
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();

        conn.transaction::<_, _, _>(|| {
            let output = OutputSql::find_by_commitment_and_cancelled(&commitment.to_vec(), false, &conn)?;

            if OutputStatus::try_from(output.status)? != OutputStatus::Invalid {
                return Err(OutputManagerStorageError::ValuesNotFound);
            }
            output.update(
                UpdateOutput {
                    status: Some(OutputStatus::Unspent),
                    ..Default::default()
                },
                &conn,
            )?;

            Ok(())
        })?;
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - revalidate_unspent_output: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }
        Ok(())
    }

    fn apply_encryption(&self, cipher: XChaCha20Poly1305) -> Result<(), OutputManagerStorageError> {
        let mut current_cipher = acquire_write_lock!(self.cipher);

        if (*current_cipher).is_some() {
            return Err(OutputManagerStorageError::AlreadyEncrypted);
        }

        let start = Instant::now();
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let mut outputs = OutputSql::index(&conn)?;

        // If the db is already encrypted then the very first output we try to encrypt will fail.
        for o in &mut outputs {
            // Test if this output is encrypted or not to avoid a double encryption.
            let _secret_key = PrivateKey::from_vec(&o.spending_key).map_err(|_| {
                error!(
                    target: LOG_TARGET,
                    "Could not create PrivateKey from stored bytes, They might already be encrypted"
                );
                OutputManagerStorageError::AlreadyEncrypted
            })?;
            o.encrypt(&cipher)
                .map_err(|_| OutputManagerStorageError::AeadError("Encryption Error".to_string()))?;
            o.update_encryption(&conn)?;
        }

        let mut known_one_sided_payment_scripts = KnownOneSidedPaymentScriptSql::index(&conn)?;

        for script in &mut known_one_sided_payment_scripts {
            let _secret_key = PrivateKey::from_vec(&script.private_key).map_err(|_| {
                error!(
                    target: LOG_TARGET,
                    "Could not create PrivateKey from stored bytes, They might already be encrypted"
                );
                OutputManagerStorageError::AlreadyEncrypted
            })?;
            script
                .encrypt(&cipher)
                .map_err(|_| OutputManagerStorageError::AeadError("Encryption Error".to_string()))?;
            script.update_encryption(&conn)?;
        }

        (*current_cipher) = Some(cipher);
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - apply_encryption: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }

        Ok(())
    }

    fn remove_encryption(&self) -> Result<(), OutputManagerStorageError> {
        let mut current_cipher = acquire_write_lock!(self.cipher);
        let cipher = if let Some(cipher) = (*current_cipher).clone().take() {
            cipher
        } else {
            return Ok(());
        };
        let start = Instant::now();
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let mut outputs = OutputSql::index(&conn)?;

        for o in &mut outputs {
            o.decrypt(&cipher)
                .map_err(|_| OutputManagerStorageError::AeadError("Encryption Error".to_string()))?;
            o.update_encryption(&conn)?;
        }

        let mut known_one_sided_payment_scripts = KnownOneSidedPaymentScriptSql::index(&conn)?;

        for script in &mut known_one_sided_payment_scripts {
            script
                .decrypt(&cipher)
                .map_err(|_| OutputManagerStorageError::AeadError("Encryption Error".to_string()))?;
            script.update_encryption(&conn)?;
        }

        // Now that all the decryption has been completed we can safely remove the cipher fully
        std::mem::drop((*current_cipher).take());
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - remove_encryption: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }
        Ok(())
    }

    fn set_coinbase_abandoned(&self, tx_id: TxId, abandoned: bool) -> Result<(), OutputManagerStorageError> {
        let start = Instant::now();
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();

        if abandoned {
            debug!(
                target: LOG_TARGET,
                "set_coinbase_abandoned(TxID: {}) as {}", tx_id, abandoned
            );
            diesel::update(
                outputs::table.filter(
                    outputs::received_in_tx_id
                        .eq(Some(tx_id.as_u64() as i64))
                        .and(outputs::coinbase_block_height.is_not_null()),
                ),
            )
            .set((outputs::status.eq(OutputStatus::AbandonedCoinbase as i32),))
            .execute(&conn)
            .num_rows_affected_or_not_found(1)?;
        } else {
            update_outputs_with_tx_id_and_status_to_new_status(
                &conn,
                tx_id,
                OutputStatus::AbandonedCoinbase,
                OutputStatus::EncumberedToBeReceived,
            )?;
        };
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - set_coinbase_abandoned: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }

        Ok(())
    }

    fn reinstate_cancelled_inbound_output(&self, tx_id: TxId) -> Result<(), OutputManagerStorageError> {
        let start = Instant::now();
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();

        update_outputs_with_tx_id_and_status_to_new_status(
            &conn,
            tx_id,
            OutputStatus::CancelledInbound,
            OutputStatus::EncumberedToBeReceived,
        )?;

        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - reinstate_cancelled_inbound_output: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }
        Ok(())
    }

    fn add_unvalidated_output(&self, output: DbUnblindedOutput, tx_id: TxId) -> Result<(), OutputManagerStorageError> {
        let start = Instant::now();
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();

        if OutputSql::find_by_commitment_and_cancelled(&output.commitment.to_vec(), false, &conn).is_ok() {
            return Err(OutputManagerStorageError::DuplicateOutput);
        }
        let mut new_output = NewOutputSql::new(output, OutputStatus::EncumberedToBeReceived, Some(tx_id), None)?;
        self.encrypt_if_necessary(&mut new_output)?;
        new_output.commit(&conn)?;

        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - add_unvalidated_output: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }
        Ok(())
    }

    /// Retrieves UTXOs than can be spent, sorted by priority, then value from smallest to largest.
    fn fetch_unspent_outputs_for_spending(
        &self,
        selection_criteria: &UtxoSelectionCriteria,
        amount: u64,
        tip_height: Option<u64>,
    ) -> Result<Vec<DbUnblindedOutput>, OutputManagerStorageError> {
        let start = Instant::now();
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let mut outputs = OutputSql::fetch_unspent_outputs_for_spending(selection_criteria, amount, tip_height, &conn)?;
        for o in &mut outputs {
            self.decrypt_if_necessary(o)?;
        }
        trace!(
            target: LOG_TARGET,
            "sqlite profile - fetch_unspent_outputs_for_spending: lock {} + db_op {} = {} ms",
            acquire_lock.as_millis(),
            (start.elapsed() - acquire_lock).as_millis(),
            start.elapsed().as_millis()
        );
        outputs
            .iter()
            .map(|o| DbUnblindedOutput::try_from(o.clone()))
            .collect::<Result<Vec<_>, _>>()
    }

    fn fetch_outputs_by_tx_id(&self, tx_id: TxId) -> Result<Vec<DbUnblindedOutput>, OutputManagerStorageError> {
        let conn = self.database_connection.get_pooled_connection()?;
        let mut outputs = OutputSql::find_by_tx_id(tx_id, &conn)?;
        for o in &mut outputs {
            self.decrypt_if_necessary(o)?;
        }
        outputs
            .iter()
            .map(|o| DbUnblindedOutput::try_from(o.clone()))
            .collect::<Result<Vec<_>, _>>()
    }

    fn fetch_outputs_by(&self, q: OutputBackendQuery) -> Result<Vec<DbUnblindedOutput>, OutputManagerStorageError> {
        let conn = self.database_connection.get_pooled_connection()?;
        Ok(OutputSql::fetch_outputs_by(q, &conn)?
            .into_iter()
            .filter_map(|mut x| {
                if let Err(e) = self.decrypt_if_necessary(&mut x) {
                    error!(target: LOG_TARGET, "failed to `decrypt_if_necessary`: {:#?}", e);
                    return None;
                }

                DbUnblindedOutput::try_from(x)
                    .map_err(|e| {
                        error!(
                            target: LOG_TARGET,
                            "failed to convert `OutputSql` to `DbUnblindedOutput`: {:#?}", e
                        );
                        e
                    })
                    .ok()
            })
            .collect())
    }
}

fn update_outputs_with_tx_id_and_status_to_new_status(
    conn: &PooledConnection<ConnectionManager<SqliteConnection>>,
    tx_id: TxId,
    from_status: OutputStatus,
    to_status: OutputStatus,
) -> Result<(), OutputManagerStorageError> {
    diesel::update(
        outputs::table
            .filter(
                outputs::received_in_tx_id
                    .eq(Some(tx_id.as_u64() as i64))
                    .or(outputs::spent_in_tx_id.eq(Some(tx_id.as_u64() as i64))),
            )
            .filter(outputs::status.eq(from_status as i32)),
    )
    .set(outputs::status.eq(to_status as i32))
    .execute(conn)?;
    Ok(())
}

/// These are the fields that can be updated for an Output
#[derive(Default)]
pub struct UpdateOutput {
    status: Option<OutputStatus>,
    received_in_tx_id: Option<Option<TxId>>,
    spent_in_tx_id: Option<Option<TxId>>,
    spending_key: Option<Vec<u8>>,
    script_private_key: Option<Vec<u8>>,
    metadata_signature_nonce: Option<Vec<u8>>,
    metadata_signature_u_key: Option<Vec<u8>>,
    mined_height: Option<Option<u64>>,
    mined_in_block: Option<Option<Vec<u8>>>,
}

#[derive(AsChangeset)]
#[table_name = "outputs"]
pub struct UpdateOutputSql {
    status: Option<i32>,
    received_in_tx_id: Option<Option<i64>>,
    spent_in_tx_id: Option<Option<i64>>,
    spending_key: Option<Vec<u8>>,
    script_private_key: Option<Vec<u8>>,
    metadata_signature_nonce: Option<Vec<u8>>,
    metadata_signature_u_key: Option<Vec<u8>>,
    mined_height: Option<Option<i64>>,
    mined_in_block: Option<Option<Vec<u8>>>,
}

/// Map a Rust friendly UpdateOutput to the Sql data type form
impl From<UpdateOutput> for UpdateOutputSql {
    fn from(u: UpdateOutput) -> Self {
        Self {
            status: u.status.map(|t| t as i32),
            spending_key: u.spending_key,
            script_private_key: u.script_private_key,
            metadata_signature_nonce: u.metadata_signature_nonce,
            metadata_signature_u_key: u.metadata_signature_u_key,
            received_in_tx_id: u.received_in_tx_id.map(|o| o.map(TxId::as_i64_wrapped)),
            spent_in_tx_id: u.spent_in_tx_id.map(|o| o.map(TxId::as_i64_wrapped)),
            mined_height: u.mined_height.map(|t| t.map(|h| h as i64)),
            mined_in_block: u.mined_in_block,
        }
    }
}

#[derive(Clone, Derivative, Queryable, Insertable, Identifiable, PartialEq, AsChangeset)]
#[derivative(Debug)]
#[table_name = "known_one_sided_payment_scripts"]
#[primary_key(script_hash)]
// #[identifiable_options(primary_key(hash))]
pub struct KnownOneSidedPaymentScriptSql {
    pub script_hash: Vec<u8>,
    #[derivative(Debug = "ignore")]
    pub private_key: Vec<u8>,
    pub script: Vec<u8>,
    pub input: Vec<u8>,
    pub script_lock_height: i64,
}

/// These are the fields that can be updated for an Output
#[derive(AsChangeset)]
#[table_name = "known_one_sided_payment_scripts"]
pub struct UpdateKnownOneSidedPaymentScript {
    private_key: Option<Vec<u8>>,
    script: Option<Vec<u8>>,
    input: Option<Vec<u8>>,
}

impl KnownOneSidedPaymentScriptSql {
    /// Write this struct to the database
    pub fn commit(&self, conn: &SqliteConnection) -> Result<(), OutputManagerStorageError> {
        diesel::insert_into(known_one_sided_payment_scripts::table)
            .values(self.clone())
            .execute(conn)?;
        Ok(())
    }

    /// Find a particular script, if it exists
    pub fn find(
        hash: &[u8],
        conn: &SqliteConnection,
    ) -> Result<KnownOneSidedPaymentScriptSql, OutputManagerStorageError> {
        Ok(known_one_sided_payment_scripts::table
            .filter(known_one_sided_payment_scripts::script_hash.eq(hash))
            .first::<KnownOneSidedPaymentScriptSql>(conn)?)
    }

    /// Return all known scripts
    pub fn index(conn: &SqliteConnection) -> Result<Vec<KnownOneSidedPaymentScriptSql>, OutputManagerStorageError> {
        Ok(known_one_sided_payment_scripts::table.load::<KnownOneSidedPaymentScriptSql>(conn)?)
    }

    pub fn delete(&self, conn: &SqliteConnection) -> Result<(), OutputManagerStorageError> {
        let num_deleted = diesel::delete(
            known_one_sided_payment_scripts::table
                .filter(known_one_sided_payment_scripts::script_hash.eq(&self.script_hash)),
        )
        .execute(conn)?;

        if num_deleted == 0 {
            return Err(OutputManagerStorageError::ValuesNotFound);
        }

        Ok(())
    }

    pub fn update(
        &self,
        updated_known_script: UpdateKnownOneSidedPaymentScript,
        conn: &SqliteConnection,
    ) -> Result<KnownOneSidedPaymentScriptSql, OutputManagerStorageError> {
        diesel::update(
            known_one_sided_payment_scripts::table
                .filter(known_one_sided_payment_scripts::script_hash.eq(&self.script_hash)),
        )
        .set(updated_known_script)
        .execute(conn)
        .num_rows_affected_or_not_found(1)?;

        KnownOneSidedPaymentScriptSql::find(&self.script_hash, conn)
    }

    /// Update the changed fields of this record after encryption/decryption is performed
    pub fn update_encryption(&self, conn: &SqliteConnection) -> Result<(), OutputManagerStorageError> {
        let _known_one_sided_payment_script_sql = self.update(
            UpdateKnownOneSidedPaymentScript {
                private_key: Some(self.private_key.clone()),
                script: None,
                input: None,
            },
            conn,
        )?;
        Ok(())
    }
}

/// Conversion from an KnownOneSidedPaymentScript to the Sql datatype form
impl TryFrom<KnownOneSidedPaymentScriptSql> for KnownOneSidedPaymentScript {
    type Error = OutputManagerStorageError;

    fn try_from(o: KnownOneSidedPaymentScriptSql) -> Result<Self, Self::Error> {
        let script_hash = o.script_hash;
        let private_key = PrivateKey::from_bytes(&o.private_key).map_err(|_| {
            error!(
                target: LOG_TARGET,
                "Could not create PrivateKey from stored bytes, They might be encrypted"
            );
            OutputManagerStorageError::ConversionError {
                reason: "PrivateKey could not be converted from bytes".to_string(),
            }
        })?;
        let script = TariScript::from_bytes(&o.script).map_err(|_| {
            error!(target: LOG_TARGET, "Could not create tari script from stored bytes");
            OutputManagerStorageError::ConversionError {
                reason: "Tari Script could not be converted from bytes".to_string(),
            }
        })?;
        let input = ExecutionStack::from_bytes(&o.input).map_err(|_| {
            error!(target: LOG_TARGET, "Could not create execution stack from stored bytes");
            OutputManagerStorageError::ConversionError {
                reason: "ExecutionStack could not be converted from bytes".to_string(),
            }
        })?;
        let script_lock_height = o.script_lock_height as u64;
        Ok(KnownOneSidedPaymentScript {
            script_hash,
            private_key,
            script,
            input,
            script_lock_height,
        })
    }
}

/// Conversion from an KnownOneSidedPaymentScriptSQL to the datatype form
impl From<KnownOneSidedPaymentScript> for KnownOneSidedPaymentScriptSql {
    fn from(known_script: KnownOneSidedPaymentScript) -> Self {
        let script_lock_height = known_script.script_lock_height as i64;
        let script_hash = known_script.script_hash;
        let private_key = known_script.private_key.as_bytes().to_vec();
        let script = known_script.script.as_bytes().to_vec();
        let input = known_script.input.as_bytes().to_vec();
        KnownOneSidedPaymentScriptSql {
            script_hash,
            private_key,
            script,
            input,
            script_lock_height,
        }
    }
}

impl Encryptable<XChaCha20Poly1305> for KnownOneSidedPaymentScriptSql {
    fn domain(&self, field_name: &'static str) -> Vec<u8> {
        [
            Self::KNOWN_ONESIDED_PAYMENT_SCRIPT,
            self.script_hash.as_slice(),
            field_name.as_bytes(),
        ]
        .concat()
        .to_vec()
    }

    fn encrypt(&mut self, cipher: &XChaCha20Poly1305) -> Result<(), String> {
        self.private_key = encrypt_bytes_integral_nonce(cipher, self.domain("private_key"), self.private_key.clone())?;
        Ok(())
    }

    fn decrypt(&mut self, cipher: &XChaCha20Poly1305) -> Result<(), String> {
        self.private_key = decrypt_bytes_integral_nonce(cipher, self.domain("private_key"), self.private_key.clone())?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::{mem::size_of, time::Duration};

    use chacha20poly1305::{aead::NewAead, Key, XChaCha20Poly1305};
    use diesel::{Connection, SqliteConnection};
    use rand::{rngs::OsRng, RngCore};
    use tari_common_sqlite::sqlite_connection_pool::SqliteConnectionPool;
    use tari_common_types::types::CommitmentFactory;
    use tari_core::transactions::{
        tari_amount::MicroTari,
        test_helpers::{create_unblinded_output, TestParams as TestParamsHelpers},
        transaction_components::{OutputFeatures, TransactionInput, UnblindedOutput},
        CryptoFactories,
    };
    use tari_script::script;
    use tari_test_utils::random;
    use tempfile::tempdir;

    use crate::{
        output_manager_service::storage::{
            database::{DbKey, OutputManagerBackend},
            models::DbUnblindedOutput,
            sqlite_db::{
                new_output_sql::NewOutputSql,
                output_sql::OutputSql,
                OutputManagerSqliteDatabase,
                OutputStatus,
                UpdateOutput,
            },
            OutputSource,
        },
        storage::sqlite_utilities::wallet_db_connection::WalletDbConnection,
        util::encryption::Encryptable,
    };

    pub fn make_input(val: MicroTari) -> (TransactionInput, UnblindedOutput) {
        let test_params = TestParamsHelpers::new();
        let factory = CommitmentFactory::default();

        let unblinded_output = create_unblinded_output(script!(Nop), OutputFeatures::default(), &test_params, val);
        let input = unblinded_output.as_transaction_input(&factory).unwrap();

        (input, unblinded_output)
    }

    #[test]
    fn test_crud() {
        let db_name = format!("{}.sqlite3", random::string(8).as_str());
        let temp_dir = tempdir().unwrap();
        let db_folder = temp_dir.path().to_str().unwrap().to_string();
        let db_path = format!("{}{}", db_folder, db_name);

        embed_migrations!("./migrations");
        let conn = SqliteConnection::establish(&db_path).unwrap_or_else(|_| panic!("Error connecting to {}", db_path));

        embedded_migrations::run_with_output(&conn, &mut std::io::stdout()).expect("Migration failed");

        conn.execute("PRAGMA foreign_keys = ON").unwrap();

        let mut outputs = Vec::new();
        let mut outputs_spent = Vec::new();
        let mut outputs_unspent = Vec::new();

        let factories = CryptoFactories::default();

        for _i in 0..2 {
            let (_, uo) = make_input(MicroTari::from(100 + OsRng.next_u64() % 1000));
            let uo = DbUnblindedOutput::from_unblinded_output(uo, &factories, None, OutputSource::Unknown).unwrap();
            let o = NewOutputSql::new(uo, OutputStatus::Unspent, None, None).unwrap();
            outputs.push(o.clone());
            outputs_unspent.push(o.clone());
            o.commit(&conn).unwrap();
        }

        for _i in 0..3 {
            let (_, uo) = make_input(MicroTari::from(100 + OsRng.next_u64() % 1000));
            let uo = DbUnblindedOutput::from_unblinded_output(uo, &factories, None, OutputSource::Unknown).unwrap();
            let o = NewOutputSql::new(uo, OutputStatus::Spent, None, None).unwrap();
            outputs.push(o.clone());
            outputs_spent.push(o.clone());
            o.commit(&conn).unwrap();
        }

        assert_eq!(
            OutputSql::index(&conn)
                .unwrap()
                .iter()
                .map(|o| o.spending_key.clone())
                .collect::<Vec<Vec<u8>>>(),
            outputs.iter().map(|o| o.spending_key.clone()).collect::<Vec<Vec<u8>>>()
        );
        assert_eq!(
            OutputSql::index_status(OutputStatus::Unspent, &conn)
                .unwrap()
                .iter()
                .map(|o| o.spending_key.clone())
                .collect::<Vec<Vec<u8>>>(),
            outputs_unspent
                .iter()
                .map(|o| o.spending_key.clone())
                .collect::<Vec<Vec<u8>>>()
        );
        assert_eq!(
            OutputSql::index_status(OutputStatus::Spent, &conn)
                .unwrap()
                .iter()
                .map(|o| o.spending_key.clone())
                .collect::<Vec<Vec<u8>>>(),
            outputs_spent
                .iter()
                .map(|o| o.spending_key.clone())
                .collect::<Vec<Vec<u8>>>()
        );

        assert_eq!(
            OutputSql::find(&outputs[0].spending_key, &conn).unwrap().spending_key,
            outputs[0].spending_key
        );

        assert_eq!(
            OutputSql::find_status(&outputs[0].spending_key, OutputStatus::Unspent, &conn)
                .unwrap()
                .spending_key,
            outputs[0].spending_key
        );

        assert!(OutputSql::find_status(&outputs[0].spending_key, OutputStatus::Spent, &conn).is_err());

        let _result = OutputSql::find(&outputs[4].spending_key, &conn).unwrap().delete(&conn);

        assert_eq!(OutputSql::index(&conn).unwrap().len(), 4);

        let _updated1 = OutputSql::find(&outputs[0].spending_key, &conn)
            .unwrap()
            .update(
                UpdateOutput {
                    status: Some(OutputStatus::Unspent),
                    received_in_tx_id: Some(Some(44u64.into())),
                    ..Default::default()
                },
                &conn,
            )
            .unwrap();

        let _updated2 = OutputSql::find(&outputs[1].spending_key, &conn)
            .unwrap()
            .update(
                UpdateOutput {
                    status: Some(OutputStatus::EncumberedToBeReceived),
                    received_in_tx_id: Some(Some(44u64.into())),
                    ..Default::default()
                },
                &conn,
            )
            .unwrap();

        let result = OutputSql::find_by_tx_id_and_encumbered(44u64.into(), &conn).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].spending_key, outputs[1].spending_key);
    }

    #[test]
    fn test_output_encryption() {
        let db_name = format!("{}.sqlite3", random::string(8).as_str());
        let tempdir = tempdir().unwrap();
        let db_folder = tempdir.path().to_str().unwrap().to_string();
        let db_path = format!("{}{}", db_folder, db_name);

        embed_migrations!("./migrations");
        let conn = SqliteConnection::establish(&db_path).unwrap_or_else(|_| panic!("Error connecting to {}", db_path));

        embedded_migrations::run_with_output(&conn, &mut std::io::stdout()).expect("Migration failed");

        conn.execute("PRAGMA foreign_keys = ON").unwrap();
        let factories = CryptoFactories::default();

        let (_, uo) = make_input(MicroTari::from(100 + OsRng.next_u64() % 1000));
        let uo = DbUnblindedOutput::from_unblinded_output(uo, &factories, None, OutputSource::Unknown).unwrap();
        let output = NewOutputSql::new(uo, OutputStatus::Unspent, None, None).unwrap();

        let mut key = [0u8; size_of::<Key>()];
        OsRng.fill_bytes(&mut key);
        let key_ga = Key::from_slice(&key);
        let cipher = XChaCha20Poly1305::new(key_ga);

        output.commit(&conn).unwrap();
        let unencrypted_output = OutputSql::find(output.spending_key.as_slice(), &conn).unwrap();

        assert!(unencrypted_output.clone().decrypt(&cipher).is_err());
        unencrypted_output.delete(&conn).unwrap();

        let mut encrypted_output = output.clone();
        encrypted_output.encrypt(&cipher).unwrap();
        encrypted_output.commit(&conn).unwrap();

        let outputs = OutputSql::index(&conn).unwrap();
        let mut decrypted_output = outputs[0].clone();
        decrypted_output.decrypt(&cipher).unwrap();
        assert_eq!(decrypted_output.spending_key, output.spending_key);

        let wrong_key = Key::from_slice(b"an example very very wrong key!!");
        let wrong_cipher = XChaCha20Poly1305::new(wrong_key);
        assert!(outputs[0].clone().decrypt(&wrong_cipher).is_err());

        decrypted_output.update_encryption(&conn).unwrap();

        assert_eq!(
            OutputSql::find(output.spending_key.as_slice(), &conn)
                .unwrap()
                .spending_key,
            output.spending_key
        );

        decrypted_output.encrypt(&cipher).unwrap();
        decrypted_output.update_encryption(&conn).unwrap();

        let outputs = OutputSql::index(&conn).unwrap();
        let mut decrypted_output2 = outputs[0].clone();
        decrypted_output2.decrypt(&cipher).unwrap();
        assert_eq!(decrypted_output2.spending_key, output.spending_key);
    }

    #[test]
    fn test_apply_remove_encryption() {
        let db_name = format!("{}.sqlite3", random::string(8).as_str());
        let temp_dir = tempdir().unwrap();
        let db_folder = temp_dir.path().to_str().unwrap().to_string();
        let db_path = format!("{}{}", db_folder, db_name);

        embed_migrations!("./migrations");
        let mut pool = SqliteConnectionPool::new(db_path.clone(), 1, true, true, Duration::from_secs(60));
        pool.create_pool()
            .unwrap_or_else(|_| panic!("Error connecting to {}", db_path));
        // Note: For this test the connection pool is setup with a pool size of one; the pooled connection must go out
        // of scope to be released once obtained otherwise subsequent calls to obtain a pooled connection will fail .
        {
            let conn = pool
                .get_pooled_connection()
                .unwrap_or_else(|_| panic!("Error connecting to {}", db_path));

            embedded_migrations::run_with_output(&conn, &mut std::io::stdout()).expect("Migration failed");
            let factories = CryptoFactories::default();

            let (_, uo) = make_input(MicroTari::from(100 + OsRng.next_u64() % 1000));
            let uo = DbUnblindedOutput::from_unblinded_output(uo, &factories, None, OutputSource::Unknown).unwrap();
            let output = NewOutputSql::new(uo, OutputStatus::Unspent, None, None).unwrap();
            output.commit(&conn).unwrap();

            let (_, uo2) = make_input(MicroTari::from(100 + OsRng.next_u64() % 1000));
            let uo2 = DbUnblindedOutput::from_unblinded_output(uo2, &factories, None, OutputSource::Unknown).unwrap();
            let output2 = NewOutputSql::new(uo2, OutputStatus::Unspent, None, None).unwrap();
            output2.commit(&conn).unwrap();
        }

        let mut key = [0u8; size_of::<Key>()];
        OsRng.fill_bytes(&mut key);
        let key_ga = Key::from_slice(&key);
        let cipher = XChaCha20Poly1305::new(key_ga);

        let connection = WalletDbConnection::new(pool, None);

        let db1 = OutputManagerSqliteDatabase::new(connection.clone(), Some(cipher.clone()));
        assert!(db1.apply_encryption(cipher.clone()).is_err());

        let db2 = OutputManagerSqliteDatabase::new(connection.clone(), None);
        assert!(db2.remove_encryption().is_ok());
        db2.apply_encryption(cipher).unwrap();

        let db3 = OutputManagerSqliteDatabase::new(connection, None);
        assert!(db3.fetch(&DbKey::UnspentOutputs).is_err());

        db2.remove_encryption().unwrap();

        assert!(db3.fetch(&DbKey::UnspentOutputs).is_ok());
    }
}
