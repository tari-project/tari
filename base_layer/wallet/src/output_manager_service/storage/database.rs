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
    storage::models::{DbUnblindedOutput, KnownOneSidedPaymentScript, OutputStatus},
};
use aes_gcm::Aes256Gcm;
use log::*;
use std::{
    fmt::{Display, Error, Formatter},
    sync::Arc,
};
use tari_common_types::{
    transaction::TxId,
    types::{BlindingFactor, Commitment, HashOutput, PrivateKey},
};
use tari_core::transactions::transaction::TransactionOutput;
use tracing::instrument;

const LOG_TARGET: &str = "wallet::output_manager_service::database";

/// This trait defines the required behaviour that a storage backend must provide for the Output Manager service.
/// Data is passed to and from the backend via the [DbKey], [DbValue], and [DbValueKey] enums. If new data types are
/// required to be supported by the backends then these enums can be updated to reflect this requirement and the trait
/// will remain the same
pub trait OutputManagerBackend: Send + Sync + Clone {
    /// Retrieve the record associated with the provided DbKey
    fn fetch(&self, key: &DbKey) -> Result<Option<DbValue>, OutputManagerStorageError>;
    /// Retrieve outputs that have been mined but not spent yet (have not been deleted)
    fn fetch_mined_unspent_outputs(&self) -> Result<Vec<DbUnblindedOutput>, OutputManagerStorageError>;
    /// Retrieve outputs that have not been found or confirmed in the block chain yet
    fn fetch_unconfirmed_outputs(&self) -> Result<Vec<DbUnblindedOutput>, OutputManagerStorageError>;
    /// Modify the state the of the backend with a write operation
    fn write(&self, op: WriteOperation) -> Result<Option<DbValue>, OutputManagerStorageError>;
    fn fetch_pending_incoming_outputs(&self) -> Result<Vec<DbUnblindedOutput>, OutputManagerStorageError>;

    fn set_received_output_mined_height(
        &self,
        hash: Vec<u8>,
        mined_height: u64,
        mined_in_block: Vec<u8>,
        mmr_position: u64,
        confirmed: bool,
    ) -> Result<(), OutputManagerStorageError>;

    fn set_output_to_unmined(&self, hash: Vec<u8>) -> Result<(), OutputManagerStorageError>;

    fn set_outputs_to_be_revalidated(&self) -> Result<(), OutputManagerStorageError>;

    fn mark_output_as_spent(
        &self,
        hash: Vec<u8>,
        mark_deleted_at_height: u64,
        mark_deleted_in_block: Vec<u8>,
        confirmed: bool,
    ) -> Result<(), OutputManagerStorageError>;

    fn mark_output_as_unspent(&self, hash: Vec<u8>) -> Result<(), OutputManagerStorageError>;
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
    /// This method will increment the currently stored key index for the key manager config. Increment this after each
    /// key is generated
    fn increment_key_index(&self) -> Result<(), OutputManagerStorageError>;
    /// This method will set the currently stored key index for the key manager
    fn set_key_index(&self, index: u64) -> Result<(), OutputManagerStorageError>;
    /// This method will update an output's metadata signature, akin to 'finalize output'
    fn update_output_metadata_signature(&self, output: &TransactionOutput) -> Result<(), OutputManagerStorageError>;
    /// If an invalid output is found to be valid this function will turn it back into an unspent output
    fn revalidate_unspent_output(&self, spending_key: &Commitment) -> Result<(), OutputManagerStorageError>;
    /// Apply encryption to the backend.
    fn apply_encryption(&self, cipher: Aes256Gcm) -> Result<(), OutputManagerStorageError>;
    /// Remove encryption from the backend.
    fn remove_encryption(&self) -> Result<(), OutputManagerStorageError>;

