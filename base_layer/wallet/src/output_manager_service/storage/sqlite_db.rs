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

use crate::{
    output_manager_service::{
        error::OutputManagerStorageError,
        service::Balance,
        storage::{
            database::{DbKey, DbKeyValuePair, DbValue, KeyManagerState, OutputManagerBackend, WriteOperation},
            models::{DbUnblindedOutput, KnownOneSidedPaymentScript, OutputStatus},
        },
        TxId,
    },
    schema::{key_manager_states, known_one_sided_payment_scripts, outputs},
    storage::sqlite_utilities::WalletDbConnection,
    util::{
        diesel_ext::ExpectedRowsExtension,
        encryption::{decrypt_bytes_integral_nonce, encrypt_bytes_integral_nonce, Encryptable},
    },
};
use aes_gcm::{aead::Error as AeadError, Aes256Gcm, Error};
use chrono::{NaiveDateTime, Utc};
use diesel::{prelude::*, result::Error as DieselError, sql_query, SqliteConnection};
use log::*;
use std::{
    convert::{TryFrom, TryInto},
    str::from_utf8,
    sync::{Arc, RwLock},
};
use tari_common_types::types::{ComSignature, Commitment, PrivateKey, PublicKey};
use tari_core::{
    tari_utilities::hash::Hashable,
    transactions::{
        tari_amount::MicroTari,
        transaction::{OutputFeatures, OutputFlags, TransactionOutput, UnblindedOutput},
        CryptoFactories,
    },
};
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    script::{ExecutionStack, TariScript},
    tari_utilities::{
        hex::{from_hex, Hex},
        ByteArray,
    },
};

const LOG_TARGET: &str = "wallet::output_manager_service::database::sqlite_db";

/// A Sqlite backend for the Output Manager Service. The Backend is accessed via a connection pool to the Sqlite file.
#[derive(Clone)]
pub struct OutputManagerSqliteDatabase {
    database_connection: WalletDbConnection,
    cipher: Arc<RwLock<Option<Aes256Gcm>>>,
}

impl OutputManagerSqliteDatabase {
    pub fn new(database_connection: WalletDbConnection, cipher: Option<Aes256Gcm>) -> Self {
        Self {
            database_connection,
            cipher: Arc::new(RwLock::new(cipher)),
        }
    }

    fn decrypt_if_necessary<T: Encryptable<Aes256Gcm>>(&self, o: &mut T) -> Result<(), OutputManagerStorageError> {
        let cipher = acquire_read_lock!(self.cipher);
        if let Some(cipher) = cipher.as_ref() {
            o.decrypt(cipher)
                .map_err(|_| OutputManagerStorageError::AeadError("Decryption Error".to_string()))?;
        }
        Ok(())
    }

    fn encrypt_if_necessary<T: Encryptable<Aes256Gcm>>(&self, o: &mut T) -> Result<(), OutputManagerStorageError> {
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
                if OutputSql::find_by_commitment_and_cancelled(&c.to_vec(), false, &(*conn)).is_ok() {
                    return Err(OutputManagerStorageError::DuplicateOutput);
                }
                let mut new_output = NewOutputSql::new(*o, OutputStatus::Unspent, None, None)?;
                self.encrypt_if_necessary(&mut new_output)?;
                new_output.commit(&(*conn))?
            },
            DbKeyValuePair::UnspentOutputWithTxId(c, (tx_id, o)) => {
                if OutputSql::find_by_commitment_and_cancelled(&c.to_vec(), false, &(*conn)).is_ok() {
                    return Err(OutputManagerStorageError::DuplicateOutput);
                }
                let mut new_output = NewOutputSql::new(*o, OutputStatus::Unspent, Some(tx_id), None)?;
                self.encrypt_if_necessary(&mut new_output)?;
                new_output.commit(&(*conn))?
            },
            DbKeyValuePair::OutputToBeReceived(c, (tx_id, o, coinbase_block_height)) => {
                if OutputSql::find_by_commitment_and_cancelled(&c.to_vec(), false, &(*conn)).is_ok() {
                    return Err(OutputManagerStorageError::DuplicateOutput);
                }
                let mut new_output = NewOutputSql::new(
                    *o,
                    OutputStatus::EncumberedToBeReceived,
                    Some(tx_id),
                    coinbase_block_height,
                )?;
                self.encrypt_if_necessary(&mut new_output)?;
                new_output.commit(&(*conn))?
            },
            DbKeyValuePair::KeyManagerState(km) => {
                let mut km_sql = NewKeyManagerStateSql::from(km);
                self.encrypt_if_necessary(&mut km_sql)?;
                km_sql.commit(&(*conn))?
            },
            DbKeyValuePair::KnownOneSidedPaymentScripts(script) => {
                let mut script_sql = KnownOneSidedPaymentScriptSql::from(script);
                self.encrypt_if_necessary(&mut script_sql)?;
                script_sql.commit(&(*conn))?
            },
        }
        Ok(())
    }
}

