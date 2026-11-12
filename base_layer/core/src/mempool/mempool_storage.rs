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
    collections::{HashMap, HashSet, VecDeque},
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
    BanPeriod,
    BanReason,
    helpers::borsh::SerializedSize,
    rpc::models::FeePerGramStat,
    transaction_components::{Transaction, TransactionError},
    validation::{
        AggregatedBodyValidationError,
        aggregate_body::{run_height_reading_scripts, validate_covenants},
    },
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
            HeightValidation,
            MAX_BLOCK_TEMPLATE_BODY_BYTES,
            RetrieveResults,
            TemplateFilter,
            TransactionKey,
            UnconfirmedPool,
            UnconfirmedPoolError,
            reads_block_height,
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
    /// validating them does not block the mempool. A transaction with an input script or covenant that reads the
    /// height is validated against `tip` (see [MempoolStorage::chain_tip], fetched once per batch by the caller), so
    /// that the block it was validated for is known; any other is validated against the validator's own tip.
    pub fn insert(
        &mut self,
        tx: Arc<Transaction>,
        tip: Option<&ChainMetadata>,
    ) -> Result<TxStorageResponse, UnconfirmedPoolError> {
        let timer = Instant::now();
        if let Some(response) = self
            .check_fee(&tx)
            .or_else(|| Self::check_body_size(&tx))
            .or_else(|| Self::check_kernel_excesses(&tx))
        {
            return Ok(response);
        }
        let tip = if reads_block_height(&tx.body) { tip } else { None };
        let parents = match self.validate_locked(&tx, tip) {
            Ok(parents) => parents,
            Err(response) => return Ok(response),
        };
        let validation = match height_validation(&tx, tip) {
            Ok(validation) => validation,
            Err(response) => return Ok(response),
        };
        debug!(
            target: LOG_TARGET,
            "Transaction {} is VALID ({:.2?}), inserting in unconfirmed pool",
            tx_id(&tx),
            timer.elapsed()
        );
        self.insert_into_unconfirmed_pool(tx, parents.map(|p| p.outputs), validation)
    }

    /// The current chain tip, to validate a batch of transactions against with [MempoolStorage::insert], or `None` if
    /// it cannot be fetched
    fn chain_tip(&self) -> Option<ChainMetadata> {
        self.validator.chain_metadata().unwrap_or_else(|e| {
            warn!(target: LOG_TARGET, "Could not fetch the chain tip: {e}");
            None
        })
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
    /// since the mempool reconciles its stored transactions with it when it does process it: a block removes the
    /// transactions it mined or double spent and re-checks the validator node transactions, and a reorg or sync
    /// revalidates every transaction. A transaction with an input script or covenant that reads the height is
    /// re-checked when it is selected for a block template, at the template's height, and by a bounded background
    /// re-check after each block (see `Mempool::retrieve` and `Mempool::sweep_height_reading`); one that fails is
    /// dropped with its descendants. Block templates also leave out transactions that are not mature, or whose kernels
    /// are locked, at the template's height.
    ///
    /// Most failures are final. Only a failure that depends on the chain tip (inputs not in the chain or pool, input
    /// maturity and lock heights, and scripts and covenants that read the height of the next block; see
    /// `is_tip_dependent`) may be due to the chain changing during validation. Such a failure is validated again, in
    /// the same way, if the mempool processed a chain change or the chain tip moved since the attempt started. The
    /// near-free fee and weight checks do not depend on the chain and are not repeated. Failure-driven retries are
    /// capped at the unlocked attempts and never take the lock: if the last unlocked attempt failed, that failure
    /// is returned. Only a stale success falls back to the locked attempt, so a transaction that keeps failing
    /// (e.g. a never-valid spend of an output whose script reads the block height, during block sync) costs at most
    /// the unlocked attempts.
    pub fn insert_unlocked(
        storage: &RwLock<MempoolStorage>,
        tx: Arc<Transaction>,
    ) -> Result<TxStorageResponse, MempoolError> {
        Self::insert_unlocked_with_ban_reason(storage, tx).map(|(response, _)| response)
    }

    /// As [`MempoolStorage::insert_unlocked`], also returning a ban reason if the transaction failed a final internal
    /// consistency check (see `internal_failure_ban_reason`).
    pub fn insert_unlocked_with_ban_reason(
        storage: &RwLock<MempoolStorage>,
        tx: Arc<Transaction>,
    ) -> Result<(TxStorageResponse, Option<BanReason>), MempoolError> {
        // The failure of the last attempt, if it failed (rather than succeeding at a stale chain state)
        let mut last_failure = None;
        for attempt in 1..MAX_VALIDATION_ATTEMPTS {
            let timer = Instant::now();
            let (validator, generation) = {
                let lock = storage.read().map_err(|_| MempoolError::RwLockPoisonError)?;
                if let Some(response) = lock.pre_check(&tx) {
                    return Ok((response, None));
                }
                (lock.validator.clone(), lock.chain_generation)
            };

            let tip = validator.chain_metadata().unwrap_or_else(|e| {
                warn!(target: LOG_TARGET, "Could not fetch the chain tip: {e}");
                None
            });
            let ValidatedTransaction { parents, validation } =
                match validate_attempt(storage, validator.as_ref(), &tx, tip.as_ref())? {
                    Ok(validated) => validated,
                    Err(AttemptFailure {
                        response,
                        tip_dependent,
                        covenant_height_dependent,
                        ban_reason,
                    }) => {
                        // A failure that depends on the tip (e.g. a mempool parent that was mined and removed from the
                        // pool, or a height check) may be due to the chain changing during validation, so it is only
                        // final if the chain did not change. Any other failure is final.
                        if tip_dependent &&
                            chain_changed(storage, validator.as_ref(), generation, tip.as_ref())? &&
                            (!covenant_height_dependent || covenants_pass_at_current_tip(validator.as_ref(), &tx))
                        {
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
                        return Ok((response, ban_reason));
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
            if let Some(parents) = &parents &&
                let Err(response) = check_pool_parents(&lock.unconfirmed_pool, parents)
            {
                return Ok((response, None));
            }
            return lock
                .insert_into_unconfirmed_pool(tx, parents.map(|p| p.outputs), validation)
                .map(|response| (response, None))
                .map_err(|e| MempoolError::InternalError(e.to_string()));
        }
        // A transaction that failed its last attempt is rejected; failure-driven retries never take the lock
        if let Some(response) = last_failure {
            return Ok((response, None));
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
        let tip = lock.chain_tip();
        lock.insert(tx, tip.as_ref())
            .map(|response| (response, None))
            .map_err(|e| MempoolError::InternalError(e.to_string()))
    }

    /// The near-free checks done before a new transaction is validated: its fee and its weight. Returns the response
    /// if the transaction is not to be validated.
    ///
    /// This does no kernel deduplication. Callers drop an exact kernel-set match or a union of pool transactions before
    /// validation with [MempoolStorage::has_transaction], and `insert_into_unconfirmed_pool` does not store one; any
    /// other reuse of pooled kernels is validated.
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
        let mut excesses = HashSet::new();
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
    fn validate_locked(
        &self,
        tx: &Transaction,
        tip: Option<&ChainMetadata>,
    ) -> Result<Option<PoolParents>, TxStorageResponse> {
        let parents = validate_chain_linked(self.validator.as_ref(), tx)?;
        if let Some(parents) = &parents {
            check_pool_parents(&self.unconfirmed_pool, parents)?;
        }
        validate_internal_consistency(self.validator.as_ref(), tx, tip)?;
        Ok(parents)
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

    /// Inserts a validated transaction, with the block it was validated for if it reads the height (see
    /// `height_validation`)
    fn insert_into_unconfirmed_pool(
        &mut self,
        tx: Arc<Transaction>,
        dependent_outputs: Option<Vec<HashOutput>>,
        validation: HeightValidation,
    ) -> Result<TxStorageResponse, UnconfirmedPoolError> {
        let timer = Instant::now();
        let tx_id = tx_id(&tx);
        if self.unconfirmed_pool.has_transaction(&tx) {
            debug!(target: LOG_TARGET, "Tx: ({tx_id}) is already in the unconfirmed pool");
            return Ok(TxStorageResponse::UnconfirmedPool);
        }
        let weight = self.get_transaction_weighting();
        // At most one pool transaction may register or exit a given validator node (two would make every template
        // invalid). A transaction for a slot that is already held replaces the holder only if it pays a strictly higher
        // fee per gram, so that a third party copying a gossiped registration/exit into a low-fee transaction cannot
        // squat the slot. The replaced holder is dropped (not moved to the reorg pool); its owner can resubmit with a
        // higher fee.
        let holders = self.unconfirmed_pool.validator_node_conflict_holders(&tx);
        if !holders.is_empty() {
            let outbids = fee_per_gram_millis(&tx, &weight).is_some_and(|rate| {
                holders.iter().all(|key| {
                    self.unconfirmed_pool
                        .fee_per_gram_millis(*key)
                        .is_some_and(|held| rate > held)
                })
            });
            if !outbids {
                debug!(
                    target: LOG_TARGET,
                    "Tx: ({tx_id}) registers or exits a validator node that a pool transaction with an equal or higher \
                     fee already registers or exits, rejecting"
                );
                return Ok(TxStorageResponse::NotStoredValidatorNodeSlotTaken);
            }
            for key in holders {
                if let Some(replaced) = self.unconfirmed_pool.remove_transaction(key)? {
                    debug!(
                        target: LOG_TARGET,
                        "Tx: ({tx_id}) replaces pool transaction {} in its validator node slot with a higher fee",
                        replaced
                            .first_kernel_excess_sig()
                            .map(|sig| sig.get_signature().to_hex())
                            .unwrap_or_default()
                    );
                }
            }
        }
        self.unconfirmed_pool
            .insert_with_height_validation(tx, dependent_outputs, &weight, validation)?;
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
        self.unconfirmed_pool
            .remove_transactions(transactions.iter().map(|(tx_key, _)| *tx_key))
            .map_err(|e| MempoolError::InternalError(e.to_string()))?;
        self.insert_txs(transactions.iter().map(|(_, tx)| tx.clone()).collect())
            .map_err(|e| MempoolError::InternalError(e.to_string()))?;

        Ok(())
    }

    // Insert a set of transactions into the UTxPool, retrying deferred orphans as their parent outputs are stored.
    fn insert_txs(&mut self, txs: Vec<Arc<Transaction>>) -> Result<i64, UnconfirmedPoolError> {
        // Fetching the tip reads the database, which is not worth doing under the write lock for nothing
        if txs.is_empty() {
            return Ok(0);
        }
        let tip = self.chain_tip();
        let mut num_invalid_txs: i64 = 0;
        let mut queue: VecDeque<Arc<Transaction>> = txs.into();
        let mut orphans: Vec<(Option<Arc<Transaction>>, usize)> = Vec::new();
        let mut waiting_by_output: HashMap<HashOutput, Vec<usize>> = HashMap::new();

        while let Some(tx) = queue.pop_front() {
            let resp = self.insert(tx.clone(), tip.as_ref())?;
            match resp {
                TxStorageResponse::UnconfirmedPool => {
                    for output in tx.body.outputs() {
                        let output_hash = output.hash();
                        if self.unconfirmed_pool.contains_all_outputs(&[output_hash]) &&
                            let Some(waiting_indices) = waiting_by_output.remove(&output_hash)
                        {
                            for idx in waiting_indices {
                                if let Some((orphan_tx, missing)) = orphans.get_mut(idx) {
                                    *missing = missing.saturating_sub(1);
                                    if *missing == 0 &&
                                        let Some(orphan_tx) = orphan_tx.take()
                                    {
                                        queue.push_back(orphan_tx);
                                    }
                                }
                            }
                        }
                    }
                },
                TxStorageResponse::NotStoredOrphan => {
                    let idx = orphans.len();
                    let missing_outputs: HashSet<HashOutput> = tx
                        .body
                        .inputs()
                        .iter()
                        .map(|input| input.output_hash())
                        .filter(|output_hash| !self.unconfirmed_pool.contains_all_outputs(&[*output_hash]))
                        .collect();
                    for output_hash in &missing_outputs {
                        waiting_by_output.entry(*output_hash).or_default().push(idx);
                    }
                    orphans.push((Some(tx), missing_outputs.len()));
                },
                TxStorageResponse::NotStoredAlreadySpent => {
                    num_invalid_txs = num_invalid_txs.saturating_add(1);
                },
                _ => {},
            }
        }

        let remaining_orphans = orphans.iter().filter(|(tx, _)| tx.is_some()).count();
        if remaining_orphans > 0 {
            warn!(
                target: LOG_TARGET,
                "Mempool re-insertion finished with {remaining_orphans} unresolved orphan transaction(s)"
            );
        }

        Ok(num_invalid_txs)
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
        // Validator node registrations and exits whose max_epoch is before the epoch of the next block can never be
        // mined; they are dropped (not moved to the reorg pool).
        let next_height = published_block.header.height.saturating_add(1);
        let next_epoch = self
            .rules
            .consensus_constants(next_height)
            .block_height_to_epoch(next_height);
        let expired = self
            .unconfirmed_pool
            .remove_expired_validator_node_transactions(next_epoch)?;
        if !expired.is_empty() {
            debug!(
                target: LOG_TARGET,
                "Removed {} validator node transaction(s) that expired before epoch {}",
                expired.len(),
                next_epoch
            );
        }
        // The validity of the remaining validator node transactions (registrations, exits and spends of a pending
        // registration) depends on the validator node set and the epoch: a cancelling spend becomes invalid once its
        // registration activates, an exit once the node has exited, a registration once the node is registered by
        // another transaction. Templates are built from the pool without validating them again in full (only maturity,
        // lock heights, and input scripts and covenants that read the height are re-checked, at the template's height),
        // so when this block can have changed validator node state (an epoch boundary, or validator node transactions
        // in the block), re-check the chain-linked rules for each of them in place and drop the ones that fail. Only
        // the cheap chain-linked checks are re-run (not internal consistency), and only for the validator node
        // transactions, so a flood of them cannot stall the mempool on every block.
        let block_epoch = self
            .rules
            .consensus_constants(published_block.header.height)
            .block_height_to_epoch(published_block.header.height);
        if block_epoch != next_epoch || UnconfirmedPool::body_touches_validator_nodes(&published_block.body) {
            let mut invalid = Vec::new();
            for (key, tx) in self.unconfirmed_pool.validator_node_transactions() {
                let still_valid = match validate_chain_linked(self.validator.as_ref(), &tx) {
                    Ok(None) => true,
                    Ok(Some(parents)) => check_pool_parents(&self.unconfirmed_pool, &parents).is_ok(),
                    Err(response) => {
                        debug!(
                            target: LOG_TARGET,
                            "Tx: ({}) is no longer valid after block #{}: {response}, dropping",
                            tx_id(&tx),
                            published_block.header.height
                        );
                        false
                    },
                };
                if !still_valid {
                    invalid.push(key);
                }
            }
            if !invalid.is_empty() {
                // Descendants go too: each was checked against a pool that still held the others, so one spending an
                // invalidated transaction's outputs may have passed
                let dropped = self.unconfirmed_pool.remove_transactions_and_descendants(invalid)?;
                debug!(
                    target: LOG_TARGET,
                    "Dropped {dropped} validator node transaction(s) invalidated by block #{}, with their descendants",
                    published_block.header.height
                );
            }
        }
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

    /// Records the outcome of re-checking height-reading transactions for a block on top of `prev_hash` (see
    /// [height_check]), read from the pool when the mempool was at `generation` (see
    /// [`MempoolStorage::chain_generation`]). If the mempool has processed a chain change since, nothing is recorded:
    /// the pool may have been revalidated in the meantime. Otherwise the ones that `passed` are not checked again for
    /// that block, and if `prev_hash` is the block the mempool last processed, the ones that `failed` are dropped with
    /// every pool transaction that spends their outputs (not moved to the reorg pool, and not banned). Returns the
    /// number of transactions dropped, or `None` if the failures were not dropped.
    pub fn record_height_checks(
        &mut self,
        generation: u64,
        passed: &[TransactionKey],
        failed: &[TransactionKey],
        prev_hash: HashOutput,
    ) -> Result<Option<usize>, MempoolError> {
        if self.chain_generation != generation {
            return Ok(None);
        }
        for key in passed {
            self.unconfirmed_pool.record_height_check_passed(*key, prev_hash);
        }
        if self.last_seen_hash != prev_hash {
            return Ok(None);
        }
        Ok(Some(
            self.unconfirmed_pool
                .remove_transactions_and_descendants(failed.iter().copied())?,
        ))
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

        // Drain the unconfirmed pool first and keep drained unconfirmed transactions ahead of reorg pool transactions
        // so standalone unconfirmed transactions are restored before aggregates that include them. Any child whose
        // parent has not been restored yet is deferred and re-queued once its parent's outputs enter the pool.
        let removed_txs = self.unconfirmed_pool.drain_all_mempool_transactions();
        let num_removed_txs = removed_txs.len();

        // Remove re-orged transactions from reorg pool and re-submit them to the unconfirmed mempool
        let reorg_txs = self
            .reorg_pool
            .remove_reorged_txs_and_discard_double_spends(removed_blocks, new_blocks);
        let num_reorg_txs = reorg_txs.len();

        let pending_txs: Vec<Arc<Transaction>> = removed_txs.into_iter().chain(reorg_txs).collect();
        let num_invalid_txs = self
            .insert_txs(pending_txs)
            .map_err(|e| MempoolError::InternalError(e.to_string()))?;

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
        // Remove and revalidate all transactions from the unconfirmed and reorg pools in dependency order.
        let removed_txs = self.unconfirmed_pool.drain_all_mempool_transactions();
        let reorg_txs = self.reorg_pool.clear_and_retrieve_all();
        let pending_txs: Vec<Arc<Transaction>> = removed_txs.into_iter().chain(reorg_txs).collect();
        self.insert_txs(pending_txs)
            .map_err(|e| MempoolError::InternalError(e.to_string()))?;
        Ok(())
    }

    /// Returns all unconfirmed transaction stored in the Mempool, except the transactions stored in the ReOrgPool.
    pub fn snapshot(&self) -> Vec<Arc<Transaction>> {
        self.unconfirmed_pool.snapshot()
    }

    /// Returns a list of transaction ranked by transaction priority up to a given weight, leaving out what `filter`
    /// leaves out. Will only return transactions that will fit into the given weight
    pub fn retrieve(&self, total_weight: u64, filter: &TemplateFilter) -> Result<RetrieveResults, MempoolError> {
        // Only a ratio for ordering under byte pressure; the constants at the last seen height are close enough
        let max_block_transaction_weight = self
            .rules
            .consensus_constants(self.last_seen_height)
            .max_block_transaction_weight();
        self.unconfirmed_pool
            .fetch_template_txs(total_weight, max_block_transaction_weight, filter)
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

    /// Check if the specified transaction is stored in the Mempool (see `UnconfirmedPool::has_transaction` and
    /// `ReorgPool::has_transaction`).
    pub fn has_transaction(&self, tx: &Transaction) -> Result<TxStorageResponse, MempoolError> {
        if tx.body.kernels().is_empty() {
            return Err(MempoolError::TransactionNoKernels);
        }
        if self.unconfirmed_pool.has_transaction(tx) {
            Ok(TxStorageResponse::UnconfirmedPool)
        } else if self.reorg_pool.has_transaction(tx) {
            Ok(TxStorageResponse::ReorgPool)
        } else {
            Ok(TxStorageResponse::NotStored(None))
        }
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

/// Fee per gram x1000, computed as for the stored pool priority (`FeePriority`)
fn fee_per_gram_millis(tx: &Transaction, weighting: &TransactionWeight) -> Option<u64> {
    let weight = tx.calculate_weight(weighting).ok()?;
    tx.body
        .get_total_fee()
        .ok()?
        .as_u64()
        .saturating_mul(1000)
        .checked_div(weight)
}

/// Whether any input covenant of `tx` reads the block height
fn has_height_reading_covenant(tx: &Transaction) -> bool {
    tx.body
        .inputs()
        .iter()
        .any(|input| input.covenant().is_ok_and(|covenant| covenant.reads_block_height()))
}

/// The outcome of re-checking a pool transaction whose input scripts or covenants read the block height
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum HeightCheck {
    Passed,
    /// Not valid at a height at or above the one it was validated for: it can be dropped
    Failed(String),
    /// Not valid at a height below the one it was validated for: it may be valid again later, so it is kept
    NotYetValid(String),
}

/// Re-checks a pool transaction whose input scripts or covenants read the block height, for the block at `height` on
/// top of `prev_hash`, given `validation`, the block it was validated for.
///
/// The script signatures and the script offset were verified against the keys its height-reading scripts returned
/// then. A script that returns the same key gives the same result for both, and a different key cannot satisfy the
/// script signature, so only the height-reading scripts are run (no signature checks, other than any signature opcodes
/// in the scripts themselves), and their keys compared. Covenants are evaluated if one of them reads the height.
pub(crate) fn height_check(
    tx: &Transaction,
    validation: &HeightValidation,
    height: u64,
    prev_hash: &HashOutput,
) -> HeightCheck {
    let failure = match run_height_reading_scripts(&tx.body, height, prev_hash) {
        Err(e) => Some(e.to_string()),
        Ok(keys) if keys != validation.script_keys => Some("an input script returns a different key".to_string()),
        Ok(_) if has_height_reading_covenant(tx) => validate_covenants(&tx.body, height).err().map(|e| e.to_string()),
        Ok(_) => None,
    };
    // A failure at or above the validation height is treated as final, assuming it does not revert at a later height:
    // a spend whose script passes again later is dropped, and must be broadcast again then
    match failure {
        None => HeightCheck::Passed,
        Some(reason) if height < validation.height => HeightCheck::NotYetValid(reason),
        Some(reason) => HeightCheck::Failed(reason),
    }
}

/// The block after `tip`, the tip `tx` was validated against, if `tx` has an input script or covenant that reads the
/// height, with the keys its height-reading input scripts return there, to compare against when it is re-checked (see
/// [height_check]). Such a transaction is rejected if that tip is not known, or if its scripts fail there.
fn height_validation(tx: &Transaction, tip: Option<&ChainMetadata>) -> Result<HeightValidation, TxStorageResponse> {
    if !reads_block_height(&tx.body) {
        return Ok(HeightValidation::default());
    }
    let Some(tip) = tip else {
        return Err(TxStorageResponse::NotStored(Some(
            "The chain tip the transaction was validated against is not known".to_string(),
        )));
    };
    let height = tip.best_block_height().saturating_add(1);
    let script_keys = run_height_reading_scripts(&tx.body, height, tip.best_block_hash())
        .map_err(|e| TxStorageResponse::NotStored(Some(e.to_string())))?;
    Ok(HeightValidation { height, script_keys })
}

fn tx_id(tx: &Transaction) -> String {
    tx.body
        .kernels()
        .first()
        .map(|k| k.excess_sig.get_signature().to_hex())
        .unwrap_or_else(|| "None?!".into())
}

/// A successful validation attempt
struct ValidatedTransaction {
    /// The outputs the transaction spends from the mempool, if any
    parents: Option<PoolParents>,
    /// The block it was validated for, if it reads the height
    validation: HeightValidation,
}

/// A failed validation attempt
struct AttemptFailure {
    response: TxStorageResponse,
    /// Whether the failure may be due to the chain tip the transaction was validated against, so that it may be
    /// retried if the chain changed during validation
    tip_dependent: bool,
    /// Whether it is a failure of covenants that read the height: it is only retried if they pass at the new tip (see
    /// `covenants_pass_at_current_tip`)
    covenant_height_dependent: bool,
    /// Set if the failure is a final internal consistency failure
    ban_reason: Option<BanReason>,
}

/// One unlocked validation attempt: chain-linked checks, then the mempool parents of any inputs not in the chain
/// (under a brief read lock), then internal consistency against `tip`. On success, returns the hashes of the outputs
/// spent in the mempool, if any, and the block it was validated for (see `height_validation`), so that no script runs
/// under the write lock.
fn validate_attempt(
    storage: &RwLock<MempoolStorage>,
    validator: &dyn TransactionValidator,
    tx: &Transaction,
    tip: Option<&ChainMetadata>,
) -> Result<Result<ValidatedTransaction, AttemptFailure>, MempoolError> {
    let failure = |error: ValidationError| AttemptFailure {
        tip_dependent: is_tip_dependent(&error),
        covenant_height_dependent: matches!(
            error,
            ValidationError::AggregatedBodyValidationError(
                AggregatedBodyValidationError::Covenant {
                    height_dependent: true,
                    ..
                } | AggregatedBodyValidationError::CovenantOutputsClaimedTwice {
                    height_dependent: true,
                    ..
                },
            )
        ),
        response: validation_error_to_response(error),
        ban_reason: None,
    };
    let parents = match pool_parents(validator.validate_chain_linked(tx)) {
        Ok(parents) => parents,
        Err(e) => return Ok(Err(failure(e))),
    };
    if let Some(parents) = &parents {
        let lock = storage.read().map_err(|_| MempoolError::RwLockPoisonError)?;
        if let Err(response) = check_pool_parents(&lock.unconfirmed_pool, parents) {
            // The parents may have been mined (and so removed from the pool) during validation
            return Ok(Err(AttemptFailure {
                response,
                tip_dependent: true,
                covenant_height_dependent: false,
                ban_reason: None,
            }));
        }
    }
    if let Err(e) = validator.validate_internal_consistency(tx, tip) {
        let ban_reason = internal_failure_ban_reason(&e);
        return Ok(Err(AttemptFailure {
            ban_reason,
            ..failure(e)
        }));
    }
    match height_validation(tx, tip) {
        Ok(validation) => Ok(Ok(ValidatedTransaction { parents, validation })),
        Err(response) => Ok(Err(AttemptFailure {
            response,
            tip_dependent: false,
            covenant_height_dependent: false,
            ban_reason: None,
        })),
    }
}

/// A transaction that fails internal consistency validation with a failure that does not depend on the chain tip can
/// never be valid, and gets a long ban. Covenant failures are not banned, whether or not the failing covenant reads the
/// height (see `is_tip_dependent`), and exceeding the covenant work limit is not banned either. An input script failure
/// (see `is_tip_dependent`) is only excluded if the input that failed reads the block height. Such a failure cannot be
/// told apart from the height having moved on, so it is never banned, even if the script could never have succeeded;
/// reaching it means passing every earlier check first.
fn internal_failure_ban_reason(error: &ValidationError) -> Option<BanReason> {
    match error {
        ValidationError::AggregatedBodyValidationError(
            AggregatedBodyValidationError::Covenant { .. } |
            AggregatedBodyValidationError::CovenantWorkLimitExceeded { .. } |
            AggregatedBodyValidationError::CovenantOutputsClaimedTwice { .. },
        ) => None,
        ValidationError::AggregatedBodyValidationError(_) if !is_tip_dependent(error) => Some(BanReason {
            reason: error.to_string(),
            ban_duration: BanPeriod::Long,
        }),
        _ => None,
    }
}

/// Returns true if a validation error may be due to the chain tip the transaction was validated against, i.e. the
/// transaction may be valid at a different tip:
/// - inputs that are not (or no longer) in the chain,
/// - input maturity and kernel lock heights,
/// - an input script failure (running the script, its script signature or the key it returns), but only if the input
///   that failed reads the height of the next block. Any output in the chain can be spent with made-up input data to
///   produce a failing script, so other script failures are final. A script offset mismatch is final too: it is only
///   checked once every script signature has verified against the key its script returned. Since failure-driven retries
///   never take the lock, a height-reading script that can never succeed costs at most the unlocked attempts.
/// - a covenant failure, but only if a covenant that reads the block height is involved (the failing input's, or for an
///   output claimed twice any input's). Exceeding the covenant work limit is final.
///
/// Every other failure is final, since it cannot be fixed by a different tip (signatures, range proofs, balances,
/// sizes, sorting, weight, versions etc.), even if the transaction has an input script that reads the height. That
/// includes an input that is already spent and a kernel that is already mined: only a reorg can undo those, which is
/// rare, and the mempool revalidates its transactions after a reorg anyway.
fn is_tip_dependent(error: &ValidationError) -> bool {
    match error {
        ValidationError::UnknownInputs(_) | ValidationError::UnknownInput | ValidationError::MaturityError => true,
        ValidationError::TransactionError(e) |
        ValidationError::AggregatedBodyValidationError(AggregatedBodyValidationError::TransactionError(e)) => {
            is_tip_dependent_transaction_error(e)
        },
        ValidationError::AggregatedBodyValidationError(
            AggregatedBodyValidationError::MaturityError |
            AggregatedBodyValidationError::InputScript {
                height_dependent: true, ..
            } |
            AggregatedBodyValidationError::Covenant {
                height_dependent: true, ..
            } |
            AggregatedBodyValidationError::CovenantOutputsClaimedTwice {
                height_dependent: true, ..
            },
        ) => true,
        _ => false,
    }
}

/// Script errors only arrive wrapped in `AggregatedBodyValidationError::InputScript`, so they are not handled here
fn is_tip_dependent_transaction_error(error: &TransactionError) -> bool {
    matches!(error, TransactionError::InputMaturity)
}

/// Whether the covenants of `tx` pass for the block after the current chain tip. Covenants are checked before scripts,
/// so anyone can spend another's output whose covenant reads the height with made-up script data; such a covenant
/// failure is only worth validating again if the covenants now pass. No crypto, and bounded by the covenant work limit.
fn covenants_pass_at_current_tip(validator: &dyn TransactionValidator, tx: &Transaction) -> bool {
    match validator.chain_metadata() {
        Ok(Some(tip)) => validate_covenants(&tx.body, tip.best_block_height().saturating_add(1)).is_ok(),
        _ => false,
    }
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

/// The outputs spent by a transaction that are not in the chain's unspent set, and so must be in the mempool
struct PoolParents {
    outputs: Vec<HashOutput>,
    /// Those of `outputs` that were already mined and spent: without them in the pool the transaction is a double
    /// spend
    spent: Vec<HashOutput>,
}

/// Turns the result of chain-linked validation into the outputs the transaction spends from the mempool, if any
fn pool_parents(result: Result<(), ValidationError>) -> Result<Option<PoolParents>, ValidationError> {
    match result {
        Ok(()) => Ok(None),
        Err(ValidationError::UnknownInputs(outputs)) => Ok(Some(PoolParents { outputs, spent: vec![] })),
        Err(ValidationError::SpentInputs { mut unknown, spent }) => {
            unknown.extend_from_slice(&spent);
            Ok(Some(PoolParents {
                outputs: unknown,
                spent,
            }))
        },
        Err(e) => Err(e),
    }
}

/// Validates the transaction against the chain state. On success, returns the outputs spent by the transaction that
/// must be in the mempool, if any. On failure, returns the storage response for the rejected transaction.
fn validate_chain_linked(
    validator: &dyn TransactionValidator,
    tx: &Transaction,
) -> Result<Option<PoolParents>, TxStorageResponse> {
    pool_parents(validator.validate_chain_linked(tx)).map_err(validation_error_to_response)
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

/// Checks that every parent output is in the unconfirmed pool. The unconfirmed pool indexes outputs by their hash, and
/// an input's output hash commits to all of the spent output's data (commitment, script, features etc.), so an input
/// that does not spend a pool output exactly as it was created is not found and is an orphan, or a double spend if
/// the output was already spent in the chain.
fn check_pool_parents(pool: &UnconfirmedPool, parents: &PoolParents) -> Result<(), TxStorageResponse> {
    if !pool.contains_all_outputs(&parents.spent) {
        return Err(TxStorageResponse::NotStoredAlreadySpent);
    }
    if pool.contains_all_outputs(&parents.outputs) {
        Ok(())
    } else {
        Err(TxStorageResponse::NotStoredOrphan)
    }
}

fn validation_error_to_response(error: ValidationError) -> TxStorageResponse {
    match error {
        ValidationError::UnknownInputs(_) => TxStorageResponse::NotStoredOrphan,
        ValidationError::ContainsSTxO | ValidationError::SpentInputs { .. } => {
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
    use std::sync::{
        Mutex,
        Weak,
        atomic::{AtomicUsize, Ordering},
    };

    use tari_script::{ScriptError, TariScript, script};
    use tari_transaction_components::{
        MicroMinotari,
        covenant,
        key_manager::KeyManager,
        transaction_components::{
            SpentOutput,
            WalletOutput,
            covenants::{Covenant, CovenantError},
        },
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
                storage.write().unwrap().insert(reusing.clone(), None).unwrap()
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

    fn script_error(error: ScriptError, height_dependent: bool) -> ValidationError {
        AggregatedBodyValidationError::InputScript {
            error: TransactionError::ScriptError(error),
            height_dependent,
        }
        .into()
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
            script_error(ScriptError::VerifyFailed, false),
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
    fn insert_unlocked_keeps_the_script_keys_returned_at_the_validation_tip() {
        let key_manager = KeyManager::new_random().unwrap();
        let tx = with_input_script(&create_tx(&key_manager), script!(CheckHeightVerify(2) Nop).unwrap());
        let tip = FixedHash::from([1u8; 32]);
        let validator = ScriptedValidator::new(vec![Ok(())], vec![tip]);
        let storage = create_storage(&validator);
        let response = MempoolStorage::insert_unlocked(&storage, tx.clone()).unwrap();
        assert_eq!(response, TxStorageResponse::UnconfirmedPool);

        // The scripted tip is at height 1, so the transaction was validated for height 2
        let expected = run_height_reading_scripts(&tx.body, 2, &tip).unwrap();
        assert_eq!(expected.len(), 1);
        let lock = storage.read().unwrap();
        let to_check = lock
            .unconfirmed_pool
            .height_reading_to_check(&[tx], &FixedHash::from([2u8; 32]));
        assert_eq!(to_check.len(), 1);
        assert_eq!(to_check[0].2, HeightValidation {
            height: 2,
            script_keys: expected
        });
    }

    /// A copy of `tx` whose first input's covenant is replaced by `covenant`
    fn with_input_covenant(tx: &Transaction, covenant: Covenant) -> Arc<Transaction> {
        let mut inputs = tx.body.inputs().clone();
        match &mut inputs[0].spent_output {
            SpentOutput::OutputData { covenant: c, .. } => *c = covenant,
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

    #[test]
    fn height_dependent_covenant_failure_is_retried_only_if_the_covenants_pass_at_the_new_tip() {
        let key_manager = KeyManager::new_random().unwrap();
        let validated_at = FixedHash::from([1u8; 32]);
        let new_tip = FixedHash::from([2u8; 32]);
        let covenant_error = || -> ValidationError {
            AggregatedBodyValidationError::Covenant {
                error: CovenantError::NoMatchingOutputs,
                height_dependent: true,
            }
            .into()
        };
        // The scripted tips are at height 1, so the new tip's next block is at height 2
        for (covenant, retried) in [
            (covenant!(absolute_height(@uint(100))).unwrap(), false),
            (covenant!(absolute_height(@uint(2))).unwrap(), true),
        ] {
            let tx = with_input_covenant(&create_tx(&key_manager), covenant);
            let validator = ScriptedValidator::new(vec![], vec![validated_at, new_tip]);
            validator.chain_changes.store(1, Ordering::SeqCst);
            *validator.internal_results.lock().unwrap() = vec![Err(covenant_error()), Ok(())].into();
            let storage = create_storage(&validator);
            let response = MempoolStorage::insert_unlocked(&storage, tx).unwrap();
            if retried {
                assert_eq!(response, TxStorageResponse::UnconfirmedPool);
                assert_eq!(validator.internal_calls(), 2);
            } else {
                assert!(matches!(response, TxStorageResponse::NotStored(_)), "{response:?}");
                assert_eq!(validator.internal_calls(), 1);
            }
        }
    }

    #[test]
    fn height_check_results_are_recorded_only_if_the_chain_did_not_change() {
        let key_manager = KeyManager::new_random().unwrap();
        let validator = ScriptedValidator::new(vec![], vec![]);
        let storage = create_storage(&validator);
        let tx = with_input_script(&create_tx(&key_manager), script!(CheckHeightVerify(2) Nop).unwrap());
        let tip = FixedHash::from([1u8; 32]);
        let mut lock = storage.write().unwrap();
        lock.unconfirmed_pool
            .insert(tx.clone(), None, &TransactionWeight::latest())
            .unwrap();
        lock.last_seen_hash = tip;
        let generation = lock.chain_generation;
        let (key, ..) = lock
            .unconfirmed_pool
            .height_reading_to_check(std::slice::from_ref(&tx), &tip)[0]
            .clone();

        // A chain change since the check was read: nothing is recorded
        lock.chain_generation = generation.wrapping_add(1);
        assert_eq!(lock.record_height_checks(generation, &[key], &[], tip).unwrap(), None);
        assert_eq!(
            lock.unconfirmed_pool
                .height_reading_to_check(std::slice::from_ref(&tx), &tip)
                .len(),
            1
        );
        assert_eq!(lock.record_height_checks(generation, &[], &[key], tip).unwrap(), None);
        assert_eq!(lock.unconfirmed_pool.len(), 1);
        let generation = lock.chain_generation;

        // Not on top of the mempool's tip: a failure is kept
        let other = FixedHash::from([2u8; 32]);
        assert_eq!(lock.record_height_checks(generation, &[], &[key], other).unwrap(), None);
        assert_eq!(lock.unconfirmed_pool.len(), 1);

        // Otherwise it is dropped
        assert_eq!(
            lock.record_height_checks(generation, &[], &[key], tip).unwrap(),
            Some(1)
        );
        assert_eq!(lock.unconfirmed_pool.len(), 0);
    }

    #[test]
    fn height_reading_transaction_is_rejected_if_its_validation_tip_is_unknown() {
        let key_manager = KeyManager::new_random().unwrap();
        let height_reading = with_input_script(&create_tx(&key_manager), script!(CheckHeightVerify(2) Nop).unwrap());
        // No chain tip is known, so the validator validates against its own
        let validator = ScriptedValidator::new(vec![], vec![]);
        let storage = create_storage(&validator);
        let response = MempoolStorage::insert_unlocked(&storage, height_reading.clone()).unwrap();
        assert!(matches!(response, TxStorageResponse::NotStored(_)), "{response:?}");
        let response = storage.write().unwrap().insert(height_reading, None).unwrap();
        assert!(matches!(response, TxStorageResponse::NotStored(_)), "{response:?}");
        assert_eq!(pool_len(&storage), 0);

        // A transaction without such a script does not need the tip
        let response = MempoolStorage::insert_unlocked(&storage, create_tx(&key_manager)).unwrap();
        assert_eq!(response, TxStorageResponse::UnconfirmedPool);
    }

    #[test]
    fn height_dependent_script_failure_is_retried_if_chain_changed() {
        let key_manager = KeyManager::new_random().unwrap();
        let validated_at = FixedHash::from([1u8; 32]);
        let new_tip = FixedHash::from([2u8; 32]);
        // Reads the height, and runs at the height after the scripted tips, where its keys are cached on insert
        let tx = with_input_script(&create_tx(&key_manager), script!(CheckHeightVerify(2) Nop).unwrap());

        let validator = ScriptedValidator::new(vec![], vec![validated_at, new_tip]);
        validator.chain_changes.store(1, Ordering::SeqCst);
        *validator.internal_results.lock().unwrap() =
            vec![Err(script_error(ScriptError::VerifyFailed, true)), Ok(())].into();
        let storage = create_storage(&validator);
        let response = MempoolStorage::insert_unlocked(&storage, tx.clone()).unwrap();
        assert_eq!(response, TxStorageResponse::UnconfirmedPool);
        assert_eq!(validator.chain_linked_calls(), 2);
        assert_eq!(validator.internal_calls(), 2);
        assert_eq!(validator.internal_lock_free(), vec![true, true]);

        // Without a chain change, the same failure is final
        let validator = ScriptedValidator::new(vec![], vec![validated_at]);
        *validator.internal_results.lock().unwrap() = vec![Err(script_error(ScriptError::VerifyFailed, true))].into();
        let storage = create_storage(&validator);
        let response = MempoolStorage::insert_unlocked(&storage, tx).unwrap();
        assert!(matches!(response, TxStorageResponse::NotStored(_)), "{response:?}");
        assert_eq!(validator.internal_calls(), 1);
    }

    #[test]
    fn only_a_height_dependent_input_script_failure_escapes_the_ban() {
        let key_manager = KeyManager::new_random().unwrap();
        let tx = with_input_script(&create_tx(&key_manager), script!(CheckHeightVerify(100) Nop).unwrap());
        let ban_for = |error: ValidationError| {
            let validator = ScriptedValidator::new(vec![], vec![FixedHash::from([1u8; 32])]);
            *validator.internal_results.lock().unwrap() = vec![Err(error)].into();
            let storage = create_storage(&validator);
            let (response, ban) = MempoolStorage::insert_unlocked_with_ban_reason(&storage, tx.clone()).unwrap();
            assert!(matches!(response, TxStorageResponse::NotStored(_)), "{response:?}");
            ban.map(|ban| ban.ban_duration())
        };

        // Failures unrelated to the input scripts are banned, even though the transaction reads the height
        let kernel_signature = AggregatedBodyValidationError::TransactionError(
            TransactionError::InvalidSignatureError("Verifying kernel signature".to_string()),
        );
        let range_proof =
            AggregatedBodyValidationError::TransactionError(TransactionError::RangeProofError("bad".to_string()));
        let script_offset = AggregatedBodyValidationError::TransactionError(TransactionError::ScriptOffset);
        for error in [
            kernel_signature,
            range_proof,
            AggregatedBodyValidationError::InvalidAccountingBalance,
            script_offset,
        ] {
            assert_eq!(ban_for(error.into()), Some(BanPeriod::Long));
        }

        // An input script failure is banned unless the failing input reads the height
        assert_eq!(
            ban_for(script_error(ScriptError::VerifyFailed, false)),
            Some(BanPeriod::Long)
        );
        let height_dependent = script_error(ScriptError::VerifyFailed, true);
        assert!(is_tip_dependent(&height_dependent));
        assert_eq!(ban_for(height_dependent), None);
    }

    #[test]
    fn covenant_failure_is_tip_dependent_only_if_a_covenant_reads_the_height() {
        let covenant = |height_dependent| -> ValidationError {
            AggregatedBodyValidationError::Covenant {
                error: CovenantError::NoMatchingOutputs,
                height_dependent,
            }
            .into()
        };
        let claimed_twice = |height_dependent| -> ValidationError {
            AggregatedBodyValidationError::CovenantOutputsClaimedTwice {
                inputs: 2,
                matched: 1,
                height_dependent,
            }
            .into()
        };
        assert!(is_tip_dependent(&covenant(true)));
        assert!(is_tip_dependent(&claimed_twice(true)));
        assert!(!is_tip_dependent(&covenant(false)));
        assert!(!is_tip_dependent(&claimed_twice(false)));
        let work_limit = AggregatedBodyValidationError::CovenantWorkLimitExceeded { limit: 1 }.into();
        assert!(!is_tip_dependent(&work_limit));
        // None of them is banned
        for error in [
            covenant(true),
            covenant(false),
            claimed_twice(true),
            claimed_twice(false),
            work_limit,
        ] {
            assert!(internal_failure_ban_reason(&error).is_none(), "{error}");
        }
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
            (height_script_tx, || script_error(ScriptError::VerifyFailed, true)),
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
        let response = storage.write().unwrap().insert(create_tx(&key_manager), None).unwrap();
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
                storage.write().unwrap().insert(child.clone(), None).unwrap()
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

    #[test]
    fn a_validator_node_transaction_invalidated_by_a_block_is_dropped_with_its_spenders() {
        use tari_common_types::epoch::VnEpoch;
        use tari_crypto::keys::SecretKey;
        use tari_transaction_components::transaction_components::{OutputFeatures, ValidatorNodeSignature};
        let key_manager = KeyManager::new_random().unwrap();
        let exit_features = || {
            let vn_key = PrivateKey::random(&mut rand::rng());
            let signature = ValidatorNodeSignature::sign_for_exit(&vn_key, 0x10, None, VnEpoch(0), VnEpoch(10));
            OutputFeatures::for_validator_node_exit(signature, None, VnEpoch(0), VnEpoch(10))
        };
        // Valid when inserted, invalid when re-checked after the block
        let validator = ScriptedValidator::new(vec![Ok(()), Ok(()), Err(ValidationError::ContainsSTxO)], vec![
            FixedHash::zero(),
        ]);
        let storage = create_storage(&validator);
        let (vn_tx, vn_outputs) =
            tx!(MicroMinotari(100_000), fee: MicroMinotari(5), inputs: 1, outputs: 1, features: exit_features(), &key_manager)
                .map(|(tx, _, outputs)| (Arc::new(tx), outputs))
                .unwrap();
        let child = spending(&create_tx(&key_manager), &vn_outputs[0], &key_manager);
        for tx in [&vn_tx, &child] {
            let response = storage.write().unwrap().insert(tx.clone(), None).unwrap();
            assert_eq!(response, TxStorageResponse::UnconfirmedPool);
        }

        // A block exiting another validator node re-checks the pool's validator node transactions
        let other_vn_tx =
            tx!(MicroMinotari(100_000), fee: MicroMinotari(5), inputs: 1, outputs: 1, features: exit_features(), &key_manager)
                .unwrap()
                .0;
        let block = Block::new(
            tari_node_components::blocks::BlockHeader::new(0),
            other_vn_tx.body.clone(),
        );
        storage.write().unwrap().process_published_block(&block).unwrap();

        assert_eq!(pool_len(&storage), 0);
        assert!(storage.read().unwrap().unconfirmed_pool.check_data_consistency());
    }

    fn setup_three_block_chain_and_mempool() -> (
        crate::test_helpers::blockchain::TestBlockchain,
        crate::mempool::Mempool,
        WalletOutput,
        WalletOutput,
        Arc<tari_node_components::blocks::ChainBlock>,
    ) {
        use tari_transaction_components::crypto_factories::CryptoFactories;

        use crate::{
            mempool::Mempool,
            test_helpers::blockchain::TestBlockchain,
            validation::transaction::TransactionFullValidator,
        };

        let rules = create_consensus_rules();
        let mut blockchain = TestBlockchain::create(rules.clone());
        let (_block_1, coinbase_1) = blockchain.add_next_tip(crate::block_spec!("1->GB")).unwrap();
        let (_block_2, coinbase_2) = blockchain.add_next_tip(crate::block_spec!("2->1")).unwrap();
        let (block_3, _coinbase_3) = blockchain.add_next_tip(crate::block_spec!("3->2")).unwrap();

        let validator = TransactionFullValidator::new(
            CryptoFactories::default(),
            false,
            blockchain.db().clone(),
            rules.clone(),
        );
        let mempool = Mempool::new(MempoolConfig::default(), rules, Box::new(validator));
        (blockchain, mempool, coinbase_1, coinbase_2, block_3)
    }

    #[allow(clippy::arithmetic_side_effects)] // Transaction aggregation, not integer arithmetic
    fn aggregate_two_transactions(tx1: &Transaction, tx2: &Transaction) -> Arc<Transaction> {
        let mut agg_inputs = tx1.body.inputs().clone();
        agg_inputs.extend(tx2.body.inputs().iter().cloned());
        let mut agg_outputs = tx1.body.outputs().clone();
        agg_outputs.extend(tx2.body.outputs().iter().cloned());
        let mut agg_kernels = tx1.body.kernels().clone();
        agg_kernels.extend(tx2.body.kernels().iter().cloned());
        let mut tx_agg = Transaction::new(
            agg_inputs,
            agg_outputs,
            agg_kernels,
            &tx1.offset + &tx2.offset,
            &tx1.script_offset + &tx2.script_offset,
        );
        tx_agg.body.sort();
        Arc::new(tx_agg)
    }

    fn reorg_onto_two_block_fork(
        blockchain: &mut crate::test_helpers::blockchain::TestBlockchain,
        fork_point: Arc<tari_node_components::blocks::ChainBlock>,
    ) -> Vec<Arc<Block>> {
        use crate::{chain_storage::BlockAddResult, test_helpers::blockchain::create_chained_blocks};

        let (_fork_names, fork_blocks) = create_chained_blocks(
            blockchain.db(),
            crate::block_specs!(["fb1->GB"], ["fb2->fb1"]),
            fork_point,
        );
        let block_b1 = fork_blocks.get("fb1").unwrap().clone().to_arc_block();
        let block_b2 = fork_blocks.get("fb2").unwrap().clone().to_arc_block();
        blockchain.db().add_block(block_b1.clone()).unwrap();
        let reorg_result = blockchain.db().add_block(block_b2.clone()).unwrap().result;
        assert!(
            matches!(reorg_result, BlockAddResult::ChainReorg { .. }),
            "Expected ChainReorg when adding heavier fork, got: {reorg_result:?}"
        );
        vec![block_b1, block_b2]
    }

    #[tokio::test]
    async fn process_reorg_keeps_valid_child_of_reorged_block_transaction() {
        use tari_transaction_components::{test_helpers::spend_utxos, txn_schema};

        let (mut blockchain, mempool, coinbase_1, _coinbase_2, block_3) = setup_three_block_chain_and_mempool();

        let (tx_parent, parent_outputs) = spend_utxos(
            txn_schema!(from: vec![coinbase_1], to: vec![MicroMinotari(2_000_000)], fee: MicroMinotari(100)),
            &blockchain.km,
        );
        let (tx_child, _child_outputs) = spend_utxos(
            txn_schema!(from: vec![parent_outputs[0].clone()], to: vec![MicroMinotari(1_000_000)], fee: MicroMinotari(100)),
            &blockchain.km,
        );
        let tx_parent = Arc::new(tx_parent);
        let tx_child = Arc::new(tx_child);

        assert_eq!(
            mempool.insert(tx_parent.clone()).await.unwrap(),
            TxStorageResponse::UnconfirmedPool
        );
        let (block_4a, _) = blockchain
            .add_next_tip(crate::block_spec!("4a->3", transactions: vec![(*tx_parent).clone()]))
            .unwrap();
        mempool.process_published_block(block_4a.to_arc_block()).await.unwrap();
        assert_eq!(
            mempool.has_transaction(tx_parent.clone()).await.unwrap(),
            TxStorageResponse::ReorgPool
        );

        assert_eq!(
            mempool.insert(tx_child.clone()).await.unwrap(),
            TxStorageResponse::UnconfirmedPool
        );

        let new_blocks = reorg_onto_two_block_fork(&mut blockchain, block_3);
        mempool
            .process_reorg(vec![block_4a.to_arc_block()], new_blocks)
            .await
            .unwrap();

        assert_eq!(
            mempool.has_transaction(tx_parent).await.unwrap(),
            TxStorageResponse::UnconfirmedPool
        );
        assert_eq!(
            mempool.has_transaction(tx_child).await.unwrap(),
            TxStorageResponse::UnconfirmedPool
        );
    }

    #[tokio::test]
    async fn reorg_pool_partial_kernel_match_cleans_up_all_kernel_signatures() {
        use tari_transaction_components::{test_helpers::spend_utxos, txn_schema};

        let (mut blockchain, mempool, coinbase_1, coinbase_2, _block_3) = setup_three_block_chain_and_mempool();

        let mut setup_schema = txn_schema!(
            from: vec![coinbase_2],
            to: vec![MicroMinotari(1_000_000)],
            fee: MicroMinotari(100)
        );
        setup_schema.script =
            script!(CheckHeight(5) LeZero IfThen Nop Else Drop PushPubKey(Box::default()) EndIf).unwrap();
        let (tx_setup, setup_outputs) = spend_utxos(setup_schema, &blockchain.km);
        let (block_4, _coinbase_4) = blockchain
            .add_next_tip(crate::block_spec!("4->3", transactions: vec![tx_setup]))
            .unwrap();

        let (tx1, _) = spend_utxos(
            txn_schema!(from: vec![coinbase_1], to: vec![MicroMinotari(1_000_000)], fee: MicroMinotari(100)),
            &blockchain.km,
        );
        let (tx2, _) = spend_utxos(
            txn_schema!(from: vec![setup_outputs[0].clone()], to: vec![MicroMinotari(500_000)], fee: MicroMinotari(100)),
            &blockchain.km,
        );
        let k2_sig = tx2.body.kernels()[0].excess_sig.get_signature().clone();
        let tx_agg = aggregate_two_transactions(&tx1, &tx2);

        assert_eq!(
            mempool.insert(tx_agg.clone()).await.unwrap(),
            TxStorageResponse::UnconfirmedPool
        );

        let (block_5a, _) = blockchain
            .add_next_tip(crate::block_spec!("5a->4", transactions: vec![tx1]))
            .unwrap();
        mempool.process_published_block(block_5a.to_arc_block()).await.unwrap();
        assert_eq!(
            mempool.has_transaction(tx_agg.clone()).await.unwrap(),
            TxStorageResponse::ReorgPool
        );

        let new_blocks = reorg_onto_two_block_fork(&mut blockchain, block_4);
        mempool
            .process_reorg(vec![block_5a.to_arc_block()], new_blocks)
            .await
            .unwrap();

        let (found, remaining) = mempool.retrieve_by_excess_sigs(vec![k2_sig.clone()]).await.unwrap();
        assert!(found.is_empty());
        assert_eq!(remaining, vec![k2_sig]);
    }

    #[tokio::test]
    async fn process_reorg_keeps_reorged_aggregate_depending_on_unconfirmed_parent() {
        use tari_transaction_components::{test_helpers::spend_utxos, txn_schema};

        let (mut blockchain, mempool, coinbase_1, coinbase_2, block_3) = setup_three_block_chain_and_mempool();

        // A stays unconfirmed; B spends A's output; C spends coinbase_2; aggregate B+C is in the mempool.
        let (tx_a, a_outputs) = spend_utxos(
            txn_schema!(from: vec![coinbase_1], to: vec![MicroMinotari(2_000_000)], fee: MicroMinotari(100)),
            &blockchain.km,
        );
        let (tx_b, _) = spend_utxos(
            txn_schema!(from: vec![a_outputs[0].clone()], to: vec![MicroMinotari(1_000_000)], fee: MicroMinotari(100)),
            &blockchain.km,
        );
        let (tx_c, _) = spend_utxos(
            txn_schema!(from: vec![coinbase_2], to: vec![MicroMinotari(1_000_000)], fee: MicroMinotari(100)),
            &blockchain.km,
        );
        let tx_a = Arc::new(tx_a);
        let tx_bc = aggregate_two_transactions(&tx_b, &tx_c);

        assert_eq!(
            mempool.insert(tx_a.clone()).await.unwrap(),
            TxStorageResponse::UnconfirmedPool
        );
        assert_eq!(
            mempool.insert(tx_bc.clone()).await.unwrap(),
            TxStorageResponse::UnconfirmedPool
        );

        // Block 4a mines only independent C, moving aggregate B+C to reorg_pool while A remains unconfirmed.
        let (block_4a, _) = blockchain
            .add_next_tip(crate::block_spec!("4a->3", transactions: vec![tx_c]))
            .unwrap();
        mempool.process_published_block(block_4a.to_arc_block()).await.unwrap();
        assert_eq!(
            mempool.has_transaction(tx_a.clone()).await.unwrap(),
            TxStorageResponse::UnconfirmedPool
        );
        assert_eq!(
            mempool.has_transaction(tx_bc.clone()).await.unwrap(),
            TxStorageResponse::ReorgPool
        );

        let new_blocks = reorg_onto_two_block_fork(&mut blockchain, block_3);
        mempool
            .process_reorg(vec![block_4a.to_arc_block()], new_blocks)
            .await
            .unwrap();

        assert_eq!(
            mempool.has_transaction(tx_a).await.unwrap(),
            TxStorageResponse::UnconfirmedPool
        );
        assert_eq!(
            mempool.has_transaction(tx_bc).await.unwrap(),
            TxStorageResponse::UnconfirmedPool
        );
    }

    #[tokio::test]
    async fn process_reorg_keeps_unconfirmed_parent_and_child_in_reverse_drain_order() {
        use tari_transaction_components::{crypto_factories::CryptoFactories, test_helpers::spend_utxos, txn_schema};

        use crate::validation::transaction::TransactionFullValidator;

        let (mut blockchain, _mempool, coinbase_1, _coinbase_2, block_3) = setup_three_block_chain_and_mempool();
        let rules = create_consensus_rules();
        let validator = TransactionFullValidator::new(
            CryptoFactories::default(),
            false,
            blockchain.db().clone(),
            rules.clone(),
        );
        let mut storage = MempoolStorage::new(MempoolConfig::default(), rules, Box::new(validator));

        let (tx_parent, parent_outputs) = spend_utxos(
            txn_schema!(from: vec![coinbase_1], to: vec![MicroMinotari(2_000_000)], fee: MicroMinotari(100)),
            &blockchain.km,
        );
        let (tx_child, _child_outputs) = spend_utxos(
            txn_schema!(from: vec![parent_outputs[0].clone()], to: vec![MicroMinotari(1_000_000)], fee: MicroMinotari(100)),
            &blockchain.km,
        );
        let tx_parent = Arc::new(tx_parent);
        let tx_child = Arc::new(tx_child);

        // Advance `key_counter` until the `HashMap` bucket order yields `tx_child` before `tx_parent` on drain.
        let mut found_reverse_order = false;
        for _ in 0..64 {
            assert_eq!(
                storage.insert(tx_parent.clone(), None).unwrap(),
                TxStorageResponse::UnconfirmedPool
            );
            assert_eq!(
                storage.insert(tx_child.clone(), None).unwrap(),
                TxStorageResponse::UnconfirmedPool
            );
            if storage.snapshot().first() == Some(&tx_child) {
                found_reverse_order = true;
                break;
            }
            storage.unconfirmed_pool.drain_all_mempool_transactions();
        }
        assert!(
            found_reverse_order,
            "Failed to observe reverse HashMap drain order (tx_child before tx_parent) within 64 attempts"
        );

        let (block_4a, _) = blockchain.add_next_tip(crate::block_spec!("4a->3")).unwrap();
        let new_blocks = reorg_onto_two_block_fork(&mut blockchain, block_3);
        storage.process_reorg(&[block_4a.to_arc_block()], &new_blocks).unwrap();

        assert_eq!(
            storage.has_transaction(&tx_parent).unwrap(),
            TxStorageResponse::UnconfirmedPool
        );
        assert_eq!(
            storage.has_transaction(&tx_child).unwrap(),
            TxStorageResponse::UnconfirmedPool
        );
    }

    #[tokio::test]
    async fn process_sync_keeps_valid_child_of_reorg_pool_transaction() {
        use tari_transaction_components::{test_helpers::spend_utxos, txn_schema};

        let (mut blockchain, mempool, coinbase_1, _coinbase_2, block_3) = setup_three_block_chain_and_mempool();

        let (tx_parent, parent_outputs) = spend_utxos(
            txn_schema!(from: vec![coinbase_1], to: vec![MicroMinotari(2_000_000)], fee: MicroMinotari(100)),
            &blockchain.km,
        );
        let (tx_child, _child_outputs) = spend_utxos(
            txn_schema!(from: vec![parent_outputs[0].clone()], to: vec![MicroMinotari(1_000_000)], fee: MicroMinotari(100)),
            &blockchain.km,
        );
        let tx_parent = Arc::new(tx_parent);
        let tx_child = Arc::new(tx_child);

        assert_eq!(
            mempool.insert(tx_parent.clone()).await.unwrap(),
            TxStorageResponse::UnconfirmedPool
        );
        let (block_4a, _) = blockchain
            .add_next_tip(crate::block_spec!("4a->3", transactions: vec![(*tx_parent).clone()]))
            .unwrap();
        mempool.process_published_block(block_4a.to_arc_block()).await.unwrap();
        assert_eq!(
            mempool.has_transaction(tx_parent.clone()).await.unwrap(),
            TxStorageResponse::ReorgPool
        );
        assert_eq!(
            mempool.insert(tx_child.clone()).await.unwrap(),
            TxStorageResponse::UnconfirmedPool
        );

        let _new_blocks = reorg_onto_two_block_fork(&mut blockchain, block_3);
        mempool.process_sync().await.unwrap();

        assert_eq!(
            mempool.has_transaction(tx_parent).await.unwrap(),
            TxStorageResponse::UnconfirmedPool
        );
        assert_eq!(
            mempool.has_transaction(tx_child).await.unwrap(),
            TxStorageResponse::UnconfirmedPool
        );
    }

    #[test]
    fn insert_txs_wakes_deferred_child_when_parent_evicts_from_full_pool() {
        let key_manager = KeyManager::new_random().unwrap();
        let tx_with_fee = |fee: u64| {
            let (tx, _, outputs) =
                tx!(MicroMinotari(100_000), fee: MicroMinotari(fee), inputs: 1, outputs: 1, &key_manager).unwrap();
            (Arc::new(tx), outputs)
        };
        let (low_1, _) = tx_with_fee(5);
        let (low_2, _) = tx_with_fee(10);
        let (parent, parent_outputs) = tx_with_fee(50);
        let (child_base, _) = tx_with_fee(100);
        let child = spending(&child_base, &parent_outputs[0], &key_manager);
        let parent_output = child.body.inputs()[0].output_hash();

        let validator = ScriptedValidator::new(
            vec![
                Err(ValidationError::UnknownInputs(vec![parent_output])),
                Ok(()),
                Ok(()),
                Ok(()),
                Err(ValidationError::UnknownInputs(vec![parent_output])),
            ],
            vec![FixedHash::zero()],
        );
        let storage = create_storage(&validator);
        storage.write().unwrap().unconfirmed_pool.config.storage_capacity = 2;

        // `child` is deferred first; `low_1` and `low_2` fill the pool to capacity 2; `parent` evicts `low_1` (pool
        // length stays 2) and must still wake `child`, which then evicts `low_2`.
        let invalid = storage
            .write()
            .unwrap()
            .insert_txs(vec![child.clone(), low_1.clone(), low_2.clone(), parent.clone()])
            .unwrap();
        assert_eq!(invalid, 0);
        assert_eq!(pool_len(&storage), 2);
        let lock = storage.read().unwrap();
        assert_eq!(
            lock.has_transaction(&parent).unwrap(),
            TxStorageResponse::UnconfirmedPool
        );
        assert_eq!(
            lock.has_transaction(&child).unwrap(),
            TxStorageResponse::UnconfirmedPool
        );
        assert_eq!(
            lock.has_transaction(&low_1).unwrap(),
            TxStorageResponse::NotStored(None)
        );
        assert_eq!(
            lock.has_transaction(&low_2).unwrap(),
            TxStorageResponse::NotStored(None)
        );
        assert!(lock.unconfirmed_pool.check_data_consistency());
    }

    #[test]
    fn insert_txs_waits_for_all_missing_parents_before_retrying_orphan() {
        let key_manager = KeyManager::new_random().unwrap();
        let (parent_1, outputs_1) = create_tx_with_outputs(&key_manager);
        let (parent_2, outputs_2) = create_tx_with_outputs(&key_manager);
        let base = create_tx(&key_manager);
        let input_1 = outputs_1[0].to_transaction_input(&key_manager).unwrap();
        let input_2 = outputs_2[0].to_transaction_input(&key_manager).unwrap();
        let out_1 = input_1.output_hash();
        let out_2 = input_2.output_hash();
        let mut child_body = Transaction::new(
            vec![input_1, input_2],
            base.body.outputs().clone(),
            base.body.kernels().clone(),
            base.offset.clone(),
            base.script_offset.clone(),
        );
        child_body.body.sort();
        let child = Arc::new(child_body);

        let validator = ScriptedValidator::new(
            vec![
                Err(ValidationError::UnknownInputs(vec![out_1, out_2])),
                Ok(()),
                Ok(()),
                Err(ValidationError::UnknownInputs(vec![out_1, out_2])),
            ],
            vec![FixedHash::zero()],
        );
        let storage = create_storage(&validator);
        storage
            .write()
            .unwrap()
            .insert_txs(vec![child.clone(), parent_1.clone(), parent_2.clone()])
            .unwrap();

        assert_eq!(validator.chain_linked_calls(), 4);
        assert_eq!(pool_len(&storage), 3);
        let lock = storage.read().unwrap();
        assert_eq!(
            lock.has_transaction(&parent_1).unwrap(),
            TxStorageResponse::UnconfirmedPool
        );
        assert_eq!(
            lock.has_transaction(&parent_2).unwrap(),
            TxStorageResponse::UnconfirmedPool
        );
        assert_eq!(
            lock.has_transaction(&child).unwrap(),
            TxStorageResponse::UnconfirmedPool
        );
    }
}
