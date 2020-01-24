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
    storage::database::{
        DbKey,
        DbKeyValuePair,
        DbValue,
        KeyManagerState,
        OutputManagerBackend,
        PendingTransactionOutputs,
        WriteOperation,
    },
    TxId,
};
use chrono::{Duration as ChronoDuration, Utc};
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::Duration,
};
use tari_core::transactions::transaction::UnblindedOutput;

/// This structure is an In-Memory database backend that implements the `OutputManagerBackend` trait and provides all
/// the functionality required by the trait.
pub struct InnerDatabase {
    unspent_outputs: Vec<UnblindedOutput>,
    spent_outputs: Vec<UnblindedOutput>,
    pending_transactions: HashMap<TxId, PendingTransactionOutputs>,
    key_manager_state: Option<KeyManagerState>,
}

impl InnerDatabase {
    pub fn new() -> Self {
        Self {
            unspent_outputs: Vec::new(),
            spent_outputs: Vec::new(),
            pending_transactions: HashMap::new(),
            key_manager_state: None,
        }
    }
}

#[derive(Clone)]
pub struct OutputManagerMemoryDatabase {
    db: Arc<RwLock<InnerDatabase>>,
}

impl OutputManagerMemoryDatabase {
    pub fn new() -> Self {
        Self {
            db: Arc::new(RwLock::new(InnerDatabase::new())),
        }
    }
}

impl OutputManagerBackend for OutputManagerMemoryDatabase {
    fn fetch(&self, key: &DbKey) -> Result<Option<DbValue>, OutputManagerStorageError> {
        let db = acquire_read_lock!(self.db);
        let result = match key {
            DbKey::SpentOutput(k) => db
                .spent_outputs
                .iter()
                .find(|v| &v.spending_key == k)
                .map(|v| DbValue::SpentOutput(Box::new(v.clone()))),
            DbKey::UnspentOutput(k) => db
                .unspent_outputs
                .iter()
                .find(|v| &v.spending_key == k)
                .map(|v| DbValue::UnspentOutput(Box::new(v.clone()))),
            DbKey::PendingTransactionOutputs(tx_id) => db
                .pending_transactions
                .get(tx_id)
                .map(|v| DbValue::PendingTransactionOutputs(Box::new(v.clone()))),
            DbKey::UnspentOutputs => Some(DbValue::UnspentOutputs(db.unspent_outputs.clone())),
            DbKey::SpentOutputs => Some(DbValue::SpentOutputs(db.spent_outputs.clone())),
            DbKey::AllPendingTransactionOutputs => {
                Some(DbValue::AllPendingTransactionOutputs(db.pending_transactions.clone()))
            },
            DbKey::KeyManagerState => db
                .key_manager_state
                .as_ref()
                .map(|km| DbValue::KeyManagerState(km.clone())),
        };

        Ok(result)
    }

    fn write(&self, op: WriteOperation) -> Result<Option<DbValue>, OutputManagerStorageError> {
        let mut db = acquire_write_lock!(self.db);
        match op {
            WriteOperation::Insert(kvp) => match kvp {
                DbKeyValuePair::SpentOutput(k, o) => {
                    if db.spent_outputs.iter().any(|v| v.spending_key == k) ||
                        db.unspent_outputs.iter().any(|v| v.spending_key == k)
                    {
                        return Err(OutputManagerStorageError::DuplicateOutput);
                    }
                    db.spent_outputs.push(*o);
                },
                DbKeyValuePair::UnspentOutput(k, o) => {
                    if db.unspent_outputs.iter().any(|v| v.spending_key == k) ||
                        db.spent_outputs.iter().any(|v| v.spending_key == k)
                    {
                        return Err(OutputManagerStorageError::DuplicateOutput);
                    }
                    db.unspent_outputs.push(*o);
                },
                DbKeyValuePair::PendingTransactionOutputs(t, p) => {
                    db.pending_transactions.insert(t, *p);
                },
                DbKeyValuePair::KeyManagerState(km) => db.key_manager_state = Some(km),
            },
            WriteOperation::Remove(k) => match k {
                DbKey::SpentOutput(k) => match db.spent_outputs.iter().position(|v| v.spending_key == k) {
                    None => return Err(OutputManagerStorageError::ValueNotFound(DbKey::SpentOutput(k))),
                    Some(pos) => {
                        return Ok(Some(DbValue::SpentOutput(Box::new(db.spent_outputs.remove(pos)))));
                    },
                },
                DbKey::UnspentOutput(k) => match db.unspent_outputs.iter().position(|v| v.spending_key == k) {
                    None => return Err(OutputManagerStorageError::ValueNotFound(DbKey::UnspentOutput(k))),
                    Some(pos) => {
                        return Ok(Some(DbValue::UnspentOutput(Box::new(db.unspent_outputs.remove(pos)))));
                    },
                },
                DbKey::PendingTransactionOutputs(tx_id) => {
                    if let Some(p) = db.pending_transactions.remove(&tx_id) {
                        return Ok(Some(DbValue::PendingTransactionOutputs(Box::new(p))));
                    } else {
                        return Err(OutputManagerStorageError::ValueNotFound(
                            DbKey::PendingTransactionOutputs(tx_id),
                        ));
                    }
                },
                DbKey::UnspentOutputs => return Err(OutputManagerStorageError::OperationNotSupported),
                DbKey::SpentOutputs => return Err(OutputManagerStorageError::OperationNotSupported),
                DbKey::AllPendingTransactionOutputs => return Err(OutputManagerStorageError::OperationNotSupported),
                DbKey::KeyManagerState => return Err(OutputManagerStorageError::OperationNotSupported),
            },
        }
        Ok(None)
    }

