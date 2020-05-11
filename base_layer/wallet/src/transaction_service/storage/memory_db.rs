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
        storage::database::{
            CompletedTransaction,
            DbKey,
            DbKeyValuePair,
            DbValue,
            InboundTransaction,
            OutboundTransaction,
            TransactionBackend,
            TransactionStatus,
            WriteOperation,
        },
    },
};
#[cfg(feature = "test_harness")]
use chrono::NaiveDateTime;
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};
use tari_comms::types::CommsPublicKey;

#[derive(Default)]
struct InnerDatabase {
    pending_outbound_transactions: HashMap<TxId, OutboundTransaction>,
    pending_inbound_transactions: HashMap<TxId, InboundTransaction>,
    completed_transactions: HashMap<TxId, CompletedTransaction>,
}

impl InnerDatabase {
    pub fn new() -> Self {
        Self {
            pending_outbound_transactions: HashMap::new(),
            pending_inbound_transactions: HashMap::new(),
            completed_transactions: HashMap::new(),
        }
    }
}

#[derive(Clone, Default)]
pub struct TransactionMemoryDatabase {
    db: Arc<RwLock<InnerDatabase>>,
}

impl TransactionMemoryDatabase {
    pub fn new() -> Self {
        Self {
            db: Arc::new(RwLock::new(InnerDatabase::new())),
        }
    }
}

impl TransactionBackend for TransactionMemoryDatabase {
    fn fetch(&self, key: &DbKey) -> Result<Option<DbValue>, TransactionStorageError> {
        let db = acquire_read_lock!(self.db);
        let result = match key {
            DbKey::PendingOutboundTransaction(t) => {
                let mut result = None;
                if let Some(v) = db.pending_outbound_transactions.get(t) {
                    if !v.cancelled {
                        result = Some(DbValue::PendingOutboundTransaction(Box::new(v.clone())));
                    }
                }
                result
            },
            DbKey::PendingInboundTransaction(t) => {
                let mut result = None;
                if let Some(v) = db.pending_inbound_transactions.get(t) {
                    if !v.cancelled {
                        result = Some(DbValue::PendingInboundTransaction(Box::new(v.clone())));
                    }
                }
                result
            },
            DbKey::CompletedTransaction(t) => {
                let mut result = None;
                if let Some(v) = db.completed_transactions.get(t) {
                    if !v.cancelled {
                        result = Some(DbValue::CompletedTransaction(Box::new(v.clone())));
                    }
                }
                result
            },
            DbKey::PendingOutboundTransactions => {
                // Filter out cancelled transactions
                let mut result = HashMap::new();
                for (k, v) in db.pending_outbound_transactions.iter() {
                    if !v.cancelled {
                        result.insert(k.clone(), v.clone());
                    }
                }
                Some(DbValue::PendingOutboundTransactions(result))
            },
            DbKey::PendingInboundTransactions => {
                // Filter out cancelled transactions
                let mut result = HashMap::new();
                for (k, v) in db.pending_inbound_transactions.iter() {
                    if !v.cancelled {
                        result.insert(k.clone(), v.clone());
                    }
                }
                Some(DbValue::PendingInboundTransactions(result))
            },
            DbKey::CompletedTransactions => {
                // Filter out cancelled transactions
                let mut result = HashMap::new();
                for (k, v) in db.completed_transactions.iter() {
                    if !v.cancelled {
                        result.insert(k.clone(), v.clone());
                    }
                }
                Some(DbValue::CompletedTransactions(result))
            },
            DbKey::CancelledPendingOutboundTransactions => {
                // Filter out cancelled transactions
                let mut result = HashMap::new();
                for (k, v) in db.pending_outbound_transactions.iter() {
                    if v.cancelled {
                        result.insert(k.clone(), v.clone());
                    }
                }
                Some(DbValue::PendingOutboundTransactions(result))
            },
            DbKey::CancelledPendingInboundTransactions => {
                // Filter out cancelled transactions
                let mut result = HashMap::new();
                for (k, v) in db.pending_inbound_transactions.iter() {
                    if v.cancelled {
                        result.insert(k.clone(), v.clone());
                    }
                }
                Some(DbValue::PendingInboundTransactions(result))
            },
            DbKey::CancelledCompletedTransactions => {
                let mut result = HashMap::new();
                for (k, v) in db.completed_transactions.iter() {
                    if v.cancelled {
                        result.insert(k.clone(), v.clone());
                    }
                }
                Some(DbValue::CompletedTransactions(result))
            },
        };

        Ok(result)
    }