    /// Get the output that was most recently mined, ordered descending by mined height
    fn get_last_mined_output(&self) -> Result<Option<DbUnblindedOutput>, OutputManagerStorageError>;
    /// Get the output that was most recently spent, ordered descending by mined height
    fn get_last_spent_output(&self) -> Result<Option<DbUnblindedOutput>, OutputManagerStorageError>;
    /// Check if there is a pending coinbase transaction at this block height, if there is clear it.
    fn clear_pending_coinbase_transaction_at_block_height(
        &self,
        block_height: u64,
    ) -> Result<(), OutputManagerStorageError>;
    /// Set if a coinbase output is abandoned or not
    fn set_coinbase_abandoned(&self, tx_id: TxId, abandoned: bool) -> Result<(), OutputManagerStorageError>;
    /// Reinstate a cancelled inbound output
    fn reinstate_cancelled_inbound_output(&self, tx_id: TxId) -> Result<(), OutputManagerStorageError>;
    /// Return the available, time locked, pending incoming and pending outgoing balance
    fn get_balance(
        &self,
        current_tip_for_time_lock_calculation: Option<u64>,
    ) -> Result<Balance, OutputManagerStorageError>;
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
    AnyOutputByCommitment(Commitment),
    TimeLockedUnspentOutputs(u64),
    UnspentOutputs,
    SpentOutputs,
    KeyManagerState,
    InvalidOutputs,
    KnownOneSidedPaymentScripts,
    OutputsByTxIdAndStatus(TxId, OutputStatus),
}

#[derive(Debug)]
pub enum DbValue {
    SpentOutput(Box<DbUnblindedOutput>),
    UnspentOutput(Box<DbUnblindedOutput>),
    UnspentOutputs(Vec<DbUnblindedOutput>),
    SpentOutputs(Vec<DbUnblindedOutput>),
    InvalidOutputs(Vec<DbUnblindedOutput>),
    KeyManagerState(KeyManagerState),
    KnownOneSidedPaymentScripts(Vec<KnownOneSidedPaymentScript>),
    AnyOutput(Box<DbUnblindedOutput>),
    AnyOutputs(Vec<DbUnblindedOutput>),
}

pub enum DbKeyValuePair {
    UnspentOutput(Commitment, Box<DbUnblindedOutput>),
    UnspentOutputWithTxId(Commitment, (TxId, Box<DbUnblindedOutput>)),
    OutputToBeReceived(Commitment, (TxId, Box<DbUnblindedOutput>, Option<u64>)),
    KeyManagerState(KeyManagerState),
    KnownOneSidedPaymentScripts(KnownOneSidedPaymentScript),
}

pub enum WriteOperation {
    Insert(DbKeyValuePair),
    Remove(DbKey),
}

/// This structure holds an inner type that implements the `OutputManagerBackend` trait and contains the more complex
/// data access logic required by the module built onto the functionality defined by the trait
#[derive(Clone)]
pub struct OutputManagerDatabase<T> {
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

