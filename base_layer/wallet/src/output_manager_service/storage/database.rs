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

use crate::output_manager_service::{
    error::OutputManagerStorageError,
    service::Balance,
    storage::models::DbUnblindedOutput,
    TxId,
};
use aes_gcm::Aes256Gcm;
use chrono::{NaiveDateTime, Utc};
use log::*;
use std::{
    collections::HashMap,
    fmt::{Display, Error, Formatter},
    sync::Arc,
    time::Duration,
};
use tari_core::transactions::{
    tari_amount::MicroTari,
    types::{BlindingFactor, Commitment, CryptoFactories, PrivateKey},
    OutputBuilder,
    OutputFeatures,
};

const LOG_TARGET: &str = "wallet::output_manager_service::database";

/// This trait defines the required behaviour that a storage backend must provide for the Output Manager service.
/// Data is passed to and from the backend via the [DbKey], [DbValue], and [DbValueKey] enums. If new data types are
/// required to be supported by the backends then these enums can be updated to reflect this requirement and the trait
/// will remain the same
pub trait OutputManagerBackend: Send + Sync + Clone {
    /// Retrieve the record associated with the provided DbKey
    fn fetch(&self, key: &DbKey) -> Result<Option<DbValue>, OutputManagerStorageError>;
    /// Modify the state the of the backend with a write operation
    fn write(&self, op: WriteOperation) -> Result<Option<DbValue>, OutputManagerStorageError>;
    /// This method is called when a pending transaction is to be confirmed. It must move the `outputs_to_be_spent` and
    /// `outputs_to_be_received` from a `PendingTransactionOutputs` record into the `unspent_outputs` and
    /// `spent_outputs` collections.
    fn confirm_transaction(&self, tx_id: TxId) -> Result<(), OutputManagerStorageError>;
    /// This method encumbers the specified outputs into a `PendingTransactionOutputs` record. This is a short term
    /// encumberance in case the app is closed or crashes before transaction neogtiation is complete. These will be
    /// cleared on startup of the service.
    fn short_term_encumber_outputs(
        &self,
        tx_id: TxId,
        outputs_to_send: &[DbUnblindedOutput],
        outputs_to_receive: &[DbUnblindedOutput],
    ) -> Result<(), OutputManagerStorageError>;
    /// This method confirms that a transaction negotiation is complete and outputs can be fully encumbered. This
    /// reserves these outputs until the transaction is confirmed or cancelled
    fn confirm_encumbered_outputs(&self, tx_id: TxId) -> Result<(), OutputManagerStorageError>;
    /// Clear all pending transaction encumberances marked as short term. These are the result of an unfinished
    /// transaction negotiation
    fn clear_short_term_encumberances(&self) -> Result<(), OutputManagerStorageError>;
    /// This method must take all the `outputs_to_be_spent` from the specified transaction and move them back into the
    /// `UnspentOutputs` pool. The `outputs_to_be_received`'` will be marked as cancelled inbound outputs in case they
    /// need to be recovered.
    fn cancel_pending_transaction(&self, tx_id: TxId) -> Result<(), OutputManagerStorageError>;
    /// This method must run through all the `PendingTransactionOutputs` and test if any have existed for longer that
    /// the specified duration. If they have they should be cancelled.
    fn timeout_pending_transactions(&self, period: Duration) -> Result<(), OutputManagerStorageError>;
    /// This method will increment the currently stored key index for the key manager config. Increment this after each
    /// key is generated
    fn increment_key_index(&self) -> Result<(), OutputManagerStorageError>;
    /// If an unspent output is detected as invalid (i.e. not available on the blockchain) then it should be moved to
    /// the invalid outputs collection. The function will return the last recorded TxId associated with this output.
    fn invalidate_unspent_output(&self, output: &DbUnblindedOutput) -> Result<Option<TxId>, OutputManagerStorageError>;
    /// If an invalid output is found to be valid this function will turn it back into an unspent output
    fn revalidate_unspent_output(&self, spending_key: &Commitment) -> Result<(), OutputManagerStorageError>;
    /// Check to see if there exist any pending transaction with a blockheight equal that provided and cancel those
    /// pending transaction outputs.
    fn cancel_pending_transaction_at_block_height(&self, block_height: u64) -> Result<(), OutputManagerStorageError>;
    /// Apply encryption to the backend.
    fn apply_encryption(&self, cipher: Aes256Gcm) -> Result<(), OutputManagerStorageError>;
    /// Remove encryption from the backend.
    fn remove_encryption(&self) -> Result<(), OutputManagerStorageError>;
    /// Update a Spent output to be Unspent
    fn update_spent_output_to_unspent(
        &self,
        commitment: &Commitment,
    ) -> Result<DbUnblindedOutput, OutputManagerStorageError>;
}

