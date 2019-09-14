// Copyright 2019. The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use crate::{
    blocks::Block,
    chain_storage::{BlockchainBackend, BlockchainDatabase},
    mempool::{
        error::MempoolError,
        orphan_pool::{OrphanPool, OrphanPoolConfig},
        pending_pool::{PendingPool, PendingPoolConfig},
        reorg_pool::{ReorgPool, ReorgPoolConfig},
        unconfirmed_pool::{UnconfirmedPool, UnconfirmedPoolConfig},
    },
    transaction::Transaction,
    types::{Signature, COMMITMENT_FACTORY, PROVER},
};
use std::sync::Arc;
use tari_utilities::hash::Hashable;

#[derive(Debug, PartialEq)]
pub enum TxStorageResponse {
    UnconfirmedPool,
    OrphanPool,
    PendingPool,
    ReorgPool,
    NotStored,
}

#[derive(Debug, PartialEq)]
pub struct StatsResponse {
    pub total_txs: usize,
    pub unconfirmed_txs: usize,
    pub orphan_txs: usize,
    pub timelocked_txs: usize,
    pub published_txs: usize,
    pub total_weight: u64,
}

/// Configuration for the Mempool.
#[derive(Clone, Copy)]
pub struct MempoolConfig {
    pub unconfirmed_pool_config: UnconfirmedPoolConfig,
    pub orphan_pool_config: OrphanPoolConfig,
    pub pending_pool_config: PendingPoolConfig,
    pub reorg_pool_config: ReorgPoolConfig,
}

impl Default for MempoolConfig {
    fn default() -> Self {
        Self {
            unconfirmed_pool_config: UnconfirmedPoolConfig::default(),
            orphan_pool_config: OrphanPoolConfig::default(),
            pending_pool_config: PendingPoolConfig::default(),
            reorg_pool_config: ReorgPoolConfig::default(),
        }
    }
}

/// The Mempool consists of an Unconfirmed Transaction Pool, Pending Pool, Orphan Pool and Reorg Pool and is responsible
/// for managing and maintaining all unconfirmed transactions have not yet been included in a block, and transactions
/// that have recently been included in a block.
pub struct Mempool<T>
where T: BlockchainBackend
{
    blockchain_db: Arc<BlockchainDatabase<T>>,
    unconfirmed_pool: UnconfirmedPool,
    orphan_pool: OrphanPool<T>,
    pending_pool: PendingPool,
    reorg_pool: ReorgPool,
}

