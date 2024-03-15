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

use std::{sync::Arc, time::Instant};

use log::*;
use tari_common_types::types::{PrivateKey, Signature};
use tari_utilities::hex::Hex;

use crate::{
    blocks::Block,
    consensus::ConsensusManager,
    mempool::{
        error::MempoolError,
        reorg_pool::ReorgPool,
        unconfirmed_pool::{RetrieveResults, TransactionKey, UnconfirmedPool, UnconfirmedPoolError},
        FeePerGramStat,
        MempoolConfig,
        StateResponse,
        StatsResponse,
        TxStorageResponse,
    },
    transactions::{
        transaction_components::{Transaction, TransactionError},
        weight::TransactionWeight,
    },
    validation::{TransactionValidator, ValidationError},
};

pub const LOG_TARGET: &str = "c::mp::mempool_storage";

/// The Mempool consists of an Unconfirmed Transaction Pool and Reorg Pool and is responsible
/// for managing and maintaining all unconfirmed transactions have not yet been included in a block, and transactions
/// that have recently been included in a block.
pub struct MempoolStorage {
    pub(crate) unconfirmed_pool: UnconfirmedPool,
    reorg_pool: ReorgPool,
    validator: Box<dyn TransactionValidator>,
    rules: ConsensusManager,
    last_seen_height: u64,
}

impl MempoolStorage {
    /// Create a new Mempool with an UnconfirmedPool and ReOrgPool.
    pub fn new(config: MempoolConfig, rules: ConsensusManager, validator: Box<dyn TransactionValidator>) -> Self {
        Self {
            unconfirmed_pool: UnconfirmedPool::new(config.unconfirmed_pool),
            reorg_pool: ReorgPool::new(config.reorg_pool),
            validator,
            rules,
            last_seen_height: 0,
        }
    }

    /// Insert an unconfirmed transaction into the Mempool.
    pub fn insert(&mut self, tx: Arc<Transaction>) -> Result<TxStorageResponse, UnconfirmedPoolError> {
        let tx_id = tx
            .body
            .kernels()
            .first()
            .map(|k| k.excess_sig.get_signature().to_hex())
            .unwrap_or_else(|| "None?!".into());
        let timer = Instant::now();
        debug!(target: LOG_TARGET, "Inserting tx into mempool: {}", tx_id);
        let tx_fee = match tx.body.get_total_fee() {
            Ok(fee) => fee,
            Err(e) => {
                warn!(target: LOG_TARGET, "Invalid transaction: {}", e);
                return Ok(TxStorageResponse::NotStoredConsensus);
            },
        };
        // This check is almost free, so lets check this before we do any expensive validation.
        if tx_fee.as_u64() < self.unconfirmed_pool.config.min_fee {
            debug!(target: LOG_TARGET, "Tx: ({}) fee too low, rejecting",tx_id);
            return Ok(TxStorageResponse::NotStoredFeeTooLow);
        }
        match self.validator.validate(&tx) {
            Ok(()) => {
                debug!(
                    target: LOG_TARGET,
                    "Transaction {} is VALID ({:.2?}), inserting in unconfirmed pool in",
                    tx_id,
                    timer.elapsed()
                );
                let timer = Instant::now();
                let weight = self.get_transaction_weighting();
                self.unconfirmed_pool.insert(tx, None, &weight)?;
                debug!(
                    target: LOG_TARGET,
                    "Transaction {} inserted in {:.2?}",
                    tx_id,
                    timer.elapsed()
                );
                Ok(TxStorageResponse::UnconfirmedPool)
            },
            Err(ValidationError::UnknownInputs(dependent_outputs)) => {
                if self.unconfirmed_pool.contains_all_outputs(&dependent_outputs) {
                    let weight = self.get_transaction_weighting();
                    self.unconfirmed_pool.insert(tx, Some(dependent_outputs), &weight)?;
                    Ok(TxStorageResponse::UnconfirmedPool)
                } else {
                    warn!(target: LOG_TARGET, "Validation failed due to unknown inputs");
                    Ok(TxStorageResponse::NotStoredOrphan)
                }
            },
            Err(ValidationError::ContainsSTxO) => {
                warn!(target: LOG_TARGET, "Validation failed due to already spent input");
                Ok(TxStorageResponse::NotStoredAlreadySpent)
            },
            Err(ValidationError::MaturityError) => {
                warn!(target: LOG_TARGET, "Validation failed due to maturity error");
                Ok(TxStorageResponse::NotStoredTimeLocked)
            },
            Err(ValidationError::ConsensusError(msg)) => {
                warn!(target: LOG_TARGET, "Validation failed due to consensus rule: {}", msg);
                Ok(TxStorageResponse::NotStoredConsensus)
            },
            Err(ValidationError::DuplicateKernelError(msg)) => {
                debug!(
                    target: LOG_TARGET,
                    "Validation failed due to already mined kernel: {}", msg
                );
                Ok(TxStorageResponse::NotStoredAlreadyMined)
            },
            Err(e) => {
                eprintln!("Validation failed due to error: {}", e);
                warn!(target: LOG_TARGET, "Validation failed due to error: {}", e);
                Ok(TxStorageResponse::NotStored)
            },
        }
    }

