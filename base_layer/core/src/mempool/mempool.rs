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
    mempool::{
        error::MempoolError,
        orphan_pool::{OrphanPool, OrphanPoolConfig},
        pending_pool::{PendingPool, PendingPoolConfig},
        reorg_pool::{ReorgPool, ReorgPoolConfig},
        unconfirmed_pool::{UnconfirmedPool, UnconfirmedPoolConfig},
    },
    transaction::Transaction,
    types::Signature,
};
use std::sync::Arc;

/// Configuration for the Mempool
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
pub struct Mempool {
    unconfirmed_pool: UnconfirmedPool,
    orphan_pool: OrphanPool,
    pending_pool: PendingPool,
    reorg_pool: ReorgPool,
}

impl Mempool {
    /// Create a new Mempool with a UnconfirmedPool, OrphanPool, PendingPool and ReOrgPool
    pub fn new(config: MempoolConfig) -> Self {
        Self {
            unconfirmed_pool: UnconfirmedPool::new(config.unconfirmed_pool_config),
            orphan_pool: OrphanPool::new(config.orphan_pool_config),
            pending_pool: PendingPool::new(config.pending_pool_config),
            reorg_pool: ReorgPool::new(config.reorg_pool_config),
        }
    }

    /// Insert an unconfirmed transaction into the Mempool
    pub fn insert(&mut self, _utx: Transaction) -> Result<(), MempoolError> {
        // TODO: Verify incoming txs and check for timelocks and that valid UTXOs are spent

        // TODO: UTxs that have passed all the verification steps and checks, except they attempt to spend UTXOs that
        // don't exist should  be added to Orphan Pool.

        // TODO: UTxs constrained by timelocks and attempt to spend nonexistent UTXOs should be added to orphan pool.

        // TODO: UTxs that have passed all the verification steps and checks, Time-locked utxs should be added to
        // Pending Pool

        // TODO: Utxs that have been received, verified and have passed all checks, don't have time-locks and only spend
        // valid UTXOs should be added to UTxPool.
        Ok(())
    }

    ///  Insert a set of new transactions into the UTxPool
    fn insert_txs(&mut self, txs: Vec<Transaction>) -> Result<(), MempoolError> {
        for tx in txs {
            self.insert(tx)?;
        }

        Ok(())
    }

    /// Update the Mempool based on the received published block
    pub fn process_published_block(&mut self, _published_block: &Block) -> Result<(), MempoolError> {
        // Move published txs to ReOrgPool and discard double spends
        // self.reorg_pool.insert_txs(self.unconfirmed_pool.remove_published_and_discard_double_spends(published_block)?
        // )?;

        // Move txs with valid input UTXOs and expired time-locks to UnconfirmedPool and discard double spends
        // unconfirmed_pool.insert_txs(pending_pool.remove_unlocked_and_discard_double_spends()?)?;

        // Move Time-locked txs that have input UTXOs that have recently become valid to PendingPool. Move txs with no
        // or recently expired time-locks that have input UTXOs that have recently become valid to the UnconfirmedPool
        // let (txs,time_locked_txs)=orphan_pool.remove_valid(published_block.header.height,utxos)?;
        // pending_pool.insert_txs(time_locked_txs)?;
        // unconfirmed_pool.insert_txs(txs)?;

        // Txs stored in the OrphanPool and ReOrgPool will be removed when their TTLs have been reached.

        Ok(())
    }

    /// Update the Mempool based on the received set of published blocks
    pub fn process_published_blocks(&mut self, published_blocks: &Vec<Block>) -> Result<(), MempoolError> {
        for published_block in published_blocks {
            self.process_published_block(published_block)?;
        }
        Ok(())
    }

    /// In the event of a ReOrg, resubmit all ReOrged transactions into the Mempool and process each newly introduced
    /// block from the latest longest chain
    pub fn process_reorg(&mut self, _removed_blocks: Vec<Block>, _new_blocks: Vec<Block>) -> Result<(), MempoolError> {
        // let reorg_txs=self.reorg_pool.scan_for_and_remove_reorged_txs(removed_blocks);
        // self.insert_txs(reorg_txs)?;
        // self.process_published_blocks(&new_blocks)?;
        Ok(())
    }

    /// Returns all unconfirmed transaction stored in the Mempool, except the transactions stored in the ReOrgPool.
    // TODO: Investigate returning an iterator rather than a large vector of transactions
    pub fn snapshot(&self) -> Result<Vec<Arc<Transaction>>, MempoolError> {
        // return content of UnconfirmedPool, OrphanPool and PendingPool

        Ok(Vec::new())
    }

    /// Returns a list of transaction ranked by transaction priority up to a given weight.
    pub fn retrieve(&self, _total_weight: usize) -> Result<Vec<Arc<Transaction>>, MempoolError> {
        Ok(Vec::new())
    }

    /// Check if the specified transaction is stored in the Mempool.
    pub fn has_tx_with_excess_sig(&self, _excess_sig: &Signature) -> Result<(), MempoolError> {
        // Return Some(Sub-pool enum) or None when it is not stored in the Mempool

        Ok(())
    }

    /// Returns the Mempool stats for the Mempool
    pub fn stats(&self) -> Result<(), MempoolError> {
        // Return the stats of the Mempool, including subpools (OrphanPool, PendingPool and ReOrgPool). The number of
        // unconfirmed transactions. The number of orphaned transactions. The current size of the mempool (in
        // transaction weight).

        Ok(())
    }
}
