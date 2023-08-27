// Copyright 2019. The Taiji Project
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
    collections::HashMap,
    convert::TryFrom,
    fmt,
    fmt::{Display, Error, Formatter},
    sync::Arc,
};

use chrono::{NaiveDateTime, Utc};
use log::*;
use taiji_common_types::{
    taiji_address::TaijiAddress,
    transaction::{ImportStatus, TransactionDirection, TransactionStatus, TxId},
    types::{BlockHash, PrivateKey},
};
use taiji_core::transactions::{taiji_amount::MicroMinotaiji, transaction_components::Transaction};

use crate::transaction_service::{
    error::TransactionStorageError,
    storage::{
        models::{
            CompletedTransaction,
            InboundTransaction,
            OutboundTransaction,
            TxCancellationReason,
            WalletTransaction,
        },
        sqlite_db::{InboundTransactionSenderInfo, UnconfirmedTransactionInfo},
    },
};

const LOG_TARGET: &str = "wallet::transaction_service::database";

/// This trait defines the required behaviour that a storage backend must provide for the Transactionservice.
/// Data is passed to and from the backend via the [DbKey], [DbValue], and [DbValueKey] enums. If new data types are
/// required to be supported by the backends then these enums can be updated to reflect this requirement and the trait
/// will remain the same
pub trait TransactionBackend: Send + Sync + Clone {
    /// Retrieve the record associated with the provided DbKey
    fn fetch(&self, key: &DbKey) -> Result<Option<DbValue>, TransactionStorageError>;

    fn fetch_last_mined_transaction(&self) -> Result<Option<CompletedTransaction>, TransactionStorageError>;

    /// Light weight method to retrieve pertinent unconfirmed transactions info from completed transactions
    fn fetch_unconfirmed_transactions_info(&self) -> Result<Vec<UnconfirmedTransactionInfo>, TransactionStorageError>;

    fn get_transactions_to_be_broadcast(&self) -> Result<Vec<CompletedTransaction>, TransactionStorageError>;

    /// Check for presence of any form of cancelled transaction with this TxId
    fn fetch_any_cancelled_transaction(
        &self,
        tx_id: TxId,
    ) -> Result<Option<WalletTransaction>, TransactionStorageError>;

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
    /// Cancel Completed transaction, this will update the transaction status
    fn reject_completed_transaction(
        &self,
        tx_id: TxId,
        reason: TxCancellationReason,
    ) -> Result<(), TransactionStorageError>;
    /// Set cancellation on Pending transaction, this will update the transaction status
    fn set_pending_transaction_cancellation_status(
        &self,
        tx_id: TxId,
        cancelled: bool,
    ) -> Result<(), TransactionStorageError>;
    /// Search all pending transaction for the provided tx_id and if it exists return the public key of the counterparty
    fn get_pending_transaction_counterparty_address_by_tx_id(
        &self,
        tx_id: TxId,
    ) -> Result<TaijiAddress, TransactionStorageError>;
    /// Mark a pending transaction direct send attempt as a success
    fn mark_direct_send_success(&self, tx_id: TxId) -> Result<(), TransactionStorageError>;
    /// Cancel coinbase transactions at a specific block height
    fn cancel_coinbase_transactions_at_block_height(&self, block_height: u64) -> Result<(), TransactionStorageError>;
    /// Find coinbase transaction at a specific block height for a given amount
    fn find_coinbase_transaction_at_block_height(
        &self,
        block_height: u64,
        amount: MicroMinotaiji,
    ) -> Result<Option<CompletedTransaction>, TransactionStorageError>;
    /// Increment the send counter and timestamp of a transaction
    fn increment_send_count(&self, tx_id: TxId) -> Result<(), TransactionStorageError>;
    /// Update a transactions mined height. A transaction can either be mined as valid or mined as invalid
    /// A normal transaction can only be mined with valid = true,
    /// A coinbase transaction can either be mined as valid = true, meaning that it is the coinbase in that block
    /// or valid =false, meaning that the coinbase has been awarded to another tx, but this has been confirmed by blocks
    /// The mined height and block are used to determine reorgs
    fn update_mined_height(
        &self,
        tx_id: TxId,
        mined_height: u64,
        mined_in_block: BlockHash,
        mined_timestamp: u64,
        num_confirmations: u64,
        is_confirmed: bool,
        is_faux: bool,
    ) -> Result<(), TransactionStorageError>;
    /// Clears the mined block and height of a transaction
    fn set_transaction_as_unmined(&self, tx_id: TxId) -> Result<(), TransactionStorageError>;
    /// Reset optional 'mined height' and 'mined in block' fields to nothing
    fn mark_all_transactions_as_unvalidated(&self) -> Result<(), TransactionStorageError>;
    /// Light weight method to retrieve pertinent transaction sender info for all pending inbound transactions
    fn get_pending_inbound_transaction_sender_info(
        &self,
    ) -> Result<Vec<InboundTransactionSenderInfo>, TransactionStorageError>;
    fn fetch_imported_transactions(&self) -> Result<Vec<CompletedTransaction>, TransactionStorageError>;
    fn fetch_unconfirmed_faux_transactions(&self) -> Result<Vec<CompletedTransaction>, TransactionStorageError>;
    fn fetch_confirmed_faux_transactions_from_height(
        &self,
        height: u64,
    ) -> Result<Vec<CompletedTransaction>, TransactionStorageError>;
    fn abandon_coinbase_transaction(&self, tx_id: TxId) -> Result<(), TransactionStorageError>;
}

