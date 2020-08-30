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
    storage::{
        database::{
            DbKey,
            DbKeyValuePair,
            DbValue,
            KeyManagerState,
            OutputManagerBackend,
            PendingTransactionOutputs,
            WriteOperation,
        },
        models::DbUnblindedOutput,
    },
    TxId,
};
use aes_gcm::Aes256Gcm;
use chrono::{Duration as ChronoDuration, Utc};
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::Duration,
};
use tari_core::transactions::types::{Commitment, CryptoFactories};

/// This structure is an In-Memory database backend that implements the `OutputManagerBackend` trait and provides all
/// the functionality required by the trait.
#[derive(Default)]
pub struct InnerDatabase {
    unspent_outputs: Vec<DbOutput>,
    spent_outputs: Vec<DbOutput>,
    invalid_outputs: Vec<DbOutput>,
    pending_transactions: HashMap<TxId, PendingTransactionOutputs>,
    short_term_pending_transactions: HashMap<TxId, PendingTransactionOutputs>,
    key_manager_state: Option<KeyManagerState>,
}

impl InnerDatabase {
    pub fn new() -> Self {
        Self {
            unspent_outputs: Vec::new(),
            spent_outputs: Vec::new(),
            invalid_outputs: Vec::new(),
            pending_transactions: HashMap::new(),
            short_term_pending_transactions: Default::default(),
            key_manager_state: None,
        }
    }
}

#[derive(Clone, Default)]
pub struct OutputManagerMemoryDatabase {
    db: Arc<RwLock<InnerDatabase>>,
    factories: CryptoFactories,
}

impl OutputManagerMemoryDatabase {
    pub fn new() -> Self {
        Self {
            db: Arc::new(RwLock::new(InnerDatabase::new())),
            factories: CryptoFactories::default(),
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
                .find(|v| v.output.unblinded_output.spending_key() == k)
                .map(|v| DbValue::SpentOutput(Box::new(DbUnblindedOutput::from((*v).clone())))),
            DbKey::UnspentOutput(k) => db
                .unspent_outputs
                .iter()
                .find(|v| v.output.unblinded_output.spending_key() == k)
                .map(|v| DbValue::UnspentOutput(Box::new(DbUnblindedOutput::from((*v).clone())))),
            DbKey::PendingTransactionOutputs(tx_id) => {
                let mut result = db.pending_transactions.get(tx_id);
                if result.is_none() {
                    result = db.short_term_pending_transactions.get(&tx_id);
                }
                result.map(|v| DbValue::PendingTransactionOutputs(Box::new(v.clone())))
            },
            DbKey::UnspentOutputs => Some(DbValue::UnspentOutputs(
                db.unspent_outputs
                    .iter()
                    .map(|o| DbUnblindedOutput::from((*o).clone()))
                    .collect(),
            )),
            DbKey::SpentOutputs => Some(DbValue::SpentOutputs(
                db.spent_outputs
                    .iter()
                    .map(|o| DbUnblindedOutput::from((*o).clone()))
                    .collect(),
            )),
            DbKey::TimeLockedUnspentOutputs(tip) => Some(DbValue::UnspentOutputs(
                db.unspent_outputs
                    .iter()
                    .filter_map(|o| {
                        if (*o).output.unblinded_output.features().maturity > *tip {
                            Some(DbUnblindedOutput::from((*o).clone()))
                        } else {
                            None
                        }
                    })
                    .collect(),
            )),
            DbKey::AllPendingTransactionOutputs => {
                let mut pending_tx_outputs = db.pending_transactions.clone();
                for (k, v) in db.short_term_pending_transactions.iter() {
                    pending_tx_outputs.insert(*k, v.clone());
                }
                Some(DbValue::AllPendingTransactionOutputs(pending_tx_outputs))
            },
            DbKey::KeyManagerState => db
                .key_manager_state
                .as_ref()
                .map(|km| DbValue::KeyManagerState(km.clone())),
            DbKey::InvalidOutputs => Some(DbValue::InvalidOutputs(
                db.invalid_outputs
                    .iter()
                    .map(|o| DbUnblindedOutput::from((*o).clone()))
                    .collect(),
            )),
        };

        Ok(result)
    }

