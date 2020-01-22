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
    consts::{MEMPOOL_UNCONFIRMED_POOL_STORAGE_CAPACITY, MEMPOOL_UNCONFIRMED_POOL_WEIGHT_TRANSACTION_SKIP_COUNT},
    mempool::unconfirmed_pool::{UnconfirmedPoolError, UnconfirmedPoolStorage},
    transactions::{transaction::Transaction, types::Signature},
};
use std::sync::{Arc, RwLock};

/// Configuration for the UnconfirmedPool
#[derive(Clone, Copy)]
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
pub struct UnconfirmedPool {
    pool_storage: Arc<RwLock<UnconfirmedPoolStorage>>,
}

impl UnconfirmedPool {
    /// Create a new UnconfirmedPool with the specified configuration
    pub fn new(config: UnconfirmedPoolConfig) -> Self {
        Self {
            pool_storage: Arc::new(RwLock::new(UnconfirmedPoolStorage::new(config))),
        }
    }

    /// Insert a new transaction into the UnconfirmedPool. Low priority transactions will be removed to make space for
    /// higher priority transactions. The lowest priority transactions will be removed when the maximum capacity is
    /// reached and the new transaction has a higher priority than the currently stored lowest priority transaction.
    pub fn insert(&self, transaction: Arc<Transaction>) -> Result<(), UnconfirmedPoolError> {
        self.pool_storage
            .write()
            .map_err(|_| UnconfirmedPoolError::PoisonedAccess)?
            .insert(transaction)
    }

    ///  Insert a set of new transactions into the UnconfirmedPool
    pub fn insert_txs(&self, transactions: Vec<Arc<Transaction>>) -> Result<(), UnconfirmedPoolError> {
        self.pool_storage
            .write()
            .map_err(|_| UnconfirmedPoolError::PoisonedAccess)?
            .insert_txs(transactions)
    }

    /// Check if a transaction is available in the UnconfirmedPool
    pub fn has_tx_with_excess_sig(&self, excess_sig: &Signature) -> Result<bool, UnconfirmedPoolError> {
        Ok(self
            .pool_storage
            .read()
            .map_err(|_| UnconfirmedPoolError::PoisonedAccess)?
            .has_tx_with_excess_sig(excess_sig))
    }

    /// Returns a set of the highest priority unconfirmed transactions, that can be included in a block
    pub fn highest_priority_txs(&self, total_weight: u64) -> Result<Vec<Arc<Transaction>>, UnconfirmedPoolError> {
        self.pool_storage
            .read()
            .map_err(|_| UnconfirmedPoolError::PoisonedAccess)?
            .highest_priority_txs(total_weight)
    }

    /// Remove all published transactions from the UnconfirmedPool and discard all double spend transactions.
    /// Returns a list of all transactions that were removed the unconfirmed pool as a result of appearing in the block.
    pub fn remove_published_and_discard_double_spends(
        &self,
        published_block: &Block,
    ) -> Result<Vec<Arc<Transaction>>, UnconfirmedPoolError>
    {
        Ok(self
            .pool_storage
            .write()
            .map_err(|_| UnconfirmedPoolError::PoisonedAccess)?
            .remove_published_and_discard_double_spends(published_block))
    }

    /// Returns the total number of unconfirmed transactions stored in the UnconfirmedPool
    pub fn len(&self) -> Result<usize, UnconfirmedPoolError> {
        Ok(self
            .pool_storage
            .read()
            .map_err(|_| UnconfirmedPoolError::PoisonedAccess)?
            .len())
    }

    /// Returns all transaction stored in the UnconfirmedPool.
    pub fn snapshot(&self) -> Result<Vec<Arc<Transaction>>, UnconfirmedPoolError> {
        Ok(self
            .pool_storage
            .read()
            .map_err(|_| UnconfirmedPoolError::PoisonedAccess)?
            .snapshot())
    }

