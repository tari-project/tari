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
    convert::TryFrom,
    sync::{Arc, RwLock},
};

use chacha20poly1305::XChaCha20Poly1305;
use chrono::{NaiveDateTime, Utc};
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
use tari_common_sqlite::{sqlite_connection_pool::PooledDbConnection, util::diesel_ext::ExpectedRowsExtension};
use tari_common_types::{
    encryption::{decrypt_bytes_integral_nonce, encrypt_bytes_integral_nonce, Encryptable},
    transaction::TxId,
    types::{Commitment, FixedHash, PrivateKey},
};
use tari_core::transactions::transaction_components::{OutputType, TransactionOutput};
use tari_crypto::tari_utilities::{hex::Hex, ByteArray};
use tari_script::{ExecutionStack, TariScript};
use tari_utilities::Hidden;
use tokio::time::Instant;
use zeroize::Zeroize;

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
};
mod new_output_sql;
mod output_sql;
const LOG_TARGET: &str = "wallet::output_manager_service::database::wallet";

/// A Sqlite backend for the Output Manager Service. The Backend is accessed via a connection pool to the Sqlite file.
#[derive(Clone)]
pub struct OutputManagerSqliteDatabase {
    database_connection: WalletDbConnection,
    cipher: Arc<RwLock<XChaCha20Poly1305>>,
}

impl OutputManagerSqliteDatabase {
    pub fn new(database_connection: WalletDbConnection, cipher: XChaCha20Poly1305) -> Self {
        Self {
            database_connection,
            cipher: Arc::new(RwLock::new(cipher)),
        }
    }

