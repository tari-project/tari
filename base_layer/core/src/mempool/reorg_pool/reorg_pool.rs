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
    consts::{MEMPOOL_REORG_POOL_CACHE_TTL, MEMPOOL_REORG_POOL_STORAGE_CAPACITY},
    transaction::Transaction,
    types::Signature,
};
use std::{sync::Arc, time::Duration};
use ttl_cache::TtlCache;

/// Configuration for the ReorgPool
#[derive(Clone, Copy)]
pub struct ReorgPoolConfig {
    /// The maximum number of transactions that can be stored in the ReorgPool
    pub storage_capacity: usize,
    /// The Time-to-live for each stored transaction
    pub tx_ttl: Duration,
}

impl Default for ReorgPoolConfig {
    fn default() -> Self {
        Self {
            storage_capacity: MEMPOOL_REORG_POOL_STORAGE_CAPACITY,
            tx_ttl: MEMPOOL_REORG_POOL_CACHE_TTL,
        }
    }
}

/// The ReorgPool consists of all transactions that have recently been added to blocks.
/// When a potential blockchain reorganization occurs the transactions can be recovered from the ReorgPool and can be
/// added back into the UnconfirmedPool. Transactions in the ReOrg pool have a limited Time-to-live and will be removed
/// from the pool when the Time-to-live thresholds is reached. Also, when the capacity of the pool has been reached, the
/// oldest transactions will be removed to make space for incoming transactions.
pub struct ReorgPool {
    config: ReorgPoolConfig,
    txs_by_signature: TtlCache<Signature, Arc<Transaction>>,
}

impl ReorgPool {
    /// Create a new ReorgPool with the specified configuration
    pub fn new(config: ReorgPoolConfig) -> Self {
        Self {
            config,
            txs_by_signature: TtlCache::new(config.storage_capacity),
        }
    }

    /// Insert a new transaction into the ReorgPool. Published transactions will have a limited Time-to-live in the
    /// ReorgPool and will be discarded once the Time-to-live threshold has been reached.
    pub fn insert(&mut self, tx: Transaction) {
        let tx_key = tx.body.kernels[0].excess_sig.clone();
        let _ = self.txs_by_signature.insert(tx_key, Arc::new(tx), self.config.tx_ttl);
    }

    /// Insert a set of new transactions into the ReorgPool
    pub fn insert_txs(&mut self, txs: Vec<Transaction>) {
        for tx in txs.into_iter() {
            self.insert(tx);
        }
    }

    /// Check if a transaction is stored in the ReorgPool
    pub fn has_tx_with_excess_sig(&self, excess_sig: &Signature) -> bool {
        self.txs_by_signature.contains_key(excess_sig)
    }

    /// Remove the transactions from the ReorgPool that were used in provided removed blocks. The transactions can be
    /// resubmitted to the Unconfirmed Pool.
    pub fn scan_for_and_remove_reorged_txs(&mut self, removed_blocks: Vec<Block>) -> Vec<Arc<Transaction>> {
        let mut removed_txs: Vec<Arc<Transaction>> = Vec::new();
        for block in &removed_blocks {
            for kernel in &block.body.kernels {
                if let Some(removed_tx) = self.txs_by_signature.remove(&kernel.excess_sig) {
                    removed_txs.push(removed_tx);
                }
            }
        }
        removed_txs
    }

    /// Returns the total number of published transactions stored in the ReorgPool
    pub fn len(&mut self) -> usize {
        let mut count = 0;
        self.txs_by_signature.iter().for_each(|_| count += 1);
        (count)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        tari_amount::MicroTari,
        test_utils::builders::{create_test_block, create_test_tx},
        transaction::TransactionInput,
    };
    use std::{thread, time::Duration};

