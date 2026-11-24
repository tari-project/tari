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

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use log::*;
use serde::{Deserialize, Serialize};
use tari_common_types::types::{CompressedSignature, PrivateKey};
use tari_node_components::blocks::Block;
use tari_transaction_components::transaction_components::Transaction;
use tari_utilities::hex::Hex;

use crate::mempool::{MempoolError, kernel_set, shrink_hashmap::shrink_hashmap};

pub const LOG_TARGET: &str = "c::mp::reorg_pool::reorg_pool_storage";

/// Configuration for the ReorgPool
#[derive(Clone, Copy, Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct ReorgPoolConfig {
    /// The height horizon to clear transactions from the reorg pool.
    pub expiry_height: u64,
}

impl Default for ReorgPoolConfig {
    fn default() -> Self {
        Self { expiry_height: 5 }
    }
}

type TransactionId = usize;

/// The ReorgPool consists of all transactions that have recently been added to blocks.
/// When a potential blockchain reorganization occurs the transactions can be recovered from the ReorgPool and can be
/// added back into the UnconfirmedPool. Transactions in the ReOrg pool expire as block height moves on.
pub struct ReorgPool {
    config: ReorgPoolConfig,
    key_counter: usize,
    tx_by_key: HashMap<TransactionId, Arc<Transaction>>,
    txs_by_signature: HashMap<PrivateKey, Vec<TransactionId>>,
    /// The transaction with each kernel set (see [kernel_set]). Insert refuses an exact match, so there is one.
    tx_by_kernel_set: HashMap<Vec<PrivateKey>, TransactionId>,
    txs_by_height: HashMap<u64, Vec<TransactionId>>,
}

impl ReorgPool {
    /// Create a new ReorgPool with the specified configuration
    pub fn new(config: ReorgPoolConfig) -> Self {
        Self {
            config,
            key_counter: 0,
            tx_by_key: HashMap::new(),
            txs_by_signature: HashMap::new(),
            tx_by_kernel_set: HashMap::new(),
            txs_by_height: HashMap::new(),
        }
    }

    /// Insert a new transaction into the ReorgPool. Published transactions will be discarded once they are
    /// `config.expiry_height` blocks old.
    fn insert(&mut self, height: u64, tx: Arc<Transaction>) {
        if self.has_transaction(&tx) {
            let excess_hex = tx
                .first_kernel_excess_sig()
                .map(|s| s.get_signature().to_hex())
                .unwrap_or_else(|| "no kernel!".to_string());
            debug!(
                target: LOG_TARGET,
                "Transaction {excess_hex} already found in reorg pool"
            );
            self.cleanup_expired(height);
            return;
        }

        let new_key = self.get_next_key();
        for kernel in tx.body.kernels() {
            let sig = kernel.excess_sig.get_signature();
            self.txs_by_signature.entry(sig.clone()).or_default().push(new_key);
        }
        if let Some(set) = kernel_set(&tx) {
            self.tx_by_kernel_set.insert(set, new_key);
        }

        trace!(
            target: LOG_TARGET,
            "Inserted transaction {new_key} into reorg pool at height {height}"
        );
        self.tx_by_key.insert(new_key, tx);
        self.txs_by_height.entry(height).or_default().push(new_key);
        self.cleanup_expired(height);
    }

    /// Insert a set of new transactions into the ReorgPool
    pub fn insert_all(&mut self, height: u64, txs: Vec<Arc<Transaction>>) {
        debug!(
            target: LOG_TARGET,
            "Inserting {} transaction(s) into reorg pool at height {}",
            txs.len(),
            height
        );

        // Even if we are not inserting any transactions, we still need to clear out the pool by height
        if txs.is_empty() {
            self.cleanup_expired(height);
        }
        for tx in txs {
            self.insert(height, tx);
        }
    }