impl<T> Mempool<T>
where T: BlockchainBackend
{
    /// Create a new Mempool with an UnconfirmedPool, OrphanPool, PendingPool and ReOrgPool.
    pub fn new(blockchain_db: Arc<BlockchainDatabase<T>>, config: MempoolConfig) -> Self {
        Self {
            blockchain_db: blockchain_db.clone(),
            unconfirmed_pool: UnconfirmedPool::new(config.unconfirmed_pool_config),
            orphan_pool: OrphanPool::new(blockchain_db, config.orphan_pool_config),
            pending_pool: PendingPool::new(config.pending_pool_config),
            reorg_pool: ReorgPool::new(config.reorg_pool_config),
        }
    }

    fn check_input_utxos(&mut self, tx: &Transaction) -> Result<bool, MempoolError> {
        for input in &tx.body.inputs {
            if !self.blockchain_db.is_utxo(input.hash())? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn check_timelocks(&mut self, tx: &Transaction) -> Result<bool, MempoolError> {
        Ok(tx.max_timelock_height() >
            self.blockchain_db
                .get_height()?
                .ok_or(MempoolError::ChainHeightUndefined)?)
    }

    /// Insert an unconfirmed transaction into the Mempool.
    pub fn insert(&mut self, tx: Arc<Transaction>) -> Result<(), MempoolError> {
        tx.validate_internal_consistency(&PROVER, &COMMITMENT_FACTORY)?;

        if self.check_input_utxos(&tx)? {
            if self.check_timelocks(&tx)? {
                self.pending_pool.insert(tx)?;
            } else {
                self.unconfirmed_pool.insert(tx)?;
            }
        } else {
            self.orphan_pool.insert(tx)?;
        }

        Ok(())
    }

    /// Insert a set of new transactions into the UTxPool.
    fn insert_txs(&mut self, txs: Vec<Arc<Transaction>>) -> Result<(), MempoolError> {
        for tx in txs {
            self.insert(tx)?;
        }
        Ok(())
    }

    /// Update the Mempool based on the received published block.
    pub fn process_published_block(&mut self, published_block: &Block) -> Result<(), MempoolError> {
        // Move published txs to ReOrgPool and discard double spends
        self.reorg_pool.insert_txs(
            self.unconfirmed_pool
                .remove_published_and_discard_double_spends(published_block)?,
        )?;

        // Move txs with valid input UTXOs and expired time-locks to UnconfirmedPool and discard double spends
        self.unconfirmed_pool.insert_txs(
            self.pending_pool
                .remove_unlocked_and_discard_double_spends(published_block)?,
        )?;

        // Move txs with recently expired time-locks that have input UTXOs that have recently become valid to the
        // UnconfirmedPool
        let (txs, time_locked_txs) = self.orphan_pool.scan_for_and_remove_unorphaned_txs()?;
        self.unconfirmed_pool.insert_txs(txs)?;
        // Move Time-locked txs that have input UTXOs that have recently become valid to PendingPool.
        self.pending_pool.insert_txs(time_locked_txs)?;

        Ok(())
    }

    /// Update the Mempool based on the received set of published blocks.
    pub fn process_published_blocks(&mut self, published_blocks: &Vec<Block>) -> Result<(), MempoolError> {
        for published_block in published_blocks {
            self.process_published_block(published_block)?;
        }
        Ok(())
    }

    /// In the event of a ReOrg, resubmit all ReOrged transactions into the Mempool and process each newly introduced
    /// block from the latest longest chain.
    pub fn process_reorg(&mut self, removed_blocks: Vec<Block>, new_blocks: Vec<Block>) -> Result<(), MempoolError> {
        self.insert_txs(self.reorg_pool.scan_for_and_remove_reorged_txs(removed_blocks)?)?;
        self.process_published_blocks(&new_blocks)?;
        Ok(())
    }

    /// Returns all unconfirmed transaction stored in the Mempool, except the transactions stored in the ReOrgPool.
    // TODO: Investigate returning an iterator rather than a large vector of transactions
    pub fn snapshot(&self) -> Result<Vec<Arc<Transaction>>, MempoolError> {
        let mut txs = self.unconfirmed_pool.snapshot()?;
        txs.append(&mut self.orphan_pool.snapshot()?);
        txs.append(&mut self.pending_pool.snapshot()?);
        Ok(txs)
    }

    /// Returns a list of transaction ranked by transaction priority up to a given weight.
    pub fn retrieve(&self, total_weight: u64) -> Result<Vec<Arc<Transaction>>, MempoolError> {
        Ok(self.unconfirmed_pool.highest_priority_txs(total_weight)?)
    }

    /// Check if the specified transaction is stored in the Mempool.
    pub fn has_tx_with_excess_sig(&self, excess_sig: &Signature) -> Result<TxStorageResponse, MempoolError> {
        if self.unconfirmed_pool.has_tx_with_excess_sig(excess_sig)? {
            Ok(TxStorageResponse::UnconfirmedPool)
        } else if self.orphan_pool.has_tx_with_excess_sig(excess_sig)? {
            Ok(TxStorageResponse::OrphanPool)
        } else if self.pending_pool.has_tx_with_excess_sig(excess_sig)? {
            Ok(TxStorageResponse::PendingPool)
        } else if self.reorg_pool.has_tx_with_excess_sig(excess_sig)? {
            Ok(TxStorageResponse::ReorgPool)
        } else {
            Ok(TxStorageResponse::NotStored)
        }
    }

    // Returns the total number of transactions in the Mempool.
    fn len(&self) -> Result<usize, MempoolError> {
        Ok(
            self.unconfirmed_pool.len()? +
                self.orphan_pool.len()? +
                self.pending_pool.len()? +
                self.reorg_pool.len()?,
        )
    }

    // Returns the total weight of all transactions stored in the Mempool.
    fn calculate_weight(&self) -> Result<u64, MempoolError> {
        Ok(self.unconfirmed_pool.calculate_weight()? +
            self.orphan_pool.calculate_weight()? +
            self.pending_pool.calculate_weight()? +
            self.reorg_pool.calculate_weight()?)
    }

    /// Gathers and returns the stats of the Mempool.
    pub fn stats(&self) -> Result<StatsResponse, MempoolError> {
        Ok(StatsResponse {
            total_txs: self.len()?,
            unconfirmed_txs: self.unconfirmed_pool.len()?,
            orphan_txs: self.orphan_pool.len()?,
            timelocked_txs: self.pending_pool.len()?,
            published_txs: self.reorg_pool.len()?,
            total_weight: self.calculate_weight()?,
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        blocks::genesis_block::get_genesis_block,
        chain_storage::{DbTransaction, MemoryDatabase},
        tari_amount::MicroTari,
        test_utils::builders::{create_test_block, create_test_input, create_test_tx_spending_utxos},
        tx,
        types::HashDigest,
    };

    fn create_test_tx_spending_utxo<T: BlockchainBackend>(
        blockchain_db: &Arc<BlockchainDatabase<T>>,
        orphaned: bool,
        fee_per_gram: MicroTari,
        lock_height: u64,
        input_maturity: u64,
    ) -> Transaction
    {
        if orphaned {
            (tx!( MicroTari(5_000), fee: fee_per_gram, lock: lock_height, inputs: 2, maturity: input_maturity, outputs: 2)).0
        } else {
            let utxo1 = create_test_input(2_500.into(), input_maturity.clone());
            let utxo2 = create_test_input(2_500.into(), input_maturity);

            let mut db_txn = DbTransaction::new();
            db_txn.insert_utxo(
                utxo1
                    .1
                    .as_transaction_output(&PROVER, &COMMITMENT_FACTORY, utxo1.0.features.clone())
                    .unwrap(),
            );
            db_txn.insert_utxo(
                utxo2
                    .1
                    .as_transaction_output(&PROVER, &COMMITMENT_FACTORY, utxo2.0.features.clone())
                    .unwrap(),
            );
            blockchain_db.commit(db_txn).unwrap();

            create_test_tx_spending_utxos(fee_per_gram, lock_height, vec![utxo1, utxo2], 2).0
        }
    }

    #[test]
    fn test_insert_and_process_published_block() {
        let store = Arc::new(BlockchainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap());
        let genesis_block = get_genesis_block();
        store.add_block(genesis_block.clone()).unwrap();
        let mut mempool = Mempool::new(store.clone(), MempoolConfig::default());

        let tx1 = Arc::new(create_test_tx_spending_utxo(&store, true, MicroTari(20), 0, 0));
        let tx2 = Arc::new(create_test_tx_spending_utxo(&store, false, MicroTari(20), 0, 1));
        let tx3 = Arc::new(create_test_tx_spending_utxo(&store, false, MicroTari(20), 2, 1));
        let tx4 = Arc::new(create_test_tx_spending_utxo(&store, false, MicroTari(20), 0, 0));
        let tx5 = Arc::new(create_test_tx_spending_utxo(&store, false, MicroTari(20), 1, 2));
        let tx6 = Arc::new(create_test_tx_spending_utxo(&store, false, MicroTari(20), 3, 2));

        mempool.insert(tx1.clone()).unwrap();
        mempool.insert(tx2.clone()).unwrap();
        mempool.insert(tx3.clone()).unwrap();
        mempool.insert(tx4.clone()).unwrap();
        mempool.insert(tx5.clone()).unwrap();

        let published_block = create_test_block(1, Some(genesis_block), vec![(*tx4).clone()]);
        store.add_block(published_block.clone()).unwrap();
        mempool.process_published_block(&published_block).unwrap();

        assert_eq!(
            mempool.has_tx_with_excess_sig(&tx1.body.kernels[0].excess_sig).unwrap(),
            TxStorageResponse::OrphanPool
        );
        assert_eq!(
            mempool.has_tx_with_excess_sig(&tx2.body.kernels[0].excess_sig).unwrap(),
            TxStorageResponse::UnconfirmedPool
        );
        assert_eq!(
            mempool.has_tx_with_excess_sig(&tx3.body.kernels[0].excess_sig).unwrap(),
            TxStorageResponse::PendingPool
        );
        assert_eq!(
            mempool.has_tx_with_excess_sig(&tx4.body.kernels[0].excess_sig).unwrap(),
            TxStorageResponse::ReorgPool
        );
        assert_eq!(
            mempool.has_tx_with_excess_sig(&tx5.body.kernels[0].excess_sig).unwrap(),
            TxStorageResponse::PendingPool
        );
        assert_eq!(
            mempool.has_tx_with_excess_sig(&tx6.body.kernels[0].excess_sig).unwrap(),
            TxStorageResponse::NotStored
        );

        let snapshot_txs = mempool.snapshot().unwrap();
        assert_eq!(snapshot_txs.len(), 4);
        assert!(snapshot_txs.contains(&tx1));
        assert!(snapshot_txs.contains(&tx2));
        assert!(snapshot_txs.contains(&tx3));
        assert!(snapshot_txs.contains(&tx5));

        let stats = mempool.stats().unwrap();
        assert_eq!(stats.total_txs, 5);
        assert_eq!(stats.unconfirmed_txs, 1);
        assert_eq!(stats.orphan_txs, 1);
        assert_eq!(stats.timelocked_txs, 2);
        assert_eq!(stats.published_txs, 1);
        assert_eq!(stats.total_weight, 50);

        let published_block = create_test_block(2, Some(published_block), vec![(*tx2).clone()]);
        store.add_block(published_block.clone()).unwrap();
        mempool.process_published_block(&published_block).unwrap();

        assert_eq!(
            mempool.has_tx_with_excess_sig(&tx1.body.kernels[0].excess_sig).unwrap(),
            TxStorageResponse::OrphanPool
        );
        assert_eq!(
            mempool.has_tx_with_excess_sig(&tx2.body.kernels[0].excess_sig).unwrap(),
            TxStorageResponse::ReorgPool
        );
        assert_eq!(
            mempool.has_tx_with_excess_sig(&tx3.body.kernels[0].excess_sig).unwrap(),
            TxStorageResponse::UnconfirmedPool
        );
        assert_eq!(
            mempool.has_tx_with_excess_sig(&tx4.body.kernels[0].excess_sig).unwrap(),
            TxStorageResponse::ReorgPool
        );
        assert_eq!(
            mempool.has_tx_with_excess_sig(&tx5.body.kernels[0].excess_sig).unwrap(),
            TxStorageResponse::UnconfirmedPool
        );
        assert_eq!(
            mempool.has_tx_with_excess_sig(&tx6.body.kernels[0].excess_sig).unwrap(),
            TxStorageResponse::NotStored
        );

        let snapshot_txs = mempool.snapshot().unwrap();
        assert_eq!(snapshot_txs.len(), 3);
        assert!(snapshot_txs.contains(&tx1));
        assert!(snapshot_txs.contains(&tx3));
        assert!(snapshot_txs.contains(&tx5));

        let stats = mempool.stats().unwrap();
        assert_eq!(stats.total_txs, 5);
        assert_eq!(stats.unconfirmed_txs, 2);
        assert_eq!(stats.orphan_txs, 1);
        assert_eq!(stats.timelocked_txs, 0);
        assert_eq!(stats.published_txs, 2);
        assert_eq!(stats.total_weight, 50);
    }

    #[test]
    fn test_retrieve() {
        let store = Arc::new(BlockchainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap());
        let genesis_block = get_genesis_block();
        store.add_block(genesis_block.clone()).unwrap();
        let mut mempool = Mempool::new(store.clone(), MempoolConfig::default());

        let tx1 = Arc::new(create_test_tx_spending_utxo(&store, false, MicroTari(30), 0, 0));
        let tx2 = Arc::new(create_test_tx_spending_utxo(&store, false, MicroTari(20), 0, 0));
        let tx3 = Arc::new(create_test_tx_spending_utxo(&store, false, MicroTari(40), 0, 0));
        let tx4 = Arc::new(create_test_tx_spending_utxo(&store, false, MicroTari(50), 0, 0));
        let tx5 = Arc::new(create_test_tx_spending_utxo(&store, false, MicroTari(20), 1, 0));
        let tx6 = Arc::new(create_test_tx_spending_utxo(&store, false, MicroTari(20), 1, 0));
        let tx7 = Arc::new(create_test_tx_spending_utxo(&store, false, MicroTari(60), 0, 1));
        let tx8 = Arc::new(create_test_tx_spending_utxo(&store, false, MicroTari(30), 0, 1));

        mempool.insert(tx1.clone()).unwrap();
        mempool.insert(tx2.clone()).unwrap();
        mempool.insert(tx3.clone()).unwrap();
        mempool.insert(tx4.clone()).unwrap();
        mempool.insert(tx5.clone()).unwrap();
        mempool.insert(tx6.clone()).unwrap();
        mempool.insert(tx7.clone()).unwrap();
        mempool.insert(tx8.clone()).unwrap();

        let weight = tx1.calculate_weight() + tx3.calculate_weight() + tx4.calculate_weight();
        let retrieved_txs = mempool.retrieve(weight).unwrap();
        assert_eq!(retrieved_txs.len(), 3);
        assert!(retrieved_txs.contains(&tx1));
        assert!(retrieved_txs.contains(&tx3));
        assert!(retrieved_txs.contains(&tx4));
        let stats = mempool.stats().unwrap();
        assert_eq!(stats.unconfirmed_txs, 4);
        assert_eq!(stats.timelocked_txs, 4);
        assert_eq!(stats.published_txs, 0);

        let published_block = create_test_block(1, Some(genesis_block), vec![
            (*retrieved_txs[0]).clone(),
            (*retrieved_txs[1]).clone(),
            (*retrieved_txs[2]).clone(),
        ]);
        store.add_block(published_block.clone()).unwrap();
        mempool.process_published_block(&published_block).unwrap();

        let weight = tx7.calculate_weight() + tx8.calculate_weight();
        let retrieved_txs = mempool.retrieve(weight).unwrap();
        assert_eq!(retrieved_txs.len(), 2);
        assert!(retrieved_txs.contains(&tx7));
        assert!(retrieved_txs.contains(&tx8));
        let stats = mempool.stats().unwrap();
        assert_eq!(stats.unconfirmed_txs, 5);
        assert_eq!(stats.timelocked_txs, 0);
        assert_eq!(stats.published_txs, 3);
    }

    #[test]
    fn test_reorg() {
        let store = Arc::new(BlockchainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap());
        let genesis_block = get_genesis_block();
        store.add_block(genesis_block.clone()).unwrap();
        let mut mempool = Mempool::new(store.clone(), MempoolConfig::default());

        let tx1 = Arc::new(create_test_tx_spending_utxo(&store, false, MicroTari(20), 0, 0));
        let tx2 = Arc::new(create_test_tx_spending_utxo(&store, false, MicroTari(20), 0, 0));
        let tx3 = Arc::new(create_test_tx_spending_utxo(&store, false, MicroTari(20), 0, 0));
        let tx4 = Arc::new(create_test_tx_spending_utxo(&store, false, MicroTari(20), 1, 0));
        let tx5 = Arc::new(create_test_tx_spending_utxo(&store, false, MicroTari(20), 0, 1));
        let tx6 = Arc::new(create_test_tx_spending_utxo(&store, false, MicroTari(20), 2, 2));
        let tx7 = Arc::new(create_test_tx_spending_utxo(&store, false, MicroTari(20), 1, 0));
        let tx8 = Arc::new(create_test_tx_spending_utxo(&store, false, MicroTari(20), 2, 0));

        mempool.insert(tx1.clone()).unwrap();
        mempool.insert(tx2.clone()).unwrap();
        mempool.insert(tx3.clone()).unwrap();
        mempool.insert(tx4.clone()).unwrap();
        mempool.insert(tx5.clone()).unwrap();
        mempool.insert(tx6.clone()).unwrap();
        mempool.insert(tx7.clone()).unwrap();
        mempool.insert(tx8.clone()).unwrap();
        let stats = mempool.stats().unwrap();
        assert_eq!(stats.unconfirmed_txs, 3);
        assert_eq!(stats.timelocked_txs, 5);
        assert_eq!(stats.published_txs, 0);

        let published_block1 = create_test_block(1, Some(genesis_block), vec![
            (*tx1).clone(),
            (*tx2).clone(),
            (*tx3).clone(),
        ]);
        store.add_block(published_block1.clone()).unwrap();
        mempool.process_published_block(&published_block1).unwrap();
        let stats = mempool.stats().unwrap();
        assert_eq!(stats.unconfirmed_txs, 3);
        assert_eq!(stats.timelocked_txs, 2);
        assert_eq!(stats.published_txs, 3);

        let published_block2 =
            create_test_block(2, Some(published_block1.clone()), vec![(*tx4).clone(), (*tx5).clone()]);
        store.add_block(published_block2.clone()).unwrap();
        mempool.process_published_block(&published_block2).unwrap();
        let stats = mempool.stats().unwrap();
        assert_eq!(stats.unconfirmed_txs, 3);
        assert_eq!(stats.timelocked_txs, 0);
        assert_eq!(stats.published_txs, 5);

        let published_block3 = create_test_block(3, Some(published_block2.clone()), vec![(*tx6).clone()]);
        store.add_block(published_block3.clone()).unwrap();
        mempool.process_published_block(&published_block3).unwrap();
        let stats = mempool.stats().unwrap();
        assert_eq!(stats.unconfirmed_txs, 2);
        assert_eq!(stats.timelocked_txs, 0);
        assert_eq!(stats.published_txs, 6);

        // TODO: Add code back in when rewind_to_height has been implemented.
        // let mut db_txn = DbTransaction::new();
        // db_txn.rewind_to_height(1);
        // store.commit(db_txn).unwrap();
        // let new_block1 = create_test_block(2, Some(published_block1), vec![(*tx7).clone()]);
        // let new_block2 = create_test_block(3, Some(new_block1.clone()), vec![(*tx8).clone()]);
        // mempool
        // .process_reorg(vec![published_block2, published_block3], vec![new_block1, new_block2])
        // .unwrap();
        // let stats = mempool.stats().unwrap();
        // assert_eq!(stats.unconfirmed_txs, 3);
        // assert_eq!(stats.timelocked_txs, 0);
        // assert_eq!(stats.published_txs, 5);
        //
        // assert_eq!(mempool.has_tx_with_excess_sig(&tx1.body.kernels[0].excess_sig).unwrap(),TxStorageResponse::
        // ReorgPool); assert_eq!(mempool.has_tx_with_excess_sig(&tx2.body.kernels[0].excess_sig).unwrap(),
        // TxStorageResponse::ReorgPool); assert_eq!(mempool.has_tx_with_excess_sig(&tx3.body.kernels[0].
        // excess_sig).unwrap(),TxStorageResponse::ReorgPool); assert_eq!(mempool.has_tx_with_excess_sig(&tx4.
        // body.kernels[0].excess_sig).unwrap(),TxStorageResponse::UnconfirmedPool); assert_eq!(mempool.
        // has_tx_with_excess_sig(&tx5.body.kernels[0].excess_sig).unwrap(),TxStorageResponse::UnconfirmedPool);
        // assert_eq!(mempool.has_tx_with_excess_sig(&tx6.body.kernels[0].excess_sig).unwrap(),TxStorageResponse::
        // UnconfirmedPool); assert_eq!(mempool.has_tx_with_excess_sig(&tx7.body.kernels[0].excess_sig).
        // unwrap(),TxStorageResponse::ReorgPool); assert_eq!(mempool.has_tx_with_excess_sig(&tx8.body.
        // kernels[0].excess_sig).unwrap(),TxStorageResponse::ReorgPool);
    }
}