#[derive(Clone, PartialEq)]
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

impl fmt::Debug for DbKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        #[allow(clippy::enum_glob_use)]
        use DbKey::*;
        // Add in i64 representatives for easy debugging in sqlite. This should probably be removed at some point
        match self {
            PendingOutboundTransaction(tx_id) => {
                write!(
                    f,
                    "PendingOutboundTransaction ({}u64, {}i64)",
                    tx_id,
                    tx_id.as_i64_wrapped()
                )
            },
            PendingInboundTransaction(tx_id) => {
                write!(
                    f,
                    "PendingInboundTransaction ({}u64, {}i64)",
                    tx_id,
                    tx_id.as_i64_wrapped()
                )
            },
            CompletedTransaction(tx_id) => {
                write!(f, "CompletedTransaction ({}u64, {}i64)", tx_id, tx_id.as_i64_wrapped())
            },
            PendingOutboundTransactions => {
                write!(f, "PendingOutboundTransactions ")
            },
            PendingInboundTransactions => {
                write!(f, "PendingInboundTransactions")
            },
            CompletedTransactions => {
                write!(f, "CompletedTransactions ")
            },
            CancelledPendingOutboundTransactions => {
                write!(f, "CancelledPendingOutboundTransactions ")
            },
            CancelledPendingInboundTransactions => {
                write!(f, "CancelledPendingInboundTransactions")
            },
            CancelledCompletedTransactions => {
                write!(f, "CancelledCompletedTransactions")
            },
            CancelledPendingOutboundTransaction(tx_id) => {
                write!(
                    f,
                    "CancelledPendingOutboundTransaction ({}u64, {}i64)",
                    tx_id,
                    tx_id.as_i64_wrapped()
                )
            },
            CancelledPendingInboundTransaction(tx_id) => {
                write!(
                    f,
                    "CancelledPendingInboundTransaction ({}u64, {}i64)",
                    tx_id,
                    tx_id.as_i64_wrapped()
                )
            },
            AnyTransaction(tx_id) => {
                write!(f, "AnyTransaction ({}u64, {}i64)", tx_id, tx_id.as_i64_wrapped())
            },
        }
    }
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
pub struct TransactionDatabase<T> {
    db: Arc<T>,
}