    fn confirm_transaction(&self, tx_id: TxId) -> Result<(), OutputManagerStorageError> {
        let mut db = acquire_write_lock!(self.db);
        let mut pending_tx = db
            .pending_transactions
            .remove(&tx_id)
            .ok_or(OutputManagerStorageError::ValueNotFound(
                DbKey::PendingTransactionOutputs(tx_id.clone()),
            ))?;

        // Add Spent outputs
        for o in pending_tx.outputs_to_be_spent.drain(..) {
            db.spent_outputs.push(o)
        }

        // Add Unspent outputs
        for o in pending_tx.outputs_to_be_received.drain(..) {
            db.unspent_outputs.push(o);
        }

        Ok(())
    }

    fn encumber_outputs(
        &self,
        tx_id: TxId,
        outputs_to_send: &Vec<UnblindedOutput>,
        change_output: Option<UnblindedOutput>,
    ) -> Result<(), OutputManagerStorageError>
    {
        let mut db = acquire_write_lock!(self.db);
        let mut outputs_to_be_spent = Vec::new();
        for i in outputs_to_send {
            if let Some(pos) = db.unspent_outputs.iter().position(|v| v.spending_key == i.spending_key) {
                outputs_to_be_spent.push(db.unspent_outputs.remove(pos));
            } else {
                return Err(OutputManagerStorageError::ValuesNotFound);
            }
        }

        let mut pending_transaction = PendingTransactionOutputs {
            tx_id: tx_id.clone(),
            outputs_to_be_spent,
            outputs_to_be_received: Vec::new(),
            timestamp: Utc::now().naive_utc(),
        };

        if let Some(co) = change_output {
            pending_transaction.outputs_to_be_received.push(co);
        }

        db.pending_transactions.insert(tx_id, pending_transaction);

        Ok(())
    }

    fn cancel_pending_transaction(&self, tx_id: TxId) -> Result<(), OutputManagerStorageError> {
        let mut db = acquire_write_lock!(self.db);
        let mut pending_tx = db
            .pending_transactions
            .remove(&tx_id)
            .ok_or(OutputManagerStorageError::ValueNotFound(
                DbKey::PendingTransactionOutputs(tx_id.clone()),
            ))?;
        for o in pending_tx.outputs_to_be_spent.drain(..) {
            db.unspent_outputs.push(o);
        }

        Ok(())
    }

    fn timeout_pending_transactions(&self, period: Duration) -> Result<(), OutputManagerStorageError> {
        let db = acquire_write_lock!(self.db);
        let mut transactions_to_be_cancelled = Vec::new();
        for (tx_id, pt) in db.pending_transactions.iter() {
            if pt.timestamp + ChronoDuration::from_std(period)? < Utc::now().naive_utc() {
                transactions_to_be_cancelled.push(tx_id.clone());
            }
        }
        drop(db);
        for t in transactions_to_be_cancelled {
            self.cancel_pending_transaction(t.clone())?;
        }

        Ok(())
    }

    fn increment_key_index(&self) -> Result<(), OutputManagerStorageError> {
        let mut db = acquire_write_lock!(self.db);

        if db.key_manager_state.is_none() {
            return Err(OutputManagerStorageError::KeyManagerNotInitialized);
        }
        db.key_manager_state = db.key_manager_state.clone().map(|mut state| {
            state.primary_key_index += 1;
            state
        });

        Ok(())
    }
}
