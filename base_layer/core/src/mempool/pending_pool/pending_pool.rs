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
    blocks::block::Block,
    consts::MEMPOOL_PENDING_POOL_STORAGE_CAPACITY,
    mempool::pending_pool::{PendingPoolError, PendingPoolStorage},
    transaction::Transaction,
    types::Signature,
};
use std::sync::{Arc, RwLock};

/// Configuration for the PendingPool.
#[derive(Clone, Copy)]
pub struct PendingPoolConfig {
    /// The maximum number of transactions that can be stored in the Pending pool.
    pub storage_capacity: usize,
}

impl Default for PendingPoolConfig {
    fn default() -> Self {
        Self {
            storage_capacity: MEMPOOL_PENDING_POOL_STORAGE_CAPACITY,
        }
    }
}

/// The Pending Pool contains all transactions that are restricted by time-locks. Once the time-locks have expired then
/// the transactions can be moved to the Unconfirmed Transaction Pool for inclusion in future blocks.
pub struct PendingPool {
    pool_storage: RwLock<PendingPoolStorage>,
}

impl PendingPool {
    /// Create a new PendingPool with the specified configuration.
    pub fn new(config: PendingPoolConfig) -> Self {
        Self {
            pool_storage: RwLock::new(PendingPoolStorage::new(config)),
        }
    }

    /// Insert a new transaction into the PendingPool. Low priority transactions will be removed to make space for
    /// higher priority transactions. The lowest priority transactions will be removed when the maximum capacity is
    /// reached and the new transaction has a higher priority than the currently stored lowest priority transaction.
    pub fn insert(&mut self, transaction: Transaction) -> Result<(), PendingPoolError> {
        self.pool_storage
            .write()
            .map_err(|_| PendingPoolError::PoisonedAccess)?
            .insert(transaction)
    }

    /// Insert a set of new transactions into the PendingPool.
    pub fn insert_txs(&mut self, transactions: Vec<Transaction>) -> Result<(), PendingPoolError> {
        self.pool_storage
            .write()
            .map_err(|_| PendingPoolError::PoisonedAccess)?
            .insert_txs(transactions)
    }

    /// Check if a specific transaction is available in the PendingPool.
    pub fn has_tx_with_excess_sig(&self, excess_sig: &Signature) -> Result<bool, PendingPoolError> {
        Ok(self
            .pool_storage
            .read()
            .map_err(|_| PendingPoolError::PoisonedAccess)?
            .has_tx_with_excess_sig(excess_sig))
    }

    /// Remove transactions with expired time-locks so that they can be move to the UnconfirmedPool. Double spend
    /// transactions are also removed.
    pub fn remove_unlocked_and_discard_double_spends(
        &mut self,
        published_block: &Block,
    ) -> Result<Vec<Arc<Transaction>>, PendingPoolError>
    {
        self.pool_storage
            .write()
            .map_err(|_| PendingPoolError::PoisonedAccess)?
            .remove_unlocked_and_discard_double_spends(published_block)
    }

    /// Returns the total number of time-locked transactions stored in the PendingPool.
    pub fn len(&self) -> Result<usize, PendingPoolError> {
        Ok(self
            .pool_storage
            .read()
            .map_err(|_| PendingPoolError::PoisonedAccess)?
            .len())
    }

    #[cfg(test)]
    /// Checks the consistency status of the Hashmap and BtreeMaps.
    pub fn check_status(&self) -> Result<bool, PendingPoolError> {
        Ok(self
            .pool_storage
            .read()
            .map_err(|_| PendingPoolError::PoisonedAccess)?
            .check_status())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        tari_amount::MicroTari,
        test_utils::builders::{create_test_block, create_test_tx},
    };

