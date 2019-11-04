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
            WriteOperation,
        },
    },
};
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

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
            DbKey::PendingOutboundTransaction(t) => db
                .pending_outbound_transactions
                .get(t)
                .map(|v| DbValue::PendingOutboundTransaction(Box::new(v.clone()))),
            DbKey::PendingInboundTransaction(t) => db
                .pending_inbound_transactions
                .get(t)
                .map(|v| DbValue::PendingInboundTransaction(Box::new(v.clone()))),
            DbKey::CompletedTransaction(t) => db
                .completed_transactions
                .get(t)
                .map(|v| DbValue::CompletedTransaction(Box::new(v.clone()))),
            DbKey::PendingOutboundTransactions => Some(DbValue::PendingOutboundTransactions(Box::new(
                db.pending_outbound_transactions.clone(),
            ))),
            DbKey::PendingInboundTransactions => Some(DbValue::PendingInboundTransactions(Box::new(
                db.pending_inbound_transactions.clone(),
            ))),
            DbKey::CompletedTransactions => Some(DbValue::CompletedTransactions(Box::new(
                db.completed_transactions.clone(),
            ))),
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
        };

        Ok(result)
    }

    fn write(&mut self, op: WriteOperation) -> Result<Option<DbValue>, TransactionStorageError> {
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
            },
        }

        Ok(None)
    }

    fn complete_transaction(
        &mut self,
        tx_id: TxId,
        transaction: CompletedTransaction,
    ) -> Result<(), TransactionStorageError>
    {
        let mut db = acquire_write_lock!(self.db);
        let _ = db
            .pending_outbound_transactions
            .remove(&tx_id)
            .ok_or(TransactionStorageError::ValueNotFound(
                DbKey::PendingOutboundTransaction(tx_id.clone()),
            ))?;

        db.completed_transactions.insert(tx_id, transaction);

        Ok(())
    }
}
