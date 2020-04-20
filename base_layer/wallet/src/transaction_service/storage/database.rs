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

use crate::{output_manager_service::TxId, transaction_service::error::TransactionStorageError};
use chrono::{NaiveDateTime, Utc};
use log::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    convert::TryFrom,
    fmt::{Display, Error, Formatter},
    sync::Arc,
};
use tari_comms::types::CommsPublicKey;
use tari_core::transactions::{
    tari_amount::{uT, MicroTari},
    transaction::Transaction,
    types::{BlindingFactor, Commitment},
    ReceiverTransactionProtocol,
    SenderTransactionProtocol,
};

const LOG_TARGET: &str = "wallet::transaction_service::database";

/// This trait defines the required behaviour that a storage backend must provide for the Transactionservice.
/// Data is passed to and from the backend via the [DbKey], [DbValue], and [DbValueKey] enums. If new data types are
/// required to be supported by the backends then these enums can be updated to reflect this requirement and the trait
/// will remain the same
pub trait TransactionBackend: Send + Sync {
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
    /// Complete pending coinbase transaction, this operation must delete the `PendingCoinbaseTransaction` with the
    /// provided `TxId` and insert the provided `CompletedTransaction` into `CompletedTransactions`.
    fn complete_coinbase_transaction(
        &self,
        tx_id: TxId,
        completed_transaction: CompletedTransaction,
    ) -> Result<(), TransactionStorageError>;
    /// Indicated that a completed transaction has been broadcast to the mempools
    fn broadcast_completed_transaction(&self, tx_id: TxId) -> Result<(), TransactionStorageError>;
    /// Indicated that a completed transaction has been detected as mined on the base layer
    fn mine_completed_transaction(&self, tx_id: TxId) -> Result<(), TransactionStorageError>;
    /// Cancel Completed transaction, this will update the transaction status
    fn cancel_completed_transaction(&self, tx_id: TxId) -> Result<(), TransactionStorageError>;
    /// Cancel Completed transaction, this will update the transaction status
    fn cancel_pending_transaction(&self, tx_id: TxId) -> Result<(), TransactionStorageError>;
    /// Update a completed transactions timestamp for use in test data generation
    #[cfg(feature = "test_harness")]
    fn update_completed_transaction_timestamp(
        &self,
        tx_id: TxId,
        timestamp: NaiveDateTime,
    ) -> Result<(), TransactionStorageError>;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TransactionStatus {
    /// This transaction has been completed between the parties but has not been broadcast to the base layer network.
    Completed,
    /// This transaction has been broadcast to the base layer network and is currently in one or more base node
    /// mempools.
    Broadcast,
    /// This transaction has been mined and included in a block.
    Mined,
    /// This transaction was generated as part of importing a spendable UTXO
    Imported,
    /// This transaction is still being negotiated by the parties
    Pending,
    /// This transaction has been cancelled
    Cancelled,
}

impl TryFrom<i32> for TransactionStatus {
    type Error = TransactionStorageError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(TransactionStatus::Completed),
            1 => Ok(TransactionStatus::Broadcast),
            2 => Ok(TransactionStatus::Mined),
            3 => Ok(TransactionStatus::Imported),
            4 => Ok(TransactionStatus::Pending),
            5 => Ok(TransactionStatus::Cancelled),
            _ => Err(TransactionStorageError::ConversionError),
        }
    }
}