    fn contains(&self, key: &DbKey) -> Result<bool, TransactionStorageError> {
        let db = acquire_read_lock!(self.db);
        let result = match key {
            DbKey::PendingOutboundTransaction(k) => db.pending_outbound_transactions.contains_key(k),
            DbKey::PendingInboundTransaction(k) => db.pending_inbound_transactions.contains_key(k),
            DbKey::CompletedTransaction(k) => db.completed_transactions.contains_key(k),
            DbKey::PendingOutboundTransactions => false,
            DbKey::PendingInboundTransactions => false,
            DbKey::CompletedTransactions => false,
            DbKey::CancelledPendingOutboundTransactions => false,
            DbKey::CancelledPendingInboundTransactions => false,
            DbKey::CancelledCompletedTransactions => false,
        };

        Ok(result)
    }

    fn write(&self, op: WriteOperation) -> Result<Option<DbValue>, TransactionStorageError> {
        let mut db = acquire_write_lock!(self.db);
        match op {
            WriteOperation::Insert(kvp) => match kvp {
                DbKeyValuePair::PendingOutboundTransaction(k, v) => {
                    if db.pending_outbound_transactions.contains_key(&k) {
                        return Err(TransactionStorageError::DuplicateOutput);
                    }
                    db.pending_outbound_transactions.insert(k, *v);
                },
                DbKeyValuePair::PendingInboundTransaction(k, v) => {
                    if db.pending_inbound_transactions.contains_key(&k) {
                        return Err(TransactionStorageError::DuplicateOutput);
                    }
                    db.pending_inbound_transactions.insert(k, *v);
                },
                DbKeyValuePair::CompletedTransaction(k, v) => {
                    if db.completed_transactions.contains_key(&k) {
                        return Err(TransactionStorageError::DuplicateOutput);
                    }
                    db.completed_transactions.insert(k, *v);
                },
            },
            WriteOperation::Remove(k) => match k {
                DbKey::PendingOutboundTransaction(k) => {
                    if let Some(p) = db.pending_outbound_transactions.remove(&k) {
                        return Ok(Some(DbValue::PendingOutboundTransaction(Box::new(p))));
                    } else {
                        return Err(TransactionStorageError::ValueNotFound(
                            DbKey::PendingOutboundTransaction(k),
                        ));
                    }
                },
                DbKey::PendingInboundTransaction(k) => {
                    if let Some(p) = db.pending_inbound_transactions.remove(&k) {
                        return Ok(Some(DbValue::PendingInboundTransaction(Box::new(p))));
                    } else {
                        return Err(TransactionStorageError::ValueNotFound(
                            DbKey::PendingInboundTransaction(k),
                        ));
                    }
                },
                DbKey::CompletedTransaction(k) => {
                    if let Some(p) = db.completed_transactions.remove(&k) {
                        return Ok(Some(DbValue::CompletedTransaction(Box::new(p))));
                    } else {
                        return Err(TransactionStorageError::ValueNotFound(DbKey::CompletedTransaction(k)));
                    }
                },
                DbKey::PendingInboundTransactions => return Err(TransactionStorageError::OperationNotSupported),
                DbKey::PendingOutboundTransactions => return Err(TransactionStorageError::OperationNotSupported),
                DbKey::CompletedTransactions => return Err(TransactionStorageError::OperationNotSupported),
                DbKey::CancelledPendingOutboundTransactions => {
                    return Err(TransactionStorageError::OperationNotSupported)
                },
                DbKey::CancelledPendingInboundTransactions => {
                    return Err(TransactionStorageError::OperationNotSupported)
                },
                DbKey::CancelledCompletedTransactions => return Err(TransactionStorageError::OperationNotSupported),
            },
        }

        Ok(None)
    }

    fn transaction_exists(&self, tx_id: u64) -> Result<bool, TransactionStorageError> {
        let db = acquire_read_lock!(self.db);

        Ok(db.pending_outbound_transactions.contains_key(&tx_id) ||
            db.pending_inbound_transactions.contains_key(&tx_id) ||
            db.completed_transactions.contains_key(&tx_id))
    }

