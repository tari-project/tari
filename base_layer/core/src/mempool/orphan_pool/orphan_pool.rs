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
    chain_storage::{BlockchainBackend, BlockchainDatabase},
    consts::{MEMPOOL_ORPHAN_POOL_CACHE_TTL, MEMPOOL_ORPHAN_POOL_STORAGE_CAPACITY},
    mempool::orphan_pool::{error::OrphanPoolError, orphan_pool_storage::OrphanPoolStorage},
};
use std::{
    sync::{Arc, RwLock},
    time::Duration,
};
use tari_transactions::{transaction::Transaction, types::Signature};

/// Configuration for the OrphanPool
#[derive(Clone, Copy)]
pub struct OrphanPoolConfig {
    /// The maximum number of transactions that can be stored in the Orphan pool
    pub storage_capacity: usize,
    /// The Time-to-live for each stored transaction
    pub tx_ttl: Duration,
}

impl Default for OrphanPoolConfig {
    fn default() -> Self {
        Self {
            storage_capacity: MEMPOOL_ORPHAN_POOL_STORAGE_CAPACITY,
            tx_ttl: MEMPOOL_ORPHAN_POOL_CACHE_TTL,
        }
    }
}

/// The Orphan Pool contains all the received transactions that attempt to spend UTXOs that don't exist. These UTXOs
/// might exist in the future if these transactions are from a series or set of transactions that need to be processed
/// in a specific order. Some of these transactions might still be constrained by pending time-locks.
pub struct OrphanPool<T>
where T: BlockchainBackend
{
    pool_storage: Arc<RwLock<OrphanPoolStorage<T>>>,
}

impl<T> OrphanPool<T>
where T: BlockchainBackend
{
    /// Create a new OrphanPool with the specified configuration
    pub fn new(blockchain_db: BlockchainDatabase<T>, config: OrphanPoolConfig) -> Self {
        Self {
            pool_storage: Arc::new(RwLock::new(OrphanPoolStorage::new(blockchain_db, config))),
        }
    }

    /// Insert a new transaction into the OrphanPool. Orphaned transactions will have a limited Time-to-live and will be
    /// discarded if the UTXOs they require are not created before the Time-to-live threshold is reached.
    pub fn insert(&self, transaction: Arc<Transaction>) -> Result<(), OrphanPoolError> {
        Ok(self
            .pool_storage
            .write()
            .map_err(|_| OrphanPoolError::PoisonedAccess)?
            .insert(transaction))
    }

    #[cfg(test)]
    /// Insert a set of new transactions into the OrphanPool
    pub fn insert_txs(&self, transactions: Vec<Arc<Transaction>>) -> Result<(), OrphanPoolError> {
        Ok(self
            .pool_storage
            .write()
            .map_err(|_| OrphanPoolError::PoisonedAccess)?
            .insert_txs(transactions))
    }

    /// Check if a transaction is stored in the OrphanPool
    pub fn has_tx_with_excess_sig(&self, excess_sig: &Signature) -> Result<bool, OrphanPoolError> {
        Ok(self
            .pool_storage
            .read()
            .map_err(|_| OrphanPoolError::PoisonedAccess)?
            .has_tx_with_excess_sig(excess_sig))
    }

    /// Check if the required UTXOs have been created and if the status of any of the transactions in the OrphanPool has
    /// changed. Remove valid transactions and valid transactions with time-locks from the OrphanPool.
    pub fn scan_for_and_remove_unorphaned_txs(
        &self,
    ) -> Result<(Vec<Arc<Transaction>>, Vec<Arc<Transaction>>), OrphanPoolError> {
        self.pool_storage
            .write()
            .map_err(|_| OrphanPoolError::PoisonedAccess)?
            .scan_for_and_remove_unorphaned_txs()
    }

    /// Returns the total number of orphaned transactions stored in the OrphanPool
    pub fn len(&self) -> Result<usize, OrphanPoolError> {
        Ok(self
            .pool_storage
            .write()
            .map_err(|_| OrphanPoolError::PoisonedAccess)?
            .len())
    }

    /// Returns all transaction stored in the OrphanPool.
    pub fn snapshot(&self) -> Result<Vec<Arc<Transaction>>, OrphanPoolError> {
        Ok(self
            .pool_storage
            .write()
            .map_err(|_| OrphanPoolError::PoisonedAccess)?
            .snapshot())
    }

    /// Returns the total weight of all transactions stored in the pool.
    pub fn calculate_weight(&self) -> Result<u64, OrphanPoolError> {
        Ok(self
            .pool_storage
            .write()
            .map_err(|_| OrphanPoolError::PoisonedAccess)?
            .calculate_weight())
    }
}