    /// This will clear the reorg pool and return all transactions contained within
    pub fn clear_and_retrieve_all(&mut self) -> Vec<Arc<Transaction>> {
        let mut result = Vec::new();
        for (_, tx) in self.tx_by_key.drain() {
            result.push(tx);
        }
        self.txs_by_signature.clear();
        self.tx_by_kernel_set.clear();
        self.txs_by_height.clear();
        result
    }

    pub fn retrieve_by_excess_sigs(
        &self,
        excess_sigs: &[PrivateKey],
    ) -> Result<(Vec<Arc<Transaction>>, Vec<PrivateKey>), MempoolError> {
        // Hashset used to prevent duplicates
        let mut found = HashSet::new();
        let mut remaining = Vec::new();

        for sig in excess_sigs {
            let mut matched = Vec::new();
            if let Some(ids) = self.txs_by_signature.get(sig) {
                for id in ids {
                    if self.tx_by_key.contains_key(id) {
                        matched.push(*id);
                    } else {
                        warn!(
                            target: LOG_TARGET,
                            "Reorg pool signature index has dangling transaction id {id}"
                        );
                    }
                }
            }
            if matched.is_empty() {
                remaining.push(sig.clone());
            } else {
                found.extend(matched);
            }
        }

        let found = found
            .into_iter()
            .filter_map(|id| self.tx_by_key.get(&id).cloned())
            .collect();

        Ok((found, remaining))
    }

    /// Check if a transaction is stored in the ReorgPool
    pub fn has_tx_with_excess_sig(&self, excess_sig: &CompressedSignature) -> bool {
        self.txs_by_signature
            .get(excess_sig.get_signature())
            .is_some_and(|ids| ids.iter().any(|id| self.tx_by_key.contains_key(id)))
    }

    /// True if a reorg pool transaction has exactly the kernel set of `tx`
    pub fn has_transaction(&self, tx: &Transaction) -> bool {
        kernel_set(tx).is_some_and(|set| self.tx_by_kernel_set.contains_key(&set))
    }

    fn remove_from_kernel_set_index(&mut self, tx_id: TransactionId, tx: &Transaction) {
        if let Some(set) = kernel_set(tx) &&
            self.tx_by_kernel_set.get(&set) == Some(&tx_id)
        {
            self.tx_by_kernel_set.remove(&set);
        }
    }

    /// Remove the transactions from the ReorgPool that were used in provided removed blocks. The transactions
    /// can be resubmitted to the Unconfirmed Pool.
    pub fn remove_reorged_txs_and_discard_double_spends(
        &mut self,
        removed_blocks: &[Arc<Block>],
        new_blocks: &[Arc<Block>],
    ) -> Vec<Arc<Transaction>> {
        for block in new_blocks {
            debug!(
                target: LOG_TARGET,
                "Mempool processing reorg added new block {} ({})",
                block.header.height,
                block.header.hash().to_hex(),
            );
            self.discard_double_spends(block);
        }

        let mut removed_txs = Vec::new();
        for block in removed_blocks {
            debug!(
                target: LOG_TARGET,
                "Mempool processing reorg removed block {} ({})",
                block.header.height,
                block.header.hash().to_hex(),
            );
            for kernel in block.body.kernels() {
                if let Some(removed_tx_ids) = self.txs_by_signature.get(kernel.excess_sig.get_signature()).cloned() {
                    for tx_id in removed_tx_ids {
                        if let Some(tx) = self.remove(tx_id) {
                            trace!(target: LOG_TARGET, "Removed tx from reorg pool: {tx_id:?}");
                            removed_txs.push(tx);
                        }
                    }
                    self.txs_by_signature.remove(kernel.excess_sig.get_signature());
                }
            }
        }

        removed_txs
    }

    fn remove_from_height_index(&mut self, tx_id: TransactionId) {
        let mut heights_to_remove = Vec::new();
        for (height, ids) in &mut self.txs_by_height {
            if let Some(pos) = ids.iter().position(|id| *id == tx_id) {
                ids.remove(pos);
                if ids.is_empty() {
                    heights_to_remove.push(*height);
                }
            }
        }

        for h in heights_to_remove {
            self.txs_by_height.remove(&h);
        }
    }