    fn write(&self, op: WriteOperation) -> Result<Option<DbValue>, OutputManagerStorageError> {
        let mut db = acquire_write_lock!(self.db);
        match op {
            WriteOperation::Insert(kvp) => match kvp {
                DbKeyValuePair::SpentOutput(k, o) => {
                    if db
                        .spent_outputs
                        .iter()
                        .any(|v| v.output.unblinded_output.spending_key() == &k) ||
                        db.unspent_outputs
                            .iter()
                            .any(|v| v.output.unblinded_output.spending_key() == &k)
                    {
                        return Err(OutputManagerStorageError::DuplicateOutput);
                    }
                    db.spent_outputs.push(DbOutput::from(*o));
                },
                DbKeyValuePair::UnspentOutput(k, o) => {
                    if db
                        .unspent_outputs
                        .iter()
                        .any(|v| v.output.unblinded_output.spending_key() == &k) ||
                        db.spent_outputs
                            .iter()
                            .any(|v| v.output.unblinded_output.spending_key() == &k)
                    {
                        return Err(OutputManagerStorageError::DuplicateOutput);
                    }
                    db.unspent_outputs.push(DbOutput::from(*o));
                },
                DbKeyValuePair::PendingTransactionOutputs(t, p) => {
                    db.short_term_pending_transactions.insert(t, *p);
                },
                DbKeyValuePair::KeyManagerState(km) => db.key_manager_state = Some(km),
            },
            WriteOperation::Remove(k) => match k {
                DbKey::SpentOutput(k) => match db
                    .spent_outputs
                    .iter()
                    .position(|v| v.output.unblinded_output.spending_key() == &k)
                {
                    None => return Err(OutputManagerStorageError::ValueNotFound(DbKey::SpentOutput(k))),
                    Some(pos) => {
                        return Ok(Some(DbValue::SpentOutput(Box::new(DbUnblindedOutput::from(
                            db.spent_outputs.remove(pos),
                        )))));
                    },
                },
                DbKey::UnspentOutput(k) => match db
                    .unspent_outputs
                    .iter()
                    .position(|v| v.output.unblinded_output.spending_key() == &k)
                {
                    None => return Err(OutputManagerStorageError::ValueNotFound(DbKey::UnspentOutput(k))),
                    Some(pos) => {
                        return Ok(Some(DbValue::UnspentOutput(Box::new(DbUnblindedOutput::from(
                            db.unspent_outputs.remove(pos),
                        )))));
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
                DbKey::InvalidOutputs => return Err(OutputManagerStorageError::OperationNotSupported),
                DbKey::TimeLockedUnspentOutputs(_) => return Err(OutputManagerStorageError::OperationNotSupported),
            },
        }
        Ok(None)
    }

    fn confirm_transaction(&self, tx_id: TxId) -> Result<(), OutputManagerStorageError> {
        let mut db = acquire_write_lock!(self.db);

        let mut pending_tx = db.pending_transactions.remove(&tx_id);
        if pending_tx.is_none() {
            pending_tx = db.short_term_pending_transactions.remove(&tx_id);
        }

        let mut pending_tx = pending_tx
            .ok_or_else(|| OutputManagerStorageError::ValueNotFound(DbKey::PendingTransactionOutputs(tx_id)))?;

        // Add Spent outputs
        for o in pending_tx.outputs_to_be_spent.drain(..) {
            db.spent_outputs.push(DbOutput::new(tx_id, o))
        }

        // Add Unspent outputs
        for o in pending_tx.outputs_to_be_received.drain(..) {
            db.unspent_outputs.push(DbOutput::new(tx_id, o));
        }

        Ok(())
    }

    fn short_term_encumber_outputs(
        &self,
        tx_id: TxId,
        outputs_to_send: &[DbUnblindedOutput],
        outputs_to_receive: &[DbUnblindedOutput],
    ) -> Result<(), OutputManagerStorageError>
    {
        let mut db = acquire_write_lock!(self.db);
        let mut outputs_to_be_spent = Vec::new();
        for i in outputs_to_send {
            if let Some(pos) = db
                .unspent_outputs
                .iter()
                .position(|v| v.output.unblinded_output.spending_key() == i.unblinded_output.spending_key())
            {
                outputs_to_be_spent.push(DbUnblindedOutput::from(db.unspent_outputs.remove(pos)));
            } else {
                return Err(OutputManagerStorageError::ValuesNotFound);
            }
        }

        let mut pending_transaction = PendingTransactionOutputs {
            tx_id,
            outputs_to_be_spent,
            outputs_to_be_received: Vec::new(),
            timestamp: Utc::now().naive_utc(),
            coinbase_block_height: None,
        };

        for co in outputs_to_receive {
            pending_transaction.outputs_to_be_received.push(co.clone());
        }

        db.short_term_pending_transactions.insert(tx_id, pending_transaction);

        Ok(())
    }

    fn confirm_encumbered_outputs(&self, tx_id: u64) -> Result<(), OutputManagerStorageError> {
        let mut db = acquire_write_lock!(self.db);

        let pending_tx = db
            .short_term_pending_transactions
            .remove(&tx_id)
            .ok_or_else(|| OutputManagerStorageError::ValueNotFound(DbKey::PendingTransactionOutputs(tx_id)))?;

        let _ = db.pending_transactions.insert(pending_tx.tx_id, pending_tx);

        Ok(())
    }

    fn clear_short_term_encumberances(&self) -> Result<(), OutputManagerStorageError> {
        let db = acquire_write_lock!(self.db);

        let short_term_encumberances = db.short_term_pending_transactions.clone();

        drop(db);

        for tx_id in short_term_encumberances.keys() {
            self.cancel_pending_transaction(*tx_id)?;
        }
        Ok(())
    }

    fn cancel_pending_transaction(&self, tx_id: TxId) -> Result<(), OutputManagerStorageError> {
        let mut db = acquire_write_lock!(self.db);
        let mut pending_tx = db.pending_transactions.remove(&tx_id);

        if pending_tx.is_none() {
            pending_tx = db.short_term_pending_transactions.remove(&tx_id);
        }

        let mut pending_tx = pending_tx
            .ok_or_else(|| OutputManagerStorageError::ValueNotFound(DbKey::PendingTransactionOutputs(tx_id)))?;

        for o in pending_tx.outputs_to_be_spent.drain(..) {
            db.unspent_outputs.push(DbOutput::new(tx_id, o));
        }

        Ok(())
    }

    fn timeout_pending_transactions(&self, period: Duration) -> Result<(), OutputManagerStorageError> {
        let db = acquire_write_lock!(self.db);
        let mut transactions_to_be_cancelled = Vec::new();

        for (tx_id, pt) in db.pending_transactions.iter() {
            if pt.timestamp + ChronoDuration::from_std(period)? < Utc::now().naive_utc() {
                transactions_to_be_cancelled.push(*tx_id);
            }
        }
        for (tx_id, pt) in db.short_term_pending_transactions.iter() {
            if pt.timestamp + ChronoDuration::from_std(period)? < Utc::now().naive_utc() {
                transactions_to_be_cancelled.push(*tx_id);
            }
        }

        drop(db);
        for t in transactions_to_be_cancelled {
            self.cancel_pending_transaction(t)?;
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

    fn invalidate_unspent_output(&self, output: &DbUnblindedOutput) -> Result<Option<TxId>, OutputManagerStorageError> {
        let mut db = acquire_write_lock!(self.db);
        match db
            .unspent_outputs
            .iter()
            .position(|v| v.output.unblinded_output.spending_key() == output.unblinded_output.spending_key())
        {
            Some(pos) => {
                let output = db.unspent_outputs.remove(pos);
                db.invalid_outputs.push(output.clone());
                Ok(output.tx_id)
            },
            None => Err(OutputManagerStorageError::ValuesNotFound),
        }
    }

    fn revalidate_unspent_output(&self, commitment: &Commitment) -> Result<(), OutputManagerStorageError> {
        let mut db = acquire_write_lock!(self.db);
        match db
            .invalid_outputs
            .iter()
            .position(|v| v.output.commitment == *commitment)
        {
            Some(pos) => {
                let output = db.invalid_outputs.remove(pos);
                db.unspent_outputs.push(output);
                Ok(())
            },
            None => Err(OutputManagerStorageError::ValuesNotFound),
        }
    }

    fn cancel_pending_transaction_at_block_height(&self, block_height: u64) -> Result<(), OutputManagerStorageError> {
        let pending_txs;
        {
            let db = acquire_write_lock!(self.db);
            pending_txs = db.pending_transactions.clone();
        }

        for (tx_id, p) in pending_txs {
            if let Some(bh) = p.coinbase_block_height {
                if bh == block_height {
                    self.cancel_pending_transaction(tx_id)?;
                }
            }
        }

        Ok(())
    }

    fn apply_encryption(&self, _: Aes256Gcm) -> Result<(), OutputManagerStorageError> {
        Ok(())
    }

    fn remove_encryption(&self) -> Result<(), OutputManagerStorageError> {
        Ok(())
    }
}

// A struct that contains the extra info we are using in the Sql version of this backend
#[derive(Clone)]
struct DbOutput {
    output: DbUnblindedOutput,
    tx_id: Option<u64>,
}

impl DbOutput {
    pub fn new(tx_id: u64, output: DbUnblindedOutput) -> Self {
        Self {
            output,
            tx_id: Some(tx_id),
        }
    }
}

impl From<DbUnblindedOutput> for DbOutput {
    fn from(uo: DbUnblindedOutput) -> Self {
        Self {
            output: uo,
            tx_id: None,
        }
    }
}

impl From<DbOutput> for DbUnblindedOutput {
    fn from(o: DbOutput) -> Self {
        o.output
    }
}
