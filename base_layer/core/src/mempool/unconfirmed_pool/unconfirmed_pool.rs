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
        consts::{MEMPOOL_UNCONFIRMED_POOL_STORAGE_CAPACITY, MEMPOOL_UNCONFIRMED_POOL_WEIGHT_TRANSACTION_SKIP_COUNT},
        priority::{FeePriority, PrioritizedTransaction},
        unconfirmed_pool::UnconfirmedPoolError,
    },
    transactions::{transaction::Transaction, types::Signature},
};
use log::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap},
    convert::TryFrom,
    sync::Arc,
};
use tari_crypto::tari_utilities::hex::Hex;

pub const LOG_TARGET: &str = "c::mp::unconfirmed_pool::unconfirmed_pool_storage";

/// Configuration for the UnconfirmedPool
#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct UnconfirmedPoolConfig {
    /// The maximum number of transactions that can be stored in the Unconfirmed Transaction pool
    pub storage_capacity: usize,
    /// The maximum number of transactions that can be skipped when compiling a set of highest priority transactions,
    /// skipping over large transactions are performed in an attempt to fit more transactions into the remaining space.
    pub weight_tx_skip_count: usize,
}

impl Default for UnconfirmedPoolConfig {
    fn default() -> Self {
        Self {
            storage_capacity: MEMPOOL_UNCONFIRMED_POOL_STORAGE_CAPACITY,
            weight_tx_skip_count: MEMPOOL_UNCONFIRMED_POOL_WEIGHT_TRANSACTION_SKIP_COUNT,
        }
    }
}

/// The Unconfirmed Transaction Pool consists of all unconfirmed transactions that are ready to be included in a block
/// and they are prioritised according to the priority metric.
/// The txs_by_signature HashMap is used to find a transaction using its excess_sig, this functionality is used to match
/// transactions included in blocks with transactions stored in the pool. The txs_by_priority BTreeMap prioritise the
/// transactions in the pool according to TXPriority, it allows transactions to be inserted in sorted order by their
/// priority. The txs_by_priority BTreeMap makes it easier to select the set of highest priority transactions that can
/// be included in a block. The excess_sig of a transaction is used a key to uniquely identify a specific transaction in
/// these containers.
pub struct UnconfirmedPool {
    config: UnconfirmedPoolConfig,
    txs_by_signature: HashMap<Signature, PrioritizedTransaction>,
    txs_by_priority: BTreeMap<FeePriority, Signature>,
}

impl UnconfirmedPool {
    /// Create a new UnconfirmedPool with the specified configuration
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

