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
        mempool_storage::MempoolStorage,
        MempoolConfig,
        StateResponse,
        StatsResponse,
        TxStorageResponse,
    },
    transactions::{transaction::Transaction, types::Signature},
    validation::MempoolTransactionValidation,
};
use std::sync::{Arc, RwLock};

/// The Mempool consists of an Unconfirmed Transaction Pool, Pending Pool, Orphan Pool and Reorg Pool and is responsible
/// for managing and maintaining all unconfirmed transactions have not yet been included in a block, and transactions
/// that have recently been included in a block.
#[derive(Clone)]
pub struct Mempool {
    pool_storage: Arc<RwLock<MempoolStorage>>,
}

impl Mempool {
    /// Create a new Mempool with an UnconfirmedPool, OrphanPool, PendingPool and ReOrgPool.
    pub fn new(config: MempoolConfig, validator: Arc<dyn MempoolTransactionValidation>) -> Self {
        Self {
            pool_storage: Arc::new(RwLock::new(MempoolStorage::new(config, validator))),
        }
    }

    /// Insert an unconfirmed transaction into the Mempool. The transaction *MUST* have passed through the validation
    /// pipeline already and will thus always be internally consistent by this stage
    pub fn insert(&self, tx: Arc<Transaction>) -> Result<TxStorageResponse, MempoolError> {
        self.pool_storage
            .write()
            .map_err(|e| MempoolError::BackendError(e.to_string()))?
            .insert(tx)
    }

    /// Update the Mempool based on the received published block.
    pub fn process_published_block(&self, published_block: Arc<Block>) -> Result<(), MempoolError> {
        self.pool_storage
            .write()
            .map_err(|e| MempoolError::BackendError(e.to_string()))?
            .process_published_block(published_block)
    }

    /// In the event of a ReOrg, resubmit all ReOrged transactions into the Mempool and process each newly introduced
    /// block from the latest longest chain.
    pub fn process_reorg(
        &self,
        removed_blocks: Vec<Arc<Block>>,
        new_blocks: Vec<Arc<Block>>,
    ) -> Result<(), MempoolError>
    {
        self.pool_storage
            .write()
            .map_err(|e| MempoolError::BackendError(e.to_string()))?
            .process_reorg(removed_blocks, new_blocks)
    }

    /// Returns all unconfirmed transaction stored in the Mempool, except the transactions stored in the ReOrgPool.
    // TODO: Investigate returning an iterator rather than a large vector of transactions
    pub fn snapshot(&self) -> Result<Vec<Arc<Transaction>>, MempoolError> {
        self.pool_storage
            .read()
            .map_err(|e| MempoolError::BackendError(e.to_string()))?
            .snapshot()
    }

    /// Returns a list of transaction ranked by transaction priority up to a given weight.
    /// Only transactions that fit into a block will be returned
    pub fn retrieve(&self, total_weight: u64) -> Result<Vec<Arc<Transaction>>, MempoolError> {
        self.pool_storage
            .read()
            .map_err(|e| MempoolError::BackendError(e.to_string()))?
            .retrieve(total_weight)
    }

    /// Check if the specified transaction is stored in the Mempool.
    pub fn has_tx_with_excess_sig(&self, excess_sig: Signature) -> Result<TxStorageResponse, MempoolError> {
        self.pool_storage
            .read()
            .map_err(|e| MempoolError::BackendError(e.to_string()))?
            .has_tx_with_excess_sig(excess_sig)
    }

    /// Gathers and returns the stats of the Mempool.
    pub fn stats(&self) -> Result<StatsResponse, MempoolError> {
        self.pool_storage
            .read()
            .map_err(|e| MempoolError::BackendError(e.to_string()))?
            .stats()
    }

    /// Gathers and returns a breakdown of all the transaction in the Mempool.
    pub fn state(&self) -> Result<StateResponse, MempoolError> {
        self.pool_storage
            .read()
            .map_err(|e| MempoolError::BackendError(e.to_string()))?
            .state()
    }
}