impl<T> Clone for OrphanPool<T>
where T: BlockchainBackend
{
    fn clone(&self) -> Self {
        OrphanPool {
            pool_storage: self.pool_storage.clone(),
        }
    }
}

#[cfg(test)]
mod test {
    use crate::{
        helpers::create_mem_db,
        mempool::orphan_pool::{OrphanPool, OrphanPoolConfig},
    };
    use std::{sync::Arc, thread, time::Duration};
    use tari_transactions::{
        tari_amount::{uT, MicroTari, T},
        transaction::OutputFeatures,
        tx,
    };

    #[test]
    fn test_insert_rlu_and_ttl() {
        let tx1 = Arc::new(tx!(MicroTari(10_000), fee: MicroTari(500), lock: 4000, inputs: 2, outputs: 1).0);
        let tx2 = Arc::new(tx!(MicroTari(10_000), fee: MicroTari(300), lock: 3000, inputs: 2, outputs: 1).0);
        let tx3 = Arc::new(tx!(MicroTari(10_000), fee: MicroTari(100), lock: 2500, inputs: 2, outputs: 1).0);
        let tx4 = Arc::new(tx!(MicroTari(10_000), fee: MicroTari(200), lock: 1000, inputs: 2, outputs: 1).0);
        let tx5 = Arc::new(tx!(MicroTari(10_000), fee: MicroTari(500), lock: 2000, inputs: 2, outputs: 1).0);
        let tx6 = Arc::new(tx!(MicroTari(10_000), fee: MicroTari(600), lock: 5500, inputs: 2, outputs: 1).0);

        let store = create_mem_db();
        let orphan_pool = OrphanPool::new(store, OrphanPoolConfig {
            storage_capacity: 3,
            tx_ttl: Duration::from_millis(50),
        });
        orphan_pool
            .insert_txs(vec![tx1.clone(), tx2.clone(), tx3.clone(), tx4.clone()])
            .unwrap();
        // Check that oldest utx was removed to make room for new incoming transaction
        assert_eq!(
            orphan_pool
                .has_tx_with_excess_sig(&tx1.body.kernels()[0].excess_sig)
                .unwrap(),
            false
        );
        assert_eq!(
            orphan_pool
                .has_tx_with_excess_sig(&tx2.body.kernels()[0].excess_sig)
                .unwrap(),
            true
        );
        assert_eq!(
            orphan_pool
                .has_tx_with_excess_sig(&tx3.body.kernels()[0].excess_sig)
                .unwrap(),
            true
        );
        assert_eq!(
            orphan_pool
                .has_tx_with_excess_sig(&tx4.body.kernels()[0].excess_sig)
                .unwrap(),
            true
        );

        let snapshot_txs = orphan_pool.snapshot().unwrap();
        assert_eq!(snapshot_txs.len(), 3);
        assert!(snapshot_txs.contains(&tx2));
        assert!(snapshot_txs.contains(&tx3));
        assert!(snapshot_txs.contains(&tx4));

        // Check that transactions that have been in the pool for longer than their Time-to-live have been removed
        thread::sleep(Duration::from_millis(51));
        orphan_pool.insert_txs(vec![tx5.clone(), tx6.clone()]).unwrap();
        assert_eq!(orphan_pool.len().unwrap(), 2);
        assert_eq!(
            orphan_pool
                .has_tx_with_excess_sig(&tx1.body.kernels()[0].excess_sig)
                .unwrap(),
            false
        );
        assert_eq!(
            orphan_pool
                .has_tx_with_excess_sig(&tx2.body.kernels()[0].excess_sig)
                .unwrap(),
            false
        );
        assert_eq!(
            orphan_pool
                .has_tx_with_excess_sig(&tx3.body.kernels()[0].excess_sig)
                .unwrap(),
            false
        );
        assert_eq!(
            orphan_pool
                .has_tx_with_excess_sig(&tx4.body.kernels()[0].excess_sig)
                .unwrap(),
            false
        );
        assert_eq!(
            orphan_pool
                .has_tx_with_excess_sig(&tx5.body.kernels()[0].excess_sig)
                .unwrap(),
            true
        );
        assert_eq!(
            orphan_pool
                .has_tx_with_excess_sig(&tx6.body.kernels()[0].excess_sig)
                .unwrap(),
            true
        );
    }
}
