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
    collections::HashSet,
    sync::{
        Arc,
        RwLock,
        atomic::{AtomicBool, Ordering},
    },
};

use log::{debug, info};
use tari_common_types::types::{CompressedSignature, FixedHash, HashOutput, PrivateKey};
use tari_node_components::blocks::Block;
use tari_transaction_components::{
    BanReason,
    rpc::models::FeePerGramStat,
    transaction_components::{Transaction, covenants::MAX_COVENANT_WORK_PER_BODY},
};
use tari_utilities::hex::Hex;
use tokio::{
    sync::{OwnedSemaphorePermit, Semaphore, watch},
    task,
};

use crate::{
    consensus::BaseNodeConsensusManager,
    mempool::{
        MempoolConfig,
        StateResponse,
        StatsResponse,
        TxStorageResponse,
        error::MempoolError,
        mempool_storage::{HeightCheck, MempoolStorage, height_check},
        unconfirmed_pool::{HeightValidation, TemplateFilter, TransactionKey},
    },
    validation::TransactionValidator,
};

pub const LOG_TARGET: &str = "c::mp::mempool";

/// The most times a block template is selected from the pool (see [Mempool::retrieve])
const MAX_TEMPLATE_SELECTION_PASSES: usize = 3;

/// The most pool transactions with an input script or covenant that reads the height that the background re-check
/// after each block visits (see [Mempool::sweep_height_reading]), which bounds its walk of the pool's index
const HEIGHT_READING_SWEEP_BATCH: usize = 500;

/// The most work the background re-check after each block does (see [Mempool::sweep_height_reading]), in covenant
/// work units: input scripts that read the height are charged `SCRIPT_INPUT_WORK` each (about 0.8 ms at most), and a
/// transaction with a covenant that reads the height is charged its covenant work limit, `MAX_COVENANT_WORK_PER_BODY`
/// (at most about a second, if every unit hashes a field at its size limit). So a re-check runs at most about a second
/// of scripts and covenants, without holding the storage lock, and only one runs at a time. A single transaction over
/// the budget is still re-checked, alone: about a second for its covenants, plus its height-reading scripts, of which
/// the block weight allows a few thousand.
const HEIGHT_READING_SWEEP_WORK: u64 = MAX_COVENANT_WORK_PER_BODY;

/// Clears the flag of a running background re-check when dropped, however the re-check ends
struct SweepRunning(Arc<AtomicBool>);

impl Drop for SweepRunning {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// The outcome of re-checking transactions whose input scripts or covenants read the height (see [height_check])
#[derive(Default)]
struct HeightChecks {
    passed: Vec<TransactionKey>,
    failed: Vec<TransactionKey>,
    not_yet_valid: Vec<TransactionKey>,
}

/// A snapshot of the last block the mempool has processed. Broadcast over a watch channel so consumers can wait for
/// the mempool to reach a given tip - and detect when the mempool has moved *past* it - without busy-polling.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MempoolLastSeen {
    pub hash: FixedHash,
    pub height: u64,
}

/// The Mempool consists of an Unconfirmed Transaction Pool, Pending Pool, Orphan Pool and Reorg Pool and is responsible
/// for managing and maintaining all unconfirmed transactions that have not yet been included in a block, and
/// transactions that have recently been included in a block.
#[derive(Clone)]
pub struct Mempool {
    pool_storage: Arc<RwLock<MempoolStorage>>,
    // Broadcasts the last-seen block (hash + height) every time the mempool processes a block (or reorg). Consumers
    // that need to wait for the mempool to catch up to a given tip can subscribe instead of busy-polling.
    last_seen_tx: watch::Sender<MempoolLastSeen>,
    // Bounds how many new transactions are validated at the same time. Validation is done without holding the storage
    // lock, so without this every inbound transaction would get its own blocking thread, starving block validation of
    // CPU and holding a database read transaction each.
    validation_permits: Arc<Semaphore>,
    // A separate bound for transactions fetched to reconstruct a compact block (`insert_all`), so that block
    // reconstruction never waits behind a flood of inbound transactions. That work is triggered by block propagation
    // and bounded by the block size, but still bounded here, since several peers can announce blocks at once.
    reconciliation_permits: Arc<Semaphore>,
    // Set while a background re-check of height-reading transactions runs (see `Mempool::sweep_height_reading`)
    sweep_running: Arc<AtomicBool>,
    // Held by a block template request (see `Mempool::retrieve`), so that concurrent requests for the same tip do not
    // re-check the same transactions: a request waiting for it sees the results of the one before. Never taken while
    // holding the storage lock.
    template_checks: Arc<tokio::sync::Mutex<()>>,
    // The number of height-reading transactions block template requests re-checked
    #[cfg(test)]
    height_checks_run: Arc<std::sync::atomic::AtomicUsize>,
}

