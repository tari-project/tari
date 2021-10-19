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
    consensus::ConsensusManager,
    mempool::{
        error::MempoolError,
        reorg_pool::ReorgPool,
        unconfirmed_pool::UnconfirmedPool,
        MempoolConfig,
        StateResponse,
        StatsResponse,
        TxStorageResponse,
    },
    transactions::{transaction::Transaction, weight::TransactionWeight},
    validation::{MempoolTransactionValidation, ValidationError},
};
use log::*;
use std::sync::Arc;
use tari_common_types::types::Signature;
use tari_crypto::tari_utilities::{hex::Hex, Hashable};

pub const LOG_TARGET: &str = "c::mp::mempool_storage";

/// The Mempool consists of an Unconfirmed Transaction Pool and Reorg Pool and is responsible
/// for managing and maintaining all unconfirmed transactions have not yet been included in a block, and transactions
/// that have recently been included in a block.
pub struct MempoolStorage {
    unconfirmed_pool: UnconfirmedPool,
    reorg_pool: ReorgPool,
    validator: Arc<dyn MempoolTransactionValidation>,
    rules: ConsensusManager,
}

impl MempoolStorage {
    /// Create a new Mempool with an UnconfirmedPool and ReOrgPool.
    pub fn new(
        config: MempoolConfig,
        rules: ConsensusManager,
        validator: Arc<dyn MempoolTransactionValidation>,
    ) -> Self {
        Self {
            unconfirmed_pool: UnconfirmedPool::new(config.unconfirmed_pool),
            reorg_pool: ReorgPool::new(config.reorg_pool),
            validator,
            rules,
        }
    }

    /// Insert an unconfirmed transaction into the Mempool. The transaction *MUST* have passed through the validation
    /// pipeline already and will thus always be internally consistent by this stage
    pub fn insert(&mut self, tx: Arc<Transaction>) -> Result<TxStorageResponse, MempoolError> {
        debug!(
            target: LOG_TARGET,
            "Inserting tx into mempool: {}",
            tx.body
                .kernels()
                .first()
                .map(|k| k.excess_sig.get_signature().to_hex())
                .unwrap_or_else(|| "None".into())
        );
        match self.validator.validate(&tx) {
            Ok(()) => {
                let weight = *self.get_transaction_weight(0);
                self.unconfirmed_pool.insert(tx, None, &weight)?;
                Ok(TxStorageResponse::UnconfirmedPool)
            },
            Err(ValidationError::UnknownInputs(dependent_outputs)) => {
                if self.unconfirmed_pool.verify_outputs_exist(&dependent_outputs) {
                    let weight = *self.get_transaction_weight(0);
                    self.unconfirmed_pool.insert(tx, Some(dependent_outputs), &weight)?;
                    Ok(TxStorageResponse::UnconfirmedPool)
                } else {
                    warn!(target: LOG_TARGET, "Validation failed due to unknown inputs");
                    Ok(TxStorageResponse::NotStoredOrphan)
                }
            },
            Err(ValidationError::ContainsSTxO) => {
                warn!(target: LOG_TARGET, "Validation failed due to already spent output");
                Ok(TxStorageResponse::NotStoredAlreadySpent)
            },
            Err(ValidationError::MaturityError) => {
                warn!(target: LOG_TARGET, "Validation failed due to maturity error");
                Ok(TxStorageResponse::NotStoredTimeLocked)
            },
            Err(e) => {
                warn!(target: LOG_TARGET, "Validation failed due to error:{}", e);
                Ok(TxStorageResponse::NotStored)
            },
        }
    }

    fn get_transaction_weight(&self, height: u64) -> &TransactionWeight {
        self.rules.consensus_constants(height).transaction_weight()
    }

    // Insert a set of new transactions into the UTxPool.
    fn insert_txs(&mut self, txs: Vec<Arc<Transaction>>) -> Result<(), MempoolError> {
        for tx in txs {
            self.insert(tx)?;
        }
        Ok(())
    }

    /// Update the Mempool based on the received published block.
    pub fn process_published_block(&mut self, published_block: Arc<Block>) -> Result<(), MempoolError> {
        trace!(target: LOG_TARGET, "Mempool processing new block: {}", published_block);
        // Move published txs to ReOrgPool and discard double spends
        self.reorg_pool.insert_txs(
            self.unconfirmed_pool
                .remove_published_and_discard_deprecated_transactions(&published_block),
        )?;

        Ok(())
    }