    fn get_pending_transaction_counterparty_pub_key_by_tx_id(
        &self,
        tx_id: u64,
    ) -> Result<CommsPublicKey, TransactionStorageError>
    {
        let db = acquire_read_lock!(self.db);

        if let Some(pending_inbound_tx) = db.pending_inbound_transactions.get(&tx_id) {
            return Ok(pending_inbound_tx.source_public_key.clone());
        } else {
            if let Some(pending_outbound_tx) = db.pending_outbound_transactions.get(&tx_id) {
                return Ok(pending_outbound_tx.destination_public_key.clone());
            }
        }
        Err(TransactionStorageError::ValuesNotFound)
    }

    fn complete_outbound_transaction(
        &self,
        tx_id: TxId,
        transaction: CompletedTransaction,
    ) -> Result<(), TransactionStorageError>
    {
        let mut db = acquire_write_lock!(self.db);

        if db.completed_transactions.contains_key(&tx_id) {
            return Err(TransactionStorageError::TransactionAlreadyExists);
        }

        let _ = db
            .pending_outbound_transactions
            .remove(&tx_id)
            .ok_or_else(|| TransactionStorageError::ValueNotFound(DbKey::PendingOutboundTransaction(tx_id)))?;

        db.completed_transactions.insert(tx_id, transaction);

        Ok(())
    }

    fn complete_inbound_transaction(
        &self,
        tx_id: TxId,
        transaction: CompletedTransaction,
    ) -> Result<(), TransactionStorageError>
    {
        let mut db = acquire_write_lock!(self.db);

        if db.completed_transactions.contains_key(&tx_id) {
            return Err(TransactionStorageError::TransactionAlreadyExists);
        }
        let _ = db
            .pending_inbound_transactions
            .remove(&tx_id)
            .ok_or_else(|| TransactionStorageError::ValueNotFound(DbKey::PendingInboundTransaction(tx_id)))?;

        db.completed_transactions.insert(tx_id, transaction);
        Ok(())
    }

    fn broadcast_completed_transaction(&self, tx_id: TxId) -> Result<(), TransactionStorageError> {
        let mut db = acquire_write_lock!(self.db);

        let mut completed_tx = db
            .completed_transactions
            .get_mut(&tx_id)
            .ok_or_else(|| TransactionStorageError::ValueNotFound(DbKey::CompletedTransaction(tx_id)))?;

        if completed_tx.status == TransactionStatus::Completed {
            completed_tx.status = TransactionStatus::Broadcast;
        }

        Ok(())
    }

    fn mine_completed_transaction(&self, tx_id: TxId) -> Result<(), TransactionStorageError> {
        let mut db = acquire_write_lock!(self.db);

        let mut completed_tx = db
            .completed_transactions
            .get_mut(&tx_id)
            .ok_or_else(|| TransactionStorageError::ValueNotFound(DbKey::CompletedTransaction(tx_id)))?;

        if completed_tx.cancelled {
            return Err(TransactionStorageError::ValueNotFound(DbKey::CompletedTransaction(
                tx_id,
            )));
        }

        completed_tx.status = TransactionStatus::Mined;

        Ok(())
    }

    fn cancel_completed_transaction(&self, tx_id: TxId) -> Result<(), TransactionStorageError> {
        let mut db = acquire_write_lock!(self.db);

        let mut completed_tx = db
            .completed_transactions
            .get_mut(&tx_id)
            .ok_or_else(|| TransactionStorageError::ValueNotFound(DbKey::CompletedTransaction(tx_id)))?;

        completed_tx.cancelled = true;

        Ok(())
    }

    fn cancel_pending_transaction(&self, tx_id: u64) -> Result<(), TransactionStorageError> {
        let mut db = acquire_write_lock!(self.db);

        if db.pending_inbound_transactions.contains_key(&tx_id) {
            if let Some(inbound) = db.pending_inbound_transactions.get_mut(&tx_id) {
                inbound.cancelled = true;
            }
        } else if db.pending_outbound_transactions.contains_key(&tx_id) {
            if let Some(outbound) = db.pending_outbound_transactions.get_mut(&tx_id) {
                outbound.cancelled = true;
            }
        } else {
            return Err(TransactionStorageError::ValuesNotFound);
        }
        Ok(())
    }

    #[cfg(feature = "test_harness")]
    fn update_completed_transaction_timestamp(
        &self,
        tx_id: u64,
        timestamp: NaiveDateTime,
    ) -> Result<(), TransactionStorageError>
    {
        let mut db = acquire_write_lock!(self.db);

        if let Some(tx) = db.completed_transactions.get_mut(&tx_id) {
            tx.timestamp = timestamp;
        }

        Ok(())
    }
}
