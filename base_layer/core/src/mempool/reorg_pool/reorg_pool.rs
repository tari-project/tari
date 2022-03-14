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
    hash::Hash,
    sync::Arc,
};

use log::*;
use serde::{Deserialize, Serialize};
use tari_common_types::types::{PrivateKey, Signature};
use tari_utilities::{hex::Hex, Hashable};

use crate::{blocks::Block, transactions::transaction_components::Transaction};

pub const LOG_TARGET: &str = "c::mp::reorg_pool::reorg_pool_storage";

/// Configuration for the ReorgPool
#[derive(Clone, Copy, Serialize, Deserialize, Debug)]
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
            txs_by_height: HashMap::new(),
        }
    }

    /// Insert a new transaction into the ReorgPool. Published transactions will be discarded once they are
    /// `config.expiry_height` blocks old.
    fn insert(&mut self, height: u64, tx: Arc<Transaction>) {
        let excess_hex = tx
            .first_kernel_excess_sig()
            .map(|s| s.get_signature().to_hex())
            .unwrap_or_else(|| "no kernel!".to_string());
        if tx
            .body
            .kernels()
            .iter()
            .all(|k| self.txs_by_signature.contains_key(k.excess_sig.get_signature()))
        {
            debug!(
                target: LOG_TARGET,
                "Transaction {} already found in reorg pool", excess_hex
            );
            self.cleanup_expired(height);
            return;
        }

        let new_key = self.get_next_key();
        for kernel in tx.body.kernels() {
            let sig = kernel.excess_sig.get_signature();
            self.txs_by_signature.entry(sig.clone()).or_default().push(new_key);
        }

        trace!(
            target: LOG_TARGET,
            "Inserted transaction {} into reorg pool at height {}",
            new_key,
            height
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
        for tx in txs.into_iter() {
            self.insert(height, tx);
        }
    }

    pub fn retrieve_by_excess_sigs(&self, excess_sigs: &[PrivateKey]) -> (Vec<Arc<Transaction>>, Vec<PrivateKey>) {
        // Hashset used to prevent duplicates
        let mut found = HashSet::new();
        let mut remaining = Vec::new();

        for sig in excess_sigs {
            match self.txs_by_signature.get(sig) {
                Some(ids) => found.extend(ids.iter()),
                None => remaining.push(sig.clone()),
            }
        }

        let found = found
            .into_iter()
            .map(|id| {
                self.tx_by_key
                    .get(id)
                    .expect("mempool indexes out of sync: transaction exists in txs_by_signature but not in tx_by_key")
            })
            .cloned()
            .collect();

        (found, remaining)
    }

    /// Check if a transaction is stored in the ReorgPool
    pub fn has_tx_with_excess_sig(&self, excess_sig: &Signature) -> bool {
        self.txs_by_signature.contains_key(excess_sig.get_signature())
    }

    /// Remove the transactions from the ReorgPoolthat were used in provided removed blocks. The transactions
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
                if let Some(removed_tx_ids) = self.txs_by_signature.remove(kernel.excess_sig.get_signature()) {
                    for tx_id in removed_tx_ids {
                        if let Some(tx) = self.tx_by_key.remove(&tx_id) {
                            self.remove_from_height_index(tx_id);
                            trace!(target: LOG_TARGET, "Removed tx from reorg pool: {:?}", tx_id);
                            removed_txs.push(tx);
                        }
                    }
                }
            }
        }

        removed_txs
    }

    fn remove_from_height_index(&mut self, tx_id: TransactionId) {
        let mut heights_to_remove = Vec::new();
        for (height, ids) in self.txs_by_height.iter_mut() {
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
        for (id, tx) in self.tx_by_key.iter() {
            for input in tx.body.inputs() {
                if published_block.body.inputs().contains(input) {
                    to_remove.push(*id);
                }
            }
        }

        for id in to_remove {
            self.remove(id);
            trace!(target: LOG_TARGET, "Removed double spend tx {} from reorg pool", id);
        }
    }

    fn remove(&mut self, tx_id: TransactionId) -> Option<Arc<Transaction>> {
        let tx = self.tx_by_key.remove(&tx_id)?;

        for kernel in tx.body.kernels() {
            let sig = kernel.excess_sig.get_signature();
            let ids = self.txs_by_signature.get_mut(sig).expect("reorg pool out of sync");
            let pos = ids.iter().position(|k| *k == tx_id).expect("reorg mempool out of sync");
            ids.remove(pos);
            if ids.is_empty() {
                self.txs_by_signature.remove(sig);
            }
        }

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
        self.key_counter = (self.key_counter + 1) % usize::MAX;
        key
    }

    fn cleanup_expired(&mut self, height: u64) {
        let height = match height.checked_sub(self.config.expiry_height) {
            Some(h) => h,
            None => return,
        };

        // let heights_to_remove = self
        //     .txs_by_height
        //     .keys()
        //     .filter(|h| **h <= height)
        //     .copied()
        //     .collect::<Vec<_>>();
        // for height in heights_to_remove {
        if let Some(tx_ids) = self.txs_by_height.remove(&height) {
            debug!(
                target: LOG_TARGET,
                "Clearing {} transactions from mempool for height {}",
                tx_ids.len(),
                height
            );
            for tx_id in tx_ids {
                let tx = self.tx_by_key.remove(&tx_id).expect("reorg mempool out of sync");

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

    pub fn compact(&mut self) {
        fn shrink_hashmap<K: Eq + Hash, V>(map: &mut HashMap<K, V>) -> (usize, usize) {
            let cap = map.capacity();
            let extra_cap = cap - map.len();
            if extra_cap > 100 {
                map.shrink_to(map.len() + (extra_cap / 2));
            }

            (cap, map.capacity())
        }

        let (old, new) = shrink_hashmap(&mut self.tx_by_key);
        shrink_hashmap(&mut self.txs_by_signature);
        shrink_hashmap(&mut self.txs_by_height);

        if old - new > 0 {
            debug!(
                target: LOG_TARGET,
                "Shrunk reorg mempool memory usage ({}/{}) ~{}%",
                new,
                old,
                (((old - new) as f32 / old as f32) * 100.0).round() as usize
            );
        }
    }
}

#[cfg(test)]
mod test {

    use tari_common::configuration::Network;

    use super::*;
    use crate::{
        consensus::ConsensusManagerBuilder,
        test_helpers::create_orphan_block,
        transactions::tari_amount::MicroTari,
        tx,
    };

    #[test]
    fn test_insert_expire_by_height() {
        let tx1 = Arc::new(tx!(MicroTari(100_000), fee: MicroTari(100), lock: 4000, inputs: 2, outputs: 1).0);
        let tx2 = Arc::new(tx!(MicroTari(100_000), fee: MicroTari(60), lock: 3000, inputs: 2, outputs: 1).0);
        let tx3 = Arc::new(tx!(MicroTari(100_000), fee: MicroTari(20), lock: 2500, inputs: 2, outputs: 1).0);
        let tx4 = Arc::new(tx!(MicroTari(100_000), fee: MicroTari(40), lock: 1000, inputs: 2, outputs: 1).0);
        let tx5 = Arc::new(tx!(MicroTari(100_000), fee: MicroTari(100), lock: 2000, inputs: 2, outputs: 1).0);
        let tx6 = Arc::new(tx!(MicroTari(100_000), fee: MicroTari(120), lock: 5500, inputs: 2, outputs: 1).0);

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

    #[test]
    fn remove_scan_for_and_remove_reorged_txs() {
        let network = Network::LocalNet;
        let consensus = ConsensusManagerBuilder::new(network).build();
        let tx1 = Arc::new(tx!(MicroTari(10_000), fee: MicroTari(10), lock: 4000, inputs: 2, outputs: 1).0);
        let tx2 = Arc::new(tx!(MicroTari(10_000), fee: MicroTari(6), lock: 3000, inputs: 2, outputs: 1).0);
        let tx3 = Arc::new(tx!(MicroTari(10_000), fee: MicroTari(4), lock: 2500, inputs: 2, outputs: 1).0);
        let tx4 = Arc::new(tx!(MicroTari(10_000), fee: MicroTari(4), lock: 1000, inputs: 2, outputs: 1).0);
        let tx5 = Arc::new(tx!(MicroTari(10_000), fee: MicroTari(10), lock: 2000, inputs: 2, outputs: 1).0);
        let tx6 = Arc::new(tx!(MicroTari(10_000), fee: MicroTari(12), lock: 5500, inputs: 2, outputs: 1).0);

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
}