impl Default for TransactionStatus {
    fn default() -> Self {
        TransactionStatus::Pending
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InboundTransaction {
    pub tx_id: TxId,
    pub source_public_key: CommsPublicKey,
    pub amount: MicroTari,
    pub receiver_protocol: ReceiverTransactionProtocol,
    pub status: TransactionStatus,
    pub message: String,
    pub timestamp: NaiveDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OutboundTransaction {
    pub tx_id: TxId,
    pub destination_public_key: CommsPublicKey,
    pub amount: MicroTari,
    pub fee: MicroTari,
    pub sender_protocol: SenderTransactionProtocol,
    pub status: TransactionStatus,
    pub message: String,
    pub timestamp: NaiveDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PendingCoinbaseTransaction {
    pub tx_id: TxId,
    pub amount: MicroTari,
    pub commitment: Commitment,
    pub timestamp: NaiveDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompletedTransaction {
    pub tx_id: TxId,
    pub source_public_key: CommsPublicKey,
    pub destination_public_key: CommsPublicKey,
    pub amount: MicroTari,
    pub fee: MicroTari,
    pub transaction: Transaction,
    pub status: TransactionStatus,
    pub message: String,
    pub timestamp: NaiveDateTime,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DbKey {
    PendingOutboundTransaction(TxId),
    PendingInboundTransaction(TxId),
    CompletedTransaction(TxId),
    PendingCoinbaseTransaction(TxId),
    PendingOutboundTransactions,
    PendingInboundTransactions,
    PendingCoinbaseTransactions,
    CompletedTransactions,
}

#[derive(Debug)]
pub enum DbValue {
    PendingOutboundTransaction(Box<OutboundTransaction>),
    PendingInboundTransaction(Box<InboundTransaction>),
    PendingCoinbaseTransaction(Box<PendingCoinbaseTransaction>),
    CompletedTransaction(Box<CompletedTransaction>),
    PendingOutboundTransactions(HashMap<TxId, OutboundTransaction>),
    PendingInboundTransactions(HashMap<TxId, InboundTransaction>),
    PendingCoinbaseTransactions(HashMap<TxId, PendingCoinbaseTransaction>),
    CompletedTransactions(HashMap<TxId, CompletedTransaction>),
}

pub enum DbKeyValuePair {
    PendingOutboundTransaction(TxId, Box<OutboundTransaction>),
    PendingInboundTransaction(TxId, Box<InboundTransaction>),
    PendingCoinbaseTransaction(TxId, Box<PendingCoinbaseTransaction>),
    CompletedTransaction(TxId, Box<CompletedTransaction>),
}

pub enum WriteOperation {
    Insert(DbKeyValuePair),
    Remove(DbKey),
}

impl From<CompletedTransaction> for InboundTransaction {
    fn from(ct: CompletedTransaction) -> Self {
        Self {
            tx_id: ct.tx_id,
            source_public_key: ct.source_public_key,
            amount: ct.amount,
            receiver_protocol: ReceiverTransactionProtocol::new_placeholder(),
            status: ct.status,
            message: ct.message,
            timestamp: ct.timestamp,
        }
    }
}

impl From<CompletedTransaction> for OutboundTransaction {
    fn from(ct: CompletedTransaction) -> Self {
        Self {
            tx_id: ct.tx_id,
            destination_public_key: ct.destination_public_key,
            amount: ct.amount,
            fee: ct.fee,
            sender_protocol: SenderTransactionProtocol::new_placeholder(),
            status: ct.status,
            message: ct.message,
            timestamp: ct.timestamp,
        }
    }
}

// Private macro that pulls out all the boiler plate of extracting a DB query result from its variants
macro_rules! fetch {
    ($db:ident, $key_val:expr, $key_var:ident) => {{
        let key = DbKey::$key_var($key_val);
        match $db.fetch(&key) {
            Ok(None) => Err(TransactionStorageError::ValueNotFound(key)),
            Ok(Some(DbValue::$key_var(k))) => Ok(*k),
            Ok(Some(other)) => unexpected_result(key, other),
            Err(e) => log_error(key, e),
        }
    }};
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
        .or_else(|err| Err(TransactionStorageError::BlockingTaskSpawnError(err.to_string())))??;

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
        .or_else(|err| Err(TransactionStorageError::BlockingTaskSpawnError(err.to_string())))??;
        Ok(())
    }

    pub async fn remove_pending_outbound_transaction(&self, tx_id: TxId) -> Result<(), TransactionStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || {
            db_clone.write(WriteOperation::Remove(DbKey::PendingOutboundTransaction(tx_id)))
        })
        .await
        .or_else(|err| Err(TransactionStorageError::BlockingTaskSpawnError(err.to_string())))??;
        Ok(())
    }

    pub async fn add_pending_coinbase_transaction(
        &self,
        tx_id: TxId,
        coinbase_tx: PendingCoinbaseTransaction,
    ) -> Result<(), TransactionStorageError>
    {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || {
            db_clone.write(WriteOperation::Insert(DbKeyValuePair::PendingCoinbaseTransaction(
                tx_id,
                Box::new(coinbase_tx),
            )))
        })
        .await
        .or_else(|err| Err(TransactionStorageError::BlockingTaskSpawnError(err.to_string())))??;
        Ok(())
    }

    /// Check if a transaction with the specified TxId exists in any of the collections
    pub async fn transaction_exists(&self, tx_id: TxId) -> Result<bool, TransactionStorageError> {
        let db_clone = self.db.clone();
        let tx_id_clone = tx_id;
        tokio::task::spawn_blocking(move || db_clone.transaction_exists(tx_id_clone))
            .await
            .or_else(|err| Err(TransactionStorageError::BlockingTaskSpawnError(err.to_string())))
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
        .or_else(|err| Err(TransactionStorageError::BlockingTaskSpawnError(err.to_string())))
        .and_then(|inner_result| inner_result)
    }

    pub async fn get_pending_outbound_transaction(
        &self,
        tx_id: TxId,
    ) -> Result<OutboundTransaction, TransactionStorageError>
    {
        let db_clone = self.db.clone();
        let result = tokio::task::spawn_blocking(move || fetch!(db_clone, tx_id, PendingOutboundTransaction))
            .await
            .or_else(|err| Err(TransactionStorageError::BlockingTaskSpawnError(err.to_string())))??;
        Ok(result)
    }

    pub async fn get_pending_inbound_transaction(
        &self,
        tx_id: TxId,
    ) -> Result<InboundTransaction, TransactionStorageError>
    {
        let db_clone = self.db.clone();

        let result = tokio::task::spawn_blocking(move || fetch!(db_clone, tx_id, PendingInboundTransaction))
            .await
            .or_else(|err| Err(TransactionStorageError::BlockingTaskSpawnError(err.to_string())))??;

        Ok(result)
    }

    pub async fn get_pending_coinbase_transaction(
        &self,
        tx_id: TxId,
    ) -> Result<PendingCoinbaseTransaction, TransactionStorageError>
    {
        let db_clone = self.db.clone();

        let result = tokio::task::spawn_blocking(move || fetch!(db_clone, tx_id, PendingCoinbaseTransaction))
            .await
            .or_else(|err| Err(TransactionStorageError::BlockingTaskSpawnError(err.to_string())))??;

        Ok(result)
    }

    pub async fn get_completed_transaction(
        &self,
        tx_id: TxId,
    ) -> Result<CompletedTransaction, TransactionStorageError>
    {
        let db_clone = self.db.clone();

        let result = tokio::task::spawn_blocking(move || fetch!(db_clone, tx_id, CompletedTransaction))
            .await
            .or_else(|err| Err(TransactionStorageError::BlockingTaskSpawnError(err.to_string())))??;
        Ok(result)
    }

    pub async fn get_pending_inbound_transactions(
        &self,
    ) -> Result<HashMap<TxId, InboundTransaction>, TransactionStorageError> {
        let db_clone = self.db.clone();

        let t = tokio::task::spawn_blocking(move || match db_clone.fetch(&DbKey::PendingInboundTransactions) {
            Ok(None) => log_error(
                DbKey::PendingInboundTransactions,
                TransactionStorageError::UnexpectedResult(
                    "Could not retrieve pending inbound transactions".to_string(),
                ),
            ),
            Ok(Some(DbValue::PendingInboundTransactions(pt))) => Ok(pt),
            Ok(Some(other)) => unexpected_result(DbKey::PendingInboundTransactions, other),
            Err(e) => log_error(DbKey::PendingInboundTransactions, e),
        })
        .await
        .or_else(|err| Err(TransactionStorageError::BlockingTaskSpawnError(err.to_string())))??;
        Ok(t)
    }

    pub async fn get_pending_outbound_transactions(
        &self,
    ) -> Result<HashMap<TxId, OutboundTransaction>, TransactionStorageError> {
        let db_clone = self.db.clone();

        let t = tokio::task::spawn_blocking(move || match db_clone.fetch(&DbKey::PendingOutboundTransactions) {
            Ok(None) => log_error(
                DbKey::PendingOutboundTransactions,
                TransactionStorageError::UnexpectedResult(
                    "Could not retrieve pending outbound transactions".to_string(),
                ),
            ),
            Ok(Some(DbValue::PendingOutboundTransactions(pt))) => Ok(pt),
            Ok(Some(other)) => unexpected_result(DbKey::PendingOutboundTransactions, other),
            Err(e) => log_error(DbKey::PendingOutboundTransactions, e),
        })
        .await
        .or_else(|err| Err(TransactionStorageError::BlockingTaskSpawnError(err.to_string())))??;
        Ok(t)
    }

    pub async fn get_pending_coinbase_transactions(
        &self,
    ) -> Result<HashMap<TxId, PendingCoinbaseTransaction>, TransactionStorageError> {
        let db_clone = self.db.clone();

        let t = tokio::task::spawn_blocking(move || match db_clone.fetch(&DbKey::PendingCoinbaseTransactions) {
            Ok(None) => log_error(
                DbKey::PendingCoinbaseTransactions,
                TransactionStorageError::UnexpectedResult(
                    "Could not retrieve pending coinbase transactions".to_string(),
                ),
            ),
            Ok(Some(DbValue::PendingCoinbaseTransactions(pt))) => Ok(pt),
            Ok(Some(other)) => unexpected_result(DbKey::PendingCoinbaseTransactions, other),
            Err(e) => log_error(DbKey::PendingCoinbaseTransactions, e),
        })
        .await
        .or_else(|err| Err(TransactionStorageError::BlockingTaskSpawnError(err.to_string())))??;
        Ok(t)
    }

    pub async fn get_completed_transactions(
        &self,
    ) -> Result<HashMap<TxId, CompletedTransaction>, TransactionStorageError> {
        let db_clone = self.db.clone();

        let t = tokio::task::spawn_blocking(move || match db_clone.fetch(&DbKey::CompletedTransactions) {
            Ok(None) => log_error(
                DbKey::CompletedTransactions,
                TransactionStorageError::UnexpectedResult("Could not retrieve completed transactions".to_string()),
            ),
            Ok(Some(DbValue::CompletedTransactions(pt))) => Ok(pt),
            Ok(Some(other)) => unexpected_result(DbKey::CompletedTransactions, other),
            Err(e) => log_error(DbKey::CompletedTransactions, e),
        })
        .await
        .or_else(|err| Err(TransactionStorageError::BlockingTaskSpawnError(err.to_string())))??;
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
            .or_else(|err| Err(TransactionStorageError::BlockingTaskSpawnError(err.to_string())))
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
            .or_else(|err| Err(TransactionStorageError::BlockingTaskSpawnError(err.to_string())))
            .and_then(|inner_result| inner_result)
    }

    /// This method moves a `PendingCoinbaseTransaction` to the `CompleteTransaction` collection.
    pub async fn complete_coinbase_transaction(
        &self,
        tx_id: TxId,
        transaction: CompletedTransaction,
    ) -> Result<(), TransactionStorageError>
    {
        let db_clone = self.db.clone();

        tokio::task::spawn_blocking(move || db_clone.complete_coinbase_transaction(tx_id, transaction))
            .await
            .or_else(|err| Err(TransactionStorageError::BlockingTaskSpawnError(err.to_string())))
            .and_then(|inner_result| inner_result)
    }

    pub async fn cancel_coinbase_transaction(&mut self, tx_id: TxId) -> Result<(), TransactionStorageError> {
        let db_clone = self.db.clone();

        tokio::task::spawn_blocking(move || {
            db_clone.write(WriteOperation::Remove(DbKey::PendingCoinbaseTransaction(tx_id)))
        })
        .await
        .or_else(|err| Err(TransactionStorageError::BlockingTaskSpawnError(err.to_string())))??;
        Ok(())
    }

    pub async fn cancel_completed_transaction(&mut self, tx_id: TxId) -> Result<(), TransactionStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || db_clone.cancel_completed_transaction(tx_id))
            .await
            .or_else(|err| Err(TransactionStorageError::BlockingTaskSpawnError(err.to_string())))??;
        Ok(())
    }

