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
    output_manager_service::TxId,
    transaction_service::{
        error::TransactionStorageError,
        storage::models::{
            CompletedTransaction,
            InboundTransaction,
            OutboundTransaction,
            TransactionDirection,
            TransactionStatus,
        },
    },
};
use aes_gcm::Aes256Gcm;
#[cfg(feature = "test_harness")]
use chrono::NaiveDateTime;
use chrono::Utc;
use log::*;

use crate::transaction_service::storage::models::WalletTransaction;
use std::{
    collections::HashMap,
    fmt::{Display, Error, Formatter},
    sync::Arc,
};
use tari_comms::types::CommsPublicKey;
use tari_core::transactions::{tari_amount::MicroTari, transaction::Transaction, types::BlindingFactor};

const LOG_TARGET: &str = "wallet::transaction_service::database";

/// This trait defines the required behaviour that a storage backend must provide for the Transactionservice.
/// Data is passed to and from the backend via the [DbKey], [DbValue], and [DbValueKey] enums. If new data types are
/// required to be supported by the backends then these enums can be updated to reflect this requirement and the trait
/// will remain the same
pub trait TransactionBackend: Send + Sync + Clone {
    /// Retrieve the record associated with the provided DbKey
    fn fetch(&self, key: &DbKey) -> Result<Option<DbValue>, TransactionStorageError>;
    /// Check if a record with the provided key exists in the backend.
    fn contains(&self, key: &DbKey) -> Result<bool, TransactionStorageError>;
    /// Modify the state the of the backend with a write operation
    fn write(&self, op: WriteOperation) -> Result<Option<DbValue>, TransactionStorageError>;
    /// Check if a transaction exists in any of the collections
    fn transaction_exists(&self, tx_id: TxId) -> Result<bool, TransactionStorageError>;
    /// Complete outbound transaction, this operation must delete the `OutboundTransaction` with the provided
    /// `TxId` and insert the provided `CompletedTransaction` into `CompletedTransactions`.
    fn complete_outbound_transaction(
        &self,
        tx_id: TxId,
        completed_transaction: CompletedTransaction,
    ) -> Result<(), TransactionStorageError>;
    /// Complete inbound transaction, this operation must delete the `InboundTransaction` with the provided
    /// `TxId` and insert the provided `CompletedTransaction` into `CompletedTransactions`.
    fn complete_inbound_transaction(
        &self,
        tx_id: TxId,
        completed_transaction: CompletedTransaction,
    ) -> Result<(), TransactionStorageError>;
    /// Indicated that a completed transaction has been broadcast to the mempools
    fn broadcast_completed_transaction(&self, tx_id: TxId) -> Result<(), TransactionStorageError>;
    /// Indicated that a completed transaction has been detected as mined on a base node
    fn mine_completed_transaction(&self, tx_id: TxId) -> Result<(), TransactionStorageError>;
    /// Indicated that a broadcast transaction has been detected as confirm on a base node
    fn confirm_broadcast_or_coinbase_transaction(&self, tx_id: TxId) -> Result<(), TransactionStorageError>;
    /// Indicated that a mined transaction has been detected as unconfirmed on a base node, due to reorg or base node
    /// switch
    fn unconfirm_mined_transaction(&self, tx_id: TxId) -> Result<(), TransactionStorageError>;
    /// Set transaction's validity
    fn set_completed_transaction_validity(&self, tx_id: TxId, valid: bool) -> Result<(), TransactionStorageError>;
    /// Cancel Completed transaction, this will update the transaction status
    fn cancel_completed_transaction(&self, tx_id: TxId) -> Result<(), TransactionStorageError>;
    /// Cancel Completed transaction, this will update the transaction status
    fn cancel_pending_transaction(&self, tx_id: TxId) -> Result<(), TransactionStorageError>;
    /// Search all pending transaction for the provided tx_id and if it exists return the public key of the counterparty
    fn get_pending_transaction_counterparty_pub_key_by_tx_id(
        &self,
        tx_id: TxId,
    ) -> Result<CommsPublicKey, TransactionStorageError>;
    /// Mark a pending transaction direct send attempt as a success
    fn mark_direct_send_success(&self, tx_id: TxId) -> Result<(), TransactionStorageError>;
    /// Cancel coinbase transactions at a specific block height
    fn cancel_coinbase_transaction_at_block_height(&self, block_height: u64) -> Result<(), TransactionStorageError>;
    /// Find coinbase transaction at a specific block height for a given amount
    fn find_coinbase_transaction_at_block_height(
        &self,
        block_height: u64,
        amount: MicroTari,
    ) -> Result<Option<CompletedTransaction>, TransactionStorageError>;
    /// Update a completed transactions timestamp for use in test data generation
    #[cfg(feature = "test_harness")]
    fn update_completed_transaction_timestamp(
        &self,
        tx_id: TxId,
        timestamp: NaiveDateTime,
    ) -> Result<(), TransactionStorageError>;
    /// Apply encryption to the backend.
    fn apply_encryption(&self, cipher: Aes256Gcm) -> Result<(), TransactionStorageError>;
    /// Remove encryption from the backend.
    fn remove_encryption(&self) -> Result<(), TransactionStorageError>;
    /// Increment the send counter and timestamp of a transaction
    fn increment_send_count(&self, tx_id: TxId) -> Result<(), TransactionStorageError>;
    /// Update a transactions number of confirmations
    fn update_confirmations(&self, tx_id: TxId, confirmations: u64) -> Result<(), TransactionStorageError>;
    /// Update a transactions mined height
    fn update_mined_height(&self, tx_id: TxId, mined_height: u64) -> Result<(), TransactionStorageError>;
}