    /// Returns the total weight of all transactions stored in the pool.
    pub fn calculate_weight(&self) -> Result<u64, UnconfirmedPoolError> {
        Ok(self
            .pool_storage
            .read()
            .map_err(|_| UnconfirmedPoolError::PoisonedAccess)?
            .calculate_weight())
    }

    #[cfg(test)]
    /// Checks the consistency status of the Hashmap and BtreeMap
    pub fn check_status(&self) -> Result<bool, UnconfirmedPoolError> {
        Ok(self
            .pool_storage
            .read()
            .map_err(|_| UnconfirmedPoolError::PoisonedAccess)?
            .check_status())
    }
}

impl Clone for UnconfirmedPool {
    fn clone(&self) -> Self {
        UnconfirmedPool {
            pool_storage: self.pool_storage.clone(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{helpers::create_orphan_block, transactions::tari_amount::MicroTari, tx};

    #[test]
    fn test_insert_and_retrieve_highest_priority_txs() {
        let tx1 = Arc::new(tx!(MicroTari(5_000), fee: MicroTari(50), inputs: 2, outputs: 1).0);
        let tx2 = Arc::new(tx!(MicroTari(5_000), fee: MicroTari(20), inputs: 4, outputs: 1).0);
        let tx3 = Arc::new(tx!(MicroTari(5_000), fee: MicroTari(100), inputs: 5, outputs: 1).0);
        let tx4 = Arc::new(tx!(MicroTari(5_000), fee: MicroTari(30), inputs: 3, outputs: 1).0);
        let tx5 = Arc::new(tx!(MicroTari(5_000), fee: MicroTari(50), inputs: 5, outputs: 1).0);

        let unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: 4,
            weight_tx_skip_count: 3,
        });
        unconfirmed_pool
            .insert_txs(vec![tx1.clone(), tx2.clone(), tx3.clone(), tx4.clone(), tx5.clone()])
            .unwrap();
        // Check that lowest priority tx was removed to make room for new incoming transactions
        assert_eq!(
            unconfirmed_pool
                .has_tx_with_excess_sig(&tx1.body.kernels()[0].excess_sig)
                .unwrap(),
            true
        );
        assert_eq!(
            unconfirmed_pool
                .has_tx_with_excess_sig(&tx2.body.kernels()[0].excess_sig)
                .unwrap(),
            false
        );
        assert_eq!(
            unconfirmed_pool
                .has_tx_with_excess_sig(&tx3.body.kernels()[0].excess_sig)
                .unwrap(),
            true
        );
        assert_eq!(
            unconfirmed_pool
                .has_tx_with_excess_sig(&tx4.body.kernels()[0].excess_sig)
                .unwrap(),
            true
        );
        assert_eq!(
            unconfirmed_pool
                .has_tx_with_excess_sig(&tx5.body.kernels()[0].excess_sig)
                .unwrap(),
            true
        );
        // Retrieve the set of highest priority unspent transactions
        let desired_weight = tx1.calculate_weight() + tx3.calculate_weight() + tx4.calculate_weight();
        let selected_txs = unconfirmed_pool.highest_priority_txs(desired_weight).unwrap();
        assert_eq!(selected_txs.len(), 3);
        assert!(selected_txs.contains(&tx1));
        assert!(selected_txs.contains(&tx3));
        assert!(selected_txs.contains(&tx4));
        // Note that transaction tx5 could not be included as its weight was to big to fit into the remaining allocated
        // space, the second best transaction was then included

        assert!(unconfirmed_pool.check_status().unwrap());
    }

    #[test]
    fn test_remove_published_txs() {
        let tx1 = Arc::new(tx!(MicroTari(10_000), fee: MicroTari(50), inputs:2, outputs: 1).0);
        let tx2 = Arc::new(tx!(MicroTari(10_000), fee: MicroTari(20), inputs:3, outputs: 1).0);
        let tx3 = Arc::new(tx!(MicroTari(10_000), fee: MicroTari(100), inputs:2, outputs: 1).0);
        let tx4 = Arc::new(tx!(MicroTari(10_000), fee: MicroTari(30), inputs:4, outputs: 1).0);
        let tx5 = Arc::new(tx!(MicroTari(10_000), fee: MicroTari(50), inputs:3, outputs: 1).0);
        let tx6 = Arc::new(tx!(MicroTari(10_000), fee: MicroTari(75), inputs:2, outputs: 1).0);

        let unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
            storage_capacity: 10,
            weight_tx_skip_count: 3,
        });
        unconfirmed_pool
            .insert_txs(vec![tx1.clone(), tx2.clone(), tx3.clone(), tx4.clone(), tx5.clone()])
            .unwrap();
        // utx6 should not be added to unconfirmed_pool as it is an unknown transactions that was included in the block
        // by another node

