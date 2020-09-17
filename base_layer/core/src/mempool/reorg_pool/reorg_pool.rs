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
        consts::{MEMPOOL_REORG_POOL_CACHE_TTL, MEMPOOL_REORG_POOL_STORAGE_CAPACITY},
        reorg_pool::{ReorgPoolError, ReorgPoolStorage},
    },
    transactions::{transaction::Transaction, types::Signature},
};
use serde::{Deserialize, Serialize};
use std::{
    sync::{Arc, RwLock},
    time::Duration,
};
use tari_common::configuration::seconds;

/// Configuration for the ReorgPool
#[derive(Clone, Copy, Deserialize, Serialize)]
pub struct ReorgPoolConfig {
    /// The maximum number of transactions that can be stored in the ReorgPool
    pub storage_capacity: usize,
    /// The Time-to-live for each stored transaction
    #[serde(with = "seconds")]
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
    pool_storage: Arc<RwLock<ReorgPoolStorage>>,
}

impl ReorgPool {
    /// Create a new ReorgPool with the specified configuration
    pub fn new(config: ReorgPoolConfig) -> Self {
        Self {
            pool_storage: Arc::new(RwLock::new(ReorgPoolStorage::new(config))),
        }
    }

    /// Insert a set of new transactions into the ReorgPool. Published transactions will have a limited Time-to-live in
    /// the ReorgPool and will be discarded once the Time-to-live threshold has been reached.
    pub fn insert_txs(&self, transactions: Vec<Arc<Transaction>>) -> Result<(), ReorgPoolError> {
        self.pool_storage
            .write()
            .map_err(|e| ReorgPoolError::BackendError(e.to_string()))?
            .insert_txs(transactions);
        Ok(())
    }

    /// Insert a new transaction into the ReorgPool. Published transactions will have a limited Time-to-live in
    /// the ReorgPool and will be discarded once the Time-to-live threshold has been reached.
    pub fn insert(&self, transaction: Arc<Transaction>) -> Result<(), ReorgPoolError> {
        self.pool_storage
            .write()
            .map_err(|e| ReorgPoolError::BackendError(e.to_string()))?
            .insert(transaction);
        Ok(())
    }

    /// Check if a transaction is stored in the ReorgPool
    pub fn has_tx_with_excess_sig(&self, excess_sig: &Signature) -> Result<bool, ReorgPoolError> {
        Ok(self
            .pool_storage
            .read()
            .map_err(|e| ReorgPoolError::BackendError(e.to_string()))?
            .has_tx_with_excess_sig(excess_sig))
    }

    /// Remove the transactions from the ReorgPool that were used in provided removed blocks. The transactions can be
    /// resubmitted to the Unconfirmed Pool.
    pub fn remove_reorged_txs_and_discard_double_spends(
        &self,
        removed_blocks: Vec<Arc<Block>>,
        new_blocks: &[Arc<Block>],
    ) -> Result<Vec<Arc<Transaction>>, ReorgPoolError>
    {
        Ok(self
            .pool_storage
            .write()
            .map_err(|e| ReorgPoolError::BackendError(e.to_string()))?
            .remove_reorged_txs_and_discard_double_spends(removed_blocks, new_blocks))
    }

    /// Returns the total number of published transactions stored in the ReorgPool
    pub fn len(&self) -> Result<usize, ReorgPoolError> {
        Ok(self
            .pool_storage
            .write()
            .map_err(|e| ReorgPoolError::BackendError(e.to_string()))?
            .len())
    }

    /// Returns all transaction stored in the ReorgPool.
    pub fn snapshot(&self) -> Result<Vec<Arc<Transaction>>, ReorgPoolError> {
        Ok(self
            .pool_storage
            .write()
            .map_err(|e| ReorgPoolError::BackendError(e.to_string()))?
            .snapshot())
    }

    /// Returns the total weight of all transactions stored in the pool.
    pub fn calculate_weight(&self) -> Result<u64, ReorgPoolError> {
        Ok(self
            .pool_storage
            .write()
            .map_err(|e| ReorgPoolError::BackendError(e.to_string()))?
            .calculate_weight())
    }
}

