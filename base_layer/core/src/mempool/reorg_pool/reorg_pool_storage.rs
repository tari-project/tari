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
    mempool::reorg_pool::reorg_pool::ReorgPoolConfig,
    transactions::{transaction::Transaction, types::Signature},
};
use log::*;
use std::sync::Arc;
use ttl_cache::TtlCache;
pub const LOG_TARGET: &str = "c::mp::reorg_pool::reorg_pool_storage";

/// Reorg makes use of ReorgPoolStorage to provide thread save access to its TtlCache.
/// The ReorgPoolStorage consists of all transactions that have recently been added to blocks.
/// When a potential blockchain reorganization occurs the transactions can be recovered from the ReorgPool and can be
/// added back into the UnconfirmedPool. Transactions in the ReOrg pool have a limited Time-to-live and will be removed
/// from the pool when the Time-to-live thresholds is reached. Also, when the capacity of the pool has been reached, the
/// oldest transactions will be removed to make space for incoming transactions.
pub struct ReorgPoolStorage {
    config: ReorgPoolConfig,
    txs_by_signature: TtlCache<Signature, Arc<Transaction>>,
}

impl ReorgPoolStorage {
    /// Create a new ReorgPoolStorage with the specified configuration
    pub fn new(config: ReorgPoolConfig) -> Self {
        Self {
            config,
            txs_by_signature: TtlCache::new(config.storage_capacity),
        }
    }

    /// Insert a new transaction into the ReorgPoolStorage. Published transactions will have a limited Time-to-live in
    /// the ReorgPoolStorage and will be discarded once the Time-to-live threshold has been reached.
    pub fn insert(&mut self, tx: Arc<Transaction>) {
        let tx_key = tx.body.kernels()[0].excess_sig.clone();
        trace!(target: LOG_TARGET, "Inserting tx into reorg pool: {:?}", tx_key,);
        let _ = self.txs_by_signature.insert(tx_key, tx, self.config.tx_ttl);
    }

    /// Insert a set of new transactions into the ReorgPoolStorage
    pub fn insert_txs(&mut self, txs: Vec<Arc<Transaction>>) {
        for tx in txs.into_iter() {
            self.insert(tx);
        }
    }

    /// Check if a transaction is stored in the ReorgPoolStorage
    pub fn has_tx_with_excess_sig(&self, excess_sig: &Signature) -> bool {
        self.txs_by_signature.contains_key(excess_sig)
    }

    /// Remove double-spends from the ReorgPool. These transactions were orphaned by the provided published
    /// block. Check if any of the transactions in the ReorgPool has inputs that was spent by the provided
    /// published block.
    fn discard_double_spends(&mut self, published_block: &Block) {
        let mut removed_tx_keys: Vec<Signature> = Vec::new();
        for (tx_key, ptx) in self.txs_by_signature.iter() {
            for input in ptx.body.inputs() {
                if published_block.body.inputs().contains(input) {
                    removed_tx_keys.push(tx_key.clone());
                }
            }
        }

        for tx_key in &removed_tx_keys {
            trace!(target: LOG_TARGET, "Removed double spends: {:?}", tx_key);
            self.txs_by_signature.remove(&tx_key);
        }
    }

    /// Remove the transactions from the ReorgPoolStorage that were used in provided removed blocks. The transactions
    /// can be resubmitted to the Unconfirmed Pool.
    pub fn remove_reorged_txs_and_discard_double_spends(
        &mut self,
        removed_blocks: Vec<Block>,
        new_blocks: &Vec<Block>,
    ) -> Vec<Arc<Transaction>>
    {
        for block in new_blocks {
            self.discard_double_spends(block);
        }

        let mut removed_txs: Vec<Arc<Transaction>> = Vec::new();
        for block in &removed_blocks {
            for kernel in block.body.kernels() {
                if let Some(removed_tx) = self.txs_by_signature.remove(&kernel.excess_sig) {
                    trace!(target: LOG_TARGET, "Removing tx from reorg pool: {:?}", removed_tx);
                    removed_txs.push(removed_tx);
                }
            }
        }
        removed_txs
    }

    /// Returns the total number of published transactions stored in the ReorgPoolStorage
    pub fn len(&mut self) -> usize {
        self.txs_by_signature.iter().count()
    }

    /// Returns the total weight of all transactions stored in the pool.
    pub fn calculate_weight(&mut self) -> u64 {
        self.txs_by_signature
            .iter()
            .fold(0, |weight, (_, tx)| weight + tx.calculate_weight())
    }
}
