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
use tari_script::Opcode;
use tari_transaction_components::{
    helpers::borsh::SerializedSize,
    rpc::models::FeePerGramStat,
    transaction_components::{Transaction, TransactionError},
    validation::AggregatedBodyValidationError,
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
        unconfirmed_pool::{
            MAX_BLOCK_TEMPLATE_BODY_BYTES,
            RetrieveResults,
            TransactionKey,
            UnconfirmedPool,
            UnconfirmedPoolError,
        },
    },
    validation::{TransactionValidator, ValidationError},
};

pub const LOG_TARGET: &str = "c::mp::mempool_storage";

/// The number of times a new transaction is validated if the mempool processes a chain change during validation
pub const MAX_VALIDATION_ATTEMPTS: usize = 3;

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
    /// Incremented every time the mempool processes a change to the chain (block, reorg, sync or failed block)
    pub(crate) chain_generation: u64,
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
            chain_generation: 0,
        }
    }

    /// Insert an unconfirmed transaction into the Mempool, validating it while holding the storage (write) lock.
    ///
    /// This is used to revalidate transactions that were already in the mempool (after a sync, reorg or failed block).
    /// New transactions from peers or clients are inserted with [`MempoolStorage::insert_unlocked`] instead, so that
    /// validating them does not block the mempool.
    pub fn insert(&mut self, tx: Arc<Transaction>) -> Result<TxStorageResponse, UnconfirmedPoolError> {
        let timer = Instant::now();
        if let Some(response) = self
            .check_fee(&tx)
            .or_else(|| Self::check_body_size(&tx))
            .or_else(|| Self::check_kernel_excesses(&tx))
        {
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
    /// A snapshot of the chain tip is taken first, and internal validation is done against it. The transaction is
    /// first validated against the chain state. If some of its inputs spend outputs that are not in the chain, those
    /// outputs must be in the mempool before anything else is done (a read lock is held briefly for this lookup). Only
    /// then is the expensive internal consistency validation (scripts, signatures and range proofs) performed, with no
    /// lock held. Finally, the write lock is taken to insert the transaction.
    ///
    /// The mempool's view of the chain only changes when it processes a block, reorg, sync or failed block, each of
    /// which bumps [`MempoolStorage::chain_generation`]. If that happened while the transaction was being validated,
    /// the successful validation is stale: it is repeated from the start, without the lock. If the last unlocked
    /// attempt is also a stale success, the final one of the [`MAX_VALIDATION_ATTEMPTS`] attempts validates under the
    /// lock. A chain change that the mempool has not processed yet by the time the transaction is inserted is safe,
    /// since the mempool reconciles every stored transaction with it when it does process it.
    ///
    /// Most failures are final. Only a failure that depends on the chain tip (inputs not in the chain or pool, input
    /// maturity and lock heights, and scripts that read the block height; see `is_tip_dependent`) may be due to the
    /// chain changing during validation. Such a failure is validated again, in the same way, if the mempool processed
    /// a chain change or the chain tip moved since the attempt started. The near-free fee and weight checks do not
    /// depend on the chain and are not repeated. Failure-driven retries are capped at the unlocked attempts and never
    /// take the lock: if the last unlocked attempt failed, that failure is returned. Only a stale success falls back to
    /// the locked attempt, so a transaction that keeps failing (e.g. a never-valid spend of an output whose script
    /// reads the block height, during block sync) costs at most the unlocked attempts.
    pub fn insert_unlocked(
        storage: &RwLock<MempoolStorage>,
        tx: Arc<Transaction>,
    ) -> Result<TxStorageResponse, MempoolError> {
        // The failure of the last attempt, if it failed (rather than succeeding at a stale chain state)
        let mut last_failure = None;
        for attempt in 1..MAX_VALIDATION_ATTEMPTS {
            let timer = Instant::now();
            let (validator, generation) = {
                let lock = storage.read().map_err(|_| MempoolError::RwLockPoisonError)?;
                if let Some(response) = lock.pre_check(&tx) {
                    return Ok(response);
                }
                (lock.validator.clone(), lock.chain_generation)
            };

            let tip = validator.chain_metadata().unwrap_or_else(|e| {
                warn!(target: LOG_TARGET, "Could not fetch the chain tip: {e}");
                None
            });
            let dependent_outputs = match validate_attempt(storage, validator.as_ref(), &tx, tip.as_ref())? {
                Ok(dependent_outputs) => dependent_outputs,
                Err(AttemptFailure {
                    response,
                    tip_dependent,
                }) => {
                    // A failure that depends on the tip (e.g. a mempool parent that was mined and removed from the
                    // pool, or a height check) may be due to the chain changing during validation, so it is only
                    // final if the chain did not change. Any other failure is final.
                    if tip_dependent && chain_changed(storage, validator.as_ref(), generation, tip.as_ref())? {
                        debug!(
                            target: LOG_TARGET,
                            "Chain changed while validating transaction {} (attempt {}/{}), which failed with {}, \
                             validating it again",
                            tx_id(&tx),
                            attempt,
                            MAX_VALIDATION_ATTEMPTS,
                            response
                        );
                        last_failure = Some(response);
                        continue;
                    }
                    return Ok(response);
                },
            };
            debug!(
                target: LOG_TARGET,
                "Transaction {} is VALID ({:.2?}), inserting in unconfirmed pool",
                tx_id(&tx),
                timer.elapsed()
            );

            let mut lock = storage.write().map_err(|_| MempoolError::RwLockPoisonError)?;
            if lock.chain_generation != generation {
                debug!(
                    target: LOG_TARGET,
                    "Mempool processed a chain change while validating transaction {} (attempt {}/{}), validating it \
                     again",
                    tx_id(&tx),
                    attempt,
                    MAX_VALIDATION_ATTEMPTS
                );
                last_failure = None;
                continue;
            }
            // The parents may have been removed from the pool since they were looked up
            if let Some(dependent_outputs) = &dependent_outputs &&
                let Err(response) = check_pool_parents(&lock.unconfirmed_pool, dependent_outputs)
            {
                return Ok(response);
            }
            return lock
                .insert_into_unconfirmed_pool(tx, dependent_outputs)
                .map_err(|e| MempoolError::InternalError(e.to_string()));
        }
        // A transaction that failed its last attempt is rejected; failure-driven retries never take the lock
        if let Some(response) = last_failure {
            return Ok(response);
        }
        // The last attempt succeeded, but at a stale chain state. The final attempt validates under the write lock, so
        // that the chain cannot change during it. Rejecting the transaction instead would make a wallet treat a valid
        // transaction as invalid and cancel it. This is acceptable because only a transaction that was valid at each
        // attempt gets here, only real chain events (new blocks, reorgs to a heavier chain, sync completion) change
        // `chain_generation`, so an attacker cannot force this path without mining, and it is bounded by the number of
        // validation permits.
        debug!(
            target: LOG_TARGET,
            "Chain changed during each of {} attempts to validate transaction {}, validating it under the lock",
            MAX_VALIDATION_ATTEMPTS.saturating_sub(1),
            tx_id(&tx)
        );
        let mut lock = storage.write().map_err(|_| MempoolError::RwLockPoisonError)?;
        lock.insert(tx).map_err(|e| MempoolError::InternalError(e.to_string()))
    }

    /// The near-free checks done before a new transaction is validated: its fee and its weight. Returns the response
    /// if the transaction is not to be validated.
    ///
    /// This deliberately does not treat a transaction whose kernels are already in the pool as stored: a different
    /// (and possibly invalid) body can reuse the kernels of a pooled transaction, and the response would cause it to be
    /// propagated. Such a transaction is validated, and deduplicated by the unconfirmed pool once valid.
    pub fn pre_check(&self, tx: &Transaction) -> Option<TxStorageResponse> {
        if let Some(response) = self.check_fee(tx) {
            return Some(response);
        }
        let constants = self.rules.consensus_constants(self.last_seen_height);
        match tx.calculate_weight(constants.transaction_weight_params()) {
            Ok(weight) if weight <= constants.max_block_transaction_weight() => {},
            Ok(_) => {
                return Some(TxStorageResponse::NotStored(Some(
                    ValidationError::MaxTransactionWeightExceeded.to_string(),
                )));
            },
            Err(e) => {
                return Some(TxStorageResponse::NotStored(Some(format!(
                    "Unable to calculate the transaction weight: {e}"
                ))));
            },
        }
        Self::check_body_size(tx).or_else(|| Self::check_kernel_excesses(tx))
    }

    /// Rejects a transaction with two kernels with the same excess (they may differ in their signatures, so they sort
    /// as distinct kernels): it can never be mined, since the chain's kernel excess index is unique. Relay policy, not
    /// a consensus rule.
    fn check_kernel_excesses(tx: &Transaction) -> Option<TxStorageResponse> {
        let mut excesses = std::collections::HashSet::new();
        if tx
            .body
            .kernels()
            .iter()
            .all(|kernel| excesses.insert(tari_utilities::ByteArray::as_bytes(&kernel.excess)))
        {
            return None;
        }
        debug!(
            target: LOG_TARGET,
            "Tx: ({}) repeats a kernel excess, rejecting",
            tx_id(tx)
        );
        Some(TxStorageResponse::NotStored(Some(
            "Transaction contains the same kernel excess more than once".to_string(),
        )))
    }

    /// Rejects a transaction whose body is larger than the block template byte budget: this node could never include
    /// it in a block template, and it would only consume template skips. This is relay policy, not a consensus rule.
    fn check_body_size(tx: &Transaction) -> Option<TxStorageResponse> {
        match tx.body.get_serialized_size() {
            Ok(size) if size <= MAX_BLOCK_TEMPLATE_BODY_BYTES => None,
            Ok(size) => {
                debug!(
                    target: LOG_TARGET,
                    "Tx: ({}) body is {size} bytes, more than the block template budget of {MAX_BLOCK_TEMPLATE_BODY_BYTES} \
                     bytes, rejecting",
                    tx_id(tx)
                );
                Some(TxStorageResponse::NotStored(Some(format!(
                    "Transaction body of {size} bytes exceeds the block template budget of \
                     {MAX_BLOCK_TEMPLATE_BODY_BYTES} bytes"
                ))))
            },
            Err(e) => Some(TxStorageResponse::NotStored(Some(format!(
                "Unable to calculate the transaction body size: {e}"
            )))),
        }
    }

    /// Validates the transaction in the same order as [`MempoolStorage::insert_unlocked`]: chain-linked checks, then
    /// the mempool parents of any inputs not in the chain, then internal consistency.
    fn validate_locked(&self, tx: &Transaction) -> Result<Option<Vec<HashOutput>>, TxStorageResponse> {
        let dependent_outputs = validate_chain_linked(self.validator.as_ref(), tx)?;
        if let Some(dependent_outputs) = &dependent_outputs {
            check_pool_parents(&self.unconfirmed_pool, dependent_outputs)?;
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
        self.chain_generation = self.chain_generation.wrapping_add(1);
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
        self.chain_generation = self.chain_generation.wrapping_add(1);
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
        self.chain_generation = self.chain_generation.wrapping_add(1);
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
        self.chain_generation = self.chain_generation.wrapping_add(1);
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
        // Only a ratio for ordering under byte pressure; the constants at the last seen height are close enough
        let max_block_transaction_weight = self
            .rules
            .consensus_constants(self.last_seen_height)
            .max_block_transaction_weight();
        self.unconfirmed_pool
            .fetch_highest_priority_txs(total_weight, max_block_transaction_weight)
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

/// A failed validation attempt
struct AttemptFailure {
    response: TxStorageResponse,
    /// Whether the failure may be due to the chain tip the transaction was validated against, so that it may be
    /// retried if the chain changed during validation
    tip_dependent: bool,
}

/// One unlocked validation attempt: chain-linked checks, then the mempool parents of any inputs not in the chain
/// (under a brief read lock), then internal consistency against `tip`. On success, returns the hashes of the outputs
/// spent in the mempool, if any.
fn validate_attempt(
    storage: &RwLock<MempoolStorage>,
    validator: &dyn TransactionValidator,
    tx: &Transaction,
    tip: Option<&ChainMetadata>,
) -> Result<Result<Option<Vec<HashOutput>>, AttemptFailure>, MempoolError> {
    let failure = |error: ValidationError| AttemptFailure {
        tip_dependent: is_tip_dependent(&error, tx),
        response: validation_error_to_response(error),
    };
    let dependent_outputs = match validator.validate_chain_linked(tx) {
        Ok(()) => None,
        Err(ValidationError::UnknownInputs(dependent_outputs)) => Some(dependent_outputs),
        Err(e) => return Ok(Err(failure(e))),
    };
    if let Some(dependent_outputs) = &dependent_outputs {
        let lock = storage.read().map_err(|_| MempoolError::RwLockPoisonError)?;
        if let Err(response) = check_pool_parents(&lock.unconfirmed_pool, dependent_outputs) {
            // The parents may have been mined (and so removed from the pool) during validation
            return Ok(Err(AttemptFailure {
                response,
                tip_dependent: true,
            }));
        }
    }
    if let Err(e) = validator.validate_internal_consistency(tx, tip) {
        return Ok(Err(failure(e)));
    }
    Ok(Ok(dependent_outputs))
}

/// Returns true if a validation error may be due to the chain tip the transaction was validated against, i.e. the
/// transaction may be valid at a different tip:
/// - inputs that are not (or no longer) in the chain,
/// - input maturity and kernel lock heights,
/// - a failing input script, but only if one of the transaction's input scripts reads the block height. Any output in
///   the chain can be spent with made-up input data to produce a failing script, so script failures in general are
///   final. The script error does not identify which input failed, so any input with such a script counts; since
///   failure-driven retries never take the lock, this costs at most the unlocked attempts.
///
/// Every other failure is final, since it cannot be fixed by a different tip (signatures, range proofs, balances,
/// sizes, sorting, weight, versions etc.). That includes an input that is already spent and a kernel that is already
/// mined: only a reorg can undo those, which is rare, and the mempool revalidates its transactions after a reorg
/// anyway.
fn is_tip_dependent(error: &ValidationError, tx: &Transaction) -> bool {
    match error {
        ValidationError::UnknownInputs(_) | ValidationError::UnknownInput | ValidationError::MaturityError => true,
        ValidationError::TransactionError(e) |
        ValidationError::AggregatedBodyValidationError(AggregatedBodyValidationError::TransactionError(e)) => {
            is_tip_dependent_transaction_error(e, tx)
        },
        ValidationError::AggregatedBodyValidationError(AggregatedBodyValidationError::MaturityError) => true,
        _ => false,
    }
}

fn is_tip_dependent_transaction_error(error: &TransactionError, tx: &Transaction) -> bool {
    match error {
        TransactionError::InputMaturity => true,
        TransactionError::ScriptError(_) | TransactionError::ScriptExecutionError(_) => {
            has_height_dependent_input_script(tx)
        },
        _ => false,
    }
}

/// Returns true if any input script of the transaction uses an opcode that reads the block height from the script
/// context
fn has_height_dependent_input_script(tx: &Transaction) -> bool {
    tx.body.inputs().iter().any(|input| {
        input.script().is_ok_and(|script| {
            script.iter().any(|opcode| {
                matches!(
                    opcode,
                    Opcode::CheckHeight(_) |
                        Opcode::CheckHeightVerify(_) |
                        Opcode::CompareHeight |
                        Opcode::CompareHeightVerify
                )
            })
        })
    })
}

/// Returns true if the chain may have changed since an attempt started with the mempool at `generation` and the
/// chain at `tip`: either the mempool has processed a chain change, or the chain tip has moved (e.g. a block was
/// committed that the mempool has not processed yet). Neither check holds the mempool write lock. If the tip was not
/// known at the start, only the mempool's generation is compared; if it cannot be fetched now, the chain is assumed to
/// have changed (the number of attempts is bounded, so this cannot loop).
fn chain_changed(
    storage: &RwLock<MempoolStorage>,
    validator: &dyn TransactionValidator,
    generation: u64,
    tip: Option<&ChainMetadata>,
) -> Result<bool, MempoolError> {
    if storage
        .read()
        .map_err(|_| MempoolError::RwLockPoisonError)?
        .chain_generation !=
        generation
    {
        return Ok(true);
    }
    let Some(tip) = tip else {
        return Ok(false);
    };
    Ok(match validator.chain_metadata() {
        Ok(Some(current)) => current.best_block_hash() != tip.best_block_hash(),
        _ => true,
    })
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

/// Checks that every output in `dependent_outputs` is in the unconfirmed pool. The unconfirmed pool indexes outputs by
/// their hash, and an input's output hash commits to all of the spent output's data (commitment, script, features
/// etc.), so an input that does not spend a pool output exactly as it was created is not found and is an orphan.
fn check_pool_parents(pool: &UnconfirmedPool, dependent_outputs: &[HashOutput]) -> Result<(), TxStorageResponse> {
    if pool.contains_all_outputs(dependent_outputs) {
        Ok(())
    } else {
        Err(TxStorageResponse::NotStoredOrphan)
    }
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
            Weak,
            atomic::{AtomicUsize, Ordering},
        },
    };

    use tari_script::{ScriptError, TariScript, script};
    use tari_transaction_components::{
        MicroMinotari,
        key_manager::KeyManager,
        transaction_components::{SpentOutput, WalletOutput},
        tx,
    };

    use super::*;
    use crate::test_helpers::create_consensus_rules;

    /// A validator that returns the scripted results and chain tips in order, and records how each stage of validation
    /// is performed. It can also simulate the mempool processing a chain change during internal validation.
    #[derive(Default)]
    struct ScriptedValidator {
        storage: Mutex<Weak<RwLock<MempoolStorage>>>,
        chain_linked_results: Mutex<VecDeque<Result<(), ValidationError>>>,
        internal_results: Mutex<VecDeque<Result<(), ValidationError>>>,
        tips: Mutex<VecDeque<FixedHash>>,
        internal_tips: Mutex<Vec<Option<FixedHash>>>,
        // Whether the storage write lock was free during each internal validation
        internal_lock_free: Mutex<Vec<bool>>,
        // The number of internal validations during which a chain change is processed
        chain_changes: AtomicUsize,
        // If set, the next chain-linked validation simulates the mempool processing a block that mines every pooled
        // transaction
        mine_pool_during_chain_linked: std::sync::atomic::AtomicBool,
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

        fn internal_lock_free(&self) -> Vec<bool> {
            self.internal_lock_free.lock().unwrap().clone()
        }
    }

    impl TransactionValidator for Arc<ScriptedValidator> {
        fn validate_full(&self, _tx: &Transaction) -> Result<(), ValidationError> {
            unreachable!("the mempool validates in stages")
        }

        fn validate_chain_linked(&self, _tx: &Transaction) -> Result<(), ValidationError> {
            self.chain_linked_calls.fetch_add(1, Ordering::SeqCst);
            if self.mine_pool_during_chain_linked.swap(false, Ordering::SeqCst) &&
                let Some(storage) = self.storage.lock().unwrap().upgrade()
            {
                let mut lock = storage.write().unwrap();
                let _mined = lock.unconfirmed_pool.drain_all_mempool_transactions();
                lock.chain_generation = lock.chain_generation.wrapping_add(1);
            }
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
            if let Some(storage) = self.storage.lock().unwrap().upgrade() {
                let lock = storage.try_write();
                self.internal_lock_free.lock().unwrap().push(lock.is_ok());
                if let Ok(mut lock) = lock &&
                    self.chain_changes
                        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
                        .is_ok()
                {
                    lock.chain_generation = lock.chain_generation.wrapping_add(1);
                }
            }
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

    fn create_storage(validator: &Arc<ScriptedValidator>) -> Arc<RwLock<MempoolStorage>> {
        let mut config = MempoolConfig::default();
        config.unconfirmed_pool.min_fee = 0;
        let storage = Arc::new(RwLock::new(MempoolStorage::new(
            config,
            create_consensus_rules(),
            Box::new(validator.clone()),
        )));
        *validator.storage.lock().unwrap() = Arc::downgrade(&storage);
        storage
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
    fn insert_unlocked_validates_once_without_the_lock_if_chain_unchanged() {
        let key_manager = KeyManager::new_random().unwrap();
        let tip = FixedHash::from([1u8; 32]);
        let validator = ScriptedValidator::new(vec![Ok(())], vec![tip]);
        let storage = create_storage(&validator);

        let response = MempoolStorage::insert_unlocked(&storage, create_tx(&key_manager)).unwrap();
        assert_eq!(response, TxStorageResponse::UnconfirmedPool);
        assert_eq!(validator.chain_linked_calls(), 1);
        assert_eq!(validator.internal_calls(), 1);
        // Internal validation used the tip snapshot, and did not hold the write lock
        assert_eq!(validator.internal_tips(), vec![Some(tip)]);
        assert_eq!(validator.internal_lock_free(), vec![true]);
        assert_eq!(pool_len(&storage), 1);
    }

    #[test]
    fn insert_unlocked_revalidates_without_the_lock_if_chain_changed() {
        let key_manager = KeyManager::new_random().unwrap();
        let validated_at = FixedHash::from([1u8; 32]);
        let new_tip = FixedHash::from([2u8; 32]);

        // A chain change is processed during the first validation, so the whole validation is repeated at the new tip
        let validator = ScriptedValidator::new(vec![], vec![validated_at, new_tip]);
        validator.chain_changes.store(1, Ordering::SeqCst);
        let storage = create_storage(&validator);
        let response = MempoolStorage::insert_unlocked(&storage, create_tx(&key_manager)).unwrap();
        assert_eq!(response, TxStorageResponse::UnconfirmedPool);
        assert_eq!(validator.chain_linked_calls(), 2);
        assert_eq!(validator.internal_calls(), 2);
        assert_eq!(validator.internal_tips(), vec![Some(validated_at), Some(new_tip)]);
        assert_eq!(validator.internal_lock_free(), vec![true, true]);
        assert_eq!(pool_len(&storage), 1);

        // Internally invalid at the new tip (e.g. a script checking the block height)
        let validator = ScriptedValidator::new(vec![], vec![validated_at, new_tip]);
        validator.chain_changes.store(1, Ordering::SeqCst);
        *validator.internal_results.lock().unwrap() =
            vec![Ok(()), Err(ValidationError::InvalidAccountingBalance)].into();
        let storage = create_storage(&validator);
        let response = MempoolStorage::insert_unlocked(&storage, create_tx(&key_manager)).unwrap();
        assert!(matches!(response, TxStorageResponse::NotStored(_)), "{response:?}");
        assert_eq!(validator.internal_calls(), 2);
        assert_eq!(pool_len(&storage), 0);

        // An input spent by the new tip is caught by the chain-linked checks
        let validator = ScriptedValidator::new(vec![Ok(()), Err(ValidationError::ContainsSTxO)], vec![
            validated_at,
            new_tip,
        ]);
        validator.chain_changes.store(1, Ordering::SeqCst);
        let storage = create_storage(&validator);
        let response = MempoolStorage::insert_unlocked(&storage, create_tx(&key_manager)).unwrap();
        assert_eq!(response, TxStorageResponse::NotStoredAlreadySpent);
        assert_eq!(validator.chain_linked_calls(), 2);
        assert_eq!(validator.internal_calls(), 1);
        assert_eq!(pool_len(&storage), 0);
    }

    #[test]
    fn insert_unlocked_validates_under_the_lock_after_max_attempts() {
        let key_manager = KeyManager::new_random().unwrap();
        let validator = ScriptedValidator::new(vec![], vec![FixedHash::zero()]);
        // The chain changes during every unlocked attempt
        validator.chain_changes.store(usize::MAX, Ordering::SeqCst);
        let storage = create_storage(&validator);
        let response = MempoolStorage::insert_unlocked(&storage, create_tx(&key_manager)).unwrap();
        // The valid transaction is still stored
        assert_eq!(response, TxStorageResponse::UnconfirmedPool);
        assert_eq!(validator.chain_linked_calls(), MAX_VALIDATION_ATTEMPTS);
        assert_eq!(validator.internal_calls(), MAX_VALIDATION_ATTEMPTS);
        // Only the final attempt validated under the lock
        let mut expected = vec![true; MAX_VALIDATION_ATTEMPTS - 1];
        expected.push(false);
        assert_eq!(validator.internal_lock_free(), expected);
        assert_eq!(pool_len(&storage), 1);
    }

    #[test]
    fn transaction_reusing_pooled_kernels_is_validated() {
        let key_manager = KeyManager::new_random().unwrap();
        let validator = ScriptedValidator::new(vec![], vec![FixedHash::zero()]);
        let storage = create_storage(&validator);
        let tx = create_tx(&key_manager);
        assert_eq!(
            MempoolStorage::insert_unlocked(&storage, tx.clone()).unwrap(),
            TxStorageResponse::UnconfirmedPool
        );

        // A different, invalid body with the same kernels
        let other = create_tx(&key_manager);
        let reusing = Arc::new(Transaction::new(
            other.body.inputs().clone(),
            other.body.outputs().clone(),
            tx.body.kernels().clone(),
            other.offset.clone(),
            other.script_offset.clone(),
        ));
        assert_eq!(storage.read().unwrap().pre_check(&reusing), None);
        for locked in [false, true] {
            *validator.internal_results.lock().unwrap() = vec![Err(ValidationError::InvalidAccountingBalance)].into();
            let internal_calls = validator.internal_calls();
            let response = if locked {
                storage.write().unwrap().insert(reusing.clone()).unwrap()
            } else {
                MempoolStorage::insert_unlocked(&storage, reusing.clone()).unwrap()
            };
            assert_ne!(response, TxStorageResponse::UnconfirmedPool);
            assert!(matches!(response, TxStorageResponse::NotStored(_)), "{response:?}");
            assert_eq!(validator.internal_calls(), internal_calls + 1);
        }
        assert_eq!(pool_len(&storage), 1);
    }

    #[test]
    fn child_is_stored_if_its_pool_parent_is_mined_during_validation() {
        let key_manager = KeyManager::new_random().unwrap();
        let validator = ScriptedValidator::new(vec![], vec![FixedHash::zero()]);
        let storage = create_storage(&validator);
        let (parent, parent_outputs) = create_tx_with_outputs(&key_manager);
        assert_eq!(
            MempoolStorage::insert_unlocked(&storage, parent).unwrap(),
            TxStorageResponse::UnconfirmedPool
        );
        let child = spending(&create_tx(&key_manager), &parent_outputs[0], &key_manager);
        let parent_output = child.body.inputs()[0].output_hash();

        // The parent's output is not in the chain yet, but the parent is mined (and removed from the pool) before the
        // pool is checked for it. The retry finds the output in the chain.
        *validator.chain_linked_results.lock().unwrap() =
            vec![Err(ValidationError::UnknownInputs(vec![parent_output])), Ok(())].into();
        validator.mine_pool_during_chain_linked.store(true, Ordering::SeqCst);
        let response = MempoolStorage::insert_unlocked(&storage, child).unwrap();
        assert_eq!(response, TxStorageResponse::UnconfirmedPool);
        assert_eq!(validator.chain_linked_calls(), 3);
        assert_eq!(validator.internal_calls(), 2);
        assert_eq!(pool_len(&storage), 1);
    }

    #[test]
    fn failure_is_retried_if_mempool_processed_a_chain_change() {
        let key_manager = KeyManager::new_random().unwrap();
        let validated_at = FixedHash::from([1u8; 32]);
        let new_tip = FixedHash::from([2u8; 32]);
        // Not yet mature at the snapshot tip, but mature at the new tip
        let validator = ScriptedValidator::new(vec![], vec![validated_at, new_tip]);
        validator.chain_changes.store(1, Ordering::SeqCst);
        *validator.internal_results.lock().unwrap() = vec![Err(ValidationError::MaturityError), Ok(())].into();
        let storage = create_storage(&validator);
        let response = MempoolStorage::insert_unlocked(&storage, create_tx(&key_manager)).unwrap();
        assert_eq!(response, TxStorageResponse::UnconfirmedPool);
        assert_eq!(validator.chain_linked_calls(), 2);
        assert_eq!(validator.internal_calls(), 2);
        assert_eq!(validator.internal_tips(), vec![Some(validated_at), Some(new_tip)]);
        assert_eq!(validator.internal_lock_free(), vec![true, true]);
        assert_eq!(pool_len(&storage), 1);
    }

    #[test]
    fn failure_is_retried_if_chain_tip_moved() {
        let key_manager = KeyManager::new_random().unwrap();
        let validated_at = FixedHash::from([1u8; 32]);
        let new_tip = FixedHash::from([2u8; 32]);
        // A block is committed during validation, but the mempool has not processed it yet
        let validator = ScriptedValidator::new(vec![], vec![validated_at, new_tip]);
        *validator.internal_results.lock().unwrap() = vec![Err(ValidationError::MaturityError), Ok(())].into();
        let storage = create_storage(&validator);
        let response = MempoolStorage::insert_unlocked(&storage, create_tx(&key_manager)).unwrap();
        assert_eq!(response, TxStorageResponse::UnconfirmedPool);
        assert_eq!(validator.chain_linked_calls(), 2);
        assert_eq!(validator.internal_calls(), 2);
        assert_eq!(validator.internal_tips(), vec![Some(validated_at), Some(new_tip)]);
        assert_eq!(storage.read().unwrap().chain_generation, 0);
        assert_eq!(pool_len(&storage), 1);
    }

    /// A copy of `tx` whose first input's script is replaced by `script`
    fn with_input_script(tx: &Transaction, script: TariScript) -> Arc<Transaction> {
        let mut inputs = tx.body.inputs().clone();
        match &mut inputs[0].spent_output {
            SpentOutput::OutputData { script: s, .. } => *s = script,
            SpentOutput::OutputHash(_) => panic!("Expected a full input"),
        }
        Arc::new(Transaction::new(
            inputs,
            tx.body.outputs().clone(),
            tx.body.kernels().clone(),
            tx.offset.clone(),
            tx.script_offset.clone(),
        ))
    }

    fn script_error(error: ScriptError) -> ValidationError {
        AggregatedBodyValidationError::TransactionError(TransactionError::ScriptError(error)).into()
    }

    #[test]
    fn tip_independent_failure_is_final_even_if_chain_changed() {
        let key_manager = KeyManager::new_random().unwrap();
        let validated_at = FixedHash::from([1u8; 32]);
        let new_tip = FixedHash::from([2u8; 32]);
        let tx = create_tx(&key_manager);
        for error in [
            // A bad script offset
            ValidationError::from(AggregatedBodyValidationError::TransactionError(
                TransactionError::ScriptOffset,
            )),
            // A bad signature
            ValidationError::from(AggregatedBodyValidationError::TransactionError(
                TransactionError::InvalidSignatureError("bad".to_string()),
            )),
            // A failing script that does not read the block height, e.g. made-up input data
            script_error(ScriptError::VerifyFailed),
        ] {
            // The mempool processes a chain change, and the tip moves, during validation
            let validator = ScriptedValidator::new(vec![], vec![validated_at, new_tip]);
            validator.chain_changes.store(usize::MAX, Ordering::SeqCst);
            *validator.internal_results.lock().unwrap() = vec![Err(error)].into();
            let storage = create_storage(&validator);
            let response = MempoolStorage::insert_unlocked(&storage, tx.clone()).unwrap();
            assert!(matches!(response, TxStorageResponse::NotStored(_)), "{response:?}");
            assert_eq!(validator.chain_linked_calls(), 1);
            assert_eq!(validator.internal_calls(), 1);
            assert_eq!(validator.internal_lock_free(), vec![true]);
            assert_eq!(pool_len(&storage), 0);
        }
    }

    #[test]
    fn height_dependent_script_failure_is_retried_if_chain_changed() {
        let key_manager = KeyManager::new_random().unwrap();
        let validated_at = FixedHash::from([1u8; 32]);
        let new_tip = FixedHash::from([2u8; 32]);
        let tx = with_input_script(&create_tx(&key_manager), script!(CheckHeightVerify(100) Nop).unwrap());

        let validator = ScriptedValidator::new(vec![], vec![validated_at, new_tip]);
        validator.chain_changes.store(1, Ordering::SeqCst);
        *validator.internal_results.lock().unwrap() = vec![Err(script_error(ScriptError::VerifyFailed)), Ok(())].into();
        let storage = create_storage(&validator);
        let response = MempoolStorage::insert_unlocked(&storage, tx.clone()).unwrap();
        assert_eq!(response, TxStorageResponse::UnconfirmedPool);
        assert_eq!(validator.chain_linked_calls(), 2);
        assert_eq!(validator.internal_calls(), 2);
        assert_eq!(validator.internal_lock_free(), vec![true, true]);

        // Without a chain change, the same failure is final
        let validator = ScriptedValidator::new(vec![], vec![validated_at]);
        *validator.internal_results.lock().unwrap() = vec![Err(script_error(ScriptError::VerifyFailed))].into();
        let storage = create_storage(&validator);
        let response = MempoolStorage::insert_unlocked(&storage, tx).unwrap();
        assert!(matches!(response, TxStorageResponse::NotStored(_)), "{response:?}");
        assert_eq!(validator.internal_calls(), 1);
    }

    #[test]
    fn failure_driven_retries_never_take_the_lock() {
        let key_manager = KeyManager::new_random().unwrap();
        let validated_at = FixedHash::from([1u8; 32]);
        let new_tip = FixedHash::from([2u8; 32]);
        // A never-valid spend of an output whose script reads the block height, and a transaction that is never mature
        let height_script_tx =
            with_input_script(&create_tx(&key_manager), script!(CheckHeightVerify(100) Nop).unwrap());
        type Case = (Arc<Transaction>, fn() -> ValidationError);
        let cases: [Case; 2] = [
            (height_script_tx, || script_error(ScriptError::VerifyFailed)),
            (create_tx(&key_manager), || ValidationError::MaturityError),
        ];
        for (tx, error) in cases {
            // The mempool processes a chain change, and the tip moves, during every attempt
            let validator = ScriptedValidator::new(vec![], vec![validated_at, new_tip]);
            validator.chain_changes.store(usize::MAX, Ordering::SeqCst);
            *validator.internal_results.lock().unwrap() = (0..MAX_VALIDATION_ATTEMPTS).map(|_| Err(error())).collect();
            let storage = create_storage(&validator);
            let response = MempoolStorage::insert_unlocked(&storage, tx).unwrap();
            assert!(
                matches!(
                    response,
                    TxStorageResponse::NotStored(_) | TxStorageResponse::NotStoredTimeLocked
                ),
                "{response:?}"
            );
            assert_eq!(validator.chain_linked_calls(), MAX_VALIDATION_ATTEMPTS - 1);
            assert_eq!(validator.internal_calls(), MAX_VALIDATION_ATTEMPTS - 1);
            assert_eq!(validator.internal_lock_free(), vec![true; MAX_VALIDATION_ATTEMPTS - 1]);
            assert_eq!(pool_len(&storage), 0);
        }

        // Likewise a transaction whose mempool parent is never found while the chain tip keeps moving
        let tips = (0u8..10).map(|i| FixedHash::from([i; 32])).collect();
        let validator = ScriptedValidator::new(
            (0..MAX_VALIDATION_ATTEMPTS)
                .map(|_| Err(ValidationError::UnknownInputs(vec![FixedHash::from([99u8; 32])])))
                .collect(),
            tips,
        );
        let storage = create_storage(&validator);
        let response = MempoolStorage::insert_unlocked(&storage, create_tx(&key_manager)).unwrap();
        assert_eq!(response, TxStorageResponse::NotStoredOrphan);
        assert_eq!(validator.chain_linked_calls(), MAX_VALIDATION_ATTEMPTS - 1);
        assert_eq!(validator.internal_calls(), 0);
        assert_eq!(pool_len(&storage), 0);
    }

    #[test]
    fn failure_without_chain_change_is_final() {
        let key_manager = KeyManager::new_random().unwrap();
        let validator = ScriptedValidator::new(vec![], vec![FixedHash::from([1u8; 32])]);
        *validator.internal_results.lock().unwrap() = vec![Err(ValidationError::InvalidAccountingBalance)].into();
        let storage = create_storage(&validator);
        let response = MempoolStorage::insert_unlocked(&storage, create_tx(&key_manager)).unwrap();
        assert!(matches!(response, TxStorageResponse::NotStored(_)), "{response:?}");
        assert_eq!(validator.chain_linked_calls(), 1);
        assert_eq!(validator.internal_calls(), 1);
        assert_eq!(pool_len(&storage), 0);

        let validator = ScriptedValidator::new(vec![Err(ValidationError::ContainsSTxO)], vec![FixedHash::from(
            [1u8; 32],
        )]);
        let storage = create_storage(&validator);
        let response = MempoolStorage::insert_unlocked(&storage, create_tx(&key_manager)).unwrap();
        assert_eq!(response, TxStorageResponse::NotStoredAlreadySpent);
        assert_eq!(validator.chain_linked_calls(), 1);
        assert_eq!(validator.internal_calls(), 0);
    }

    #[test]
    fn pre_check_rejects_before_validation() {
        let key_manager = KeyManager::new_random().unwrap();
        let validator = ScriptedValidator::new(vec![], vec![FixedHash::zero()]);
        let storage = create_storage(&validator);
        assert_eq!(
            MempoolStorage::insert_unlocked(&storage, create_tx(&key_manager)).unwrap(),
            TxStorageResponse::UnconfirmedPool
        );
        assert_eq!(validator.chain_linked_calls(), 1);

        // Fee too low
        storage.write().unwrap().unconfirmed_pool.config.min_fee = u64::MAX;
        assert_eq!(
            MempoolStorage::insert_unlocked(&storage, create_tx(&key_manager)).unwrap(),
            TxStorageResponse::NotStoredFeeTooLow
        );
        assert_eq!(validator.chain_linked_calls(), 1);
        assert_eq!(validator.internal_calls(), 1);
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
        // The child really spends the parent's output
        let child = spending(&create_tx(&key_manager), &parent_outputs[0], &key_manager);
        let parent_output = child.body.inputs()[0].output_hash();
        assert!(
            storage
                .read()
                .unwrap()
                .unconfirmed_pool
                .contains_all_outputs(&[parent_output])
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