#[derive(Debug, Clone, PartialEq)]
pub enum DbKey {
    PendingOutboundTransaction(TxId),
    PendingInboundTransaction(TxId),
    CompletedTransaction(TxId),
    PendingOutboundTransactions,
    PendingInboundTransactions,
    CompletedTransactions,
    CancelledPendingOutboundTransactions,
    CancelledPendingInboundTransactions,
    CancelledCompletedTransactions,
    CancelledPendingOutboundTransaction(TxId),
    CancelledPendingInboundTransaction(TxId),
    AnyTransaction(TxId),
}

#[derive(Debug)]
pub enum DbValue {
    PendingOutboundTransaction(Box<OutboundTransaction>),
    PendingInboundTransaction(Box<InboundTransaction>),
    CompletedTransaction(Box<CompletedTransaction>),
    PendingOutboundTransactions(HashMap<TxId, OutboundTransaction>),
    PendingInboundTransactions(HashMap<TxId, InboundTransaction>),
    CompletedTransactions(HashMap<TxId, CompletedTransaction>),
    WalletTransaction(Box<WalletTransaction>),
}

pub enum DbKeyValuePair {
    PendingOutboundTransaction(TxId, Box<OutboundTransaction>),
    PendingInboundTransaction(TxId, Box<InboundTransaction>),
    CompletedTransaction(TxId, Box<CompletedTransaction>),
}

pub enum WriteOperation {
    Insert(DbKeyValuePair),
    Remove(DbKey),
}

/// This structure holds an inner type that implements the `TransactionBackend` trait and contains the more complex
/// data access logic required by the module built onto the functionality defined by the trait
#[derive(Clone)]
pub struct TransactionDatabase<T>
where T: TransactionBackend + 'static
{
    db: Arc<T>,
}