impl Clone for ReorgPool {
    fn clone(&self) -> Self {
        ReorgPool {
            pool_storage: self.pool_storage.clone(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{consensus::Network, helpers::create_orphan_block, transactions::tari_amount::MicroTari, tx};
    use std::{thread, time::Duration};

    #[test]
    fn test_insert_rlu_and_ttl() {
        let tx1 = Arc::new(tx!(MicroTari(100_000), fee: MicroTari(500), lock: 4000, inputs: 2, outputs: 1).0);
        let tx2 = Arc::new(tx!(MicroTari(100_000), fee: MicroTari(300), lock: 3000, inputs: 2, outputs: 1).0);
        let tx3 = Arc::new(tx!(MicroTari(100_000), fee: MicroTari(100), lock: 2500, inputs: 2, outputs: 1).0);
        let tx4 = Arc::new(tx!(MicroTari(100_000), fee: MicroTari(200), lock: 1000, inputs: 2, outputs: 1).0);
        let tx5 = Arc::new(tx!(MicroTari(100_000), fee: MicroTari(500), lock: 2000, inputs: 2, outputs: 1).0);
        let tx6 = Arc::new(tx!(MicroTari(100_000), fee: MicroTari(600), lock: 5500, inputs: 2, outputs: 1).0);

        let reorg_pool = ReorgPool::new(ReorgPoolConfig {
            storage_capacity: 3,
            tx_ttl: Duration::from_millis(50),
        });
        reorg_pool
            .insert_txs(vec![tx1.clone(), tx2.clone(), tx3.clone(), tx4.clone()])
            .unwrap();
        // Check that oldest utx was removed to make room for new incoming transactions
        assert_eq!(
            reorg_pool
                .has_tx_with_excess_sig(&tx1.body.kernels()[0].excess_sig)
                .unwrap(),
            false
        );
        assert_eq!(
            reorg_pool
                .has_tx_with_excess_sig(&tx2.body.kernels()[0].excess_sig)
                .unwrap(),
            true
        );
        assert_eq!(
            reorg_pool
                .has_tx_with_excess_sig(&tx3.body.kernels()[0].excess_sig)
                .unwrap(),
            true
        );
        assert_eq!(
            reorg_pool
                .has_tx_with_excess_sig(&tx4.body.kernels()[0].excess_sig)
                .unwrap(),
            true
        );

        // Check that transactions that have been in the pool for longer than their Time-to-live have been removed
        thread::sleep(Duration::from_millis(51));
        reorg_pool.insert_txs(vec![tx5.clone(), tx6.clone()]).unwrap();
        assert_eq!(reorg_pool.len().unwrap(), 2);
        assert_eq!(
            reorg_pool
                .has_tx_with_excess_sig(&tx1.body.kernels()[0].excess_sig)
                .unwrap(),
            false
        );
        assert_eq!(
            reorg_pool
                .has_tx_with_excess_sig(&tx2.body.kernels()[0].excess_sig)
                .unwrap(),
            false
        );
        assert_eq!(
            reorg_pool
                .has_tx_with_excess_sig(&tx3.body.kernels()[0].excess_sig)
                .unwrap(),
            false
        );
        assert_eq!(
            reorg_pool
                .has_tx_with_excess_sig(&tx4.body.kernels()[0].excess_sig)
                .unwrap(),
            false
        );
        assert_eq!(
            reorg_pool
                .has_tx_with_excess_sig(&tx5.body.kernels()[0].excess_sig)
                .unwrap(),
            true
        );
        assert_eq!(
            reorg_pool
                .has_tx_with_excess_sig(&tx6.body.kernels()[0].excess_sig)
                .unwrap(),
            true
        );
    }

    #[test]
    fn remove_scan_for_and_remove_reorged_txs() {
        let network = Network::LocalNet;
        let consensus_constants = network.create_consensus_constants();
        let tx1 = Arc::new(tx!(MicroTari(10_000), fee: MicroTari(50), lock: 4000, inputs: 2, outputs: 1).0);
        let tx2 = Arc::new(tx!(MicroTari(10_000), fee: MicroTari(30), lock: 3000, inputs: 2, outputs: 1).0);
        let tx3 = Arc::new(tx!(MicroTari(10_000), fee: MicroTari(20), lock: 2500, inputs: 2, outputs: 1).0);
        let tx4 = Arc::new(tx!(MicroTari(10_000), fee: MicroTari(20), lock: 1000, inputs: 2, outputs: 1).0);
        let tx5 = Arc::new(tx!(MicroTari(10_000), fee: MicroTari(50), lock: 2000, inputs: 2, outputs: 1).0);
        let tx6 = Arc::new(tx!(MicroTari(10_000), fee: MicroTari(60), lock: 5500, inputs: 2, outputs: 1).0);

        let reorg_pool = ReorgPool::new(ReorgPoolConfig {
            storage_capacity: 5,
            tx_ttl: Duration::from_millis(50),
        });
        reorg_pool
            .insert_txs(vec![
                tx1.clone(),
                tx2.clone(),
                tx3.clone(),
                tx4.clone(),
                tx5.clone(),
                tx6.clone(),
            ])
            .unwrap();
        // Oldest transaction tx1 is removed to make space for new incoming transactions
        assert_eq!(reorg_pool.len().unwrap(), 5);
        assert_eq!(
            reorg_pool
                .has_tx_with_excess_sig(&tx1.body.kernels()[0].excess_sig)
                .unwrap(),
            false
        );
        assert_eq!(
            reorg_pool
                .has_tx_with_excess_sig(&tx2.body.kernels()[0].excess_sig)
                .unwrap(),
            true
        );
        assert_eq!(
            reorg_pool
                .has_tx_with_excess_sig(&tx3.body.kernels()[0].excess_sig)
                .unwrap(),
            true
        );
        assert_eq!(
            reorg_pool
                .has_tx_with_excess_sig(&tx4.body.kernels()[0].excess_sig)
                .unwrap(),
            true
        );
        assert_eq!(
            reorg_pool
                .has_tx_with_excess_sig(&tx5.body.kernels()[0].excess_sig)
                .unwrap(),
            true
        );
        assert_eq!(
            reorg_pool
                .has_tx_with_excess_sig(&tx6.body.kernels()[0].excess_sig)
                .unwrap(),
            true
        );

        let reorg_blocks = vec![
            create_orphan_block(3000, vec![(*tx3).clone(), (*tx4).clone()], &consensus_constants).into(),
            create_orphan_block(4000, vec![(*tx1).clone(), (*tx2).clone()], &consensus_constants).into(),
        ];

        let removed_txs = reorg_pool
            .remove_reorged_txs_and_discard_double_spends(reorg_blocks, &vec![])
            .unwrap();
        assert_eq!(removed_txs.len(), 3);
        assert!(removed_txs.contains(&tx2));
        assert!(removed_txs.contains(&tx3));
        assert!(removed_txs.contains(&tx4));

        assert_eq!(reorg_pool.len().unwrap(), 2);
        assert_eq!(
            reorg_pool
                .has_tx_with_excess_sig(&tx1.body.kernels()[0].excess_sig)
                .unwrap(),
            false
        );
        assert_eq!(
            reorg_pool
                .has_tx_with_excess_sig(&tx2.body.kernels()[0].excess_sig)
                .unwrap(),
            false
        );
        assert_eq!(
            reorg_pool
                .has_tx_with_excess_sig(&tx3.body.kernels()[0].excess_sig)
                .unwrap(),
            false
        );
        assert_eq!(
            reorg_pool
                .has_tx_with_excess_sig(&tx4.body.kernels()[0].excess_sig)
                .unwrap(),
            false
        );
        assert_eq!(
            reorg_pool
                .has_tx_with_excess_sig(&tx5.body.kernels()[0].excess_sig)
                .unwrap(),
            true
        );
        assert_eq!(
            reorg_pool
                .has_tx_with_excess_sig(&tx6.body.kernels()[0].excess_sig)
                .unwrap(),
            true
        );
    }
}
