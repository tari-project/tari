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

use std::{
    sync::{Arc, RwLock},
    time::Instant,
};

use log::*;
use tari_common_types::{
    chain_metadata::ChainMetadata,
    types::{CompressedSignature, FixedHash, HashOutput, PrivateKey},
};
use tari_node_components::blocks::Block;
use tari_transaction_components::{
    rpc::models::FeePerGramStat,
    transaction_components::{Transaction, TransactionError},
    weight::TransactionWeight,
};
use tari_utilities::hex::Hex;

#[cfg(feature = "metrics")]
use crate::mempool::metrics;
use crate::{
    consensus::BaseNodeConsensusManager,
    mempool::{
        MempoolConfig,
        StateResponse,
        StatsResponse,
        TxStorageResponse,
        error::MempoolError,
        reorg_pool::ReorgPool,
        unconfirmed_pool::{RetrieveResults, TransactionKey, UnconfirmedPool, UnconfirmedPoolError},
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
    validator: Arc<dyn TransactionValidator>,
    rules: BaseNodeConsensusManager,
    pub(crate) last_seen_height: u64,
    pub(crate) last_seen_hash: FixedHash,
}

impl MempoolStorage {
    /// Create a new Mempool with an UnconfirmedPool and ReOrgPool.
    pub fn new(
        config: MempoolConfig,
        rules: BaseNodeConsensusManager,
        validator: Box<dyn TransactionValidator>,
    ) -> Self {
        Self {
            unconfirmed_pool: UnconfirmedPool::new(config.unconfirmed_pool),
            reorg_pool: ReorgPool::new(config.reorg_pool),
            validator: Arc::from(validator),
            rules,
            last_seen_height: 0,
            last_seen_hash: Default::default(),
        }
    }

    /// Insert an unconfirmed transaction into the Mempool, validating it while holding the storage (write) lock.
    ///
    /// This is used to revalidate transactions that were already in the mempool (after a sync, reorg or failed block).
    /// New transactions from peers or clients are inserted with [`MempoolStorage::insert_unlocked`] instead, so that
    /// validating them does not block the mempool.
    pub fn insert(&mut self, tx: Arc<Transaction>) -> Result<TxStorageResponse, UnconfirmedPoolError> {
        let timer = Instant::now();
        if let Some(response) = self.check_fee(&tx) {
            return Ok(response);
        }
        let dependent_outputs = match self.validate_locked(&tx) {
            Ok(dependent_outputs) => dependent_outputs,
            Err(response) => return Ok(response),
        };
        debug!(
            target: LOG_TARGET,
            "Transaction {} is VALID ({:.2?}), inserting in unconfirmed pool",
            tx_id(&tx),
            timer.elapsed()
        );
        self.insert_into_unconfirmed_pool(tx, dependent_outputs)
    }

    /// Insert a new unconfirmed transaction into the Mempool, without holding the storage write lock while validating
    /// it.
    ///
    /// A snapshot of the chain tip is taken first, and all validation is done against it. The transaction is first
    /// validated against the chain state. If some of its inputs spend outputs that are not in the chain, those outputs
    /// must be in the mempool, exactly as the inputs spend them, before anything else is done (a read lock is held
    /// briefly for this lookup). Only then is the expensive internal consistency validation (scripts, signatures and
    /// range proofs) performed, with no lock held. Finally, the write lock is taken to insert the transaction. If the
    /// chain tip has moved since the snapshot (which requires a new block, so is rare), the transaction is fully
    /// validated again under the lock, as by [`MempoolStorage::insert`]. Otherwise only the mempool parents of the
    /// transaction are looked up again, since they may have been removed in the meantime.
    pub fn insert_unlocked(
        storage: &RwLock<MempoolStorage>,
        tx: Arc<Transaction>,
    ) -> Result<TxStorageResponse, MempoolError> {
        let timer = Instant::now();
        let validator = {
            let lock = storage.read().map_err(|_| MempoolError::RwLockPoisonError)?;
            if let Some(response) = lock.check_fee(&tx) {
                return Ok(response);
            }
            lock.validator.clone()
        };

        let tip = validator.chain_metadata().unwrap_or_else(|e| {
            warn!(target: LOG_TARGET, "Could not fetch the chain tip: {e}");
            None
        });
        let dependent_outputs = match validate_chain_linked(validator.as_ref(), &tx) {
            Ok(dependent_outputs) => dependent_outputs,
            Err(response) => return Ok(response),
        };
        if let Some(dependent_outputs) = &dependent_outputs {
            let lock = storage.read().map_err(|_| MempoolError::RwLockPoisonError)?;
            if let Err(response) = check_pool_parents(&lock.unconfirmed_pool, &tx, dependent_outputs) {
                return Ok(response);
            }
        }
        if let Err(response) = validate_internal_consistency(validator.as_ref(), &tx, tip.as_ref()) {
            return Ok(response);
        }
        debug!(
            target: LOG_TARGET,
            "Transaction {} is VALID ({:.2?}), inserting in unconfirmed pool",
            tx_id(&tx),
            timer.elapsed()
        );

        let mut lock = storage.write().map_err(|_| MempoolError::RwLockPoisonError)?;
        let tip_unchanged = match (tip, validator.chain_metadata()) {
            (Some(validated_at), Ok(Some(current))) => validated_at.best_block_hash() == current.best_block_hash(),
            _ => false,
        };
        if !tip_unchanged {
            debug!(
                target: LOG_TARGET,
                "Chain tip changed while validating transaction {}, validating it again",
                tx_id(&tx)
            );
            return lock.insert(tx).map_err(|e| MempoolError::InternalError(e.to_string()));
        }
        // The parents may have been removed from the pool since they were looked up
        if let Some(dependent_outputs) = &dependent_outputs &&
            let Err(response) = check_pool_parents(&lock.unconfirmed_pool, &tx, dependent_outputs)
        {
            return Ok(response);
        }
        lock.insert_into_unconfirmed_pool(tx, dependent_outputs)
            .map_err(|e| MempoolError::InternalError(e.to_string()))
    }

    /// Validates the transaction in the same order as [`MempoolStorage::insert_unlocked`]: chain-linked checks, then
    /// the mempool parents of any inputs not in the chain, then internal consistency.
    fn validate_locked(&self, tx: &Transaction) -> Result<Option<Vec<HashOutput>>, TxStorageResponse> {
        let dependent_outputs = validate_chain_linked(self.validator.as_ref(), tx)?;
        if let Some(dependent_outputs) = &dependent_outputs {
            check_pool_parents(&self.unconfirmed_pool, tx, dependent_outputs)?;
        }
        validate_internal_consistency(self.validator.as_ref(), tx, None)?;
        Ok(dependent_outputs)
    }

    /// Checks the fee of the transaction, which is almost free, so is done before any expensive validation. Returns
    /// the response if the transaction is rejected.
    fn check_fee(&self, tx: &Transaction) -> Option<TxStorageResponse> {
        let tx_fee = match tx.body.get_total_fee() {
            Ok(fee) => fee,
            Err(e) => {
                warn!(target: LOG_TARGET, "Invalid transaction: {e}");
                return Some(TxStorageResponse::NotStoredConsensus(Some(e.to_string())));
            },
        };
        if tx_fee.as_u64() < self.unconfirmed_pool.config.min_fee {
            debug!(target: LOG_TARGET, "Tx: ({}) fee too low, rejecting", tx_id(tx));
            return Some(TxStorageResponse::NotStoredFeeTooLow);
        }
        None
    }

    fn insert_into_unconfirmed_pool(
        &mut self,
        tx: Arc<Transaction>,
        dependent_outputs: Option<Vec<HashOutput>>,
    ) -> Result<TxStorageResponse, UnconfirmedPoolError> {
        let timer = Instant::now();
        let tx_id = tx_id(&tx);
        let weight = self.get_transaction_weighting();
        self.unconfirmed_pool.insert(tx, dependent_outputs, &weight)?;
        debug!(
            target: LOG_TARGET,
            "Transaction {} inserted in {:.2?}",
            tx_id,
            timer.elapsed()
        );
        Ok(TxStorageResponse::UnconfirmedPool)
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
        self.last_seen_hash = published_block.header.hash();
        debug!(target: LOG_TARGET, "Compaction took {:.2?}", timer.elapsed());
        match self.stats() {
            Ok(stats) => debug!(target: LOG_TARGET, "{stats}"),
            Err(e) => warn!(target: LOG_TARGET, "error to obtain stats: {e}"),
        }

        // we set this to 0, as we have not removed any invalid double spent txs due to a reorg
        #[cfg(feature = "metrics")]
        metrics::reorg_invalid_transactions().set(0);
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

        let mut num_invalid_txs: i64 = 0;

        // Clear out all transactions from the unconfirmed pool and re-submit them to the unconfirmed mempool for
        // validation. This is important as invalid transactions that have not been mined yet may remain in the mempool
        // after a reorg.
        let removed_txs = self.unconfirmed_pool.drain_all_mempool_transactions();
        let num_removed_txs = removed_txs.len();
        // Try to add in all the transactions again.
        for tx in removed_txs {
            let resp = self
                .insert(tx)
                .map_err(|e| MempoolError::InternalError(e.to_string()))?;
            if resp == TxStorageResponse::NotStoredAlreadySpent {
                num_invalid_txs = num_invalid_txs.saturating_add(1);
            }
        }

        // Remove re-orged transactions from reorg pool and re-submit them to the unconfirmed mempool
        let reorg_txs = self
            .reorg_pool
            .remove_reorged_txs_and_discard_double_spends(removed_blocks, new_blocks);
        let num_reorg_txs = reorg_txs.len();
        for tx in reorg_txs {
            let resp = self
                .insert(tx)
                .map_err(|e| MempoolError::InternalError(e.to_string()))?;
            if resp == TxStorageResponse::NotStoredAlreadySpent {
                num_invalid_txs = num_invalid_txs.saturating_add(1);
            }
        }

        if num_invalid_txs > 0 {
            warn!(
                target: LOG_TARGET,
                "Mempool reorg: {num_invalid_txs} transaction(s) invalidated \
                 (from {num_removed_txs} unconfirmed and {num_reorg_txs} reorg pool transactions)"
            );
        }
        #[cfg(feature = "metrics")]
        metrics::reorg_invalid_transactions().set(num_invalid_txs);

        if let Some((height, hash)) = new_blocks
            .last()
            .or_else(|| removed_blocks.first())
            .map(|block| (block.header.height, block.header.hash()))
        {
            self.last_seen_height = height;
            self.last_seen_hash = hash;
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

    /// Returns the subset of provided output hashes that exist in the mempool's unconfirmed pool.
    pub fn filter_outputs_in_mempool(&self, output_hashes: &[HashOutput]) -> Vec<HashOutput> {
        self.unconfirmed_pool.filter_outputs(output_hashes)
    }

    /// Check if the specified excess signature is found in the Mempool.
    pub fn has_tx_with_excess_sig(&self, excess_sig: &CompressedSignature) -> TxStorageResponse {
        if self.unconfirmed_pool.has_tx_with_excess_sig(excess_sig) {
            TxStorageResponse::UnconfirmedPool
        } else if self.reorg_pool.has_tx_with_excess_sig(excess_sig) {
            TxStorageResponse::ReorgPool
        } else {
            TxStorageResponse::NotStored(None)
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
            .max_block_transaction_weight();
        let stats = self.unconfirmed_pool.get_fee_per_gram_stats(count, target_weight)?;
        Ok(stats)
    }
}

fn tx_id(tx: &Transaction) -> String {
    tx.body
        .kernels()
        .first()
        .map(|k| k.excess_sig.get_signature().to_hex())
        .unwrap_or_else(|| "None?!".into())
}

/// Validates the transaction against the chain state. On success, returns the hashes of the outputs spent by the
/// transaction that are not in the chain (and must therefore be in the mempool), if any. On failure, returns the
/// storage response for the rejected transaction.
fn validate_chain_linked(
    validator: &dyn TransactionValidator,
    tx: &Transaction,
) -> Result<Option<Vec<HashOutput>>, TxStorageResponse> {
    match validator.validate_chain_linked(tx) {
        Ok(()) => Ok(None),
        Err(ValidationError::UnknownInputs(dependent_outputs)) => Ok(Some(dependent_outputs)),
        Err(e) => Err(validation_error_to_response(e)),
    }
}

fn validate_internal_consistency(
    validator: &dyn TransactionValidator,
    tx: &Transaction,
    tip: Option<&ChainMetadata>,
) -> Result<(), TxStorageResponse> {
    validator
        .validate_internal_consistency(tx, tip)
        .map_err(validation_error_to_response)
}

/// Checks that every output in `dependent_outputs` is in the unconfirmed pool, and that each input spending one of
/// them spends it exactly as it was created there (the output with the input's commitment has the input's output hash,
/// so that e.g. its script is the one that was committed to).
fn check_pool_parents(
    pool: &UnconfirmedPool,
    tx: &Transaction,
    dependent_outputs: &[HashOutput],
) -> Result<(), TxStorageResponse> {
    if !pool.contains_all_outputs(dependent_outputs) {
        return Err(TxStorageResponse::NotStoredOrphan);
    }
    for input in tx.body.inputs() {
        let output_hash = input.output_hash();
        if !dependent_outputs.contains(&output_hash) {
            continue;
        }
        let matches = input
            .commitment()
            .is_ok_and(|commitment| pool.contains_matching_output(&output_hash, commitment));
        if !matches {
            warn!(
                target: LOG_TARGET,
                "Transaction {} has an input that does not match the mempool output it spends ({})",
                tx_id(tx),
                output_hash.to_hex()
            );
            return Err(TxStorageResponse::NotStored(Some(format!(
                "Input does not match the mempool output it spends ({})",
                output_hash.to_hex()
            ))));
        }
    }
    Ok(())
}

fn validation_error_to_response(error: ValidationError) -> TxStorageResponse {
    match error {
        ValidationError::UnknownInputs(_) => TxStorageResponse::NotStoredOrphan,
        ValidationError::ContainsSTxO => {
            info!(target: LOG_TARGET, "Validation failed due to already spent input");
            TxStorageResponse::NotStoredAlreadySpent
        },
        ValidationError::MaturityError => TxStorageResponse::NotStoredTimeLocked,
        ValidationError::ConsensusError(msg) => {
            warn!(target: LOG_TARGET, "Validation failed due to consensus rule: {msg}");
            TxStorageResponse::NotStoredConsensus(Some(msg))
        },
        ValidationError::DuplicateKernelError(msg) => {
            debug!(
                target: LOG_TARGET,
                "Validation failed due to already mined kernel: {msg}"
            );
            TxStorageResponse::NotStoredAlreadyMined
        },
        e => {
            info!(target: LOG_TARGET, "Validation failed due to error: {e}");
            TxStorageResponse::NotStored(Some(e.to_string()))
        },
    }
}

#[cfg(test)]
mod test {
    #![allow(clippy::indexing_slicing)]
    use std::{
        collections::VecDeque,
        sync::{
            Mutex,
            atomic::{AtomicUsize, Ordering},
        },
    };

    use tari_transaction_components::{
        MicroMinotari,
        key_manager::KeyManager,
        transaction_components::WalletOutput,
        tx,
    };

    use super::*;
    use crate::test_helpers::create_consensus_rules;

    /// A validator that returns the scripted chain-linked results and chain tips in order, and records how often each
    /// stage of validation is performed
    #[derive(Default)]
    struct ScriptedValidator {
        chain_linked_results: Mutex<VecDeque<Result<(), ValidationError>>>,
        tips: Mutex<VecDeque<FixedHash>>,
        internal_results: Mutex<VecDeque<Result<(), ValidationError>>>,
        internal_tips: Mutex<Vec<Option<FixedHash>>>,
        chain_linked_calls: AtomicUsize,
        internal_calls: AtomicUsize,
    }

    impl ScriptedValidator {
        fn new(chain_linked_results: Vec<Result<(), ValidationError>>, tips: Vec<FixedHash>) -> Arc<Self> {
            Arc::new(Self {
                chain_linked_results: Mutex::new(chain_linked_results.into()),
                tips: Mutex::new(tips.into()),
                ..Default::default()
            })
        }

        fn chain_linked_calls(&self) -> usize {
            self.chain_linked_calls.load(Ordering::SeqCst)
        }

        fn internal_calls(&self) -> usize {
            self.internal_calls.load(Ordering::SeqCst)
        }

        fn internal_tips(&self) -> Vec<Option<FixedHash>> {
            self.internal_tips.lock().unwrap().clone()
        }
    }

    impl TransactionValidator for Arc<ScriptedValidator> {
        fn validate(&self, _tx: &Transaction) -> Result<(), ValidationError> {
            unreachable!("the mempool validates in stages")
        }

        fn validate_chain_linked(&self, _tx: &Transaction) -> Result<(), ValidationError> {
            self.chain_linked_calls.fetch_add(1, Ordering::SeqCst);
            self.chain_linked_results.lock().unwrap().pop_front().unwrap_or(Ok(()))
        }

        fn validate_internal_consistency(
            &self,
            _tx: &Transaction,
            tip: Option<&ChainMetadata>,
        ) -> Result<(), ValidationError> {
            self.internal_calls.fetch_add(1, Ordering::SeqCst);
            self.internal_tips
                .lock()
                .unwrap()
                .push(tip.map(|tip| *tip.best_block_hash()));
            self.internal_results.lock().unwrap().pop_front().unwrap_or(Ok(()))
        }

        fn chain_metadata(&self) -> Result<Option<ChainMetadata>, ValidationError> {
            let mut tips = self.tips.lock().unwrap();
            // Keep returning the last tip once the scripted ones run out
            let tip = if tips.len() > 1 {
                tips.pop_front()
            } else {
                tips.front().copied()
            };
            Ok(tip.map(|hash| ChainMetadata::new(1, hash, 0, 0, 1u64.into(), 0).unwrap()))
        }
    }

    fn create_storage(validator: &Arc<ScriptedValidator>) -> RwLock<MempoolStorage> {
        let mut config = MempoolConfig::default();
        config.unconfirmed_pool.min_fee = 0;
        RwLock::new(MempoolStorage::new(
            config,
            create_consensus_rules(),
            Box::new(validator.clone()),
        ))
    }

    fn create_tx_with_outputs(key_manager: &KeyManager) -> (Arc<Transaction>, Vec<WalletOutput>) {
        let (tx, _, outputs) = tx!(MicroMinotari(100_000), fee: MicroMinotari(5), inputs: 1, outputs: 1, key_manager)
            .expect("Failed to get tx");
        (Arc::new(tx), outputs)
    }

    fn create_tx(key_manager: &KeyManager) -> Arc<Transaction> {
        create_tx_with_outputs(key_manager).0
    }

    /// A copy of `tx` whose inputs are replaced by one spending `spent`
    fn spending(tx: &Transaction, spent: &WalletOutput, key_manager: &KeyManager) -> Arc<Transaction> {
        let input = spent.to_transaction_input(key_manager).unwrap();
        Arc::new(Transaction::new(
            vec![input],
            tx.body.outputs().clone(),
            tx.body.kernels().clone(),
            tx.offset.clone(),
            tx.script_offset.clone(),
        ))
    }

    fn pool_len(storage: &RwLock<MempoolStorage>) -> usize {
        storage.read().unwrap().unconfirmed_pool.len()
    }

    #[test]
    fn insert_unlocked_does_not_revalidate_if_tip_unchanged() {
        let key_manager = KeyManager::new_random().unwrap();
        let tip = FixedHash::from([1u8; 32]);
        let validator = ScriptedValidator::new(vec![Ok(())], vec![tip]);
        let storage = create_storage(&validator);

        let response = MempoolStorage::insert_unlocked(&storage, create_tx(&key_manager)).unwrap();
        assert_eq!(response, TxStorageResponse::UnconfirmedPool);
        assert_eq!(validator.chain_linked_calls(), 1);
        assert_eq!(validator.internal_calls(), 1);
        // Internal validation used the tip snapshot
        assert_eq!(validator.internal_tips(), vec![Some(tip)]);
        assert_eq!(pool_len(&storage), 1);
    }

    #[test]
    fn insert_unlocked_fully_revalidates_if_tip_changed() {
        let key_manager = KeyManager::new_random().unwrap();
        let validated_at = FixedHash::from([1u8; 32]);
        let new_tip = FixedHash::from([2u8; 32]);

        // Valid at the snapshot, but internally invalid at the new tip (e.g. a script checking the block height)
        let validator = ScriptedValidator::new(vec![], vec![validated_at, new_tip]);
        *validator.internal_results.lock().unwrap() =
            vec![Ok(()), Err(ValidationError::InvalidAccountingBalance)].into();
        let storage = create_storage(&validator);
        let response = MempoolStorage::insert_unlocked(&storage, create_tx(&key_manager)).unwrap();
        assert!(matches!(response, TxStorageResponse::NotStored(_)), "{response:?}");
        assert_eq!(validator.chain_linked_calls(), 2);
        assert_eq!(validator.internal_calls(), 2);
        // The fallback validates against the current tip, not the stale snapshot
        assert_eq!(validator.internal_tips(), vec![Some(validated_at), None]);
        assert_eq!(pool_len(&storage), 0);

        // An input spent by the new tip is caught by the chain-linked checks
        let validator = ScriptedValidator::new(vec![Ok(()), Err(ValidationError::ContainsSTxO)], vec![
            validated_at,
            new_tip,
        ]);
        let storage = create_storage(&validator);
        let response = MempoolStorage::insert_unlocked(&storage, create_tx(&key_manager)).unwrap();
        assert_eq!(response, TxStorageResponse::NotStoredAlreadySpent);
        assert_eq!(validator.chain_linked_calls(), 2);
        assert_eq!(pool_len(&storage), 0);

        // And a transaction that is still valid at the new tip is stored
        let validator = ScriptedValidator::new(vec![], vec![validated_at, new_tip]);
        let storage = create_storage(&validator);
        let response = MempoolStorage::insert_unlocked(&storage, create_tx(&key_manager)).unwrap();
        assert_eq!(response, TxStorageResponse::UnconfirmedPool);
        assert_eq!(validator.chain_linked_calls(), 2);
        assert_eq!(validator.internal_calls(), 2);
        assert_eq!(pool_len(&storage), 1);
    }

    #[test]
    fn orphan_is_rejected_without_internal_validation() {
        let key_manager = KeyManager::new_random().unwrap();
        let unknown_output = FixedHash::from([3u8; 32]);
        let validator = ScriptedValidator::new(
            vec![
                Err(ValidationError::UnknownInputs(vec![unknown_output])),
                Err(ValidationError::UnknownInputs(vec![unknown_output])),
            ],
            vec![FixedHash::zero()],
        );
        let storage = create_storage(&validator);

        let response = MempoolStorage::insert_unlocked(&storage, create_tx(&key_manager)).unwrap();
        assert_eq!(response, TxStorageResponse::NotStoredOrphan);
        let response = storage.write().unwrap().insert(create_tx(&key_manager)).unwrap();
        assert_eq!(response, TxStorageResponse::NotStoredOrphan);
        assert_eq!(validator.internal_calls(), 0);
        assert_eq!(pool_len(&storage), 0);
    }

    #[test]
    fn transaction_with_pool_parents_is_internally_validated() {
        let key_manager = KeyManager::new_random().unwrap();
        let validator = ScriptedValidator::new(vec![], vec![FixedHash::zero()]);
        let storage = create_storage(&validator);
        let (parent, parent_outputs) = create_tx_with_outputs(&key_manager);
        assert_eq!(
            MempoolStorage::insert_unlocked(&storage, parent).unwrap(),
            TxStorageResponse::UnconfirmedPool
        );
        // The child really spends the parent's output, so the pool parent check has an input to match
        let child = spending(&create_tx(&key_manager), &parent_outputs[0], &key_manager);
        let parent_output = child.body.inputs()[0].output_hash();
        assert!(
            storage
                .read()
                .unwrap()
                .unconfirmed_pool
                .contains_matching_output(&parent_output, child.body.inputs()[0].commitment().unwrap())
        );

        for locked in [false, true] {
            *validator.chain_linked_results.lock().unwrap() =
                vec![Err(ValidationError::UnknownInputs(vec![parent_output]))].into();
            *validator.internal_results.lock().unwrap() = vec![Err(ValidationError::InvalidAccountingBalance)].into();
            let internal_calls = validator.internal_calls();
            let response = if locked {
                storage.write().unwrap().insert(child.clone()).unwrap()
            } else {
                MempoolStorage::insert_unlocked(&storage, child.clone()).unwrap()
            };
            assert!(matches!(response, TxStorageResponse::NotStored(_)), "{response:?}");
            assert_eq!(validator.internal_calls(), internal_calls + 1);
            assert_eq!(pool_len(&storage), 1);
        }

        // A child spending an output that is not in the pool is an orphan
        let (_, unknown_outputs) = create_tx_with_outputs(&key_manager);
        let orphan = spending(&create_tx(&key_manager), &unknown_outputs[0], &key_manager);
        *validator.chain_linked_results.lock().unwrap() = vec![Err(ValidationError::UnknownInputs(vec![
            orphan.body.inputs()[0].output_hash(),
        ]))]
        .into();
        let internal_calls = validator.internal_calls();
        let response = MempoolStorage::insert_unlocked(&storage, orphan).unwrap();
        assert_eq!(response, TxStorageResponse::NotStoredOrphan);
        assert_eq!(validator.internal_calls(), internal_calls);

        // Once internally valid, the child is stored with its dependency on the parent
        *validator.chain_linked_results.lock().unwrap() =
            vec![Err(ValidationError::UnknownInputs(vec![parent_output]))].into();
        let response = MempoolStorage::insert_unlocked(&storage, child).unwrap();
        assert_eq!(response, TxStorageResponse::UnconfirmedPool);
        assert_eq!(pool_len(&storage), 2);
    }
}
