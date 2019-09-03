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
    mempool::orphan_pool::{error::OrphanPoolError, orphan_pool::OrphanPoolConfig},
    transaction::Transaction,
    types::Signature,
};
use std::sync::Arc;
use tari_utilities::hash::Hashable;
use ttl_cache::TtlCache;

/// OrphanPool makes use of OrphanPoolStorage to provide thread save access to its TtlCache.
/// The Orphan Pool contains all the received transactions that attempt to spend UTXOs that don't exist. These UTXOs
/// might exist in the future if these transactions are from a series or set of transactions that need to be processed
/// in a specific order. Some of these transactions might still be constrained by pending time-locks.
pub struct OrphanPoolStorage<T>
where T: BlockchainBackend
{
    blockchain_db: Arc<BlockchainDatabase<T>>,
    config: OrphanPoolConfig,
    txs_by_signature: TtlCache<Signature, Arc<Transaction>>,
}

impl<T> OrphanPoolStorage<T>
where T: BlockchainBackend
{
    /// Create a new OrphanPoolStorage with the specified configuration
    pub fn new(blockchain_db: Arc<BlockchainDatabase<T>>, config: OrphanPoolConfig) -> Self {
        Self {
            blockchain_db,
            config,
            txs_by_signature: TtlCache::new(config.storage_capacity),
        }
    }

    /// Insert a new transaction into the OrphanPoolStorage. Orphaned transactions will have a limited Time-to-live and
    /// will be discarded if the UTXOs they require are not created before the Time-to-live threshold is reached.
    pub fn insert(&mut self, tx: Arc<Transaction>) {
        let tx_key = tx.body.kernels[0].excess_sig.clone();
        let _ = self.txs_by_signature.insert(tx_key, tx, self.config.tx_ttl);
    }

    /// Insert a set of new transactions into the OrphanPoolStorage
    #[allow(dead_code)]
    pub fn insert_txs(&mut self, txs: Vec<Arc<Transaction>>) {
        for tx in txs.into_iter() {
            self.insert(tx);
        }
    }

    /// Check if a transaction is stored in the OrphanPoolStorage
    pub fn has_tx_with_excess_sig(&self, excess_sig: &Signature) -> bool {
        self.txs_by_signature.contains_key(excess_sig)
    }

    /// Check if the required UTXOs have been created and if the status of any of the transactions in the
    /// OrphanPoolStorage has changed. Remove valid transactions and valid transactions with time-locks from the
    /// OrphanPoolStorage.
    pub fn scan_for_and_remove_unorphaned_txs(
        &mut self,
    ) -> Result<(Vec<Arc<Transaction>>, Vec<Arc<Transaction>>), OrphanPoolError> {
        let mut removed_tx_keys: Vec<Signature> = Vec::new();
        let mut removed_timelocked_tx_keys: Vec<Signature> = Vec::new();
        let height = self
            .blockchain_db
            .get_height()?
            .ok_or(OrphanPoolError::ChainHeightUndefined)?;
        'outer: for (tx_key, tx) in self.txs_by_signature.iter() {
            for input in &tx.body.inputs {
                if !self.blockchain_db.is_utxo(input.hash())? {
                    continue 'outer;
                }
            }

            if tx.max_timelock_height() > height {
                removed_timelocked_tx_keys.push(tx_key.clone());
            } else {
                removed_tx_keys.push(tx_key.clone());
            }
        }

        let mut removed_txs: Vec<Arc<Transaction>> = Vec::with_capacity(removed_tx_keys.len());
        removed_tx_keys.iter().for_each(|tx_key| {
            if let Some(tx) = self.txs_by_signature.remove(&tx_key) {
                removed_txs.push(tx);
            }
        });

        let mut removed_timelocked_txs: Vec<Arc<Transaction>> = Vec::with_capacity(removed_timelocked_tx_keys.len());
        removed_timelocked_tx_keys.iter().for_each(|tx_key| {
            if let Some(tx) = self.txs_by_signature.remove(&tx_key) {
                removed_timelocked_txs.push(tx);
            }
        });

        Ok((removed_txs, removed_timelocked_txs))
    }

    /// Returns the total number of orphaned transactions stored in the OrphanPoolStorage
    pub fn len(&mut self) -> usize {
        let mut count = 0;
        self.txs_by_signature.iter().for_each(|_| count += 1);
        (count)
    }

    /// Returns all transaction stored in the OrphanPoolStorage.
    pub fn snapshot(&mut self) -> Vec<Arc<Transaction>> {
        let mut txs: Vec<Arc<Transaction>> = Vec::new();
        self.txs_by_signature.iter().for_each(|(_, tx)| txs.push(tx.clone()));
        txs
    }

    /// Returns the total weight of all transactions stored in the pool.
    pub fn calculate_weight(&mut self) -> u64 {
        let mut weight: u64 = 0;
        self.txs_by_signature
            .iter()
            .for_each(|(_, tx)| weight += tx.calculate_weight());
        (weight)
    }
}
