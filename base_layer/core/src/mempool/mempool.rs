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

use tari_common_types::types::{PrivateKey, Signature};
use tokio::task;

use crate::{
    blocks::Block,
    consensus::ConsensusManager,
    mempool::{
        error::MempoolError,
        mempool_storage::MempoolStorage,
        MempoolConfig,
        StateResponse,
        StatsResponse,
        TxStorageResponse,
    },
    transactions::transaction_components::Transaction,
    validation::MempoolTransactionValidation,
};

/// The Mempool consists of an Unconfirmed Transaction Pool, Pending Pool, Orphan Pool and Reorg Pool and is responsible
/// for managing and maintaining all unconfirmed transactions that have not yet been included in a block, and
/// transactions that have recently been included in a block.
#[derive(Clone)]
pub struct Mempool {
    pool_storage: Arc<RwLock<MempoolStorage>>,
}

impl Mempool {
    /// Create a new Mempool with an UnconfirmedPool and ReOrgPool.
    pub fn new(
        config: MempoolConfig,
        rules: ConsensusManager,
        validator: Box<dyn MempoolTransactionValidation>,
    ) -> Self {
        Self {
            pool_storage: Arc::new(RwLock::new(MempoolStorage::new(config, rules, validator))),
        }
    }

    /// Insert an unconfirmed transaction into the Mempool.
    pub async fn insert(&self, tx: Arc<Transaction>) -> Result<TxStorageResponse, MempoolError> {
        self.do_write_task(|storage| storage.insert(tx)).await
    }

    /// Inserts all transactions into the mempool.
    pub async fn insert_all(&self, transactions: Vec<Arc<Transaction>>) -> Result<(), MempoolError> {
        self.do_write_task(|storage| {
            for tx in transactions {
                storage.insert(tx)?;
            }

            Ok(())
        })
        .await
    }

    /// Update the Mempool based on the received published block.
    pub async fn process_published_block(&self, published_block: Arc<Block>) -> Result<(), MempoolError> {
        self.do_write_task(move |storage| storage.process_published_block(&published_block))
            .await
    }

    /// In the event of a ReOrg, resubmit all ReOrged transactions into the Mempool and process each newly introduced
    /// block from the latest longest chain.
    pub async fn process_reorg(
        &self,
        removed_blocks: Vec<Arc<Block>>,
        new_blocks: Vec<Arc<Block>>,
    ) -> Result<(), MempoolError> {
        self.do_write_task(move |storage| storage.process_reorg(&removed_blocks, &new_blocks))
            .await
    }

    /// Returns all unconfirmed transaction stored in the Mempool, except the transactions stored in the ReOrgPool.
    pub async fn snapshot(&self) -> Result<Vec<Arc<Transaction>>, MempoolError> {
        self.do_read_task(|storage| Ok(storage.snapshot())).await
    }

    /// Returns a list of transaction ranked by transaction priority up to a given weight.
    /// Only transactions that fit into a block will be returned
    pub async fn retrieve(&self, total_weight: u64) -> Result<Vec<Arc<Transaction>>, MempoolError> {
        self.do_write_task(move |storage| storage.retrieve_and_revalidate(total_weight))
            .await
    }

    pub async fn retrieve_by_excess_sigs(
        &self,
        excess_sigs: Vec<PrivateKey>,
    ) -> Result<(Vec<Arc<Transaction>>, Vec<PrivateKey>), MempoolError> {
        self.do_read_task(move |storage| Ok(storage.retrieve_by_excess_sigs(&excess_sigs)))
            .await
    }

    /// Check if the specified excess signature is found in the Mempool.
    pub async fn has_tx_with_excess_sig(&self, excess_sig: Signature) -> Result<TxStorageResponse, MempoolError> {
        self.do_read_task(move |storage| Ok(storage.has_tx_with_excess_sig(&excess_sig)))
            .await
    }

    /// Check if the specified transaction is stored in the Mempool.
    pub async fn has_transaction(&self, tx: Arc<Transaction>) -> Result<TxStorageResponse, MempoolError> {
        self.do_read_task(move |storage| storage.has_transaction(&tx)).await
    }

    /// Gathers and returns the stats of the Mempool.
    pub async fn stats(&self) -> Result<StatsResponse, MempoolError> {
        self.do_read_task(|storage| Ok(storage.stats())).await
    }

    /// Gathers and returns a breakdown of all the transaction in the Mempool.
    pub async fn state(&self) -> Result<StateResponse, MempoolError> {
        self.do_read_task(|storage| Ok(storage.state())).await
    }

    async fn do_read_task<F, T>(&self, callback: F) -> Result<T, MempoolError>
    where
        F: FnOnce(&MempoolStorage) -> Result<T, MempoolError> + Send + 'static,
        T: Send + 'static,
    {
        let storage = self.pool_storage.clone();
        task::spawn_blocking(move || {
            let lock = storage.read().map_err(|_| MempoolError::RwLockPoisonError)?;
            callback(&*lock)
        })
        .await?
    }

    async fn do_write_task<F, T>(&self, callback: F) -> Result<T, MempoolError>
    where
        F: FnOnce(&mut MempoolStorage) -> Result<T, MempoolError> + Send + 'static,
        T: Send + 'static,
    {
        let storage = self.pool_storage.clone();
        task::spawn_blocking(move || {
            let mut lock = storage.write().map_err(|_| MempoolError::RwLockPoisonError)?;
            callback(&mut *lock)
        })
        .await?
    }
}