    pub async fn cancel_pending_transaction(&mut self, tx_id: TxId) -> Result<(), TransactionStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || db_clone.cancel_pending_transaction(tx_id))
            .await
            .or_else(|err| Err(TransactionStorageError::BlockingTaskSpawnError(err.to_string())))??;
        Ok(())
    }

    /// Indicated that the specified completed transaction has been broadcast into the mempool
    pub async fn broadcast_completed_transaction(&mut self, tx_id: TxId) -> Result<(), TransactionStorageError> {
        let db_clone = self.db.clone();

        tokio::task::spawn_blocking(move || db_clone.broadcast_completed_transaction(tx_id))
            .await
            .or_else(|err| Err(TransactionStorageError::BlockingTaskSpawnError(err.to_string())))
            .and_then(|inner_result| inner_result)
    }

    /// Indicated that the specified completed transaction has been detected as mined on the base layer
    pub async fn mine_completed_transaction(&mut self, tx_id: TxId) -> Result<(), TransactionStorageError> {
        let db_clone = self.db.clone();

        tokio::task::spawn_blocking(move || db_clone.mine_completed_transaction(tx_id))
            .await
            .or_else(|err| Err(TransactionStorageError::BlockingTaskSpawnError(err.to_string())))
            .and_then(|inner_result| inner_result)
    }

    #[allow(clippy::erasing_op)] // this is for 0 * uT
    pub async fn add_utxo_import_transaction(
        &mut self,
        tx_id: TxId,
        amount: MicroTari,
        source_public_key: CommsPublicKey,
        comms_public_key: CommsPublicKey,
        message: String,
    ) -> Result<(), TransactionStorageError>
    {
        let transaction = CompletedTransaction {
            tx_id,
            source_public_key: source_public_key.clone(),
            destination_public_key: comms_public_key.clone(),
            amount,
            fee: 0 * uT,
            transaction: Transaction::new(Vec::new(), Vec::new(), Vec::new(), BlindingFactor::default()),
            status: TransactionStatus::Imported,
            message,
            timestamp: Utc::now().naive_utc(),
        };

        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || {
            db_clone.write(WriteOperation::Insert(DbKeyValuePair::CompletedTransaction(
                tx_id,
                Box::new(transaction),
            )))
        })
        .await
        .or_else(|err| Err(TransactionStorageError::BlockingTaskSpawnError(err.to_string())))??;
        Ok(())
    }
}

