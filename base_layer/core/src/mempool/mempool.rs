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
use tari_common_types::types::{CompressedSignature, FixedHash, HashOutput, PrivateKey};
use tari_node_components::blocks::Block;
use tari_transaction_components::{rpc::models::FeePerGramStat, transaction_components::Transaction};
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
        mempool_storage::MempoolStorage,
    },
    validation::TransactionValidator,
};

pub const LOG_TARGET: &str = "c::mp::mempool";

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

/// A permit to decode one inbound block message, obtained from [Mempool::acquire_reconciliation_permit]. It draws from
/// the same bound as [Mempool::insert_all], and must only cover CPU-bound work: drop it before any network round trip
/// or block processing. Dropping it releases the permit.
#[must_use]
pub struct ReconciliationPermit {
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

    /// Acquire a permit from the bound on concurrent compact block reconciliations, to cover decoding an inbound block
    /// message. Hold it only for the decode: reconciliation acquires its own permit around the CPU-bound validation of
    /// fetched transactions ([Mempool::insert_all]), and never holds one across a network round trip.
    pub async fn acquire_reconciliation_permit(&self) -> Result<ReconciliationPermit, MempoolError> {
        Ok(ReconciliationPermit {
            _permit: acquire_permit(&self.reconciliation_permits).await?,
        })
    }

    /// Insert an unconfirmed transaction into the Mempool.
    ///
    /// The near-free checks (fee, weight, already in the pool) are done first, under a brief read lock. The transaction
    /// is then validated without holding the mempool write lock (see [`MempoolStorage::insert_unlocked`]), with at
    /// most a bounded number of transactions being validated at once.
    pub async fn insert(&self, tx: Arc<Transaction>) -> Result<TxStorageResponse, MempoolError> {
        self.insert_inner(tx, None).await
    }

    /// Identical to [Mempool::insert], except that validation uses the given, already acquired, permit instead of
    /// acquiring a new one. The permit is released once validation is done (or the pre-checks return early).
    pub async fn insert_with_permit(
        &self,
        tx: Arc<Transaction>,
        permit: ValidationPermit,
    ) -> Result<TxStorageResponse, MempoolError> {
        self.insert_inner(tx, Some(permit)).await
    }

    async fn insert_inner(
        &self,
        tx: Arc<Transaction>,
        permit: Option<ValidationPermit>,
    ) -> Result<TxStorageResponse, MempoolError> {
        let tx_clone = tx.clone();
        if let Some(response) = self
            .with_read_access(move |storage| Ok(storage.pre_check(&tx_clone)))
            .await?
        {
            return Ok(response);
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
            MempoolStorage::insert_unlocked(&storage, tx)
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

    #[tokio::test]
    async fn reconciliation_permit_is_released_on_drop() {
        let mempool = create_mempool(Arc::new(SlowValidator::default()));
        let permit = mempool.acquire_reconciliation_permit().await.unwrap();
        assert_eq!(
            mempool.available_reconciliation_permits(),
            MAX_CONCURRENT_RECONCILIATIONS - 1
        );
        drop(permit);
        assert_eq!(
            mempool.available_reconciliation_permits(),
            MAX_CONCURRENT_RECONCILIATIONS
        );
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
        let retrieved = mempool.retrieve(u64::MAX).await.unwrap();
        assert_eq!(retrieved.len(), normal.len());
        for tx in &normal {
            assert!(retrieved.contains(tx));
        }
    }
}