/// Holds the outputs that have been selected for a given pending transaction waiting for confirmation
#[derive(Debug, Clone, PartialEq)]
pub struct PendingTransactionOutputs {
    pub tx_id: u64,
    pub outputs_to_be_spent: Vec<DbUnblindedOutput>,
    pub outputs_to_be_received: Vec<DbUnblindedOutput>,
    pub timestamp: NaiveDateTime,
    pub coinbase_block_height: Option<u64>,
}

/// Holds the state of the KeyManager being used by the Output Manager Service
#[derive(Clone, Debug, PartialEq)]
pub struct KeyManagerState {
    pub master_key: PrivateKey,
    pub branch_seed: String,
    pub primary_key_index: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DbKey {
    SpentOutput(BlindingFactor),
    UnspentOutput(BlindingFactor),
    PendingTransactionOutputs(TxId),
    TimeLockedUnspentOutputs(u64),
    UnspentOutputs,
    SpentOutputs,
    AllPendingTransactionOutputs,
    KeyManagerState,
    InvalidOutputs,
}

#[derive(Debug)]
pub enum DbValue {
    SpentOutput(Box<DbUnblindedOutput>),
    UnspentOutput(Box<DbUnblindedOutput>),
    PendingTransactionOutputs(Box<PendingTransactionOutputs>),
    UnspentOutputs(Vec<DbUnblindedOutput>),
    SpentOutputs(Vec<DbUnblindedOutput>),
    InvalidOutputs(Vec<DbUnblindedOutput>),
    AllPendingTransactionOutputs(HashMap<TxId, PendingTransactionOutputs>),
    KeyManagerState(KeyManagerState),
}

pub enum DbKeyValuePair {
    SpentOutput(BlindingFactor, Box<DbUnblindedOutput>),
    UnspentOutput(BlindingFactor, Box<DbUnblindedOutput>),
    PendingTransactionOutputs(TxId, Box<PendingTransactionOutputs>),
    KeyManagerState(KeyManagerState),
}

pub enum WriteOperation {
    Insert(DbKeyValuePair),
    Remove(DbKey),
}

// Private macro that pulls out all the boiler plate of extracting a DB query result from its variants
macro_rules! fetch {
    ($db:ident, $key_val:expr, $key_var:ident) => {{
        let key = DbKey::$key_var($key_val);
        match $db.fetch(&key) {
            Ok(None) => Err(OutputManagerStorageError::ValueNotFound(key)),
            Ok(Some(DbValue::$key_var(k))) => Ok(*k),
            Ok(Some(other)) => unexpected_result(key, other),
            Err(e) => log_error(key, e),
        }
    }};
}

/// This structure holds an inner type that implements the `OutputManagerBackend` trait and contains the more complex
/// data access logic required by the module built onto the functionality defined by the trait
#[derive(Clone)]
pub struct OutputManagerDatabase<T>
where T: OutputManagerBackend + 'static
{
    db: Arc<T>,
}

impl<T> OutputManagerDatabase<T>
where T: OutputManagerBackend + 'static
{
    pub fn new(db: T) -> Self {
        Self { db: Arc::new(db) }
    }

    pub async fn get_key_manager_state(&self) -> Result<Option<KeyManagerState>, OutputManagerStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || match db_clone.fetch(&DbKey::KeyManagerState) {
            Ok(None) => Ok(None),
            Ok(Some(DbValue::KeyManagerState(c))) => Ok(Some(c)),
            Ok(Some(other)) => unexpected_result(DbKey::KeyManagerState, other),
            Err(e) => log_error(DbKey::KeyManagerState, e),
        })
        .await
        .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))
        .and_then(|inner_result| inner_result)
    }

    pub async fn set_key_manager_state(&self, state: KeyManagerState) -> Result<(), OutputManagerStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || {
            db_clone.write(WriteOperation::Insert(DbKeyValuePair::KeyManagerState(state)))
        })
        .await
        .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))??;

        Ok(())
    }

    pub async fn increment_key_index(&self) -> Result<(), OutputManagerStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || db_clone.increment_key_index())
            .await
            .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(())
    }

    pub async fn add_unspent_output(&self, output: DbUnblindedOutput) -> Result<(), OutputManagerStorageError> {
        let db_clone = self.db.clone();
        let key = output.unblinded_output.spending_key().clone();
        tokio::task::spawn_blocking(move || {
            db_clone.write(WriteOperation::Insert(DbKeyValuePair::UnspentOutput(
                key,
                Box::new(output),
            )))
        })
        .await
        .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(())
    }

    pub async fn get_balance(&self, current_chain_tip: Option<u64>) -> Result<Balance, OutputManagerStorageError> {
        let db_clone = self.db.clone();
        let db_clone2 = self.db.clone();
        let db_clone3 = self.db.clone();

        let pending_txs = tokio::task::spawn_blocking(move || {
            db_clone.fetch(&DbKey::AllPendingTransactionOutputs)?.ok_or_else(|| {
                OutputManagerStorageError::UnexpectedResult(
                    "Pending Transaction Outputs cannot be retrieved".to_string(),
                )
            })
        })
        .await
        .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))??;

        let unspent_outputs = tokio::task::spawn_blocking(move || {
            db_clone2.fetch(&DbKey::UnspentOutputs)?.ok_or_else(|| {
                OutputManagerStorageError::UnexpectedResult("Unspent Outputs cannot be retrieved".to_string())
            })
        })
        .await
        .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))??;

        if let DbValue::UnspentOutputs(uo) = unspent_outputs {
            if let DbValue::AllPendingTransactionOutputs(pto) = pending_txs {
                let available_balance = uo
                    .iter()
                    .fold(MicroTari::from(0), |acc, x| acc + x.unblinded_output.value());
                let time_locked_balance = if let Some(tip) = current_chain_tip {
                    let time_locked_outputs = tokio::task::spawn_blocking(move || {
                        db_clone3.fetch(&DbKey::TimeLockedUnspentOutputs(tip))?.ok_or_else(|| {
                            OutputManagerStorageError::UnexpectedResult(
                                "Time-locked Outputs cannot be retrieved".to_string(),
                            )
                        })
                    })
                    .await
                    .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))??;
                    if let DbValue::UnspentOutputs(time_locked_uo) = time_locked_outputs {
                        Some(
                            time_locked_uo
                                .iter()
                                .fold(MicroTari::from(0), |acc, x| acc + x.unblinded_output.value()),
                        )
                    } else {
                        None
                    }
                } else {
                    None
                };
                let mut pending_incoming = MicroTari::from(0);
                let mut pending_outgoing = MicroTari::from(0);

                for v in pto.values() {
                    pending_incoming += v
                        .outputs_to_be_received
                        .iter()
                        .fold(MicroTari::from(0), |acc, x| acc + x.unblinded_output.value());
                    pending_outgoing += v
                        .outputs_to_be_spent
                        .iter()
                        .fold(MicroTari::from(0), |acc, x| acc + x.unblinded_output.value());
                }

                return Ok(Balance {
                    available_balance,
                    time_locked_balance,
                    pending_incoming_balance: pending_incoming,
                    pending_outgoing_balance: pending_outgoing,
                });
            }
        }

        Err(OutputManagerStorageError::UnexpectedResult(
            "Unexpected result from database backend".to_string(),
        ))
    }

    pub async fn add_pending_transaction_outputs(
        &self,
        pending_transaction_outputs: PendingTransactionOutputs,
    ) -> Result<(), OutputManagerStorageError>
    {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || {
            db_clone.write(WriteOperation::Insert(DbKeyValuePair::PendingTransactionOutputs(
                pending_transaction_outputs.tx_id,
                Box::new(pending_transaction_outputs),
            )))
        })
        .await
        .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))??;

        Ok(())
    }

    pub async fn fetch_pending_transaction_outputs(
        &self,
        tx_id: TxId,
    ) -> Result<PendingTransactionOutputs, OutputManagerStorageError>
    {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || fetch!(db_clone, tx_id, PendingTransactionOutputs))
            .await
            .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))
            .and_then(|inner_result| inner_result)
    }

    /// This method is called when a pending transaction is confirmed. It moves the `outputs_to_be_spent` and
    /// `outputs_to_be_received` from a `PendingTransactionOutputs` record into the `unspent_outputs` and
    /// `spent_outputs` collections.
    pub async fn confirm_pending_transaction_outputs(&self, tx_id: TxId) -> Result<(), OutputManagerStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || db_clone.confirm_transaction(tx_id))
            .await
            .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))
            .and_then(|inner_result| inner_result)
    }

    /// This method accepts and stores a pending inbound transaction and creates the `output_to_be_received` from the
    /// amount and provided spending key.
    pub async fn accept_incoming_pending_transaction(
        &self,
        tx_id: TxId,
        amount: MicroTari,
        spending_key: PrivateKey,
        output_features: OutputFeatures,
        factory: &CryptoFactories,
        coinbase_block_height: Option<u64>,
    ) -> Result<(), OutputManagerStorageError>
    {
        let db_clone = self.db.clone();
        let uo = OutputBuilder::new()
            .with_value(amount)
            .with_spending_key(spending_key.clone())
            .with_features(output_features)
            .build(&factory.commitment)?;
        let output = DbUnblindedOutput::from_unblinded_output(uo, factory)?;
        tokio::task::spawn_blocking(move || {
            db_clone.write(WriteOperation::Insert(DbKeyValuePair::PendingTransactionOutputs(
                tx_id,
                Box::new(PendingTransactionOutputs {
                    tx_id,
                    outputs_to_be_spent: Vec::new(),
                    outputs_to_be_received: vec![output],
                    timestamp: Utc::now().naive_utc(),
                    coinbase_block_height,
                }),
            )))
        })
        .await
        .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(())
    }

    /// This method is called when a transaction is built to be sent. It will encumber unspent outputs against a pending
    /// transaction in the short term.
    pub async fn encumber_outputs(
        &self,
        tx_id: TxId,
        outputs_to_send: Vec<DbUnblindedOutput>,
        outputs_to_receive: Vec<DbUnblindedOutput>,
    ) -> Result<(), OutputManagerStorageError>
    {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || {
            db_clone.short_term_encumber_outputs(tx_id, &outputs_to_send, &outputs_to_receive)
        })
        .await
        .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))
        .and_then(|inner_result| inner_result)
    }

    /// This method is called when a transaction is finished being negotiated. This will fully encumber the outputs
    /// against a pending transaction.
    pub async fn confirm_encumbered_outputs(&self, tx_id: TxId) -> Result<(), OutputManagerStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || db_clone.confirm_encumbered_outputs(tx_id))
            .await
            .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))
            .and_then(|inner_result| inner_result)
    }

    /// Clear all pending transaction encumberances marked as short term. These are the result of an unfinished
    /// transaction negotiation
    pub async fn clear_short_term_encumberances(&self) -> Result<(), OutputManagerStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || db_clone.clear_short_term_encumberances())
            .await
            .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))
            .and_then(|inner_result| inner_result)
    }

    /// When a pending transaction is cancelled the encumbered outputs are moved back to the `unspent_outputs`
    /// collection.
    pub async fn cancel_pending_transaction_outputs(&self, tx_id: TxId) -> Result<(), OutputManagerStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || db_clone.cancel_pending_transaction(tx_id))
            .await
            .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))
            .and_then(|inner_result| inner_result)
    }

    /// This method is check all pending transactions to see if any are older that the provided duration. If they are
    /// they will be cancelled.
    pub async fn timeout_pending_transaction_outputs(&self, period: Duration) -> Result<(), OutputManagerStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || db_clone.timeout_pending_transactions(period))
            .await
            .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))
            .and_then(|inner_result| inner_result)
    }

    pub async fn fetch_sorted_unspent_outputs(&self) -> Result<Vec<DbUnblindedOutput>, OutputManagerStorageError> {
        let db_clone = self.db.clone();

        let mut uo = tokio::task::spawn_blocking(move || match db_clone.fetch(&DbKey::UnspentOutputs) {
            Ok(None) => log_error(
                DbKey::UnspentOutputs,
                OutputManagerStorageError::UnexpectedResult("Could not retrieve unspent outputs".to_string()),
            ),
            Ok(Some(DbValue::UnspentOutputs(uo))) => Ok(uo),
            Ok(Some(other)) => unexpected_result(DbKey::UnspentOutputs, other),
            Err(e) => log_error(DbKey::UnspentOutputs, e),
        })
        .await
        .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))??;

        uo.sort();
        Ok(uo)
    }

    pub async fn fetch_spent_outputs(&self) -> Result<Vec<DbUnblindedOutput>, OutputManagerStorageError> {
        let db_clone = self.db.clone();

        let uo = tokio::task::spawn_blocking(move || match db_clone.fetch(&DbKey::SpentOutputs) {
            Ok(None) => log_error(
                DbKey::SpentOutputs,
                OutputManagerStorageError::UnexpectedResult("Could not retrieve spent outputs".to_string()),
            ),
            Ok(Some(DbValue::SpentOutputs(uo))) => Ok(uo),
            Ok(Some(other)) => unexpected_result(DbKey::SpentOutputs, other),
            Err(e) => log_error(DbKey::SpentOutputs, e),
        })
        .await
        .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(uo)
    }

    pub async fn fetch_all_pending_transaction_outputs(
        &self,
    ) -> Result<HashMap<u64, PendingTransactionOutputs>, OutputManagerStorageError> {
        let db_clone = self.db.clone();

        let uo = tokio::task::spawn_blocking(move || match db_clone.fetch(&DbKey::AllPendingTransactionOutputs) {
            Ok(None) => log_error(
                DbKey::AllPendingTransactionOutputs,
                OutputManagerStorageError::UnexpectedResult(
                    "Could not retrieve pending transaction outputs".to_string(),
                ),
            ),
            Ok(Some(DbValue::AllPendingTransactionOutputs(pt))) => Ok(pt),
            Ok(Some(other)) => unexpected_result(DbKey::AllPendingTransactionOutputs, other),
            Err(e) => log_error(DbKey::AllPendingTransactionOutputs, e),
        })
        .await
        .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(uo)
    }

    pub async fn get_unspent_outputs(&self) -> Result<Vec<DbUnblindedOutput>, OutputManagerStorageError> {
        let db_clone = self.db.clone();

        let uo = tokio::task::spawn_blocking(move || match db_clone.fetch(&DbKey::UnspentOutputs) {
            Ok(None) => log_error(
                DbKey::UnspentOutputs,
                OutputManagerStorageError::UnexpectedResult("Could not retrieve unspent outputs".to_string()),
            ),
            Ok(Some(DbValue::UnspentOutputs(uo))) => Ok(uo),
            Ok(Some(other)) => unexpected_result(DbKey::UnspentOutputs, other),
            Err(e) => log_error(DbKey::UnspentOutputs, e),
        })
        .await
        .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(uo)
    }

    pub async fn get_spent_outputs(&self) -> Result<Vec<DbUnblindedOutput>, OutputManagerStorageError> {
        let db_clone = self.db.clone();

        let uo = tokio::task::spawn_blocking(move || match db_clone.fetch(&DbKey::SpentOutputs) {
            Ok(None) => log_error(
                DbKey::SpentOutputs,
                OutputManagerStorageError::UnexpectedResult("Could not retrieve spent outputs".to_string()),
            ),
            Ok(Some(DbValue::SpentOutputs(uo))) => Ok(uo),
            Ok(Some(other)) => unexpected_result(DbKey::SpentOutputs, other),
            Err(e) => log_error(DbKey::SpentOutputs, e),
        })
        .await
        .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(uo)
    }

    pub async fn get_timelocked_outputs(&self, tip: u64) -> Result<Vec<DbUnblindedOutput>, OutputManagerStorageError> {
        let db_clone = self.db.clone();

        let uo = tokio::task::spawn_blocking(move || match db_clone.fetch(&DbKey::TimeLockedUnspentOutputs(tip)) {
            Ok(None) => log_error(
                DbKey::UnspentOutputs,
                OutputManagerStorageError::UnexpectedResult("Could not retrieve unspent outputs".to_string()),
            ),
            Ok(Some(DbValue::UnspentOutputs(uo))) => Ok(uo),
            Ok(Some(other)) => unexpected_result(DbKey::UnspentOutputs, other),
            Err(e) => log_error(DbKey::UnspentOutputs, e),
        })
        .await
        .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(uo)
    }

    pub async fn get_invalid_outputs(&self) -> Result<Vec<DbUnblindedOutput>, OutputManagerStorageError> {
        let db_clone = self.db.clone();

        let uo = tokio::task::spawn_blocking(move || match db_clone.fetch(&DbKey::InvalidOutputs) {
            Ok(None) => log_error(
                DbKey::InvalidOutputs,
                OutputManagerStorageError::UnexpectedResult("Could not retrieve invalid outputs".to_string()),
            ),
            Ok(Some(DbValue::InvalidOutputs(uo))) => Ok(uo),
            Ok(Some(other)) => unexpected_result(DbKey::InvalidOutputs, other),
            Err(e) => log_error(DbKey::InvalidOutputs, e),
        })
        .await
        .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(uo)
    }

    pub async fn invalidate_output(
        &self,
        output: DbUnblindedOutput,
    ) -> Result<Option<TxId>, OutputManagerStorageError>
    {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || db_clone.invalidate_unspent_output(&output))
            .await
            .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))
            .and_then(|inner_result| inner_result)
    }

    pub async fn revalidate_output(&self, commitment: Commitment) -> Result<(), OutputManagerStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || db_clone.revalidate_unspent_output(&commitment))
            .await
            .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))
            .and_then(|inner_result| inner_result)
    }

    pub async fn update_spent_output_to_unspent(
        &self,
        commitment: Commitment,
    ) -> Result<DbUnblindedOutput, OutputManagerStorageError>
    {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || db_clone.update_spent_output_to_unspent(&commitment))
            .await
            .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))
            .and_then(|inner_result| inner_result)
    }

    pub async fn cancel_pending_transaction_at_block_height(
        &self,
        block_height: u64,
    ) -> Result<(), OutputManagerStorageError>
    {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || db_clone.cancel_pending_transaction_at_block_height(block_height))
            .await
            .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))
            .and_then(|inner_result| inner_result)
    }

    pub async fn apply_encryption(&self, cipher: Aes256Gcm) -> Result<(), OutputManagerStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || db_clone.apply_encryption(cipher))
            .await
            .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))
            .and_then(|inner_result| inner_result)
    }

    pub async fn remove_encryption(&self) -> Result<(), OutputManagerStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || db_clone.remove_encryption())
            .await
            .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))
            .and_then(|inner_result| inner_result)
    }
}

