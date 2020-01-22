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
        priority::{FeePriority, PrioritizedTransaction},
        unconfirmed_pool::{UnconfirmedPoolConfig, UnconfirmedPoolError},
    },
    transactions::{transaction::Transaction, types::Signature},
};
use std::{
    collections::{BTreeMap, HashMap},
    convert::TryFrom,
    sync::Arc,
};

/// UnconfirmedPool makes use of UnconfirmedPoolStorage to provide thread save access to its Hashmap and BTreeMap.
/// The txs_by_signature HashMap is used to find a transaction using its excess_sig, this functionality is used to match
/// transactions included in blocks with transactions stored in the pool. The txs_by_priority BTreeMap prioritise the
/// transactions in the pool according to TXPriority, it allows transactions to be inserted in sorted order by their
/// priority. The txs_by_priority BTreeMap makes it easier to select the set of highest priority transactions that can
/// be included in a block. The excess_sig of a transaction is used a key to uniquely identify a specific transaction in
/// these containers.
pub struct UnconfirmedPoolStorage {
    config: UnconfirmedPoolConfig,
    txs_by_signature: HashMap<Signature, PrioritizedTransaction>,
    txs_by_priority: BTreeMap<FeePriority, Signature>,
}

impl UnconfirmedPoolStorage {
    /// Create a new UnconfirmedPoolStorage with the specified configuration
    pub fn new(config: UnconfirmedPoolConfig) -> Self {
        Self {
            config,
            txs_by_signature: HashMap::new(),
            txs_by_priority: BTreeMap::new(),
        }
    }

    fn lowest_priority(&self) -> &FeePriority {
        self.txs_by_priority.iter().next().unwrap().0
    }

    fn remove_lowest_priority_tx(&mut self) {
        if let Some((priority, sig)) = self.txs_by_priority.iter().next().map(|(p, s)| (p.clone(), s.clone())) {
            self.txs_by_signature.remove(&sig);
            self.txs_by_priority.remove(&priority);
        }
    }

    /// Insert a new transaction into the UnconfirmedPoolStorage. Low priority transactions will be removed to make
    /// space for higher priority transactions. The lowest priority transactions will be removed when the maximum
    /// capacity is reached and the new transaction has a higher priority than the currently stored lowest priority
    /// transaction.
    pub fn insert(&mut self, tx: Arc<Transaction>) -> Result<(), UnconfirmedPoolError> {
        let tx_key = tx.body.kernels()[0].excess_sig.clone();
        if !self.txs_by_signature.contains_key(&tx_key) {
            let prioritized_tx = PrioritizedTransaction::try_from((*tx).clone())?;
            if self.txs_by_signature.len() >= self.config.storage_capacity {
                if prioritized_tx.priority < *self.lowest_priority() {
                    return Ok(());
                }
                self.remove_lowest_priority_tx();
            }

            self.txs_by_priority
                .insert(prioritized_tx.priority.clone(), tx_key.clone());
            self.txs_by_signature.insert(tx_key, prioritized_tx);
        }
        Ok(())
    }

    /// Insert a set of new transactions into the UnconfirmedPoolStorage
    pub fn insert_txs(&mut self, txs: Vec<Arc<Transaction>>) -> Result<(), UnconfirmedPoolError> {
        for tx in txs.into_iter() {
            self.insert(tx)?;
        }
        Ok(())
    }

    /// Check if a transaction is stored in the UnconfirmedPoolStorage
    pub fn has_tx_with_excess_sig(&self, excess_sig: &Signature) -> bool {
        self.txs_by_signature.contains_key(excess_sig)
    }

    /// Returns a set of the highest priority unconfirmed transactions, that can be included in a block.
    pub fn highest_priority_txs(&self, total_weight: u64) -> Result<Vec<Arc<Transaction>>, UnconfirmedPoolError> {
        let mut selected_txs: Vec<Arc<Transaction>> = Vec::new();
        let mut curr_weight: u64 = 0;
        let mut curr_skip_count: usize = 0;
        for (_, tx_key) in self.txs_by_priority.iter().rev() {
            let ptx = self
                .txs_by_signature
                .get(tx_key)
                .ok_or(UnconfirmedPoolError::StorageOutofSync)?;

            if curr_weight + ptx.weight <= total_weight {
                curr_weight += ptx.weight;
                selected_txs.push(ptx.transaction.clone());
            } else {
                // Check if some the next few txs with slightly lower priority wont fit in the remaining space.
                curr_skip_count += 1;
                if curr_skip_count >= self.config.weight_tx_skip_count {
                    break;
                }
            }
        }
        Ok(selected_txs)
    }

    // Remove double-spends from the UnconfirmedPoolStorage. These transactions were orphaned by the provided published
    // block. Check if any of the unspent transactions in the UnconfirmedPool has inputs that was spent by the provided
    // published block.
    fn discard_double_spends(&mut self, published_block: &Block) {
        let mut removed_tx_keys: Vec<Signature> = Vec::new();
        for (tx_key, ptx) in self.txs_by_signature.iter() {
            for input in ptx.transaction.body.inputs() {
                if published_block.body.inputs().contains(input) {
                    self.txs_by_priority.remove(&ptx.priority);
                    removed_tx_keys.push(tx_key.clone());
                }
            }
        }

        for tx_key in &removed_tx_keys {
            self.txs_by_signature.remove(&tx_key);
        }
    }

    /// Remove all published transactions from the UnconfirmedPoolStorage and discard double spends
    pub fn remove_published_and_discard_double_spends(&mut self, published_block: &Block) -> Vec<Arc<Transaction>> {
        let mut removed_txs: Vec<Arc<Transaction>> = Vec::new();
        published_block.body.kernels().iter().for_each(|kernel| {
            if let Some(ptx) = self.txs_by_signature.get(&kernel.excess_sig) {
                self.txs_by_priority.remove(&ptx.priority);
                removed_txs.push(self.txs_by_signature.remove(&kernel.excess_sig).unwrap().transaction);
            }
        });
        // First remove published transactions before discarding double spends
        self.discard_double_spends(published_block);

        removed_txs
    }

    /// Returns the total number of unconfirmed transactions stored in the UnconfirmedPoolStorage
    pub fn len(&self) -> usize {
        self.txs_by_signature.len()
    }

    /// Returns all transaction stored in the UnconfirmedPoolStorage.
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
    /// Checks the consistency status of the Hashmap and BtreeMap
    pub fn check_status(&self) -> bool {
        if self.txs_by_priority.len() != self.txs_by_signature.len() {
            return false;
        }
        self.txs_by_priority
            .iter()
            .all(|(_, tx_key)| self.txs_by_signature.contains_key(tx_key))
    }
}
