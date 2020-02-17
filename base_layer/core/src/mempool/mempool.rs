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
        orphan_pool::OrphanPool,
        pending_pool::PendingPool,
        reorg_pool::ReorgPool,
        unconfirmed_pool::UnconfirmedPool,
        MempoolConfig,
        StatsResponse,
        TxStorageResponse,
    },
    transactions::{transaction::Transaction, types::Signature},
    validation::{Validation, ValidationError, Validator},
};
use log::*;
use std::sync::Arc;

pub const LOG_TARGET: &str = "c::mp::mempool";

/// Struct containing the validators the mempool needs to run, It forces the correct amount of validators are given
pub struct MempoolValidators<B: BlockchainBackend> {
    mempool: Box<dyn Validation<Transaction, B>>,
    orphan: Box<dyn Validation<Transaction, B>>,
}

impl<B: BlockchainBackend> MempoolValidators<B> {
    pub fn new(
        mempool: impl Validation<Transaction, B> + 'static,
        orphan: impl Validation<Transaction, B> + 'static,
    ) -> Self
    {
        Self {
            mempool: Box::new(mempool),
            orphan: Box::new(orphan),
        }
    }

    pub fn into_validators(self) -> (Box<dyn Validation<Transaction, B>>, Box<dyn Validation<Transaction, B>>) {
        (self.mempool, self.orphan)
    }
}

/// The Mempool consists of an Unconfirmed Transaction Pool, Pending Pool, Orphan Pool and Reorg Pool and is responsible
/// for managing and maintaining all unconfirmed transactions have not yet been included in a block, and transactions
/// that have recently been included in a block.
pub struct Mempool<T>
where T: BlockchainBackend
{
    blockchain_db: BlockchainDatabase<T>,
    unconfirmed_pool: UnconfirmedPool,
    orphan_pool: OrphanPool<T>,
    pending_pool: PendingPool,
    reorg_pool: ReorgPool,
    validator: Arc<Validator<Transaction, T>>,
}

impl<T> Mempool<T>
where T: BlockchainBackend
{
    /// Create a new Mempool with an UnconfirmedPool, OrphanPool, PendingPool and ReOrgPool.
    pub fn new(blockchain_db: BlockchainDatabase<T>, config: MempoolConfig, validators: MempoolValidators<T>) -> Self {
        let (mempool_validator, orphan_validator) = validators.into_validators();
        Self {
            unconfirmed_pool: UnconfirmedPool::new(config.unconfirmed_pool_config),
            orphan_pool: OrphanPool::new(config.orphan_pool_config, orphan_validator),
            pending_pool: PendingPool::new(config.pending_pool_config),
            reorg_pool: ReorgPool::new(config.reorg_pool_config),
            blockchain_db,
            validator: Arc::new(mempool_validator),
        }
    }

    /// Insert an unconfirmed transaction into the Mempool. The transaction *MUST* have passed through the validation
    /// pipeline already and will thus always be internally consistent by this stage
    pub fn insert(&self, tx: Arc<Transaction>) -> Result<(), MempoolError> {
        // The transaction is already internally consistent
        match self.validator.validate(&tx) {
            Ok(()) => self.unconfirmed_pool.insert(tx)?,
            Err(ValidationError::UnknownInputs) => self.orphan_pool.insert(tx)?,
            Err(ValidationError::MaturityError) => self.pending_pool.insert(tx)?,
            _ => return Err(MempoolError::ValidationError),
        };
        Ok(())
    }

    /// Insert a set of new transactions into the UTxPool.
    fn insert_txs(&self, txs: Vec<Arc<Transaction>>) -> Result<(), MempoolError> {
        for tx in txs {
            self.insert(tx)?;
        }
        Ok(())
    }

    /// Update the Mempool based on the received published block.
    pub fn process_published_block(&self, published_block: &Block) -> Result<(), MempoolError> {
        trace!(target: LOG_TARGET, "Mempool processing new block: {}", published_block);
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
    pub fn process_published_blocks(&self, published_blocks: &Vec<Block>) -> Result<(), MempoolError> {
        for published_block in published_blocks {
            self.process_published_block(published_block)?;
        }
        Ok(())
    }

    /// In the event of a ReOrg, resubmit all ReOrged transactions into the Mempool and process each newly introduced
    /// block from the latest longest chain.
    pub fn process_reorg(&self, removed_blocks: Vec<Block>, new_blocks: Vec<Block>) -> Result<(), MempoolError> {
        debug!(target: LOG_TARGET, "Mempool processing reorg");
        for block in &removed_blocks {
            trace!(target: LOG_TARGET, "Mempool processing reorg removed block: {}", block);
        }
        for block in &new_blocks {
            trace!(
                target: LOG_TARGET,
                "Mempool processing reorg added new block: {}",
                block
            );
        }
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

impl<T> Clone for Mempool<T>
where T: BlockchainBackend
{
    fn clone(&self) -> Self {
        Mempool {
            blockchain_db: self.blockchain_db.clone(),
            unconfirmed_pool: self.unconfirmed_pool.clone(),
            orphan_pool: self.orphan_pool.clone(),
            pending_pool: self.pending_pool.clone(),
            reorg_pool: self.reorg_pool.clone(),
            validator: self.validator.clone(),
        }
    }
}