fn unexpected_result<T>(req: DbKey, res: DbValue) -> Result<T, OutputManagerStorageError> {
    let msg = format!("Unexpected result for database query {}. Response: {}", req, res);
    error!(target: LOG_TARGET, "{}", msg);
    Err(OutputManagerStorageError::UnexpectedResult(msg))
}

impl Display for DbKey {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            DbKey::SpentOutput(_) => f.write_str(&"Spent Output Key".to_string()),
            DbKey::UnspentOutput(_) => f.write_str(&"Unspent Output Key".to_string()),
            DbKey::PendingTransactionOutputs(tx_id) => {
                f.write_str(&format!("Pending Transaction Outputs TX_ID: {}", tx_id))
            },
            DbKey::UnspentOutputs => f.write_str(&"Unspent Outputs Key".to_string()),
            DbKey::SpentOutputs => f.write_str(&"Spent Outputs Key".to_string()),
            DbKey::AllPendingTransactionOutputs => f.write_str(&"All Pending Transaction Outputs".to_string()),
            DbKey::KeyManagerState => f.write_str(&"Key Manager State".to_string()),
            DbKey::InvalidOutputs => f.write_str(&"Invalid Outputs Key"),
            DbKey::TimeLockedUnspentOutputs(_t) => f.write_str(&"Timelocked Outputs"),
        }
    }
}

impl Display for DbValue {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            DbValue::SpentOutput(_) => f.write_str("Spent Output"),
            DbValue::UnspentOutput(_) => f.write_str("Unspent Output"),
            DbValue::PendingTransactionOutputs(_) => f.write_str("Pending Transaction Outputs"),
            DbValue::UnspentOutputs(_) => f.write_str("Unspent Outputs"),
            DbValue::SpentOutputs(_) => f.write_str("Spent Outputs"),
            DbValue::AllPendingTransactionOutputs(_) => f.write_str("All Pending Transaction Outputs"),
            DbValue::KeyManagerState(_) => f.write_str("Key Manager State"),
            DbValue::InvalidOutputs(_) => f.write_str("Invalid Outputs"),
        }
    }
}

fn log_error<T>(req: DbKey, err: OutputManagerStorageError) -> Result<T, OutputManagerStorageError> {
    error!(
        target: LOG_TARGET,
        "Database access error on request: {}: {}",
        req,
        err.to_string()
    );
    Err(err)
}