impl OutputManagerBackend for OutputManagerSqliteDatabase {
    #[allow(clippy::cognitive_complexity)]
    fn fetch(&self, key: &DbKey) -> Result<Option<DbValue>, OutputManagerStorageError> {
        let conn = self.database_connection.acquire_lock();

        let result = match key {
            DbKey::SpentOutput(k) => match OutputSql::find_status(&k.to_vec(), OutputStatus::Spent, &(*conn)) {
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
            DbKey::UnspentOutput(k) => match OutputSql::find_status(&k.to_vec(), OutputStatus::Unspent, &(*conn)) {
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

            DbKey::AnyOutputByCommitment(commitment) => {
                match OutputSql::find_by_commitment(&commitment.to_vec(), &(*conn)) {
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
                }
            },
            DbKey::OutputsByTxIdAndStatus(tx_id, status) => {
                let mut outputs = OutputSql::find_by_tx_id_and_status(*tx_id, *status, &(*conn))?;
                for o in outputs.iter_mut() {
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
                let mut outputs = OutputSql::index_status(OutputStatus::Unspent, &(*conn))?;
                for o in outputs.iter_mut() {
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
                let mut outputs = OutputSql::index_status(OutputStatus::Spent, &(*conn))?;
                for o in outputs.iter_mut() {
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
                let mut outputs = OutputSql::index_time_locked(*tip, &(*conn))?;
                for o in outputs.iter_mut() {
                    self.decrypt_if_necessary(o)?;
                }

                Some(DbValue::UnspentOutputs(
                    outputs
                        .iter()
                        .map(|o| DbUnblindedOutput::try_from(o.clone()))
                        .collect::<Result<Vec<_>, _>>()?,
                ))
            },
            DbKey::KeyManagerState => match KeyManagerStateSql::get_state(&(*conn)).ok() {
                None => None,
                Some(mut km) => {
                    self.decrypt_if_necessary(&mut km)?;

                    // TODO: This is a problem because the keymanager state does not have an index
                    // meaning that update round trips to the database can't be found again.
                    // I would suggest changing this to a different pattern for retrieval, perhaps
                    // only returning the columns that are needed.
                    Some(DbValue::KeyManagerState(KeyManagerState::try_from(km)?))
                },
            },
            DbKey::InvalidOutputs => {
                let mut outputs = OutputSql::index_status(OutputStatus::Invalid, &(*conn))?;
                for o in outputs.iter_mut() {
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
                let mut known_one_sided_payment_scripts = KnownOneSidedPaymentScriptSql::index(&(*conn))?;
                for script in known_one_sided_payment_scripts.iter_mut() {
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

        Ok(result)
    }

    fn fetch_mined_unspent_outputs(&self) -> Result<Vec<DbUnblindedOutput>, OutputManagerStorageError> {
        let conn = self.database_connection.acquire_lock();
        let mut outputs = OutputSql::index_marked_deleted_in_block_is_null(&(*conn))?;
        for output in outputs.iter_mut() {
            self.decrypt_if_necessary(output)?;
        }

        outputs
            .into_iter()
            .map(DbUnblindedOutput::try_from)
            .collect::<Result<Vec<_>, _>>()
    }

    fn fetch_unconfirmed_outputs(&self) -> Result<Vec<DbUnblindedOutput>, OutputManagerStorageError> {
        let conn = self.database_connection.acquire_lock();
        let mut outputs = OutputSql::index_unconfirmed(&(*conn))?;
        for output in outputs.iter_mut() {
            self.decrypt_if_necessary(output)?;
        }

        outputs
            .into_iter()
            .map(DbUnblindedOutput::try_from)
            .collect::<Result<Vec<_>, _>>()
    }

    fn fetch_pending_incoming_outputs(&self) -> Result<Vec<DbUnblindedOutput>, OutputManagerStorageError> {
        let conn = self.database_connection.acquire_lock();

        let mut outputs = OutputSql::index_status(OutputStatus::EncumberedToBeReceived, &conn)?;
        outputs.extend(OutputSql::index_status(
            OutputStatus::ShortTermEncumberedToBeReceived,
            &conn,
        )?);
        outputs.extend(OutputSql::index_status(OutputStatus::UnspentMinedUnconfirmed, &conn)?);
        for o in outputs.iter_mut() {
            self.decrypt_if_necessary(o)?;
        }
        outputs
            .iter()
            .map(|o| DbUnblindedOutput::try_from(o.clone()))
            .collect::<Result<Vec<_>, _>>()
    }

    fn write(&self, op: WriteOperation) -> Result<Option<DbValue>, OutputManagerStorageError> {
        let conn = self.database_connection.acquire_lock();

        match op {
            WriteOperation::Insert(kvp) => self.insert(kvp, &conn)?,
            WriteOperation::Remove(k) => match k {
                DbKey::AnyOutputByCommitment(commitment) => {
                    // Used by coinbase when mining.
                    match OutputSql::find_by_commitment(&commitment.to_vec(), &(*conn)) {
                        Ok(mut o) => {
                            o.delete(&(*conn))?;
                            self.decrypt_if_necessary(&mut o)?;
                            return Ok(Some(DbValue::AnyOutput(Box::new(DbUnblindedOutput::try_from(o)?))));
                        },
                        Err(e) => {
                            match e {
                                OutputManagerStorageError::DieselError(DieselError::NotFound) => (),
                                e => return Err(e),
                            };
                        },
                    }
                },
                DbKey::SpentOutput(_s) => return Err(OutputManagerStorageError::OperationNotSupported),
                DbKey::UnspentOutput(_k) => return Err(OutputManagerStorageError::OperationNotSupported),
                DbKey::UnspentOutputs => return Err(OutputManagerStorageError::OperationNotSupported),
                DbKey::SpentOutputs => return Err(OutputManagerStorageError::OperationNotSupported),
                DbKey::KeyManagerState => return Err(OutputManagerStorageError::OperationNotSupported),
                DbKey::InvalidOutputs => return Err(OutputManagerStorageError::OperationNotSupported),
                DbKey::TimeLockedUnspentOutputs(_) => return Err(OutputManagerStorageError::OperationNotSupported),
                DbKey::KnownOneSidedPaymentScripts => return Err(OutputManagerStorageError::OperationNotSupported),
                DbKey::OutputsByTxIdAndStatus(_, _) => return Err(OutputManagerStorageError::OperationNotSupported),
            },
        }

        Ok(None)
    }

    fn set_output_to_unmined(&self, hash: Vec<u8>) -> Result<(), OutputManagerStorageError> {
        let conn = self.database_connection.acquire_lock();
        // Only allow updating of non-deleted utxos
        diesel::update(outputs::table.filter(outputs::hash.eq(hash).and(outputs::marked_deleted_at_height.is_null())))
            .set((
                outputs::mined_height.eq::<Option<i64>>(None),
                outputs::mined_in_block.eq::<Option<Vec<u8>>>(None),
                outputs::mined_mmr_position.eq::<Option<i64>>(None),
                outputs::status.eq(OutputStatus::Invalid as i32),
            ))
            .execute(&(*conn))
            .num_rows_affected_or_not_found(1)?;

        Ok(())
    }

    fn set_received_output_mined_height(
        &self,
        hash: Vec<u8>,
        mined_height: u64,
        mined_in_block: Vec<u8>,
        mmr_position: u64,
        confirmed: bool,
    ) -> Result<(), OutputManagerStorageError> {
        let conn = self.database_connection.acquire_lock();
        let status = if confirmed {
            OutputStatus::Unspent as i32
        } else {
            OutputStatus::UnspentMinedUnconfirmed as i32
        };
        error!(
            target: LOG_TARGET,
            "`set_received_output_mined_height` status: {}", status
        );
        // Only allow updating of non-deleted utxos
        diesel::update(outputs::table.filter(outputs::hash.eq(hash).and(outputs::marked_deleted_at_height.is_null())))
            .set((
                outputs::mined_height.eq(mined_height as i64),
                outputs::mined_in_block.eq(mined_in_block),
                outputs::mined_mmr_position.eq(mmr_position as i64),
                outputs::status.eq(status),
            ))
            .execute(&(*conn))
            .num_rows_affected_or_not_found(1)?;

        Ok(())
    }

    fn mark_output_as_spent(
        &self,
        hash: Vec<u8>,
        mark_deleted_at_height: u64,
        mark_deleted_in_block: Vec<u8>,
        confirmed: bool,
    ) -> Result<(), OutputManagerStorageError> {
        let conn = self.database_connection.acquire_lock();
        let status = if confirmed {
            OutputStatus::Spent as i32
        } else {
            OutputStatus::SpentMinedUnconfirmed as i32
        };
        // Only allow updating of non-deleted utxos
        diesel::update(
            outputs::table.filter(
                outputs::hash.eq(hash).and(
                    outputs::marked_deleted_in_block
                        .is_null()
                        .or(outputs::status.eq(OutputStatus::SpentMinedUnconfirmed as i32)),
                ),
            ),
        )
        .set((
            outputs::marked_deleted_at_height.eq(mark_deleted_at_height as i64),
            outputs::marked_deleted_in_block.eq(mark_deleted_in_block),
            outputs::status.eq(status),
        ))
        .execute(&(*conn))
        .num_rows_affected_or_not_found(1)?;

        Ok(())
    }

    fn mark_output_as_unspent(&self, hash: Vec<u8>) -> Result<(), OutputManagerStorageError> {
        let conn = self.database_connection.acquire_lock();

        debug!(target: LOG_TARGET, "mark_output_as_unspent({})", hash.to_hex());
        diesel::update(
            outputs::table.filter(
                outputs::hash
                    .eq(hash)
                    .and(outputs::marked_deleted_at_height.is_not_null())
                    .and(outputs::mined_height.is_not_null()),
            ),
        )
        .set((
            outputs::marked_deleted_at_height.eq::<Option<i64>>(None),
            outputs::marked_deleted_in_block.eq::<Option<Vec<u8>>>(None),
            outputs::status.eq(OutputStatus::Unspent as i32),
        ))
        .execute(&(*conn))
        .num_rows_affected_or_not_found(1)?;

        Ok(())
    }

    fn set_coinbase_abandoned(&self, tx_id: TxId, abandoned: bool) -> Result<(), OutputManagerStorageError> {
        let conn = self.database_connection.acquire_lock();

        if abandoned {
            debug!(
                target: LOG_TARGET,
                "set_coinbase_abandoned(TxID: {}) as {}", tx_id, abandoned
            );
            diesel::update(
                outputs::table.filter(
                    outputs::received_in_tx_id
                        .eq(Some(tx_id as i64))
                        .and(outputs::coinbase_block_height.is_not_null()),
                ),
            )
            .set((outputs::status.eq(OutputStatus::AbandonedCoinbase as i32),))
            .execute(&(*conn))
            .num_rows_affected_or_not_found(1)?;
        } else {
            let output = OutputSql::find_by_tx_id_and_status(tx_id, OutputStatus::AbandonedCoinbase, &conn)?;
            for o in output.into_iter() {
                o.update(
                    UpdateOutput {
                        status: Some(OutputStatus::EncumberedToBeReceived),
                        ..Default::default()
                    },
                    &conn,
                )?;
            }
        };

        Ok(())
    }

    fn short_term_encumber_outputs(
        &self,
        tx_id: u64,
        outputs_to_send: &[DbUnblindedOutput],
        outputs_to_receive: &[DbUnblindedOutput],
    ) -> Result<(), OutputManagerStorageError> {
        let conn = self.database_connection.acquire_lock();

        let mut outputs_to_be_spent = Vec::with_capacity(outputs_to_send.len());
        for i in outputs_to_send {
            let output = OutputSql::find_by_commitment_and_cancelled(i.commitment.as_bytes(), false, &(*conn))?;
            if output.status != (OutputStatus::Unspent as i32) {
                return Err(OutputManagerStorageError::OutputAlreadySpent);
            }
            outputs_to_be_spent.push(output);
        }

        for o in outputs_to_be_spent {
            o.update(
                UpdateOutput {
                    status: Some(OutputStatus::ShortTermEncumberedToBeSpent),
                    spent_in_tx_id: Some(Some(tx_id)),
                    ..Default::default()
                },
                &(*conn),
            )?;
        }

        for co in outputs_to_receive {
            let mut new_output = NewOutputSql::new(
                co.clone(),
                OutputStatus::ShortTermEncumberedToBeReceived,
                Some(tx_id),
                None,
            )?;
            self.encrypt_if_necessary(&mut new_output)?;
            new_output.commit(&(*conn))?;
        }

        Ok(())
    }

    fn confirm_encumbered_outputs(&self, tx_id: TxId) -> Result<(), OutputManagerStorageError> {
        let conn = self.database_connection.acquire_lock();

        let outputs_to_be_received =
            OutputSql::find_by_tx_id_and_status(tx_id, OutputStatus::ShortTermEncumberedToBeReceived, &conn)?;
        for o in outputs_to_be_received.iter() {
            o.update(
                UpdateOutput {
                    status: Some(OutputStatus::EncumberedToBeReceived),
                    ..Default::default()
                },
                &(*conn),
            )?;
        }

        let outputs_to_be_spent =
            OutputSql::find_by_tx_id_and_status(tx_id, OutputStatus::ShortTermEncumberedToBeSpent, &conn)?;
        for o in outputs_to_be_spent.iter() {
            o.update(
                UpdateOutput {
                    status: Some(OutputStatus::EncumberedToBeSpent),
                    ..Default::default()
                },
                &(*conn),
            )?;
        }

        Ok(())
    }

    fn clear_short_term_encumberances(&self) -> Result<(), OutputManagerStorageError> {
        let conn = self.database_connection.acquire_lock();

        let outputs_to_be_received = OutputSql::index_status(OutputStatus::ShortTermEncumberedToBeReceived, &conn)?;
        for o in outputs_to_be_received.iter() {
            o.update(
                UpdateOutput {
                    status: Some(OutputStatus::CancelledInbound),
                    ..Default::default()
                },
                &(*conn),
            )?;
        }

        let outputs_to_be_spent = OutputSql::index_status(OutputStatus::ShortTermEncumberedToBeSpent, &conn)?;
        for o in outputs_to_be_spent.iter() {
            o.update(
                UpdateOutput {
                    status: Some(OutputStatus::Unspent),
                    ..Default::default()
                },
                &(*conn),
            )?;
        }

        Ok(())
    }

    fn get_last_mined_output(&self) -> Result<Option<DbUnblindedOutput>, OutputManagerStorageError> {
        let conn = self.database_connection.acquire_lock();

        let output = OutputSql::first_by_mined_height_desc(&(*conn))?;
        match output {
            Some(mut o) => {
                self.decrypt_if_necessary(&mut o)?;
                Ok(Some(o.try_into()?))
            },
            None => Ok(None),
        }
    }

    fn get_last_spent_output(&self) -> Result<Option<DbUnblindedOutput>, OutputManagerStorageError> {
        let conn = self.database_connection.acquire_lock();

        let output = OutputSql::first_by_marked_deleted_height_desc(&(*conn))?;
        match output {
            Some(mut o) => {
                self.decrypt_if_necessary(&mut o)?;
                Ok(Some(o.try_into()?))
            },
            None => Ok(None),
        }
    }

    fn get_balance(&self, tip: Option<u64>) -> Result<Balance, OutputManagerStorageError> {
        let conn = self.database_connection.acquire_lock();

        OutputSql::get_balance(tip, &(*conn))
    }

    fn cancel_pending_transaction(&self, tx_id: TxId) -> Result<(), OutputManagerStorageError> {
        let conn = self.database_connection.acquire_lock();

        let outputs = OutputSql::find_by_tx_id_and_encumbered(tx_id, &conn)?;

        if outputs.is_empty() {
            return Err(OutputManagerStorageError::ValueNotFound);
        }

        for output in outputs.iter() {
            if output.received_in_tx_id == Some(tx_id as i64) {
                output.update(
                    UpdateOutput {
                        status: Some(OutputStatus::CancelledInbound),
                        ..Default::default()
                    },
                    &(*conn),
                )?;
            } else if output.spent_in_tx_id == Some(tx_id as i64) {
                output.update(
                    UpdateOutput {
                        status: Some(OutputStatus::Unspent),
                        spent_in_tx_id: Some(None),
                        ..Default::default()
                    },
                    &(*conn),
                )?;
            }
        }

        Ok(())
    }

    fn clear_pending_coinbase_transaction_at_block_height(
        &self,
        block_height: u64,
    ) -> Result<(), OutputManagerStorageError> {
        let conn = self.database_connection.acquire_lock();

        let output = OutputSql::find_pending_coinbase_at_block_height(block_height, &conn)?;

        output.delete(&conn)?;

        Ok(())
    }

    fn increment_key_index(&self) -> Result<(), OutputManagerStorageError> {
        let conn = self.database_connection.acquire_lock();

        KeyManagerStateSql::increment_index(&(*conn))?;

        Ok(())
    }

    fn set_key_index(&self, index: u64) -> Result<(), OutputManagerStorageError> {
        let conn = self.database_connection.acquire_lock();

        KeyManagerStateSql::set_index(index, &(*conn))?;

        Ok(())
    }

    fn update_output_metadata_signature(&self, output: &TransactionOutput) -> Result<(), OutputManagerStorageError> {
        let conn = self.database_connection.acquire_lock();
        let db_output = OutputSql::find_by_commitment_and_cancelled(&output.commitment.to_vec(), false, &conn)?;
        db_output.update(
            UpdateOutput {
                metadata_signature_nonce: Some(output.metadata_signature.public_nonce().to_vec()),
                metadata_signature_u_key: Some(output.metadata_signature.u().to_vec()),
                ..Default::default()
            },
            &(*conn),
        )?;

        Ok(())
    }

    fn revalidate_unspent_output(&self, commitment: &Commitment) -> Result<(), OutputManagerStorageError> {
        let conn = self.database_connection.acquire_lock();
        let output = OutputSql::find_by_commitment_and_cancelled(&commitment.to_vec(), false, &conn)?;

        if OutputStatus::try_from(output.status)? != OutputStatus::Invalid {
            return Err(OutputManagerStorageError::ValuesNotFound);
        }
        output.update(
            UpdateOutput {
                status: Some(OutputStatus::Unspent),
                ..Default::default()
            },
            &(*conn),
        )?;
        Ok(())
    }

    fn reinstate_cancelled_inbound_output(&self, tx_id: TxId) -> Result<(), OutputManagerStorageError> {
        let conn = self.database_connection.acquire_lock();
        let outputs = OutputSql::find_by_tx_id_and_status(tx_id, OutputStatus::CancelledInbound, &conn)?;

        for o in outputs {
            o.update(
                UpdateOutput {
                    status: Some(OutputStatus::EncumberedToBeReceived),
                    ..Default::default()
                },
                &(*conn),
            )?;
        }
        Ok(())
    }

    fn apply_encryption(&self, cipher: Aes256Gcm) -> Result<(), OutputManagerStorageError> {
        let mut current_cipher = acquire_write_lock!(self.cipher);

        if (*current_cipher).is_some() {
            return Err(OutputManagerStorageError::AlreadyEncrypted);
        }

        let conn = self.database_connection.acquire_lock();
        let mut outputs = OutputSql::index(&conn)?;

        // If the db is already encrypted then the very first output we try to encrypt will fail.
        for o in outputs.iter_mut() {
            // Test if this output is encrypted or not to avoid a double encryption.
            let _ = PrivateKey::from_vec(&o.spending_key).map_err(|_| {
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

        let mut key_manager_state = KeyManagerStateSql::get_state(&conn)?;

        let _ = PrivateKey::from_vec(&key_manager_state.master_key).map_err(|_| {
            error!(
                target: LOG_TARGET,
                "Could not create PrivateKey from stored bytes, They might already be encrypted"
            );
            OutputManagerStorageError::AlreadyEncrypted
        })?;

        key_manager_state
            .encrypt(&cipher)
            .map_err(|_| OutputManagerStorageError::AeadError("Encryption Error".to_string()))?;
        key_manager_state.set_state(&conn)?;

        let mut known_one_sided_payment_scripts = KnownOneSidedPaymentScriptSql::index(&conn)?;

        for script in known_one_sided_payment_scripts.iter_mut() {
            let _ = PrivateKey::from_vec(&script.private_key).map_err(|_| {
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

        Ok(())
    }

    fn remove_encryption(&self) -> Result<(), OutputManagerStorageError> {
        let mut current_cipher = acquire_write_lock!(self.cipher);
        let cipher = if let Some(cipher) = (*current_cipher).clone().take() {
            cipher
        } else {
            return Ok(());
        };
        let conn = self.database_connection.acquire_lock();
        let mut outputs = OutputSql::index(&conn)?;

        for o in outputs.iter_mut() {
            o.decrypt(&cipher)
                .map_err(|_| OutputManagerStorageError::AeadError("Encryption Error".to_string()))?;
            o.update_encryption(&conn)?;
        }

        let mut key_manager_state = KeyManagerStateSql::get_state(&conn)?;
        key_manager_state
            .decrypt(&cipher)
            .map_err(|_| OutputManagerStorageError::AeadError("Encryption Error".to_string()))?;
        key_manager_state.set_state(&conn)?;

        let mut known_one_sided_payment_scripts = KnownOneSidedPaymentScriptSql::index(&conn)?;

        for script in known_one_sided_payment_scripts.iter_mut() {
            script
                .decrypt(&cipher)
                .map_err(|_| OutputManagerStorageError::AeadError("Encryption Error".to_string()))?;
            script.update_encryption(&conn)?;
        }

        // Now that all the decryption has been completed we can safely remove the cipher fully
        let _ = (*current_cipher).take();
        Ok(())
    }
}

impl TryFrom<i32> for OutputStatus {
    type Error = OutputManagerStorageError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(OutputStatus::Unspent),
            1 => Ok(OutputStatus::Spent),
            2 => Ok(OutputStatus::EncumberedToBeReceived),
            3 => Ok(OutputStatus::EncumberedToBeSpent),
            4 => Ok(OutputStatus::Invalid),
            5 => Ok(OutputStatus::CancelledInbound),
            6 => Ok(OutputStatus::UnspentMinedUnconfirmed),
            7 => Ok(OutputStatus::SpentMinedUnconfirmed),
            8 => Ok(OutputStatus::ShortTermEncumberedToBeSpent),
            9 => Ok(OutputStatus::ShortTermEncumberedToBeReceived),
            _ => Err(OutputManagerStorageError::ConversionError),
        }
    }
}

/// This struct represents an Output in the Sql database. A distinct struct is required to define the Sql friendly
/// equivalent datatypes for the members.
#[derive(Clone, Debug, Insertable, PartialEq)]
#[table_name = "outputs"]
struct NewOutputSql {
    commitment: Option<Vec<u8>>,
    spending_key: Vec<u8>,
    value: i64,
    flags: i32,
    maturity: i64,
    status: i32,
    hash: Option<Vec<u8>>,
    script: Vec<u8>,
    input_data: Vec<u8>,
    script_private_key: Vec<u8>,
    sender_offset_public_key: Vec<u8>,
    metadata_signature_nonce: Vec<u8>,
    metadata_signature_u_key: Vec<u8>,
    metadata_signature_v_key: Vec<u8>,
    received_in_tx_id: Option<i64>,
    coinbase_block_height: Option<i64>,
}

impl NewOutputSql {
    pub fn new(
        output: DbUnblindedOutput,
        status: OutputStatus,
        received_in_tx_id: Option<TxId>,
        coinbase_block_height: Option<u64>,
    ) -> Result<Self, OutputManagerStorageError> {
        Ok(Self {
            commitment: Some(output.commitment.to_vec()),
            spending_key: output.unblinded_output.spending_key.to_vec(),
            value: (u64::from(output.unblinded_output.value)) as i64,
            flags: output.unblinded_output.features.flags.bits() as i32,
            maturity: output.unblinded_output.features.maturity as i64,
            status: status as i32,
            received_in_tx_id: received_in_tx_id.map(|i| i as i64),
            hash: Some(output.hash),
            script: output.unblinded_output.script.as_bytes(),
            input_data: output.unblinded_output.input_data.as_bytes(),
            script_private_key: output.unblinded_output.script_private_key.to_vec(),
            sender_offset_public_key: output.unblinded_output.sender_offset_public_key.to_vec(),
            metadata_signature_nonce: output.unblinded_output.metadata_signature.public_nonce().to_vec(),
            metadata_signature_u_key: output.unblinded_output.metadata_signature.u().to_vec(),
            metadata_signature_v_key: output.unblinded_output.metadata_signature.v().to_vec(),
            coinbase_block_height: coinbase_block_height.map(|bh| bh as i64),
        })
    }

    /// Write this struct to the database
    pub fn commit(&self, conn: &SqliteConnection) -> Result<(), OutputManagerStorageError> {
        diesel::insert_into(outputs::table).values(self.clone()).execute(conn)?;
        Ok(())
    }
}

impl Encryptable<Aes256Gcm> for NewOutputSql {
    fn encrypt(&mut self, cipher: &Aes256Gcm) -> Result<(), AeadError> {
        self.spending_key = encrypt_bytes_integral_nonce(cipher, self.spending_key.clone())?;
        self.script_private_key = encrypt_bytes_integral_nonce(cipher, self.script_private_key.clone())?;
        Ok(())
    }

    fn decrypt(&mut self, cipher: &Aes256Gcm) -> Result<(), AeadError> {
        self.spending_key = decrypt_bytes_integral_nonce(cipher, self.spending_key.clone())?;
        self.script_private_key = decrypt_bytes_integral_nonce(cipher, self.script_private_key.clone())?;
        Ok(())
    }
}

#[derive(Clone, Debug, Queryable, Identifiable, PartialEq)]
#[table_name = "outputs"]
struct OutputSql {
    id: i32, // Auto inc primary key
    commitment: Option<Vec<u8>>,
    spending_key: Vec<u8>,
    value: i64,
    flags: i32,
    maturity: i64,
    status: i32,
    hash: Option<Vec<u8>>,
    script: Vec<u8>,
    input_data: Vec<u8>,
    script_private_key: Vec<u8>,
    sender_offset_public_key: Vec<u8>,
    metadata_signature_nonce: Vec<u8>,
    metadata_signature_u_key: Vec<u8>,
    metadata_signature_v_key: Vec<u8>,
    mined_height: Option<i64>,
    mined_in_block: Option<Vec<u8>>,
    mined_mmr_position: Option<i64>,
    marked_deleted_at_height: Option<i64>,
    marked_deleted_in_block: Option<Vec<u8>>,
    received_in_tx_id: Option<i64>,
    spent_in_tx_id: Option<i64>,
    coinbase_block_height: Option<i64>,
}

impl OutputSql {
    /// Return all outputs
    pub fn index(conn: &SqliteConnection) -> Result<Vec<OutputSql>, OutputManagerStorageError> {
        Ok(outputs::table.load::<OutputSql>(conn)?)
    }

    /// Return all outputs with a given status
    pub fn index_status(
        status: OutputStatus,
        conn: &SqliteConnection,
    ) -> Result<Vec<OutputSql>, OutputManagerStorageError> {
        Ok(outputs::table.filter(outputs::status.eq(status as i32)).load(conn)?)
    }

    /// Return all unspent outputs that have a maturity above the provided chain tip
    pub fn index_time_locked(tip: u64, conn: &SqliteConnection) -> Result<Vec<OutputSql>, OutputManagerStorageError> {
        Ok(outputs::table
            .filter(outputs::status.eq(OutputStatus::Unspent as i32))
            .filter(outputs::maturity.gt(tip as i64))
            .load(conn)?)
    }

    pub fn index_unconfirmed(conn: &SqliteConnection) -> Result<Vec<OutputSql>, OutputManagerStorageError> {
        Ok(outputs::table
            .filter(
                outputs::status
                    .eq(OutputStatus::UnspentMinedUnconfirmed as i32)
                    .or(outputs::mined_in_block.is_null()),
            )
            .order(outputs::id.asc())
            .load(conn)?)
    }

    pub fn index_marked_deleted_in_block_is_null(
        conn: &SqliteConnection,
    ) -> Result<Vec<OutputSql>, OutputManagerStorageError> {
        Ok(outputs::table
            // Return outputs not marked as deleted or confirmed 
            .filter(outputs::marked_deleted_in_block.is_null().or(outputs::status.eq(OutputStatus::SpentMinedUnconfirmed as i32)))
            // Only return mined
            .filter(outputs::mined_in_block.is_not_null())
            .order(outputs::id.asc())
            .load(conn)?)
    }

    pub fn first_by_mined_height_desc(conn: &SqliteConnection) -> Result<Option<OutputSql>, OutputManagerStorageError> {
        Ok(outputs::table
            .filter(outputs::mined_height.is_not_null())
            .order(outputs::mined_height.desc())
            .first(conn)
            .optional()?)
    }

    pub fn first_by_marked_deleted_height_desc(
        conn: &SqliteConnection,
    ) -> Result<Option<OutputSql>, OutputManagerStorageError> {
        Ok(outputs::table
            .filter(outputs::marked_deleted_at_height.is_not_null())
            .order(outputs::marked_deleted_at_height.desc())
            .first(conn)
            .optional()?)
    }

    /// Find a particular Output, if it exists
    pub fn find(spending_key: &[u8], conn: &SqliteConnection) -> Result<OutputSql, OutputManagerStorageError> {
        Ok(outputs::table
            .filter(outputs::spending_key.eq(spending_key))
            .first::<OutputSql>(conn)?)
    }

    /// Return the available, time locked, pending incoming and pending outgoing balance
    pub fn get_balance(tip: Option<u64>, conn: &SqliteConnection) -> Result<Balance, OutputManagerStorageError> {
        #[derive(QueryableByName, Clone)]
        struct BalanceQueryResult {
            #[sql_type = "diesel::sql_types::BigInt"]
            amount: i64,
            #[sql_type = "diesel::sql_types::Text"]
            category: String,
        }
        let balance_query_result = if let Some(val) = tip {
            let balance_query = sql_query(
                "SELECT coalesce(sum(value), 0) as amount, 'available_balance' as category \
                 FROM outputs WHERE status = ? \
                 UNION ALL \
                 SELECT coalesce(sum(value), 0) as amount, 'time_locked_balance' as category \
                 FROM outputs WHERE status = ? AND maturity > ? \
                 UNION ALL \
                 SELECT coalesce(sum(value), 0) as amount, 'pending_incoming_balance' as category \
                 FROM outputs WHERE status = ? OR status = ? OR status = ? \
                 UNION ALL \
                 SELECT coalesce(sum(value), 0) as amount, 'pending_outgoing_balance' as category \
                 FROM outputs WHERE status = ? OR status = ? OR status = ?",
            )
                // available_balance
                .bind::<diesel::sql_types::Integer, _>(OutputStatus::Unspent as i32)
                // time_locked_balance
                .bind::<diesel::sql_types::Integer, _>(OutputStatus::Unspent as i32)
                .bind::<diesel::sql_types::BigInt, _>(val as i64)
                // pending_incoming_balance
                .bind::<diesel::sql_types::Integer, _>(OutputStatus::EncumberedToBeReceived as i32)
                .bind::<diesel::sql_types::Integer, _>(OutputStatus::ShortTermEncumberedToBeReceived as i32)
                .bind::<diesel::sql_types::Integer, _>(OutputStatus::UnspentMinedUnconfirmed as i32)
                // pending_outgoing_balance
                .bind::<diesel::sql_types::Integer, _>(OutputStatus::EncumberedToBeSpent as i32)
                .bind::<diesel::sql_types::Integer, _>(OutputStatus::ShortTermEncumberedToBeSpent as i32)
                .bind::<diesel::sql_types::Integer, _>(OutputStatus::SpentMinedUnconfirmed as i32);
            balance_query.load::<BalanceQueryResult>(conn)?
        } else {
            let balance_query = sql_query(
                "SELECT coalesce(sum(value), 0) as amount, 'available_balance' as category \
                 FROM outputs WHERE status = ? \
                 UNION ALL \
                 SELECT coalesce(sum(value), 0) as amount, 'pending_incoming_balance' as category \
                 FROM outputs WHERE status = ? OR status = ? OR status = ? \
                 UNION ALL \
                 SELECT coalesce(sum(value), 0) as amount, 'pending_outgoing_balance' as category \
                 FROM outputs WHERE status = ? OR status = ? OR status = ?",
            )
                // available_balance
                .bind::<diesel::sql_types::Integer, _>(OutputStatus::Unspent as i32)
                // pending_incoming_balance
                .bind::<diesel::sql_types::Integer, _>(OutputStatus::EncumberedToBeReceived as i32)
                .bind::<diesel::sql_types::Integer, _>(OutputStatus::ShortTermEncumberedToBeReceived as i32)
                .bind::<diesel::sql_types::Integer, _>(OutputStatus::UnspentMinedUnconfirmed as i32)
                // pending_outgoing_balance
                .bind::<diesel::sql_types::Integer, _>(OutputStatus::EncumberedToBeSpent as i32)
                .bind::<diesel::sql_types::Integer, _>(OutputStatus::ShortTermEncumberedToBeSpent as i32)
                .bind::<diesel::sql_types::Integer, _>(OutputStatus::SpentMinedUnconfirmed as i32);
            balance_query.load::<BalanceQueryResult>(conn)?
        };
        let mut available_balance = None;
        let mut time_locked_balance = Some(None);
        let mut pending_incoming_balance = None;
        let mut pending_outgoing_balance = None;
        for balance in balance_query_result.clone() {
            match balance.category.as_str() {
                "available_balance" => available_balance = Some(MicroTari::from(balance.amount as u64)),
                "time_locked_balance" => time_locked_balance = Some(Some(MicroTari::from(balance.amount as u64))),
                "pending_incoming_balance" => pending_incoming_balance = Some(MicroTari::from(balance.amount as u64)),
                "pending_outgoing_balance" => pending_outgoing_balance = Some(MicroTari::from(balance.amount as u64)),
                _ => {
                    return Err(OutputManagerStorageError::UnexpectedResult(
                        "Unexpected category in balance query".to_string(),
                    ))
                },
            }
        }

        Ok(Balance {
            available_balance: available_balance.ok_or_else(|| {
                OutputManagerStorageError::UnexpectedResult("Available balance could not be calculated".to_string())
            })?,
            time_locked_balance: time_locked_balance.ok_or_else(|| {
                OutputManagerStorageError::UnexpectedResult("Time locked balance could not be calculated".to_string())
            })?,
            pending_incoming_balance: pending_incoming_balance.ok_or_else(|| {
                OutputManagerStorageError::UnexpectedResult(
                    "Pending incoming balance could not be calculated".to_string(),
                )
            })?,
            pending_outgoing_balance: pending_outgoing_balance.ok_or_else(|| {
                OutputManagerStorageError::UnexpectedResult(
                    "Pending outgoing balance could not be calculated".to_string(),
                )
            })?,
        })
    }

    pub fn find_by_commitment(
        commitment: &[u8],
        conn: &SqliteConnection,
    ) -> Result<OutputSql, OutputManagerStorageError> {
        Ok(outputs::table
            .filter(outputs::commitment.eq(commitment))
            .first::<OutputSql>(conn)?)
    }

    pub fn find_by_commitment_and_cancelled(
        commitment: &[u8],
        cancelled: bool,
        conn: &SqliteConnection,
    ) -> Result<OutputSql, OutputManagerStorageError> {
        let cancelled_flag = OutputStatus::CancelledInbound as i32;

        let mut request = outputs::table.filter(outputs::commitment.eq(commitment)).into_boxed();
        if cancelled {
            request = request.filter(outputs::status.eq(cancelled_flag))
        } else {
            request = request.filter(outputs::status.ne(cancelled_flag))
        };

        Ok(request.first::<OutputSql>(conn)?)
    }

    pub fn find_by_tx_id_and_status(
        tx_id: TxId,
        status: OutputStatus,
        conn: &SqliteConnection,
    ) -> Result<Vec<OutputSql>, OutputManagerStorageError> {
        Ok(outputs::table
            .filter(
                outputs::received_in_tx_id
                    .eq(Some(tx_id as i64))
                    .or(outputs::spent_in_tx_id.eq(Some(tx_id as i64))),
            )
            .filter(outputs::status.eq(status as i32))
            .load(conn)?)
    }

    /// Find outputs via tx_id that are encumbered. Any outputs that are encumbered cannot be marked as spent.
    pub fn find_by_tx_id_and_encumbered(
        tx_id: TxId,
        conn: &SqliteConnection,
    ) -> Result<Vec<OutputSql>, OutputManagerStorageError> {
        Ok(outputs::table
            .filter(
                outputs::received_in_tx_id
                    .eq(Some(tx_id as i64))
                    .or(outputs::spent_in_tx_id.eq(Some(tx_id as i64))),
            )
            .filter(
                outputs::status
                    .eq(OutputStatus::EncumberedToBeReceived as i32)
                    .or(outputs::status.eq(OutputStatus::EncumberedToBeSpent as i32))
                    .or(outputs::status.eq(OutputStatus::ShortTermEncumberedToBeReceived as i32))
                    .or(outputs::status.eq(OutputStatus::ShortTermEncumberedToBeSpent as i32)),
            )
            .load(conn)?)
    }

    /// Find a particular Output, if it exists and is in the specified Spent state
    pub fn find_status(
        spending_key: &[u8],
        status: OutputStatus,
        conn: &SqliteConnection,
    ) -> Result<OutputSql, OutputManagerStorageError> {
        Ok(outputs::table
            .filter(outputs::status.eq(status as i32))
            .filter(outputs::spending_key.eq(spending_key))
            .first::<OutputSql>(conn)?)
    }

    /// Find a particular Output, if it exists and is in the specified Spent state
    pub fn find_pending_coinbase_at_block_height(
        block_height: u64,
        conn: &SqliteConnection,
    ) -> Result<OutputSql, OutputManagerStorageError> {
        Ok(outputs::table
            .filter(outputs::status.ne(OutputStatus::Unspent as i32))
            .filter(outputs::coinbase_block_height.eq(block_height as i64))
            .first::<OutputSql>(conn)?)
    }

    pub fn delete(&self, conn: &SqliteConnection) -> Result<(), OutputManagerStorageError> {
        let num_deleted =
            diesel::delete(outputs::table.filter(outputs::spending_key.eq(&self.spending_key))).execute(conn)?;

        if num_deleted == 0 {
            return Err(OutputManagerStorageError::ValuesNotFound);
        }

        Ok(())
    }

    pub fn update(
        &self,
        updated_output: UpdateOutput,
        conn: &SqliteConnection,
    ) -> Result<OutputSql, OutputManagerStorageError> {
        diesel::update(outputs::table.filter(outputs::id.eq(&self.id)))
            .set(UpdateOutputSql::from(updated_output))
            .execute(conn)
            .num_rows_affected_or_not_found(1)?;

        OutputSql::find(&self.spending_key, conn)
    }

    /// Update the changed fields of this record after encryption/decryption is performed
    pub fn update_encryption(&self, conn: &SqliteConnection) -> Result<(), OutputManagerStorageError> {
        let _ = self.update(
            UpdateOutput {
                spending_key: Some(self.spending_key.clone()),
                script_private_key: Some(self.script_private_key.clone()),
                ..Default::default()
            },
            conn,
        )?;
        Ok(())
    }
}

/// Conversion from an DbUnblindedOutput to the Sql datatype form
impl TryFrom<OutputSql> for DbUnblindedOutput {
    type Error = OutputManagerStorageError;

    fn try_from(o: OutputSql) -> Result<Self, Self::Error> {
        let unblinded_output = UnblindedOutput::new(
            MicroTari::from(o.value as u64),
            PrivateKey::from_vec(&o.spending_key).map_err(|_| {
                error!(
                    target: LOG_TARGET,
                    "Could not create PrivateKey from stored bytes, They might be encrypted"
                );
                OutputManagerStorageError::ConversionError
            })?,
            OutputFeatures {
                flags: OutputFlags::from_bits(o.flags as u8).ok_or(OutputManagerStorageError::ConversionError)?,
                maturity: o.maturity as u64,
            },
            TariScript::from_bytes(o.script.as_slice())?,
            ExecutionStack::from_bytes(o.input_data.as_slice())?,
            PrivateKey::from_vec(&o.script_private_key).map_err(|_| {
                error!(
                    target: LOG_TARGET,
                    "Could not create PrivateKey from stored bytes, They might be encrypted"
                );
                OutputManagerStorageError::ConversionError
            })?,
            PublicKey::from_vec(&o.sender_offset_public_key).map_err(|_| {
                error!(
                    target: LOG_TARGET,
                    "Could not create PublicKey from stored bytes, They might be encrypted"
                );
                OutputManagerStorageError::ConversionError
            })?,
            ComSignature::new(
                Commitment::from_vec(&o.metadata_signature_nonce).map_err(|_| {
                    error!(
                        target: LOG_TARGET,
                        "Could not create PublicKey from stored bytes, They might be encrypted"
                    );
                    OutputManagerStorageError::ConversionError
                })?,
                PrivateKey::from_vec(&o.metadata_signature_u_key).map_err(|_| {
                    error!(
                        target: LOG_TARGET,
                        "Could not create PrivateKey from stored bytes, They might be encrypted"
                    );
                    OutputManagerStorageError::ConversionError
                })?,
                PrivateKey::from_vec(&o.metadata_signature_v_key).map_err(|_| {
                    error!(
                        target: LOG_TARGET,
                        "Could not create PrivateKey from stored bytes, They might be encrypted"
                    );
                    OutputManagerStorageError::ConversionError
                })?,
            ),
        );

        let hash = match o.hash {
            None => {
                let factories = CryptoFactories::default();
                unblinded_output.as_transaction_output(&factories)?.hash()
            },
            Some(v) => v,
        };
        let commitment = match o.commitment {
            None => {
                let factories = CryptoFactories::default();
                factories
                    .commitment
                    .commit(&unblinded_output.spending_key, &unblinded_output.value.into())
            },
            Some(c) => Commitment::from_vec(&c)?,
        };

        Ok(Self {
            commitment,
            unblinded_output,
            hash,
            mined_height: o.mined_height.map(|mh| mh as u64),
            mined_in_block: o.mined_in_block,
            mined_mmr_position: o.mined_mmr_position.map(|mp| mp as u64),
            marked_deleted_at_height: o.marked_deleted_at_height.map(|d| d as u64),
            marked_deleted_in_block: o.marked_deleted_in_block,
        })
    }
}

impl Encryptable<Aes256Gcm> for OutputSql {
    fn encrypt(&mut self, cipher: &Aes256Gcm) -> Result<(), AeadError> {
        self.spending_key = encrypt_bytes_integral_nonce(cipher, self.spending_key.clone())?;
        self.script_private_key = encrypt_bytes_integral_nonce(cipher, self.script_private_key.clone())?;
        Ok(())
    }

    fn decrypt(&mut self, cipher: &Aes256Gcm) -> Result<(), AeadError> {
        self.spending_key = decrypt_bytes_integral_nonce(cipher, self.spending_key.clone())?;
        self.script_private_key = decrypt_bytes_integral_nonce(cipher, self.script_private_key.clone())?;
        Ok(())
    }
}

impl From<OutputSql> for NewOutputSql {
    fn from(o: OutputSql) -> Self {
        Self {
            commitment: o.commitment,
            spending_key: o.spending_key,
            value: o.value,
            flags: o.flags,
            maturity: o.maturity,
            status: o.status,
            hash: o.hash,
            script: o.script,
            input_data: o.input_data,
            script_private_key: o.script_private_key,
            sender_offset_public_key: o.sender_offset_public_key,
            metadata_signature_nonce: o.metadata_signature_nonce,
            metadata_signature_u_key: o.metadata_signature_u_key,
            metadata_signature_v_key: o.metadata_signature_v_key,
            received_in_tx_id: o.received_in_tx_id,
            coinbase_block_height: o.coinbase_block_height,
        }
    }
}

impl PartialEq<NewOutputSql> for OutputSql {
    fn eq(&self, other: &NewOutputSql) -> bool {
        &NewOutputSql::from(self.clone()) == other
    }
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
            received_in_tx_id: u.received_in_tx_id.map(|o| o.map(|t| t as i64)),
            spent_in_tx_id: u.spent_in_tx_id.map(|o| o.map(|t| t as i64)),
        }
    }
}

#[derive(Clone, Debug, Queryable, Identifiable)]
#[table_name = "key_manager_states"]
struct KeyManagerStateSql {
    id: i32,
    master_key: Vec<u8>,
    branch_seed: String,
    primary_key_index: i64,
    timestamp: NaiveDateTime,
}

#[derive(Clone, Debug, Insertable)]
#[table_name = "key_manager_states"]
struct NewKeyManagerStateSql {
    master_key: Vec<u8>,
    branch_seed: String,
    primary_key_index: i64,
    timestamp: NaiveDateTime,
}

impl From<KeyManagerState> for NewKeyManagerStateSql {
    fn from(km: KeyManagerState) -> Self {
        Self {
            master_key: km.master_key.to_vec(),
            branch_seed: km.branch_seed,
            primary_key_index: km.primary_key_index as i64,
            timestamp: Utc::now().naive_utc(),
        }
    }
}
impl TryFrom<KeyManagerStateSql> for KeyManagerState {
    type Error = OutputManagerStorageError;

    fn try_from(km: KeyManagerStateSql) -> Result<Self, Self::Error> {
        Ok(Self {
            master_key: PrivateKey::from_vec(&km.master_key).map_err(|_| OutputManagerStorageError::ConversionError)?,
            branch_seed: km.branch_seed,
            primary_key_index: km.primary_key_index as u64,
        })
    }
}

impl NewKeyManagerStateSql {
    fn commit(&self, conn: &SqliteConnection) -> Result<(), OutputManagerStorageError> {
        diesel::insert_into(key_manager_states::table)
            .values(self.clone())
            .execute(conn)?;
        Ok(())
    }
}

impl KeyManagerStateSql {
    pub fn get_state(conn: &SqliteConnection) -> Result<KeyManagerStateSql, OutputManagerStorageError> {
        key_manager_states::table
            .first::<KeyManagerStateSql>(conn)
            .map_err(|_| OutputManagerStorageError::KeyManagerNotInitialized)
    }

    pub fn set_state(&self, conn: &SqliteConnection) -> Result<(), OutputManagerStorageError> {
        match KeyManagerStateSql::get_state(conn) {
            Ok(km) => {
                let update = KeyManagerStateUpdateSql {
                    master_key: Some(self.master_key.clone()),
                    branch_seed: Some(self.branch_seed.clone()),
                    primary_key_index: Some(self.primary_key_index),
                };

                diesel::update(key_manager_states::table.filter(key_manager_states::id.eq(&km.id)))
                    .set(update)
                    .execute(conn)
                    .num_rows_affected_or_not_found(1)?;
            },
            Err(_) => {
                let inserter = NewKeyManagerStateSql {
                    master_key: self.master_key.clone(),
                    branch_seed: self.branch_seed.clone(),
                    primary_key_index: self.primary_key_index,
                    timestamp: self.timestamp,
                };
                inserter.commit(conn)?;
            },
        }
        Ok(())
    }

    pub fn increment_index(conn: &SqliteConnection) -> Result<i64, OutputManagerStorageError> {
        Ok(match KeyManagerStateSql::get_state(conn) {
            Ok(km) => {
                let current_index = km.primary_key_index + 1;
                let update = KeyManagerStateUpdateSql {
                    master_key: None,
                    branch_seed: None,
                    primary_key_index: Some(current_index),
                };
                diesel::update(key_manager_states::table.filter(key_manager_states::id.eq(&km.id)))
                    .set(update)
                    .execute(conn)
                    .num_rows_affected_or_not_found(1)?;
                current_index
            },
            Err(_) => return Err(OutputManagerStorageError::KeyManagerNotInitialized),
        })
    }

    pub fn set_index(index: u64, conn: &SqliteConnection) -> Result<(), OutputManagerStorageError> {
        match KeyManagerStateSql::get_state(conn) {
            Ok(km) => {
                let update = KeyManagerStateUpdateSql {
                    master_key: None,
                    branch_seed: None,
                    primary_key_index: Some(index as i64),
                };
                diesel::update(key_manager_states::table.filter(key_manager_states::id.eq(&km.id)))
                    .set(update)
                    .execute(conn)
                    .num_rows_affected_or_not_found(1)?;
                Ok(())
            },
            Err(_) => Err(OutputManagerStorageError::KeyManagerNotInitialized),
        }
    }
}

#[derive(AsChangeset)]
#[table_name = "key_manager_states"]
struct KeyManagerStateUpdateSql {
    master_key: Option<Vec<u8>>,
    branch_seed: Option<String>,
    primary_key_index: Option<i64>,
}

impl Encryptable<Aes256Gcm> for KeyManagerStateSql {
    fn encrypt(&mut self, cipher: &Aes256Gcm) -> Result<(), Error> {
        let encrypted_master_key = encrypt_bytes_integral_nonce(cipher, self.master_key.clone())?;
        let encrypted_branch_seed = encrypt_bytes_integral_nonce(cipher, self.branch_seed.clone().into_bytes())?;
        self.master_key = encrypted_master_key;
        self.branch_seed = encrypted_branch_seed.to_hex();
        Ok(())
    }

    fn decrypt(&mut self, cipher: &Aes256Gcm) -> Result<(), Error> {
        let decrypted_master_key = decrypt_bytes_integral_nonce(cipher, self.master_key.clone())?;
        let decrypted_branch_seed =
            decrypt_bytes_integral_nonce(cipher, from_hex(self.branch_seed.as_str()).map_err(|_| Error)?)?;
        self.master_key = decrypted_master_key;
        self.branch_seed = from_utf8(decrypted_branch_seed.as_slice())
            .map_err(|_| Error)?
            .to_string();
        Ok(())
    }
}

impl Encryptable<Aes256Gcm> for NewKeyManagerStateSql {
    fn encrypt(&mut self, cipher: &Aes256Gcm) -> Result<(), Error> {
        let encrypted_master_key = encrypt_bytes_integral_nonce(cipher, self.master_key.clone())?;
        let encrypted_branch_seed = encrypt_bytes_integral_nonce(cipher, self.branch_seed.clone().as_bytes().to_vec())?;
        self.master_key = encrypted_master_key;
        self.branch_seed = encrypted_branch_seed.to_hex();
        Ok(())
    }

    fn decrypt(&mut self, _cipher: &Aes256Gcm) -> Result<(), Error> {
        unimplemented!("Not supported")
        // let decrypted_master_key = decrypt_bytes_integral_nonce(&cipher, self.master_key.clone())?;
        // let decrypted_branch_seed =
        //     decrypt_bytes_integral_nonce(&cipher, from_hex(self.branch_seed.as_str()).map_err(|_| Error)?)?;
        // self.master_key = decrypted_master_key;
        // self.branch_seed = from_utf8(decrypted_branch_seed.as_slice())
        //     .map_err(|_| Error)?
        //     .to_string();
        // Ok(())
    }
}

#[derive(Clone, Debug, Queryable, Insertable, Identifiable, PartialEq, AsChangeset)]
#[table_name = "known_one_sided_payment_scripts"]
#[primary_key(script_hash)]
// #[identifiable_options(primary_key(hash))]
pub struct KnownOneSidedPaymentScriptSql {
    pub script_hash: Vec<u8>,
    pub private_key: Vec<u8>,
    pub script: Vec<u8>,
    pub input: Vec<u8>,
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

    /// Find a particular Output, if it exists
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
        let _ = self.update(
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
            OutputManagerStorageError::ConversionError
        })?;
        let script = TariScript::from_bytes(&o.script).map_err(|_| {
            error!(target: LOG_TARGET, "Could not create tari script from stored bytes");
            OutputManagerStorageError::ConversionError
        })?;
        let input = ExecutionStack::from_bytes(&o.input).map_err(|_| {
            error!(target: LOG_TARGET, "Could not create execution stack from stored bytes");
            OutputManagerStorageError::ConversionError
        })?;
        Ok(KnownOneSidedPaymentScript {
            script_hash,
            private_key,
            script,
            input,
        })
    }
}

/// Conversion from an KnownOneSidedPaymentScriptSQL to the datatype form
impl From<KnownOneSidedPaymentScript> for KnownOneSidedPaymentScriptSql {
    fn from(known_script: KnownOneSidedPaymentScript) -> Self {
        let script_hash = known_script.script_hash;
        let private_key = known_script.private_key.as_bytes().to_vec();
        let script = known_script.script.as_bytes().to_vec();
        let input = known_script.input.as_bytes().to_vec();
        KnownOneSidedPaymentScriptSql {
            script_hash,
            private_key,
            script,
            input,
        }
    }
}

impl Encryptable<Aes256Gcm> for KnownOneSidedPaymentScriptSql {
    fn encrypt(&mut self, cipher: &Aes256Gcm) -> Result<(), AeadError> {
        self.private_key = encrypt_bytes_integral_nonce(cipher, self.private_key.clone())?;
        Ok(())
    }

    fn decrypt(&mut self, cipher: &Aes256Gcm) -> Result<(), AeadError> {
        self.private_key = decrypt_bytes_integral_nonce(cipher, self.private_key.clone())?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::convert::TryFrom;

    use aes_gcm::{
        aead::{generic_array::GenericArray, NewAead},
        Aes256Gcm,
    };
    use diesel::{Connection, SqliteConnection};
    use rand::{rngs::OsRng, RngCore};
    use tari_crypto::{keys::SecretKey, script};
    use tempfile::tempdir;

    use tari_common_types::types::{CommitmentFactory, PrivateKey};
    use tari_core::transactions::{
        helpers::{create_unblinded_output, TestParams as TestParamsHelpers},
        tari_amount::MicroTari,
        transaction::{OutputFeatures, TransactionInput, UnblindedOutput},
        CryptoFactories,
    };
    use tari_test_utils::random;

    use crate::{
        output_manager_service::storage::{
            database::{DbKey, KeyManagerState, OutputManagerBackend},
            models::DbUnblindedOutput,
            sqlite_db::{
                KeyManagerStateSql,
                NewKeyManagerStateSql,
                NewOutputSql,
                OutputManagerSqliteDatabase,
                OutputSql,
                OutputStatus,
                UpdateOutput,
            },
        },
        storage::sqlite_utilities::WalletDbConnection,
        util::encryption::Encryptable,
    };

    pub fn make_input(val: MicroTari) -> (TransactionInput, UnblindedOutput) {
        let test_params = TestParamsHelpers::new();
        let factory = CommitmentFactory::default();

        let unblinded_output = create_unblinded_output(script!(Nop), OutputFeatures::default(), test_params, val);
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
            let uo = DbUnblindedOutput::from_unblinded_output(uo, &factories).unwrap();
            let o = NewOutputSql::new(uo, OutputStatus::Unspent, None, None).unwrap();
            outputs.push(o.clone());
            outputs_unspent.push(o.clone());
            o.commit(&conn).unwrap();
        }

        for _i in 0..3 {
            let (_, uo) = make_input(MicroTari::from(100 + OsRng.next_u64() % 1000));
            let uo = DbUnblindedOutput::from_unblinded_output(uo, &factories).unwrap();
            let o = NewOutputSql::new(uo, OutputStatus::Spent, None, None).unwrap();
            outputs.push(o.clone());
            outputs_spent.push(o.clone());
            o.commit(&conn).unwrap();
        }

        assert_eq!(OutputSql::index(&conn).unwrap(), outputs);
        assert_eq!(
            OutputSql::index_status(OutputStatus::Unspent, &conn).unwrap(),
            outputs_unspent
        );
        assert_eq!(
            OutputSql::index_status(OutputStatus::Spent, &conn).unwrap(),
            outputs_spent
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

        let _ = OutputSql::find(&outputs[4].spending_key, &conn).unwrap().delete(&conn);

        assert_eq!(OutputSql::index(&conn).unwrap().len(), 4);

        let _updated1 = OutputSql::find(&outputs[0].spending_key, &conn)
            .unwrap()
            .update(
                UpdateOutput {
                    status: Some(OutputStatus::Unspent),
                    received_in_tx_id: Some(Some(44u64)),
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
                    received_in_tx_id: Some(Some(44u64)),
                    ..Default::default()
                },
                &conn,
            )
            .unwrap();

        let result = OutputSql::find_by_tx_id_and_encumbered(44u64, &conn).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].spending_key, outputs[1].spending_key);
    }

    #[test]
    fn test_key_manager_crud() {
        let db_name = format!("{}.sqlite3", random::string(8).as_str());
        let temp_dir = tempdir().unwrap();
        let db_folder = temp_dir.path().to_str().unwrap().to_string();
        let db_path = format!("{}{}", db_folder, db_name);

        embed_migrations!("./migrations");
        let conn = SqliteConnection::establish(&db_path).unwrap_or_else(|_| panic!("Error connecting to {}", db_path));

        embedded_migrations::run_with_output(&conn, &mut std::io::stdout()).expect("Migration failed");

        conn.execute("PRAGMA foreign_keys = ON").unwrap();

        assert!(KeyManagerStateSql::get_state(&conn).is_err());

        let state1 = KeyManagerState {
            master_key: PrivateKey::random(&mut OsRng),
            branch_seed: random::string(8),
            primary_key_index: 0,
        };

        NewKeyManagerStateSql::from(state1.clone()).commit(&conn).unwrap();
        let state1_read = KeyManagerStateSql::get_state(&conn).unwrap();

        assert_eq!(state1, KeyManagerState::try_from(state1_read).unwrap());

        KeyManagerStateSql::increment_index(&conn).unwrap();
        KeyManagerStateSql::increment_index(&conn).unwrap();

        let state3_read = KeyManagerStateSql::get_state(&conn).unwrap();

        assert_eq!(state3_read.primary_key_index, 2);
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
        let uo = DbUnblindedOutput::from_unblinded_output(uo, &factories).unwrap();
        let output = NewOutputSql::new(uo, OutputStatus::Unspent, None, None).unwrap();

        let key = GenericArray::from_slice(b"an example very very secret key.");
        let cipher = Aes256Gcm::new(key);

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

        let wrong_key = GenericArray::from_slice(b"an example very very wrong key!!");
        let wrong_cipher = Aes256Gcm::new(wrong_key);
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
    fn test_key_manager_encryption() {
        let db_name = format!("{}.sqlite3", random::string(8).as_str());
        let temp_dir = tempdir().unwrap();
        let db_folder = temp_dir.path().to_str().unwrap().to_string();
        let db_path = format!("{}{}", db_folder, db_name);

        embed_migrations!("./migrations");
        let conn = SqliteConnection::establish(&db_path).unwrap_or_else(|_| panic!("Error connecting to {}", db_path));

        embedded_migrations::run_with_output(&conn, &mut std::io::stdout()).expect("Migration failed");

        let key = GenericArray::from_slice(b"an example very very secret key.");
        let cipher = Aes256Gcm::new(key);

        let starting_state = KeyManagerState {
            master_key: PrivateKey::random(&mut OsRng),
            branch_seed: "boop boop".to_string(),
            primary_key_index: 1,
        };

        NewKeyManagerStateSql::from(starting_state.clone())
            .commit(&conn)
            .unwrap();

        let state_sql = KeyManagerStateSql::get_state(&conn).unwrap();

        let mut encrypted_state = state_sql;
        encrypted_state.encrypt(&cipher).unwrap();

        encrypted_state.set_state(&conn).unwrap();
        KeyManagerStateSql::increment_index(&conn).unwrap();
        let mut db_state = KeyManagerStateSql::get_state(&conn).unwrap();

        assert!(KeyManagerState::try_from(db_state.clone()).is_err());
        assert_eq!(db_state.primary_key_index, 2);

        db_state.decrypt(&cipher).unwrap();
        let decrypted_data = KeyManagerState::try_from(db_state).unwrap();

        assert_eq!(decrypted_data.master_key, starting_state.master_key);
        assert_eq!(decrypted_data.branch_seed, starting_state.branch_seed);
        assert_eq!(decrypted_data.primary_key_index, 2);
    }

    #[test]
    fn test_apply_remove_encryption() {
        let db_name = format!("{}.sqlite3", random::string(8).as_str());
        let temp_dir = tempdir().unwrap();
        let db_folder = temp_dir.path().to_str().unwrap().to_string();
        let db_path = format!("{}{}", db_folder, db_name);

        embed_migrations!("./migrations");
        let conn = SqliteConnection::establish(&db_path).unwrap_or_else(|_| panic!("Error connecting to {}", db_path));

        embedded_migrations::run_with_output(&conn, &mut std::io::stdout()).expect("Migration failed");
        let factories = CryptoFactories::default();

        let starting_state = KeyManagerState {
            master_key: PrivateKey::random(&mut OsRng),
            branch_seed: "boop boop".to_string(),
            primary_key_index: 1,
        };

        let _state_sql = NewKeyManagerStateSql::from(starting_state).commit(&conn).unwrap();

        let (_, uo) = make_input(MicroTari::from(100 + OsRng.next_u64() % 1000));
        let uo = DbUnblindedOutput::from_unblinded_output(uo, &factories).unwrap();
        let output = NewOutputSql::new(uo, OutputStatus::Unspent, None, None).unwrap();
        output.commit(&conn).unwrap();

        let (_, uo2) = make_input(MicroTari::from(100 + OsRng.next_u64() % 1000));
        let uo2 = DbUnblindedOutput::from_unblinded_output(uo2, &factories).unwrap();
        let output2 = NewOutputSql::new(uo2, OutputStatus::Unspent, None, None).unwrap();
        output2.commit(&conn).unwrap();

        let key = GenericArray::from_slice(b"an example very very secret key.");
        let cipher = Aes256Gcm::new(key);

        let connection = WalletDbConnection::new(conn, None);

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