    #[test]
    fn test_insert_and_lru() {
        let tx1 = create_test_tx(MicroTari(10_000), MicroTari(500), 500, 2, 1);
        let tx2 = create_test_tx(MicroTari(10_000), MicroTari(100), 2150, 1, 2);
        let tx3 = create_test_tx(MicroTari(10_000), MicroTari(1000), 1000, 2, 1);
        let tx4 = create_test_tx(MicroTari(10_000), MicroTari(200), 2450, 2, 2);
        let tx5 = create_test_tx(MicroTari(10_000), MicroTari(500), 1000, 3, 3);
        let tx6 = create_test_tx(MicroTari(10_000), MicroTari(750), 1850, 2, 2);

        let mut pending_pool = PendingPool::new(PendingPoolConfig { storage_capacity: 3 });
        pending_pool
            .insert_txs(vec![
                tx1.clone(),
                tx2.clone(),
                tx3.clone(),
                tx4.clone(),
                tx5.clone(),
                tx6.clone(),
            ])
            .unwrap();
        // Check that lowest priority txs were removed to make room for higher priority transactions
        assert_eq!(pending_pool.len().unwrap(), 3);
        assert_eq!(
            pending_pool
                .has_tx_with_excess_sig(&tx1.body.kernels[0].excess_sig)
                .unwrap(),
            true
        );
        assert_eq!(
            pending_pool
                .has_tx_with_excess_sig(&tx2.body.kernels[0].excess_sig)
                .unwrap(),
            false
        );
        assert_eq!(
            pending_pool
                .has_tx_with_excess_sig(&tx3.body.kernels[0].excess_sig)
                .unwrap(),
            true
        );
        assert_eq!(
            pending_pool
                .has_tx_with_excess_sig(&tx4.body.kernels[0].excess_sig)
                .unwrap(),
            false
        );
        assert_eq!(
            pending_pool
                .has_tx_with_excess_sig(&tx5.body.kernels[0].excess_sig)
                .unwrap(),
            false
        );
        assert_eq!(
            pending_pool
                .has_tx_with_excess_sig(&tx6.body.kernels[0].excess_sig)
                .unwrap(),
            true
        );

        assert!(pending_pool.check_status().unwrap());
    }

    #[test]
    fn test_remove_unlocked_and_discard_double_spends() {
        let tx1 = create_test_tx(MicroTari(10_000), MicroTari(500), 500, 2, 1);
        let tx2 = create_test_tx(MicroTari(10_000), MicroTari(100), 2150, 1, 2);
        let tx3 = create_test_tx(MicroTari(10_000), MicroTari(1000), 1000, 2, 1);
        let tx4 = create_test_tx(MicroTari(10_000), MicroTari(200), 2450, 2, 2);
        let tx5 = create_test_tx(MicroTari(10_000), MicroTari(500), 1000, 3, 3);
        let tx6 = create_test_tx(MicroTari(10_000), MicroTari(750), 1450, 2, 2);

        let mut pending_pool = PendingPool::new(PendingPoolConfig { storage_capacity: 10 });
        pending_pool
            .insert_txs(vec![
                tx1.clone(),
                tx2.clone(),
                tx3.clone(),
                tx4.clone(),
                tx5.clone(),
                tx6.clone(),
            ])
            .unwrap();
        assert_eq!(pending_pool.len().unwrap(), 6);

        let published_block = create_test_block(1500, vec![tx6.clone()]);
        let unlocked_txs = pending_pool
            .remove_unlocked_and_discard_double_spends(&published_block)
            .unwrap();

        assert_eq!(pending_pool.len().unwrap(), 2);
        assert_eq!(
            pending_pool
                .has_tx_with_excess_sig(&tx2.body.kernels[0].excess_sig)
                .unwrap(),
            true
        );
        assert_eq!(
            pending_pool
                .has_tx_with_excess_sig(&tx4.body.kernels[0].excess_sig)
                .unwrap(),
            true
        );

        assert_eq!(unlocked_txs.len(), 3);
        assert!(unlocked_txs.iter().any(|tx| **tx == tx1));
        assert!(unlocked_txs.iter().any(|tx| **tx == tx3));
        assert!(unlocked_txs.iter().any(|tx| **tx == tx5));

        assert!(pending_pool.check_status().unwrap());
    }
}