    fn insert(
        &self,
        key_value_pair: DbKeyValuePair,
        conn: &mut SqliteConnection,
    ) -> Result<(), OutputManagerStorageError> {
        let cipher = acquire_read_lock!(self.cipher);

        match key_value_pair {
            DbKeyValuePair::UnspentOutput(c, o) => {
                if OutputSql::find_by_commitment_and_cancelled(&c.to_vec(), false, conn).is_ok() {
                    return Err(OutputManagerStorageError::DuplicateOutput);
                }
                let new_output = NewOutputSql::new(*o, OutputStatus::Unspent, None, None, &cipher)?;
                new_output.commit(conn)?
            },
            DbKeyValuePair::UnspentOutputWithTxId(c, (tx_id, o)) => {
                if OutputSql::find_by_commitment_and_cancelled(&c.to_vec(), false, conn).is_ok() {
                    return Err(OutputManagerStorageError::DuplicateOutput);
                }
                let new_output = NewOutputSql::new(*o, OutputStatus::Unspent, Some(tx_id), None, &cipher)?;
                new_output.commit(conn)?
            },
            DbKeyValuePair::OutputToBeReceived(c, (tx_id, o, coinbase_block_height)) => {
                if OutputSql::find_by_commitment_and_cancelled(&c.to_vec(), false, conn).is_ok() {
                    return Err(OutputManagerStorageError::DuplicateOutput);
                }
                let new_output = NewOutputSql::new(
                    *o,
                    OutputStatus::EncumberedToBeReceived,
                    Some(tx_id),
                    coinbase_block_height,
                    &cipher,
                )?;
                new_output.commit(conn)?
            },

            DbKeyValuePair::KnownOneSidedPaymentScripts(script) => {
                let script_sql = KnownOneSidedPaymentScriptSql::from_known_one_sided_payment_script(script, &cipher)?;
                if KnownOneSidedPaymentScriptSql::find(&script_sql.script_hash, conn).is_ok() {
                    return Err(OutputManagerStorageError::DuplicateScript);
                }
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
        let mut conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let cipher = acquire_read_lock!(self.cipher);

        let result = match key {
            DbKey::SpentOutput(k) => match OutputSql::find_status(&k.to_vec(), OutputStatus::Spent, &mut conn) {
                Ok(o) => Some(DbValue::SpentOutput(Box::new(o.to_db_unblinded_output(&cipher)?))),
                Err(e) => {
                    match e {
                        OutputManagerStorageError::DieselError(DieselError::NotFound) => (),
                        e => return Err(e),
                    };
                    None
                },
            },
            DbKey::UnspentOutput(k) => match OutputSql::find_status(&k.to_vec(), OutputStatus::Unspent, &mut conn) {
                Ok(o) => Some(DbValue::UnspentOutput(Box::new(o.to_db_unblinded_output(&cipher)?))),
                Err(e) => {
                    match e {
                        OutputManagerStorageError::DieselError(DieselError::NotFound) => (),
                        e => return Err(e),
                    };
                    None
                },
            },
            DbKey::UnspentOutputHash(hash) => {
                match OutputSql::find_by_hash(hash.as_slice(), OutputStatus::Unspent, &mut conn) {
                    Ok(o) => Some(DbValue::UnspentOutput(Box::new(o.to_db_unblinded_output(&cipher)?))),
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
                match OutputSql::find_by_commitment(&commitment.to_vec(), &mut conn) {
                    Ok(o) => Some(DbValue::AnyOutput(Box::new(o.to_db_unblinded_output(&cipher)?))),
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
                let outputs = OutputSql::find_by_tx_id_and_status(*tx_id, *status, &mut conn)?;

                Some(DbValue::AnyOutputs(
                    outputs
                        .iter()
                        .map(|o| o.clone().to_db_unblinded_output(&cipher))
                        .collect::<Result<Vec<_>, _>>()?,
                ))
            },
            DbKey::UnspentOutputs => {
                let outputs = OutputSql::index_status(
                    vec![OutputStatus::Unspent, OutputStatus::UnspentMinedUnconfirmed],
                    &mut conn,
                )?;

                Some(DbValue::UnspentOutputs(
                    outputs
                        .iter()
                        .map(|o| o.clone().to_db_unblinded_output(&cipher))
                        .collect::<Result<Vec<_>, _>>()?,
                ))
            },
            DbKey::SpentOutputs => {
                let outputs = OutputSql::index_status(vec![OutputStatus::Spent], &mut conn)?;

                Some(DbValue::SpentOutputs(
                    outputs
                        .iter()
                        .map(|o| o.clone().to_db_unblinded_output(&cipher))
                        .collect::<Result<Vec<_>, _>>()?,
                ))
            },
            DbKey::TimeLockedUnspentOutputs(tip) => {
                let outputs = OutputSql::index_time_locked(*tip, &mut conn)?;

                Some(DbValue::UnspentOutputs(
                    outputs
                        .iter()
                        .map(|o| o.clone().to_db_unblinded_output(&cipher))
                        .collect::<Result<Vec<_>, _>>()?,
                ))
            },
            DbKey::InvalidOutputs => {
                let outputs = OutputSql::index_status(vec![OutputStatus::Invalid], &mut conn)?;

                Some(DbValue::InvalidOutputs(
                    outputs
                        .iter()
                        .map(|o| o.clone().to_db_unblinded_output(&cipher))
                        .collect::<Result<Vec<_>, _>>()?,
                ))
            },
            DbKey::KnownOneSidedPaymentScripts => {
                let known_one_sided_payment_scripts = KnownOneSidedPaymentScriptSql::index(&mut conn)?;

                Some(DbValue::KnownOneSidedPaymentScripts(
                    known_one_sided_payment_scripts
                        .iter()
                        .map(|script| script.clone().to_known_one_sided_payment_script(&cipher))
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
        let mut conn = self.database_connection.get_pooled_connection()?;
        let outputs = OutputSql::index_by_output_type(output_type, &mut conn)?;
        let cipher = acquire_read_lock!(self.cipher);

        outputs
            .iter()
            .map(|o| o.clone().to_db_unblinded_output(&cipher))
            .collect::<Result<Vec<_>, _>>()
    }

    fn fetch_sorted_unspent_outputs(&self) -> Result<Vec<DbUnblindedOutput>, OutputManagerStorageError> {
        let mut conn = self.database_connection.get_pooled_connection()?;
        let outputs = OutputSql::index_unspent(&mut conn)?;
        let cipher = acquire_read_lock!(self.cipher);

        outputs
            .into_iter()
            .map(|o| o.to_db_unblinded_output(&cipher))
            .collect::<Result<Vec<_>, _>>()
    }

    fn fetch_mined_unspent_outputs(&self) -> Result<Vec<DbUnblindedOutput>, OutputManagerStorageError> {
        let start = Instant::now();
        let mut conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let outputs = OutputSql::index_marked_deleted_in_block_is_null(&mut conn)?;
        let cipher = acquire_read_lock!(self.cipher);

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
            .map(|o| o.to_db_unblinded_output(&cipher))
            .collect::<Result<Vec<_>, _>>()
    }

    fn fetch_invalid_outputs(&self, timestamp: i64) -> Result<Vec<DbUnblindedOutput>, OutputManagerStorageError> {
        let start = Instant::now();
        let mut conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let outputs = OutputSql::index_invalid(&NaiveDateTime::from_timestamp_opt(timestamp, 0).unwrap(), &mut conn)?;
        let cipher = acquire_read_lock!(self.cipher);

        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - fetch_invalid_outputs: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }

        outputs
            .into_iter()
            .map(|o| o.to_db_unblinded_output(&cipher))
            .collect::<Result<Vec<_>, _>>()
    }

    fn fetch_unspent_mined_unconfirmed_outputs(&self) -> Result<Vec<DbUnblindedOutput>, OutputManagerStorageError> {
        let start = Instant::now();
        let mut conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let outputs = OutputSql::index_unconfirmed(&mut conn)?;
        let cipher = acquire_read_lock!(self.cipher);

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
            .map(|o| o.to_db_unblinded_output(&cipher))
            .collect::<Result<Vec<_>, _>>()
    }

    fn write(&self, op: WriteOperation) -> Result<Option<DbValue>, OutputManagerStorageError> {
        let start = Instant::now();
        let mut conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let cipher = acquire_read_lock!(self.cipher);

        let mut msg = "".to_string();
        let result = match op {
            WriteOperation::Insert(kvp) => {
                msg.push_str("Insert");
                self.insert(kvp, &mut conn)?;
                Ok(None)
            },
            WriteOperation::Remove(k) => match k {
                DbKey::AnyOutputByCommitment(commitment) => {
                    conn.transaction::<_, _, _>(|conn| {
                        msg.push_str("Remove");
                        // Used by coinbase when mining.
                        match OutputSql::find_by_commitment(&commitment.to_vec(), conn) {
                            Ok(o) => {
                                o.delete(conn)?;
                                Ok(Some(DbValue::AnyOutput(Box::new(o.to_db_unblinded_output(&cipher)?))))
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
        let mut conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let cipher = acquire_read_lock!(self.cipher);

        let outputs = OutputSql::index_status(
            vec![
                OutputStatus::EncumberedToBeReceived,
                OutputStatus::UnspentMinedUnconfirmed,
                OutputStatus::ShortTermEncumberedToBeReceived,
            ],
            &mut conn,
        )?;

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
            .map(|o| o.clone().to_db_unblinded_output(&cipher))
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
        let mut conn = self.database_connection.get_pooled_connection()?;
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
        let timestamp = NaiveDateTime::from_timestamp_opt(mined_timestamp as i64, 0).ok_or(
            OutputManagerStorageError::ConversionError {
                reason: format!("Could not create timestamp mined_timestamp: {}", mined_timestamp),
            },
        )?;
        diesel::update(outputs::table.filter(outputs::hash.eq(hash)))
            .set((
                outputs::mined_height.eq(mined_height as i64),
                outputs::mined_in_block.eq(mined_in_block),
                outputs::mined_mmr_position.eq(mmr_position as i64),
                outputs::status.eq(status),
                outputs::mined_timestamp.eq(timestamp),
                outputs::marked_deleted_at_height.eq::<Option<i64>>(None),
                outputs::marked_deleted_in_block.eq::<Option<Vec<u8>>>(None),
                outputs::last_validation_timestamp.eq::<Option<NaiveDateTime>>(None),
            ))
            .execute(&mut conn)
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
        let mut conn = self.database_connection.get_pooled_connection()?;
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
            .execute(&mut conn)
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
        let mut conn = self.database_connection.get_pooled_connection()?;
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
            .execute(&mut conn)?;

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

    fn update_last_validation_timestamp(&self, hash: FixedHash) -> Result<(), OutputManagerStorageError> {
        let start = Instant::now();
        let mut conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let hash = hash.to_vec();
        diesel::update(outputs::table.filter(outputs::hash.eq(hash)))
            .set((outputs::last_validation_timestamp
                .eq::<Option<NaiveDateTime>>(NaiveDateTime::from_timestamp_opt(Utc::now().timestamp(), 0)),))
            .execute(&mut conn)
            .num_rows_affected_or_not_found(1)?;
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - set_output_to_be_revalidated_in_the_future: lock {} + db_op {} = {} ms",
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
        let mut conn = self.database_connection.get_pooled_connection()?;
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
            .execute(&mut conn)
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
        let mut conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let hash = hash.to_vec();
        debug!(target: LOG_TARGET, "mark_output_as_unspent({})", hash.to_hex());
        diesel::update(outputs::table.filter(outputs::hash.eq(hash)))
            .set((
                outputs::marked_deleted_at_height.eq::<Option<i64>>(None),
                outputs::marked_deleted_in_block.eq::<Option<Vec<u8>>>(None),
                outputs::status.eq(OutputStatus::Unspent as i32),
            ))
            .execute(&mut conn)
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
        let mut conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let cipher = acquire_read_lock!(self.cipher);

        let mut commitments = Vec::with_capacity(outputs_to_send.len());
        for output in outputs_to_send {
            commitments.push(output.commitment.as_bytes());
        }
        conn.transaction::<_, _, _>(|conn| {
            // Any output in the list without the `Unspent` status will invalidate the encumberance
            if !OutputSql::find_by_commitments_excluding_status(commitments.clone(), OutputStatus::Unspent, conn)?
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
                conn,
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
            let new_output = NewOutputSql::new(
                co.clone(),
                OutputStatus::ShortTermEncumberedToBeReceived,
                Some(tx_id),
                None,
                &cipher,
            )?;
            new_output.commit(&mut conn)?;
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
        let mut conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();

        conn.transaction::<_, _, _>(|conn| {
            update_outputs_with_tx_id_and_status_to_new_status(
                conn,
                tx_id,
                OutputStatus::ShortTermEncumberedToBeReceived,
                OutputStatus::EncumberedToBeReceived,
            )?;

            update_outputs_with_tx_id_and_status_to_new_status(
                conn,
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
        let mut conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();

        conn.transaction::<_, _, _>(|conn| {
            diesel::update(
                outputs::table.filter(outputs::status.eq(OutputStatus::ShortTermEncumberedToBeReceived as i32)),
            )
            .set((
                outputs::status.eq(OutputStatus::CancelledInbound as i32),
                outputs::last_validation_timestamp
                    .eq(NaiveDateTime::from_timestamp_opt(Utc::now().timestamp(), 0).unwrap()),
            ))
            .execute(conn)?;

            diesel::update(outputs::table.filter(outputs::status.eq(OutputStatus::ShortTermEncumberedToBeSpent as i32)))
                .set((outputs::status.eq(OutputStatus::Unspent as i32),))
                .execute(conn)
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
        let mut conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let cipher = acquire_read_lock!(self.cipher);

        let output = OutputSql::first_by_mined_height_desc(&mut conn)?;
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
            Some(o) => Ok(Some(o.to_db_unblinded_output(&cipher)?)),
            None => Ok(None),
        }
    }

    fn get_last_spent_output(&self) -> Result<Option<DbUnblindedOutput>, OutputManagerStorageError> {
        let start = Instant::now();
        let mut conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let cipher = acquire_read_lock!(self.cipher);

        let output = OutputSql::first_by_marked_deleted_height_desc(&mut conn)?;
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
            Some(o) => Ok(Some(o.to_db_unblinded_output(&cipher)?)),
            None => Ok(None),
        }
    }

    fn get_balance(
        &self,
        current_tip_for_time_lock_calculation: Option<u64>,
    ) -> Result<Balance, OutputManagerStorageError> {
        let start = Instant::now();
        let mut conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();

        let result = OutputSql::get_balance(current_tip_for_time_lock_calculation, &mut conn);
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
        let mut conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();

        conn.transaction::<_, _, _>(|conn| {
            let outputs = OutputSql::find_by_tx_id_and_encumbered(tx_id, conn)?;

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
                            last_validation_timestamp: Some(Some(
                                NaiveDateTime::from_timestamp_opt(Utc::now().timestamp(), 0).unwrap(),
                            )),
                            ..Default::default()
                        },
                        conn,
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
                        conn,
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
        let mut conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();

        conn.transaction::<_, OutputManagerStorageError, _>(|conn| {
            let db_output = OutputSql::find_by_commitment_and_cancelled(&output.commitment.to_vec(), false, conn)?;
            db_output.update(
                // Note: Only the `ephemeral_pubkey` and `u_y` portion needs to be updated at this time as the rest was
                // already correct
                UpdateOutput {
                    metadata_signature_ephemeral_pubkey: Some(output.metadata_signature.ephemeral_pubkey().to_vec()),
                    metadata_signature_u_y: Some(output.metadata_signature.u_y().to_vec()),
                    ..Default::default()
                },
                conn,
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
        let mut conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();

        conn.transaction::<_, _, _>(|conn| {
            let output = OutputSql::find_by_commitment_and_cancelled(&commitment.to_vec(), false, conn)?;

            if OutputStatus::try_from(output.status)? != OutputStatus::Invalid {
                return Err(OutputManagerStorageError::ValuesNotFound);
            }
            output.update(
                UpdateOutput {
                    status: Some(OutputStatus::Unspent),
                    ..Default::default()
                },
                conn,
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

    fn set_coinbase_abandoned(&self, tx_id: TxId, abandoned: bool) -> Result<(), OutputManagerStorageError> {
        let start = Instant::now();
        let mut conn = self.database_connection.get_pooled_connection()?;
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
            .execute(&mut conn)
            .num_rows_affected_or_not_found(1)?;
        } else {
            update_outputs_with_tx_id_and_status_to_new_status(
                &mut conn,
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
        let mut conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();

        update_outputs_with_tx_id_and_status_to_new_status(
            &mut conn,
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
        let mut conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let cipher = acquire_read_lock!(self.cipher);

        if OutputSql::find_by_commitment_and_cancelled(&output.commitment.to_vec(), false, &mut conn).is_ok() {
            return Err(OutputManagerStorageError::DuplicateOutput);
        }
        let new_output = NewOutputSql::new(output, OutputStatus::EncumberedToBeReceived, Some(tx_id), None, &cipher)?;
        new_output.commit(&mut conn)?;

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
        let mut conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let cipher = acquire_read_lock!(self.cipher);

        let outputs = OutputSql::fetch_unspent_outputs_for_spending(selection_criteria, amount, tip_height, &mut conn)?;

        trace!(
            target: LOG_TARGET,
            "sqlite profile - fetch_unspent_outputs_for_spending: lock {} + db_op {} = {} ms",
            acquire_lock.as_millis(),
            (start.elapsed() - acquire_lock).as_millis(),
            start.elapsed().as_millis()
        );
        outputs
            .iter()
            .map(|o| o.clone().to_db_unblinded_output(&cipher))
            .collect::<Result<Vec<_>, _>>()
    }

    fn fetch_outputs_by_tx_id(&self, tx_id: TxId) -> Result<Vec<DbUnblindedOutput>, OutputManagerStorageError> {
        let mut conn = self.database_connection.get_pooled_connection()?;
        let outputs = OutputSql::find_by_tx_id(tx_id, &mut conn)?;
        let cipher = acquire_read_lock!(self.cipher);

        outputs
            .iter()
            .map(|o| o.clone().to_db_unblinded_output(&cipher))
            .collect::<Result<Vec<_>, _>>()
    }

    fn fetch_outputs_by(&self, q: OutputBackendQuery) -> Result<Vec<DbUnblindedOutput>, OutputManagerStorageError> {
        let mut conn = self.database_connection.get_pooled_connection()?;
        let cipher = acquire_read_lock!(self.cipher);
        Ok(OutputSql::fetch_outputs_by(q, &mut conn)?
            .into_iter()
            .filter_map(|x| {
                x.to_db_unblinded_output(&cipher)
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
    conn: &mut PooledConnection<ConnectionManager<SqliteConnection>>,
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
#[derive(Clone, Default)]
pub struct UpdateOutput {
    status: Option<OutputStatus>,
    received_in_tx_id: Option<Option<TxId>>,
    spent_in_tx_id: Option<Option<TxId>>,
    metadata_signature_ephemeral_commitment: Option<Vec<u8>>,
    metadata_signature_ephemeral_pubkey: Option<Vec<u8>>,
    metadata_signature_u_a: Option<Vec<u8>>,
    metadata_signature_u_x: Option<Vec<u8>>,
    metadata_signature_u_y: Option<Vec<u8>>,
    mined_height: Option<Option<u64>>,
    mined_in_block: Option<Option<Vec<u8>>>,
    last_validation_timestamp: Option<Option<NaiveDateTime>>,
}

#[derive(AsChangeset)]
#[diesel(table_name = outputs)]
pub struct UpdateOutputSql {
    status: Option<i32>,
    received_in_tx_id: Option<Option<i64>>,
    spent_in_tx_id: Option<Option<i64>>,
    metadata_signature_ephemeral_commitment: Option<Vec<u8>>,
    metadata_signature_ephemeral_pubkey: Option<Vec<u8>>,
    metadata_signature_u_a: Option<Vec<u8>>,
    metadata_signature_u_x: Option<Vec<u8>>,
    metadata_signature_u_y: Option<Vec<u8>>,
    mined_height: Option<Option<i64>>,
    mined_in_block: Option<Option<Vec<u8>>>,
    last_validation_timestamp: Option<Option<NaiveDateTime>>,
}

/// Map a Rust friendly UpdateOutput to the Sql data type form
impl From<UpdateOutput> for UpdateOutputSql {
    fn from(u: UpdateOutput) -> Self {
        Self {
            status: u.status.map(|t| t as i32),
            metadata_signature_ephemeral_commitment: u.metadata_signature_ephemeral_commitment,
            metadata_signature_ephemeral_pubkey: u.metadata_signature_ephemeral_pubkey,
            metadata_signature_u_a: u.metadata_signature_u_a,
            metadata_signature_u_x: u.metadata_signature_u_x,
            metadata_signature_u_y: u.metadata_signature_u_y,
            received_in_tx_id: u.received_in_tx_id.map(|o| o.map(TxId::as_i64_wrapped)),
            spent_in_tx_id: u.spent_in_tx_id.map(|o| o.map(TxId::as_i64_wrapped)),
            mined_height: u.mined_height.map(|t| t.map(|h| h as i64)),
            mined_in_block: u.mined_in_block,
            last_validation_timestamp: u.last_validation_timestamp,
        }
    }
}

#[derive(Clone, Derivative, Queryable, Insertable, Identifiable, PartialEq, AsChangeset)]
#[derivative(Debug)]
#[diesel(table_name = known_one_sided_payment_scripts)]
#[diesel(primary_key(script_hash))]
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
#[diesel(table_name = known_one_sided_payment_scripts)]
pub struct UpdateKnownOneSidedPaymentScript {
    script: Option<Vec<u8>>,
    input: Option<Vec<u8>>,
}

impl KnownOneSidedPaymentScriptSql {
    /// Write this struct to the database
    pub fn commit(&self, conn: &mut SqliteConnection) -> Result<(), OutputManagerStorageError> {
        diesel::insert_into(known_one_sided_payment_scripts::table)
            .values(self.clone())
            .execute(conn)?;
        Ok(())
    }

    /// Find a particular script, if it exists
    pub fn find(
        hash: &[u8],
        conn: &mut SqliteConnection,
    ) -> Result<KnownOneSidedPaymentScriptSql, OutputManagerStorageError> {
        Ok(known_one_sided_payment_scripts::table
            .filter(known_one_sided_payment_scripts::script_hash.eq(hash))
            .first::<KnownOneSidedPaymentScriptSql>(conn)?)
    }

    /// Return all known scripts
    pub fn index(conn: &mut SqliteConnection) -> Result<Vec<KnownOneSidedPaymentScriptSql>, OutputManagerStorageError> {
        Ok(known_one_sided_payment_scripts::table.load::<KnownOneSidedPaymentScriptSql>(conn)?)
    }

    pub fn delete(&self, conn: &mut SqliteConnection) -> Result<(), OutputManagerStorageError> {
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
        conn: &mut SqliteConnection,
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

    /// Conversion from an KnownOneSidedPaymentScriptSQL to the datatype form
    pub fn to_known_one_sided_payment_script(
        self,
        cipher: &XChaCha20Poly1305,
    ) -> Result<KnownOneSidedPaymentScript, OutputManagerStorageError> {
        let mut output = self.decrypt(cipher).map_err(OutputManagerStorageError::AeadError)?;

        let script_hash = output.script_hash;
        let private_key = PrivateKey::from_bytes(&output.private_key).map_err(|_| {
            error!(
                target: LOG_TARGET,
                "Could not create PrivateKey from stored bytes, They might be encrypted"
            );
            OutputManagerStorageError::ConversionError {
                reason: "PrivateKey could not be converted from bytes".to_string(),
            }
        })?;

        // in order to avoid memory leaks of sensitive data, we zeroize the current private key buffer
        output.private_key.zeroize();

        let script = TariScript::from_bytes(&output.script).map_err(|_| {
            error!(target: LOG_TARGET, "Could not create tari script from stored bytes");
            OutputManagerStorageError::ConversionError {
                reason: "Tari Script could not be converted from bytes".to_string(),
            }
        })?;
        let input = ExecutionStack::from_bytes(&output.input).map_err(|_| {
            error!(target: LOG_TARGET, "Could not create execution stack from stored bytes");
            OutputManagerStorageError::ConversionError {
                reason: "ExecutionStack could not be converted from bytes".to_string(),
            }
        })?;
        let script_lock_height = output.script_lock_height as u64;

        Ok(KnownOneSidedPaymentScript {
            script_hash,
            private_key,
            script,
            input,
            script_lock_height,
        })
    }

    /// Conversion from an KnownOneSidedPaymentScriptSQL to the datatype form
    pub fn from_known_one_sided_payment_script(
        mut known_script: KnownOneSidedPaymentScript,
        cipher: &XChaCha20Poly1305,
    ) -> Result<Self, OutputManagerStorageError> {
        let script_lock_height = known_script.script_lock_height as i64;
        let script_hash = known_script.script_hash;
        let private_key = known_script.private_key.as_bytes().to_vec();
        let script = known_script.script.to_bytes().to_vec();
        let input = known_script.input.to_bytes().to_vec();

        let payment_script = KnownOneSidedPaymentScriptSql {
            script_hash,
            private_key,
            script,
            input,
            script_lock_height,
        };

        // zeroize sensitive data
        known_script.private_key.zeroize();

        let payment_script = payment_script
            .encrypt(cipher)
            .map_err(OutputManagerStorageError::AeadError)?;

        Ok(payment_script)
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

    fn encrypt(mut self, cipher: &XChaCha20Poly1305) -> Result<Self, String> {
        self.private_key =
            encrypt_bytes_integral_nonce(cipher, self.domain("private_key"), Hidden::hide(self.private_key))?;
        Ok(self)
    }

    fn decrypt(mut self, cipher: &XChaCha20Poly1305) -> Result<Self, String> {
        self.private_key = decrypt_bytes_integral_nonce(cipher, self.domain("private_key"), &self.private_key)?;
        Ok(self)
    }
}

#[cfg(test)]
mod test {
    use std::mem::size_of;

    use chacha20poly1305::{Key, KeyInit, XChaCha20Poly1305};
    use diesel::{sql_query, Connection, RunQueryDsl, SqliteConnection};
    use diesel_migrations::{EmbeddedMigrations, MigrationHarness};
    use rand::{rngs::OsRng, RngCore};
    use tari_common_types::{encryption::Encryptable, types::CommitmentFactory};
    use tari_core::transactions::{
        tari_amount::MicroTari,
        test_helpers::{create_non_recoverable_unblinded_output, TestParams as TestParamsHelpers},
        transaction_components::{OutputFeatures, TransactionInput, UnblindedOutput},
        CryptoFactories,
    };
    use tari_script::script;
    use tari_test_utils::random;
    use tari_utilities::ByteArray;
    use tempfile::tempdir;

    use crate::output_manager_service::storage::{
        models::DbUnblindedOutput,
        sqlite_db::{new_output_sql::NewOutputSql, output_sql::OutputSql, OutputStatus, UpdateOutput},
        OutputSource,
    };

    pub fn make_input(val: MicroTari) -> (TransactionInput, UnblindedOutput) {
        let test_params = TestParamsHelpers::new();
        let factory = CommitmentFactory::default();

        let unblinded_output =
            create_non_recoverable_unblinded_output(script!(Nop), OutputFeatures::default(), &test_params, val)
                .unwrap();
        let input = unblinded_output.as_transaction_input(&factory).unwrap();

        (input, unblinded_output)
    }

    #[allow(clippy::too_many_lines)]
    #[test]
    fn test_crud() {
        let db_name = format!("{}.sqlite3", random::string(8).as_str());
        let temp_dir = tempdir().unwrap();
        let db_folder = temp_dir.path().to_str().unwrap().to_string();
        let db_path = format!("{}{}", db_folder, db_name);

        const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./migrations");

        let mut conn =
            SqliteConnection::establish(&db_path).unwrap_or_else(|_| panic!("Error connecting to {}", db_path));

        conn.run_pending_migrations(MIGRATIONS)
            .map(|v| {
                v.into_iter()
                    .map(|b| {
                        let m = format!("Running migration {}", b);
                        // std::io::stdout()
                        //     .write_all(m.as_ref())
                        //     .expect("Couldn't write migration number to stdout");
                        m
                    })
                    .collect::<Vec<String>>()
            })
            .expect("Migrations failed");

        sql_query("PRAGMA foreign_keys = ON").execute(&mut conn).unwrap();

        let mut outputs = Vec::new();
        let mut outputs_spent = Vec::new();
        let mut outputs_unspent = Vec::new();

        let factories = CryptoFactories::default();

        let mut key = [0u8; size_of::<Key>()];
        OsRng.fill_bytes(&mut key);
        let key_ga = Key::from_slice(&key);
        let cipher = XChaCha20Poly1305::new(key_ga);

        for _i in 0..2 {
            let (_, uo) = make_input(MicroTari::from(100 + OsRng.next_u64() % 1000));
            let uo = DbUnblindedOutput::from_unblinded_output(uo, &factories, None, OutputSource::Unknown, None, None)
                .unwrap();
            let o = NewOutputSql::new(uo, OutputStatus::Unspent, None, None, &cipher).unwrap();
            outputs.push(o.clone());
            outputs_unspent.push(o.clone());
            o.commit(&mut conn).unwrap();
        }

        for _i in 0..3 {
            let (_, uo) = make_input(MicroTari::from(100 + OsRng.next_u64() % 1000));
            let uo = DbUnblindedOutput::from_unblinded_output(uo, &factories, None, OutputSource::Unknown, None, None)
                .unwrap();
            let o = NewOutputSql::new(uo, OutputStatus::Spent, None, None, &cipher).unwrap();
            outputs.push(o.clone());
            outputs_spent.push(o.clone());
            o.commit(&mut conn).unwrap();
        }

        assert_eq!(
            OutputSql::index(&mut conn)
                .unwrap()
                .iter()
                .map(|o| o.spending_key.clone())
                .collect::<Vec<Vec<u8>>>(),
            outputs.iter().map(|o| o.spending_key.clone()).collect::<Vec<Vec<u8>>>()
        );
        assert_eq!(
            OutputSql::index_status(vec!(OutputStatus::Unspent), &mut conn)
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
            OutputSql::index_status(vec!(OutputStatus::Spent), &mut conn)
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
            OutputSql::find(&outputs[0].spending_key, &mut conn)
                .unwrap()
                .spending_key,
            outputs[0].spending_key
        );

        assert_eq!(
            OutputSql::find_status(&outputs[0].spending_key, OutputStatus::Unspent, &mut conn)
                .unwrap()
                .spending_key,
            outputs[0].spending_key
        );

        assert!(OutputSql::find_status(&outputs[0].spending_key, OutputStatus::Spent, &mut conn).is_err());

        let _result = OutputSql::find(&outputs[4].spending_key, &mut conn)
            .unwrap()
            .delete(&mut conn);

        assert_eq!(OutputSql::index(&mut conn).unwrap().len(), 4);

        let _updated1 = OutputSql::find(&outputs[0].spending_key, &mut conn)
            .unwrap()
            .update(
                UpdateOutput {
                    status: Some(OutputStatus::Unspent),
                    received_in_tx_id: Some(Some(44u64.into())),
                    ..Default::default()
                },
                &mut conn,
            )
            .unwrap();

        let _updated2 = OutputSql::find(&outputs[1].spending_key, &mut conn)
            .unwrap()
            .update(
                UpdateOutput {
                    status: Some(OutputStatus::EncumberedToBeReceived),
                    received_in_tx_id: Some(Some(44u64.into())),
                    ..Default::default()
                },
                &mut conn,
            )
            .unwrap();

        let result = OutputSql::find_by_tx_id_and_encumbered(44u64.into(), &mut conn).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].spending_key, outputs[1].spending_key);
    }

    #[test]
    fn test_output_encryption() {
        let db_name = format!("{}.sqlite3", random::string(8).as_str());
        let tempdir = tempdir().unwrap();
        let db_folder = tempdir.path().to_str().unwrap().to_string();
        let db_path = format!("{}{}", db_folder, db_name);

        const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./migrations");
        let mut conn =
            SqliteConnection::establish(&db_path).unwrap_or_else(|_| panic!("Error connecting to {}", db_path));

        conn.run_pending_migrations(MIGRATIONS)
            .map(|v| {
                v.into_iter()
                    .map(|b| {
                        let m = format!("Running migration {}", b);
                        // std::io::stdout()
                        //     .write_all(m.as_ref())
                        //     .expect("Couldn't write migration number to stdout");
                        m
                    })
                    .collect::<Vec<String>>()
            })
            .expect("Migrations failed");

        sql_query("PRAGMA foreign_keys = ON").execute(&mut conn).unwrap();
        let factories = CryptoFactories::default();

        let mut key = [0u8; size_of::<Key>()];
        OsRng.fill_bytes(&mut key);
        let key_ga = Key::from_slice(&key);
        let cipher = XChaCha20Poly1305::new(key_ga);

        let (_, uo) = make_input(MicroTari::from(100 + OsRng.next_u64() % 1000));
        let decrypted_spending_key = uo.spending_key.to_vec();

        let uo =
            DbUnblindedOutput::from_unblinded_output(uo, &factories, None, OutputSource::Unknown, None, None).unwrap();

        let output = NewOutputSql::new(uo, OutputStatus::Unspent, None, None, &cipher).unwrap();

        output.commit(&mut conn).unwrap();
        let encrypted_output = OutputSql::find(output.spending_key.as_slice(), &mut conn).unwrap();

        // Aead encryption of spending key contains 24 bytes nonce + 16 bytes tag + 32 bytes encrypted spneding key
        assert_eq!(encrypted_output.spending_key.len(), 32 + 24 + 16);
        assert_eq!(encrypted_output.spending_key, output.spending_key);

        let decrypted_output = encrypted_output.clone();

        let decrypted_output = decrypted_output.decrypt(&cipher).unwrap();
        assert_eq!(decrypted_output.spending_key.len(), 32);
        assert_eq!(decrypted_output.spending_key, decrypted_spending_key);

        let output_2 = output.clone();
        let output_2 = output_2.decrypt(&cipher).unwrap();
        assert_eq!(decrypted_output.spending_key, output_2.spending_key);

        let wrong_key = Key::from_slice(b"an example very very wrong key!!");
        let wrong_cipher = XChaCha20Poly1305::new(wrong_key);
        assert!(encrypted_output.decrypt(&wrong_cipher).is_err());
        assert!(output.clone().decrypt(&wrong_cipher).is_err());

        assert_eq!(
            OutputSql::find(output.spending_key.as_slice(), &mut conn)
                .unwrap()
                .spending_key,
            output.spending_key
        );

        let outputs = OutputSql::index(&mut conn).unwrap();
        let decrypted_output2 = outputs[0].clone();
        let decrypted_output2 = decrypted_output2.decrypt(&cipher).unwrap();
        assert_eq!(decrypted_output2.spending_key, decrypted_output.spending_key);
    }
}
