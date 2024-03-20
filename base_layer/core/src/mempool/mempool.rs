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

use std::sync::{Arc, RwLock};

use log::debug;
use tari_common_types::types::{PrivateKey, Signature};
use tokio::task;

use crate::{
    blocks::Block,
    consensus::ConsensusManager,
    mempool::{
        error::MempoolError,
        mempool_storage::MempoolStorage,
        FeePerGramStat,
        MempoolConfig,
        StateResponse,
        StatsResponse,
        TxStorageResponse,
    },
    transactions::transaction_components::Transaction,
    validation::TransactionValidator,
};

pub const LOG_TARGET: &str = "c::mp::mempool";

/// The Mempool consists of an Unconfirmed Transaction Pool, Pending Pool, Orphan Pool and Reorg Pool and is responsible
/// for managing and maintaining all unconfirmed transactions that have not yet been included in a block, and
/// transactions that have recently been included in a block.
#[derive(Clone)]
pub struct Mempool {
    pool_storage: Arc<RwLock<MempoolStorage>>,
}

impl Mempool {
    /// Create a new Mempool with an UnconfirmedPool and ReOrgPool.
    pub fn new(config: MempoolConfig, rules: ConsensusManager, validator: Box<dyn TransactionValidator>) -> Self {
        Self {
            pool_storage: Arc::new(RwLock::new(MempoolStorage::new(config, rules, validator))),
        }
    }

    /// Insert an unconfirmed transaction into the Mempool.
    pub async fn insert(&self, tx: Arc<Transaction>) -> Result<TxStorageResponse, MempoolError> {
        self.with_write_access(|storage| {
            storage
                .insert(tx)
                .map_err(|e| MempoolError::InternalError(e.to_string()))
        })
        .await
    }

    /// Inserts all transactions into the mempool.
    pub async fn insert_all(&self, transactions: Vec<Arc<Transaction>>) -> Result<(), MempoolError> {
        self.with_write_access(|storage| {
            for tx in transactions {
                storage
                    .insert(tx)
                    .map_err(|e| MempoolError::InternalError(e.to_string()))?;
            }

            Ok(())
        })
        .await
    }

    /// Update the Mempool based on the received published block.
    pub async fn process_published_block(&self, published_block: Arc<Block>) -> Result<(), MempoolError> {
        self.with_write_access(move |storage| storage.process_published_block(&published_block))
            .await
    }

    /// Update the Mempool by clearing transactions for a block that failed to validate.
    pub async fn clear_transactions_for_failed_block(&self, failed_block: Arc<Block>) -> Result<(), MempoolError> {
        self.with_write_access(move |storage| storage.clear_transactions_for_failed_block(&failed_block))
            .await
    }

    /// In the event of a ReOrg, resubmit all ReOrged transactions into the Mempool and process each newly introduced
    /// block from the latest longest chain.
    pub async fn process_reorg(
        &self,
        removed_blocks: Vec<Arc<Block>>,
        new_blocks: Vec<Arc<Block>>,
    ) -> Result<(), MempoolError> {
        self.with_write_access(move |storage| storage.process_reorg(&removed_blocks, &new_blocks))
            .await
    }

    /// After a sync event, we can move all orphan transactions to the unconfirmed pool after validation
    pub async fn process_sync(&self) -> Result<(), MempoolError> {
        self.with_write_access(move |storage| storage.process_sync()).await
    }

    /// Returns all unconfirmed transaction stored in the Mempool, except the transactions stored in the ReOrgPool.
    pub async fn snapshot(&self) -> Result<Vec<Arc<Transaction>>, MempoolError> {
        self.with_read_access(|storage| Ok(storage.snapshot())).await
    }

    /// Returns a list of transaction ranked by transaction priority up to a given weight.
    /// Only transactions that fit into a block will be returned
    pub async fn retrieve(&self, total_weight: u64) -> Result<Vec<Arc<Transaction>>, MempoolError> {
        let start = std::time::Instant::now();
        let retrieved = self
            .with_read_access(move |storage| storage.retrieve(total_weight))
            .await?;
        debug!(
            target: LOG_TARGET,
            "Retrieved {} highest priority transaction(s) from the mempool in {:.0?} ms",
            retrieved.retrieved_transactions.len(),
            start.elapsed()
        );

        if !retrieved.transactions_to_remove_and_insert.is_empty() {
            // we need to remove all transactions that need to be rechecked.
            debug!(
                target: LOG_TARGET,
                "Removing {} transaction(s) from unconfirmed pool because they need re-evaluation",
                retrieved.transactions_to_remove_and_insert.len()
            );

            let transactions_to_remove_and_insert = retrieved.transactions_to_remove_and_insert.clone();
            self.with_write_access(move |storage| {
                storage.remove_and_reinsert_transactions(transactions_to_remove_and_insert)
            })
            .await?;
        }

        Ok(retrieved.retrieved_transactions)
    }

    pub async fn retrieve_by_excess_sigs(
        &self,
        excess_sigs: Vec<PrivateKey>,
    ) -> Result<(Vec<Arc<Transaction>>, Vec<PrivateKey>), MempoolError> {
        self.with_read_access(move |storage| storage.retrieve_by_excess_sigs(&excess_sigs))
            .await
    }

    /// Check if the specified excess signature is found in the Mempool.
    pub async fn has_tx_with_excess_sig(&self, excess_sig: Signature) -> Result<TxStorageResponse, MempoolError> {
        self.with_read_access(move |storage| Ok(storage.has_tx_with_excess_sig(&excess_sig)))
            .await
    }

    /// Check if the specified transaction is stored in the Mempool.
    pub async fn has_transaction(&self, tx: Arc<Transaction>) -> Result<TxStorageResponse, MempoolError> {
        self.with_read_access(move |storage| storage.has_transaction(&tx)).await
    }

    /// Gathers and returns the stats of the Mempool.
    pub async fn stats(&self) -> Result<StatsResponse, MempoolError> {
        self.with_read_access(|storage| storage.stats().map_err(|e| MempoolError::InternalError(e.to_string())))
            .await
    }

    /// Gathers and returns a breakdown of all the transaction in the Mempool.
    pub async fn state(&self) -> Result<StateResponse, MempoolError> {
        self.with_read_access(|storage| Ok(storage.state())).await
    }

    pub async fn get_fee_per_gram_stats(
        &self,
        count: usize,
        tip_height: u64,
    ) -> Result<Vec<FeePerGramStat>, MempoolError> {
        self.with_read_access(move |storage| storage.get_fee_per_gram_stats(count, tip_height))
            .await
    }

    async fn with_read_access<F, T>(&self, callback: F) -> Result<T, MempoolError>
    where
        F: FnOnce(&MempoolStorage) -> Result<T, MempoolError> + Send + 'static,
        T: Send + 'static,
    {
        let storage = self.pool_storage.clone();
        task::spawn_blocking(move || {
            let lock = storage.read().map_err(|_| MempoolError::RwLockPoisonError)?;
            callback(&lock)
        })
        .await?
    }

    async fn with_write_access<F, T>(&self, callback: F) -> Result<T, MempoolError>
    where
        F: FnOnce(&mut MempoolStorage) -> Result<T, MempoolError> + Send + 'static,
        T: Send + 'static,
    {
        let storage = self.pool_storage.clone();
        task::spawn_blocking(move || {
            let mut lock = storage.write().map_err(|_| MempoolError::RwLockPoisonError)?;
            callback(&mut lock)
        })
        .await?
    }
}
