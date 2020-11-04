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
    mempool::{
        consts::{MEMPOOL_ORPHAN_POOL_CACHE_TTL, MEMPOOL_ORPHAN_POOL_STORAGE_CAPACITY},
        orphan_pool::{error::OrphanPoolError, orphan_pool_storage::OrphanPoolStorage},
    },
    transactions::{transaction::Transaction, types::Signature},
    validation::Validator,
};
use serde::{Deserialize, Serialize};
use std::{
    sync::{Arc, RwLock},
    time::Duration,
};
use tari_common::configuration::seconds;

/// Configuration for the OrphanPool
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub struct OrphanPoolConfig {
    /// The maximum number of transactions that can be stored in the Orphan pool
    pub storage_capacity: usize,
    /// The Time-to-live for each stored transaction
    #[serde(with = "seconds")]
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
#[derive(Clone)]
pub struct OrphanPool {
    pool_storage: Arc<RwLock<OrphanPoolStorage>>,
}

impl OrphanPool {
    /// Create a new OrphanPool with the specified configuration
    pub fn new(config: OrphanPoolConfig, validator: Validator<Transaction>) -> Self {
        Self {
            pool_storage: Arc::new(RwLock::new(OrphanPoolStorage::new(config, validator))),
        }
    }

    /// Insert a new transaction into the OrphanPool. Orphaned transactions will have a limited Time-to-live and will be
    /// discarded if the UTXOs they require are not created before the Time-to-live threshold is reached.
    pub fn insert(&self, transaction: Arc<Transaction>) -> Result<(), OrphanPoolError> {
        self.pool_storage
            .write()
            .map_err(|e| OrphanPoolError::BackendError(e.to_string()))?
            .insert(transaction)?;
        Ok(())
    }

    /// Check if a transaction is stored in the OrphanPool
    pub fn has_tx_with_excess_sig(&self, excess_sig: &Signature) -> Result<bool, OrphanPoolError> {
        Ok(self
            .pool_storage
            .read()
            .map_err(|e| OrphanPoolError::BackendError(e.to_string()))?
            .has_tx_with_excess_sig(excess_sig))
    }

    /// Check if the required UTXOs have been created and if the status of any of the transactions in the OrphanPool has
    /// changed. Remove valid transactions and valid transactions with time-locks from the OrphanPool.
    #[allow(clippy::type_complexity)]
    pub fn scan_for_and_remove_unorphaned_txs(
        &self,
    ) -> Result<(Vec<Arc<Transaction>>, Vec<Arc<Transaction>>), OrphanPoolError> {
        self.pool_storage
            .write()
            .map_err(|e| OrphanPoolError::BackendError(e.to_string()))?
            .scan_for_and_remove_unorphaned_txs()
    }

    /// Returns the total number of orphaned transactions stored in the OrphanPool
    pub fn len(&self) -> Result<usize, OrphanPoolError> {
        Ok(self
            .pool_storage
            .write()
            .map_err(|e| OrphanPoolError::BackendError(e.to_string()))?
            .len())
    }

    /// Returns all transaction stored in the OrphanPool.
    pub fn snapshot(&self) -> Result<Vec<Arc<Transaction>>, OrphanPoolError> {
        Ok(self
            .pool_storage
            .write()
            .map_err(|e| OrphanPoolError::BackendError(e.to_string()))?
            .snapshot())
    }

    /// Returns the total weight of all transactions stored in the pool.
    pub fn calculate_weight(&self) -> Result<u64, OrphanPoolError> {
        Ok(self
            .pool_storage
            .write()
            .map_err(|e| OrphanPoolError::BackendError(e.to_string()))?
            .calculate_weight())
    }
}

#[cfg(test)]
mod test {
    use crate::{
        mempool::orphan_pool::{OrphanPool, OrphanPoolConfig},
        transactions::tari_amount::MicroTari,
        tx,
        validation::mocks::MockValidator,
    };
    use std::{sync::Arc, thread, time::Duration};

    #[test]
    fn test_insert_rlu_and_ttl() {
        let tx1 = Arc::new(tx!(MicroTari(100_000), fee: MicroTari(500), lock: 4000, inputs: 2, outputs: 1).0);
        let tx2 = Arc::new(tx!(MicroTari(100_000), fee: MicroTari(300), lock: 3000, inputs: 2, outputs: 1).0);
        let tx3 = Arc::new(tx!(MicroTari(100_000), fee: MicroTari(100), lock: 2500, inputs: 2, outputs: 1).0);
        let tx4 = Arc::new(tx!(MicroTari(100_000), fee: MicroTari(200), lock: 1000, inputs: 2, outputs: 1).0);
        let tx5 = Arc::new(tx!(MicroTari(100_000), fee: MicroTari(500), lock: 2000, inputs: 2, outputs: 1).0);
        let tx6 = Arc::new(tx!(MicroTari(100_000), fee: MicroTari(600), lock: 5500, inputs: 2, outputs: 1).0);
        let mempool_validator = Box::new(MockValidator::new(true));
        let orphan_pool = OrphanPool::new(
            OrphanPoolConfig {
                storage_capacity: 3,
                tx_ttl: Duration::from_millis(50),
            },
            mempool_validator,
        );

        for tx in vec![tx1.clone(), tx2.clone(), tx3.clone(), tx4.clone()] {
            orphan_pool.insert(tx).unwrap();
        }
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
        for tx in vec![tx5.clone(), tx6.clone()] {
            orphan_pool.insert(tx).unwrap();
        }
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