    /// In the event of a ReOrg, resubmit all ReOrged transactions into the Mempool and process each newly introduced
    /// block from the latest longest chain.
    pub fn process_reorg(
        &mut self,
        removed_blocks: Vec<Arc<Block>>,
        new_blocks: Vec<Arc<Block>>,
    ) -> Result<(), MempoolError> {
        debug!(target: LOG_TARGET, "Mempool processing reorg");
        for block in &removed_blocks {
            debug!(
                target: LOG_TARGET,
                "Mempool processing reorg removed block {} ({})",
                block.header.height,
                block.header.hash().to_hex(),
            );
        }
        for block in &new_blocks {
            debug!(
                target: LOG_TARGET,
                "Mempool processing reorg added new block {} ({})",
                block.header.height,
                block.header.hash().to_hex(),
            );
        }

        let previous_tip = removed_blocks.last().map(|block| block.header.height);
        let new_tip = new_blocks.last().map(|block| block.header.height);

        // Clear out all transactions from the unconfirmed pool and re-submit them to the unconfirmed mempool for
        // validation. This is important as invalid transactions that have not been mined yet may remain in the mempool
        // after a reorg.
        let removed_txs = self.unconfirmed_pool.drain_all_mempool_transactions();
        self.insert_txs(removed_txs)?;
        // Remove re-orged transactions from reorg  pool and re-submit them to the unconfirmed mempool
        let removed_txs = self
            .reorg_pool
            .remove_reorged_txs_and_discard_double_spends(removed_blocks, &new_blocks)?;
        self.insert_txs(removed_txs)?;
        // Update the Mempool based on the received set of new blocks.
        for block in new_blocks {
            self.process_published_block(block)?;
        }

        if let (Some(previous_tip_height), Some(new_tip_height)) = (previous_tip, new_tip) {
            if new_tip_height < previous_tip_height {
                debug!(
                    target: LOG_TARGET,
                    "Checking for time locked transactions in unconfirmed pool as chain height was reduced from {} to \
                     {} during reorg.",
                    previous_tip_height,
                    new_tip_height,
                );
                self.unconfirmed_pool.remove_timelocked(new_tip_height);
            } else {
                debug!(
                    target: LOG_TARGET,
                    "No need to check for time locked transactions in unconfirmed pool. Previous tip height: {}. New \
                     tip height: {}.",
                    previous_tip_height,
                    new_tip_height,
                );
            }
        }

        Ok(())
    }

    /// Returns all unconfirmed transaction stored in the Mempool, except the transactions stored in the ReOrgPool.
    // TODO: Investigate returning an iterator rather than a large vector of transactions
    pub fn snapshot(&self) -> Result<Vec<Arc<Transaction>>, MempoolError> {
        let txs = self.unconfirmed_pool.snapshot();
        Ok(txs)
    }

    /// Returns a list of transaction ranked by transaction priority up to a given weight.
    /// Will only return transactions that will fit into a block
    pub fn retrieve(&mut self, total_weight: u64) -> Result<Vec<Arc<Transaction>>, MempoolError> {
        let results = self.unconfirmed_pool.highest_priority_txs(total_weight)?;
        self.insert_txs(results.transactions_to_insert)?;
        Ok(results.retrieved_transactions)
    }

    /// Check if the specified transaction is stored in the Mempool.
    pub fn has_tx_with_excess_sig(&self, excess_sig: Signature) -> Result<TxStorageResponse, MempoolError> {
        if self.unconfirmed_pool.has_tx_with_excess_sig(&excess_sig) {
            Ok(TxStorageResponse::UnconfirmedPool)
        } else if self.reorg_pool.has_tx_with_excess_sig(&excess_sig)? {
            Ok(TxStorageResponse::ReorgPool)
        } else {
            Ok(TxStorageResponse::NotStored)
        }
    }

    // Returns the total number of transactions in the Mempool.
    fn len(&self) -> Result<usize, MempoolError> {
        Ok(self.unconfirmed_pool.len())
    }

    /// Gathers and returns the stats of the Mempool.
    pub fn stats(&mut self) -> Result<StatsResponse, MempoolError> {
        let weight = *self.get_transaction_weight(0);
        Ok(StatsResponse {
            total_txs: self.len()?,
            unconfirmed_txs: self.unconfirmed_pool.len(),
            reorg_txs: self.reorg_pool.len()?,
            total_weight: self.unconfirmed_pool.calculate_weight(&weight),
        })
    }

    /// Gathers and returns a breakdown of all the transaction in the Mempool.
    pub fn state(&mut self) -> Result<StateResponse, MempoolError> {
        let unconfirmed_pool = self
            .unconfirmed_pool
            .snapshot()
            .iter()
            .map(|tx| tx.as_ref().clone())
            .collect::<Vec<_>>();
        let reorg_pool = self
            .reorg_pool
            .snapshot()?
            .iter()
            .map(|tx| tx.body.kernels()[0].excess_sig.clone())
            .collect::<Vec<_>>();
        Ok(StateResponse {
            unconfirmed_pool,
            reorg_pool,
        })
    }
}
