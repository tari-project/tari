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
    chain_storage::BlockchainBackend,
    mempool::orphan_pool::{error::OrphanPoolError, orphan_pool::OrphanPoolConfig},
    transactions::{transaction::Transaction, types::Signature},
    validation::{ValidationError, Validator},
};
use log::*;
use std::sync::Arc;
use ttl_cache::TtlCache;

pub const LOG_TARGET: &str = "c::mp::orphan_pool::orphan_pool_storage";

/// OrphanPool makes use of OrphanPoolStorage to provide thread save access to its TtlCache.
/// The Orphan Pool contains all the received transactions that attempt to spend UTXOs that don't exist. These UTXOs
/// might exist in the future if these transactions are from a series or set of transactions that need to be processed
/// in a specific order. Some of these transactions might still be constrained by pending time-locks.
pub struct OrphanPoolStorage<T>
where T: BlockchainBackend
{
    config: OrphanPoolConfig,
    txs_by_signature: TtlCache<Signature, Arc<Transaction>>,
    validator: Validator<Transaction, T>,
}

impl<T> OrphanPoolStorage<T>
where T: BlockchainBackend
{
    /// Create a new OrphanPoolStorage with the specified configuration
    pub fn new(config: OrphanPoolConfig, validator: Validator<Transaction, T>) -> Self {
        Self {
            config,
            txs_by_signature: TtlCache::new(config.storage_capacity),
            validator,
        }
    }

    /// Insert a new transaction into the OrphanPoolStorage. Orphaned transactions will have a limited Time-to-live and
    /// will be discarded if the UTXOs they require are not created before the Time-to-live threshold is reached.
    pub fn insert(&mut self, tx: Arc<Transaction>) {
        trace!(target: LOG_TARGET, "Adding tx to orphan pool: {:?}", tx.clone());
        let tx_key = tx.body.kernels()[0].excess_sig.clone();
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

        // We dont care about tx's that appeared in valid blocks. Those tx's will time out in orphan pool and remove
        // them selves.
        for (tx_key, tx) in self.txs_by_signature.iter() {
            match self.validator.validate(&tx) {
                Ok(()) => {
                    trace!(
                        target: LOG_TARGET,
                        "Removing key from orphan pool: {:?}",
                        tx_key.clone()
                    );
                    removed_tx_keys.push(tx_key.clone());
                },
                Err(ValidationError::MaturityError) => {
                    trace!(
                        target: LOG_TARGET,
                        "Removing timelocked key from orphan pool: {:?}",
                        tx_key.clone()
                    );
                    removed_timelocked_tx_keys.push(tx_key.clone());
                },
                _ => {},
            };
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
        self.txs_by_signature.iter().count()
    }

    /// Returns all transaction stored in the OrphanPoolStorage.
    pub fn snapshot(&mut self) -> Vec<Arc<Transaction>> {
        self.txs_by_signature.iter().map(|(_, tx)| tx).cloned().collect()
    }

    /// Returns the total weight of all transactions stored in the pool.
    pub fn calculate_weight(&mut self) -> u64 {
        self.txs_by_signature
            .iter()
            .fold(0, |weight, (_, tx)| weight + tx.calculate_weight())
    }
}
