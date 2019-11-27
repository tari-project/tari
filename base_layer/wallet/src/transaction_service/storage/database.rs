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
use chrono::NaiveDateTime;
use log::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fmt::{Display, Error, Formatter},
};
use tari_comms::types::CommsPublicKey;
use tari_transactions::{
    tari_amount::MicroTari,
    transaction::Transaction,
    ReceiverTransactionProtocol,
    SenderTransactionProtocol,
};

const LOG_TARGET: &'static str = "wallet::transaction_service::database";

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
    fn write(&mut self, op: WriteOperation) -> Result<Option<DbValue>, TransactionStorageError>;
    /// Complete outbound transaction, this operation must delete the `OutboundTransaction` with the provided
    /// `TxId` and insert the provided `CompletedTransaction` into `CompletedTransactions`.
    fn complete_outbound_transaction(
        &mut self,
        tx_id: TxId,
        completed_transaction: CompletedTransaction,
    ) -> Result<(), TransactionStorageError>;
    /// Complete inbound transaction, this operation must delete the `InboundTransaction` with the provided
    /// `TxId` and insert the provided `CompletedTransaction` into `CompletedTransactions`.
    fn complete_inbound_transaction(
        &mut self,
        tx_id: TxId,
        completed_transaction: CompletedTransaction,
    ) -> Result<(), TransactionStorageError>;
    /// Indicated that a completed transaction has been detected as mined on the base layer
    #[cfg(feature = "test_harness")]
    fn mine_completed_transaction(&mut self, tx_id: TxId) -> Result<(), TransactionStorageError>;
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InboundTransaction {
    pub tx_id: TxId,
    pub source_public_key: CommsPublicKey,
    pub amount: MicroTari,
    pub receiver_protocol: ReceiverTransactionProtocol,
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
    pub message: String,
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
    PendingOutboundTransactions,
    PendingInboundTransactions,
    CompletedTransactions,
}