    fn get_transaction_weighting(&self) -> TransactionWeight {
        *self
            .rules
            .consensus_constants(self.last_seen_height)
            .transaction_weight_params()
    }

    /// Ensures that all transactions are safely deleted in order and from all storage and then
    /// re-inserted
    pub(crate) fn remove_and_reinsert_transactions(
        &mut self,
        transactions: Vec<(TransactionKey, Arc<Transaction>)>,
    ) -> Result<(), MempoolError> {
        for (tx_key, _) in &transactions {
            self.unconfirmed_pool
                .remove_transaction(*tx_key)
                .map_err(|e| MempoolError::InternalError(e.to_string()))?;
        }
        self.insert_txs(transactions.iter().map(|(_, tx)| tx.clone()).collect())
            .map_err(|e| MempoolError::InternalError(e.to_string()))?;

        Ok(())
    }

    // Insert a set of new transactions into the UTxPool.
    fn insert_txs(&mut self, txs: Vec<Arc<Transaction>>) -> Result<(), UnconfirmedPoolError> {
        for tx in txs {
            self.insert(tx)?;
        }
        Ok(())
    }

    /// Update the Mempool based on the received published block.
    pub fn process_published_block(&mut self, published_block: &Block) -> Result<(), MempoolError> {
        debug!(
            target: LOG_TARGET,
            "Mempool processing new block: #{} ({}) {}",
            published_block.header.height,
            published_block.header.hash().to_hex(),
            published_block.body.to_counts_string()
        );
        let timer = Instant::now();
        // Move published txs to ReOrgPool and discard double spends
        let removed_transactions = self
            .unconfirmed_pool
            .remove_published_and_discard_deprecated_transactions(published_block)?;
        debug!(
            target: LOG_TARGET,
            "{} transactions removed from unconfirmed pool in {:.2?}, moving them to reorg pool for block #{} ({}) {}",
            removed_transactions.len(),
            timer.elapsed(),
            published_block.header.height,
            published_block.header.hash().to_hex(),
            published_block.body.to_counts_string()
        );
        let timer = Instant::now();
        self.reorg_pool
            .insert_all(published_block.header.height, removed_transactions);
        debug!(
            target: LOG_TARGET,
            "Transactions added to reorg pool in {:.2?} for block #{} ({}) {}",
            timer.elapsed(),
            published_block.header.height,
            published_block.header.hash().to_hex(),
            published_block.body.to_counts_string()
        );
        let timer = Instant::now();
        self.unconfirmed_pool.compact();
        self.reorg_pool.compact();

        self.last_seen_height = published_block.header.height;
        debug!(target: LOG_TARGET, "Compaction took {:.2?}", timer.elapsed());
        match self.stats() {
            Ok(stats) => debug!(target: LOG_TARGET, "{}", stats),
            Err(e) => warn!(target: LOG_TARGET, "error to obtain stats: {}", e),
        }
        Ok(())
    }