    /// Remove double-spends from the ReorgPool. These transactions were orphaned by the provided published
    /// block. Check if any of the transactions in the ReorgPool has inputs that was spent by the provided
    /// published block.
    fn discard_double_spends(&mut self, published_block: &Block) {
        let mut to_remove = Vec::new();
        for (id, tx) in &self.tx_by_key {
            for input in tx.body.inputs() {
                if published_block.body.inputs().contains(input) {
                    to_remove.push(*id);
                }
            }
        }

        for id in to_remove {
            self.remove(id);
            trace!(target: LOG_TARGET, "Removed double spend tx {id} from reorg pool");
        }
    }

    fn remove(&mut self, tx_id: TransactionId) -> Option<Arc<Transaction>> {
        let tx = self.tx_by_key.remove(&tx_id)?;

        for kernel in tx.body.kernels() {
            let sig = kernel.excess_sig.get_signature();
            if let Some(ids) = self.txs_by_signature.get_mut(sig) {
                if let Some(pos) = ids.iter().position(|k| *k == tx_id) {
                    ids.remove(pos);
                } else {
                    warn!(
                        target: LOG_TARGET,
                        "Reorg pool signature index missing transaction id {tx_id}"
                    );
                }
                if ids.is_empty() {
                    self.txs_by_signature.remove(sig);
                }
            } else {
                warn!(
                    target: LOG_TARGET,
                    "Reorg pool signature index missing entry for transaction id {tx_id}"
                );
            }
        }

        self.remove_from_kernel_set_index(tx_id, &tx);
        self.remove_from_height_index(tx_id);

        Some(tx)
    }

    /// Returns the total number of published transactions stored in the ReorgPool
    pub fn len(&self) -> usize {
        self.tx_by_key.len()
    }

    /// Returns all transaction stored in the ReorgPool.
    pub fn snapshot(&self) -> Vec<Arc<Transaction>> {
        self.tx_by_key.values().cloned().collect()
    }

    fn get_next_key(&mut self) -> usize {
        let key = self.key_counter;
        self.key_counter = self.key_counter.wrapping_add(1) % usize::MAX;
        key
    }

    fn cleanup_expired(&mut self, height: u64) {
        let height = match height.checked_sub(self.config.expiry_height) {
            Some(h) => h,
            None => return,
        };

        if let Some(tx_ids) = self.txs_by_height.remove(&height) {
            debug!(
                target: LOG_TARGET,
                "Clearing {} transactions from mempool for height {}",
                tx_ids.len(),
                height
            );
            for tx_id in tx_ids {
                let tx = self.tx_by_key.remove(&tx_id).expect("reorg mempool out of sync");
                self.remove_from_kernel_set_index(tx_id, &tx);

                for kernel in tx.body.kernels() {
                    let sig = kernel.excess_sig.get_signature();
                    if let Some(keys) = self.txs_by_signature.get_mut(sig) {
                        let pos = keys
                            .iter()
                            .position(|k| *k == tx_id)
                            .expect("reorg mempool out of sync");
                        keys.remove(pos);
                        if keys.is_empty() {
                            self.txs_by_signature.remove(sig);
                        }
                    }
                }
            }
        }
    }

    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_sign_loss)]
    pub fn compact(&mut self) {
        let (old, new) = shrink_hashmap(&mut self.tx_by_key);
        shrink_hashmap(&mut self.txs_by_signature);
        shrink_hashmap(&mut self.tx_by_kernel_set);
        shrink_hashmap(&mut self.txs_by_height);

        if old > new {
            debug!(
                target: LOG_TARGET,
                "Shrunk reorg mempool memory usage ({}/{}) ~{}%",
                new,
                old,
                ((old.saturating_sub(new) as f32 / old as f32) * 100.0).round() as usize
            );
        }
    }
}