    /// Insert a new transaction into the UnconfirmedPool. Low priority transactions will be removed to make space for
    /// higher priority transactions. The lowest priority transactions will be removed when the maximum capacity is
    /// reached and the new transaction has a higher priority than the currently stored lowest priority transaction.
    #[allow(clippy::map_entry)]
    pub fn insert(&mut self, tx: Arc<Transaction>) -> Result<(), UnconfirmedPoolError> {
        let tx_key = tx.body.kernels()[0].excess_sig.clone();
        if !self.txs_by_signature.contains_key(&tx_key) {
            debug!(
                target: LOG_TARGET,
                "Inserting tx into unconfirmed pool: {}",
                tx_key.get_signature().to_hex()
            );
            trace!(target: LOG_TARGET, "Transaction inserted: {}", tx);
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

    /// Insert a set of new transactions into the UnconfirmedPool
    #[cfg(test)]
    pub fn insert_txs(&mut self, txs: Vec<Arc<Transaction>>) -> Result<(), UnconfirmedPoolError> {
        for tx in txs.into_iter() {
            self.insert(tx)?;
        }
        Ok(())
    }

    /// Check if a transaction is available in the UnconfirmedPool
    pub fn has_tx_with_excess_sig(&self, excess_sig: &Signature) -> bool {
        self.txs_by_signature.contains_key(excess_sig)
    }

    /// Returns a set of the highest priority unconfirmed transactions, that can be included in a block
    pub fn highest_priority_txs(&self, total_weight: u64) -> Result<Vec<Arc<Transaction>>, UnconfirmedPoolError> {
        let mut selected_txs: Vec<Arc<Transaction>> = Vec::new();
        let mut curr_weight: u64 = 0;
        let mut curr_skip_count: usize = 0;
        for (_, tx_key) in self.txs_by_priority.iter().rev() {
            let ptx = self
                .txs_by_signature
                .get(tx_key)
                .ok_or_else(|| UnconfirmedPoolError::StorageOutofSync)?;

            if curr_weight + ptx.weight <= total_weight {
                if !UnconfirmedPool::find_duplicate_input(&selected_txs, &ptx.transaction) {
                    curr_weight += ptx.weight;
                    selected_txs.push(ptx.transaction.clone());
                }
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

    // This will search a Vec<Arc<Transaction>> for duplicate inputs of a tx
    fn find_duplicate_input(array_of_tx: &[Arc<Transaction>], tx: &Arc<Transaction>) -> bool {
        for transaction in array_of_tx {
            for input in transaction.body.inputs() {
                if tx.body.inputs().contains(input) {
                    return true;
                }
            }
        }
        false
    }

    /// Remove all published transactions from the UnconfirmedPool and discard all double spend transactions.
    /// Returns a list of all transactions that were removed the unconfirmed pool as a result of appearing in the block.
    fn discard_double_spends(&mut self, published_block: &Block) {
        let mut removed_tx_keys = Vec::new();
        for (tx_key, ptx) in self.txs_by_signature.iter() {
            for input in ptx.transaction.body.inputs() {
                if published_block.body.inputs().contains(input) {
                    self.txs_by_priority.remove(&ptx.priority);
                    removed_tx_keys.push(tx_key.clone());
                }
            }
        }

        for tx_key in &removed_tx_keys {
            trace!(
                target: LOG_TARGET,
                "Removing double spends from unconfirmed pool: {:?}",
                tx_key
            );
            self.txs_by_signature.remove(&tx_key);
        }
    }

    /// Remove all published transactions from the UnconfirmedPoolStorage and discard double spends
    pub fn remove_published_and_discard_double_spends(&mut self, published_block: &Block) -> Vec<Arc<Transaction>> {
        let mut removed_txs = Vec::new();
        published_block.body.kernels().iter().for_each(|kernel| {
            if let Some(ptx) = self.txs_by_signature.get(&kernel.excess_sig) {
                self.txs_by_priority.remove(&ptx.priority);
                if let Some(ptx) = self.txs_by_signature.remove(&kernel.excess_sig) {
                    removed_txs.push(ptx.transaction);
                }
            }
        });
        // First remove published transactions before discarding double spends
        self.discard_double_spends(published_block);

        removed_txs
    }

    /// Remove all unconfirmed transactions that have become time locked. This can happen when the chain height was
    /// reduced on some reorgs.
    pub fn remove_timelocked(&mut self, tip_height: u64) -> Vec<Arc<Transaction>> {
        let mut removed_tx_keys: Vec<Signature> = Vec::new();
        for (tx_key, ptx) in self.txs_by_signature.iter() {
            if ptx.transaction.min_spendable_height() > tip_height + 1 {
                self.txs_by_priority.remove(&ptx.priority);
                removed_tx_keys.push(tx_key.clone());
            }
        }
        let mut removed_txs: Vec<Arc<Transaction>> = Vec::new();
        for tx_key in removed_tx_keys {
            trace!(
                target: LOG_TARGET,
                "Removing time locked transaction from unconfirmed pool: {:?}",
                tx_key
            );
            if let Some(ptx) = self.txs_by_signature.remove(&tx_key) {
                removed_txs.push(ptx.transaction);
            }
        }
        removed_txs
    }

    /// Returns the total number of unconfirmed transactions stored in the UnconfirmedPool.
    pub fn len(&self) -> usize {
        self.txs_by_signature.len()
    }

    /// Returns all transaction stored in the UnconfirmedPool.
    pub fn snapshot(&self) -> Vec<Arc<Transaction>> {
        self.txs_by_signature
            .iter()
            .map(|(_, ptx)| ptx.transaction.clone())
            .collect()
    }

    /// Returns the total weight of all transactions stored in the pool.
    pub fn calculate_weight(&self) -> u64 {
        self.txs_by_signature
            .iter()
            .fold(0, |weight, (_, ptx)| weight + ptx.transaction.calculate_weight())
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

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        consensus::{ConsensusManagerBuilder, Network},
        test_helpers::create_orphan_block,
        transactions::tari_amount::MicroTari,
        tx,
    };

    #[test]
    fn test_insert_and_retrieve_highest_priority_txs() {
        let tx1 = Arc::new(tx!(MicroTari(5_000), fee: MicroTari(50), inputs: 2, outputs: 1).0);
        let tx2 = Arc::new(tx!(MicroTari(5_000), fee: MicroTari(20), inputs: 4, outputs: 1).0);
        let tx3 = Arc::new(tx!(MicroTari(5_000), fee: MicroTari(100), inputs: 5, outputs: 1).0);
        let tx4 = Arc::new(tx!(MicroTari(5_000), fee: MicroTari(30), inputs: 3, outputs: 1).0);
        let tx5 = Arc::new(tx!(MicroTari(5_000), fee: MicroTari(55), inputs: 5, outputs: 1).0);

        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: 4,
            weight_tx_skip_count: 3,
        });
        unconfirmed_pool
            .insert_txs(vec![tx1.clone(), tx2.clone(), tx3.clone(), tx4.clone(), tx5.clone()])
            .unwrap();
        // Check that lowest priority tx was removed to make room for new incoming transactions
        assert_eq!(
            unconfirmed_pool.has_tx_with_excess_sig(&tx1.body.kernels()[0].excess_sig),
            true
        );
        assert_eq!(
            unconfirmed_pool.has_tx_with_excess_sig(&tx2.body.kernels()[0].excess_sig),
            false
        );
        assert_eq!(
            unconfirmed_pool.has_tx_with_excess_sig(&tx3.body.kernels()[0].excess_sig),
            true
        );
        assert_eq!(
            unconfirmed_pool.has_tx_with_excess_sig(&tx4.body.kernels()[0].excess_sig),
            true
        );
        assert_eq!(
            unconfirmed_pool.has_tx_with_excess_sig(&tx5.body.kernels()[0].excess_sig),
            true
        );
        // Retrieve the set of highest priority unspent transactions
        let desired_weight = tx1.calculate_weight() + tx3.calculate_weight() + tx5.calculate_weight();
        let selected_txs = unconfirmed_pool.highest_priority_txs(desired_weight).unwrap();
        assert_eq!(selected_txs.len(), 3);
        assert!(selected_txs.contains(&tx1));
        assert!(selected_txs.contains(&tx3));
        assert!(selected_txs.contains(&tx5));
        // Note that transaction tx5 could not be included as its weight was to big to fit into the remaining allocated
        // space, the second best transaction was then included

        assert!(unconfirmed_pool.check_status());
    }

    #[test]
    fn test_remove_reorg_txs() {
        let network = Network::LocalNet;
        let consensus = ConsensusManagerBuilder::new(network).build();
        let tx1 = Arc::new(tx!(MicroTari(10_000), fee: MicroTari(50), inputs:2, outputs: 1).0);
        let tx2 = Arc::new(tx!(MicroTari(10_000), fee: MicroTari(20), inputs:3, outputs: 1).0);
        let tx3 = Arc::new(tx!(MicroTari(10_000), fee: MicroTari(100), inputs:2, outputs: 1).0);
        let tx4 = Arc::new(tx!(MicroTari(10_000), fee: MicroTari(30), inputs:4, outputs: 1).0);
        let tx5 = Arc::new(tx!(MicroTari(10_000), fee: MicroTari(50), inputs:3, outputs: 1).0);
        let tx6 = Arc::new(tx!(MicroTari(10_000), fee: MicroTari(75), inputs:2, outputs: 1).0);

        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: 10,
            weight_tx_skip_count: 3,
        });
        unconfirmed_pool
            .insert_txs(vec![tx1.clone(), tx2.clone(), tx3.clone(), tx4.clone(), tx5.clone()])
            .unwrap();
        // utx6 should not be added to unconfirmed_pool as it is an unknown transactions that was included in the block
        // by another node

        let snapshot_txs = unconfirmed_pool.snapshot();
        assert_eq!(snapshot_txs.len(), 5);
        assert!(snapshot_txs.contains(&tx1));
        assert!(snapshot_txs.contains(&tx2));
        assert!(snapshot_txs.contains(&tx3));
        assert!(snapshot_txs.contains(&tx4));
        assert!(snapshot_txs.contains(&tx5));

        let published_block = create_orphan_block(0, vec![(*tx1).clone(), (*tx3).clone(), (*tx5).clone()], &consensus);
        let _ = unconfirmed_pool.remove_published_and_discard_double_spends(&published_block);

        assert_eq!(
            unconfirmed_pool.has_tx_with_excess_sig(&tx1.body.kernels()[0].excess_sig),
            false
        );
        assert_eq!(
            unconfirmed_pool.has_tx_with_excess_sig(&tx2.body.kernels()[0].excess_sig),
            true
        );
        assert_eq!(
            unconfirmed_pool.has_tx_with_excess_sig(&tx3.body.kernels()[0].excess_sig),
            false
        );
        assert_eq!(
            unconfirmed_pool.has_tx_with_excess_sig(&tx4.body.kernels()[0].excess_sig),
            true
        );
        assert_eq!(
            unconfirmed_pool.has_tx_with_excess_sig(&tx5.body.kernels()[0].excess_sig),
            false
        );
        assert_eq!(
            unconfirmed_pool.has_tx_with_excess_sig(&tx6.body.kernels()[0].excess_sig),
            false
        );

        assert!(unconfirmed_pool.check_status());
    }

    #[test]
    fn test_discard_double_spend_txs() {
        let network = Network::LocalNet;
        let consensus = ConsensusManagerBuilder::new(network).build();
        let tx1 = Arc::new(tx!(MicroTari(5_000), fee: MicroTari(50), inputs:2, outputs:1).0);
        let tx2 = Arc::new(tx!(MicroTari(5_000), fee: MicroTari(20), inputs:3, outputs:1).0);
        let tx3 = Arc::new(tx!(MicroTari(5_000), fee: MicroTari(100), inputs:2, outputs:1).0);
        let tx4 = Arc::new(tx!(MicroTari(5_000), fee: MicroTari(30), inputs:2, outputs:1).0);
        let mut tx5 = tx!(MicroTari(5_000), fee:MicroTari(50), inputs:3, outputs:1).0;
        let mut tx6 = tx!(MicroTari(5_000), fee:MicroTari(75), inputs: 2, outputs: 1).0;
        // tx1 and tx5 have a shared input. Also, tx3 and tx6 have a shared input
        tx5.body.inputs_mut()[0] = tx1.body.inputs()[0].clone();
        tx6.body.inputs_mut()[1] = tx3.body.inputs()[1].clone();
        let tx5 = Arc::new(tx5);
        let tx6 = Arc::new(tx6);

        let mut unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: 10,
            weight_tx_skip_count: 3,
        });
        unconfirmed_pool
            .insert_txs(vec![
                tx1.clone(),
                tx2.clone(),
                tx3.clone(),
                tx4.clone(),
                tx5.clone(),
                tx6.clone(),
            ])
            .unwrap();