/// The maximum number of compact block reconciliations (`Mempool::insert_all`) validated concurrently
const MAX_CONCURRENT_RECONCILIATIONS: usize = 2;

async fn acquire_permit(semaphore: &Arc<Semaphore>) -> Result<OwnedSemaphorePermit, MempoolError> {
    semaphore
        .clone()
        .acquire_owned()
        .await
        .map_err(|e| MempoolError::InternalError(format!("Mempool validation semaphore closed: {e}")))
}

/// A permit to decode and validate one inbound transaction, obtained from [Mempool::acquire_validation_permit] and
/// consumed by [Mempool::insert_with_permit]. It draws from the same bound as [Mempool::insert], so that decoding and
/// validation of a transaction are covered by a single permit. Dropping it releases the permit.
#[must_use]
pub struct ValidationPermit {
    _permit: OwnedSemaphorePermit,
}

/// The maximum number of new transactions that are validated concurrently: half the available cores, at least 1 and at
/// most 8.
fn max_concurrent_validations() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get() / 2)
        .unwrap_or(1)
        .clamp(1, 8)
}

impl Mempool {
    /// Create a new Mempool with an UnconfirmedPool and ReOrgPool.
    pub fn new(
        config: MempoolConfig,
        rules: BaseNodeConsensusManager,
        validator: Box<dyn TransactionValidator>,
    ) -> Self {
        let (last_seen_tx, _) = watch::channel(MempoolLastSeen::default());
        Self {
            pool_storage: Arc::new(RwLock::new(MempoolStorage::new(config, rules, validator))),
            last_seen_tx,
            validation_permits: Arc::new(Semaphore::new(max_concurrent_validations())),
            reconciliation_permits: Arc::new(Semaphore::new(MAX_CONCURRENT_RECONCILIATIONS)),
            sweep_running: Arc::new(AtomicBool::new(false)),
            template_checks: Arc::new(tokio::sync::Mutex::new(())),
            #[cfg(test)]
            height_checks_run: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }

    /// Subscribe to changes of the mempool's last-seen block. The returned receiver yields the most recent block
    /// (hash + height) the mempool has processed; its initial value is the state at the time of subscription.
    pub fn subscribe_last_seen(&self) -> watch::Receiver<MempoolLastSeen> {
        self.last_seen_tx.subscribe()
    }

    /// Acquire a permit from the bound on concurrent transaction validations. Used to cover the decoding of an inbound
    /// transaction as well as its validation (see [Mempool::insert_with_permit]).
    pub async fn acquire_validation_permit(&self) -> Result<ValidationPermit, MempoolError> {
        Ok(ValidationPermit {
            _permit: acquire_permit(&self.validation_permits).await?,
        })
    }

    /// Insert an unconfirmed transaction into the Mempool.
    ///
    /// The near-free checks (fee, weight, already in the pool) are done first, under a brief read lock. The transaction
    /// is then validated without holding the mempool write lock (see [`MempoolStorage::insert_unlocked`]), with at
    /// most a bounded number of transactions being validated at once.
    pub async fn insert(&self, tx: Arc<Transaction>) -> Result<TxStorageResponse, MempoolError> {
        let (response, _) = self.insert_with_ban_reason(tx, None).await?;
        Ok(response)
    }

    /// Identical to [Mempool::insert], except that validation uses the given, already acquired, permit instead of
    /// acquiring a new one. The permit is released once validation is done (or the pre-checks return early).
    pub async fn insert_with_permit(
        &self,
        tx: Arc<Transaction>,
        permit: ValidationPermit,
    ) -> Result<TxStorageResponse, MempoolError> {
        let (response, _) = self.insert_with_ban_reason(tx, Some(permit)).await?;
        Ok(response)
    }

    /// As [Mempool::insert] (or [Mempool::insert_with_permit] if a permit is given), also returning a ban reason if
    /// the transaction failed a final internal consistency check.
    pub async fn insert_with_ban_reason(
        &self,
        tx: Arc<Transaction>,
        permit: Option<ValidationPermit>,
    ) -> Result<(TxStorageResponse, Option<BanReason>), MempoolError> {
        let tx_clone = tx.clone();
        if let Some(response) = self
            .with_read_access(move |storage| Ok(storage.pre_check(&tx_clone)))
            .await?
        {
            return Ok((response, None));
        }
        let permit = match permit {
            Some(permit) => permit,
            None => self.acquire_validation_permit().await?,
        };
        let storage = self.pool_storage.clone();
        task::spawn_blocking(move || {
            // The permit is held by the blocking task, so that it is only released once validation is done, even if
            // the caller stops waiting for it
            let _permit = permit;
            MempoolStorage::insert_unlocked_with_ban_reason(&storage, tx)
        })
        .await?
    }

    /// Inserts all transactions into the mempool, to reconstruct a compact block. Each transaction is validated
    /// without holding the mempool write lock (see [`MempoolStorage::insert_unlocked`]). This has its own bound on
    /// concurrency, separate from [`Mempool::insert`], so that it does not wait behind inbound transactions.
    pub async fn insert_all(&self, transactions: Vec<Arc<Transaction>>) -> Result<(), MempoolError> {
        let permit = acquire_permit(&self.reconciliation_permits).await?;
        let storage = self.pool_storage.clone();
        task::spawn_blocking(move || {
            let _permit = permit;
            for tx in transactions {
                MempoolStorage::insert_unlocked(&storage, tx)?;
            }
            Ok(())
        })
        .await?
    }

    /// The number of validation permits currently available
    #[cfg(test)]
    pub(crate) fn available_validation_permits(&self) -> usize {
        self.validation_permits.available_permits()
    }

    /// The number of reconciliation permits currently available
    #[cfg(test)]
    pub(crate) fn available_reconciliation_permits(&self) -> usize {
        self.reconciliation_permits.available_permits()
    }

    /// Update the Mempool based on the received published block.
    pub async fn process_published_block(&self, published_block: Arc<Block>) -> Result<(), MempoolError> {
        let last_seen = self
            .with_write_access(move |storage| {
                storage.process_published_block(&published_block)?;
                Ok(MempoolLastSeen {
                    hash: storage.last_seen_hash,
                    height: storage.last_seen_height,
                })
            })
            .await?;
        // Notify any subscribers (e.g. new-block-template requests) that the mempool has advanced.
        self.last_seen_tx.send_replace(last_seen);
        self.sweep_height_reading(last_seen.height.saturating_add(1), last_seen.hash)
            .await
    }

    /// Re-checks the next batch of pool transactions with an input script or covenant that reads the height (see
    /// [HEIGHT_READING_SWEEP_BATCH] and [HEIGHT_READING_SWEEP_WORK]) for the block at `height` on top of `prev_hash`,
    /// and drops the ones that are not valid there with their descendants, so that they do not stay in the pool of a
    /// node that never builds block templates. The scripts run without holding the storage lock. If the re-check for
    /// an earlier block is still running, this one is skipped: the next one continues where the running one stops.
    async fn sweep_height_reading(&self, height: u64, prev_hash: HashOutput) -> Result<(), MempoolError> {
        if self.sweep_running.swap(true, Ordering::SeqCst) {
            debug!(
                target: LOG_TARGET,
                "A re-check of height-reading transactions is still running, skipping the one for height {height}"
            );
            return Ok(());
        }
        let _running = SweepRunning(self.sweep_running.clone());
        let (batch, next_cursor, generation) = self
            .with_read_access(move |storage| {
                let (batch, next_cursor) = storage.unconfirmed_pool.height_reading_sweep_batch(
                    HEIGHT_READING_SWEEP_BATCH,
                    HEIGHT_READING_SWEEP_WORK,
                    height,
                    &prev_hash,
                );
                Ok((batch, next_cursor, storage.chain_generation))
            })
            .await?;
        let checks = run_height_checks(batch, height, prev_hash).await?;
        let dropped = checks.failed.len();
        // Nothing is recorded, and the cursor stays, if the mempool processed a chain change in the meantime
        let removed = self
            .with_write_access(move |storage| {
                let removed = storage.record_height_checks(generation, &checks.passed, &checks.failed, prev_hash)?;
                if removed.is_some() {
                    storage.unconfirmed_pool.set_sweep_cursor(next_cursor);
                }
                Ok(removed)
            })
            .await?;
        if let Some(removed) = removed &&
            dropped > 0
        {
            info!(
                target: LOG_TARGET,
                "Dropped {dropped} transaction(s) that are not valid at height {height}, with {} descendant(s)",
                removed.saturating_sub(dropped)
            );
        }
        Ok(())
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
        let last_seen = self
            .with_write_access(move |storage| {
                storage.process_reorg(&removed_blocks, &new_blocks)?;
                Ok(MempoolLastSeen {
                    hash: storage.last_seen_hash,
                    height: storage.last_seen_height,
                })
            })
            .await?;
        self.last_seen_tx.send_replace(last_seen);
        Ok(())
    }

    /// After a sync event, we can move all orphan transactions to the unconfirmed pool after validation
    pub async fn process_sync(&self) -> Result<(), MempoolError> {
        self.with_write_access(move |storage| storage.process_sync()).await
    }

    /// Returns all unconfirmed transaction stored in the Mempool, except the transactions stored in the ReOrgPool.
    pub async fn snapshot(&self) -> Result<Vec<Arc<Transaction>>, MempoolError> {
        self.with_read_access(|storage| Ok(storage.snapshot())).await
    }

    /// Returns a list of transaction ranked by transaction priority up to a given weight, for a block template at
    /// `height` on top of the block `prev_hash`. Only transactions that fit into a block will be returned, and none
    /// with an input that is not mature at `height` or a kernel locked above it.
    ///
    /// A selected transaction with an input script or covenant that reads the block height is re-checked at the
    /// template's height (see `height_check`), without holding the storage lock. The ones that fail are dropped from
    /// the pool with their descendants, and the template is selected again. One that fails only below the height it
    /// was validated for (the template is built on an older block) is left out of the template but kept. The last of
    /// at most [MAX_TEMPLATE_SELECTION_PASSES] selections leaves out every such transaction that has not passed for
    /// this block, so it checks nothing. A transaction that passed is not checked again for a block on top of the same
    /// one, so the work per tip is bounded by the height-reading transactions that get selected, each failing one being
    /// dropped once; concurrent requests run one after the other, so they do not repeat each other's checks. A failure
    /// is only dropped from the pool if the mempool processed no chain change during the check and `prev_hash` is the
    /// block it last processed; otherwise it is only left out of this template.
    pub async fn retrieve(
        &self,
        total_weight: u64,
        height: u64,
        prev_hash: HashOutput,
    ) -> Result<Vec<Arc<Transaction>>, MempoolError> {
        let _checks = self.template_checks.lock().await;
        let mut filter = TemplateFilter {
            height,
            excluded: HashSet::new(),
            only_checked_at: None,
        };
        let mut dropped = 0usize;
        let mut removed = 0usize;
        let mut passes = 0usize;
        loop {
            passes = passes.saturating_add(1);
            let last_pass = passes >= MAX_TEMPLATE_SELECTION_PASSES;
            if last_pass {
                filter.only_checked_at = Some(prev_hash);
            }
            let start = std::time::Instant::now();
            let pass_filter = filter.clone();
            let (retrieved, to_check, generation) = self
                .with_read_access(move |storage| {
                    let retrieved = storage.retrieve(total_weight, &pass_filter)?;
                    let to_check = storage
                        .unconfirmed_pool
                        .height_reading_to_check(&retrieved.retrieved_transactions, &prev_hash);
                    Ok((retrieved, to_check, storage.chain_generation))
                })
                .await?;
            debug!(
                target: LOG_TARGET,
                "Retrieved {} highest priority transaction(s) from the mempool in {:.0?} ms",
                retrieved.retrieved_transactions.len(),
                start.elapsed()
            );

            // Nothing is left to check on the last pass
            #[cfg(test)]
            self.height_checks_run.fetch_add(to_check.len(), Ordering::SeqCst);
            let checks = run_height_checks(to_check, height, prev_hash).await?;
            let failed = checks.failed.len();
            let not_yet_valid = checks.not_yet_valid.len();
            let transactions_to_remove_and_insert = retrieved.transactions_to_remove_and_insert.clone();
            if !checks.passed.is_empty() || failed > 0 || !transactions_to_remove_and_insert.is_empty() {
                debug!(
                    target: LOG_TARGET,
                    "Removing {failed} transaction(s) that failed a height re-check and {} that need re-evaluation \
                     from the unconfirmed pool",
                    transactions_to_remove_and_insert.len()
                );
                let (passed, failed_keys) = (checks.passed, checks.failed.clone());
                let recorded = self
                    .with_write_access(move |storage| {
                        let removed = storage.record_height_checks(generation, &passed, &failed_keys, prev_hash)?;
                        storage.remove_and_reinsert_transactions(transactions_to_remove_and_insert)?;
                        Ok(removed)
                    })
                    .await?;
                match recorded {
                    Some(count) => {
                        removed = removed.saturating_add(count);
                        dropped = dropped.saturating_add(failed);
                    },
                    // The chain changed, or the template is not on top of the mempool's tip: the failures are only
                    // left out of this template
                    None => filter.excluded.extend(checks.failed),
                }
            }
            filter.excluded.extend(checks.not_yet_valid);

            if last_pass || (failed == 0 && not_yet_valid == 0) {
                if dropped > 0 || !filter.excluded.is_empty() {
                    info!(
                        target: LOG_TARGET,
                        "Block template at height {height}: dropped {dropped} transaction(s) that failed a height \
                         re-check, with {} descendant(s), and left out {} not valid at this height, in {passes} \
                         selection pass(es){}",
                        removed.saturating_sub(dropped),
                        filter.excluded.len(),
                        if last_pass {
                            ", the last leaving out unchecked height-reading transactions"
                        } else {
                            ""
                        }
                    );
                }
                return Ok(retrieved.retrieved_transactions);
            }
        }
    }

    pub async fn retrieve_by_excess_sigs(
        &self,
        excess_sigs: Vec<PrivateKey>,
    ) -> Result<(Vec<Arc<Transaction>>, Vec<PrivateKey>), MempoolError> {
        self.with_read_access(move |storage| storage.retrieve_by_excess_sigs(&excess_sigs))
            .await
    }

    /// Returns the subset of provided output hashes that exist in the mempool's unconfirmed pool.
    pub async fn filter_outputs_in_mempool(
        &self,
        output_hashes: Vec<HashOutput>,
    ) -> Result<Vec<HashOutput>, MempoolError> {
        self.with_read_access(move |storage| Ok(storage.filter_outputs_in_mempool(&output_hashes)))
            .await
    }

    /// Check if the specified excess signature is found in the Mempool.
    pub async fn has_tx_with_excess_sig(
        &self,
        excess_sig: CompressedSignature,
    ) -> Result<TxStorageResponse, MempoolError> {
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

    pub async fn get_last_seen_hash(&self) -> Result<FixedHash, MempoolError> {
        self.with_read_access(|storage| Ok(storage.last_seen_hash)).await
    }
}

/// Re-checks `to_check` for the block at `height` on top of `prev_hash` (see [height_check]), without holding the
/// storage lock
async fn run_height_checks(
    to_check: Vec<(TransactionKey, Arc<Transaction>, HeightValidation)>,
    height: u64,
    prev_hash: HashOutput,
) -> Result<HeightChecks, MempoolError> {
    if to_check.is_empty() {
        return Ok(HeightChecks::default());
    }
    let checks = task::spawn_blocking(move || {
        let mut checks = HeightChecks::default();
        for (key, tx, validation) in to_check {
            match height_check(&tx, &validation, height, &prev_hash) {
                HeightCheck::Passed => checks.passed.push(key),
                HeightCheck::Failed(reason) => {
                    debug!(
                        target: LOG_TARGET,
                        "Tx: ({}) is not valid at height {height}: {reason}, dropping",
                        tx_id(&tx)
                    );
                    checks.failed.push(key);
                },
                HeightCheck::NotYetValid(reason) => {
                    debug!(
                        target: LOG_TARGET,
                        "Tx: ({}) is not valid at height {height}, below the height it was validated for: {reason}",
                        tx_id(&tx)
                    );
                    checks.not_yet_valid.push(key);
                },
            }
        }
        checks
    })
    .await?;
    Ok(checks)
}

fn tx_id(tx: &Transaction) -> String {
    tx.first_kernel_excess_sig()
        .map(|sig| sig.get_signature().to_hex())
        .unwrap_or_default()
}

#[cfg(test)]
mod test {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use tari_transaction_components::{MicroMinotari, key_manager::KeyManager, tx};

    use super::*;
    use crate::{test_helpers::create_consensus_rules, validation::ValidationError};

    /// A validator that takes a while to validate, and records the maximum number of concurrent validations
    #[derive(Default)]
    struct SlowValidator {
        current: AtomicUsize,
        max: AtomicUsize,
    }

    impl TransactionValidator for Arc<SlowValidator> {
        fn validate_full(&self, _tx: &Transaction) -> Result<(), ValidationError> {
            let current = self.current.fetch_add(1, Ordering::SeqCst).saturating_add(1);
            self.max.fetch_max(current, Ordering::SeqCst);
            std::thread::sleep(std::time::Duration::from_millis(50));
            self.current.fetch_sub(1, Ordering::SeqCst);
            Ok(())
        }

        fn validate_chain_linked(&self, tx: &Transaction) -> Result<(), ValidationError> {
            self.validate_full(tx)
        }

        fn validate_internal_consistency(
            &self,
            _tx: &Transaction,
            _tip: Option<&tari_common_types::chain_metadata::ChainMetadata>,
        ) -> Result<(), ValidationError> {
            Ok(())
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_inserts_are_bounded() {
        let key_manager = KeyManager::new_random().unwrap();
        let validator = Arc::new(SlowValidator::default());
        let mempool = create_mempool(validator.clone());
        let permits = max_concurrent_validations();

        let num_txs = permits * 4 + 2;
        // Create the transactions up front, so that the inserts really are concurrent
        let txs = (0..num_txs).map(|_| create_tx(&key_manager)).collect::<Vec<_>>();
        let mut tasks = Vec::with_capacity(num_txs);
        for tx in txs {
            let mempool = mempool.clone();
            tasks.push(tokio::spawn(async move { mempool.insert(tx).await.map(|_| ()) }));
        }
        for task in tasks {
            task.await.unwrap().unwrap();
        }

        assert!(validator.max.load(Ordering::SeqCst) >= 1);
        assert!(
            validator.max.load(Ordering::SeqCst) <= permits,
            "{} concurrent validations with {permits} permits",
            validator.max.load(Ordering::SeqCst)
        );
        assert_eq!(mempool.stats().await.unwrap().unconfirmed_txs, num_txs as u64);
    }

    /// A validator that blocks until it is released
    #[derive(Default)]
    struct GatedValidator {
        entered: AtomicUsize,
        released: std::sync::Mutex<bool>,
        condvar: std::sync::Condvar,
    }

    impl GatedValidator {
        fn release(&self) {
            *self.released.lock().unwrap() = true;
            self.condvar.notify_all();
        }
    }

    impl TransactionValidator for Arc<GatedValidator> {
        fn validate_full(&self, _tx: &Transaction) -> Result<(), ValidationError> {
            self.entered.fetch_add(1, Ordering::SeqCst);
            let mut released = self.released.lock().unwrap();
            while !*released {
                released = self.condvar.wait(released).unwrap();
            }
            Ok(())
        }

        fn validate_chain_linked(&self, tx: &Transaction) -> Result<(), ValidationError> {
            self.validate_full(tx)
        }

        fn validate_internal_consistency(
            &self,
            _tx: &Transaction,
            _tip: Option<&tari_common_types::chain_metadata::ChainMetadata>,
        ) -> Result<(), ValidationError> {
            Ok(())
        }
    }

    fn create_mempool<V: TransactionValidator + 'static>(validator: V) -> Mempool {
        let mut config = MempoolConfig::default();
        config.unconfirmed_pool.min_fee = 0;
        Mempool::new(config, create_consensus_rules(), Box::new(validator))
    }

    fn create_tx(key_manager: &KeyManager) -> Arc<Transaction> {
        Arc::new(
            tx!(MicroMinotari(100_000), fee: MicroMinotari(5), inputs: 1, outputs: 1, key_manager)
                .expect("Failed to get tx")
                .0,
        )
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn insert_all_does_not_wait_for_inbound_permits() {
        let key_manager = KeyManager::new_random().unwrap();
        let mempool = create_mempool(Arc::new(SlowValidator::default()));
        // Every inbound validation permit is taken
        let _held = mempool
            .validation_permits
            .clone()
            .acquire_many_owned(u32::try_from(max_concurrent_validations()).unwrap())
            .await
            .unwrap();
        assert_eq!(mempool.validation_permits.available_permits(), 0);

        let txs = vec![create_tx(&key_manager), create_tx(&key_manager)];
        tokio::time::timeout(std::time::Duration::from_secs(10), mempool.insert_all(txs))
            .await
            .expect("insert_all waited for an inbound permit")
            .unwrap();
        assert_eq!(mempool.stats().await.unwrap().unconfirmed_txs, 2);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn permit_is_held_until_validation_finishes() {
        let key_manager = KeyManager::new_random().unwrap();
        let validator = Arc::new(GatedValidator::default());
        let mempool = create_mempool(validator.clone());
        let permits = max_concurrent_validations();
        // Release the blocked validation even if an assertion fails, so that the runtime can shut down
        struct ReleaseOnDrop(Arc<GatedValidator>);
        impl Drop for ReleaseOnDrop {
            fn drop(&mut self) {
                self.0.release();
            }
        }
        let _release = ReleaseOnDrop(validator.clone());

        let task = tokio::spawn({
            let mempool = mempool.clone();
            let tx = create_tx(&key_manager);
            async move { mempool.insert(tx).await }
        });
        while validator.entered.load(Ordering::SeqCst) == 0 {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        assert_eq!(mempool.validation_permits.available_permits(), permits - 1);

        // The caller stops waiting, but validation is still running, so the permit is still held
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(mempool.validation_permits.available_permits(), permits - 1);

        // It is released once validation finishes
        validator.release();
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            while mempool.validation_permits.available_permits() != permits {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("permit was not released");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn insert_with_permit_uses_the_given_permit() {
        let key_manager = KeyManager::new_random().unwrap();
        let mempool = create_mempool(Arc::new(SlowValidator::default()));
        let permits = max_concurrent_validations();
        let permit = mempool.acquire_validation_permit().await.unwrap();
        // Every other validation permit is taken, so acquiring another one would never complete
        let held = mempool
            .validation_permits
            .clone()
            .acquire_many_owned(u32::try_from(permits - 1).unwrap())
            .await
            .unwrap();
        assert_eq!(mempool.available_validation_permits(), 0);

        let tx = create_tx(&key_manager);
        let response = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            mempool.insert_with_permit(tx.clone(), permit),
        )
        .await
        .expect("insert_with_permit acquired a second permit")
        .unwrap();
        assert_eq!(response, TxStorageResponse::UnconfirmedPool);
        // The given permit was released once validation finished
        assert_eq!(mempool.available_validation_permits(), 1);

        // Inserting the same transaction again also releases the permit
        let permit = mempool.acquire_validation_permit().await.unwrap();
        let _response = mempool.insert_with_permit(tx, permit).await.unwrap();
        assert_eq!(mempool.available_validation_permits(), 1);

        drop(held);
        assert_eq!(mempool.available_validation_permits(), permits);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn transactions_larger_than_the_template_byte_budget_are_rejected() {
        use tari_transaction_components::helpers::borsh::SerializedSize;

        use crate::mempool::unconfirmed_pool::MAX_BLOCK_TEMPLATE_BODY_BYTES;

        let key_manager = KeyManager::new_random().unwrap();
        let mempool = create_mempool(Arc::new(SlowValidator::default()));
        let template = create_tx(&key_manager);
        let input = template.body.inputs().first().unwrap().clone();
        let input_size = input.get_serialized_size().unwrap();
        // Enough inputs (8 grams each) to exceed the byte budget while staying under the maximum transaction weight
        let num_inputs = MAX_BLOCK_TEMPLATE_BODY_BYTES / input_size + 1;

        for _ in 0..21 {
            let kernels = create_tx(&key_manager).body.kernels().clone();
            let big = Arc::new(Transaction::new(
                vec![input.clone(); num_inputs],
                template.body.outputs().clone(),
                kernels,
                Default::default(),
                Default::default(),
            ));
            assert!(big.body.get_serialized_size().unwrap() > MAX_BLOCK_TEMPLATE_BODY_BYTES);
            let response = mempool.insert(big).await.unwrap();
            assert!(
                matches!(&response, TxStorageResponse::NotStored(Some(reason)) if reason.contains("block template budget")),
                "{response:?}"
            );
        }
        assert_eq!(mempool.stats().await.unwrap().unconfirmed_txs, 0);

        // Normal transactions are accepted and fill the template
        let normal = (0..5).map(|_| create_tx(&key_manager)).collect::<Vec<_>>();
        for tx in &normal {
            assert_eq!(
                mempool.insert(tx.clone()).await.unwrap(),
                TxStorageResponse::UnconfirmedPool
            );
        }
        let retrieved = mempool.retrieve(u64::MAX, 1, FixedHash::zero()).await.unwrap();
        assert_eq!(retrieved.len(), normal.len());
        for tx in &normal {
            assert!(retrieved.contains(tx));
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_transaction_repeating_a_kernel_excess_is_rejected() {
        let key_manager = KeyManager::new_random().unwrap();
        let mempool = create_mempool(Arc::new(SlowValidator::default()));
        let tx = create_tx(&key_manager);
        let other = create_tx(&key_manager);
        // The same kernel excess twice, with different signatures
        let kernel = tx.body.kernels().first().unwrap().clone();
        let mut again = kernel.clone();
        again.excess_sig = other.body.kernels().first().unwrap().excess_sig.clone();
        let repeating = Arc::new(Transaction::new(
            tx.body.inputs().clone(),
            tx.body.outputs().clone(),
            vec![kernel, again],
            Default::default(),
            Default::default(),
        ));
        let response = mempool.insert(repeating).await.unwrap();
        assert!(
            matches!(&response, TxStorageResponse::NotStored(Some(reason)) if reason.contains("kernel excess")),
            "{response:?}"
        );
        assert_eq!(mempool.stats().await.unwrap().unconfirmed_txs, 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_background_re_check_is_skipped_while_one_runs() {
        use tari_script::script;
        use tari_transaction_components::{transaction_components::SpentOutput, weight::TransactionWeight};

        let key_manager = KeyManager::new_random().unwrap();
        let mempool = create_mempool(Arc::new(SlowValidator::default()));
        // Its input script only passes from height 100 on
        let tx = create_tx(&key_manager);
        let mut inputs = tx.body.inputs().clone();
        match &mut inputs.first_mut().unwrap().spent_output {
            SpentOutput::OutputData { script, .. } => *script = script!(CheckHeightVerify(100)).unwrap(),
            SpentOutput::OutputHash(_) => panic!("Expected a full input"),
        }
        let tx = Arc::new(Transaction::new(
            inputs,
            tx.body.outputs().clone(),
            tx.body.kernels().clone(),
            tx.offset.clone(),
            tx.script_offset.clone(),
        ));
        mempool
            .pool_storage
            .write()
            .unwrap()
            .unconfirmed_pool
            .insert(tx.clone(), None, &TransactionWeight::latest())
            .unwrap();
        let excess_sig = tx.first_kernel_excess_sig().unwrap().clone();

        // While a re-check runs, another one does nothing
        mempool.sweep_running.store(true, Ordering::SeqCst);
        mempool.sweep_height_reading(2, FixedHash::zero()).await.unwrap();
        assert_eq!(
            mempool.has_tx_with_excess_sig(excess_sig.clone()).await.unwrap(),
            TxStorageResponse::UnconfirmedPool
        );

        // Otherwise it drops the transaction, and is no longer running when it returns
        mempool.sweep_running.store(false, Ordering::SeqCst);
        mempool.sweep_height_reading(2, FixedHash::zero()).await.unwrap();
        assert_eq!(
            mempool.has_tx_with_excess_sig(excess_sig).await.unwrap(),
            TxStorageResponse::NotStored(None)
        );
        assert!(!mempool.sweep_running.load(Ordering::SeqCst));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_template_requests_do_not_repeat_height_checks() {
        use tari_script::script;
        use tari_transaction_components::{
            transaction_components::SpentOutput,
            validation::aggregate_body::run_height_reading_scripts,
            weight::TransactionWeight,
        };

        let key_manager = KeyManager::new_random().unwrap();
        let mempool = create_mempool(Arc::new(SlowValidator::default()));
        // Its input script passes from height 1 on
        let tx = create_tx(&key_manager);
        let mut inputs = tx.body.inputs().clone();
        match &mut inputs.first_mut().unwrap().spent_output {
            SpentOutput::OutputData { script, .. } => *script = script!(CheckHeightVerify(1) Nop).unwrap(),
            SpentOutput::OutputHash(_) => panic!("Expected a full input"),
        }
        let tx = Arc::new(Transaction::new(
            inputs,
            tx.body.outputs().clone(),
            tx.body.kernels().clone(),
            tx.offset.clone(),
            tx.script_offset.clone(),
        ));
        let tip = FixedHash::zero();
        let validation = HeightValidation {
            height: 2,
            script_keys: run_height_reading_scripts(&tx.body, 2, &tip).unwrap(),
        };
        mempool
            .pool_storage
            .write()
            .unwrap()
            .unconfirmed_pool
            .insert_with_height_validation(tx.clone(), None, &TransactionWeight::latest(), validation)
            .unwrap();

        let (first, second) = tokio::join!(mempool.retrieve(u64::MAX, 2, tip), mempool.retrieve(u64::MAX, 2, tip));
        assert_eq!(first.unwrap(), vec![tx.clone()]);
        assert_eq!(second.unwrap(), vec![tx]);
        assert_eq!(mempool.height_checks_run.load(Ordering::SeqCst), 1);
    }
}
