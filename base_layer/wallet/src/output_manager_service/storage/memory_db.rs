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
        OutputManagerBackend,
        PendingTransactionOutputs,
        WriteOperation,
    },
    TxId,
};
use chrono::{Duration as ChronoDuration, Utc};
use std::{collections::HashMap, time::Duration};
use tari_core::transaction::UnblindedOutput;
/// This structure is an In-Memory database backend that implements the `OutputManagerBackend` trait and provides all
/// the functionality required by the trait.
pub struct OutputManagerMemoryDatabase {
    unspent_outputs: Vec<UnblindedOutput>,
    spent_outputs: Vec<UnblindedOutput>,
    pending_transactions: HashMap<TxId, PendingTransactionOutputs>,
}

impl OutputManagerMemoryDatabase {
    pub fn new() -> Self {
        Self {
            unspent_outputs: Vec::new(),
            spent_outputs: Vec::new(),
            pending_transactions: HashMap::new(),
        }
    }
}

impl OutputManagerBackend for OutputManagerMemoryDatabase {
    fn fetch(&self, key: &DbKey) -> Result<Option<DbValue>, OutputManagerStorageError> {
        let result = match key {
            DbKey::SpentOutput(k) => self
                .spent_outputs
                .iter()
                .find(|v| &v.spending_key == k)
                .map(|v| DbValue::SpentOutput(Box::new(v.clone()))),
            DbKey::UnspentOutput(k) => self
                .unspent_outputs
                .iter()
                .find(|v| &v.spending_key == k)
                .map(|v| DbValue::UnspentOutput(Box::new(v.clone()))),
            DbKey::PendingTransactionOutputs(tx_id) => self
                .pending_transactions
                .get(tx_id)
                .map(|v| DbValue::PendingTransactionOutputs(Box::new(v.clone()))),
            DbKey::UnspentOutputs => Some(DbValue::UnspentOutputs(Box::new(self.unspent_outputs.clone()))),
            DbKey::SpentOutputs => Some(DbValue::SpentOutputs(Box::new(self.spent_outputs.clone()))),
            DbKey::AllPendingTransactionOutputs => Some(DbValue::AllPendingTransactionOutputs(Box::new(
                self.pending_transactions.clone(),
            ))),
        };

        Ok(result)
    }

    fn contains(&self, key: &DbKey) -> Result<bool, OutputManagerStorageError> {
        Ok(match key {
            DbKey::SpentOutput(k) => self.spent_outputs.iter().any(|v| &v.spending_key == k),
            DbKey::UnspentOutput(k) => self.unspent_outputs.iter().any(|v| &v.spending_key == k),
            DbKey::PendingTransactionOutputs(tx_id) => self.pending_transactions.get(tx_id).is_some(),
            DbKey::UnspentOutputs => false,
            DbKey::SpentOutputs => false,
            DbKey::AllPendingTransactionOutputs => false,
        })
    }

    fn write(&mut self, op: WriteOperation) -> Result<Option<DbValue>, OutputManagerStorageError> {
        match op {
            WriteOperation::Insert(kvp) => match kvp {
                DbKeyValuePair::SpentOutput(k, o) => {
                    if self.contains(&DbKey::SpentOutput(k))? {
                        return Err(OutputManagerStorageError::DuplicateOutput);
                    }
                    self.spent_outputs.push(*o);
                },
                DbKeyValuePair::UnspentOutput(k, o) => {
                    if self.contains(&DbKey::UnspentOutput(k))? {
                        return Err(OutputManagerStorageError::DuplicateOutput);
                    }
                    self.unspent_outputs.push(*o);
                },
                DbKeyValuePair::PendingTransactionOutputs(t, p) => {
                    self.pending_transactions.insert(t, *p);
                },
            },
            WriteOperation::Remove(k) => match k {
                DbKey::SpentOutput(k) => match self.spent_outputs.iter().position(|v| v.spending_key == k) {
                    None => return Err(OutputManagerStorageError::ValueNotFound(DbKey::SpentOutput(k))),
                    Some(pos) => {
                        return Ok(Some(DbValue::SpentOutput(Box::new(self.spent_outputs.remove(pos)))));
                    },
                },
                DbKey::UnspentOutput(k) => match self.unspent_outputs.iter().position(|v| v.spending_key == k) {
                    None => return Err(OutputManagerStorageError::ValueNotFound(DbKey::UnspentOutput(k))),
                    Some(pos) => {
                        return Ok(Some(DbValue::UnspentOutput(Box::new(self.unspent_outputs.remove(pos)))));
                    },
                },
                DbKey::PendingTransactionOutputs(tx_id) => {
                    if let Some(p) = self.pending_transactions.remove(&tx_id) {
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
            },
        }
        Ok(None)
    }

    fn confirm_transaction(&mut self, tx_id: TxId) -> Result<(), OutputManagerStorageError> {
        let mut pending_tx =
            self.pending_transactions
                .remove(&tx_id)
                .ok_or(OutputManagerStorageError::ValueNotFound(
                    DbKey::PendingTransactionOutputs(tx_id.clone()),
                ))?;

        // Add Spent outputs
        for o in pending_tx.outputs_to_be_spent.drain(..) {
            self.spent_outputs.push(o)
        }

        // Add Unspent outputs
        for o in pending_tx.outputs_to_be_received.drain(..) {
            self.unspent_outputs.push(o);
        }

        Ok(())
    }

    fn encumber_outputs(
        &mut self,
        tx_id: TxId,
        outputs_to_send: Vec<UnblindedOutput>,
        change_output: Option<UnblindedOutput>,
    ) -> Result<(), OutputManagerStorageError>
    {
        let mut outputs_to_be_spent = Vec::new();
        for i in outputs_to_send {
            if let Some(pos) = self
                .unspent_outputs
                .iter()
                .position(|v| v.spending_key == i.spending_key)
            {
                outputs_to_be_spent.push(self.unspent_outputs.remove(pos));
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

        self.pending_transactions.insert(tx_id, pending_transaction);

        Ok(())
    }

    fn cancel_pending_transaction(&mut self, tx_id: TxId) -> Result<(), OutputManagerStorageError> {
        let mut pending_tx =
            self.pending_transactions
                .remove(&tx_id)
                .ok_or(OutputManagerStorageError::ValueNotFound(
                    DbKey::PendingTransactionOutputs(tx_id.clone()),
                ))?;
        for o in pending_tx.outputs_to_be_spent.drain(..) {
            self.unspent_outputs.push(o);
        }

        Ok(())
    }

    fn timeout_pending_transactions(&mut self, period: Duration) -> Result<(), OutputManagerStorageError> {
        let mut transactions_to_be_cancelled = Vec::new();
        for (tx_id, pt) in self.pending_transactions.iter() {
            if pt.timestamp + ChronoDuration::from_std(period)? < Utc::now().naive_utc() {
                transactions_to_be_cancelled.push(tx_id.clone());
            }
        }

        for t in transactions_to_be_cancelled {
            self.cancel_pending_transaction(t.clone())?;
        }

        Ok(())
    }
}
