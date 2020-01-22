//  Copyright 2019 The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use crate::{
    blocks::Block,
    mempool::{
        pending_pool::{PendingPoolConfig, PendingPoolError},
        priority::{FeePriority, TimelockPriority, TimelockedTransaction},
    },
    transactions::{transaction::Transaction, types::Signature},
};
use std::{
    collections::{BTreeMap, HashMap},
    convert::TryFrom,
    sync::Arc,
};

/// PendingPool makes use of PendingPoolStorage to provide thread safe access to its Hashmap and BTreeMaps.
/// The txs_by_signature HashMap is used to find a transaction using its excess_sig, this functionality is used to match
/// transactions included in blocks with transactions stored in the pool.
/// The txs_by_fee_priority BTreeMap prioritize the transactions in the pool according to FeePriority, it allows
/// transactions to be inserted in sorted order based on their priority. The txs_by_timelock_priority BTreeMap
/// prioritize the transactions in the pool according to TimelockPriority, it allows transactions to be inserted in
/// sorted order based on the expiry of their time-locks.
pub struct PendingPoolStorage {
    config: PendingPoolConfig,
    txs_by_signature: HashMap<Signature, TimelockedTransaction>,
    txs_by_fee_priority: BTreeMap<FeePriority, Signature>,
    txs_by_timelock_priority: BTreeMap<TimelockPriority, Signature>,
}

impl PendingPoolStorage {
    /// Create a new PendingPoolStorage with the specified configuration
    pub fn new(config: PendingPoolConfig) -> Self {
        Self {
            config,
            txs_by_signature: HashMap::new(),
            txs_by_fee_priority: BTreeMap::new(),
            txs_by_timelock_priority: BTreeMap::new(),
        }
    }

    fn lowest_fee_priority(&self) -> &FeePriority {
        self.txs_by_fee_priority.iter().next().unwrap().0
    }

    fn remove_tx_with_lowest_fee_priority(&mut self) {
        if let Some((_, tx_key)) = self
            .txs_by_fee_priority
            .iter()
            .next()
            .map(|(p, s)| (p.clone(), s.clone()))
        {
            if let Some(removed_tx) = self.txs_by_signature.remove(&tx_key) {
                self.txs_by_fee_priority.remove(&removed_tx.fee_priority);
                self.txs_by_timelock_priority.remove(&removed_tx.timelock_priority);
            }
        }
    }

    /// Insert a new transaction into the PendingPoolStorage. Low priority transactions will be removed to make space
    /// for higher priority transactions.
    pub fn insert(&mut self, tx: Arc<Transaction>) -> Result<(), PendingPoolError> {
        let tx_key = tx.body.kernels()[0].excess_sig.clone();
        if !self.txs_by_signature.contains_key(&tx_key) {
            let prioritized_tx = TimelockedTransaction::try_from((*tx).clone())?;
            if self.txs_by_signature.len() >= self.config.storage_capacity {
                if prioritized_tx.fee_priority < *self.lowest_fee_priority() {
                    return Ok(());
                }
                self.remove_tx_with_lowest_fee_priority();
            }

            self.txs_by_fee_priority
                .insert(prioritized_tx.fee_priority.clone(), tx_key.clone());
            self.txs_by_timelock_priority
                .insert(prioritized_tx.timelock_priority.clone(), tx_key.clone());
            self.txs_by_signature.insert(tx_key, prioritized_tx);
        }
        Ok(())
    }

    /// Insert a set of new transactions into the PendingPoolStorage
    pub fn insert_txs(&mut self, txs: Vec<Arc<Transaction>>) -> Result<(), PendingPoolError> {
        for tx in txs.into_iter() {
            self.insert(tx)?;
        }
        Ok(())
    }

    /// Check if a transaction is stored in the PendingPoolStorage
    pub fn has_tx_with_excess_sig(&self, excess_sig: &Signature) -> bool {
        self.txs_by_signature.contains_key(excess_sig)
    }

    /// Remove double-spends from the PendingPoolStorage. These transactions were orphaned by the provided published
    /// block. Check if any of the unspent transactions in the PendingPool has inputs that was spent by the provided
    /// published block.
    fn discard_double_spends(&mut self, published_block: &Block) {
        let mut removed_tx_keys: Vec<Signature> = Vec::new();
        for (tx_key, ptx) in self.txs_by_signature.iter() {
            for input in ptx.transaction.body.inputs() {
                if published_block.body.inputs().contains(input) {
                    self.txs_by_fee_priority.remove(&ptx.fee_priority);
                    self.txs_by_timelock_priority.remove(&ptx.timelock_priority);
                    removed_tx_keys.push(tx_key.clone());
                }
            }
        }

        for tx_key in &removed_tx_keys {
            self.txs_by_signature.remove(&tx_key);
        }
    }

    /// Remove all published transactions from the UnconfirmedPoolStorage and discard double spends
    pub fn remove_unlocked_and_discard_double_spends(
        &mut self,
        published_block: &Block,
    ) -> Result<Vec<Arc<Transaction>>, PendingPoolError>
    {
        self.discard_double_spends(published_block);

        let mut removed_txs: Vec<Arc<Transaction>> = Vec::new();
        let mut removed_tx_keys: Vec<TimelockPriority> = Vec::new();
        for (_, tx_key) in self.txs_by_timelock_priority.iter() {
            if self
                .txs_by_signature
                .get(tx_key)
                .ok_or(PendingPoolError::StorageOutofSync)?
                .max_timelock_height >
                published_block.header.height
            {
                break;
            }

            if let Some(removed_ptx) = self.txs_by_signature.remove(&tx_key) {
                self.txs_by_fee_priority.remove(&removed_ptx.fee_priority);
                removed_tx_keys.push(removed_ptx.timelock_priority);
                removed_txs.push(removed_ptx.transaction);
            }
        }

        for tx_key in &removed_tx_keys {
            self.txs_by_timelock_priority.remove(&tx_key);
        }

        Ok(removed_txs)
    }

    /// Returns the total number of unconfirmed transactions stored in the PendingPoolStorage
    pub fn len(&self) -> usize {
        self.txs_by_signature.len()
    }

    /// Returns all transaction stored in the PendingPoolStorage.
    pub fn snapshot(&self) -> Vec<Arc<Transaction>> {
        let mut txs: Vec<Arc<Transaction>> = Vec::with_capacity(self.txs_by_signature.len());
        self.txs_by_signature
            .iter()
            .for_each(|(_, ptx)| txs.push(ptx.transaction.clone()));
        txs
    }

    /// Returns the total weight of all transactions stored in the pool.
    pub fn calculate_weight(&self) -> u64 {
        let mut weight: u64 = 0;
        self.txs_by_signature
            .iter()
            .for_each(|(_, ptx)| weight += ptx.transaction.calculate_weight());
        (weight)
    }

    #[cfg(test)]
    /// Checks the consistency status of the Hashmap and the BtreeMaps
    pub fn check_status(&self) -> bool {
        if (self.txs_by_fee_priority.len() != self.txs_by_signature.len()) ||
            (self.txs_by_timelock_priority.len() != self.txs_by_signature.len())
        {
            return false;
        }
        self.txs_by_fee_priority
            .iter()
            .all(|(_, tx_key)| self.txs_by_signature.contains_key(tx_key)) &&
            self.txs_by_timelock_priority
                .iter()
                .all(|(_, tx_key)| self.txs_by_signature.contains_key(tx_key))
    }
}
