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
        storage::{
            database::{
                DbKey,
                DbKeyValuePair,
                DbValue,
                KeyManagerState,
                OutputManagerBackend,
                PendingTransactionOutputs,
                WriteOperation,
            },
            models::DbUnblindedOutput,
        },
        TxId,
    },
    schema::{key_manager_states, outputs, pending_transaction_outputs},
    storage::sqlite_utilities::WalletDbConnection,
    util::encryption::{decrypt_bytes_integral_nonce, encrypt_bytes_integral_nonce, Encryptable},
};
use aes_gcm::{aead::Error as AeadError, Aes256Gcm, Error};
use chrono::{Duration as ChronoDuration, NaiveDateTime, Utc};
use diesel::{prelude::*, result::Error as DieselError, SqliteConnection};
use log::*;
use std::{
    collections::HashMap,
    convert::TryFrom,
    str::from_utf8,
    sync::{Arc, RwLock},
    time::Duration,
};
use tari_core::{
    tari_utilities::hash::Hashable,
    transactions::{
        tari_amount::MicroTari,
        transaction::{OutputFeatures, OutputFlags, UnblindedOutput},
        types::{Commitment, CryptoFactories, PrivateKey, PublicKey},
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
}
impl OutputManagerBackend for OutputManagerSqliteDatabase {
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
            DbKey::PendingTransactionOutputs(tx_id) => match PendingTransactionOutputSql::find(*tx_id, &(*conn)) {
                Ok(p) => {
                    let mut outputs = OutputSql::find_by_tx_id_and_encumbered(*tx_id, &(*conn))?;
                    for o in outputs.iter_mut() {
                        self.decrypt_if_necessary(o)?;
                    }
                    Some(DbValue::PendingTransactionOutputs(Box::new(
                        pending_transaction_outputs_from_sql_outputs(
                            p.tx_id as u64,
                            &p.timestamp,
                            outputs,
                            p.coinbase_block_height.map(|h| h as u64),
                        )?,
                    )))
                },
                Err(e) => {
                    match e {
                        OutputManagerStorageError::DieselError(DieselError::NotFound) => (),
                        e => return Err(e),
                    };
                    None
                },
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
            DbKey::AllPendingTransactionOutputs => {
                let pending_sql_txs = PendingTransactionOutputSql::index(&(*conn))?;
                let mut pending_txs = HashMap::new();
                for p_tx in pending_sql_txs {
                    let mut outputs = OutputSql::find_by_tx_id_and_encumbered(p_tx.tx_id as u64, &(*conn))?;

                    for o in outputs.iter_mut() {
                        self.decrypt_if_necessary(o)?;
                    }

                    pending_txs.insert(
                        p_tx.tx_id as u64,
                        pending_transaction_outputs_from_sql_outputs(
                            p_tx.tx_id as u64,
                            &p_tx.timestamp,
                            outputs,
                            p_tx.coinbase_block_height.map(|h| h as u64),
                        )?,
                    );
                }
                Some(DbValue::AllPendingTransactionOutputs(pending_txs))
            },
            DbKey::KeyManagerState => match KeyManagerStateSql::get_state(&(*conn)).ok() {
                None => None,
                Some(mut km) => {
                    self.decrypt_if_necessary(&mut km)?;

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
        };

        Ok(result)
    }

    fn write(&self, op: WriteOperation) -> Result<Option<DbValue>, OutputManagerStorageError> {
        let conn = self.database_connection.acquire_lock();

        match op {
            WriteOperation::Insert(kvp) => match kvp {
                DbKeyValuePair::SpentOutput(c, o) => {
                    if OutputSql::find_by_commitment(&c.to_vec(), &(*conn)).is_ok() {
                        return Err(OutputManagerStorageError::DuplicateOutput);
                    }
                    let mut new_output = NewOutputSql::new(*o, OutputStatus::Spent, None);

                    self.encrypt_if_necessary(&mut new_output)?;

                    new_output.commit(&(*conn))?
                },
                DbKeyValuePair::UnspentOutput(c, o) => {
                    if OutputSql::find_by_commitment(&c.to_vec(), &(*conn)).is_ok() {
                        return Err(OutputManagerStorageError::DuplicateOutput);
                    }
                    let mut new_output = NewOutputSql::new(*o, OutputStatus::Unspent, None);
                    self.encrypt_if_necessary(&mut new_output)?;
                    new_output.commit(&(*conn))?
                },
                DbKeyValuePair::PendingTransactionOutputs(tx_id, p) => {
                    if PendingTransactionOutputSql::find(tx_id, &(*conn)).is_ok() {
                        return Err(OutputManagerStorageError::DuplicateOutput);
                    }

                    PendingTransactionOutputSql::new(
                        p.tx_id,
                        true,
                        p.timestamp,
                        p.coinbase_block_height.map(|h| h as i64),
                    )
                    .commit(&(*conn))?;
                    for o in p.outputs_to_be_spent {
                        let mut new_output = NewOutputSql::new(o, OutputStatus::EncumberedToBeSpent, Some(p.tx_id));
                        self.encrypt_if_necessary(&mut new_output)?;
                        new_output.commit(&(*conn))?;
                    }
                    for o in p.outputs_to_be_received {
                        let mut new_output = NewOutputSql::new(o, OutputStatus::EncumberedToBeReceived, Some(p.tx_id));
                        self.encrypt_if_necessary(&mut new_output)?;
                        new_output.commit(&(*conn))?;
                    }
                },
                DbKeyValuePair::KeyManagerState(km) => {
                    let mut km_sql = KeyManagerStateSql::from(km);
                    self.encrypt_if_necessary(&mut km_sql)?;
                    km_sql.set_state(&(*conn))?
                },
            },
            WriteOperation::Remove(k) => match k {
                DbKey::SpentOutput(s) => match OutputSql::find_status(&s.to_vec(), OutputStatus::Spent, &(*conn)) {
                    Ok(o) => {
                        o.delete(&(*conn))?;
                        return Ok(Some(DbValue::SpentOutput(Box::new(DbUnblindedOutput::try_from(o)?))));
                    },
                    Err(e) => {
                        match e {
                            OutputManagerStorageError::DieselError(DieselError::NotFound) => (),
                            e => return Err(e),
                        };
                    },
                },
                DbKey::UnspentOutput(k) => match OutputSql::find_status(&k.to_vec(), OutputStatus::Unspent, &(*conn)) {
                    Ok(o) => {
                        o.delete(&(*conn))?;
                        return Ok(Some(DbValue::UnspentOutput(Box::new(DbUnblindedOutput::try_from(o)?))));
                    },
                    Err(e) => {
                        match e {
                            OutputManagerStorageError::DieselError(DieselError::NotFound) => (),
                            e => return Err(e),
                        };
                    },
                },
                DbKey::PendingTransactionOutputs(tx_id) => match PendingTransactionOutputSql::find(tx_id, &(*conn)) {
                    Ok(p) => {
                        let mut outputs = OutputSql::find_by_tx_id_and_encumbered(p.tx_id as u64, &(*conn))?;

                        for o in outputs.iter_mut() {
                            self.decrypt_if_necessary(o)?;
                        }

                        p.delete(&(*conn))?;
                        return Ok(Some(DbValue::PendingTransactionOutputs(Box::new(
                            pending_transaction_outputs_from_sql_outputs(
                                p.tx_id as u64,
                                &p.timestamp,
                                outputs,
                                p.coinbase_block_height.map(|h| h as u64),
                            )?,
                        ))));
                    },
                    Err(e) => {
                        match e {
                            OutputManagerStorageError::DieselError(DieselError::NotFound) => (),
                            e => return Err(e),
                        };
                    },
                },
                DbKey::UnspentOutputs => return Err(OutputManagerStorageError::OperationNotSupported),
                DbKey::SpentOutputs => return Err(OutputManagerStorageError::OperationNotSupported),
                DbKey::AllPendingTransactionOutputs => return Err(OutputManagerStorageError::OperationNotSupported),
                DbKey::KeyManagerState => return Err(OutputManagerStorageError::OperationNotSupported),
                DbKey::InvalidOutputs => return Err(OutputManagerStorageError::OperationNotSupported),
                DbKey::TimeLockedUnspentOutputs(_) => return Err(OutputManagerStorageError::OperationNotSupported),
            },
        }

        Ok(None)
    }

    fn confirm_transaction(&self, tx_id: u64) -> Result<(), OutputManagerStorageError> {
        let conn = self.database_connection.acquire_lock();

        match PendingTransactionOutputSql::find(tx_id, &(*conn)) {
            Ok(p) => {
                let outputs = OutputSql::find_by_tx_id_and_encumbered(tx_id, &(*conn))?;

                for o in outputs {
                    if o.status == (OutputStatus::EncumberedToBeReceived as i32) {
                        o.update(
                            UpdateOutput {
                                status: Some(OutputStatus::Unspent),
                                tx_id: None,
                                spending_key: None,
                                height: None,
                                script_private_key: None,
                            },
                            &(*conn),
                        )?;
                    } else if o.status == (OutputStatus::EncumberedToBeSpent as i32) {
                        o.update(
                            UpdateOutput {
                                status: Some(OutputStatus::Spent),
                                tx_id: None,
                                spending_key: None,
                                height: None,
                                script_private_key: None,
                            },
                            &(*conn),
                        )?;
                    }
                }

                p.delete(&(*conn))?;
            },
            Err(e) => {
                match e {
                    OutputManagerStorageError::DieselError(DieselError::NotFound) => (),
                    e => return Err(e),
                };
            },
        }

        Ok(())
    }

    fn short_term_encumber_outputs(
        &self,
        tx_id: u64,
        outputs_to_send: &[DbUnblindedOutput],
        outputs_to_receive: &[DbUnblindedOutput],
    ) -> Result<(), OutputManagerStorageError>
    {
        let conn = self.database_connection.acquire_lock();

        let mut outputs_to_be_spent = Vec::with_capacity(outputs_to_send.len());
        for i in outputs_to_send {
            let output = OutputSql::find_by_commitment(i.commitment.as_bytes(), &(*conn))?;
            if output.status == (OutputStatus::Spent as i32) {
                return Err(OutputManagerStorageError::OutputAlreadySpent);
            }
            outputs_to_be_spent.push(output);
        }

        PendingTransactionOutputSql::new(tx_id, true, Utc::now().naive_utc(), None).commit(&(*conn))?;

        for o in outputs_to_be_spent {
            o.update(
                UpdateOutput {
                    status: Some(OutputStatus::EncumberedToBeSpent),
                    tx_id: Some(tx_id),
                    spending_key: None,
                    height: None,
                    script_private_key: None,
                },
                &(*conn),
            )?;
        }

        for co in outputs_to_receive {
            let mut new_output = NewOutputSql::new(co.clone(), OutputStatus::EncumberedToBeReceived, Some(tx_id));
            self.encrypt_if_necessary(&mut new_output)?;
            new_output.commit(&(*conn))?;
        }

        Ok(())
    }

    fn confirm_encumbered_outputs(&self, tx_id: TxId) -> Result<(), OutputManagerStorageError> {
        let conn = self.database_connection.acquire_lock();

        match PendingTransactionOutputSql::find(tx_id, &(*conn)) {
            Ok(p) => {
                p.clear_short_term(&(*conn))?;
            },
            Err(e) => {
                match e {
                    OutputManagerStorageError::DieselError(DieselError::NotFound) => {
                        return Err(OutputManagerStorageError::ValueNotFound(
                            DbKey::PendingTransactionOutputs(tx_id),
                        ))
                    },
                    e => return Err(e),
                };
            },
        }

        Ok(())
    }

    fn clear_short_term_encumberances(&self) -> Result<(), OutputManagerStorageError> {
        let conn = self.database_connection.acquire_lock();

        let pending_transaction_outputs = PendingTransactionOutputSql::index_short_term(&(*conn))?;
        drop(conn);

        for pto in pending_transaction_outputs.iter() {
            self.cancel_pending_transaction(pto.tx_id as u64)?;
        }

        Ok(())
    }

    fn cancel_pending_transaction(&self, tx_id: u64) -> Result<(), OutputManagerStorageError> {
        let conn = self.database_connection.acquire_lock();

        match PendingTransactionOutputSql::find(tx_id, &(*conn)) {
            Ok(p) => {
                let outputs = OutputSql::find_by_tx_id_and_encumbered(tx_id, &(*conn))?;

                for o in outputs {
                    if o.status == (OutputStatus::EncumberedToBeReceived as i32) {
                        o.update(
                            UpdateOutput {
                                status: Some(OutputStatus::CancelledInbound),
                                tx_id: None,
                                spending_key: None,
                                height: None,
                                script_private_key: None,
                            },
                            &(*conn),
                        )?;
                    } else if o.status == (OutputStatus::EncumberedToBeSpent as i32) {
                        o.update(
                            UpdateOutput {
                                status: Some(OutputStatus::Unspent),
                                tx_id: None,
                                spending_key: None,
                                height: None,
                                script_private_key: None,
                            },
                            &(*conn),
                        )?;
                        o.update_null(NullOutputSql { tx_id: None }, &(*conn))?;
                    }
                }

                p.delete(&(*conn))?;
            },
            Err(e) => {
                match e {
                    OutputManagerStorageError::DieselError(DieselError::NotFound) => {
                        return Err(OutputManagerStorageError::ValueNotFound(
                            DbKey::PendingTransactionOutputs(tx_id),
                        ))
                    },
                    e => return Err(e),
                };
            },
        }

        Ok(())
    }

    fn timeout_pending_transactions(&self, period: Duration) -> Result<(), OutputManagerStorageError> {
        let conn = self.database_connection.acquire_lock();

        let older_pending_txs = PendingTransactionOutputSql::index_older(
            Utc::now().naive_utc() - ChronoDuration::from_std(period)?,
            &(*conn),
        )?;
        drop(conn);
        for ptx in older_pending_txs {
            self.cancel_pending_transaction(ptx.tx_id as u64)?;
        }
        Ok(())
    }

    fn increment_key_index(&self) -> Result<(), OutputManagerStorageError> {
        let conn = self.database_connection.acquire_lock();

        KeyManagerStateSql::increment_index(&(*conn))?;

        Ok(())
    }

    fn invalidate_unspent_output(&self, output: &DbUnblindedOutput) -> Result<Option<TxId>, OutputManagerStorageError> {
        let conn = self.database_connection.acquire_lock();
        let output = OutputSql::find_by_commitment(&output.commitment.to_vec(), &conn)?;
        let tx_id = output.tx_id.clone().map(|id| id as u64);
        let _ = output.update(
            UpdateOutput {
                status: Some(OutputStatus::Invalid),
                tx_id: None,
                spending_key: None,
                height: None,
                script_private_key: None,
            },
            &(*conn),
        )?;

        Ok(tx_id)
    }

    fn revalidate_unspent_output(&self, commitment: &Commitment) -> Result<(), OutputManagerStorageError> {
        let conn = self.database_connection.acquire_lock();
        let output = OutputSql::find_by_commitment(&commitment.to_vec(), &conn)?;

        if OutputStatus::try_from(output.status)? != OutputStatus::Invalid {
            return Err(OutputManagerStorageError::ValuesNotFound);
        }
        let _ = output.update(
            UpdateOutput {
                status: Some(OutputStatus::Unspent),
                tx_id: None,
                spending_key: None,
                height: None,
                script_private_key: None,
            },
            &(*conn),
        )?;
        Ok(())
    }

    fn update_spent_output_to_unspent(
        &self,
        commitment: &Commitment,
    ) -> Result<DbUnblindedOutput, OutputManagerStorageError>
    {
        let conn = self.database_connection.acquire_lock();
        let output = OutputSql::find_by_commitment(&commitment.to_vec(), &conn)?;

        if OutputStatus::try_from(output.status)? != OutputStatus::Spent {
            return Err(OutputManagerStorageError::ValuesNotFound);
        }

        let mut o = output.update(
            UpdateOutput {
                status: Some(OutputStatus::Unspent),
                tx_id: None,
                spending_key: None,
                height: None,
                script_private_key: None,
            },
            &(*conn),
        )?;
        self.decrypt_if_necessary(&mut o)?;

        Ok(DbUnblindedOutput::try_from(o)?)
    }

    fn cancel_pending_transaction_at_block_height(&self, block_height: u64) -> Result<(), OutputManagerStorageError> {
        let pending_txs;
        {
            let conn = self.database_connection.acquire_lock();
            pending_txs = PendingTransactionOutputSql::index_block_height(block_height as i64, &conn)?;
        }
        for p in pending_txs {
            self.cancel_pending_transaction(p.tx_id as u64)?;
        }
        Ok(())
    }

    fn update_mined_height(&self, tx_id: u64, height: u64) -> Result<(), OutputManagerStorageError> {
        let conn = self.database_connection.acquire_lock();
        let output = OutputSql::find_by_tx_id(tx_id, &conn)?;

        for o in output.iter() {
            let _ = o.update(
                UpdateOutput {
                    status: None,
                    tx_id: None,
                    spending_key: None,
                    height: Some(height),
                    script_private_key: None,
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

        // Now that all the decryption has been completed we can safely remove the cipher fully
        let _ = (*current_cipher).take();
        Ok(())
    }
}

/// A utility function to construct a PendingTransactionOutputs structure for a TxId, set of Outputs and a Timestamp
fn pending_transaction_outputs_from_sql_outputs(
    tx_id: TxId,
    timestamp: &NaiveDateTime,
    outputs: Vec<OutputSql>,
    coinbase_block_height: Option<u64>,
) -> Result<PendingTransactionOutputs, OutputManagerStorageError>
{
    let mut outputs_to_be_spent = Vec::new();
    let mut outputs_to_be_received = Vec::new();
    for o in outputs {
        if o.status == (OutputStatus::EncumberedToBeReceived as i32) {
            outputs_to_be_received.push(DbUnblindedOutput::try_from(o.clone())?);
        } else if o.status == (OutputStatus::EncumberedToBeSpent as i32) {
            outputs_to_be_spent.push(DbUnblindedOutput::try_from(o.clone())?);
        }
    }

    Ok(PendingTransactionOutputs {
        tx_id,
        outputs_to_be_spent,
        outputs_to_be_received,
        timestamp: *timestamp,
        coinbase_block_height,
    })
}

/// The status of a given output
#[derive(PartialEq)]
enum OutputStatus {
    Unspent,
    Spent,
    EncumberedToBeReceived,
    EncumberedToBeSpent,
    Invalid,
    CancelledInbound,
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
    tx_id: Option<i64>,
    hash: Option<Vec<u8>>,
    script: Vec<u8>,
    input_data: Vec<u8>,
    height: i64,
    script_private_key: Vec<u8>,
    script_offset_public_key: Vec<u8>,
}

impl NewOutputSql {
    pub fn new(output: DbUnblindedOutput, status: OutputStatus, tx_id: Option<TxId>) -> Self {
        Self {
            commitment: Some(output.commitment.to_vec()),
            spending_key: output.unblinded_output.spending_key.to_vec(),
            value: (u64::from(output.unblinded_output.value)) as i64,
            flags: output.unblinded_output.features.flags.bits() as i32,
            maturity: output.unblinded_output.features.maturity as i64,
            status: status as i32,
            tx_id: tx_id.map(|i| i as i64),
            hash: Some(output.hash),
            script: output.unblinded_output.script.as_bytes(),
            input_data: output.unblinded_output.input_data.as_bytes(),
            height: output.unblinded_output.height as i64,
            script_private_key: output.unblinded_output.script_private_key.to_vec(),
            script_offset_public_key: output.unblinded_output.script_offset_public_key.to_vec(),
        }
    }

    /// Write this struct to the database
    pub fn commit(&self, conn: &SqliteConnection) -> Result<(), OutputManagerStorageError> {
        diesel::insert_into(outputs::table).values(self.clone()).execute(conn)?;
        Ok(())
    }
}

impl Encryptable<Aes256Gcm> for NewOutputSql {
    fn encrypt(&mut self, cipher: &Aes256Gcm) -> Result<(), AeadError> {
        self.spending_key = encrypt_bytes_integral_nonce(&cipher, self.spending_key.clone())?;
        self.script_private_key = encrypt_bytes_integral_nonce(&cipher, self.script_private_key.clone())?;
        Ok(())
    }

    fn decrypt(&mut self, cipher: &Aes256Gcm) -> Result<(), AeadError> {
        self.spending_key = decrypt_bytes_integral_nonce(&cipher, self.spending_key.clone())?;
        self.script_private_key = decrypt_bytes_integral_nonce(&cipher, self.script_private_key.clone())?;
        Ok(())
    }
}

#[derive(Clone, Debug, Queryable, Identifiable, PartialEq)]
#[table_name = "outputs"]
struct OutputSql {
    id: i32,
    commitment: Option<Vec<u8>>,
    spending_key: Vec<u8>,
    value: i64,
    flags: i32,
    maturity: i64,
    status: i32,
    tx_id: Option<i64>,
    hash: Option<Vec<u8>>,
    script: Vec<u8>,
    input_data: Vec<u8>,
    height: i64,
    script_private_key: Vec<u8>,
    script_offset_public_key: Vec<u8>,
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
    ) -> Result<Vec<OutputSql>, OutputManagerStorageError>
    {
        Ok(outputs::table.filter(outputs::status.eq(status as i32)).load(conn)?)
    }

    /// Return all unspent outputs that have a maturity above the provided chain tip
    pub fn index_time_locked(tip: u64, conn: &SqliteConnection) -> Result<Vec<OutputSql>, OutputManagerStorageError> {
        Ok(outputs::table
            .filter(outputs::status.eq(OutputStatus::Unspent as i32))
            .filter(outputs::maturity.gt(tip as i64))
            .load(conn)?)
    }

    /// Find a particular Output, if it exists
    pub fn find(spending_key: &[u8], conn: &SqliteConnection) -> Result<OutputSql, OutputManagerStorageError> {
        Ok(outputs::table
            .filter(outputs::spending_key.eq(spending_key))
            .first::<OutputSql>(conn)?)
    }

    /// Find a particular Output by its public_spending_key
    pub fn find_by_commitment(
        commitment: &[u8],
        conn: &SqliteConnection,
    ) -> Result<OutputSql, OutputManagerStorageError>
    {
        let cancelled = OutputStatus::CancelledInbound as i32;
        Ok(outputs::table
            .filter(outputs::status.ne(cancelled))
            .filter(outputs::commitment.eq(commitment))
            .first::<OutputSql>(conn)?)
    }

    /// Find outputs via tx_id
    pub fn find_by_tx_id(tx_id: TxId, conn: &SqliteConnection) -> Result<Vec<OutputSql>, OutputManagerStorageError> {
        Ok(outputs::table
            .filter(outputs::tx_id.eq(Some(tx_id as i64)))
            .load(conn)?)
    }

    /// Find outputs via tx_id that are encumbered. Any outputs that are encumbered cannot be marked as spent.
    pub fn find_by_tx_id_and_encumbered(
        tx_id: TxId,
        conn: &SqliteConnection,
    ) -> Result<Vec<OutputSql>, OutputManagerStorageError>
    {
        Ok(outputs::table
            .filter(outputs::tx_id.eq(Some(tx_id as i64)))
            .filter(
                outputs::status
                    .eq(OutputStatus::EncumberedToBeReceived as i32)
                    .or(outputs::status.eq(OutputStatus::EncumberedToBeSpent as i32)),
            )
            .load(conn)?)
    }

    /// Find a particular Output, if it exists and is in the specified Spent state
    pub fn find_status(
        spending_key: &[u8],
        status: OutputStatus,
        conn: &SqliteConnection,
    ) -> Result<OutputSql, OutputManagerStorageError>
    {
        Ok(outputs::table
            .filter(outputs::status.eq(status as i32))
            .filter(outputs::spending_key.eq(spending_key))
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
    ) -> Result<OutputSql, OutputManagerStorageError>
    {
        let num_updated = diesel::update(outputs::table.filter(outputs::id.eq(&self.id)))
            .set(UpdateOutputSql::from(updated_output))
            .execute(conn)?;

        if num_updated == 0 {
            return Err(OutputManagerStorageError::UnexpectedResult(
                "Database update error".to_string(),
            ));
        }

        Ok(OutputSql::find(&self.spending_key, conn)?)
    }

    /// This function is used to update an existing record to set fields to null
    pub fn update_null(
        &self,
        updated_null: NullOutputSql,
        conn: &SqliteConnection,
    ) -> Result<OutputSql, OutputManagerStorageError>
    {
        let num_updated = diesel::update(outputs::table.filter(outputs::spending_key.eq(&self.spending_key)))
            .set(updated_null)
            .execute(conn)?;

        if num_updated == 0 {
            return Err(OutputManagerStorageError::UnexpectedResult(
                "Database update error".to_string(),
            ));
        }

        Ok(OutputSql::find(&self.spending_key, conn)?)
    }

    /// Update the changed fields of this record after encryption/decryption is performed
    pub fn update_encryption(&self, conn: &SqliteConnection) -> Result<(), OutputManagerStorageError> {
        let _ = self.update(
            UpdateOutput {
                status: None,
                tx_id: None,
                spending_key: Some(self.spending_key.clone()),
                height: None,
                script_private_key: Some(self.script_private_key.clone()),
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
            Some(OutputFeatures {
                flags: OutputFlags::from_bits(o.flags as u8)
                    .ok_or_else(|| OutputManagerStorageError::ConversionError)?,
                maturity: o.maturity as u64,
            }),
            TariScript::from_bytes(o.script.as_slice())?,
            ExecutionStack::from_bytes(o.input_data.as_slice())?,
            o.height as u64,
            PrivateKey::from_vec(&o.script_private_key).map_err(|_| {
                error!(
                    target: LOG_TARGET,
                    "Could not create PrivateKey from stored bytes, They might be encrypted"
                );
                OutputManagerStorageError::ConversionError
            })?,
            PublicKey::from_vec(&o.script_offset_public_key).map_err(|_| {
                error!(
                    target: LOG_TARGET,
                    "Could not create PrivateKey from stored bytes, They might be encrypted"
                );
                OutputManagerStorageError::ConversionError
            })?,
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
        })
    }
}

impl Encryptable<Aes256Gcm> for OutputSql {
    fn encrypt(&mut self, cipher: &Aes256Gcm) -> Result<(), AeadError> {
        self.spending_key = encrypt_bytes_integral_nonce(&cipher, self.spending_key.clone())?;
        self.script_private_key = encrypt_bytes_integral_nonce(&cipher, self.script_private_key.clone())?;
        Ok(())
    }

    fn decrypt(&mut self, cipher: &Aes256Gcm) -> Result<(), AeadError> {
        self.spending_key = decrypt_bytes_integral_nonce(&cipher, self.spending_key.clone())?;
        self.script_private_key = decrypt_bytes_integral_nonce(&cipher, self.script_private_key.clone())?;
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
            tx_id: o.tx_id,
            hash: o.hash,
            script: o.script,
            input_data: o.input_data,
            height: o.height,
            script_private_key: o.script_private_key,
            script_offset_public_key: o.script_offset_public_key,
        }
    }
}

impl PartialEq<NewOutputSql> for OutputSql {
    fn eq(&self, other: &NewOutputSql) -> bool {
        &NewOutputSql::from(self.clone()) == other
    }
}

/// These are the fields that can be updated for an Output
pub struct UpdateOutput {
    status: Option<OutputStatus>,
    tx_id: Option<TxId>,
    spending_key: Option<Vec<u8>>,
    height: Option<u64>,
    script_private_key: Option<Vec<u8>>,
}

#[derive(AsChangeset)]
#[table_name = "outputs"]
pub struct UpdateOutputSql {
    status: Option<i32>,
    tx_id: Option<i64>,
    spending_key: Option<Vec<u8>>,
    height: Option<i64>,
    script_private_key: Option<Vec<u8>>,
}

#[derive(AsChangeset)]
#[table_name = "outputs"]
#[changeset_options(treat_none_as_null = "true")]
/// This struct is used to set the contained field to null
pub struct NullOutputSql {
    tx_id: Option<i64>,
}

/// Map a Rust friendly UpdateOutput to the Sql data type form
impl From<UpdateOutput> for UpdateOutputSql {
    fn from(u: UpdateOutput) -> Self {
        Self {
            status: u.status.map(|t| t as i32),
            tx_id: u.tx_id.map(|t| t as i64),
            spending_key: u.spending_key,
            height: u.height.map(|t| t as i64),
            script_private_key: u.script_private_key,
        }
    }
}

/// This struct represents a PendingTransactionOutputs  in the Sql database. A distinct struct is required to define the
/// Sql friendly equivalent datatypes for the members.
#[derive(Debug, Clone, Queryable, Insertable)]
#[table_name = "pending_transaction_outputs"]
struct PendingTransactionOutputSql {
    tx_id: i64,
    short_term: i32,
    timestamp: NaiveDateTime,
    coinbase_block_height: Option<i64>,
}
impl PendingTransactionOutputSql {
    pub fn new(tx_id: TxId, short_term: bool, timestamp: NaiveDateTime, coinbase_block_height: Option<i64>) -> Self {
        Self {
            tx_id: tx_id as i64,
            short_term: short_term as i32,
            timestamp,
            coinbase_block_height,
        }
    }

    pub fn commit(&self, conn: &SqliteConnection) -> Result<(), OutputManagerStorageError> {
        diesel::insert_into(pending_transaction_outputs::table)
            .values(self.clone())
            .execute(conn)?;
        Ok(())
    }

    pub fn find(
        tx_id: TxId,
        conn: &SqliteConnection,
    ) -> Result<PendingTransactionOutputSql, OutputManagerStorageError>
    {
        Ok(pending_transaction_outputs::table
            .filter(pending_transaction_outputs::tx_id.eq(tx_id as i64))
            .first::<PendingTransactionOutputSql>(conn)?)
    }

    pub fn index(conn: &SqliteConnection) -> Result<Vec<PendingTransactionOutputSql>, OutputManagerStorageError> {
        Ok(pending_transaction_outputs::table.load::<PendingTransactionOutputSql>(conn)?)
    }

    pub fn index_short_term(
        conn: &SqliteConnection,
    ) -> Result<Vec<PendingTransactionOutputSql>, OutputManagerStorageError> {
        Ok(pending_transaction_outputs::table
            .filter(pending_transaction_outputs::short_term.eq(1i32))
            .load::<PendingTransactionOutputSql>(conn)?)
    }

    pub fn index_older(
        timestamp: NaiveDateTime,
        conn: &SqliteConnection,
    ) -> Result<Vec<PendingTransactionOutputSql>, OutputManagerStorageError>
    {
        Ok(pending_transaction_outputs::table
            .filter(pending_transaction_outputs::timestamp.lt(timestamp))
            .load::<PendingTransactionOutputSql>(conn)?)
    }

    /// Find pending transaction outputs with specified block_height
    pub fn index_block_height(
        block_height: i64,
        conn: &SqliteConnection,
    ) -> Result<Vec<PendingTransactionOutputSql>, OutputManagerStorageError>
    {
        Ok(pending_transaction_outputs::table
            .filter(pending_transaction_outputs::coinbase_block_height.eq(block_height))
            .load::<PendingTransactionOutputSql>(conn)?)
    }

    pub fn delete(&self, conn: &SqliteConnection) -> Result<(), OutputManagerStorageError> {
        let num_deleted = diesel::delete(
            pending_transaction_outputs::table.filter(pending_transaction_outputs::tx_id.eq(&self.tx_id)),
        )
        .execute(conn)?;

        if num_deleted == 0 {
            return Err(OutputManagerStorageError::ValuesNotFound);
        }

        let outputs = OutputSql::find_by_tx_id_and_encumbered(self.tx_id as u64, &(*conn))?;
        for o in outputs {
            o.delete(&(*conn))?;
        }

        Ok(())
    }

    /// This function is used to update an existing record to set fields to null
    pub fn clear_short_term(
        &self,
        conn: &SqliteConnection,
    ) -> Result<PendingTransactionOutputSql, OutputManagerStorageError>
    {
        let num_updated = diesel::update(
            pending_transaction_outputs::table.filter(pending_transaction_outputs::tx_id.eq(&self.tx_id)),
        )
        .set(UpdatePendingTransactionOutputSql { short_term: Some(0i32) })
        .execute(conn)?;

        if num_updated == 0 {
            return Err(OutputManagerStorageError::UnexpectedResult(
                "Database update error".to_string(),
            ));
        }

        Ok(PendingTransactionOutputSql::find(self.tx_id as u64, conn)?)
    }
}

#[derive(AsChangeset)]
#[table_name = "pending_transaction_outputs"]
pub struct UpdatePendingTransactionOutputSql {
    short_term: Option<i32>,
}

#[derive(Clone, Debug, Queryable, Insertable)]
#[table_name = "key_manager_states"]
struct KeyManagerStateSql {
    id: Option<i64>,
    master_key: Vec<u8>,
    branch_seed: String,
    primary_key_index: i64,
    timestamp: NaiveDateTime,
}

impl From<KeyManagerState> for KeyManagerStateSql {
    fn from(km: KeyManagerState) -> Self {
        Self {
            id: None,
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

impl KeyManagerStateSql {
    fn commit(&self, conn: &SqliteConnection) -> Result<(), OutputManagerStorageError> {
        diesel::insert_into(key_manager_states::table)
            .values(self.clone())
            .execute(conn)?;
        Ok(())
    }

    pub fn get_state(conn: &SqliteConnection) -> Result<KeyManagerStateSql, OutputManagerStorageError> {
        Ok(key_manager_states::table
            .first::<KeyManagerStateSql>(conn)
            .map_err(|_| OutputManagerStorageError::KeyManagerNotInitialized)?)
    }

    pub fn set_state(&self, conn: &SqliteConnection) -> Result<(), OutputManagerStorageError> {
        match KeyManagerStateSql::get_state(conn) {
            Ok(km) => {
                let update = KeyManagerStateUpdateSql {
                    master_key: Some(self.master_key.clone()),
                    branch_seed: Some(self.branch_seed.clone()),
                    primary_key_index: Some(self.primary_key_index),
                };

                let num_updated = diesel::update(key_manager_states::table.filter(key_manager_states::id.eq(&km.id)))
                    .set(update)
                    .execute(conn)?;
                if num_updated == 0 {
                    return Err(OutputManagerStorageError::UnexpectedResult(
                        "Database update error".to_string(),
                    ));
                }
            },
            Err(_) => self.commit(conn)?,
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
                let num_updated = diesel::update(key_manager_states::table.filter(key_manager_states::id.eq(&km.id)))
                    .set(update)
                    .execute(conn)?;
                if num_updated == 0 {
                    return Err(OutputManagerStorageError::UnexpectedResult(
                        "Database update error".to_string(),
                    ));
                }
                current_index
            },
            Err(_) => return Err(OutputManagerStorageError::KeyManagerNotInitialized),
        })
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
        let encrypted_master_key = encrypt_bytes_integral_nonce(&cipher, self.master_key.clone())?;
        let encrypted_branch_seed =
            encrypt_bytes_integral_nonce(&cipher, self.branch_seed.clone().as_bytes().to_vec())?;
        self.master_key = encrypted_master_key;
        self.branch_seed = encrypted_branch_seed.to_hex();
        Ok(())
    }

    fn decrypt(&mut self, cipher: &Aes256Gcm) -> Result<(), Error> {
        let decrypted_master_key = decrypt_bytes_integral_nonce(&cipher, self.master_key.clone())?;
        let decrypted_branch_seed =
            decrypt_bytes_integral_nonce(&cipher, from_hex(self.branch_seed.as_str()).map_err(|_| Error)?)?;
        self.master_key = decrypted_master_key;
        self.branch_seed = from_utf8(decrypted_branch_seed.as_slice())
            .map_err(|_| Error)?
            .to_string();
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use crate::{
        output_manager_service::storage::{
            database::{DbKey, KeyManagerState, OutputManagerBackend},
            models::DbUnblindedOutput,
            sqlite_db::{
                KeyManagerStateSql,
                NewOutputSql,
                OutputManagerSqliteDatabase,
                OutputSql,
                OutputStatus,
                PendingTransactionOutputSql,
                UpdateOutput,
            },
        },
        storage::sqlite_utilities::WalletDbConnection,
        util::encryption::Encryptable,
    };
    use aes_gcm::{
        aead::{generic_array::GenericArray, NewAead},
        Aes256Gcm,
    };
    use chrono::{Duration as ChronoDuration, Utc};
    use diesel::{Connection, SqliteConnection};
    use rand::{distributions::Alphanumeric, rngs::OsRng, CryptoRng, Rng, RngCore};
    use std::{convert::TryFrom, iter, time::Duration};
    use tari_core::transactions::{
        tari_amount::MicroTari,
        transaction::{TransactionInput, UnblindedOutput},
        types::{CommitmentFactory, CryptoFactories, PrivateKey, PublicKey},
    };
    use tari_crypto::{
        inputs,
        keys::{PublicKey as PublicKeyTrait, SecretKey},
        script,
    };
    use tempfile::tempdir;

    pub fn random_string(len: usize) -> String {
        iter::repeat(()).map(|_| OsRng.sample(Alphanumeric)).take(len).collect()
    }

    pub fn make_input<R: Rng + CryptoRng>(rng: &mut R, val: MicroTari) -> (TransactionInput, UnblindedOutput) {
        let key = PrivateKey::random(rng);
        let script_key = PrivateKey::random(rng);
        let script_offset_private_key = PrivateKey::random(rng);
        let factory = CommitmentFactory::default();

        let script = script!(Nop);
        let input_data = inputs!(PublicKey::from_secret_key(&script_key));
        let height = 0;
        let unblinded_output = UnblindedOutput::new(
            val,
            key,
            None,
            script,
            input_data,
            height,
            script_key,
            PublicKey::from_secret_key(&script_offset_private_key),
        );

        let input = unblinded_output.as_transaction_input(&factory).unwrap();

        (input, unblinded_output)
    }

    #[test]
    fn test_crud() {
        let db_name = format!("{}.sqlite3", random_string(8).as_str());
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
            let (_, uo) = make_input(&mut OsRng.clone(), MicroTari::from(100 + OsRng.next_u64() % 1000));
            let uo = DbUnblindedOutput::from_unblinded_output(uo, &factories).unwrap();
            let o = NewOutputSql::new(uo, OutputStatus::Unspent, None);
            outputs.push(o.clone());
            outputs_unspent.push(o.clone());
            o.commit(&conn).unwrap();
        }

        for _i in 0..3 {
            let (_, uo) = make_input(&mut OsRng.clone(), MicroTari::from(100 + OsRng.next_u64() % 1000));
            let uo = DbUnblindedOutput::from_unblinded_output(uo, &factories).unwrap();
            let o = NewOutputSql::new(uo, OutputStatus::Spent, None);
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

        let tx_id = 44u64;

        PendingTransactionOutputSql::new(tx_id, true, Utc::now().naive_utc(), Some(1))
            .commit(&conn)
            .unwrap();

        PendingTransactionOutputSql::new(11u64, true, Utc::now().naive_utc(), Some(2))
            .commit(&conn)
            .unwrap();

        let pt = PendingTransactionOutputSql::find(tx_id, &conn).unwrap();

        assert_eq!(pt.tx_id as u64, tx_id);

        let pts = PendingTransactionOutputSql::index(&conn).unwrap();

        assert_eq!(pts.len(), 2);

        let _updated1 = OutputSql::find(&outputs[0].spending_key, &conn)
            .unwrap()
            .update(
                UpdateOutput {
                    status: Some(OutputStatus::Unspent),
                    tx_id: Some(44u64),
                    spending_key: None,
                    height: None,
                    script_private_key: None,
                },
                &conn,
            )
            .unwrap();

        let _updated2 = OutputSql::find(&outputs[1].spending_key, &conn)
            .unwrap()
            .update(
                UpdateOutput {
                    status: Some(OutputStatus::EncumberedToBeReceived),
                    tx_id: Some(44u64),
                    spending_key: None,
                    height: None,
                    script_private_key: None,
                },
                &conn,
            )
            .unwrap();

        let result = OutputSql::find_by_tx_id_and_encumbered(44u64, &conn).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].spending_key, outputs[1].spending_key);

        PendingTransactionOutputSql::new(
            12u64,
            true,
            Utc::now().naive_utc() - ChronoDuration::from_std(Duration::from_millis(600_000)).unwrap(),
            Some(3),
        )
        .commit(&conn)
        .unwrap();

        let pending_older1 = PendingTransactionOutputSql::index_older(Utc::now().naive_utc(), &conn).unwrap();
        assert_eq!(pending_older1.len(), 3);

        let pending_older2 = PendingTransactionOutputSql::index_older(
            Utc::now().naive_utc() - ChronoDuration::from_std(Duration::from_millis(200_000)).unwrap(),
            &conn,
        )
        .unwrap();
        assert_eq!(pending_older2.len(), 1);

        PendingTransactionOutputSql::new(13u64, true, Utc::now().naive_utc(), None)
            .commit(&conn)
            .unwrap();

        let pending_block_height = PendingTransactionOutputSql::index_block_height(2, &conn).unwrap();

        assert_eq!(pending_block_height.len(), 1);
        assert!(pending_block_height.iter().any(|p| p.tx_id == 11));
    }

    #[test]
    fn test_key_manager_crud() {
        let db_name = format!("{}.sqlite3", random_string(8).as_str());
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
            branch_seed: random_string(8),
            primary_key_index: 0,
        };

        KeyManagerStateSql::from(state1.clone()).set_state(&conn).unwrap();
        let state1_read = KeyManagerStateSql::get_state(&conn).unwrap();

        assert_eq!(state1, KeyManagerState::try_from(state1_read).unwrap());

        let state2 = KeyManagerState {
            master_key: PrivateKey::random(&mut OsRng),
            branch_seed: random_string(8),
            primary_key_index: 0,
        };

        KeyManagerStateSql::from(state2.clone()).set_state(&conn).unwrap();

        let state2_read = KeyManagerStateSql::get_state(&conn).unwrap();

        assert_eq!(state2, KeyManagerState::try_from(state2_read).unwrap());

        KeyManagerStateSql::increment_index(&conn).unwrap();
        KeyManagerStateSql::increment_index(&conn).unwrap();

        let state3_read = KeyManagerStateSql::get_state(&conn).unwrap();

        assert_eq!(state3_read.primary_key_index, 2);
    }

    #[test]
    fn test_output_encryption() {
        let db_name = format!("{}.sqlite3", random_string(8).as_str());
        let tempdir = tempdir().unwrap();
        let db_folder = tempdir.path().to_str().unwrap().to_string();
        let db_path = format!("{}{}", db_folder, db_name);

        embed_migrations!("./migrations");
        let conn = SqliteConnection::establish(&db_path).unwrap_or_else(|_| panic!("Error connecting to {}", db_path));

        embedded_migrations::run_with_output(&conn, &mut std::io::stdout()).expect("Migration failed");

        conn.execute("PRAGMA foreign_keys = ON").unwrap();
        let factories = CryptoFactories::default();

        let (_, uo) = make_input(&mut OsRng.clone(), MicroTari::from(100 + OsRng.next_u64() % 1000));
        let uo = DbUnblindedOutput::from_unblinded_output(uo, &factories).unwrap();
        let output = NewOutputSql::new(uo, OutputStatus::Unspent, None);

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
        let db_name = format!("{}.sqlite3", random_string(8).as_str());
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

        let state_sql = KeyManagerStateSql::from(starting_state.clone());
        state_sql.set_state(&conn).unwrap();

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
        let db_name = format!("{}.sqlite3", random_string(8).as_str());
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

        let state_sql = KeyManagerStateSql::from(starting_state);
        state_sql.set_state(&conn).unwrap();

        let (_, uo) = make_input(&mut OsRng.clone(), MicroTari::from(100 + OsRng.next_u64() % 1000));
        let uo = DbUnblindedOutput::from_unblinded_output(uo, &factories).unwrap();
        let output = NewOutputSql::new(uo, OutputStatus::Unspent, None);
        output.commit(&conn).unwrap();

        let (_, uo2) = make_input(&mut OsRng.clone(), MicroTari::from(100 + OsRng.next_u64() % 1000));
        let uo2 = DbUnblindedOutput::from_unblinded_output(uo2, &factories).unwrap();
        let output2 = NewOutputSql::new(uo2, OutputStatus::Unspent, None);
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