impl<T> TransactionDatabase<T>
where T: TransactionBackend + 'static
{
    pub fn new(db: T) -> Self {
        Self { db: Arc::new(db) }
    }

    pub async fn add_pending_inbound_transaction(
        &self,
        tx_id: TxId,
        inbound_tx: InboundTransaction,
    ) -> Result<(), TransactionStorageError>
    {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || {
            db_clone.write(WriteOperation::Insert(DbKeyValuePair::PendingInboundTransaction(
                tx_id,
                Box::new(inbound_tx),
            )))
        })
        .await
        .map_err(|err| TransactionStorageError::BlockingTaskSpawnError(err.to_string()))??;

        Ok(())
    }

    pub async fn add_pending_outbound_transaction(
        &self,
        tx_id: TxId,
        outbound_tx: OutboundTransaction,
    ) -> Result<(), TransactionStorageError>
    {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || {
            db_clone.write(WriteOperation::Insert(DbKeyValuePair::PendingOutboundTransaction(
                tx_id,
                Box::new(outbound_tx),
            )))
        })
        .await
        .map_err(|err| TransactionStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(())
    }

    pub async fn remove_pending_outbound_transaction(&self, tx_id: TxId) -> Result<(), TransactionStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || {
            db_clone.write(WriteOperation::Remove(DbKey::PendingOutboundTransaction(tx_id)))
        })
        .await
        .map_err(|err| TransactionStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(())
    }

    /// Check if a transaction with the specified TxId exists in any of the collections
    pub async fn transaction_exists(&self, tx_id: TxId) -> Result<bool, TransactionStorageError> {
        let db_clone = self.db.clone();
        let tx_id_clone = tx_id;
        tokio::task::spawn_blocking(move || db_clone.transaction_exists(tx_id_clone))
            .await
            .map_err(|err| TransactionStorageError::BlockingTaskSpawnError(err.to_string()))
            .and_then(|inner_result| inner_result)
    }

    pub async fn insert_completed_transaction(
        &self,
        tx_id: TxId,
        transaction: CompletedTransaction,
    ) -> Result<Option<DbValue>, TransactionStorageError>
    {
        let db_clone = self.db.clone();

        tokio::task::spawn_blocking(move || {
            db_clone.write(WriteOperation::Insert(DbKeyValuePair::CompletedTransaction(
                tx_id,
                Box::new(transaction),
            )))
        })
        .await
        .map_err(|err| TransactionStorageError::BlockingTaskSpawnError(err.to_string()))
        .and_then(|inner_result| inner_result)
    }

    pub async fn get_pending_outbound_transaction(
        &self,
        tx_id: TxId,
    ) -> Result<OutboundTransaction, TransactionStorageError>
    {
        self.get_pending_outbound_transaction_by_cancelled(tx_id, false).await
    }

    pub async fn get_cancelled_pending_outbound_transaction(
        &self,
        tx_id: TxId,
    ) -> Result<OutboundTransaction, TransactionStorageError>
    {
        self.get_pending_outbound_transaction_by_cancelled(tx_id, true).await
    }

    pub async fn get_pending_outbound_transaction_by_cancelled(
        &self,
        tx_id: TxId,
        cancelled: bool,
    ) -> Result<OutboundTransaction, TransactionStorageError>
    {
        let db_clone = self.db.clone();
        let key = if cancelled {
            DbKey::CancelledPendingOutboundTransaction(tx_id)
        } else {
            DbKey::PendingOutboundTransaction(tx_id)
        };
        let t = tokio::task::spawn_blocking(move || match db_clone.fetch(&key) {
            Ok(None) => Err(TransactionStorageError::ValueNotFound(key)),
            Ok(Some(DbValue::PendingOutboundTransaction(pt))) => Ok(pt),
            Ok(Some(other)) => unexpected_result(key, other),
            Err(e) => log_error(key, e),
        })
        .await
        .map_err(|err| TransactionStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(*t)
    }

    pub async fn get_pending_inbound_transaction(
        &self,
        tx_id: TxId,
    ) -> Result<InboundTransaction, TransactionStorageError>
    {
        self.get_pending_inbound_transaction_by_cancelled(tx_id, false).await
    }

    pub async fn get_cancelled_pending_inbound_transaction(
        &self,
        tx_id: TxId,
    ) -> Result<InboundTransaction, TransactionStorageError>
    {
        self.get_pending_inbound_transaction_by_cancelled(tx_id, true).await
    }

    pub async fn get_pending_inbound_transaction_by_cancelled(
        &self,
        tx_id: TxId,
        cancelled: bool,
    ) -> Result<InboundTransaction, TransactionStorageError>
    {
        let db_clone = self.db.clone();
        let key = if cancelled {
            DbKey::CancelledPendingInboundTransaction(tx_id)
        } else {
            DbKey::PendingInboundTransaction(tx_id)
        };
        let t = tokio::task::spawn_blocking(move || match db_clone.fetch(&key) {
            Ok(None) => Err(TransactionStorageError::ValueNotFound(key)),
            Ok(Some(DbValue::PendingInboundTransaction(pt))) => Ok(pt),
            Ok(Some(other)) => unexpected_result(key, other),
            Err(e) => log_error(key, e),
        })
        .await
        .map_err(|err| TransactionStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(*t)
    }

    pub async fn get_completed_transaction(
        &self,
        tx_id: TxId,
    ) -> Result<CompletedTransaction, TransactionStorageError>
    {
        self.get_completed_transaction_by_cancelled(tx_id, false).await
    }

    pub async fn get_cancelled_completed_transaction(
        &self,
        tx_id: TxId,
    ) -> Result<CompletedTransaction, TransactionStorageError>
    {
        self.get_completed_transaction_by_cancelled(tx_id, true).await
    }

    pub async fn get_completed_transaction_by_cancelled(
        &self,
        tx_id: TxId,
        cancelled: bool,
    ) -> Result<CompletedTransaction, TransactionStorageError>
    {
        let db_clone = self.db.clone();
        let key = DbKey::CompletedTransaction(tx_id);
        let t = tokio::task::spawn_blocking(move || match db_clone.fetch(&DbKey::CompletedTransaction(tx_id)) {
            Ok(None) => Err(TransactionStorageError::ValueNotFound(key)),
            Ok(Some(DbValue::CompletedTransaction(pt))) => {
                if pt.cancelled == cancelled {
                    Ok(pt)
                } else {
                    Err(TransactionStorageError::ValueNotFound(key))
                }
            },
            Ok(Some(other)) => unexpected_result(key, other),
            Err(e) => log_error(key, e),
        })
        .await
        .map_err(|err| TransactionStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(*t)
    }

    pub async fn get_completed_transaction_cancelled_or_not(
        &self,
        tx_id: TxId,
    ) -> Result<CompletedTransaction, TransactionStorageError>
    {
        let db_clone = self.db.clone();
        let key = DbKey::CompletedTransaction(tx_id);
        let t = tokio::task::spawn_blocking(move || match db_clone.fetch(&DbKey::CompletedTransaction(tx_id)) {
            Ok(None) => Err(TransactionStorageError::ValueNotFound(key)),
            Ok(Some(DbValue::CompletedTransaction(pt))) => Ok(pt),
            Ok(Some(other)) => unexpected_result(key, other),
            Err(e) => log_error(key, e),
        })
        .await
        .map_err(|err| TransactionStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(*t)
    }

    pub async fn get_pending_inbound_transactions(
        &self,
    ) -> Result<HashMap<TxId, InboundTransaction>, TransactionStorageError> {
        self.get_pending_inbound_transactions_by_cancelled(false).await
    }

    pub async fn get_cancelled_pending_inbound_transactions(
        &self,
    ) -> Result<HashMap<TxId, InboundTransaction>, TransactionStorageError> {
        self.get_pending_inbound_transactions_by_cancelled(true).await
    }

    async fn get_pending_inbound_transactions_by_cancelled(
        &self,
        cancelled: bool,
    ) -> Result<HashMap<TxId, InboundTransaction>, TransactionStorageError>
    {
        let db_clone = self.db.clone();

        let key = if cancelled {
            DbKey::CancelledPendingInboundTransactions
        } else {
            DbKey::PendingInboundTransactions
        };

        let t = tokio::task::spawn_blocking(move || match db_clone.fetch(&key) {
            Ok(None) => log_error(
                key,
                TransactionStorageError::UnexpectedResult(
                    "Could not retrieve pending inbound transactions".to_string(),
                ),
            ),
            Ok(Some(DbValue::PendingInboundTransactions(pt))) => Ok(pt),
            Ok(Some(other)) => unexpected_result(key, other),
            Err(e) => log_error(key, e),
        })
        .await
        .map_err(|err| TransactionStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(t)
    }

    pub async fn get_pending_outbound_transactions(
        &self,
    ) -> Result<HashMap<TxId, OutboundTransaction>, TransactionStorageError> {
        self.get_pending_outbound_transactions_by_cancelled(false).await
    }

    pub async fn get_cancelled_pending_outbound_transactions(
        &self,
    ) -> Result<HashMap<TxId, OutboundTransaction>, TransactionStorageError> {
        self.get_pending_outbound_transactions_by_cancelled(true).await
    }

    async fn get_pending_outbound_transactions_by_cancelled(
        &self,
        cancelled: bool,
    ) -> Result<HashMap<TxId, OutboundTransaction>, TransactionStorageError>
    {
        let db_clone = self.db.clone();

        let key = if cancelled {
            DbKey::CancelledPendingOutboundTransactions
        } else {
            DbKey::PendingOutboundTransactions
        };

        let t = tokio::task::spawn_blocking(move || match db_clone.fetch(&key) {
            Ok(None) => log_error(
                key,
                TransactionStorageError::UnexpectedResult(
                    "Could not retrieve pending outbound transactions".to_string(),
                ),
            ),
            Ok(Some(DbValue::PendingOutboundTransactions(pt))) => Ok(pt),
            Ok(Some(other)) => unexpected_result(key, other),
            Err(e) => log_error(key, e),
        })
        .await
        .map_err(|err| TransactionStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(t)
    }

    pub async fn get_pending_transaction_counterparty_pub_key_by_tx_id(
        &mut self,
        tx_id: TxId,
    ) -> Result<CommsPublicKey, TransactionStorageError>
    {
        let db_clone = self.db.clone();
        let pub_key =
            tokio::task::spawn_blocking(move || db_clone.get_pending_transaction_counterparty_pub_key_by_tx_id(tx_id))
                .await
                .map_err(|err| TransactionStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(pub_key)
    }

    pub async fn get_completed_transactions(
        &self,
    ) -> Result<HashMap<TxId, CompletedTransaction>, TransactionStorageError> {
        self.get_completed_transactions_by_cancelled(false).await
    }

    pub async fn get_cancelled_completed_transactions(
        &self,
    ) -> Result<HashMap<TxId, CompletedTransaction>, TransactionStorageError> {
        self.get_completed_transactions_by_cancelled(true).await
    }

    // TODO: all the single getters should use an Option rather than an error to indicate not found.
    pub async fn get_any_transaction(&self, tx_id: TxId) -> Result<Option<WalletTransaction>, TransactionStorageError> {
        let db_clone = self.db.clone();
        let key = DbKey::AnyTransaction(tx_id);
        let t = tokio::task::spawn_blocking(move || match db_clone.fetch(&key) {
            Ok(None) => Ok(None),
            Ok(Some(DbValue::WalletTransaction(pt))) => Ok(Some(*pt)),
            Ok(Some(other)) => unexpected_result(key, other),
            Err(e) => log_error(key, e),
        })
        .await
        .map_err(|err| TransactionStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(t)
    }

    async fn get_completed_transactions_by_cancelled(
        &self,
        cancelled: bool,
    ) -> Result<HashMap<TxId, CompletedTransaction>, TransactionStorageError>
    {
        let db_clone = self.db.clone();

        let key = if cancelled {
            DbKey::CancelledCompletedTransactions
        } else {
            DbKey::CompletedTransactions
        };

        let t = tokio::task::spawn_blocking(move || match db_clone.fetch(&key) {
            Ok(None) => log_error(
                key,
                TransactionStorageError::UnexpectedResult("Could not retrieve completed transactions".to_string()),
            ),
            Ok(Some(DbValue::CompletedTransactions(pt))) => Ok(pt),
            Ok(Some(other)) => unexpected_result(key, other),
            Err(e) => log_error(key, e),
        })
        .await
        .map_err(|err| TransactionStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(t)
    }

    /// This method moves a `PendingOutboundTransaction` to the `CompleteTransaction` collection.
    pub async fn complete_outbound_transaction(
        &self,
        tx_id: TxId,
        transaction: CompletedTransaction,
    ) -> Result<(), TransactionStorageError>
    {
        let db_clone = self.db.clone();

        tokio::task::spawn_blocking(move || db_clone.complete_outbound_transaction(tx_id, transaction))
            .await
            .map_err(|err| TransactionStorageError::BlockingTaskSpawnError(err.to_string()))
            .and_then(|inner_result| inner_result)
    }

    /// This method moves a `PendingInboundTransaction` to the `CompleteTransaction` collection.
    pub async fn complete_inbound_transaction(
        &self,
        tx_id: TxId,
        transaction: CompletedTransaction,
    ) -> Result<(), TransactionStorageError>
    {
        let db_clone = self.db.clone();

        tokio::task::spawn_blocking(move || db_clone.complete_inbound_transaction(tx_id, transaction))
            .await
            .map_err(|err| TransactionStorageError::BlockingTaskSpawnError(err.to_string()))
            .and_then(|inner_result| inner_result)
    }

    pub async fn cancel_completed_transaction(&self, tx_id: TxId) -> Result<(), TransactionStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || db_clone.cancel_completed_transaction(tx_id))
            .await
            .map_err(|err| TransactionStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(())
    }

    pub async fn cancel_pending_transaction(&self, tx_id: TxId) -> Result<(), TransactionStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || db_clone.cancel_pending_transaction(tx_id))
            .await
            .map_err(|err| TransactionStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(())
    }

    pub async fn mark_direct_send_success(&self, tx_id: TxId) -> Result<(), TransactionStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || db_clone.mark_direct_send_success(tx_id))
            .await
            .map_err(|err| TransactionStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(())
    }

    /// Indicated that the specified completed transaction has been broadcast into the mempool
    pub async fn broadcast_completed_transaction(&self, tx_id: TxId) -> Result<(), TransactionStorageError> {
        let db_clone = self.db.clone();

        tokio::task::spawn_blocking(move || db_clone.broadcast_completed_transaction(tx_id))
            .await
            .map_err(|err| TransactionStorageError::BlockingTaskSpawnError(err.to_string()))
            .and_then(|inner_result| inner_result)
    }

    /// Indicated that the specified completed transaction has been detected as mined on the base layer
    pub async fn mine_completed_transaction(&self, tx_id: TxId) -> Result<(), TransactionStorageError> {
        let db_clone = self.db.clone();

        tokio::task::spawn_blocking(move || db_clone.mine_completed_transaction(tx_id))
            .await
            .map_err(|err| TransactionStorageError::BlockingTaskSpawnError(err.to_string()))
            .and_then(|inner_result| inner_result)
    }

    pub async fn add_utxo_import_transaction(
        &self,
        tx_id: TxId,
        amount: MicroTari,
        source_public_key: CommsPublicKey,
        comms_public_key: CommsPublicKey,
        message: String,
    ) -> Result<(), TransactionStorageError>
    {
        let transaction = CompletedTransaction::new(
            tx_id,
            source_public_key.clone(),
            comms_public_key.clone(),
            amount,
            MicroTari::from(0),
            Transaction::new(Vec::new(), Vec::new(), Vec::new(), BlindingFactor::default()),
            TransactionStatus::Imported,
            message,
            Utc::now().naive_utc(),
            TransactionDirection::Inbound,
            None,
        );

        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || {
            db_clone.write(WriteOperation::Insert(DbKeyValuePair::CompletedTransaction(
                tx_id,
                Box::new(transaction),
            )))
        })
        .await
        .map_err(|err| TransactionStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(())
    }

    pub async fn cancel_coinbase_transaction_at_block_height(
        &self,
        block_height: u64,
    ) -> Result<(), TransactionStorageError>
    {
        let db_clone = self.db.clone();

        tokio::task::spawn_blocking(move || db_clone.cancel_coinbase_transaction_at_block_height(block_height))
            .await
            .map_err(|err| TransactionStorageError::BlockingTaskSpawnError(err.to_string()))
            .and_then(|inner_result| inner_result)
    }

    pub async fn find_coinbase_transaction_at_block_height(
        &self,
        block_height: u64,
        amount: MicroTari,
    ) -> Result<Option<CompletedTransaction>, TransactionStorageError>
    {
        let db_clone = self.db.clone();

        tokio::task::spawn_blocking(move || db_clone.find_coinbase_transaction_at_block_height(block_height, amount))
            .await
            .map_err(|err| TransactionStorageError::BlockingTaskSpawnError(err.to_string()))
            .and_then(|inner_result| inner_result)
    }

    pub async fn apply_encryption(&self, cipher: Aes256Gcm) -> Result<(), TransactionStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || db_clone.apply_encryption(cipher))
            .await
            .map_err(|err| TransactionStorageError::BlockingTaskSpawnError(err.to_string()))
            .and_then(|inner_result| inner_result)
    }

    pub async fn remove_encryption(&self) -> Result<(), TransactionStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || db_clone.remove_encryption())
            .await
            .map_err(|err| TransactionStorageError::BlockingTaskSpawnError(err.to_string()))
            .and_then(|inner_result| inner_result)
    }

    pub async fn increment_send_count(&self, tx_id: TxId) -> Result<(), TransactionStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || db_clone.increment_send_count(tx_id))
            .await
            .map_err(|err| TransactionStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(())
    }

    pub async fn confirm_broadcast_or_coinbase_transaction(&self, tx_id: TxId) -> Result<(), TransactionStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || db_clone.confirm_broadcast_or_coinbase_transaction(tx_id))
            .await
            .map_err(|err| TransactionStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(())
    }

    pub async fn unconfirm_mined_transaction(&self, tx_id: TxId) -> Result<(), TransactionStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || db_clone.unconfirm_mined_transaction(tx_id))
            .await
            .map_err(|err| TransactionStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(())
    }

    pub async fn set_completed_transaction_validity(
        &self,
        tx_id: TxId,
        valid: bool,
    ) -> Result<(), TransactionStorageError>
    {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || db_clone.set_completed_transaction_validity(tx_id, valid))
            .await
            .map_err(|err| TransactionStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(())
    }

    pub async fn set_transaction_confirmations(
        &self,
        tx_id: TxId,
        confirmations: u64,
    ) -> Result<(), TransactionStorageError>
    {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || db_clone.update_confirmations(tx_id, confirmations))
            .await
            .map_err(|err| TransactionStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(())
    }

    pub async fn set_transaction_mined_height(
        &self,
        tx_id: TxId,
        mined_height: u64,
    ) -> Result<(), TransactionStorageError>
    {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || db_clone.update_mined_height(tx_id, mined_height))
            .await
            .map_err(|err| TransactionStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(())
    }
}