    #[test]
    fn test_insert_rlu_and_ttl() {
        let tx1 = create_test_tx(MicroTari(10_000), MicroTari(500), 4000, 2, 1);
        let tx2 = create_test_tx(MicroTari(10_000), MicroTari(300), 3000, 2, 1);
        let tx3 = create_test_tx(MicroTari(10_000), MicroTari(100), 2500, 2, 1);
        let tx4 = create_test_tx(MicroTari(10_000), MicroTari(200), 1000, 2, 1);
        let tx5 = create_test_tx(MicroTari(10_000), MicroTari(500), 2000, 2, 1);
        let tx6 = create_test_tx(MicroTari(10_000), MicroTari(600), 5500, 2, 1);

        let mut reorg_pool = ReorgPool::new(ReorgPoolConfig {
            storage_capacity: 3,
            tx_ttl: Duration::from_millis(50),
        });
        reorg_pool.insert_txs(vec![tx1.clone(), tx2.clone(), tx3.clone(), tx4.clone()]);
        // Check that oldest utx was removed to make room for new incoming transactions
        assert_eq!(
            reorg_pool.has_tx_with_excess_sig(&tx1.body.kernels[0].excess_sig),
            false
        );
        assert_eq!(reorg_pool.has_tx_with_excess_sig(&tx2.body.kernels[0].excess_sig), true);
        assert_eq!(reorg_pool.has_tx_with_excess_sig(&tx3.body.kernels[0].excess_sig), true);
        assert_eq!(reorg_pool.has_tx_with_excess_sig(&tx4.body.kernels[0].excess_sig), true);

        // Check that transactions that have been in the pool for longer than their Time-to-live have been removed
        thread::sleep(Duration::from_millis(51));
        reorg_pool.insert_txs(vec![tx5.clone(), tx6.clone()]);
        assert_eq!(
            reorg_pool.has_tx_with_excess_sig(&tx1.body.kernels[0].excess_sig),
            false
        );
        assert_eq!(
            reorg_pool.has_tx_with_excess_sig(&tx2.body.kernels[0].excess_sig),
            false
        );
        assert_eq!(
            reorg_pool.has_tx_with_excess_sig(&tx3.body.kernels[0].excess_sig),
            false
        );
        assert_eq!(
            reorg_pool.has_tx_with_excess_sig(&tx4.body.kernels[0].excess_sig),
            false
        );
        assert_eq!(reorg_pool.has_tx_with_excess_sig(&tx5.body.kernels[0].excess_sig), true);
        assert_eq!(reorg_pool.has_tx_with_excess_sig(&tx6.body.kernels[0].excess_sig), true);
        assert_eq!(reorg_pool.len(), 2);
    }

    #[test]
    fn remove_scan_for_and_remove_reorged_txs() {
        let tx1 = create_test_tx(MicroTari(10_000), MicroTari(500), 4000, 2, 1);
        let tx2 = create_test_tx(MicroTari(10_000), MicroTari(300), 3000, 2, 1);
        let tx3 = create_test_tx(MicroTari(10_000), MicroTari(100), 2500, 2, 1);
        let tx4 = create_test_tx(MicroTari(10_000), MicroTari(200), 1000, 2, 1);
        let tx5 = create_test_tx(MicroTari(10_000), MicroTari(500), 2000, 2, 1);
        let tx6 = create_test_tx(MicroTari(10_000), MicroTari(600), 5500, 2, 1);

        let mut reorg_pool = ReorgPool::new(ReorgPoolConfig {
            storage_capacity: 5,
            tx_ttl: Duration::from_millis(50),
        });
        reorg_pool.insert_txs(vec![
            tx1.clone(),
            tx2.clone(),
            tx3.clone(),
            tx4.clone(),
            tx5.clone(),
            tx6.clone(),
        ]);
        // Oldest transaction tx1 is removed to make space for new incoming transactions
        assert_eq!(reorg_pool.len(), 5);
        assert_eq!(
            reorg_pool.has_tx_with_excess_sig(&tx1.body.kernels[0].excess_sig),
            false
        );
        assert_eq!(reorg_pool.has_tx_with_excess_sig(&tx2.body.kernels[0].excess_sig), true);
        assert_eq!(reorg_pool.has_tx_with_excess_sig(&tx3.body.kernels[0].excess_sig), true);
        assert_eq!(reorg_pool.has_tx_with_excess_sig(&tx4.body.kernels[0].excess_sig), true);
        assert_eq!(reorg_pool.has_tx_with_excess_sig(&tx5.body.kernels[0].excess_sig), true);
        assert_eq!(reorg_pool.has_tx_with_excess_sig(&tx6.body.kernels[0].excess_sig), true);

        let reorg_blocks = vec![
            create_test_block(3000, vec![tx3.clone(), tx4.clone()]),
            create_test_block(4000, vec![tx1.clone(), tx2.clone()]),
        ];

        let removed_txs = reorg_pool.scan_for_and_remove_reorged_txs(reorg_blocks);
        assert_eq!(removed_txs.len(), 3);
        assert!(removed_txs.iter().any(|tx| **tx == tx2));
        assert!(removed_txs.iter().any(|tx| **tx == tx3));
        assert!(removed_txs.iter().any(|tx| **tx == tx4));

        assert_eq!(reorg_pool.len(), 2);
        assert_eq!(
            reorg_pool.has_tx_with_excess_sig(&tx1.body.kernels[0].excess_sig),
            false
        );
        assert_eq!(
            reorg_pool.has_tx_with_excess_sig(&tx2.body.kernels[0].excess_sig),
            false
        );
        assert_eq!(
            reorg_pool.has_tx_with_excess_sig(&tx3.body.kernels[0].excess_sig),
            false
        );
        assert_eq!(
            reorg_pool.has_tx_with_excess_sig(&tx4.body.kernels[0].excess_sig),
            false
        );
        assert_eq!(reorg_pool.has_tx_with_excess_sig(&tx5.body.kernels[0].excess_sig), true);
        assert_eq!(reorg_pool.has_tx_with_excess_sig(&tx6.body.kernels[0].excess_sig), true);
    }
}