        // The publishing of tx1 and tx3 will be double-spends and orphan tx5 and tx6
        let published_block = create_orphan_block(0, vec![(*tx1).clone(), (*tx2).clone(), (*tx3).clone()], &consensus);

        let _ = unconfirmed_pool.remove_published_and_discard_double_spends(&published_block); // Double spends are discarded

        assert_eq!(
            unconfirmed_pool.has_tx_with_excess_sig(&tx1.body.kernels()[0].excess_sig),
            false
        );
        assert_eq!(
            unconfirmed_pool.has_tx_with_excess_sig(&tx2.body.kernels()[0].excess_sig),
            false
        );
        assert_eq!(
            unconfirmed_pool.has_tx_with_excess_sig(&tx3.body.kernels()[0].excess_sig),
            false
        );
        assert_eq!(
            unconfirmed_pool.has_tx_with_excess_sig(&tx4.body.kernels()[0].excess_sig),
            true
        );
        assert_eq!(
            unconfirmed_pool.has_tx_with_excess_sig(&tx5.body.kernels()[0].excess_sig),
            false
        );
        assert_eq!(
            unconfirmed_pool.has_tx_with_excess_sig(&tx6.body.kernels()[0].excess_sig),
            false
        );

        assert!(unconfirmed_pool.check_status());
    }
}