impl Display for DbKey {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            DbKey::PendingOutboundTransaction(_) => f.write_str(&"Pending Outbound Transaction".to_string()),
            DbKey::PendingInboundTransaction(_) => f.write_str(&"Pending Inbound Transaction".to_string()),

            DbKey::CompletedTransaction(_) => f.write_str(&"Completed Transaction".to_string()),
            DbKey::PendingOutboundTransactions => f.write_str(&"All Pending Outbound Transactions".to_string()),
            DbKey::PendingInboundTransactions => f.write_str(&"All Pending Inbound Transactions".to_string()),
            DbKey::CompletedTransactions => f.write_str(&"All Complete Transactions".to_string()),
            DbKey::CancelledPendingOutboundTransactions => {
                f.write_str(&"All Cancelled Pending Inbound Transactions".to_string())
            },
            DbKey::CancelledPendingInboundTransactions => {
                f.write_str(&"All Cancelled Pending Outbound Transactions".to_string())
            },
            DbKey::CancelledCompletedTransactions => f.write_str(&"All Cancelled Complete Transactions".to_string()),
            DbKey::CancelledPendingOutboundTransaction(_) => {
                f.write_str(&"Cancelled Pending Outbound Transaction".to_string())
            },
            DbKey::CancelledPendingInboundTransaction(_) => {
                f.write_str(&"Cancelled Pending Inbound Transaction".to_string())
            },
            DbKey::AnyTransaction(_) => f.write_str(&"Any Transaction".to_string()),
        }
    }
}