impl<T> TransactionDatabase<T>
where T: TransactionBackend + 'static
{
    pub fn new(db: T) -> Self {
        Self { db: Arc::new(db) }
    }

    pub fn add_pending_inbound_transaction(
        &self,
        tx_id: TxId,
        inbound_tx: InboundTransaction,
    ) -> Result<(), TransactionStorageError> {
        self.db
            .write(WriteOperation::Insert(DbKeyValuePair::PendingInboundTransaction(
                tx_id,
                Box::new(inbound_tx),
            )))?;
        Ok(())
    }

    pub fn add_pending_outbound_transaction(
        &self,
        tx_id: TxId,
        outbound_tx: OutboundTransaction,
    ) -> Result<(), TransactionStorageError> {
        self.db
            .write(WriteOperation::Insert(DbKeyValuePair::PendingOutboundTransaction(
                tx_id,
                Box::new(outbound_tx),
            )))?;
        Ok(())
    }

    pub fn remove_pending_outbound_transaction(&self, tx_id: TxId) -> Result<(), TransactionStorageError> {
        self.db
            .write(WriteOperation::Remove(DbKey::PendingOutboundTransaction(tx_id)))?;
        Ok(())
    }

    /// Check if a transaction with the specified TxId exists in any of the collections
    pub fn transaction_exists(&self, tx_id: TxId) -> Result<bool, TransactionStorageError> {
        self.db.transaction_exists(tx_id)
    }

    pub fn insert_completed_transaction(
        &self,
        tx_id: TxId,
        transaction: CompletedTransaction,
    ) -> Result<Option<DbValue>, TransactionStorageError> {
        self.db
            .write(WriteOperation::Insert(DbKeyValuePair::CompletedTransaction(
                tx_id,
                Box::new(transaction),
            )))
    }

    pub fn get_pending_outbound_transaction(
        &self,
        tx_id: TxId,
    ) -> Result<OutboundTransaction, TransactionStorageError> {
        self.get_pending_outbound_transaction_by_cancelled(tx_id, false)
    }

    pub fn get_cancelled_pending_outbound_transaction(
        &self,
        tx_id: TxId,
    ) -> Result<OutboundTransaction, TransactionStorageError> {
        self.get_pending_outbound_transaction_by_cancelled(tx_id, true)
    }

    pub fn get_pending_outbound_transaction_by_cancelled(
        &self,
        tx_id: TxId,
        cancelled: bool,
    ) -> Result<OutboundTransaction, TransactionStorageError> {
        let key = if cancelled {
            DbKey::CancelledPendingOutboundTransaction(tx_id)
        } else {
            DbKey::PendingOutboundTransaction(tx_id)
        };
        let t = match self.db.fetch(&key) {
            Ok(None) => Err(TransactionStorageError::ValueNotFound(key)),
            Ok(Some(DbValue::PendingOutboundTransaction(pt))) => Ok(pt),
            Ok(Some(other)) => unexpected_result(key, other),
            Err(e) => log_error(key, e),
        }?;
        Ok(*t)
    }

    pub fn get_pending_inbound_transaction(&self, tx_id: TxId) -> Result<InboundTransaction, TransactionStorageError> {
        self.get_pending_inbound_transaction_by_cancelled(tx_id, false)
    }

    pub fn get_cancelled_pending_inbound_transaction(
        &self,
        tx_id: TxId,
    ) -> Result<InboundTransaction, TransactionStorageError> {
        self.get_pending_inbound_transaction_by_cancelled(tx_id, true)
    }

    pub fn get_pending_inbound_transaction_by_cancelled(
        &self,
        tx_id: TxId,
        cancelled: bool,
    ) -> Result<InboundTransaction, TransactionStorageError> {
        let key = if cancelled {
            DbKey::CancelledPendingInboundTransaction(tx_id)
        } else {
            DbKey::PendingInboundTransaction(tx_id)
        };
        let t = match self.db.fetch(&key) {
            Ok(None) => Err(TransactionStorageError::ValueNotFound(key)),
            Ok(Some(DbValue::PendingInboundTransaction(pt))) => Ok(pt),
            Ok(Some(other)) => unexpected_result(key, other),
            Err(e) => log_error(key, e),
        }?;
        Ok(*t)
    }

    pub fn get_completed_transaction(&self, tx_id: TxId) -> Result<CompletedTransaction, TransactionStorageError> {
        self.get_completed_transaction_by_cancelled(tx_id, false)
    }

    pub fn get_cancelled_completed_transaction(
        &self,
        tx_id: TxId,
    ) -> Result<CompletedTransaction, TransactionStorageError> {
        self.get_completed_transaction_by_cancelled(tx_id, true)
    }

    pub fn get_completed_transaction_by_cancelled(
        &self,
        tx_id: TxId,
        cancelled: bool,
    ) -> Result<CompletedTransaction, TransactionStorageError> {
        let key = DbKey::CompletedTransaction(tx_id);
        let t = match self.db.fetch(&DbKey::CompletedTransaction(tx_id)) {
            Ok(None) => Err(TransactionStorageError::ValueNotFound(key)),
            Ok(Some(DbValue::CompletedTransaction(pt))) => {
                if (pt.cancelled.is_some()) == cancelled {
                    Ok(pt)
                } else {
                    Err(TransactionStorageError::ValueNotFound(key))
                }
            },
            Ok(Some(other)) => unexpected_result(key, other),
            Err(e) => log_error(key, e),
        }?;
        Ok(*t)
    }

    pub fn get_imported_transactions(&self) -> Result<Vec<CompletedTransaction>, TransactionStorageError> {
        let t = self.db.fetch_imported_transactions()?;
        Ok(t)
    }

    pub fn get_unconfirmed_faux_transactions(&self) -> Result<Vec<CompletedTransaction>, TransactionStorageError> {
        let t = self.db.fetch_unconfirmed_faux_transactions()?;
        Ok(t)
    }

    pub fn get_confirmed_faux_transactions_from_height(
        &self,
        height: u64,
    ) -> Result<Vec<CompletedTransaction>, TransactionStorageError> {
        let t = self.db.fetch_confirmed_faux_transactions_from_height(height)?;
        Ok(t)
    }

    pub fn fetch_last_mined_transaction(&self) -> Result<Option<CompletedTransaction>, TransactionStorageError> {
        self.db.fetch_last_mined_transaction()
    }

    /// Light weight method to return completed but unconfirmed transactions that were not imported
    pub fn fetch_unconfirmed_transactions_info(
        &self,
    ) -> Result<Vec<UnconfirmedTransactionInfo>, TransactionStorageError> {
        self.db.fetch_unconfirmed_transactions_info()
    }

    /// This method returns all completed transactions that must be broadcast
    pub fn get_transactions_to_be_broadcast(&self) -> Result<Vec<CompletedTransaction>, TransactionStorageError> {
        self.db.get_transactions_to_be_broadcast()
    }

    pub fn get_completed_transaction_cancelled_or_not(
        &self,
        tx_id: TxId,
    ) -> Result<CompletedTransaction, TransactionStorageError> {
        let key = DbKey::CompletedTransaction(tx_id);
        let t = match self.db.fetch(&DbKey::CompletedTransaction(tx_id)) {
            Ok(None) => Err(TransactionStorageError::ValueNotFound(key)),
            Ok(Some(DbValue::CompletedTransaction(pt))) => Ok(pt),
            Ok(Some(other)) => unexpected_result(key, other),
            Err(e) => log_error(key, e),
        }?;
        Ok(*t)
    }

    pub fn get_pending_inbound_transactions(
        &self,
    ) -> Result<HashMap<TxId, InboundTransaction>, TransactionStorageError> {
        self.get_pending_inbound_transactions_by_cancelled(false)
    }

    pub fn get_cancelled_pending_inbound_transactions(
        &self,
    ) -> Result<HashMap<TxId, InboundTransaction>, TransactionStorageError> {
        self.get_pending_inbound_transactions_by_cancelled(true)
    }

    fn get_pending_inbound_transactions_by_cancelled(
        &self,
        cancelled: bool,
    ) -> Result<HashMap<TxId, InboundTransaction>, TransactionStorageError> {
        let key = if cancelled {
            DbKey::CancelledPendingInboundTransactions
        } else {
            DbKey::PendingInboundTransactions
        };

        let t = match self.db.fetch(&key) {
            Ok(None) => log_error(
                key,
                TransactionStorageError::UnexpectedResult(
                    "Could not retrieve pending inbound transactions".to_string(),
                ),
            ),
            Ok(Some(DbValue::PendingInboundTransactions(pt))) => Ok(pt),
            Ok(Some(other)) => unexpected_result(key, other),
            Err(e) => log_error(key, e),
        }?;
        Ok(t)
    }

    pub fn get_pending_outbound_transactions(
        &self,
    ) -> Result<HashMap<TxId, OutboundTransaction>, TransactionStorageError> {
        self.get_pending_outbound_transactions_by_cancelled(false)
    }

    pub fn get_cancelled_pending_outbound_transactions(
        &self,
    ) -> Result<HashMap<TxId, OutboundTransaction>, TransactionStorageError> {
        self.get_pending_outbound_transactions_by_cancelled(true)
    }

    fn get_pending_outbound_transactions_by_cancelled(
        &self,
        cancelled: bool,
    ) -> Result<HashMap<TxId, OutboundTransaction>, TransactionStorageError> {
        let key = if cancelled {
            DbKey::CancelledPendingOutboundTransactions
        } else {
            DbKey::PendingOutboundTransactions
        };

        let t = match self.db.fetch(&key) {
            Ok(None) => log_error(
                key,
                TransactionStorageError::UnexpectedResult(
                    "Could not retrieve pending outbound transactions".to_string(),
                ),
            ),
            Ok(Some(DbValue::PendingOutboundTransactions(pt))) => Ok(pt),
            Ok(Some(other)) => unexpected_result(key, other),
            Err(e) => log_error(key, e),
        }?;
        Ok(t)
    }

    pub fn get_pending_transaction_counterparty_address_by_tx_id(
        &mut self,
        tx_id: TxId,
    ) -> Result<TaijiAddress, TransactionStorageError> {
        let address = self.db.get_pending_transaction_counterparty_address_by_tx_id(tx_id)?;
        Ok(address)
    }

    pub fn get_completed_transactions(&self) -> Result<HashMap<TxId, CompletedTransaction>, TransactionStorageError> {
        self.get_completed_transactions_by_cancelled(false)
    }

    pub fn get_cancelled_completed_transactions(
        &self,
    ) -> Result<HashMap<TxId, CompletedTransaction>, TransactionStorageError> {
        self.get_completed_transactions_by_cancelled(true)
    }

    pub fn get_any_transaction(&self, tx_id: TxId) -> Result<Option<WalletTransaction>, TransactionStorageError> {
        let key = DbKey::AnyTransaction(tx_id);
        let t = match self.db.fetch(&key) {
            Ok(None) => Ok(None),
            Ok(Some(DbValue::WalletTransaction(pt))) => Ok(Some(*pt)),
            Ok(Some(other)) => unexpected_result(key, other),
            Err(e) => log_error(key, e),
        }?;
        Ok(t)
    }

    pub fn get_any_cancelled_transaction(
        &self,
        tx_id: TxId,
    ) -> Result<Option<WalletTransaction>, TransactionStorageError> {
        let tx = self.db.fetch_any_cancelled_transaction(tx_id)?;
        Ok(tx)
    }

    fn get_completed_transactions_by_cancelled(
        &self,
        cancelled: bool,
    ) -> Result<HashMap<TxId, CompletedTransaction>, TransactionStorageError> {
        let key = if cancelled {
            DbKey::CancelledCompletedTransactions
        } else {
            DbKey::CompletedTransactions
        };

        let t = match self.db.fetch(&key) {
            Ok(None) => log_error(
                key,
                TransactionStorageError::UnexpectedResult("Could not retrieve completed transactions".to_string()),
            ),
            Ok(Some(DbValue::CompletedTransactions(pt))) => Ok(pt),
            Ok(Some(other)) => unexpected_result(key, other),
            Err(e) => log_error(key, e),
        }?;
        Ok(t)
    }

    /// This method moves a `PendingOutboundTransaction` to the `CompleteTransaction` collection.
    pub fn complete_outbound_transaction(
        &self,
        tx_id: TxId,
        transaction: CompletedTransaction,
    ) -> Result<(), TransactionStorageError> {
        self.db.complete_outbound_transaction(tx_id, transaction)
    }

    /// This method moves a `PendingInboundTransaction` to the `CompleteTransaction` collection.
    pub fn complete_inbound_transaction(
        &self,
        tx_id: TxId,
        transaction: CompletedTransaction,
    ) -> Result<(), TransactionStorageError> {
        self.db.complete_inbound_transaction(tx_id, transaction)
    }

    pub fn reject_completed_transaction(
        &self,
        tx_id: TxId,
        reason: TxCancellationReason,
    ) -> Result<(), TransactionStorageError> {
        self.db.reject_completed_transaction(tx_id, reason)
    }

    pub fn cancel_pending_transaction(&self, tx_id: TxId) -> Result<(), TransactionStorageError> {
        self.db.set_pending_transaction_cancellation_status(tx_id, true)
    }

    pub fn uncancel_pending_transaction(&self, tx_id: TxId) -> Result<(), TransactionStorageError> {
        self.db.set_pending_transaction_cancellation_status(tx_id, false)
    }

    pub fn mark_direct_send_success(&self, tx_id: TxId) -> Result<(), TransactionStorageError> {
        self.db.mark_direct_send_success(tx_id)
    }

    /// Indicated that the specified completed transaction has been broadcast into the mempool
    pub fn broadcast_completed_transaction(&self, tx_id: TxId) -> Result<(), TransactionStorageError> {
        self.db.broadcast_completed_transaction(tx_id)
    }

    /// Faux transaction added to the database with imported status
    pub fn add_utxo_import_transaction_with_status(
        &self,
        tx_id: TxId,
        amount: MicroMinotaiji,
        source_address: TaijiAddress,
        comms_address: TaijiAddress,
        message: String,
        maturity: Option<u64>,
        import_status: ImportStatus,
        current_height: Option<u64>,
        mined_timestamp: Option<NaiveDateTime>,
    ) -> Result<(), TransactionStorageError> {
        let transaction = CompletedTransaction::new(
            tx_id,
            source_address,
            comms_address,
            amount,
            MicroMinotaiji::from(0),
            Transaction::new(
                Vec::new(),
                Vec::new(),
                Vec::new(),
                PrivateKey::default(),
                PrivateKey::default(),
            ),
            TransactionStatus::try_from(import_status)?,
            message,
            mined_timestamp.unwrap_or_else(|| Utc::now().naive_utc()),
            TransactionDirection::Inbound,
            maturity,
            current_height,
            mined_timestamp,
        );

        self.db
            .write(WriteOperation::Insert(DbKeyValuePair::CompletedTransaction(
                tx_id,
                Box::new(transaction),
            )))?;
        Ok(())
    }

    pub fn cancel_coinbase_transaction_at_block_height(
        &self,
        block_height: u64,
    ) -> Result<(), TransactionStorageError> {
        self.db.cancel_coinbase_transactions_at_block_height(block_height)
    }

    pub fn find_coinbase_transaction_at_block_height(
        &self,
        block_height: u64,
        amount: MicroMinotaiji,
    ) -> Result<Option<CompletedTransaction>, TransactionStorageError> {
        self.db.find_coinbase_transaction_at_block_height(block_height, amount)
    }

    pub fn increment_send_count(&self, tx_id: TxId) -> Result<(), TransactionStorageError> {
        self.db.increment_send_count(tx_id)
    }

    pub fn set_transaction_as_unmined(&self, tx_id: TxId) -> Result<(), TransactionStorageError> {
        self.db.set_transaction_as_unmined(tx_id)
    }

    pub fn mark_all_transactions_as_unvalidated(&self) -> Result<(), TransactionStorageError> {
        self.db.mark_all_transactions_as_unvalidated()
    }

    pub fn set_transaction_mined_height(
        &self,
        tx_id: TxId,
        mined_height: u64,
        mined_in_block: BlockHash,
        mined_timestamp: u64,
        num_confirmations: u64,
        is_confirmed: bool,
        is_faux: bool,
    ) -> Result<(), TransactionStorageError> {
        self.db.update_mined_height(
            tx_id,
            mined_height,
            mined_in_block,
            mined_timestamp,
            num_confirmations,
            is_confirmed,
            is_faux,
        )
    }

    pub fn get_pending_inbound_transaction_sender_info(
        &self,
    ) -> Result<Vec<InboundTransactionSenderInfo>, TransactionStorageError> {
        let t = match self.db.get_pending_inbound_transaction_sender_info() {
            Ok(v) => Ok(v),
            Err(e) => log_error(DbKey::PendingInboundTransactions, e),
        }?;
        Ok(t)
    }

    pub fn abandon_coinbase_transaction(&self, tx_id: TxId) -> Result<(), TransactionStorageError> {
        self.db.abandon_coinbase_transaction(tx_id)
    }
}