        let snapshot_txs = unconfirmed_pool.snapshot().unwrap();
        assert_eq!(snapshot_txs.len(), 5);
        assert!(snapshot_txs.contains(&tx1));
        assert!(snapshot_txs.contains(&tx2));
        assert!(snapshot_txs.contains(&tx3));
        assert!(snapshot_txs.contains(&tx4));
        assert!(snapshot_txs.contains(&tx5));

        let published_block = create_orphan_block(0, vec![(*tx1).clone(), (*tx3).clone(), (*tx5).clone()]);
        let _ = unconfirmed_pool.remove_published_and_discard_double_spends(&published_block);

        assert_eq!(
            unconfirmed_pool
                .has_tx_with_excess_sig(&tx1.body.kernels()[0].excess_sig)
                .unwrap(),
            false
        );
        assert_eq!(
            unconfirmed_pool
                .has_tx_with_excess_sig(&tx2.body.kernels()[0].excess_sig)
                .unwrap(),
            true
        );
        assert_eq!(
            unconfirmed_pool
                .has_tx_with_excess_sig(&tx3.body.kernels()[0].excess_sig)
                .unwrap(),
            false
        );
        assert_eq!(
            unconfirmed_pool
                .has_tx_with_excess_sig(&tx4.body.kernels()[0].excess_sig)
                .unwrap(),
            true
        );
        assert_eq!(
            unconfirmed_pool
                .has_tx_with_excess_sig(&tx5.body.kernels()[0].excess_sig)
                .unwrap(),
            false
        );
        assert_eq!(
            unconfirmed_pool
                .has_tx_with_excess_sig(&tx6.body.kernels()[0].excess_sig)
                .unwrap(),
            false
        );

        assert!(unconfirmed_pool.check_status().unwrap());
    }

    #[test]
    fn test_discard_double_spend_txs() {
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

        let unconfirmed_pool = UnconfirmedPool::new(UnconfirmedPoolConfig {
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
        let published_block = create_orphan_block(0, vec![(*tx1).clone(), (*tx2).clone(), (*tx3).clone()]);

        let _ = unconfirmed_pool
            .remove_published_and_discard_double_spends(&published_block)
            .unwrap(); // Double spends are discarded

        assert_eq!(
            unconfirmed_pool
                .has_tx_with_excess_sig(&tx1.body.kernels()[0].excess_sig)
                .unwrap(),
            false
        );
        assert_eq!(
            unconfirmed_pool
                .has_tx_with_excess_sig(&tx2.body.kernels()[0].excess_sig)
                .unwrap(),
            false
        );
        assert_eq!(
            unconfirmed_pool
                .has_tx_with_excess_sig(&tx3.body.kernels()[0].excess_sig)
                .unwrap(),
            false
        );
        assert_eq!(
            unconfirmed_pool
                .has_tx_with_excess_sig(&tx4.body.kernels()[0].excess_sig)
                .unwrap(),
            true
        );
        assert_eq!(
            unconfirmed_pool
                .has_tx_with_excess_sig(&tx5.body.kernels()[0].excess_sig)
                .unwrap(),
            false
        );
        assert_eq!(
            unconfirmed_pool
                .has_tx_with_excess_sig(&tx6.body.kernels()[0].excess_sig)
                .unwrap(),
            false
        );

        assert!(unconfirmed_pool.check_status().unwrap());
    }
}