impl Display for DbValue {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            DbValue::PendingOutboundTransaction(_) => f.write_str(&"Pending Outbound Transaction".to_string()),
            DbValue::PendingInboundTransaction(_) => f.write_str(&"Pending Inbound Transaction".to_string()),
            DbValue::CompletedTransaction(_) => f.write_str(&"Completed Transaction".to_string()),
            DbValue::PendingOutboundTransactions(_) => f.write_str(&"All Pending Outbound Transactions".to_string()),
            DbValue::PendingInboundTransactions(_) => f.write_str(&"All Pending Inbound Transactions".to_string()),
            DbValue::CompletedTransactions(_) => f.write_str(&"All Complete Transactions".to_string()),
            DbValue::WalletTransaction(_) => f.write_str(&"Any Wallet Transaction".to_string()),
        }
    }
}

fn log_error<T>(req: DbKey, err: TransactionStorageError) -> Result<T, TransactionStorageError> {
    error!(
        target: LOG_TARGET,
        "Database access error on request: {}: {}",
        req,
        err.to_string()
    );
    Err(err)
}

fn unexpected_result<T>(req: DbKey, res: DbValue) -> Result<T, TransactionStorageError> {
    let msg = format!("Unexpected result for database query {}. Response: {}", req, res);
    error!(target: LOG_TARGET, "{}", msg);
    Err(TransactionStorageError::UnexpectedResult(msg))
}