    #[instrument(name = "output_manager_database::set_key_manager_state", skip(self, state))]
    pub async fn set_key_manager_state(&self, state: KeyManagerState) -> Result<(), OutputManagerStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || {
            db_clone.write(WriteOperation::Insert(DbKeyValuePair::KeyManagerState(state)))
        })
        .await
        .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))??;

        Ok(())
    }

    #[instrument(name = "output_manager_database::increment_key_index", skip(self))]
    pub async fn increment_key_index(&self) -> Result<(), OutputManagerStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || db_clone.increment_key_index())
            .await
            .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(())
    }

    #[instrument(name = "output_manager_database::set_key_index", skip(self, index))]
    pub async fn set_key_index(&self, index: u64) -> Result<(), OutputManagerStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || db_clone.set_key_index(index))
            .await
            .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(())
    }

    #[instrument(name = "output_manager_database::add_unspent_output", skip(self, output))]
    pub async fn add_unspent_output(&self, output: DbUnblindedOutput) -> Result<(), OutputManagerStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || {
            db_clone.write(WriteOperation::Insert(DbKeyValuePair::UnspentOutput(
                output.commitment.clone(),
                Box::new(output),
            )))
        })
        .await
        .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))??;

        Ok(())
    }

    #[instrument(
        name = "output_manager_database::add_unspent_output_with_tx_id",
        skip(self, tx_id, output)
    )]
    pub async fn add_unspent_output_with_tx_id(
        &self,
        tx_id: TxId,
        output: DbUnblindedOutput,
    ) -> Result<(), OutputManagerStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || {
            db_clone.write(WriteOperation::Insert(DbKeyValuePair::UnspentOutputWithTxId(
                output.commitment.clone(),
                (tx_id, Box::new(output)),
            )))
        })
        .await
        .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))??;

        Ok(())
    }

    #[instrument(
        name = "output_manager_database::add_output_to_be_received",
        skip(self, tx_id, output, coinbase_block_height)
    )]
    pub async fn add_output_to_be_received(
        &self,
        tx_id: TxId,
        output: DbUnblindedOutput,
        coinbase_block_height: Option<u64>,
    ) -> Result<(), OutputManagerStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || {
            db_clone.write(WriteOperation::Insert(DbKeyValuePair::OutputToBeReceived(
                output.commitment.clone(),
                (tx_id, Box::new(output), coinbase_block_height),
            )))
        })
        .await
        .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))??;

        Ok(())
    }

    #[instrument(
        name = "output_manager_database::get_balance",
        skip(self, current_tip_for_time_lock_calculation)
    )]
    pub async fn get_balance(
        &self,
        current_tip_for_time_lock_calculation: Option<u64>,
    ) -> Result<Balance, OutputManagerStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || db_clone.get_balance(current_tip_for_time_lock_calculation))
            .await
            .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))?
    }

    /// This method is called when a transaction is built to be sent. It will encumber unspent outputs against a pending
    /// transaction in the short term.
    #[instrument(
        name = "output_manager_database::encumber_outputs",
        skip(self, tx_id, outputs_to_send, outputs_to_receive)
    )]
    pub async fn encumber_outputs(
        &self,
        tx_id: TxId,
        outputs_to_send: Vec<DbUnblindedOutput>,
        outputs_to_receive: Vec<DbUnblindedOutput>,
    ) -> Result<(), OutputManagerStorageError> {
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
    #[instrument(name = "output_manager_database::confirm_encumbered_outputs", skip(self, tx_id))]
    pub async fn confirm_encumbered_outputs(&self, tx_id: TxId) -> Result<(), OutputManagerStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || db_clone.confirm_encumbered_outputs(tx_id))
            .await
            .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))
            .and_then(|inner_result| inner_result)
    }

    /// Clear all pending transaction encumberances marked as short term. These are the result of an unfinished
    /// transaction negotiation
    #[instrument(name = "output_manager_database::clear_short_term_encumberances", skip(self))]
    pub async fn clear_short_term_encumberances(&self) -> Result<(), OutputManagerStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || db_clone.clear_short_term_encumberances())
            .await
            .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))
            .and_then(|inner_result| inner_result)
    }

    /// When a pending transaction is cancelled the encumbered outputs are moved back to the `unspent_outputs`
    /// collection.
    #[instrument(
        name = "output_manager_database::cancel_pending_transaction_outputs",
        skip(self, tx_id)
    )]
    pub async fn cancel_pending_transaction_outputs(&self, tx_id: TxId) -> Result<(), OutputManagerStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || db_clone.cancel_pending_transaction(tx_id))
            .await
            .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))
            .and_then(|inner_result| inner_result)
    }

    /// Check if there is a pending coinbase transaction at this block height, if there is clear it.
    #[instrument(
        name = "output_manager_database::clear_pending_coinbase_transaction_at_block_height",
        skip(self, block_height)
    )]
    pub async fn clear_pending_coinbase_transaction_at_block_height(
        &self,
        block_height: u64,
    ) -> Result<(), OutputManagerStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || db_clone.clear_pending_coinbase_transaction_at_block_height(block_height))
            .await
            .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))
            .and_then(|inner_result| inner_result)
    }

    /// Retrieves UTXOs sorted by value from smallest to largest.
    #[instrument(name = "output_manager_database::fetch_sorted_unspent_outputs", skip(self))]
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

    #[instrument(name = "output_manager_database::fetch_spent_outputs", skip(self))]
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

    #[instrument(name = "output_manager_database::fetch_unconfirmed_outputs", skip(self))]
    pub async fn fetch_unconfirmed_outputs(&self) -> Result<Vec<DbUnblindedOutput>, OutputManagerStorageError> {
        let db_clone = self.db.clone();
        let utxos = tokio::task::spawn_blocking(move || db_clone.fetch_unconfirmed_outputs())
            .await
            .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(utxos)
    }

    #[instrument(name = "output_manager_database::fetch_mined_unspent_outputs", skip(self))]
    pub async fn fetch_mined_unspent_outputs(&self) -> Result<Vec<DbUnblindedOutput>, OutputManagerStorageError> {
        let db_clone = self.db.clone();
        let utxos = tokio::task::spawn_blocking(move || db_clone.fetch_mined_unspent_outputs())
            .await
            .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(utxos)
    }

    #[instrument(name = "output_manager_database::get_timelocked_outputs", skip(self, tip))]
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

    #[instrument(name = "output_manager_database::get_invalid_outputs", skip(self))]
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

    #[instrument(
        name = "output_manager_database::update_output_metadata_signature",
        skip(self, output)
    )]
    pub async fn update_output_metadata_signature(
        &self,
        output: TransactionOutput,
    ) -> Result<(), OutputManagerStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || db_clone.update_output_metadata_signature(&output))
            .await
            .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))
            .and_then(|inner_result| inner_result)
    }

    #[instrument(name = "output_manager_database::revalidate_output", skip(self, commitment))]
    pub async fn revalidate_output(&self, commitment: Commitment) -> Result<(), OutputManagerStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || db_clone.revalidate_unspent_output(&commitment))
            .await
            .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))
            .and_then(|inner_result| inner_result)
    }

    #[instrument(
        name = "output_manager_database::reinstate_cancelled_inbound_output",
        skip(self, tx_id)
    )]
    pub async fn reinstate_cancelled_inbound_output(&self, tx_id: TxId) -> Result<(), OutputManagerStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || db_clone.reinstate_cancelled_inbound_output(tx_id))
            .await
            .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))
            .and_then(|inner_result| inner_result)
    }

    #[instrument(name = "output_manager_database::apply_encryption", skip(self, cipher))]
    pub async fn apply_encryption(&self, cipher: Aes256Gcm) -> Result<(), OutputManagerStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || db_clone.apply_encryption(cipher))
            .await
            .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))
            .and_then(|inner_result| inner_result)
    }

    #[instrument(name = "output_manager_database::remove_encryption", skip(self))]
    pub async fn remove_encryption(&self) -> Result<(), OutputManagerStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || db_clone.remove_encryption())
            .await
            .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))
            .and_then(|inner_result| inner_result)
    }

    #[instrument(
        name = "output_manager_database::get_all_known_one_sided_payment_scripts",
        skip(self)
    )]
    pub async fn get_all_known_one_sided_payment_scripts(
        &self,
    ) -> Result<Vec<KnownOneSidedPaymentScript>, OutputManagerStorageError> {
        let db_clone = self.db.clone();

        let scripts = tokio::task::spawn_blocking(move || match db_clone.fetch(&DbKey::KnownOneSidedPaymentScripts) {
            Ok(None) => log_error(
                DbKey::KnownOneSidedPaymentScripts,
                OutputManagerStorageError::UnexpectedResult("Could not retrieve known scripts".to_string()),
            ),
            Ok(Some(DbValue::KnownOneSidedPaymentScripts(scripts))) => Ok(scripts),
            Ok(Some(other)) => unexpected_result(DbKey::KnownOneSidedPaymentScripts, other),
            Err(e) => log_error(DbKey::KnownOneSidedPaymentScripts, e),
        })
        .await
        .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(scripts)
    }

    #[instrument(name = "output_manager_database::get_last_mined_output", skip(self))]
    pub async fn get_last_mined_output(&self) -> Result<Option<DbUnblindedOutput>, OutputManagerStorageError> {
        self.db.get_last_mined_output()
    }

    #[instrument(name = "output_manager_database::get_last_spent_output", skip(self))]
    pub async fn get_last_spent_output(&self) -> Result<Option<DbUnblindedOutput>, OutputManagerStorageError> {
        self.db.get_last_spent_output()
    }

    #[instrument(name = "output_manager_database::add_known_script", skip(self, known_script))]
    pub async fn add_known_script(
        &self,
        known_script: KnownOneSidedPaymentScript,
    ) -> Result<(), OutputManagerStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || {
            db_clone.write(WriteOperation::Insert(DbKeyValuePair::KnownOneSidedPaymentScripts(
                known_script,
            )))
        })
        .await
        .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))??;

        Ok(())
    }

    #[instrument(
        name = "output_manager_database::remove_output_by_commitment",
        skip(self, commitment)
    )]
    pub async fn remove_output_by_commitment(&self, commitment: Commitment) -> Result<(), OutputManagerStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || {
            match db_clone.write(WriteOperation::Remove(DbKey::AnyOutputByCommitment(commitment.clone()))) {
                Ok(None) => Ok(()),
                Ok(Some(DbValue::AnyOutput(_))) => Ok(()),
                Ok(Some(other)) => unexpected_result(DbKey::AnyOutputByCommitment(commitment.clone()), other),
                Err(e) => log_error(DbKey::AnyOutputByCommitment(commitment), e),
            }
        })
        .await
        .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(())
    }

    #[instrument(
        name = "output_manager_database::set_received_output_mined_height",
        skip(self, hash, mined_height, mined_in_block, mmr_position, confirmed)
    )]
    pub async fn set_received_output_mined_height(
        &self,
        hash: HashOutput,
        mined_height: u64,
        mined_in_block: HashOutput,
        mmr_position: u64,
        confirmed: bool,
    ) -> Result<(), OutputManagerStorageError> {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || {
            db.set_received_output_mined_height(hash, mined_height, mined_in_block, mmr_position, confirmed)
        })
        .await
        .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(())
    }

    #[instrument(name = "output_manager_database::set_output_to_unmined", skip(self, hash))]
    pub async fn set_output_to_unmined(&self, hash: HashOutput) -> Result<(), OutputManagerStorageError> {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || db.set_output_to_unmined(hash))
            .await
            .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(())
    }

    #[instrument(name = "output_manager_database::set_outputs_to_be_revalidated", skip(self))]
    pub async fn set_outputs_to_be_revalidated(&self) -> Result<(), OutputManagerStorageError> {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || db.set_outputs_to_be_revalidated())
            .await
            .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(())
    }

    #[instrument(
        name = "output_manager_database::mark_output_as_spent",
        skip(self, hash, deleted_height, deleted_in_block, confirmed)
    )]
    pub async fn mark_output_as_spent(
        &self,
        hash: HashOutput,
        deleted_height: u64,
        deleted_in_block: HashOutput,
        confirmed: bool,
    ) -> Result<(), OutputManagerStorageError> {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || db.mark_output_as_spent(hash, deleted_height, deleted_in_block, confirmed))
            .await
            .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(())
    }

    #[instrument(name = "output_manager_database::mark_output_as_unspent", skip(self, hash))]
    pub async fn mark_output_as_unspent(&self, hash: HashOutput) -> Result<(), OutputManagerStorageError> {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || db.mark_output_as_unspent(hash))
            .await
            .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(())
    }

    #[instrument(name = "output_manager_database::set_coinbase_abandoned", skip(self, tx_id))]
    pub async fn set_coinbase_abandoned(&self, tx_id: TxId, abandoned: bool) -> Result<(), OutputManagerStorageError> {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || db.set_coinbase_abandoned(tx_id, abandoned))
            .await
            .map_err(|err| OutputManagerStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(())
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
            DbKey::UnspentOutputs => f.write_str(&"Unspent Outputs Key".to_string()),
            DbKey::SpentOutputs => f.write_str(&"Spent Outputs Key".to_string()),
            DbKey::KeyManagerState => f.write_str(&"Key Manager State".to_string()),
            DbKey::InvalidOutputs => f.write_str("Invalid Outputs Key"),
            DbKey::TimeLockedUnspentOutputs(_t) => f.write_str("Timelocked Outputs"),
            DbKey::KnownOneSidedPaymentScripts => f.write_str("Known claiming scripts"),
            DbKey::AnyOutputByCommitment(_) => f.write_str("AnyOutputByCommitment"),
            DbKey::OutputsByTxIdAndStatus(_, _) => f.write_str("OutputsByTxIdAndStatus"),
        }
    }
}

impl Display for DbValue {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            DbValue::SpentOutput(_) => f.write_str("Spent Output"),
            DbValue::UnspentOutput(_) => f.write_str("Unspent Output"),
            DbValue::UnspentOutputs(_) => f.write_str("Unspent Outputs"),
            DbValue::SpentOutputs(_) => f.write_str("Spent Outputs"),
            DbValue::KeyManagerState(_) => f.write_str("Key Manager State"),
            DbValue::InvalidOutputs(_) => f.write_str("Invalid Outputs"),
            DbValue::KnownOneSidedPaymentScripts(_) => f.write_str("Known claiming scripts"),
            DbValue::AnyOutput(_) => f.write_str("Any Output"),
            DbValue::AnyOutputs(_) => f.write_str("Any Outputs"),
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