    pub fn clear_transactions_for_failed_block(&mut self, failed_block: &Block) -> Result<(), MempoolError> {
        warn!(
            target: LOG_TARGET,
            "Removing transaction from failed block #{} ({})",
            failed_block.header.height,
            failed_block.hash().to_hex()
        );
        let txs = self
            .unconfirmed_pool
            .remove_published_and_discard_deprecated_transactions(failed_block)?;

        // Reinsert them to validate if they are still valid
        self.insert_txs(txs)
            .map_err(|e| MempoolError::InternalError(e.to_string()))?;
        self.unconfirmed_pool.compact();

        Ok(())
    }

    /// In the event of a ReOrg, resubmit all ReOrged transactions into the Mempool and process each newly introduced
    /// block from the latest longest chain.
    pub fn process_reorg(
        &mut self,
        removed_blocks: &[Arc<Block>],
        new_blocks: &[Arc<Block>],
    ) -> Result<(), MempoolError> {
        debug!(target: LOG_TARGET, "Mempool processing reorg");

        // Clear out all transactions from the unconfirmed pool and re-submit them to the unconfirmed mempool for
        // validation. This is important as invalid transactions that have not been mined yet may remain in the mempool
        // after a reorg.
        let removed_txs = self.unconfirmed_pool.drain_all_mempool_transactions();
        // Try to add in all the transactions again.
        self.insert_txs(removed_txs)
            .map_err(|e| MempoolError::InternalError(e.to_string()))?;
        // Remove re-orged transactions from reorg  pool and re-submit them to the unconfirmed mempool
        let removed_txs = self
            .reorg_pool
            .remove_reorged_txs_and_discard_double_spends(removed_blocks, new_blocks);
        self.insert_txs(removed_txs)
            .map_err(|e| MempoolError::InternalError(e.to_string()))?;
        if let Some(height) = new_blocks
            .last()
            .or_else(|| removed_blocks.first())
            .map(|block| block.header.height)
        {
            self.last_seen_height = height;
        }
        Ok(())
    }

    /// After a sync event, we need to try to add in all the transaction form the reorg pool.
    pub fn process_sync(&mut self) -> Result<(), MempoolError> {
        debug!(target: LOG_TARGET, "Mempool processing sync finished");
        // lets remove and revalidate all transactions from the mempool. All we know is that the state has changed, but
        // we dont have the data to know what.
        let txs = self.unconfirmed_pool.drain_all_mempool_transactions();
        // lets add them all back into the mempool
        self.insert_txs(txs)
            .map_err(|e| MempoolError::InternalError(e.to_string()))?;
        // let retrieve all re-org pool transactions as well as make sure they are mined as well
        let txs = self.reorg_pool.clear_and_retrieve_all();
        self.insert_txs(txs)
            .map_err(|e| MempoolError::InternalError(e.to_string()))?;
        Ok(())
    }

    /// Returns all unconfirmed transaction stored in the Mempool, except the transactions stored in the ReOrgPool.
    pub fn snapshot(&self) -> Vec<Arc<Transaction>> {
        self.unconfirmed_pool.snapshot()
    }

    /// Returns a list of transaction ranked by transaction priority up to a given weight.
    /// Will only return transactions that will fit into the given weight
    pub fn retrieve(&self, total_weight: u64) -> Result<RetrieveResults, MempoolError> {
        self.unconfirmed_pool
            .fetch_highest_priority_txs(total_weight)
            .map_err(|e| MempoolError::InternalError(e.to_string()))
    }

