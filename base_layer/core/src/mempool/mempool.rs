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
};
use std::sync::Arc;
use tari_transactions::{
    transaction::Transaction,
    types::{Signature, COMMITMENT_FACTORY, PROVER},
};
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
        for input in tx.body.inputs() {
            if !self.blockchain_db.is_utxo(input.hash())? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn check_timelocks(&mut self, tx: &Transaction) -> Result<bool, MempoolError> {
        match tx.max_timelock_height() {
            0 => Ok(false),
            v => Ok(v - 1 >
                self.blockchain_db
                    .get_height()?
                    .ok_or(MempoolError::ChainHeightUndefined)?),
        }
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
        test_utils::{
            builders::{schema_to_transaction, spend_utxos},
            sample_blockchains::{create_new_blockchain, generate_block, generate_new_block},
        },
        tx,
        txn_schema,
    };
    use std::ops::Deref;
    use tari_transactions::{
        tari_amount::{uT, T},
        transaction::OutputFeatures,
    };

    #[test]
    fn test_insert_and_process_published_block() {
        let (mut store, mut blocks, mut outputs) = create_new_blockchain();
        // TODO - BlockchainDB is cheap to clone, so there's no need to wrap it in an Arc
        let mut mempool = Mempool::new(Arc::new(store.clone()), MempoolConfig::default());
        // Create a block with 4 outputs
        let txs = vec![txn_schema!(
            from: vec![outputs[0][0].clone()],
            to: vec![2 * T, 2 * T, 2 * T, 2 * T]
        )];
        generate_new_block(&mut store, &mut blocks, &mut outputs, txs).unwrap();
        // Create 6 new transactions to add to the mempool
        let (orphan, _, _) = tx!(1*T, fee: 100*uT);
        let orphan = Arc::new(orphan);

        let tx2 = txn_schema!(from: vec![outputs[1][0].clone()], to: vec![1*T], fee: 20*uT);
        let tx2 = Arc::new(spend_utxos(tx2).0);

        let tx3 = txn_schema!(
            from: vec![outputs[1][1].clone()],
            to: vec![1*T],
            fee: 20*uT,
            lock: 4,
            OutputFeatures::with_maturity(1)
        );
        let tx3 = Arc::new(spend_utxos(tx3).0);

        let tx5 = txn_schema!(
            from: vec![outputs[1][2].clone()],
            to: vec![1*T],
            fee: 20*uT,
            lock: 3,
            OutputFeatures::with_maturity(2)
        );
        let tx5 = Arc::new(spend_utxos(tx5).0);
        let tx6 = txn_schema!(from: vec![outputs[1][3].clone()], to: vec![1 * T]);
        let tx6 = spend_utxos(tx6).0;

        mempool.insert(orphan.clone()).unwrap();
        mempool.insert(tx2.clone()).unwrap();
        mempool.insert(tx3.clone()).unwrap();
        mempool.insert(tx5.clone()).unwrap();
        mempool.process_published_block(&blocks[1]).unwrap();

        assert_eq!(
            mempool
                .has_tx_with_excess_sig(&orphan.body.kernels()[0].excess_sig)
                .unwrap(),
            TxStorageResponse::OrphanPool
        );
        assert_eq!(
            mempool
                .has_tx_with_excess_sig(&tx2.body.kernels()[0].excess_sig)
                .unwrap(),
            TxStorageResponse::UnconfirmedPool
        );
        assert_eq!(
            mempool
                .has_tx_with_excess_sig(&tx3.body.kernels()[0].excess_sig)
                .unwrap(),
            TxStorageResponse::PendingPool
        );

        assert_eq!(
            mempool
                .has_tx_with_excess_sig(&tx5.body.kernels()[0].excess_sig)
                .unwrap(),
            TxStorageResponse::PendingPool
        );
        assert_eq!(
            mempool
                .has_tx_with_excess_sig(&tx6.body.kernels()[0].excess_sig)
                .unwrap(),
            TxStorageResponse::NotStored
        );

        let snapshot_txs = mempool.snapshot().unwrap();
        assert_eq!(snapshot_txs.len(), 4);
        assert!(snapshot_txs.contains(&orphan));
        assert!(snapshot_txs.contains(&tx2));
        assert!(snapshot_txs.contains(&tx3));
        assert!(snapshot_txs.contains(&tx5));

        let stats = mempool.stats().unwrap();
        assert_eq!(stats.total_txs, 4);
        assert_eq!(stats.unconfirmed_txs, 1);
        assert_eq!(stats.orphan_txs, 1);
        assert_eq!(stats.timelocked_txs, 2);
        assert_eq!(stats.published_txs, 0);
        assert_eq!(stats.total_weight, 36);

        // Spend tx2, so it goes in Reorg pool, tx5 matures, so goes in Unconfirmed pool
        generate_block(&mut store, &mut blocks, vec![tx2.deref().clone()]).unwrap();
        mempool.process_published_block(&blocks[2]).unwrap();

        assert_eq!(
            mempool
                .has_tx_with_excess_sig(&orphan.body.kernels()[0].excess_sig)
                .unwrap(),
            TxStorageResponse::OrphanPool
        );
        assert_eq!(
            mempool
                .has_tx_with_excess_sig(&tx2.body.kernels()[0].excess_sig)
                .unwrap(),
            TxStorageResponse::ReorgPool
        );
        assert_eq!(
            mempool
                .has_tx_with_excess_sig(&tx3.body.kernels()[0].excess_sig)
                .unwrap(),
            TxStorageResponse::PendingPool
        );
        assert_eq!(
            mempool
                .has_tx_with_excess_sig(&tx5.body.kernels()[0].excess_sig)
                .unwrap(),
            TxStorageResponse::UnconfirmedPool
        );
        assert_eq!(
            mempool
                .has_tx_with_excess_sig(&tx6.body.kernels()[0].excess_sig)
                .unwrap(),
            TxStorageResponse::NotStored
        );

        let snapshot_txs = mempool.snapshot().unwrap();
        assert_eq!(snapshot_txs.len(), 3);
        assert!(snapshot_txs.contains(&orphan));
        assert!(snapshot_txs.contains(&tx3));
        assert!(snapshot_txs.contains(&tx5));

        let stats = mempool.stats().unwrap();
        assert_eq!(stats.total_txs, 4);
        assert_eq!(stats.unconfirmed_txs, 1);
        assert_eq!(stats.orphan_txs, 1);
        assert_eq!(stats.timelocked_txs, 1);
        assert_eq!(stats.published_txs, 1);
        assert_eq!(stats.total_weight, 36);
    }

    #[test]
    fn test_retrieve() {
        let (mut store, mut blocks, mut outputs) = create_new_blockchain();
        let mut mempool = Mempool::new(Arc::new(store.clone()), MempoolConfig::default());
        let txs = vec![txn_schema!(
            from: vec![outputs[0][0].clone()],
            to: vec![1 * T, 1 * T, 1 * T, 1 * T, 1 * T, 1 * T, 1 * T]
        )];
        // "Mine" Block 1
        generate_new_block(&mut store, &mut blocks, &mut outputs, txs).unwrap();
        mempool.process_published_block(&blocks[1]).unwrap();
        // 1-Block, 8 UTXOs, empty mempool
        let txs = vec![
            txn_schema!(from: vec![outputs[1][0].clone()], to: vec![], fee: 30*uT),
            txn_schema!(from: vec![outputs[1][1].clone()], to: vec![], fee: 20*uT),
            txn_schema!(from: vec![outputs[1][2].clone()], to: vec![], fee: 40*uT),
            txn_schema!(from: vec![outputs[1][3].clone()], to: vec![], fee: 50*uT),
            txn_schema!(from: vec![outputs[1][4].clone()], to: vec![], fee: 20*uT, lock: 2, OutputFeatures::default()),
            txn_schema!(from: vec![outputs[1][5].clone()], to: vec![], fee: 20*uT, lock: 3, OutputFeatures::default()),
            // Will be time locked when a tx is added to mempool with this as an input:
            txn_schema!(from: vec![outputs[1][6].clone()], to: vec![800_000*uT], fee: 60*uT, lock: 0,
                        OutputFeatures::with_maturity(4)),
            // Will be time locked when a tx is added to mempool with this as an input:
            txn_schema!(from: vec![outputs[1][7].clone()], to: vec![800_000*uT], fee: 25*uT, lock: 0,
                        OutputFeatures::with_maturity(3)),
        ];
        let (tx, utxos) = schema_to_transaction(&txs);
        tx.iter().for_each(|t| {
            mempool.insert(t.clone()).unwrap();
        });
        // 1-block, 8 UTXOs, 8 txs in mempool
        let weight = tx[6].calculate_weight() + tx[2].calculate_weight() + tx[3].calculate_weight();
        let retrieved_txs = mempool.retrieve(weight).unwrap();
        assert_eq!(retrieved_txs.len(), 3);
        assert!(retrieved_txs.contains(&tx[6]));
        assert!(retrieved_txs.contains(&tx[2]));
        assert!(retrieved_txs.contains(&tx[3]));
        let stats = mempool.stats().unwrap();
        println!("After block 1: {:?}", stats);
        assert_eq!(stats.unconfirmed_txs, 7);
        assert_eq!(stats.timelocked_txs, 1);
        assert_eq!(stats.published_txs, 0);

        let block2_txns = vec![
            tx[0].deref().clone(),
            tx[1].deref().clone(),
            tx[2].deref().clone(),
            tx[6].deref().clone(),
            tx[7].deref().clone(),
        ];
        // "Mine" block 2
        generate_block(&mut store, &mut blocks, block2_txns).unwrap();
        println!("{}", blocks[2]);
        outputs.push(utxos);
        mempool.process_published_block(&blocks[2]).unwrap();
        // 2-blocks, 2 unconfirmed txs in mempool, 0 time locked (tx5 time-lock will expire)
        let stats = mempool.stats().unwrap();
        assert_eq!(stats.unconfirmed_txs, 3);
        assert_eq!(stats.timelocked_txs, 0);
        assert_eq!(stats.published_txs, 5);
        // Create transactions wih time-locked inputs
        let txs = vec![
            txn_schema!(from: vec![outputs[2][6].clone()], to: vec![], fee: 80*uT),
            // account for change output
            txn_schema!(from: vec![outputs[2][8].clone()], to: vec![], fee: 40*uT),
        ];
        let (tx2, _) = schema_to_transaction(&txs);
        tx2.iter().for_each(|t| {
            mempool.insert(t.clone()).unwrap();
        });
        // 2 blocks, 3 unconfirmed txs in mempool, 2 time locked

        // Top 2 txs are tx[3] (fee/g = 50) and tx2[1] (fee/g = 40). tx2[0] (fee/g = 80) is still not matured.
        let weight = tx[3].calculate_weight() + tx2[1].calculate_weight();
        let retrieved_txs = mempool.retrieve(weight).unwrap();
        let stats = mempool.stats().unwrap();
        println!("{:?}", stats);
        assert_eq!(stats.unconfirmed_txs, 4);
        assert_eq!(stats.timelocked_txs, 1);
        assert_eq!(stats.published_txs, 5);
        assert_eq!(retrieved_txs.len(), 2);
        assert!(retrieved_txs.contains(&tx[3]));
        assert!(retrieved_txs.contains(&tx2[1]));
    }

    #[test]
    fn test_reorg() {
        let (mut db, mut blocks, mut outputs) = create_new_blockchain();
        let mut mempool = Mempool::new(Arc::new(db.clone()), MempoolConfig::default());

        // "Mine" Block 1
        let txs = vec![txn_schema!(from: vec![outputs[0][0].clone()], to: vec![1 * T, 1 * T])];
        generate_new_block(&mut db, &mut blocks, &mut outputs, txs).unwrap();
        mempool.process_published_block(&blocks[1]).unwrap();

        // "Mine" block 2
        let schemas = vec![
            txn_schema!(from: vec![outputs[1][0].clone()], to: vec![]),
            txn_schema!(from: vec![outputs[1][1].clone()], to: vec![]),
            txn_schema!(from: vec![outputs[1][2].clone()], to: vec![]),
        ];
        let (txns2, utxos) = schema_to_transaction(&schemas);
        outputs.push(utxos);
        txns2.iter().for_each(|tx| {
            mempool.insert(tx.clone()).unwrap();
        });
        let stats = mempool.stats().unwrap();
        assert_eq!(stats.unconfirmed_txs, 3);
        let txns2 = txns2.iter().map(|t| t.deref().clone()).collect();
        generate_block(&mut db, &mut blocks, txns2).unwrap();
        mempool.process_published_block(&blocks[2]).unwrap();

        // "Mine" block 3
        let schemas = vec![
            txn_schema!(from: vec![outputs[2][0].clone()], to: vec![]),
            txn_schema!(from: vec![outputs[2][1].clone()], to: vec![], fee: 25*uT, lock: 5, OutputFeatures::default()),
            txn_schema!(from: vec![outputs[2][2].clone()], to: vec![], fee: 25*uT),
        ];
        let (txns3, utxos) = schema_to_transaction(&schemas);
        outputs.push(utxos);
        txns3.iter().for_each(|tx| {
            mempool.insert(tx.clone()).unwrap();
        });
        let txns3: Vec<Transaction> = txns3.iter().map(|t| t.deref().clone()).collect();

        generate_block(&mut db, &mut blocks, vec![txns3[0].clone(), txns3[2].clone()]).unwrap();
        mempool.process_published_block(&blocks[3]).unwrap();

        let stats = mempool.stats().unwrap();
        assert_eq!(stats.unconfirmed_txs, 0);
        assert_eq!(stats.timelocked_txs, 1);

        db.rewind_to_height(2).unwrap();

        mempool.process_reorg(vec![blocks[3].clone()], vec![]).unwrap();
        let stats = mempool.stats().unwrap();
        assert_eq!(stats.unconfirmed_txs, 2);
        assert_eq!(stats.timelocked_txs, 1);
        assert_eq!(stats.published_txs, 3);
    }
}