impl Display for DbKey {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            DbKey::PendingOutboundTransaction(_) => f.write_str(&"Pending Outbound Transaction".to_string()),
            DbKey::PendingInboundTransaction(_) => f.write_str(&"Pending Inbound Transaction".to_string()),
            DbKey::PendingCoinbaseTransaction(_) => f.write_str(&"Pending Pending Coinbase Transaction".to_string()),
            DbKey::CompletedTransaction(_) => f.write_str(&"Completed Transaction".to_string()),
            DbKey::PendingOutboundTransactions => f.write_str(&"All Pending Outbound Transactions".to_string()),
            DbKey::PendingInboundTransactions => f.write_str(&"All Pending Inbound Transactions".to_string()),
            DbKey::CompletedTransactions => f.write_str(&"All Complete Transactions".to_string()),
            DbKey::PendingCoinbaseTransactions => f.write_str(&"All Pending Coinbase Transactions".to_string()),
        }
    }
}

impl Display for DbValue {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            DbValue::PendingOutboundTransaction(_) => f.write_str(&"Pending Outbound Transaction".to_string()),
            DbValue::PendingInboundTransaction(_) => f.write_str(&"Pending Inbound Transaction".to_string()),
            DbValue::PendingCoinbaseTransaction(_) => f.write_str(&"Pending Coinbase Transaction".to_string()),
            DbValue::CompletedTransaction(_) => f.write_str(&"Completed Transaction".to_string()),
            DbValue::PendingOutboundTransactions(_) => f.write_str(&"All Pending Outbound Transactions".to_string()),
            DbValue::PendingInboundTransactions(_) => f.write_str(&"All Pending Inbound Transactions".to_string()),
            DbValue::CompletedTransactions(_) => f.write_str(&"All Complete Transactions".to_string()),
            DbValue::PendingCoinbaseTransactions(_) => f.write_str(&"All Pending Coinbase Transactions".to_string()),
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