#[cfg(test)]
mod test {
    #![allow(clippy::indexing_slicing)]
    use tari_common::configuration::Network;
    use tari_transaction_components::{MicroMinotari, key_manager::KeyManager, tx};

    use super::*;
    use crate::{consensus::BaseNodeConsensusManagerBuilder, test_helpers::create_orphan_block};
    #[tokio::test]
    async fn test_insert_expire_by_height() {
        let key_manager = KeyManager::new_random().unwrap();
        let tx1 = Arc::new(
            tx!(MicroMinotari(100_000), fee: MicroMinotari(100), lock: 4000, inputs: 2, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0,
        );
        let tx2 = Arc::new(
            tx!(MicroMinotari(100_000), fee: MicroMinotari(60), lock: 3000, inputs: 2, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0,
        );
        let tx3 = Arc::new(
            tx!(MicroMinotari(100_000), fee: MicroMinotari(20), lock: 2500, inputs: 2, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0,
        );
        let tx4 = Arc::new(
            tx!(MicroMinotari(100_000), fee: MicroMinotari(40), lock: 1000, inputs: 2, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0,
        );
        let tx5 = Arc::new(
            tx!(MicroMinotari(100_000), fee: MicroMinotari(100), lock: 2000, inputs: 2, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0,
        );
        let tx6 = Arc::new(
            tx!(MicroMinotari(100_000), fee: MicroMinotari(120), lock: 5500, inputs: 2, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0,
        );

        let mut reorg_pool = ReorgPool::new(ReorgPoolConfig { expiry_height: 2 });
        reorg_pool.insert(1, tx1.clone());
        reorg_pool.insert(2, tx2.clone());

        assert!(reorg_pool.has_tx_with_excess_sig(&tx1.body.kernels()[0].excess_sig));
        assert!(reorg_pool.has_tx_with_excess_sig(&tx2.body.kernels()[0].excess_sig));

        reorg_pool.insert(3, tx3.clone());
        reorg_pool.insert(4, tx4.clone());
        // Check that oldest utx was removed to make room for new incoming transactions
        assert!(!reorg_pool.has_tx_with_excess_sig(&tx1.body.kernels()[0].excess_sig));
        assert!(!reorg_pool.has_tx_with_excess_sig(&tx2.body.kernels()[0].excess_sig));
        assert!(reorg_pool.has_tx_with_excess_sig(&tx3.body.kernels()[0].excess_sig));
        assert!(reorg_pool.has_tx_with_excess_sig(&tx4.body.kernels()[0].excess_sig));

        reorg_pool.insert(5, tx5.clone());
        reorg_pool.insert(6, tx6.clone());
        assert_eq!(reorg_pool.len(), 2);
        assert!(!reorg_pool.has_tx_with_excess_sig(&tx1.body.kernels()[0].excess_sig));
        assert!(!reorg_pool.has_tx_with_excess_sig(&tx2.body.kernels()[0].excess_sig));
        assert!(!reorg_pool.has_tx_with_excess_sig(&tx3.body.kernels()[0].excess_sig));
        assert!(!reorg_pool.has_tx_with_excess_sig(&tx4.body.kernels()[0].excess_sig));
        assert!(reorg_pool.has_tx_with_excess_sig(&tx5.body.kernels()[0].excess_sig));
        assert!(reorg_pool.has_tx_with_excess_sig(&tx6.body.kernels()[0].excess_sig));
    }

    #[tokio::test]
    async fn test_remove_all() {
        let key_manager = KeyManager::new_random().unwrap();
        let tx1 = Arc::new(
            tx!(MicroMinotari(100_000), fee: MicroMinotari(100), lock: 4000, inputs: 2, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0,
        );
        let tx2 = Arc::new(
            tx!(MicroMinotari(100_000), fee: MicroMinotari(60), lock: 3000, inputs: 2, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0,
        );
        let tx3 = Arc::new(
            tx!(MicroMinotari(100_000), fee: MicroMinotari(20), lock: 2500, inputs: 2, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0,
        );

        let mut reorg_pool = ReorgPool::new(ReorgPoolConfig { expiry_height: 2 });
        reorg_pool.insert(1, tx1.clone());
        reorg_pool.insert(1, tx2.clone());
        reorg_pool.insert(1, tx3.clone());

        let txs = reorg_pool.clear_and_retrieve_all();
        assert!(!reorg_pool.has_tx_with_excess_sig(&tx1.body.kernels()[0].excess_sig));
        assert!(!reorg_pool.has_tx_with_excess_sig(&tx2.body.kernels()[0].excess_sig));
        assert!(!reorg_pool.has_tx_with_excess_sig(&tx3.body.kernels()[0].excess_sig));
        assert!(reorg_pool.txs_by_height.is_empty());
        assert!(reorg_pool.tx_by_key.is_empty());
        assert!(reorg_pool.txs_by_signature.is_empty());

        assert!(txs.contains(&tx1));
        assert!(txs.contains(&tx2));
        assert!(txs.contains(&tx3));
    }

    #[tokio::test]
    async fn remove_scan_for_and_remove_reorged_txs() {
        let key_manager = KeyManager::new_random().unwrap();
        let network = Network::LocalNet;
        let consensus = BaseNodeConsensusManagerBuilder::new(network).build().unwrap();
        let tx1 = Arc::new(
            tx!(MicroMinotari(10_000), fee: MicroMinotari(10), lock: 4000, inputs: 2, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0,
        );
        let tx2 = Arc::new(
            tx!(MicroMinotari(10_000), fee: MicroMinotari(6), lock: 3000, inputs: 2, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0,
        );
        let tx3 = Arc::new(
            tx!(MicroMinotari(10_000), fee: MicroMinotari(4), lock: 2500, inputs: 2, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0,
        );
        let tx4 = Arc::new(
            tx!(MicroMinotari(10_000), fee: MicroMinotari(4), lock: 1000, inputs: 2, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0,
        );
        let tx5 = Arc::new(
            tx!(MicroMinotari(10_000), fee: MicroMinotari(10), lock: 2000, inputs: 2, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0,
        );
        let tx6 = Arc::new(
            tx!(MicroMinotari(10_000), fee: MicroMinotari(12), lock: 5500, inputs: 2, outputs: 1, &key_manager)
                .expect("Failed to get tx")
                .0,
        );

        let mut reorg_pool = ReorgPool::new(ReorgPoolConfig { expiry_height: 10 });
        reorg_pool.insert_all(1, vec![
            tx1.clone(),
            tx2.clone(),
            tx3.clone(),
            tx4.clone(),
            tx5.clone(),
            tx6.clone(),
        ]);
        // Oldest transaction tx1 is removed to make space for new incoming transactions
        assert_eq!(reorg_pool.len(), 6);
        assert!(reorg_pool.has_tx_with_excess_sig(&tx1.body.kernels()[0].excess_sig));
        assert!(reorg_pool.has_tx_with_excess_sig(&tx2.body.kernels()[0].excess_sig));
        assert!(reorg_pool.has_tx_with_excess_sig(&tx3.body.kernels()[0].excess_sig));
        assert!(reorg_pool.has_tx_with_excess_sig(&tx4.body.kernels()[0].excess_sig));
        assert!(reorg_pool.has_tx_with_excess_sig(&tx5.body.kernels()[0].excess_sig));
        assert!(reorg_pool.has_tx_with_excess_sig(&tx6.body.kernels()[0].excess_sig));

        let reorg_blocks = &[
            create_orphan_block(3000, vec![(*tx3).clone(), (*tx4).clone()], &consensus).into(),
            create_orphan_block(4000, vec![(*tx1).clone(), (*tx2).clone()], &consensus).into(),
        ];

        let removed_txs = reorg_pool.remove_reorged_txs_and_discard_double_spends(reorg_blocks, &[]);
        assert_eq!(removed_txs.len(), 4);
        assert!(removed_txs.contains(&tx1));
        assert!(removed_txs.contains(&tx2));
        assert!(removed_txs.contains(&tx3));
        assert!(removed_txs.contains(&tx4));

        assert_eq!(reorg_pool.len(), 2);
        assert!(!reorg_pool.has_tx_with_excess_sig(&tx1.body.kernels()[0].excess_sig));
        assert!(!reorg_pool.has_tx_with_excess_sig(&tx2.body.kernels()[0].excess_sig));
        assert!(!reorg_pool.has_tx_with_excess_sig(&tx3.body.kernels()[0].excess_sig));
        assert!(!reorg_pool.has_tx_with_excess_sig(&tx4.body.kernels()[0].excess_sig));
        assert!(reorg_pool.has_tx_with_excess_sig(&tx5.body.kernels()[0].excess_sig));
        assert!(reorg_pool.has_tx_with_excess_sig(&tx6.body.kernels()[0].excess_sig));
    }

    #[test]
    fn only_a_transaction_with_the_same_kernel_set_is_a_duplicate() {
        let key_manager = KeyManager::new_random().unwrap();
        let h = tx!(MicroMinotari(100_000), fee: MicroMinotari(5), inputs: 1, outputs: 1, &key_manager)
            .unwrap()
            .0;
        let x = tx!(MicroMinotari(100_000), fee: MicroMinotari(5), inputs: 1, outputs: 1, &key_manager)
            .unwrap()
            .0;
        #[allow(clippy::arithmetic_side_effects)] // Transaction aggregation, not integer arithmetic
        let mut a = h.clone() + x.clone();
        a.body.sort();

        for (first, second) in [(&a, &h), (&h, &a)] {
            let mut reorg_pool = ReorgPool::new(ReorgPoolConfig { expiry_height: 10 });
            reorg_pool.insert(1, Arc::new(first.clone()));
            assert!(reorg_pool.has_transaction(first));
            assert!(!reorg_pool.has_transaction(second));
            assert!(!reorg_pool.has_transaction(&x));
            reorg_pool.insert(1, Arc::new(first.clone()));
            assert_eq!(reorg_pool.len(), 1);

            reorg_pool.insert(1, Arc::new(second.clone()));
            assert_eq!(reorg_pool.len(), 2);
            assert!(reorg_pool.has_transaction(second));
        }
    }

    fn kernel_set_index_is_consistent(pool: &ReorgPool) -> bool {
        pool.tx_by_kernel_set.len() == pool.tx_by_key.len() &&
            pool.tx_by_key
                .iter()
                .all(|(id, tx)| kernel_set(tx).is_some_and(|set| pool.tx_by_kernel_set.get(&set) == Some(id)))
    }

    #[test]
    fn the_kernel_set_index_follows_every_removal() {
        let key_manager = KeyManager::new_random().unwrap();
        let consensus = BaseNodeConsensusManagerBuilder::new(Network::LocalNet).build().unwrap();
        let txs = (0..4)
            .map(|_| {
                Arc::new(
                    tx!(MicroMinotari(10_000), fee: MicroMinotari(5), inputs: 1, outputs: 1, &key_manager)
                        .unwrap()
                        .0,
                )
            })
            .collect::<Vec<_>>();
        let mut reorg_pool = ReorgPool::new(ReorgPoolConfig { expiry_height: 2 });
        reorg_pool.insert(1, txs[0].clone());
        for tx in &txs[1..] {
            reorg_pool.insert(2, tx.clone());
        }
        assert!(kernel_set_index_is_consistent(&reorg_pool));

        // Removed with a reorged block
        let removed = create_orphan_block(2000, vec![(*txs[1]).clone()], &consensus).into();
        reorg_pool.remove_reorged_txs_and_discard_double_spends(&[removed], &[]);
        // A double spend of a new block
        let added = create_orphan_block(2000, vec![(*txs[2]).clone()], &consensus).into();
        reorg_pool.remove_reorged_txs_and_discard_double_spends(&[], &[added]);
        // Expired
        reorg_pool.insert_all(3, vec![]);
        assert_eq!(reorg_pool.len(), 1);
        assert!(reorg_pool.has_transaction(&txs[3]));
        assert!(kernel_set_index_is_consistent(&reorg_pool));

        // Removed transactions can be stored again
        reorg_pool.insert_all(3, txs[..3].to_vec());
        assert_eq!(reorg_pool.len(), 4);
        assert!(kernel_set_index_is_consistent(&reorg_pool));

        reorg_pool.clear_and_retrieve_all();
        assert!(reorg_pool.tx_by_kernel_set.is_empty());
    }

    #[test]
    fn retrieve_by_excess_sigs_treats_unmatched_index_entry_as_not_found() {
        let mut reorg_pool = ReorgPool::new(ReorgPoolConfig { expiry_height: 10 });
        let dangling_sig = PrivateKey::from(42u64);
        reorg_pool.txs_by_signature.insert(dangling_sig.clone(), vec![999]);

        let (found, remaining) = reorg_pool
            .retrieve_by_excess_sigs(std::slice::from_ref(&dangling_sig))
            .unwrap();
        assert!(found.is_empty());
        assert_eq!(remaining, vec![dangling_sig]);
    }

    #[tokio::test]
    async fn remove_reorged_txs_cleans_up_all_signatures_for_partially_matched_multi_kernel_tx() {
        let key_manager = KeyManager::new_random().unwrap();
        let network = Network::LocalNet;
        let consensus = BaseNodeConsensusManagerBuilder::new(network).build().unwrap();
        let tx1 = tx!(MicroMinotari(10_000), fee: MicroMinotari(10), lock: 4000, inputs: 2, outputs: 1, &key_manager)
            .expect("Failed to get tx")
            .0;
        let tx2 = tx!(MicroMinotari(10_000), fee: MicroMinotari(6), lock: 3000, inputs: 2, outputs: 1, &key_manager)
            .expect("Failed to get tx")
            .0;

        let k1_sig = tx1.body.kernels()[0].excess_sig.clone();
        let k2_sig = tx2.body.kernels()[0].excess_sig.clone();

        let mut agg_inputs = tx1.body.inputs().clone();
        agg_inputs.extend(tx2.body.inputs().iter().cloned());
        let mut agg_outputs = tx1.body.outputs().clone();
        agg_outputs.extend(tx2.body.outputs().iter().cloned());
        let mut agg_kernels = tx1.body.kernels().clone();
        agg_kernels.extend(tx2.body.kernels().iter().cloned());
        #[allow(clippy::arithmetic_side_effects)] // Transaction aggregation, not integer arithmetic
        let mut tx_agg = Transaction::new(
            agg_inputs,
            agg_outputs,
            agg_kernels,
            &tx1.offset + &tx2.offset,
            &tx1.script_offset + &tx2.script_offset,
        );
        tx_agg.body.sort();
        let tx_agg = Arc::new(tx_agg);

        let mut reorg_pool = ReorgPool::new(ReorgPoolConfig { expiry_height: 10 });
        reorg_pool.insert(1, tx_agg.clone());
        assert!(reorg_pool.has_tx_with_excess_sig(&k1_sig));
        assert!(reorg_pool.has_tx_with_excess_sig(&k2_sig));

        let reorg_blocks = &[create_orphan_block(3000, vec![tx1], &consensus).into()];
        let removed_txs = reorg_pool.remove_reorged_txs_and_discard_double_spends(reorg_blocks, &[]);
        assert_eq!(removed_txs, vec![tx_agg]);
        assert!(!reorg_pool.has_tx_with_excess_sig(&k1_sig));
        assert!(!reorg_pool.has_tx_with_excess_sig(&k2_sig));
        assert!(reorg_pool.txs_by_signature.is_empty());
        assert!(kernel_set_index_is_consistent(&reorg_pool));
    }
}