#[derive(Debug)]
pub enum DbValue {
    PendingOutboundTransaction(Box<OutboundTransaction>),
    PendingInboundTransaction(Box<InboundTransaction>),
    CompletedTransaction(Box<CompletedTransaction>),
    PendingOutboundTransactions(HashMap<TxId, OutboundTransaction>),
    PendingInboundTransactions(HashMap<TxId, InboundTransaction>),
    CompletedTransactions(HashMap<TxId, CompletedTransaction>),
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

// Private macro that pulls out all the boiler plate of extracting a DB query result from its variants
macro_rules! fetch {
    ($self:ident, $key_val:expr, $key_var:ident) => {{
        let key = DbKey::$key_var($key_val);
        match $self.db.fetch(&key) {
            Ok(None) => Err(TransactionStorageError::ValueNotFound(key)),
            Ok(Some(DbValue::$key_var(k))) => Ok(*k),
            Ok(Some(other)) => unexpected_result(key, other),
            Err(e) => log_error(key, e),
        }
    }};
}

/// This structure holds an inner type that implements the `TransactionBackend` trait and contains the more complex
/// data access logic required by the module built onto the functionality defined by the trait
pub struct TransactionDatabase<T>
where T: TransactionBackend
{
    db: T,
}

impl<T> TransactionDatabase<T>
where T: TransactionBackend
{
    pub fn new(db: T) -> Self {
        Self { db }
    }

    pub fn add_pending_inbound_transaction(
        &mut self,
        tx_id: TxId,
        inbound_tx: InboundTransaction,
    ) -> Result<(), TransactionStorageError>
    {
        self.db
            .write(WriteOperation::Insert(DbKeyValuePair::PendingInboundTransaction(
                tx_id,
                Box::new(inbound_tx),
            )))?;
        Ok(())
    }

    pub fn add_pending_outbound_transaction(
        &mut self,
        tx_id: TxId,
        outbound_tx: OutboundTransaction,
    ) -> Result<(), TransactionStorageError>
    {
        self.db
            .write(WriteOperation::Insert(DbKeyValuePair::PendingOutboundTransaction(
                tx_id,
                Box::new(outbound_tx),
            )))?;
        Ok(())
    }

    /// Check if a transaction with the specified TxId exists in any of the collections
    pub fn transaction_exists(&self, tx_id: &TxId) -> Result<bool, TransactionStorageError> {
        Ok(self.db.contains(&DbKey::PendingOutboundTransaction(tx_id.clone()))? ||
            self.db.contains(&DbKey::PendingInboundTransaction(tx_id.clone()))? ||
            self.db.contains(&DbKey::CompletedTransaction(tx_id.clone()))?)
    }

    pub fn get_pending_outbound_transaction(
        &self,
        tx_id: TxId,
    ) -> Result<OutboundTransaction, TransactionStorageError>
    {
        let result = fetch!(self, tx_id, PendingOutboundTransaction)?;
        Ok(result)
    }

    pub fn get_pending_inbound_transactions(
        &self,
    ) -> Result<HashMap<TxId, InboundTransaction>, TransactionStorageError> {
        let t = match self.db.fetch(&DbKey::PendingInboundTransactions) {
            Ok(None) => log_error(
                DbKey::PendingInboundTransactions,
                TransactionStorageError::UnexpectedResult(
                    "Could not retrieve pending inbound transactions".to_string(),
                ),
            ),
            Ok(Some(DbValue::PendingInboundTransactions(pt))) => Ok(pt),
            Ok(Some(other)) => unexpected_result(DbKey::PendingInboundTransactions, other),
            Err(e) => log_error(DbKey::PendingInboundTransactions, e),
        }?;
        Ok(t)
    }

    pub fn get_pending_outbound_transactions(
        &self,
    ) -> Result<HashMap<TxId, OutboundTransaction>, TransactionStorageError> {
        let t = match self.db.fetch(&DbKey::PendingOutboundTransactions) {
            Ok(None) => log_error(
                DbKey::PendingOutboundTransactions,
                TransactionStorageError::UnexpectedResult(
                    "Could not retrieve pending outbound transactions".to_string(),
                ),
            ),
            Ok(Some(DbValue::PendingOutboundTransactions(pt))) => Ok(pt),
            Ok(Some(other)) => unexpected_result(DbKey::PendingOutboundTransactions, other),
            Err(e) => log_error(DbKey::PendingOutboundTransactions, e),
        }?;
        Ok(t)
    }

    pub fn get_completed_transactions(&self) -> Result<HashMap<TxId, CompletedTransaction>, TransactionStorageError> {
        let t = match self.db.fetch(&DbKey::CompletedTransactions) {
            Ok(None) => log_error(
                DbKey::CompletedTransactions,
                TransactionStorageError::UnexpectedResult("Could not retrieve completed transactions".to_string()),
            ),
            Ok(Some(DbValue::CompletedTransactions(pt))) => Ok(pt),
            Ok(Some(other)) => unexpected_result(DbKey::CompletedTransactions, other),
            Err(e) => log_error(DbKey::CompletedTransactions, e),
        }?;
        Ok(t)
    }

    /// This method moves a `PendingOutboundTransaction` to the `CompleteTransaction` collection.
    pub fn complete_outbound_transaction(
        &mut self,
        tx_id: TxId,
        transaction: CompletedTransaction,
    ) -> Result<(), TransactionStorageError>
    {
        self.db.complete_outbound_transaction(tx_id, transaction)
    }

    /// This method moves a `PendingInboundTransaction` to the `CompleteTransaction` collection.
    pub fn complete_inbound_transaction(
        &mut self,
        tx_id: TxId,
        transaction: CompletedTransaction,
    ) -> Result<(), TransactionStorageError>
    {
        self.db.complete_inbound_transaction(tx_id, transaction)
    }

    /// Indicated that the specified completed transaction has been detected as mined on the base layer
    #[cfg(feature = "test_harness")]
    pub fn mine_completed_transaction(&mut self, tx_id: TxId) -> Result<(), TransactionStorageError> {
        self.db.mine_completed_transaction(tx_id)
    }
}

impl Display for DbKey {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            DbKey::PendingOutboundTransaction(_) => f.write_str(&format!("Pending Outbound Transaction")),
            DbKey::PendingInboundTransaction(_) => f.write_str(&format!("Pending Inbound Transaction")),
            DbKey::CompletedTransaction(_) => f.write_str(&format!("Completed Transaction")),
            DbKey::PendingOutboundTransactions => f.write_str(&format!("All Pending Outbound Transactions")),
            DbKey::PendingInboundTransactions => f.write_str(&format!("All Pending Inbound Transactions")),
            DbKey::CompletedTransactions => f.write_str(&format!("All Complete Transactions")),
        }
    }
}

impl Display for DbValue {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            DbValue::PendingOutboundTransaction(_) => f.write_str(&format!("Pending Outbound Transaction")),
            DbValue::PendingInboundTransaction(_) => f.write_str(&format!("Pending Inbound Transaction")),
            DbValue::CompletedTransaction(_) => f.write_str(&format!("Completed Transaction")),
            DbValue::PendingOutboundTransactions(_) => f.write_str(&format!("All Pending Outbound Transactions")),
            DbValue::PendingInboundTransactions(_) => f.write_str(&format!("All Pending Inbound Transactions")),
            DbValue::CompletedTransactions(_) => f.write_str(&format!("All Complete Transactions")),
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