impl Display for DbKey {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            DbKey::PendingOutboundTransaction(_) => f.write_str("Pending Outbound Transaction"),
            DbKey::PendingInboundTransaction(_) => f.write_str("Pending Inbound Transaction"),

            DbKey::CompletedTransaction(_) => f.write_str("Completed Transaction"),
            DbKey::PendingOutboundTransactions => f.write_str("All Pending Outbound Transactions"),
            DbKey::PendingInboundTransactions => f.write_str("All Pending Inbound Transactions"),
            DbKey::CompletedTransactions => f.write_str("All Complete Transactions"),
            DbKey::CancelledPendingOutboundTransactions => f.write_str("All Cancelled Pending Inbound Transactions"),
            DbKey::CancelledPendingInboundTransactions => f.write_str("All Cancelled Pending Outbound Transactions"),
            DbKey::CancelledCompletedTransactions => f.write_str("All Cancelled Complete Transactions"),
            DbKey::CancelledPendingOutboundTransaction(_) => f.write_str("Cancelled Pending Outbound Transaction"),
            DbKey::CancelledPendingInboundTransaction(_) => f.write_str("Cancelled Pending Inbound Transaction"),
            DbKey::AnyTransaction(_) => f.write_str("Any Transaction"),
        }
    }
}

impl Display for DbValue {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            DbValue::PendingOutboundTransaction(_) => f.write_str("Pending Outbound Transaction"),
            DbValue::PendingInboundTransaction(_) => f.write_str("Pending Inbound Transaction"),
            DbValue::CompletedTransaction(_) => f.write_str("Completed Transaction"),
            DbValue::PendingOutboundTransactions(_) => f.write_str("All Pending Outbound Transactions"),
            DbValue::PendingInboundTransactions(_) => f.write_str("All Pending Inbound Transactions"),
            DbValue::CompletedTransactions(_) => f.write_str("All Complete Transactions"),
            DbValue::WalletTransaction(_) => f.write_str("Any Wallet Transaction"),
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