    pub fn retrieve_by_excess_sigs(
        &self,
        excess_sigs: &[PrivateKey],
    ) -> Result<(Vec<Arc<Transaction>>, Vec<PrivateKey>), MempoolError> {
        let (found_txns, remaining) = self.unconfirmed_pool.retrieve_by_excess_sigs(excess_sigs)?;

        match self.reorg_pool.retrieve_by_excess_sigs(&remaining) {
            Ok((found_published_transactions, remaining)) => Ok((
                found_txns.into_iter().chain(found_published_transactions).collect(),
                remaining,
            )),
            Err(e) => Err(e),
        }
    }

    /// Check if the specified excess signature is found in the Mempool.
    pub fn has_tx_with_excess_sig(&self, excess_sig: &Signature) -> TxStorageResponse {
        if self.unconfirmed_pool.has_tx_with_excess_sig(excess_sig) {
            TxStorageResponse::UnconfirmedPool
        } else if self.reorg_pool.has_tx_with_excess_sig(excess_sig) {
            TxStorageResponse::ReorgPool
        } else {
            TxStorageResponse::NotStored
        }
    }

    /// Check if the specified transaction is stored in the Mempool.
    pub fn has_transaction(&self, tx: &Transaction) -> Result<TxStorageResponse, MempoolError> {
        tx.body
            .kernels()
            .iter()
            .fold(None, |stored, kernel| {
                if stored.is_none() {
                    return Some(self.has_tx_with_excess_sig(&kernel.excess_sig));
                }
                let stored = stored.unwrap();
                match (self.has_tx_with_excess_sig(&kernel.excess_sig), stored) {
                    // All (so far) in unconfirmed pool
                    (TxStorageResponse::UnconfirmedPool, TxStorageResponse::UnconfirmedPool) => {
                        Some(TxStorageResponse::UnconfirmedPool)
                    },
                    // Some kernels from the transaction have already been processed, and others exist in the
                    // unconfirmed pool, therefore this specific transaction has not been stored (already spent)
                    (TxStorageResponse::UnconfirmedPool, TxStorageResponse::ReorgPool) |
                    (TxStorageResponse::ReorgPool, TxStorageResponse::UnconfirmedPool) => {
                        Some(TxStorageResponse::NotStoredAlreadySpent)
                    },
                    // All (so far) in reorg pool
                    (TxStorageResponse::ReorgPool, TxStorageResponse::ReorgPool) => Some(TxStorageResponse::ReorgPool),
                    // Not stored
                    (TxStorageResponse::UnconfirmedPool, other) |
                    (TxStorageResponse::ReorgPool, other) |
                    (other, _) => Some(other),
                }
            })
            .ok_or(MempoolError::TransactionNoKernels)
    }

    /// Gathers and returns the stats of the Mempool.
    pub fn stats(&self) -> Result<StatsResponse, TransactionError> {
        let weighting = self.get_transaction_weighting();
        Ok(StatsResponse {
            unconfirmed_txs: self.unconfirmed_pool.len() as u64,
            reorg_txs: self.reorg_pool.len() as u64,
            unconfirmed_weight: self.unconfirmed_pool.calculate_weight(&weighting)?,
        })
    }

    /// Gathers and returns a breakdown of all the transaction in the Mempool.
    pub fn state(&self) -> StateResponse {
        let unconfirmed_pool = self.unconfirmed_pool.snapshot();
        let reorg_pool = self
            .reorg_pool
            .snapshot()
            .iter()
            .map(|tx| tx.first_kernel_excess_sig().cloned().unwrap_or_default())
            .collect::<Vec<_>>();
        StateResponse {
            unconfirmed_pool,
            reorg_pool,
        }
    }

    pub fn get_fee_per_gram_stats(&self, count: usize, tip_height: u64) -> Result<Vec<FeePerGramStat>, MempoolError> {
        let target_weight = self
            .rules
            .consensus_constants(tip_height)
            .max_block_weight_excluding_coinbase()
            .map_err(|e| MempoolError::InternalError(e.to_string()))?;
        let stats = self.unconfirmed_pool.get_fee_per_gram_stats(count, target_weight)?;
        Ok(stats)
    }
}
