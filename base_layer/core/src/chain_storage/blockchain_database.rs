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
    cmp,
    cmp::{Ordering, max, min},
    collections::{HashMap, VecDeque},
    convert::TryFrom,
    mem,
    ops::{Bound, RangeBounds},
    sync::{
        Arc,
        RwLock,
        RwLockReadGuard,
        RwLockWriteGuard,
        atomic::{self, AtomicBool},
    },
    time::{Duration, Instant},
};

use blake2::Blake2b;
use digest::consts::U32;
use jmt::{
    JellyfishMerkleTree,
    KeyHash,
    OwnedValue,
    Version,
    storage::{LeafNode, Node, NodeKey, TreeReader},
};
use log::*;
use primitive_types::U512;
use serde::{Deserialize, Serialize};
use tari_common_types::{
    chain_metadata::ChainMetadata,
    epoch::VnEpoch,
    types::{
        BadBlock,
        BlockHash,
        CompressedCommitment,
        CompressedPublicKey,
        CompressedSignature,
        FixedHash,
        HashOutput,
        UncompressedCommitment,
    },
};
use tari_hashing::TransactionHashDomain;
use tari_mmr::{MerkleProof, pruned_hashset::PrunedHashSet};
use tari_node_components::blocks::{
    Block,
    BlockHeader,
    BlockHeaderAccumulatedData,
    BlockHeaderValidationError,
    ChainBlock,
    ChainHeader,
    HistoricalBlock,
    NewBlockTemplate,
};
use tari_transaction_components::{
    BanPeriod,
    consensus::{ConsensusConstants, DomainSeparatedConsensusHasher},
    tari_proof_of_work::{Difficulty, PowAlgorithm},
    transaction_components::{TransactionInput, TransactionKernel, TransactionOutput},
};
use tari_utilities::{ByteArray, epoch_time::EpochTime, hex::Hex};

use super::{
    AccumulatedDataRebuildStatus,
    BlockchainCheckRequest,
    BurnCommitmentRebuildStatus,
    CheckFailure,
    MinedInfo,
    PayrefRebuildStatus,
    TemplateRegistrationEntry,
    ValidatorNodeRegistrationInfo,
    smt_hasher::SmtHasher,
};
use crate::{
    PrunedInputMmr,
    PrunedKernelMmr,
    PrunedOutputMmr,
    block_output_mr_hash_from_pruned_mmr,
    blocks::{
        BlockAccumulatedData,
        BlockHeaderAccumulatedDataBuilder,
        UpdateBlockAccumulatedData,
        genesis_block::VALIDATOR_MR_EMPTY_PLACEHOLDER_HASH,
    },
    chain_storage::{
        BlockAddResult,
        BlockchainBackend,
        DbBasicStats,
        DbTotalSizeStats,
        HorizonData,
        InputMinedInfo,
        MmrTree,
        Optional,
        OrNotFound,
        Reorg,
        TargetDifficulties,
        consts::{
            BACKGROUND_PRUNING_CHUNK_SIZE,
            BACKGROUND_PRUNING_THRESHOLD,
            BLOCKCHAIN_DATABASE_ORPHAN_STORAGE_CAPACITY,
            BLOCKCHAIN_DATABASE_PRUNED_MODE_PRUNING_INTERVAL,
            BLOCKCHAIN_DATABASE_PRUNING_HORIZON,
        },
        db_transaction::{DbKey, DbTransaction, DbValue, HorizonSyncOutputCheckpoint},
        error::ChainStorageError,
        kernel_merkle_proof::KernelMerkleProof,
        lmdb_db::{BREATHING_TIME_MS_MAX, BREATHING_TIME_MS_MIN, BlockchainCheckStatus},
        smt_hasher::ValidatorNodeJmtHasher,
        utxo_mined_info::OutputMinedInfo,
    },
    common::rolling_vec::RollingVec,
    consensus::{BaseNodeConsensusManager, chain_strength_comparer::ChainStrengthComparer},
    input_mr_hash_from_pruned_mmr,
    kernel_mr_hash_from_pruned_mmr,
    proof_of_work::{
        AchievedTargetDifficulty,
        MAX_BACKOFF_RUN_LOOKBACK,
        PowBackoffTracker,
        TargetDifficultyWindow,
        adjust_target,
        randomx_factory::RandomXFactory,
    },
    validation::{
        CandidateBlockValidator,
        DifficultyCalculator,
        HeaderChainContext,
        HeaderChainLinkedValidator,
        InternalConsistencyValidator,
        ValidationError,
        header::HeaderFullValidator,
        helpers::calc_median_timestamp,
        tari_rx_vm_key_height,
    },
};

const LOG_TARGET: &str = "c::cs::database";

/// Pause between heights in the accumulated data rebuild below the GHSA-3qmx-q9pv-f3m4 activation height, where
/// the walk may be a hundred thousand heights long and none of it is urgent.
const REBUILD_THROTTLE_MS: u64 = 100;
/// Pause between heights at and above the activation height. Much shorter, because that suffix is the part that
/// has to be repaired before the node can be trusted to compare chain strength, and it is short in practice - but
/// not zero, because each strict height runs a RandomX-backed validation and takes the backend write lock.
const STRICT_REBUILD_THROTTLE_MS: u64 = 10;
/// Configuration for the BlockchainDatabase.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockchainDatabaseConfig {
    pub orphan_storage_capacity: usize,
    pub pruning_horizon: u64,
    pub pruning_interval: u64,
    pub track_reorgs: bool,
    pub cleanup_orphans_at_startup: bool,
    pub clear_bad_blocks_at_startup: bool,
}

impl Default for BlockchainDatabaseConfig {
    fn default() -> Self {
        Self {
            orphan_storage_capacity: BLOCKCHAIN_DATABASE_ORPHAN_STORAGE_CAPACITY,
            pruning_horizon: BLOCKCHAIN_DATABASE_PRUNING_HORIZON,
            pruning_interval: BLOCKCHAIN_DATABASE_PRUNED_MODE_PRUNING_INTERVAL,
            track_reorgs: false,
            cleanup_orphans_at_startup: true,
            clear_bad_blocks_at_startup: true,
        }
    }
}

/// A placeholder struct that contains the two validators that the database uses to decide whether or not a block is
/// eligible to be added to the database. The `block` validator should perform a full consensus check. The `orphan`
/// validator needs to check that the block is internally consistent, but can't know whether the PoW is sufficient,
/// for example.
/// The `GenesisBlockValidator` is used to check that the chain builds on the correct genesis block.
/// The `ChainTipValidator` is used to check that the accounting balance and MMR states of the chain state is valid.
pub struct Validators<B> {
    pub block: Arc<dyn CandidateBlockValidator<B>>,
    pub header: Arc<dyn HeaderChainLinkedValidator<B>>,
    pub orphan: Arc<dyn InternalConsistencyValidator>,
}

impl<B: BlockchainBackend> Validators<B> {
    pub fn new(
        block: impl CandidateBlockValidator<B> + 'static,
        header: impl HeaderChainLinkedValidator<B> + 'static,
        orphan: impl InternalConsistencyValidator + 'static,
    ) -> Self {
        Self {
            block: Arc::new(block),
            header: Arc::new(header),
            orphan: Arc::new(orphan),
        }
    }
}

impl<B> Clone for Validators<B> {
    fn clone(&self) -> Self {
        Validators {
            block: Arc::clone(&self.block),
            header: Arc::clone(&self.header),
            orphan: Arc::clone(&self.orphan),
        }
    }
}

// Private macro that pulls out all the boiler plate of extracting a DB query result from its variants
macro_rules! fetch {
    ($db:ident, $key_val:expr, $key_var:ident) => {{
        let key = DbKey::$key_var($key_val);
        match $db.fetch(&key) {
            Ok(None) => Err(key.to_value_not_found_error()),
            Ok(Some(DbValue::$key_var(k))) => Ok(*k),
            Ok(Some(other)) => unexpected_result(key, other),
            Err(e) => log_error(key, e),
        }
    }};
}

// Private macro that pulls out all the boiler plate of extracting a DB query result from its variants.
// Differs from `fetch` in that it will not error if not found, but instead returns an Option
macro_rules! try_fetch {
    ($db:ident, $key_val:expr, $key_var:ident) => {{
        let key = DbKey::$key_var($key_val);
        match $db.fetch(&key) {
            Ok(None) => Ok(None),
            Ok(Some(DbValue::$key_var(k))) => Ok(Some(*k)),
            Ok(Some(other)) => unexpected_result(key, other),
            Err(e) => log_error(key, e),
        }
    }};
}

/// A generic blockchain storage mechanism. This struct defines the API for storing and retrieving Tari blockchain
/// components without being opinionated about the actual backend used.
///
/// `BlockChainDatabase` is thread-safe, since the backend must implement `Sync` and `Send`.
///
/// You typically don't interact with `BlockChainDatabase` directly, since it doesn't enforce any consensus rules; it
/// only really stores and fetches blockchain components. To create an instance of `BlockchainDatabase', you must
/// provide it with the backend it is going to use; for example, for a memory-backed DB:
pub struct BlockchainDatabase<B> {
    db: Arc<RwLock<B>>,
    validators: Validators<B>,
    config: BlockchainDatabaseConfig,
    consensus_manager: BaseNodeConsensusManager,
    difficulty_calculator: Arc<DifficultyCalculator>,
    disable_add_block_flag: Arc<AtomicBool>,
    is_background_pruning: Arc<AtomicBool>,
}

#[allow(clippy::ptr_arg)]
impl<B> BlockchainDatabase<B>
where B: BlockchainBackend
{
    /// Creates a new `BlockchainDatabase` using the provided backend.
    pub fn new(
        db: B,
        consensus_manager: BaseNodeConsensusManager,
        validators: Validators<B>,
        config: BlockchainDatabaseConfig,
        difficulty_calculator: DifficultyCalculator,
    ) -> Result<Self, ChainStorageError> {
        trace!(target: LOG_TARGET, "BlockchainDatabase config: {config:?}");
        let blockchain_db = BlockchainDatabase {
            db: Arc::new(RwLock::new(db)),
            validators,
            config,
            consensus_manager,
            difficulty_calculator: Arc::new(difficulty_calculator),
            disable_add_block_flag: Arc::new(AtomicBool::new(false)),
            is_background_pruning: Arc::new(AtomicBool::new(false)),
        };
        Ok(blockchain_db)
    }

    pub fn start_new(
        db: B,
        consensus_manager: BaseNodeConsensusManager,
        validators: Validators<B>,
        config: BlockchainDatabaseConfig,
        difficulty_calculator: DifficultyCalculator,
    ) -> Result<Self, ChainStorageError> {
        let blockchain_db = BlockchainDatabase {
            db: Arc::new(RwLock::new(db)),
            validators,
            config,
            consensus_manager,
            difficulty_calculator: Arc::new(difficulty_calculator),
            disable_add_block_flag: Arc::new(AtomicBool::new(false)),
            is_background_pruning: Arc::new(AtomicBool::new(false)),
        };
        blockchain_db.start()?;
        Ok(blockchain_db)
    }

    // Ristretto point arithmetic on commitments, not integer arithmetic: cannot overflow.
    #[allow(clippy::arithmetic_side_effects)]
    pub fn start(&self) -> Result<(), ChainStorageError> {
        let (is_empty, config) = {
            let db = self.db_read_access()?;
            (db.is_empty()?, &self.config)
        };
        let genesis_block = Arc::new(self.consensus_manager.get_genesis_block());
        if is_empty {
            info!(
                target: LOG_TARGET,
                "Blockchain db is empty. Adding genesis block {}.",
                genesis_block.block().body.to_counts_string()
            );
            let mut txn = DbTransaction::new();
            self.write(txn)?;
            txn = DbTransaction::new();
            self.insert_block(genesis_block.clone())?;
            let body = &genesis_block.block().body;

            let mut input_sum = UncompressedCommitment::default();
            for input in body.inputs() {
                input_sum = &input_sum + &input.commitment()?.to_commitment()?;
            }
            let mut output_sum = UncompressedCommitment::default();
            for output in body.outputs() {
                output_sum = &output_sum + &output.commitment.to_commitment()?;
            }
            let total_utxo_sum = CompressedCommitment::from_commitment(&output_sum - &input_sum);
            let mut kernel_sum = UncompressedCommitment::default();
            for kernel in body.kernels() {
                kernel_sum = &kernel_sum + &kernel.excess.to_commitment()?;
            }
            txn.update_block_accumulated_data(*genesis_block.hash(), UpdateBlockAccumulatedData {
                kernel_sum: Some(CompressedCommitment::from_commitment(kernel_sum.clone())),
                ..Default::default()
            });
            txn.set_pruned_height(0);
            txn.set_horizon_data(CompressedCommitment::from_commitment(kernel_sum), total_utxo_sum);
            self.write(txn)?;
            self.store_pruning_horizon(config.pruning_horizon)?;
        } else if !self.chain_block_or_orphan_block_exists(genesis_block.accumulated_data().hash)? {
            // Check the genesis block in the DB.
            error!(
                target: LOG_TARGET,
                "Genesis block in database does not match the supplied genesis block in the code! Hash in the code \
                 {:?}, hash in the database {:?}",
                self.fetch_chain_header(0)?.hash(),
                genesis_block.accumulated_data().hash
            );
            return Err(ChainStorageError::CorruptedDatabase(
                "Genesis block in database does not match the supplied genesis block in the code! Please delete and \
                 resync your blockchain database."
                    .into(),
            ));
        } else {
            info!(
                target: LOG_TARGET,
                "Blockchain db is not empty. Genesis block already exists in the database."
            );
        }

        if config.cleanup_orphans_at_startup {
            match self.cleanup_all_orphans() {
                Ok(_) => info!(target: LOG_TARGET, "Orphan database cleaned out at startup.",),
                Err(e) => warn!(
                    target: LOG_TARGET,
                    "Orphan database could not be cleaned out at startup: ({e:?})."
                ),
            }
        }

        if config.clear_bad_blocks_at_startup {
            match self.clear_all_bad_blocks() {
                Ok(_) => info!(target: LOG_TARGET, "Bad blocks cleaned out at startup.",),
                Err(e) => warn!(
                    target: LOG_TARGET,
                    "Bad blocks could not be cleaned out at startup: ({e:?})."
                ),
            }
        }

        let pruning_horizon = self.get_chain_metadata()?.pruning_horizon();
        if config.pruning_horizon != pruning_horizon {
            debug!(
                target: LOG_TARGET,
                "Updating pruning horizon from {} to {}.", pruning_horizon, config.pruning_horizon,
            );
            self.store_pruning_horizon(config.pruning_horizon)?;
        }

        if !config.track_reorgs {
            self.clear_all_reorgs()?;
        }

        self.rebuild_payref_indexes_background_task()?;
        self.rebuild_accumulated_data_background_task()?;
        self.rebuild_burn_commitment_index_background_task()?;
        self.initialize_blockchain_check_tasks()?;
        self.prune_database_background_task()?;

        Ok(())
    }

    /// If there are more than `BACKGROUND_PRUNING_THRESHOLD` blocks to prune, this spawns a background task that
    /// prunes in chunks of `BACKGROUND_PRUNING_CHUNK_SIZE` blocks. Each chunk acquires and releases the write lock
    /// independently, allowing normal node operations to proceed between chunks. Only one background pruning task
    /// can run at a time, controlled by the `is_background_pruning` flag.
    pub fn prune_database_background_task(&self) -> Result<(), ChainStorageError> {
        let metadata = {
            let db = self.db_read_access()?;
            db.fetch_chain_metadata()?
        };

        if !metadata.is_pruned_node() {
            return Ok(());
        }

        let pruning_horizon = self.config.pruning_horizon;
        let prune_to_height_target = metadata.best_block_height().saturating_sub(pruning_horizon);
        let blocks_to_prune = prune_to_height_target.saturating_sub(metadata.pruned_height());

        if blocks_to_prune <= BACKGROUND_PRUNING_THRESHOLD {
            return Ok(());
        }

        // Use compare_exchange to ensure only one background pruning task runs at a time
        if self
            .is_background_pruning
            .compare_exchange(false, true, atomic::Ordering::SeqCst, atomic::Ordering::SeqCst)
            .is_err()
        {
            debug!(
                target: LOG_TARGET,
                "Background pruning task is already running, skipping."
            );
            return Ok(());
        }

        info!(
            target: LOG_TARGET,
            "Starting background database pruning: {} blocks to prune (from height {} to {})",
            blocks_to_prune,
            metadata.pruned_height(),
            prune_to_height_target,
        );

        let db_rw_lock = self.db.clone();
        let is_pruning_flag = self.is_background_pruning.clone();

        tokio::task::spawn(async move {
            loop {
                // Allow other tasks to breathe between chunks
                tokio::time::sleep(Duration::from_millis(BREATHING_TIME_MS_MIN)).await;

                let db = db_rw_lock.clone();
                // Use a single write lock for both the metadata check and the prune operation to
                // avoid TOCTOU issues where state changes between a read lock and write lock.
                let res = tokio::task::spawn_blocking(move || -> Result<bool, ChainStorageError> {
                    let mut db = db.write().map_err(|e| {
                        ChainStorageError::AccessError(format!("Write lock on blockchain backend failed: {e:?}"))
                    })?;
                    let metadata = db.fetch_chain_metadata()?;
                    let target = metadata.best_block_height().saturating_sub(pruning_horizon);
                    let blocks_remaining = target.saturating_sub(metadata.pruned_height());
                    if blocks_remaining <= BACKGROUND_PRUNING_THRESHOLD {
                        return Ok(true);
                    }
                    let chunk_end = metadata
                        .pruned_height()
                        .saturating_add(BACKGROUND_PRUNING_CHUNK_SIZE)
                        .min(target);
                    prune_to_height(&mut *db, chunk_end)?;
                    info!(
                        target: LOG_TARGET,
                        "Background pruning: completed chunk up to height {} (target: {})",
                        chunk_end, target,
                    );
                    Ok(false)
                })
                .await;

                match res {
                    Ok(Ok(true)) => {
                        break;
                    },
                    Ok(Ok(false)) => {
                        // Continue to next chunk
                    },
                    Ok(Err(e)) => {
                        error!(
                            target: LOG_TARGET,
                            "Background pruning failed: {e}",
                        );
                        break;
                    },
                    Err(e) => {
                        error!(
                            target: LOG_TARGET,
                            "Background pruning task panicked: {e}",
                        );
                        break;
                    },
                }
            }

            is_pruning_flag.store(false, atomic::Ordering::SeqCst);
            info!(target: LOG_TARGET, "Background pruning task completed.");
        });

        Ok(())
    }

    /// This function will rebuild the accumulated data in the background if they are corrupt, up to the last stored
    /// chain header.
    pub fn rebuild_accumulated_data_background_task(&self) -> Result<(), ChainStorageError> {
        let initial_status = {
            let db = self.db_read_access()?;
            db.fetch_accumulated_data_rebuild_status()?
        };
        debug!(target: LOG_TARGET, "[AccData] Rebuilding accumulated data status: {initial_status:?}");
        if initial_status.is_rebuilt {
            debug!(target: LOG_TARGET, "[AccData] Accumulated data has already been rebuilt.");
            return Ok(());
        }

        let db_rw_lock = self.db.clone();
        let rules = self.consensus_manager.clone();
        // At and above this height the stored accumulated data was computed under rules that have since changed
        // (GHSA-3qmx-q9pv-f3m4, and the `difficulty_block_window` the activation entry may carry with it), so the
        // walk cannot merely recompute there - it has to re-validate, and rewind if a block no longer holds up.
        // `UNSCHEDULED_ACTIVATION_HEIGHT` on a network without the fork means strict mode never engages.
        let strict_from_height = rules.bipartite_cuckaroo_activation_height();

        tokio::task::spawn(run_accumulated_data_rebuild(
            db_rw_lock,
            rules,
            initial_status,
            strict_from_height,
        ));

        Ok(())
    }

    /// Rebuilds the burn commitment index in the background so that node startup is not blocked. New databases (and
    /// ones that have already finished) short-circuit immediately; otherwise a tokio task walks the blocks from the
    /// last rebuilt height to the chain tip, indexing the burn kernels in each block. Blocks added or re-orged in
    /// after this process starts already populate the index via the live insert path, so they do not need to be
    /// processed here.
    pub fn rebuild_burn_commitment_index_background_task(&self) -> Result<(), ChainStorageError> {
        let initial_status = {
            let db = self.db_read_access()?;
            db.fetch_burn_commitment_rebuild_status()?
        };
        debug!(target: LOG_TARGET, "[BurnIndex] Burn commitment index rebuild status: {initial_status:?}");
        if initial_status.is_rebuilt {
            debug!(target: LOG_TARGET, "[BurnIndex] Burn commitment index has already been rebuilt.");
            return Ok(());
        }

        // The rebuild runs on a tokio task. When the database is constructed outside a runtime (e.g. tooling or tests),
        // there is nothing to spawn onto; the rebuild status is left as-is so it runs on the next startup under a
        // runtime (a base node always runs under tokio).
        if tokio::runtime::Handle::try_current().is_err() {
            debug!(
                target: LOG_TARGET,
                "[BurnIndex] No tokio runtime available; deferring burn commitment index rebuild to the next startup."
            );
            return Ok(());
        }

        // Fix the target height at the tip as of now. Blocks beyond this are populated by the live insert path.
        let target_height = {
            let db = self.db_read_access()?;
            db.fetch_chain_metadata()?.best_block_height()
        };
        let db_rw_lock = self.db.clone();

        tokio::task::spawn(async move {
            let start_height = initial_status.last_rebuild_height.unwrap_or_default();
            let mut last_status = initial_status.clone();
            debug!(
                target: LOG_TARGET,
                "[BurnIndex] Starting burn commitment index rebuild for heights {start_height} to {target_height}"
            );

            for height in start_height..=target_height {
                // Add a small tokio sleep to allow other tasks to run more freely, mirroring the other rebuild tasks.
                tokio::time::sleep(Duration::from_millis(100)).await;
                let finalize = height == target_height;
                let db = db_rw_lock.clone();
                // We use `spawn_blocking` with `.await` here to ensure that the async spawned task will be able to
                // shut down when base node shutdown is triggered.
                let res =
                    tokio::task::spawn_blocking(move || process_burn_commitment_index_for_height(db, height, finalize))
                        .await;
                match res {
                    Ok(Ok(current_status)) => {
                        last_status = current_status;
                    },
                    Ok(Err(e)) => {
                        error!(
                            target: LOG_TARGET,
                            "[BurnIndex] Burn commitment index rebuild failed. Initial status: {initial_status:?}. \
                            Last updated status: {last_status:?} ({e})"
                        );
                        break;
                    },
                    Err(e) => {
                        error!(
                            target: LOG_TARGET,
                            "[BurnIndex] Burn commitment index rebuild failed. Initial status: {initial_status:?}. \
                            Last updated status: {last_status:?} ({e})"
                        );
                        break;
                    },
                }
                if finalize || last_status.is_rebuilt {
                    debug!(
                        target: LOG_TARGET,
                        "[BurnIndex] Burn commitment index rebuild completed, Final status: {last_status:?}"
                    );
                    break;
                }
            }
        });

        Ok(())
    }

    /// This function will check the accumulated data in the background, up to the last stored chain header, and correct
    /// any corrupt accumulated data it finds.
    #[allow(clippy::too_many_lines)]
    pub fn check_accumulated_data_background_task(&self) -> Result<(), ChainStorageError> {
        // We cannot check the accumulated data if the migration background task has not completed
        let accumulated_data_rebuild_status = {
            let db = self.db_read_access()?;
            db.fetch_accumulated_data_rebuild_status()?
        };
        if !accumulated_data_rebuild_status.is_rebuilt {
            warn!(target: LOG_TARGET, "[AccData check] Accumulated data migration task in progress, cannot continue.");
            return Err(ChainStorageError::AccDataMigrationStillInProgress);
        }

        // Now we can continue with the accumulated data check
        let check_status = self.fetch_accumulated_data_check_status()?;

        let initial_status = if let Some(status) = check_status {
            if status.checked_status().0 {
                debug!(target: LOG_TARGET, "[AccData check] Accumulated data check has concluded.");
                return Ok(());
            }
            if status.is_running() {
                debug!(target: LOG_TARGET, "[AccData check] Accumulated data check is already busy.");
                return Ok(());
            }
            debug!(target: LOG_TARGET, "[AccData check] Accumulated data check in progress: {status:?}.");
            status
        } else {
            return Ok(());
        };

        {
            let db = self.db_write_access()?;
            db.update_accumulated_data_check_status(BlockchainCheckRequest::SetRunState(true))?;
        }
        let db_rw_lock = self.db.clone();
        let rules = self.consensus_manager.clone();

        tokio::task::spawn(async move {
            let clear_flags = |db: Arc<RwLock<B>>, has_concluded: bool, last_failure: Option<CheckFailure>| match db
                .write()
            {
                Ok(db) => {
                    if let Err(e) = db.update_accumulated_data_check_status(BlockchainCheckRequest::ClearRunningFlags {
                        has_concluded,
                        last_failure,
                    }) {
                        error!(
                            target: LOG_TARGET,
                            "[Blockchain check] Failed to clear the db consistency check status run flags: {e:?}"
                        );
                    }
                },
                Err(e) => {
                    error!(target: LOG_TARGET, "[Blockchain check] Write lock on blockchain db failed: {e:?}");
                },
            };

            let difficulty_calculator = DifficultyCalculator::new(rules.clone(), RandomXFactory::new(1));
            // The genesis block will not be at fault - start at height 1 if no data exists.
            let start_height = initial_status.last_check_height.unwrap_or(1);
            let mut last_status = initial_status.clone();
            debug!(
                target: LOG_TARGET,
                "[AccData check] Start checking accumulated data from height {start_height}"

            );

            let mut height = start_height;
            let sleep_ms = last_status
                .breathing_time_ms
                .clamp(BREATHING_TIME_MS_MIN, BREATHING_TIME_MS_MAX);
            let autocorrect_enabled = initial_status.autocorrect_enabled();
            loop {
                // Add a small tokio sleep to allow other tasks to run more freely - this will push out the check a bit.
                tokio::time::sleep(Duration::from_millis(sleep_ms)).await;
                let db = db_rw_lock.clone();
                let difficulty_calculator = difficulty_calculator.clone();
                // We use `spawn_blocking` with `.await` here to ensure that the async spawned task will be able to
                // shut down when base node shutdown is triggered
                let cc = rules.consensus_constants(height).clone();
                let res = tokio::task::spawn_blocking(move || {
                    verify_accumulated_data_for_height(db, difficulty_calculator, height, &cc, autocorrect_enabled)
                })
                .await;
                match res {
                    Ok(Ok(current_status)) => {
                        last_status = current_status;
                    },
                    Ok(ref _err @ Err(ChainStorageError::CorruptedDatabase(ref e))) => {
                        error!(
                            target: LOG_TARGET,
                            "[AccData check] Accumulated data check found corruption. Initial \
                            status: {initial_status:?}. Last updated status: {last_status:?}. ({e})"
                        );
                        info!(
                            target: LOG_TARGET,
                            "[AccData check] Autocorrect flag is disabled - re-run with autocorrect flag enabled",
                        );
                        clear_flags(
                            db_rw_lock.clone(),
                            true,
                            Some(CheckFailure {
                                corrupt_db: true,
                                error: e.to_string(),
                            }),
                        );
                        break;
                    },
                    Ok(Err(e)) => {
                        error!(
                            target: LOG_TARGET,
                            "[AccData check] Checking accumulated data failed. Initial status: {initial_status:?}. \
                            Last updated status: {last_status:?} ({e})"
                        );
                        clear_flags(
                            db_rw_lock.clone(),
                            false,
                            Some(CheckFailure {
                                corrupt_db: false,
                                error: e.to_string(),
                            }),
                        );
                        break;
                    },
                    Err(e) => {
                        error!(
                            target: LOG_TARGET,
                            "[AccData check] Checking accumulated data failed. Initial status: {initial_status:?}. \
                            Last updated status: {last_status:?} ({e})",
                        );
                        clear_flags(
                            db_rw_lock.clone(),
                            false,
                            Some(CheckFailure {
                                corrupt_db: false,
                                error: e.to_string(),
                            }),
                        );
                        break;
                    },
                }

                if last_status.checked_status().0 || last_status.stop_if_running {
                    clear_flags(db_rw_lock.clone(), true, None);
                    if last_status.checked_status().0 {
                        debug!(
                            target: LOG_TARGET,
                            "[AccData check] Accumulated data check from height {start_height} completed, Final status: \
                            {last_status:?}"
                        );
                    } else {
                        debug!(
                            target: LOG_TARGET,
                            "[AccData check] Accumulated data check stopped as requested, Final status: {last_status:?}"
                        );
                    }
                    break;
                }
                height = height.saturating_add(1);
            }
        });

        Ok(())
    }

    /// This function will check the blockchain consistency in the background, up to the last stored chain header, and
    /// correct any corrupt accumulated data it finds.
    #[allow(clippy::too_many_lines)]
    pub fn check_blockchain_consistency_background_task(&self) -> Result<(), ChainStorageError> {
        // We cannot check the accumulated data if the migration background task has not completed
        let accumulated_data_rebuild_status = {
            let db = self.db_read_access()?;
            db.fetch_accumulated_data_rebuild_status()?
        };
        if !accumulated_data_rebuild_status.is_rebuilt {
            warn!(target: LOG_TARGET, "[Blockchain check] Accumulated data migration task in progress, cannot continue.");
            return Err(ChainStorageError::AccDataMigrationStillInProgress);
        }

        // Now we can continue with the accumulated data check
        let check_status = self.fetch_blockchain_consistency_check_status()?;

        let initial_status = if let Some(status) = check_status {
            if status.checked_status().0 {
                debug!(target: LOG_TARGET, "[Blockchain check] Blockchain consistency check has concluded.");
                return Ok(());
            }
            if status.is_running() {
                debug!(target: LOG_TARGET, "[Blockchain check] Blockchain consistency check is already busy.");
                return Ok(());
            }
            debug!(target: LOG_TARGET, "[Blockchain check] Blockchain consistency check in progress: {status:?}.");
            status
        } else {
            return Ok(());
        };

        {
            let db = self.db_write_access()?;
            db.update_blockchain_consistency_check_status(BlockchainCheckRequest::SetRunState(true))?;
        }
        let db_rw_lock = self.db.clone();
        let validators = self.validators.clone();

        tokio::task::spawn(async move {
            let clear_flags =
                |db: Arc<RwLock<B>>, has_concluded: bool, last_failure: Option<CheckFailure>| match db.write() {
                    Ok(db) => {
                        if let Err(e) =
                            db.update_blockchain_consistency_check_status(BlockchainCheckRequest::ClearRunningFlags {
                                has_concluded,
                                last_failure,
                            })
                        {
                            error!(
                                target: LOG_TARGET,
                                "[Blockchain check] Failed to clear the db consistency check status run flags: {e:?}"
                            );
                        }
                    },
                    Err(e) => {
                        error!(target: LOG_TARGET, "[Blockchain check] Write lock on blockchain db failed: {e:?}");
                    },
                };

            // The genesis block will not be at fault - start at height 1 if no data exists.
            let start_height = initial_status.last_check_height.unwrap_or(1);
            let mut last_status = initial_status.clone();
            debug!(
                target: LOG_TARGET,
                "[Blockchain check] Start checking blockchain consistency from height {start_height}"
            );

            let mut height = start_height;
            let sleep_ms = last_status
                .breathing_time_ms
                .clamp(BREATHING_TIME_MS_MIN, BREATHING_TIME_MS_MAX);
            loop {
                // Add a small tokio sleep to allow other tasks to run more freely - this will push out the check a bit.
                tokio::time::sleep(Duration::from_millis(sleep_ms)).await;
                let db = db_rw_lock.clone();
                let validators = validators.clone();
                let full_validation = initial_status.full_validation_enabled();
                // We use `spawn_blocking` with `.await` here to ensure that the async spawned task will be able to
                // shut down when base node shutdown is triggered
                let res = tokio::task::spawn_blocking(move || {
                    verify_blockchain_consistency_for_height(db, &validators, height, full_validation)
                })
                .await;
                match res {
                    Ok(Ok(current_status)) => {
                        last_status = current_status;
                    },
                    Ok(ref _err @ Err(ChainStorageError::CorruptedDatabase(ref e))) => {
                        error!(
                            target: LOG_TARGET,
                            "[Blockchain check] Blockchain consistency check found unrecoverable corruption. Initial \
                            status: {initial_status:?}. Last updated status: {last_status:?}. ({e})"
                        );
                        if initial_status.autocorrect_enabled() {
                            let db_rw_lock = db_rw_lock.clone();
                            let res = tokio::task::spawn_blocking(move || {
                                if let Ok(mut db) = db_rw_lock.write() {
                                    rewind_to_height(&mut *db, height.saturating_sub(1))
                                } else {
                                    Err(ChainStorageError::AccessError(
                                        "Write lock on blockchain backend failed".into(),
                                    ))
                                }
                            })
                            .await;
                            match res {
                                Ok(Ok(_)) => {
                                    info!(target: LOG_TARGET,
                                        "[Blockchain check] Rewound the blockchain to height {} after unrecoverable \
                                        corruption at height {}.",
                                        height.saturating_sub(1), height
                                    );
                                },
                                Ok(Err(e)) => {
                                    error!(target: LOG_TARGET,
                                        "[Blockchain check] Rewind after unrecoverable corruption at height {height} \
                                        failed: {e}",
                                    );
                                },
                                Err(e) => {
                                    error!(target: LOG_TARGET,
                                        "[Blockchain check] Rewind task join error after unrecoverable corruption at \
                                        height {height}: {e}",
                                    );
                                },
                            }
                        } else {
                            info!(
                                target: LOG_TARGET,
                                "[Blockchain check] Autocorrect flag is disabled - manually rewind to height {}",
                                height.saturating_sub(1),
                            );
                        }
                        clear_flags(
                            db_rw_lock.clone(),
                            true,
                            Some(CheckFailure {
                                corrupt_db: true,
                                error: e.to_string(),
                            }),
                        );
                        break;
                    },
                    Ok(Err(e)) => {
                        error!(
                            target: LOG_TARGET,
                            "[Blockchain check] Checking blockchain consistency failed. Initial status: \
                            {initial_status:?}. Last updated status: {last_status:?} ({e})"
                        );
                        clear_flags(
                            db_rw_lock.clone(),
                            false,
                            Some(CheckFailure {
                                corrupt_db: false,
                                error: e.to_string(),
                            }),
                        );
                        break;
                    },
                    Err(e) => {
                        error!(
                            target: LOG_TARGET,
                            "[Blockchain check] Checking blockchain consistency failed. Initial status: \
                            {initial_status:?}. Last updated status: {last_status:?} ({e})",
                        );
                        clear_flags(
                            db_rw_lock.clone(),
                            false,
                            Some(CheckFailure {
                                corrupt_db: false,
                                error: e.to_string(),
                            }),
                        );
                        break;
                    },
                }

                if last_status.checked_status().0 || last_status.stop_if_running {
                    clear_flags(db_rw_lock.clone(), true, None);
                    if last_status.checked_status().0 {
                        debug!(
                            target: LOG_TARGET,
                            "[Blockchain check] Blockchain consistency check from height {start_height} completed, \
                            Final status: {last_status:?}"
                        );
                    } else {
                        debug!(
                            target: LOG_TARGET,
                            "[Blockchain check] Blockchain consistency check stopped as requested, Final status: \
                            {last_status:?}"
                        );
                    }
                    break;
                }
                height = height.saturating_add(1);
            }
        });

        Ok(())
    }

    fn initialize_blockchain_check_tasks(&self) -> Result<(), ChainStorageError> {
        let db = self.db_write_access()?;
        db.update_accumulated_data_check_status(BlockchainCheckRequest::SetRunState(false))?;
        db.update_blockchain_consistency_check_status(BlockchainCheckRequest::SetRunState(false))?;

        Ok(())
    }

    /// Initialize and start the accumulated-data check (difficulty only).
    pub fn request_accumulated_data_check(
        &self,
        auto_correct: bool,
        breathing_time_ms: u64,
    ) -> Result<(), ChainStorageError> {
        {
            if self
                .fetch_accumulated_data_check_status()?
                .unwrap_or_default()
                .is_running()
            {
                return Err(ChainStorageError::InvalidOperation(
                    "[Blockchain check] Cannot start a new accumulated data check while one is already running."
                        .to_string(),
                ));
            }

            let db = self.db_write_access()?;
            db.update_accumulated_data_check_status(BlockchainCheckRequest::ResumeCheck)?;
            db.update_accumulated_data_check_status(BlockchainCheckRequest::SetAutoCorrect(auto_correct))?;
            db.update_accumulated_data_check_status(BlockchainCheckRequest::SetBreathingTime(breathing_time_ms))?;
            trace!(
                target: LOG_TARGET,
                "[AccData check] Requested accumulated data check: auto_correct({auto_correct})"
            );
        }
        self.check_accumulated_data_background_task()
    }

    /// Initialize and start the chain consistency check (blocks+headers; full or light).
    pub fn request_blockchain_consistency_check(
        &self,
        full_validation: bool,
        auto_correct: bool,
        breathing_time_ms: u64,
    ) -> Result<(), ChainStorageError> {
        {
            if self
                .fetch_blockchain_consistency_check_status()?
                .unwrap_or_default()
                .is_running()
            {
                return Err(ChainStorageError::InvalidOperation(
                    "[Blockchain check] Cannot start a new blockchain consistency check while one is already running."
                        .to_string(),
                ));
            }

            let db = self.db_write_access()?;
            db.update_blockchain_consistency_check_status(BlockchainCheckRequest::ResumeCheck)?;
            db.update_blockchain_consistency_check_status(BlockchainCheckRequest::SetFullValidation(full_validation))?;
            db.update_blockchain_consistency_check_status(BlockchainCheckRequest::SetAutoCorrect(auto_correct))?;
            db.update_blockchain_consistency_check_status(BlockchainCheckRequest::SetBreathingTime(breathing_time_ms))?;
            trace!(
                target: LOG_TARGET,
                "[Blockchain check] Requested blockchain consistency check: auto_correct({auto_correct}), \
                full_validation({full_validation})"
            );
        }
        self.check_blockchain_consistency_background_task()
    }

    /// Stop the accumulated data check task.
    pub fn stop_running_accumulated_data_check_task(&self) -> Result<(), ChainStorageError> {
        let db = self.db_write_access()?;
        db.update_accumulated_data_check_status(BlockchainCheckRequest::SetStopIfRunning(true))?;
        trace!(target: LOG_TARGET, "[AccData check] Requested stop");
        Ok(())
    }

    /// Stop the blockchain consistency task.
    pub fn stop_running_blockchain_consistency_check_task(&self) -> Result<(), ChainStorageError> {
        let db = self.db_write_access()?;
        db.update_blockchain_consistency_check_status(BlockchainCheckRequest::SetStopIfRunning(true))?;
        trace!(target: LOG_TARGET, "[Blockchain check] Requested stop");
        Ok(())
    }

    /// Reset the accumulated data check counters.
    pub fn reset_accumulated_data_check_db_counters(&self) -> Result<(), ChainStorageError> {
        let acc_diff_status = self.fetch_accumulated_data_check_status()?;
        if let Some(acc_diff) = acc_diff_status &&
            acc_diff.is_running()
        {
            return Err(ChainStorageError::InvalidOperation(
                "[AccData check] Cannot reset counters while a check is running.".to_string(),
            ));
        }
        let db = self.db_write_access()?;
        db.update_accumulated_data_check_status(BlockchainCheckRequest::ResetAllCounters)?;
        trace!(target: LOG_TARGET, "[AccData check] Requested reset counters");
        Ok(())
    }

    /// Reset the blockchain consistency check counters.
    pub fn reset_blockchain_consistency_check_db_counters(&self) -> Result<(), ChainStorageError> {
        let consistency_status = self.fetch_blockchain_consistency_check_status()?;
        if let Some(consistency) = consistency_status &&
            consistency.is_running()
        {
            return Err(ChainStorageError::InvalidOperation(
                "[Blockchain check] Cannot reset counters while a check is running.".to_string(),
            ));
        }
        let db = self.db_write_access()?;
        db.update_blockchain_consistency_check_status(BlockchainCheckRequest::ResetAllCounters)?;
        trace!(target: LOG_TARGET, "[Blockchain check] Requested reset counters");
        Ok(())
    }

    /// Fetch the current status of the accumulated data check task.
    pub fn fetch_accumulated_data_check_status(&self) -> Result<Option<BlockchainCheckStatus>, ChainStorageError> {
        let db = self.db_read_access()?;
        db.fetch_accumulated_data_check_status()
    }

    /// Fetch the current status of the blockchain consistency check task.
    pub fn fetch_blockchain_consistency_check_status(
        &self,
    ) -> Result<Option<BlockchainCheckStatus>, ChainStorageError> {
        let db = self.db_read_access()?;
        db.fetch_blockchain_consistency_check_status()
    }

    /// This function will rebuild the payref indexes in the background if they are not already rebuilt.
    pub fn rebuild_payref_indexes_background_task(&self) -> Result<(), ChainStorageError> {
        let initial_status = {
            let db = self.db_read_access()?;
            db.fetch_payref_rebuild_status()?
        };
        if initial_status.is_rebuilt {
            debug!(target: LOG_TARGET, "[PayRef] Payref indexes has already been rebuilt.");
            return Ok(());
        }

        // If we had a previous start metadata, we will use that to continue the rebuild, otherwise we will use the
        // current chain metadata to set a new target rebuild height. All new or re-orged blocks added to the database
        // after this process started will have the correct payref indexes and therefor do not need to be processed.
        let metadata_at_start = if let Some(metadata) = initial_status.metadata_at_start.clone() {
            metadata
        } else {
            let db = self.db_read_access()?;
            db.fetch_chain_metadata()?
        };
        let db_rw_lock = self.db.clone();

        tokio::task::spawn(async move {
            let start_height = initial_status.last_rebuild_height.unwrap_or_default();
            let mut last_status = initial_status.clone();
            debug!(
                target: LOG_TARGET,
                "[PayRef] Starting index rebuilding for heights {} to {}",
                start_height, metadata_at_start.best_block_height()
            );

            let mut initialize_stats = Some(metadata_at_start.best_block_height());
            for height in start_height..=metadata_at_start.best_block_height() {
                // Add a small tokio sleep to allow other tasks to run more freely - this will push out the rebuild a
                // bit, for example, 80_000 blocks will take at least 8_000 seconds longer, just over two hours.
                tokio::time::sleep(Duration::from_millis(100)).await;
                let finalize = height == metadata_at_start.best_block_height();
                let metadata = metadata_at_start.clone();
                let db = db_rw_lock.clone();
                // We use `spawn_blocking` with `.await` here to ensure that the async spawned task will be able to
                // shut down when base node shutdown is triggered
                let res = tokio::task::spawn_blocking(move || {
                    process_payref_for_height(db, height, metadata, initialize_stats, finalize)
                })
                .await;
                match res {
                    Ok(Ok(current_status)) => {
                        last_status = current_status;
                    },
                    Ok(Err(e)) => {
                        error!(
                            target: LOG_TARGET,
                            "[PayRef] Index rebuilding failed. Initial status: {initial_status:?}. Last updated status: {last_status:?} ({e})"
                        );
                        break;
                    },
                    Err(e) => {
                        error!(
                            target: LOG_TARGET,
                            "[PayRef] Index rebuilding failed. Initial status: {initial_status:?}. Last updated status: {last_status:?} ({e})"
                        );
                        break;
                    },
                }
                if initialize_stats.is_some() {
                    initialize_stats = None;
                }
                if finalize || last_status.is_rebuilt {
                    debug!(
                        target: LOG_TARGET,
                        "[PayRef] Starting index rebuilding completed, Final status: {last_status:?}",
                    );
                    break;
                }
            }
        });

        Ok(())
    }

    /// Get the genesis block form the consensus manager
    pub fn fetch_genesis_block(&self) -> ChainBlock {
        self.consensus_manager.get_genesis_block()
    }

    /// Returns a reference to the consensus cosntants at the current height
    pub fn consensus_constants(&self) -> Result<&ConsensusConstants, ChainStorageError> {
        let height = self.get_height()?;
        Ok(self.rules().consensus_constants(height))
    }

    /// Returns a reference to the consensus rules
    pub fn rules(&self) -> &BaseNodeConsensusManager {
        &self.consensus_manager
    }

    // Be careful about making this method public. Rather use `db_and_metadata_read_access`
    // so that metadata and db are read in the correct order so that deadlocks don't occur
    pub fn db_read_access(&self) -> Result<RwLockReadGuard<'_, B>, ChainStorageError> {
        self.db.read().map_err(|e| {
            error!(
                target: LOG_TARGET,
                "An attempt to get a read lock on the blockchain backend failed. {e:?}"
            );
            ChainStorageError::AccessError("Read lock on blockchain backend failed".into())
        })
    }

    #[cfg(test)]
    pub fn test_db_write_access(&self) -> Result<RwLockWriteGuard<'_, B>, ChainStorageError> {
        self.db.write().map_err(|e| {
            error!(
                target: LOG_TARGET,
                "An attempt to get a write lock on the blockchain backend failed. {e:?}"
            );
            ChainStorageError::AccessError("Write lock on blockchain backend failed".into())
        })
    }

    fn db_write_access(&self) -> Result<RwLockWriteGuard<'_, B>, ChainStorageError> {
        self.db.write().map_err(|e| {
            error!(
                target: LOG_TARGET,
                "An attempt to get a write lock on the blockchain backend failed. {e:?}"
            );
            ChainStorageError::AccessError("Write lock on blockchain backend failed".into())
        })
    }

    pub(crate) fn is_add_block_disabled(&self) -> bool {
        self.disable_add_block_flag.load(atomic::Ordering::SeqCst)
    }

    pub(crate) fn set_disable_add_block_flag(&self) {
        self.disable_add_block_flag.store(true, atomic::Ordering::SeqCst);
    }

    pub(crate) fn clear_disable_add_block_flag(&self) {
        self.disable_add_block_flag.store(false, atomic::Ordering::SeqCst);
    }

    pub fn write(&self, transaction: DbTransaction) -> Result<(), ChainStorageError> {
        let mut db = self.db_write_access()?;
        db.write(transaction)
    }

    /// Returns the height of the current longest chain. This method will only fail if there's a fairly serious
    /// synchronisation problem on the database. You can try calling [BlockchainDatabase::try_recover_metadata] in
    /// that case to re-sync the metadata; or else just exit the program.
    pub fn get_height(&self) -> Result<u64, ChainStorageError> {
        let db = self.db_read_access()?;
        Ok(db.fetch_chain_metadata()?.best_block_height())
    }

    /// Return the accumulated proof of work of the longest chain.
    /// The proof of work is returned as the product of total difficulties of all PoW algorithms
    pub fn get_accumulated_difficulty(&self) -> Result<U512, ChainStorageError> {
        let db = self.db_read_access()?;
        Ok(db.fetch_chain_metadata()?.accumulated_difficulty())
    }

    /// Returns a copy of the current blockchain database metadata
    pub fn get_chain_metadata(&self) -> Result<ChainMetadata, ChainStorageError> {
        let db = self.db_read_access()?;
        db.fetch_chain_metadata()
    }

    /// Returns the mined info for every output with this hash, one entry per header it is indexed under. A hash can be
    /// indexed under more than one header, for example across reorg history.
    pub fn fetch_outputs(&self, output_hash: HashOutput) -> Result<Vec<OutputMinedInfo>, ChainStorageError> {
        let db = self.db_read_access()?;
        db.fetch_outputs(&output_hash)
    }

    /// Returns the mined info for every input spending this output hash, one entry per header it is indexed under. A
    /// hash can be indexed under more than one header, for example across reorg history.
    pub fn fetch_inputs(&self, output_hash: HashOutput) -> Result<Vec<InputMinedInfo>, ChainStorageError> {
        let db = self.db_read_access()?;
        db.fetch_inputs(&output_hash)
    }

    /// Returns the mined info for the given payment reference
    pub fn fetch_mined_info_by_payref(&self, payref: FixedHash) -> Result<MinedInfo, ChainStorageError> {
        let db = self.db_read_access()?;
        db.fetch_mined_info_by_payref(&payref)
    }

    /// Returns the mined info for the given output hash, one entry per header it is indexed under.
    pub fn fetch_mined_info_by_output_hash(
        &self,
        output_hash: HashOutput,
    ) -> Result<Vec<MinedInfo>, ChainStorageError> {
        let db = self.db_read_access()?;
        db.fetch_mined_info_by_output_hash(&output_hash)
    }

    pub fn fetch_unspent_output_hash_by_commitment(
        &self,
        commitment: CompressedCommitment,
    ) -> Result<Option<HashOutput>, ChainStorageError> {
        let db = self.db_read_access()?;
        db.fetch_unspent_output_hash_by_commitment(&commitment)
    }

    /// Return a list of matching utxos, with each being `None` if not found. If found, the transaction
    /// output, and a boolean indicating if the UTXO was spent as of the current tip.
    pub fn fetch_outputs_with_spend_status_at_tip(
        &self,
        hashes: Vec<HashOutput>,
    ) -> Result<Vec<Option<(TransactionOutput, bool)>>, ChainStorageError> {
        let db = self.db_read_access()?;

        let (smt_reader, current_version) = db.create_smt_reader()?;

        let smt = JellyfishMerkleTree::<_, SmtHasher>::new(&smt_reader);
        let mut result = Vec::with_capacity(hashes.len());
        for hash in hashes {
            // Take the last-mined entry if the hash is indexed under several headers.
            let mut outputs = db.fetch_outputs(&hash)?;
            outputs.sort_by_key(|o| o.mined_height);
            let output = outputs.into_iter().next_back();

            trace!(
                target: LOG_TARGET,
                "fetch_outputs_with_spend_status_at_tip: hash: {}, output: {:?}",
                hash.to_hex(),
                output
            );
            if let Some(mined_info) = output {
                let smt_key = KeyHash(
                    mined_info
                        .output
                        .commitment
                        .as_bytes()
                        .try_into()
                        .expect("must be 32 bytes"),
                );

                let spent = smt
                    .get(smt_key, current_version)
                    .map_err(ChainStorageError::JellyfishMerkleTreeError)?
                    .is_none();
                trace!(
                    target: LOG_TARGET,
                    "fetch_outputs_with_spend_status_at_tip: smt_key: {smt_key:?}, spent: {spent}"
                );
                result.push(Some((mined_info.output, spent)));
            } else {
                result.push(None);
            }
        }
        Ok(result)
    }

    pub fn fetch_outputs_mined_info(
        &self,
        hashes: Vec<HashOutput>,
    ) -> Result<Vec<Option<OutputMinedInfo>>, ChainStorageError> {
        let db = self.db_read_access()?;

        let mut result = Vec::with_capacity(hashes.len());
        for hash in hashes {
            // Take the last-mined entry if the hash is indexed under several headers.
            let mut outputs = db.fetch_outputs(&hash)?;
            outputs.sort_by_key(|o| o.mined_height);
            result.push(outputs.into_iter().next_back());
        }
        Ok(result)
    }

    pub fn fetch_inputs_mined_info(
        &self,
        hashes: Vec<HashOutput>,
    ) -> Result<Vec<Option<InputMinedInfo>>, ChainStorageError> {
        let db = self.db_read_access()?;

        let mut result = Vec::with_capacity(hashes.len());
        for hash in hashes {
            // Take the last-spent entry if the hash is indexed under several headers.
            let mut inputs = db.fetch_inputs(&hash)?;
            inputs.sort_by_key(|i| i.spent_height);
            result.push(inputs.into_iter().next_back());
        }
        Ok(result)
    }

    pub fn fetch_kernel_by_excess_sig(
        &self,
        excess_sig: CompressedSignature,
    ) -> Result<Option<(TransactionKernel, HashOutput)>, ChainStorageError> {
        let db = self.db_read_access()?;
        db.fetch_kernel_by_excess_sig(&excess_sig)
    }

    pub fn fetch_kernels_in_block(&self, hash: HashOutput) -> Result<Vec<TransactionKernel>, ChainStorageError> {
        let db = self.db_read_access()?;
        db.fetch_kernels_in_block(&hash)
    }

    pub fn fetch_bad_blocks(&self) -> Result<Vec<BadBlock>, ChainStorageError> {
        let db = self.db_read_access()?;
        db.fetch_bad_blocks()
    }

    pub fn clear_all_bad_blocks(&self) -> Result<(), ChainStorageError> {
        let mut db = self.db_write_access()?;
        db.clear_all_bad_blocks()
    }

    pub fn fetch_outputs_in_block_with_spend_state(
        &self,
        header_hash: HashOutput,
        spend_status_at_header: Option<HashOutput>,
    ) -> Result<Vec<(TransactionOutput, bool)>, ChainStorageError> {
        let db = self.db_read_access()?;
        db.fetch_outputs_in_block_with_spend_state(&header_hash, spend_status_at_header.as_ref())
    }

    pub fn fetch_outputs_in_block(&self, header_hash: HashOutput) -> Result<Vec<TransactionOutput>, ChainStorageError> {
        let db = self.db_read_access()?;
        db.fetch_outputs_in_block(&header_hash)
    }

    pub fn fetch_inputs_in_block(&self, header_hash: HashOutput) -> Result<Vec<TransactionInput>, ChainStorageError> {
        let db = self.db_read_access()?;
        db.fetch_inputs_in_block(&header_hash)
    }

    /// Returns the number of UTXOs in the current unspent set
    pub fn utxo_count(&self) -> Result<usize, ChainStorageError> {
        let db = self.db_read_access()?;
        db.utxo_count()
    }

    /// Returns the block header at the given block height.
    pub fn fetch_header(&self, height: u64) -> Result<Option<BlockHeader>, ChainStorageError> {
        let db = self.db_read_access()?;
        match fetch_header(&*db, height) {
            Ok(header) => Ok(Some(header)),
            Err(err) if err.is_value_not_found() => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// Returns the block header at the given block height.
    pub fn fetch_chain_header(&self, height: u64) -> Result<ChainHeader, ChainStorageError> {
        let db = self.db_read_access()?;
        let chain_header = db.fetch_chain_header_by_height(height)?;
        Ok(chain_header)
    }

    pub fn fetch_header_containing_kernel_mmr(&self, mmr_position: u64) -> Result<ChainHeader, ChainStorageError> {
        let db = self.db_read_access()?;
        db.fetch_header_containing_kernel_mmr(mmr_position)
    }

    /// Find the first matching header in a list of block hashes, returning the index of the match and the BlockHeader.
    /// Or None if not found.
    pub fn find_headers_after_hash<I: IntoIterator<Item = HashOutput>>(
        &self,
        ordered_hashes: I,
        count: u64,
    ) -> Result<Option<(usize, Vec<BlockHeader>)>, ChainStorageError> {
        let db = self.db_read_access()?;
        for (i, hash) in ordered_hashes.into_iter().enumerate() {
            if hash.len() != 32 {
                return Err(ChainStorageError::InvalidArguments {
                    func: "find_headers_after_hash",
                    arg: "ordered_hashes",
                    message: format!(
                        "Hash at index {} was an invalid length. Expected 32 but got {}",
                        i,
                        hash.len()
                    ),
                });
            }

            match fetch_header_by_block_hash(&*db, hash)? {
                Some(header) => {
                    if count == 0 {
                        return Ok(Some((i, Vec::new())));
                    }

                    let end_height =
                        header
                            .height
                            .checked_add(count)
                            .ok_or_else(|| ChainStorageError::InvalidArguments {
                                func: "find_headers_after_hash",
                                arg: "count",
                                message: "count + block height will overflow u64".into(),
                            })?;
                    let headers = fetch_headers(&*db, header.height.saturating_add(1), end_height)?;
                    return Ok(Some((i, headers)));
                },
                None => continue,
            };
        }
        Ok(None)
    }

    pub fn fetch_block_timestamps(&self, start_hash: HashOutput) -> Result<RollingVec<EpochTime>, ChainStorageError> {
        let start_header =
            self.fetch_header_by_block_hash(start_hash)?
                .ok_or_else(|| ChainStorageError::ValueNotFound {
                    entity: "BlockHeader",
                    field: "start_hash",
                    value: start_hash.to_hex(),
                })?;
        let constants = self.consensus_manager.consensus_constants(start_header.height);
        let timestamp_window = constants.median_timestamp_count();
        let start_window = start_header.height.saturating_sub(timestamp_window as u64);

        let timestamps = self
            .fetch_headers(start_window..=start_header.height)?
            .iter()
            .map(|h| h.timestamp)
            .collect::<Vec<_>>();

        let mut rolling = RollingVec::new(timestamp_window);
        rolling.extend(timestamps);
        Ok(rolling)
    }

    /// Fetch the accumulated data stored for this header
    pub fn fetch_header_accumulated_data(
        &self,
        hash: HashOutput,
    ) -> Result<Option<BlockHeaderAccumulatedData>, ChainStorageError> {
        let db = self.db_read_access()?;
        db.fetch_header_accumulated_data(&hash)
    }

    /// Store the provided headers. This function does not do any validation and assumes the inserted header has already
    /// been validated.
    pub fn insert_valid_headers(&self, headers: Vec<ChainHeader>) -> Result<(), ChainStorageError> {
        let mut db = self.db_write_access()?;
        insert_headers(&mut *db, headers)
    }

    /// Returns the set of block headers between `start` and up to and including `end_inclusive`
    pub fn fetch_headers<T: RangeBounds<u64>>(&self, bounds: T) -> Result<Vec<BlockHeader>, ChainStorageError> {
        let db = self.db_read_access()?;
        let (start, mut end) = convert_to_option_bounds(bounds);
        if end.is_none() {
            // `(n..)` means fetch block headers until this node's tip
            end = Some(db.fetch_last_header()?.height);
        }
        let (start, end) = (start.unwrap_or(0), end.unwrap());

        if start > end {
            return Ok(Vec::new());
        }

        fetch_headers(&*db, start, end)
    }

    /// Returns the set of block headers between `start` and up to and including `end_inclusive`
    pub fn fetch_chain_headers<T: RangeBounds<u64>>(&self, bounds: T) -> Result<Vec<ChainHeader>, ChainStorageError> {
        let db = self.db_read_access()?;
        let (start, mut end) = convert_to_option_bounds(bounds);
        if end.is_none() {
            // `(n..)` means fetch block headers until this node's tip
            end = Some(db.fetch_last_header()?.height);
        }
        let (start, end) = (start.unwrap_or(0), end.unwrap());

        fetch_chain_headers(&*db, start, end)
    }

    /// Returns the block header corresponding to the provided BlockHash
    pub fn fetch_header_by_block_hash(&self, hash: HashOutput) -> Result<Option<BlockHeader>, ChainStorageError> {
        let db = self.db_read_access()?;
        fetch_header_by_block_hash(&*db, hash)
    }

    /// Returns a connected header in the main chain by block hash
    pub fn fetch_chain_header_by_block_hash(&self, hash: HashOutput) -> Result<Option<ChainHeader>, ChainStorageError> {
        let db = self.db_read_access()?;

        if let Some(header) = fetch_header_by_block_hash(&*db, hash)? {
            let accumulated_data =
                db.fetch_header_accumulated_data(&hash)?
                    .ok_or_else(|| ChainStorageError::ValueNotFound {
                        entity: "BlockHeaderAccumulatedData",
                        field: "hash",
                        value: hash.to_hex(),
                    })?;

            let height = header.height;
            let header = ChainHeader::try_construct(header, accumulated_data).ok_or_else(|| {
                ChainStorageError::DataInconsistencyDetected {
                    function: "fetch_chain_header_by_block_hash",
                    details: format!(
                        "Mismatch between header and accumulated data for header {hash} ({height}). This indicates an \
                         inconsistency in the blockchain database"
                    ),
                }
            })?;
            Ok(Some(header))
        } else {
            Ok(None)
        }
    }

    /// Returns the header at the tip of the chain according to local chain metadata
    pub fn fetch_tip_header(&self) -> Result<ChainHeader, ChainStorageError> {
        let db = self.db_read_access()?;
        db.fetch_tip_header()
    }

    /// Fetches the last  header that was added, might be past the tip, as the block body between this last  header and
    /// actual tip might not have been added yet
    pub fn fetch_last_header(&self) -> Result<BlockHeader, ChainStorageError> {
        let db = self.db_read_access()?;
        db.fetch_last_header()
    }

    /// Fetches the last chain header that was added, might be past the tip, as the block body between this last chain
    /// header and actual tip might not have been added yet
    pub fn fetch_last_chain_header(&self) -> Result<ChainHeader, ChainStorageError> {
        let db = self.db_read_access()?;
        db.fetch_last_chain_header()
    }

    /// Returns the sum of all kernels
    pub fn fetch_kernel_commitment_sum(&self, at_hash: &HashOutput) -> Result<CompressedCommitment, ChainStorageError> {
        Ok(self.fetch_block_accumulated_data(*at_hash)?.kernel_sum().clone())
    }

    /// Returns `n` hashes from height _h - offset_ where _h_ is the tip header height back to `h - n - offset`.
    pub fn fetch_block_hashes_from_header_tip(
        &self,
        n: usize,
        offset: usize,
    ) -> Result<Vec<HashOutput>, ChainStorageError> {
        if n == 0 {
            return Ok(Vec::new());
        }

        let db = self.db_read_access()?;
        let tip_header = db.fetch_last_header()?;
        let end_height = match tip_header.height.checked_sub(offset as u64) {
            Some(h) => h,
            None => {
                return Ok(Vec::new());
            },
        };
        let start = end_height.saturating_sub((n as u64).saturating_sub(1));
        let headers = fetch_headers(&*db, start, end_height)?;
        Ok(headers.into_iter().map(|h| h.hash()).rev().collect())
    }

    pub fn fetch_block_accumulated_data(&self, at_hash: HashOutput) -> Result<BlockAccumulatedData, ChainStorageError> {
        let db = self.db_read_access()?;
        db.fetch_block_accumulated_data(&at_hash)?
            .ok_or_else(|| ChainStorageError::ValueNotFound {
                entity: "BlockAccumulatedData",
                field: "at_hash",
                value: at_hash.to_hex(),
            })
    }

    pub fn fetch_block_accumulated_data_by_height(
        &self,
        height: u64,
    ) -> Result<BlockAccumulatedData, ChainStorageError> {
        let db = self.db_read_access()?;
        db.fetch_block_accumulated_data_by_height(height).or_not_found(
            "BlockAccumulatedData",
            "height",
            height.to_string(),
        )
    }

    /// Returns the orphan block with the given hash.
    pub fn fetch_orphan(&self, hash: HashOutput) -> Result<Block, ChainStorageError> {
        let db = self.db_read_access()?;
        fetch_orphan(&*db, hash)
    }

    pub fn orphan_count(&self) -> Result<usize, ChainStorageError> {
        let db = self.db_read_access()?;
        db.orphan_count()
    }

    /// Returns the set of target difficulties for the specified proof of work algorithm. The calculated target
    /// difficulty will be for the given height i.e calculated from the previous header backwards until the target
    /// difficulty window is populated according to consensus constants for the given height.
    pub fn fetch_target_difficulty_for_next_block(
        &self,
        pow_algo: PowAlgorithm,
        current_block_hash: HashOutput,
    ) -> Result<TargetDifficultyWindow, ChainStorageError> {
        let db = self.db_read_access()?;
        fetch_target_difficulty_for_next_block(&*db, &self.consensus_manager, pow_algo, &current_block_hash)
    }

    pub fn fetch_target_difficulties_for_next_block(
        &self,
        current_block_hash: HashOutput,
    ) -> Result<TargetDifficulties, ChainStorageError> {
        let db = self.db_read_access()?;
        fetch_target_difficulties_for_next_block(&*db, &self.consensus_manager, &current_block_hash)
    }

    pub fn prepare_new_block(&self, template: NewBlockTemplate) -> Result<Block, ChainStorageError> {
        let NewBlockTemplate { header, mut body, .. } = template;
        if header.height == 0 {
            return Err(ChainStorageError::InvalidArguments {
                func: "prepare_new_block",
                arg: "template",
                message: "Invalid height for NewBlockTemplate: must be greater than 0".to_string(),
            });
        }

        body.sort();
        let mut header = BlockHeader::from(header);
        let prev_block_height = header.height.saturating_sub(1);
        let min_height = header.height.saturating_sub(
            self.consensus_manager
                .consensus_constants(header.height)
                .median_timestamp_count() as u64,
        );

        let db = self.db_read_access()?;
        let tip_header = db.fetch_tip_header()?;
        if header.height != tip_header.height().saturating_add(1) {
            return Err(ChainStorageError::InvalidArguments {
                func: "prepare_new_block",
                arg: "template",
                message: format!(
                    "Expected new block template height to be {} but was {}",
                    tip_header.height().saturating_add(1),
                    header.height
                ),
            });
        }
        if header.prev_hash != *tip_header.hash() {
            return Err(ChainStorageError::InvalidArguments {
                func: "prepare_new_block",
                arg: "template",
                message: format!(
                    "Expected new block template previous hash to be set to the current tip hash ({}) but was {}",
                    tip_header.hash(),
                    header.prev_hash,
                ),
            });
        }

        let timestamps = fetch_headers(&*db, min_height, prev_block_height)?
            .iter()
            .map(|h| h.timestamp)
            .collect::<Vec<_>>();
        if timestamps.is_empty() {
            return Err(ChainStorageError::DataInconsistencyDetected {
                function: "prepare_new_block",
                details: format!(
                    "No timestamps were returned within heights {} - {} by the database despite the tip header height \
                     being {}",
                    min_height,
                    prev_block_height,
                    tip_header.height()
                ),
            });
        }

        let median_timestamp = calc_median_timestamp(&timestamps)?;
        // If someone advanced the median timestamp such that the local time is less than the median timestamp, we need
        // to increase the timestamp to be greater than the median timestamp otherwise the block wont be accepted by
        // nodes, they cannot increase it by more than the FTL so there is an upperbound here
        while median_timestamp >= header.timestamp {
            header.timestamp = median_timestamp
                .checked_add(EpochTime::from(1))
                .ok_or(ChainStorageError::UnexpectedResult("Timestamp overflowed".to_string()))?;
        }
        let mut block = Block { header, body };
        let roots = calculate_mmr_roots(&*db, self.rules(), &block)?;
        block.header.kernel_mr = roots.kernel_mr;
        block.header.kernel_mmr_size = roots.kernel_mmr_size;
        block.header.input_mr = roots.input_mr;
        block.header.output_mr = roots.output_mr;
        block.header.block_output_mr = roots.block_output_mr;
        block.header.output_smt_size = roots.output_smt_size;
        block.header.validator_node_mr = roots.validator_node_mr;
        block.header.validator_node_size = roots.validator_node_size;
        Ok(block)
    }

    /// `calculate_mmr_roots` takes a _pre-sorted_ block body and calculates the MMR roots for it.
    pub fn calculate_mmr_roots(&self, block: Block) -> Result<(Block, MmrRoots), ChainStorageError> {
        let db = self.db_read_access()?;
        if !block.body.is_sorted() {
            return Err(ChainStorageError::InvalidBlock(
                "calculate_mmr_roots expected a sorted block body, however the block body was not sorted".to_string(),
            ));
        };
        // let mut smt = self.smt_write_access()?;
        let mmr_roots = match calculate_mmr_roots(&*db, self.rules(), &block) {
            Ok(v) => v,
            Err(e) => {
                // if let ChainStorageError::CannotCalculateNonTipMmr(_) = e {
                //     warn!(target: LOG_TARGET, "Cannot calculate non tip MMR, this is expected if the block is not in
                // the main chain. SMT will be reset to the tip.");     // Do not recalc smt, it has not
                // changed.     return Err(e);
                // }
                // some error happend, lets reset the smt to its starting state
                // warn!(target: LOG_TARGET, "Reloading SMT into memory from stored db via calculate root due to '{}'",
                // e); *smt = db.calculate_tip_smt()?;
                return Err(e);
            },
        };
        Ok((block, mmr_roots))
    }

    /// Fetches the total merkle mountain range node count up to the specified height.
    pub fn fetch_mmr_size(&self, tree: MmrTree) -> Result<u64, ChainStorageError> {
        let db = self.db_read_access()?;
        db.fetch_mmr_size(tree)
    }

    pub fn get_validator_node(
        &self,
        sidechain_pk: Option<CompressedPublicKey>,
        public_key: CompressedPublicKey,
    ) -> Result<Option<ValidatorNodeRegistrationInfo>, ChainStorageError> {
        let db = self.db_read_access()?;
        db.get_validator_node(sidechain_pk.as_ref(), public_key)
    }

    /// Tries to add a block to the longest chain.
    ///
    /// The block is added to the longest chain if and only if
    ///   * Block block is not already in the database, AND
    ///   * The block is next in the chain, AND
    ///   * The Validator passes
    ///   * There are no problems with the database backend (e.g. disk full)
    ///
    /// If the block is _not_ next in the chain, the block will be added to the orphan pool if the orphan validator
    /// passes, and then the database is checked for whether there has been a chain reorganisation.
    ///
    /// # Returns
    ///
    /// An error is returned if
    /// * there was a problem accessing the database,
    /// * the validation fails
    ///
    /// Otherwise the function returns successfully.
    /// A successful return value can be one of
    ///   * `BlockExists`: the block has already been added; No action was taken.
    ///   * `Ok`: The block was added and all validation checks passed
    ///   * `OrphanBlock`: The block did not form part of the main chain and was added as an orphan.
    ///   * `ChainReorg`: The block was added, which resulted in a chain-reorg.
    ///
    /// If an error does occur while writing the new block parts, all changes are reverted before returning.
    pub fn add_block(&self, candidate_block: Arc<Block>) -> Result<BlockAddResult, ChainStorageError> {
        let timer = Instant::now();

        let block_hash = candidate_block.hash();
        if self.is_add_block_disabled() {
            warn!(
                target: LOG_TARGET,
                "add_block is disabled, node busy syncing. Ignoring candidate block #{} ({})",
                candidate_block.header.height,
                block_hash,
            );
            return Err(ChainStorageError::AddBlockOperationLocked);
        }

        let new_height = candidate_block.header.height;
        // This is important, we ask for a write lock to disable all read access to the db. The sync process sets
        // the add_block disable flag,  but we can have a race condition between the two especially
        // since the orphan validation can take some time during big blocks as it does Rangeproof and
        // metadata signature validation. Because the sync process first acquires a read_lock then a
        // write_lock, and the RWLock will be prioritised, the add_block write lock will be given out
        // before the sync write_lock.
        trace!(
            target: LOG_TARGET,
            "[add_block] waiting for write access to add block block #{} '{}'",
            new_height,
            block_hash.to_hex(),
        );
        let before_lock = timer.elapsed();
        let mut db = self.db_write_access()?;
        let after_lock = timer.elapsed();
        trace!(
            target: LOG_TARGET,
            "[add_block] acquired write access db lock for block #{} '{}' in {:.2?}",
            new_height,
            block_hash.to_hex(),
            after_lock.saturating_sub(before_lock),
        );

        // If this is true, we already got the header in our database due to header-sync, between us starting the
        // process of processing an incoming block and now getting a write-lock on the database. Block-sync will
        // download the body for us, so we can safely exit here.
        if db.contains(&DbKey::HeaderHash(block_hash))? {
            return Ok(BlockAddResult::BlockExists);
        }
        let (is_bad_block, reason) = db.bad_block_exists(block_hash)?;
        if is_bad_block {
            return Err(ChainStorageError::ValidationError {
                source: ValidationError::BadBlockFound {
                    hash: block_hash.to_hex(),
                    reason,
                },
            });
        }

        // the only fast check we can perform that is slightly expensive to fake is a min difficulty check, this is
        // done as soon as we receive the block before we do any processing on it. A proper proof of
        // work is done as soon as we can link it to the main chain. Full block validation only happens
        // when the proof of work is higher than the main chain and we want to add the block to the main
        // chain.
        let block_add_result = add_block(
            &mut *db,
            &self.config,
            &self.consensus_manager,
            &*self.validators.block,
            &*self.validators.header,
            self.consensus_manager.chain_strength_comparer(),
            candidate_block,
        )?;

        // If blocks were added and the node is in pruned mode, perform pruning
        if block_add_result.was_chain_modified() {
            info!(
                target: LOG_TARGET,
                "Best chain is now at height: {}",
                db.fetch_chain_metadata()?.best_block_height()
            );
            // Skip inline pruning if background pruning is already handling it
            if self.is_background_pruning.load(atomic::Ordering::SeqCst) {
                debug!(
                    target: LOG_TARGET,
                    "Background pruning is active, skipping inline prune_database_if_needed."
                );
            } else {
                prune_database_if_needed(&mut *db, self.config.pruning_horizon, self.config.pruning_interval)?;
            }
        }

        // Clean up orphan pool
        if let Err(e) = cleanup_orphans(&mut *db, self.config.orphan_storage_capacity) {
            warn!(target: LOG_TARGET, "Failed to clean up orphans: {e}");
        }

        debug!(
            target: LOG_TARGET,
            "[add_block] released write access db lock for block #{} in {:.2?}, `add_block` result: {}",
            new_height, timer.elapsed().saturating_sub(after_lock), block_add_result
        );
        Ok(block_add_result)
    }

    /// Clean out the entire orphan pool
    pub fn cleanup_orphans(&self) -> Result<(), ChainStorageError> {
        let mut db = self.db_write_access()?;
        cleanup_orphans(&mut *db, self.config.orphan_storage_capacity)?;
        Ok(())
    }

    pub fn clear_all_pending_headers(&self) -> Result<usize, ChainStorageError> {
        let db = self.db_write_access()?;
        db.clear_all_pending_headers()
    }

    /// Clean out the entire orphan pool
    pub fn cleanup_all_orphans(&self) -> Result<(), ChainStorageError> {
        let mut db = self.db_write_access()?;
        cleanup_orphans(&mut *db, 0)?;
        Ok(())
    }

    fn insert_block(&self, block: Arc<ChainBlock>) -> Result<(), ChainStorageError> {
        let mut db = self.db_write_access()?;

        let mut txn = DbTransaction::new();
        insert_best_block(&mut txn, block)?;
        db.write(txn)
    }

    fn store_pruning_horizon(&self, pruning_horizon: u64) -> Result<(), ChainStorageError> {
        let mut db = self.db_write_access()?;
        store_pruning_horizon(&mut *db, pruning_horizon)
    }

    /// Prunes the blockchain up to and including the given height
    pub fn prune_to_height(&self, height: u64) -> Result<(), ChainStorageError> {
        let mut db = self.db_write_access()?;
        prune_to_height(&mut *db, height)
    }

    /// Fetch a block from the blockchain database.
    ///
    /// # Returns
    /// This function returns an [HistoricalBlock] instance, which can be converted into a standard [Block], but also
    /// contains some additional information given its retrospective perspective that will be of interest to block
    /// explorers. For example, we know whether the outputs of this block have subsequently been spent or not and how
    /// many blocks have been mined on top of this block.
    ///
    /// `fetch_block` can return a `ChainStorageError` in the following cases:
    /// * There is an access problem on the back end.
    /// * The height is beyond the current chain tip.
    /// * The height is lower than the block at the pruning horizon.
    pub fn fetch_block(&self, height: u64, compact: bool) -> Result<HistoricalBlock, ChainStorageError> {
        let db = self.db_read_access()?;
        fetch_block(&*db, height, compact)
    }

    /// Returns the set of blocks according to the bounds
    pub fn fetch_blocks<T: RangeBounds<u64>>(
        &self,
        bounds: T,
        compact: bool,
    ) -> Result<Vec<HistoricalBlock>, ChainStorageError> {
        let db = self.db_read_access()?;
        let (mut start, mut end) = convert_to_option_bounds(bounds);

        let metadata = db.fetch_chain_metadata()?;

        if start.is_none() {
            // `(..n)` means fetch blocks with the lowest height possible until `n`
            start = Some(metadata.pruned_height());
        }
        if end.is_none() {
            // `(n..)` means fetch blocks until this node's tip
            end = Some(metadata.best_block_height());
        }

        let (start, end) = (start.unwrap(), end.unwrap());

        if end > metadata.best_block_height() {
            return Err(ChainStorageError::ValueNotFound {
                entity: "Block",
                field: "end height",
                value: end.to_string(),
            });
        }

        trace!(target: LOG_TARGET, "Fetching blocks {start}-{end}");
        let blocks = fetch_blocks(&*db, start, end, compact)?;
        trace!(target: LOG_TARGET, "Fetched {} block(s)", blocks.len());

        Ok(blocks)
    }

    /// Attempt to fetch the block corresponding to the provided hash from the main chain
    pub fn fetch_block_by_hash(
        &self,
        hash: BlockHash,
        compact: bool,
    ) -> Result<Option<HistoricalBlock>, ChainStorageError> {
        let db = self.db_read_access()?;
        fetch_block_by_hash(&*db, hash, compact)
    }

    /// Attempt to fetch the block corresponding to the provided hash from the main chain
    pub fn fetch_orphan_blocks(&self) -> Result<Vec<ChainHeader>, ChainStorageError> {
        let db = self.db_read_access()?;
        fetch_orphan_blocks(&*db)
    }

    /// Attempt to fetch the block corresponding to the provided kernel hash from the main chain, if the block is past
    /// pruning horizon, it will return Ok<None>
    pub fn fetch_block_with_kernel(
        &self,
        excess_sig: CompressedSignature,
    ) -> Result<Option<HistoricalBlock>, ChainStorageError> {
        let db = self.db_read_access()?;
        fetch_block_by_kernel_signature(&*db, excess_sig)
    }

    /// Attempt to fetch the block corresponding to the provided utxo hash from the main chain, if the block is past
    /// pruning horizon, it will return Ok<None>
    pub fn fetch_block_with_utxo(
        &self,
        commitment: CompressedCommitment,
    ) -> Result<Option<HistoricalBlock>, ChainStorageError> {
        let db = self.db_read_access()?;
        fetch_block_by_utxo_commitment(&*db, &commitment)
    }

    /// Returns true if this block exists in the chain, or is orphaned.
    pub fn chain_block_or_orphan_block_exists(&self, hash: BlockHash) -> Result<bool, ChainStorageError> {
        let db = self.db_read_access()?;
        // we need to check if the block accumulated data exists, and the header might exist without a body
        Ok(db.fetch_block_accumulated_data(&hash)?.is_some() || db.contains(&DbKey::OrphanBlock(hash))?)
    }

    /// Returns true if this block header in the chain, or is orphaned.
    pub fn chain_header_or_orphan_exists(&self, hash: BlockHash) -> Result<bool, ChainStorageError> {
        let db = self.db_read_access()?;
        Ok(db.contains(&DbKey::HeaderHash(hash))? || db.contains(&DbKey::OrphanBlock(hash))?)
    }

    /// Returns true if this block exists in the chain, or is orphaned.
    pub fn bad_block_exists(&self, hash: BlockHash) -> Result<(bool, String), ChainStorageError> {
        let db = self.db_read_access()?;
        db.bad_block_exists(hash)
    }

    /// Atomically commit the provided transaction to the database backend. This function does not update the metadata.
    pub fn commit(&self, txn: DbTransaction) -> Result<(), ChainStorageError> {
        let mut db = self.db_write_access()?;
        db.write(txn)
    }

    /// Rewind the blockchain state to the block height given and return the blocks that were removed and orphaned.
    ///
    /// The operation will fail if
    /// * The block height is in the future
    pub fn rewind_to_height(&self, height: u64) -> Result<Vec<Arc<ChainBlock>>, ChainStorageError> {
        let mut db = self.db_write_access()?;
        rewind_to_height(&mut *db, height)
    }

    /// Rewind the blockchain state to the block hash making the block at that hash the new tip.
    /// Returns the removed blocks.
    ///
    /// The operation will fail if
    /// * The block hash does not exist
    /// * The block hash is before the horizon block height determined by the pruning horizon
    pub fn rewind_to_hash(&self, hash: BlockHash) -> Result<Vec<Arc<ChainBlock>>, ChainStorageError> {
        let mut db = self.db_write_access()?;
        rewind_to_hash(&mut *db, hash)
    }

    /// This method will compare all chain tips the node currently knows about. This includes
    /// all tips in the orphan pool and the main active chain. It will swap the main active
    /// chain to the highest pow chain
    /// This is typically used when an attempted sync failed to sync to the expected height and
    /// we are not sure if the new chain is higher than the old one.
    pub fn swap_to_highest_pow_chain(&self) -> Result<(), ChainStorageError> {
        let mut db = self.db_write_access()?;
        swap_to_highest_pow_chain(
            &mut *db,
            &self.config,
            &self.consensus_manager,
            &*self.validators.block,
            self.consensus_manager.chain_strength_comparer(),
        )?;
        Ok(())
    }

    pub fn fetch_horizon_data(&self) -> Result<HorizonData, ChainStorageError> {
        let db = self.db_read_access()?;
        Ok(db.fetch_horizon_data()?.unwrap_or_default())
    }

    pub fn fetch_horizon_sync_output_checkpoint(
        &self,
    ) -> Result<Option<HorizonSyncOutputCheckpoint>, ChainStorageError> {
        let db = self.db_read_access()?;
        db.fetch_horizon_sync_output_checkpoint()
    }

    pub fn verify_horizon_sync_output_root(&self, expected_root: HashOutput) -> Result<(), ChainStorageError> {
        let db = self.db_read_access()?;
        db.verify_horizon_sync_output_root(expected_root)
    }

    pub fn get_stats(&self) -> Result<DbBasicStats, ChainStorageError> {
        let lock = self.db_read_access()?;
        lock.get_stats()
    }

    /// Returns total size information about each internal database. This call may be very slow and will obtain a read
    /// lock for the duration.
    pub fn fetch_total_size_stats(&self) -> Result<DbTotalSizeStats, ChainStorageError> {
        let lock = self.db_read_access()?;
        lock.fetch_total_size_stats()
    }

    pub fn fetch_all_reorgs(&self) -> Result<Vec<Reorg>, ChainStorageError> {
        let db = self.db_read_access()?;
        db.fetch_all_reorgs()
    }

    pub fn clear_all_reorgs(&self) -> Result<(), ChainStorageError> {
        let mut db = self.db_write_access()?;
        let mut txn = DbTransaction::new();
        txn.clear_all_reorgs();
        db.write(txn)
    }

    pub fn fetch_all_active_validator_nodes(
        &self,
        height: u64,
    ) -> Result<Vec<ValidatorNodeRegistrationInfo>, ChainStorageError> {
        let db = self.db_read_access()?;
        db.fetch_all_active_validator_nodes(height)
    }

    pub fn fetch_all_orphans(&self) -> Result<Vec<ChainHeader>, ChainStorageError> {
        let db = self.db_read_access()?;
        db.fetch_all_orphans()
    }

    pub fn fetch_active_validator_nodes(
        &self,
        height: u64,
        sidechain_pk: Option<CompressedPublicKey>,
    ) -> Result<Vec<ValidatorNodeRegistrationInfo>, ChainStorageError> {
        let db = self.db_read_access()?;
        db.fetch_active_validator_nodes(sidechain_pk.as_ref(), height)
    }

    pub fn fetch_validators_activating_in_epoch(
        &self,
        sidechain_pk: Option<CompressedPublicKey>,
        epoch: VnEpoch,
    ) -> Result<Vec<ValidatorNodeRegistrationInfo>, ChainStorageError> {
        let db = self.db_read_access()?;
        db.fetch_validators_activating_in_epoch(sidechain_pk.as_ref(), epoch)
    }

    pub fn fetch_validators_exiting_in_epoch(
        &self,
        sidechain_pk: Option<CompressedPublicKey>,
        epoch: VnEpoch,
    ) -> Result<Vec<ValidatorNodeRegistrationInfo>, ChainStorageError> {
        let db = self.db_read_access()?;
        db.fetch_validators_exiting_in_epoch(sidechain_pk.as_ref(), epoch)
    }

    pub fn fetch_template_registrations<T: RangeBounds<u64>>(
        &self,
        range: T,
    ) -> Result<Vec<TemplateRegistrationEntry>, ChainStorageError> {
        let db = self.db_read_access()?;
        let (start, mut end) = convert_to_option_bounds(range);
        if end.is_none() {
            // `(n..)` means fetch block headers until this node's tip
            end = Some(db.fetch_last_header()?.height);
        }
        let (start, end) = (start.unwrap_or(0), end.unwrap());
        db.fetch_template_registrations(start, end)
    }

    pub fn generate_kernel_merkle_proof(
        &self,
        excess_sig: CompressedSignature,
    ) -> Result<KernelMerkleProof, ChainStorageError> {
        const OPERATION: &str = "generate_kernel_merkle_proof";
        let db = self.db_read_access()?;

        let (kernel, block_hash) =
            db.fetch_kernel_by_excess_sig(&excess_sig)?
                .ok_or_else(|| ChainStorageError::ValueNotFound {
                    entity: "TransactionKernel",
                    field: "excess_sig",
                    value: excess_sig.get_signature().to_hex(),
                })?;

        let block = fetch_block_by_hash(&*db, block_hash, true)?.ok_or_else(|| {
            ChainStorageError::DataInconsistencyDetected {
                function: OPERATION,
                details: format!(
                    "Kernel with excess sig {} found in database, but block not found in block database",
                    excess_sig.get_signature().reveal()
                ),
            }
        })?;

        let BlockAccumulatedData { kernels, .. } = db
            .fetch_block_accumulated_data(&block.header().prev_hash)?
            .ok_or_else(|| ChainStorageError::ValueNotFound {
                entity: "BlockAccumulatedData",
                field: "block_hash",
                value: block_hash.to_hex(),
            })?;

        info!(
            target: LOG_TARGET,
            "Generating kernel merkle proof for kernel in block #{} ({}) MMR size: {}",
            block.header().height,
            block_hash,
            block.header().kernel_mmr_size,
        );

        let mut kernel_mmr = PrunedKernelMmr::new(kernels);

        for kernel in block.block().body.kernels() {
            let hash = kernel.hash();
            kernel_mmr.push(hash.to_vec())?;
        }

        let kernel_hash = kernel.hash();
        let leaf_index = kernel_mmr.find_leaf_index(kernel_hash.as_slice())?.ok_or_else(|| {
            ChainStorageError::DataInconsistencyDetected {
                function: OPERATION,
                details: format!(
                    "Kernel with hash {} found in database, but not found in MMR",
                    kernel_hash
                ),
            }
        })?;

        let merkle_proof = MerkleProof::for_leaf_node(&kernel_mmr, leaf_index)?;
        Ok(KernelMerkleProof {
            merkle_proof,
            leaf_index,
            kernel_hash,
            block_hash,
            block_height: block.header().height,
        })
    }
}

fn unexpected_result<T>(request: DbKey, response: DbValue) -> Result<T, ChainStorageError> {
    let msg = format!("Unexpected result for database query {request}. Response: {response}");
    error!(target: LOG_TARGET, "{msg}");
    Err(ChainStorageError::UnexpectedResult(msg))
}

/// Container struct for MMR roots
#[derive(Debug, Clone)]
pub struct MmrRoots {
    pub kernel_mr: FixedHash,
    pub kernel_mmr_size: u64,
    pub input_mr: FixedHash,
    pub output_mr: FixedHash,
    pub block_output_mr: FixedHash,
    pub output_smt_size: u64,
    pub validator_node_mr: FixedHash,
    pub validator_node_size: u64,
}

impl std::fmt::Display for MmrRoots {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "MMR Roots")?;
        writeln!(f, "Input MR        : {}", self.input_mr)?;
        writeln!(f, "Kernel MR       : {}", self.kernel_mr)?;
        writeln!(f, "Kernel MMR Size : {}", self.kernel_mmr_size)?;
        writeln!(f, "Output MR       : {}", self.output_mr)?;
        writeln!(f, "Block Output MR : {}", self.block_output_mr)?;
        writeln!(f, "Output SMT Size : {}", self.output_smt_size)?;
        writeln!(f, "Validator MR    : {}", self.validator_node_mr)?;
        Ok(())
    }
}

#[allow(clippy::too_many_lines)]
#[allow(clippy::similar_names)]
pub fn calculate_mmr_roots<T: BlockchainBackend>(
    db: &T,
    rules: &BaseNodeConsensusManager,
    block: &Block,
) -> Result<MmrRoots, ChainStorageError> {
    let header = &block.header;
    let body = &block.body;

    let (smt_reader, current_version) = db.create_smt_reader()?;
    let metadata = db.fetch_chain_metadata()?;
    if header.prev_hash != *metadata.best_block_hash() {
        return Err(ChainStorageError::CannotCalculateNonTipMmr(format!(
            "Block (#{}) is not building on tip, previous hash is {} but the current tip is #{} {}",
            header.height,
            header.prev_hash,
            metadata.best_block_height(),
            metadata.best_block_hash(),
        )));
    }

    let BlockAccumulatedData { kernels, .. } =
        db.fetch_block_accumulated_data(&header.prev_hash)?
            .ok_or_else(|| ChainStorageError::ValueNotFound {
                entity: "BlockAccumulatedData",
                field: "header_hash",
                value: header.prev_hash.to_hex(),
            })?;

    let mut kernel_mmr = PrunedKernelMmr::new(kernels);
    let mut input_mmr = PrunedInputMmr::new(PrunedHashSet::default());
    let mut block_output_mmr = PrunedOutputMmr::new(PrunedHashSet::default());
    let mut normal_output_mmr = PrunedOutputMmr::new(PrunedHashSet::default());
    let output_smt = JellyfishMerkleTree::<_, SmtHasher>::new(&smt_reader);

    for kernel in body.kernels() {
        kernel_mmr.push(kernel.hash().to_vec())?;
    }

    let mut batch = Vec::with_capacity(body.outputs().len().saturating_add(body.inputs().len()));
    for output in body.outputs() {
        if output.features.is_coinbase() {
            block_output_mmr.push(output.hash().to_vec())?;
        } else {
            normal_output_mmr.push(output.hash().to_vec())?;
        }
        if !output.is_burned() {
            let smt_key = KeyHash(output.commitment.as_bytes().try_into().expect("commitment is 32 bytes"));
            let smt_value = output.smt_hash(header.height);

            batch.push((smt_key, Some(smt_value.to_vec())));
        }
    }
    block_output_mmr.push(normal_output_mmr.get_merkle_root()?.to_vec())?;

    for input in body.inputs() {
        input_mmr.push(input.canonical_hash().to_vec())?;
        let smt_key = KeyHash(
            input
                .commitment()?
                .as_bytes()
                .try_into()
                .expect("Commitment is 32 bytes"),
        );
        batch.push((smt_key, None));
    }

    let block_height = block.header.height;
    let epoch_len = rules.consensus_constants(block_height).epoch_length();
    let tip_header = fetch_header(db, block_height.saturating_sub(1))?;
    let (validator_node_mr, validator_node_size) = if block_height.is_multiple_of(epoch_len) {
        // At epoch boundary, the MR is rebuilt from the current validator set
        let validator_nodes = db.fetch_all_active_validator_nodes(block_height)?;
        (calculate_validator_node_mr(&validator_nodes)?, validator_nodes.len())
    } else {
        // MR is unchanged except for epoch boundary
        // Active validator set never changes within epochs, so we can reuse the previous block VN MR
        // TODO: fetch a count for the active validator set
        (tip_header.validator_node_mr, 0)
    };

    let block_output_mr = block_output_mr_hash_from_pruned_mmr(&block_output_mmr)?;

    let (output_smt_root, changes) = output_smt
        .put_value_set(batch, current_version.saturating_add(1))
        .map_err(ChainStorageError::JellyfishMerkleTreeError)?;

    let mut size = tip_header.output_smt_size;
    size = size.saturating_add(changes.node_stats.first().map(|s| s.new_leaves).unwrap_or(0) as u64);
    size = size.saturating_sub(changes.node_stats.first().map(|s| s.stale_leaves).unwrap_or(0) as u64);

    let mmr_roots = MmrRoots {
        kernel_mr: kernel_mr_hash_from_pruned_mmr(&kernel_mmr)?,
        kernel_mmr_size: kernel_mmr.get_leaf_count()? as u64,
        input_mr: input_mr_hash_from_pruned_mmr(&input_mmr)?,
        output_mr: FixedHash::from(output_smt_root.0),
        block_output_mr,
        output_smt_size: size,
        validator_node_mr,
        validator_node_size: validator_node_size as u64,
    };
    Ok(mmr_roots)
}

pub fn calculate_validator_node_mr(
    validator_nodes: &[ValidatorNodeRegistrationInfo],
) -> Result<FixedHash, ChainStorageError> {
    if validator_nodes.is_empty() {
        return Ok(VALIDATOR_MR_EMPTY_PLACEHOLDER_HASH);
    }

    struct EmptyJmtStore;

    impl TreeReader for EmptyJmtStore {
        fn get_node_option(&self, _node_key: &NodeKey) -> anyhow::Result<Option<Node>> {
            Ok(None)
        }

        fn get_value_option(&self, _max_version: Version, _key_hash: KeyHash) -> anyhow::Result<Option<OwnedValue>> {
            Ok(None)
        }

        fn get_rightmost_leaf(&self) -> anyhow::Result<Option<(NodeKey, LeafNode)>> {
            Ok(None)
        }
    }
    fn hash_node((pk, s): &(&CompressedPublicKey, &[u8; 32])) -> KeyHash {
        KeyHash(
            DomainSeparatedConsensusHasher::<TransactionHashDomain, Blake2b<U32>>::new("validator_node")
                .chain(pk)
                .chain(s)
                .finalize()
                .into(),
        )
    }

    fn hash_sid(sid: Option<&CompressedPublicKey>) -> KeyHash {
        KeyHash(
            DomainSeparatedConsensusHasher::<TransactionHashDomain, Blake2b<U32>>::new("validator_node_sid")
                .chain(&sid)
                .finalize()
                .into(),
        )
    }

    // TODO: update validator JMTs as outputs are added to the blockchain
    // Alternatively, we could use the utxo JMT for inclusion proofs
    let mut validator_sets = Vec::<(Option<&CompressedPublicKey>, Vec<(&CompressedPublicKey, &[u8; 32])>)>::new();
    for ValidatorNodeRegistrationInfo {
        public_key: pk,
        sidechain_id,
        shard_key,
        ..
    } in validator_nodes
    {
        // NOTE: this depends on validator_nodes being ordered by sidechain ID (we happen to know this is the case, i.e
        // the natural LMDB order)
        match validator_sets.last_mut() {
            Some((sid, set)) if *sid == sidechain_id.as_ref() => {
                // If the last entry has the same sidechain ID, we can just push to it
                set.push((pk, shard_key));
            },
            Some(_) | None => {
                // If there are no entries or the sidechain id has changed, we create a new one
                validator_sets.push((sidechain_id.as_ref(), vec![(pk, shard_key)]));
            },
        }
    }
    let mut roots = Vec::with_capacity(validator_sets.len());
    for (sidechain_pk, set) in validator_sets {
        let sidechain_mt = JellyfishMerkleTree::<_, ValidatorNodeJmtHasher>::new(&EmptyJmtStore);
        let (root, _) = sidechain_mt
            .put_value_set(
                set.iter()
                    .map(|pk_and_shard_key| (hash_node(pk_and_shard_key), Some(vec![]))),
                1,
            )
            .map_err(ChainStorageError::JellyfishMerkleTreeError)?;
        roots.push((hash_sid(sidechain_pk), root));
    }

    let root_mt = JellyfishMerkleTree::<_, ValidatorNodeJmtHasher>::new(&EmptyJmtStore);
    let (root_hash, _) = root_mt
        .put_value_set(roots.into_iter().map(|(sid, root)| (sid, Some(root.0.to_vec()))), 1)
        .map_err(ChainStorageError::JellyfishMerkleTreeError)?;
    Ok(FixedHash::from(root_hash.0))
}

pub fn fetch_header<T: BlockchainBackend>(db: &T, block_num: u64) -> Result<BlockHeader, ChainStorageError> {
    fetch!(db, block_num, HeaderHeight)
}

pub fn fetch_headers<T: BlockchainBackend>(
    db: &T,
    mut start: u64,
    mut end_inclusive: u64,
) -> Result<Vec<BlockHeader>, ChainStorageError> {
    let is_reversed = start > end_inclusive;

    if is_reversed {
        mem::swap(&mut end_inclusive, &mut start);
    }

    // Allow the headers to be returned in reverse order
    #[allow(clippy::cast_possible_truncation)]
    let mut headers = Vec::with_capacity(end_inclusive.saturating_sub(start) as usize);
    for h in start..=end_inclusive {
        match db.fetch(&DbKey::HeaderHeight(h))? {
            Some(DbValue::HeaderHeight(header)) => {
                headers.push(*header);
            },
            Some(_) => unreachable!(),
            None => break,
        }
    }

    if is_reversed {
        Ok(headers.into_iter().rev().collect())
    } else {
        Ok(headers)
    }
}

pub fn fetch_chain_headers<T: BlockchainBackend>(
    db: &T,
    start: u64,
    end_inclusive: u64,
) -> Result<Vec<ChainHeader>, ChainStorageError> {
    if start > end_inclusive {
        return Err(ChainStorageError::InvalidQuery(
            "end_inclusive must be greater than start".to_string(),
        ));
    }

    #[allow(clippy::cast_possible_truncation)]
    let mut headers = Vec::with_capacity(end_inclusive.saturating_sub(start) as usize);
    for h in start..=end_inclusive {
        match db.fetch_chain_header_by_height(h) {
            Ok(header) => {
                headers.push(header);
            },
            Err(ChainStorageError::ValueNotFound { .. }) => break,
            Err(e) => return Err(e),
        }
    }

    Ok(headers)
}

fn insert_headers<T: BlockchainBackend>(db: &mut T, headers: Vec<ChainHeader>) -> Result<(), ChainStorageError> {
    let mut txn = DbTransaction::new();
    headers.into_iter().for_each(|chain_header| {
        txn.insert_chain_header(chain_header);
    });
    db.write(txn)
}

fn fetch_header_by_block_hash<T: BlockchainBackend>(
    db: &T,
    hash: BlockHash,
) -> Result<Option<BlockHeader>, ChainStorageError> {
    try_fetch!(db, hash, HeaderHash)
}

fn fetch_orphan<T: BlockchainBackend>(db: &T, hash: BlockHash) -> Result<Block, ChainStorageError> {
    fetch!(db, hash, OrphanBlock)
}

fn add_block<T: BlockchainBackend>(
    db: &mut T,
    config: &BlockchainDatabaseConfig,
    consensus_manager: &BaseNodeConsensusManager,
    block_validator: &dyn CandidateBlockValidator<T>,
    header_validator: &dyn HeaderChainLinkedValidator<T>,
    chain_strength_comparer: &dyn ChainStrengthComparer,
    candidate_block: Arc<Block>,
    // smt_writer: &mut LmdbTreeWriter,
) -> Result<BlockAddResult, ChainStorageError> {
    handle_possible_reorg(
        db,
        config,
        consensus_manager,
        block_validator,
        header_validator,
        chain_strength_comparer,
        candidate_block,
        // smt,
    )
}

/// Adds a new block onto the chain tip and sets it to the best block.
fn insert_best_block(txn: &mut DbTransaction, block: Arc<ChainBlock>) -> Result<(), ChainStorageError> {
    let block_hash = block.accumulated_data().hash;
    debug!(
        target: LOG_TARGET,
        "Storing new block #{} `{}`",
        block.header().height,
        block_hash,
    );
    let height = block.height();
    let timestamp = block.header().timestamp().as_u64();
    let accumulated_difficulty = block.accumulated_data().total_accumulated_difficulty;
    let expected_prev_best_block = block.block().header.prev_hash;
    txn.insert_chain_header(block.to_chain_header())
        .insert_tip_block_body(block)
        .set_best_block(
            height,
            block_hash,
            accumulated_difficulty,
            expected_prev_best_block,
            timestamp,
        );

    Ok(())
}

fn store_pruning_horizon<T: BlockchainBackend>(db: &mut T, pruning_horizon: u64) -> Result<(), ChainStorageError> {
    let mut txn = DbTransaction::new();
    txn.set_pruning_horizon(pruning_horizon);
    db.write(txn)
}

/// One header that belongs in a difficulty window, plus its position in the walk so that the backoff run can be
/// replayed over it in the right order.
struct WindowHeader {
    /// Index into [`DifficultyWindowWalk::algos`]
    index: usize,
    height: u64,
    algo: PowAlgorithm,
    timestamp: EpochTime,
    target_difficulty: Difficulty,
}

/// The result of walking the chain backwards far enough to fill the difficulty window(s).
///
/// Only the PoW algorithm of each walked header is retained (one byte each), because that is all the backoff run
/// needs; the full headers are deliberately *not* buffered. The walk can run all the way to genesis when one
/// permitted algorithm is under-mined, so buffering whole headers would make this a memory amplification vector for
/// any caller reachable over gRPC.
struct DifficultyWindowWalk {
    /// PoW algorithm of every walked header, newest first.
    algos: Vec<PowAlgorithm>,
    /// The headers that belong in a difficulty window, newest first. Bounded by `algo count * window capacity`.
    window: Vec<WindowHeader>,
}

/// The single database operation the difficulty window walk needs. Factored into its own trait so that the walk and
/// its replay - which are consensus critical and must agree with the incremental header sync path - can be tested
/// against an in-memory chain.
pub(crate) trait ChainHeaderSource {
    fn fetch_chain_header(&self, hash: &HashOutput) -> Result<ChainHeader, ChainStorageError>;
}

impl<T: BlockchainBackend> ChainHeaderSource for T {
    fn fetch_chain_header(&self, hash: &HashOutput) -> Result<ChainHeader, ChainStorageError> {
        // The block may be in the chained orphan pool or in the main chain
        self.fetch_chain_header_in_all_chains(hash)
    }
}

/// Walks the chain backwards from `start_header`, recording the PoW algorithm of every header and the data of the
/// headers that belong in a difficulty window, plus [`MAX_BACKOFF_RUN_LOOKBACK`] further headers of lookback.
///
/// The lookback is what makes the TIP-RFC-MT-0004 backoff modifier of the *oldest* window entry exact: if the whole
/// lookback is a single algorithm, the run is at least `MAX_BACKOFF_RUN_LOOKBACK + 1` long and the modifier is capped
/// anyway, so no deeper walk is ever required.
fn walk_difficulty_window<T, F>(
    db: &T,
    start_header: ChainHeader,
    mut take: F,
) -> Result<DifficultyWindowWalk, ChainStorageError>
where
    T: ChainHeaderSource + ?Sized,
    // Returns (belongs in a window, all windows are now full)
    F: FnMut(PowAlgorithm) -> (bool, bool),
{
    let mut walk = DifficultyWindowWalk {
        algos: Vec::new(),
        window: Vec::new(),
    };
    let mut header = start_header;
    let mut prev_hash;
    loop {
        let algo = header.header().pow_algo();
        let (in_window, is_full) = take(algo);
        if in_window {
            walk.window.push(WindowHeader {
                index: walk.algos.len(),
                height: header.height(),
                algo,
                timestamp: header.header().timestamp(),
                target_difficulty: header.accumulated_data().target_difficulty,
            });
        }
        walk.algos.push(algo);
        let height = header.height();
        prev_hash = header.header().prev_hash;
        if height == 0 || is_full {
            break;
        }
        header = db.fetch_chain_header(&prev_hash)?;
    }

    let mut height = header.height();
    for _ in 0..MAX_BACKOFF_RUN_LOOKBACK {
        if height == 0 {
            break;
        }
        header = db.fetch_chain_header(&prev_hash)?;
        walk.algos.push(header.header().pow_algo());
        height = header.height();
        prev_hash = header.header().prev_hash;
    }

    Ok(walk)
}

/// Replays a backward walk oldest -> newest, so that each window entry's backoff modifier is derived from its own
/// predecessors, and hands each window entry to `add`. Returns the seeded tracker.
fn replay_difficulty_window<F>(
    walk: &DifficultyWindowWalk,
    mut add: F,
) -> Result<PowBackoffTracker, ChainStorageError>
where
    F: FnMut(&WindowHeader, &PowBackoffTracker) -> Result<(), ChainStorageError>,
{
    let mut tracker = PowBackoffTracker::new();
    let mut window = walk.window.iter().rev().peekable();
    for (index, algo) in walk.algos.iter().enumerate().rev() {
        if window.peek().is_some_and(|entry| entry.index == index) {
            let entry = window.next().expect("just peeked");
            add(entry, &tracker)?;
        }
        tracker.push(*algo);
    }
    Ok(tracker)
}

#[allow(clippy::ptr_arg)]
pub fn fetch_target_difficulty_for_next_block<T: BlockchainBackend>(
    db: &T,
    consensus_manager: &BaseNodeConsensusManager,
    pow_algo: PowAlgorithm,
    current_block_hash: &HashOutput,
) -> Result<TargetDifficultyWindow, ChainStorageError> {
    target_difficulty_for_next_block(db, consensus_manager, pow_algo, current_block_hash)
}

pub(crate) fn target_difficulty_for_next_block<T: ChainHeaderSource + ?Sized>(
    db: &T,
    consensus_manager: &BaseNodeConsensusManager,
    pow_algo: PowAlgorithm,
    current_block_hash: &HashOutput,
) -> Result<TargetDifficultyWindow, ChainStorageError> {
    let start_header = db.fetch_chain_header(current_block_hash)?;
    let next_height = start_header.height().saturating_add(1);
    let constants = consensus_manager.consensus_constants(next_height);
    let capacity = difficulty_window_capacity(constants)?;
    let mut target_difficulties = consensus_manager
        .new_target_difficulty(pow_algo, next_height)
        .map_err(ChainStorageError::UnexpectedResult)?;

    // Pass 1: walk backwards, marking the headers of this algorithm that belong in the window.
    let mut count = 0usize;
    let walk = walk_difficulty_window(db, start_header, |algo| {
        let in_window = algo == pow_algo && count < capacity;
        if in_window {
            count = count.saturating_add(1);
        }
        (in_window, count >= capacity)
    })?;

    // Pass 2: replay oldest -> newest so that each block's backoff modifier is derived from its own predecessors.
    let tracker = replay_difficulty_window(&walk, |entry, tracker| {
        let header_constants = consensus_manager.consensus_constants(entry.height);
        let modifier = tracker.modifier_for(entry.algo, header_constants.pow_backoff_cap());
        let adjusted_target = adjust_target(
            entry.target_difficulty,
            modifier,
            header_constants.min_pow_difficulty(entry.algo),
            header_constants.max_pow_difficulty(entry.algo),
        );
        // LWMA works with the "newest" value being at the back of the array
        target_difficulties.add_back(entry.timestamp, entry.target_difficulty, adjusted_target);
        Ok(())
    })?;
    target_difficulties.set_next_modifier(tracker.modifier_for(pow_algo, constants.pow_backoff_cap()));

    Ok(target_difficulties)
}

#[allow(clippy::ptr_arg)]
pub fn fetch_target_difficulties_for_next_block<T: BlockchainBackend>(
    db: &T,
    consensus_manager: &BaseNodeConsensusManager,
    current_block_hash: &HashOutput,
) -> Result<TargetDifficulties, ChainStorageError> {
    target_difficulties_for_next_block(db, consensus_manager, current_block_hash)
}

pub(crate) fn target_difficulties_for_next_block<T: ChainHeaderSource + ?Sized>(
    db: &T,
    consensus_manager: &BaseNodeConsensusManager,
    current_block_hash: &HashOutput,
) -> Result<TargetDifficulties, ChainStorageError> {
    let start_header = db.fetch_chain_header(current_block_hash)?;
    let next_height = start_header.height().saturating_add(1);
    let constants = consensus_manager.consensus_constants(next_height);
    let capacity = difficulty_window_capacity(constants)?;
    let mut targets =
        TargetDifficulties::new(consensus_manager, next_height).map_err(ChainStorageError::UnexpectedResult)?;

    // Pass 1: walk backwards, marking the headers that belong in one of the per-algorithm windows.
    let mut counts: HashMap<PowAlgorithm, usize> = constants
        .current_permitted_pow_algos()
        .into_iter()
        .map(|algo| (algo, 0usize))
        .collect();
    let walk = walk_difficulty_window(db, start_header, |algo| {
        let in_window = match counts.get_mut(&algo) {
            Some(count) if *count < capacity => {
                *count = count.saturating_add(1);
                true
            },
            _ => false,
        };
        (in_window, counts.values().all(|count| *count >= capacity))
    })?;

    // Pass 2: replay oldest -> newest so that each block's backoff modifier is derived from its own predecessors.
    // `TargetDifficulties` owns an equivalent tracker, so the window entries are pushed through it and the headers
    // that are not in any window only advance the run.
    let mut next_index = walk.algos.len();
    for entry in walk.window.iter().rev() {
        for index in (entry.index.saturating_add(1)..next_index).rev() {
            targets.push_algo(*walk.algos.get(index).expect("index is in range"));
        }
        targets
            .add_back_parts(
                entry.algo,
                entry.timestamp,
                entry.target_difficulty,
                consensus_manager.consensus_constants(entry.height),
            )
            .map_err(ChainStorageError::UnexpectedResult)?;
        next_index = entry.index;
    }
    for index in (0..next_index).rev() {
        targets.push_algo(*walk.algos.get(index).expect("index is in range"));
    }

    Ok(targets)
}

fn difficulty_window_capacity(constants: &ConsensusConstants) -> Result<usize, ChainStorageError> {
    Ok(usize::try_from(constants.difficulty_block_window())
        .map_err(|e| ChainStorageError::UnexpectedResult(format!("difficulty block window exceeds usize::MAX: {e}")))?
        .saturating_add(1))
}

fn fetch_block<T: BlockchainBackend>(db: &T, height: u64, compact: bool) -> Result<HistoricalBlock, ChainStorageError> {
    let mark = Instant::now();
    let (tip_height, _is_pruned) = check_for_valid_height(db, height)?;
    let chain_header = db.fetch_chain_header_by_height(height)?;
    let (header, accumulated_data) = chain_header.into_parts();
    let kernels = db.fetch_kernels_in_block(&accumulated_data.hash)?;
    let outputs = db.fetch_outputs_in_block(&accumulated_data.hash)?;
    // Fetch inputs from the backend and populate their spent_output data if available
    let inputs = db
        .fetch_inputs_in_block(&accumulated_data.hash)?
        .into_iter()
        .map(|mut compact_input| {
            if compact {
                return Ok(compact_input);
            }
            // Only the output's contents are used to hydrate the input, and those are identical for
            // every index entry of the same output hash, so taking any entry is equivalent.
            let utxo_mined_info = match db
                .fetch_outputs(&compact_input.output_hash())
                .map(|o| o.into_iter().next())
            {
                Ok(Some(o)) => o,
                Ok(None) => {
                    return Err(ChainStorageError::InvalidBlock(
                        "An Input in a block doesn't contain a matching spending output".to_string(),
                    ));
                },
                Err(e) => return Err(e),
            };

            compact_input.add_output_data(utxo_mined_info.output);
            Ok(compact_input)
        })
        .collect::<Result<Vec<TransactionInput>, _>>()?;

    let block = header
        .into_builder()
        .add_inputs(inputs)
        .add_outputs(outputs)
        .add_kernels(kernels)
        .build();
    trace!(
        target: LOG_TARGET,
        "Fetched block at height:{} in {:.0?}",
        height,
        mark.elapsed()
    );
    Ok(HistoricalBlock::new(
        block,
        tip_height.saturating_sub(height).saturating_add(1),
        accumulated_data,
    ))
}

fn fetch_blocks<T: BlockchainBackend>(
    db: &T,
    start: u64,
    end_inclusive: u64,
    compact: bool,
) -> Result<Vec<HistoricalBlock>, ChainStorageError> {
    (start..=end_inclusive).map(|i| fetch_block(db, i, compact)).collect()
}

fn fetch_block_by_kernel_signature<T: BlockchainBackend>(
    db: &T,
    excess_sig: CompressedSignature,
) -> Result<Option<HistoricalBlock>, ChainStorageError> {
    match db.fetch_kernel_by_excess_sig(&excess_sig) {
        Ok(kernel) => match kernel {
            Some((_kernel, hash)) => fetch_block_by_hash(db, hash, false),
            None => Ok(None),
        },
        Err(_) => Err(ChainStorageError::ValueNotFound {
            entity: "Kernel",
            field: "Excess sig",
            value: excess_sig.get_signature().to_hex(),
        }),
    }
}

fn fetch_block_by_utxo_commitment<T: BlockchainBackend>(
    db: &T,
    commitment: &CompressedCommitment,
) -> Result<Option<HistoricalBlock>, ChainStorageError> {
    let output = db.fetch_unspent_output_hash_by_commitment(commitment)?;
    match output {
        // The hash came from the commitment index; if it is indexed under several headers, take the
        // last-mined entry.
        Some(hash) => {
            let mut outputs = db.fetch_outputs(&hash)?;
            outputs.sort_by_key(|o| o.mined_height);
            match outputs.into_iter().next_back() {
                Some(mined_info) => fetch_block_by_hash(db, mined_info.header_hash, false),
                None => Ok(None),
            }
        },
        None => Ok(None),
    }
}

fn fetch_block_by_hash<T: BlockchainBackend>(
    db: &T,
    hash: BlockHash,
    compact: bool,
) -> Result<Option<HistoricalBlock>, ChainStorageError> {
    if let Some(header) = fetch_header_by_block_hash(db, hash)? {
        return Ok(Some(fetch_block(db, header.height, compact)?));
    }
    Ok(None)
}

fn fetch_orphan_blocks<T: BlockchainBackend>(db: &T) -> Result<Vec<ChainHeader>, ChainStorageError> {
    db.fetch_all_orphans()
}

fn check_for_valid_height<T: BlockchainBackend>(db: &T, height: u64) -> Result<(u64, bool), ChainStorageError> {
    let metadata = db.fetch_chain_metadata()?;
    let tip_height = metadata.best_block_height();
    if height > tip_height {
        return Err(ChainStorageError::InvalidQuery(format!(
            "Cannot get block at height {height}. Chain tip is at {tip_height}"
        )));
    }
    let pruned_height = metadata.pruned_height();
    Ok((tip_height, height < pruned_height))
}

/// Removes blocks from the db from current tip to specified height.
/// Returns the blocks removed, ordered from tip to height.
#[allow(clippy::too_many_lines)]
pub(crate) fn rewind_to_height<T: BlockchainBackend>(
    db: &mut T,
    target_height: u64,
) -> Result<Vec<Arc<ChainBlock>>, ChainStorageError> {
    let last_header = db.fetch_last_header()?;
    // Delete headers
    let last_header_height = last_header.height;
    let metadata = db.fetch_chain_metadata()?;
    let last_block_height = metadata.best_block_height();
    // We use the cmp::max value here because we'll only delete headers here and leave remaining headers to be deleted
    // with the whole block
    let steps_back = last_header_height
        .checked_sub(cmp::max(last_block_height, target_height))
        .ok_or_else(|| {
            ChainStorageError::InvalidQuery(format!(
                "Cannot rewind to height ({}) that is greater than the tip header height {}.",
                cmp::max(target_height, last_block_height),
                last_header_height
            ))
        })?;

    if steps_back > 0 {
        info!(
            target: LOG_TARGET,
            "Rewinding headers from height {} to {}",
            last_header_height,
            last_header_height.saturating_sub(steps_back)
        );
    }
    // We might have more headers than blocks, so we first see if we need to delete the extra headers.
    let mut txn = DbTransaction::new();
    for h in 0..steps_back {
        let height = last_header_height.saturating_sub(h);
        info!(
            target: LOG_TARGET,
            "Rewinding headers at height {}",
            height,
        );
        // If block accumulated data exists at this height (e.g. from a previous incomplete rewind
        // past pruning horizon), remove it and any remaining block data before deleting the header.
        if db.fetch_block_accumulated_data_by_height(height)?.is_some() {
            let header = fetch_header(db, height)?;
            let header_hash = header.hash();
            txn.delete_block_accumulated_data(height);
            txn.delete_all_kernerls_in_block(header_hash);
            txn.delete_all_inputs_in_block(header_hash);
        }
        txn.delete_header(height);
    }
    db.write(txn)?;
    // Delete blocks
    let mut steps_back = last_block_height.saturating_sub(target_height);
    // No blocks to remove, no need to update the best block
    if steps_back == 0 {
        return Ok(vec![]);
    }

    let mut removed_blocks = Vec::with_capacity(usize::try_from(steps_back).unwrap_or(usize::MAX));
    info!(
        target: LOG_TARGET,
        "Rewinding blocks from height {last_block_height} to {target_height}"
    );

    let effective_pruning_horizon = metadata.best_block_height().saturating_sub(metadata.pruned_height());
    let prune_past_horizon = metadata.is_pruned_node() && steps_back > effective_pruning_horizon;
    if prune_past_horizon {
        warn!(
            target: LOG_TARGET,
            "WARNING, reorg past pruning horizon (more than {effective_pruning_horizon} blocks back), rewinding back to 0"
        );
        steps_back = effective_pruning_horizon;
    }

    db.set_stats_total_height(steps_back);
    for h in 0..steps_back {
        if h % 50 == 0 {
            db.update_stats_progress(h);
        }
        let mut txn = DbTransaction::new();
        let block_height = last_block_height.saturating_sub(h);
        info!(target: LOG_TARGET, "Deleting block {block_height}");
        let block = fetch_block(db, block_height, false)?;
        let block = Arc::new(block.try_into_chain_block()?);
        let block_hash = *block.hash();
        txn.delete_tip_block(block_hash);
        txn.delete_header(block_height);
        if !prune_past_horizon && !db.contains(&DbKey::OrphanBlock(*block.hash()))? {
            // Because we know we will remove blocks we can't recover, this will be a destructive rewind, so we
            // can't recover from this apart from resync from another peer. Failure here
            // should not be common as this chain has a valid proof of work that has been
            // tested at this point in time.
            txn.insert_chained_orphan(block.clone());
        }
        removed_blocks.push(block);
        // Set best block to one before, to keep DB consistent, or, if we reached pruned horizon, set best block to 0 as
        // we have run out of headers.
        let is_last_step = h.saturating_add(1) == steps_back;
        let chain_header = db.fetch_chain_header_by_height(if prune_past_horizon && is_last_step {
            0
        } else {
            block_height.saturating_sub(1)
        })?;
        let metadata = db.fetch_chain_metadata()?;
        let expected_block_hash = *metadata.best_block_hash();
        txn.set_best_block(
            chain_header.height(),
            chain_header.accumulated_data().hash,
            chain_header.accumulated_data().total_accumulated_difficulty,
            expected_block_hash,
            chain_header.timestamp(),
        );
        // When rewinding past the pruning horizon to height 0, reset pruned_height in the same
        // transaction to maintain the invariant that pruned_height <= best_block_height.
        if prune_past_horizon && is_last_step {
            txn.set_pruned_height(0);
        }
        if h == 0 {
            // insert the new orphan chain tip
            debug!(target: LOG_TARGET, "Inserting new orphan chain tip: {block_hash}");
            txn.insert_orphan_chain_tip(block_hash, chain_header.accumulated_data().total_accumulated_difficulty);
        }
        // Update metadata
        debug!(
            target: LOG_TARGET,
            "Updating best block to height (#{}), total accumulated difficulty: {}",
            chain_header.height(),
            chain_header.accumulated_data().total_accumulated_difficulty
        );
        // This write operation is inside the loop to reduce the size of the write operation; this previously caused
        // issues.
        db.write(txn)?;
    }

    if prune_past_horizon {
        // We are rewinding past pruning horizon, so we need to remove all blocks and the UTXO's from them.
        // We also delete headers above the target height since they belong to the old chain and will be
        // replaced during re-sync. The header at target_height is preserved as it is the chain split point.
        // We don't have these complete blocks, so we don't push them to the removed blocks.
        for h in 0..last_block_height.saturating_sub(steps_back) {
            let height = last_block_height.saturating_sub(h).saturating_sub(steps_back);
            debug!(
                target: LOG_TARGET,
                "Deleting pruned block data at height {}",
                height,
            );
            // For pruned blocks, we cannot use delete_tip_block because it requires JMT validation
            // which is not possible for pruned data. Instead, directly delete the accumulated data,
            // kernels, inputs and then the header (above the target height).
            let header = fetch_header(db, height)?;
            let header_hash = header.hash();
            let mut txn = DbTransaction::new();
            txn.delete_block_accumulated_data(height);
            txn.delete_all_kernerls_in_block(header_hash);
            txn.delete_all_inputs_in_block(header_hash);
            if height > target_height {
                txn.delete_header(height);
            }
            db.write(txn)?;
        }
    }

    Ok(removed_blocks)
}

fn rewind_to_hash<T: BlockchainBackend>(
    db: &mut T,
    block_hash: BlockHash,
) -> Result<Vec<Arc<ChainBlock>>, ChainStorageError> {
    let block_hash_hex = block_hash.to_hex();
    let target_header = fetch_header_by_block_hash(&*db, block_hash)?.ok_or(ChainStorageError::ValueNotFound {
        entity: "BlockHeader",
        field: "block_hash",
        value: block_hash_hex,
    })?;
    rewind_to_height(db, target_header.height)
}

/// How far above the GHSA-3qmx-q9pv-f3m4 activation height a node's own tip has to be before the deep reorg
/// anchor ([`reorg_reintroduces_pre_ghsa_blocks`]) engages.
///
/// The anchor is the only rule in the advisory's set that is not a function of the candidate blocks alone: it
/// asks where *our* tip is. Two honest nodes are never at the same height, so any such rule can make them decide
/// the same fork differently, and this one decides it permanently - the loser discards the fork, long-bans the
/// peers serving it and needs a resync. The window is what confines that disagreement to forks no honest chain
/// produces: a refusal requires the refusing node's tip to be at least `WINDOW` blocks above the activation
/// height while the fork reaches below it, so every refused reorg is at least `WINDOW + 2` blocks deep. Below
/// that depth every node agrees, whatever its tip.
///
/// 360 was chosen because:
///
///   * it is `coinbase_min_maturity` at the MainNet activation height - the depth at which the protocol already treats
///     a block as settled, since a deeper reorg would unspend matured coinbases and break far more than fork choice. A
///     chain that can reorg this deep has bigger problems than this rule;
///   * it is four times the 90 block LWMA `difficulty_block_window`, and twice that window's span in chain blocks on a
///     two algorithm chain, so a refused fork covers a full difficulty adjustment on both algorithms;
///   * observed reorgs are one to three blocks deep, so it leaves two orders of magnitude of headroom;
///   * at the effective block times of the two scheduled networks it is about 12 hours on MainNet and 3 hours on
///     Esmeralda, and far below the thousand-plus block forks the attack this anchors against contemplates.
///
/// **The cost is real and is accepted deliberately:** for `WINDOW` blocks after the activation height the anchor
/// does not engage, and a fork rooted in the pre-fork range can still replace the chain. That exposure is not
/// what decides the attack, though. The same fork is available for the entire unbounded period *before* the
/// activation height, so an attacker able to mount it can mount it a day early and the anchor never sees it
/// either way. What the anchor buys is that the pre-fork range is sealed permanently after the window closes,
/// and buying that at the cost of partitioning honest nodes on routine reorgs would be a bad trade.
pub const GHSA_DEEP_REORG_CONFIRMATION_WINDOW: u64 = 360;

/// GHSA-3qmx-q9pv-f3m4 deep reorg anchor: is this reorg trying to put blocks written under the advisory's
/// *pre-fork* proof of work rules onto a chain that has already moved past the fork?
///
/// # Why this is needed at all
///
/// Every one of the advisory's consensus rules is gated on the *block's own* height, which is what makes them a
/// fork rather than a retroactive invalidation of history. Below the activation height the forgeable legacy Monero
/// coinbase sponge and the merged-namespace Cuckaroo verifier are still the rules, and they stay the rules
/// forever, because the blocks that were mined under them are on chain and cannot be re-validated.
///
/// That grandfathering is safe for history. It is not safe for the *future* of a chain that has passed the fork,
/// because nothing else in the system bounds how far back a reorg may reach:
///
///   * the accumulator credits `achieved_target.target()` rather than the achieved difficulty
///     (`blocks/accumulated_data.rs`), so a block that did no work still counts for the full target;
///   * fork choice is pure `total_accumulated_difficulty`, with height only a tiebreak
///     (`consensus/chain_strength_comparer.rs`);
///   * there is no reorg depth cap and no consensus checkpoint, and MainNet RandomXM has `max_difficulty =
///     Difficulty::max()`.
///
/// So an attacker forks below the activation height, re-uses one real high difficulty Monero solution to mint
/// blocks for free under the legacy rules, ramps the LWMA target, out-accumulates the honest chain and unwinds
/// everything above the fork point - including blocks mined after the fork under the new rules. On a pruned node
/// the rewind additionally runs past `prune_past_horizon` and is destructive.
///
/// # The rule
///
/// Once our tip is at least [`GHSA_DEEP_REORG_CONFIRMATION_WINDOW`] blocks above the activation height, refuse a
/// reorg whose *lowest newly added block* is below that height.
///
/// Three things about that phrasing are deliberate:
///
///   * It is stated in terms of the blocks being **added**, not the fork point. Those are the blocks that would be
///     validated under the weaker rules, and they are the only thing the attack can get for free. A fork point at
///     `activation - 1` whose first added block is at the activation height adds nothing that is not subject to the new
///     rules, so there is nothing to refuse.
///   * **The confirmation window is what keeps honest nodes together.** Without it the rule is an absolute height
///     boundary, and an absolute boundary splits the network at exactly the place it is least affordable. Take a three
///     block reorg across the fork, with our tip at the activation height, the fork point two blocks below it and added
///     blocks from `activation - 1` upwards. A node at the activation height would refuse it while its neighbour, one
///     block behind, accepts it. The refusal is not a deferral either: the fork is discarded, the header sync half of
///     the anchor long-bans every peer serving it, and the node only recovers by resyncing from scratch. Reorgs two or
///     three deep are routine on a two algorithm chain with two minute blocks, so an absolute boundary would strand
///     nodes on ordinary chain activity. With the window, a refusal implies a reorg at least
///     `GHSA_DEEP_REORG_CONFIRMATION_WINDOW + 2` blocks deep, because the refusing node's tip is that far above the
///     fork point. Every reorg the network treats as ordinary is therefore accepted by every node no matter where its
///     tip is, and disagreement is confined to forks far deeper than honest chain activity produces. The price is
///     stated plainly in the constant's own documentation: for that many blocks after the activation height the node is
///     still exposed to the attack.
///   * It rides the existing activation height, derived from the consensus constants
///     (`BaseNodeConsensusManager::derived_monero_coinbase_activation_height`). There is no new consensus constant and
///     no second height to coordinate. On a network where the advisory's fork is unscheduled the activation height is
///     `u64::MAX`, so the (saturating) engagement height is `u64::MAX` too and no reachable tip reaches it, and on
///     LocalNet the activation height is 0, so `lowest_new_block_height < 0` is never true and it is equally inert. It
///     also keeps the rule inert for a node syncing history: a node below the engagement height applies the pre-fork
///     rules and reorgs freely inside the pre-fork range, exactly as it must to reconstruct history.
///
/// # This is a consensus rule
///
/// It changes chain selection, and unlike the advisory's other rules it is not a function of the candidate
/// blocks alone - two nodes at different heights can decide a fork differently. That is the trade-off the window
/// bounds rather than removes. A node that refuses the majority chain stays on its own fork permanently and has
/// to be resynced; the log message at the refusal site says so.
pub fn reorg_reintroduces_pre_ghsa_blocks(
    local_tip_height: u64,
    lowest_new_block_height: u64,
    ghsa_activation_height: u64,
) -> bool {
    local_tip_height >= ghsa_activation_height.saturating_add(GHSA_DEEP_REORG_CONFIRMATION_WINDOW) &&
        lowest_new_block_height < ghsa_activation_height
}

// Checks whether we should add the block as an orphan. If it is the case, the orphan block is added and the chain
// is reorganised if necessary.
fn handle_possible_reorg<T: BlockchainBackend>(
    db: &mut T,
    config: &BlockchainDatabaseConfig,
    consensus_manager: &BaseNodeConsensusManager,
    block_validator: &dyn CandidateBlockValidator<T>,
    header_validator: &dyn HeaderChainLinkedValidator<T>,
    chain_strength_comparer: &dyn ChainStrengthComparer,
    candidate_block: Arc<Block>,
    // smt_writer: &mut LmdbTreeWriter,
) -> Result<BlockAddResult, ChainStorageError> {
    let timer = Instant::now();
    let height = candidate_block.header.height;
    let hash = candidate_block.header.hash();
    insert_orphan_and_find_new_tips(db, candidate_block, header_validator, consensus_manager)?;
    let after_orphans = timer.elapsed();
    let res = swap_to_highest_pow_chain(db, config, consensus_manager, block_validator, chain_strength_comparer);
    trace!(
        target: LOG_TARGET,
        "[handle_possible_reorg] block #{}, insert_orphans in {:.2?}, swap_to_highest in {:.2?} '{}'",
        height,
        after_orphans,
        timer.elapsed().saturating_sub(after_orphans),
        hash.to_hex(),
    );
    res
}

/// Reorganize the main chain with the provided fork chain, starting at the specified height.
/// Returns the blocks that were removed (if any), ordered from tip to fork (ie. height highest to lowest).
fn reorganize_chain<T: BlockchainBackend>(
    backend: &mut T,
    block_validator: &dyn CandidateBlockValidator<T>,
    fork_hash: HashOutput,
    new_chain_from_fork: &VecDeque<Arc<ChainBlock>>,
) -> Result<Vec<Arc<ChainBlock>>, ChainStorageError> {
    let removed_blocks = rewind_to_hash(backend, fork_hash)?;
    debug!(
        target: LOG_TARGET,
        "Validate and add {} chain block(s) from block {}. Rewound blocks: [{}]",
        new_chain_from_fork.len(),
        fork_hash,
        removed_blocks
            .iter()
            .map(|b| b.height().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );
    for (i, block) in new_chain_from_fork.iter().enumerate() {
        let mut txn = DbTransaction::new();
        let block_hash = *block.hash();
        txn.delete_orphan(block_hash);
        let chain_metadata = backend.fetch_chain_metadata()?;
        if let Err(e) = block_validator.validate_body_with_metadata(backend, block, &chain_metadata) {
            warn!(
                target: LOG_TARGET,
                "Orphan block {} ({}) failed validation during chain reorg: {:?}",
                block.header().height,
                block_hash,
                e
            );
            if e.get_ban_reason().is_some() && e.get_ban_reason().unwrap().ban_duration != BanPeriod::Short {
                txn.insert_bad_block(block.header().hash(), block.header().height, e.to_string());
            }
            // We removed a block from the orphan chain, so the chain is now "broken", so we remove the rest of the
            // remaining blocks as well.
            for block in new_chain_from_fork.iter().skip(i.saturating_add(1)) {
                txn.delete_orphan(*block.hash());
            }
            backend.write(txn)?;

            info!(target: LOG_TARGET, "Restoring previous chain after failed reorg.");
            restore_reorged_chain(backend, fork_hash, removed_blocks)?;
            return Err(e.into());
        }

        insert_best_block(&mut txn, block.clone())?;

        // Failed to store the block - this should typically never happen unless there is a bug in the validator
        // (e.g. does not catch a double spend). In any case, we still need to restore the chain to a
        // good state before returning.
        if let Err(e) = backend.write(txn) {
            warn!(
                target: LOG_TARGET,
                "Failed to commit reorg chain: {e:?}. Restoring last chain."
            );

            restore_reorged_chain(backend, fork_hash, removed_blocks)?;
            return Err(e);
        }
    }

    Ok(removed_blocks)
}

/// The GHSA-3qmx-q9pv-f3m4 deep reorg anchor as `swap_to_highest_pow_chain` applies it. Returns `true` if the
/// swap was refused, in which case the fork has been discarded and the caller must not reorg.
///
/// See [`reorg_reintroduces_pre_ghsa_blocks`] for the rule and why it is phrased in terms of the lowest added
/// block rather than the fork point.
fn refuse_reorg_below_ghsa_activation<T: BlockchainBackend>(
    db: &mut T,
    consensus_manager: &BaseNodeConsensusManager,
    tip_header: &ChainHeader,
    best_fork_header: &ChainHeader,
    reorg_chain: &VecDeque<Arc<ChainBlock>>,
) -> Result<bool, ChainStorageError> {
    let lowest_new_block = match reorg_chain.front() {
        Some(block) => block,
        None => return Ok(false),
    };
    let activation_height = consensus_manager.derived_monero_coinbase_activation_height();
    if !reorg_reintroduces_pre_ghsa_blocks(tip_header.height(), lowest_new_block.height(), activation_height) {
        return Ok(false);
    }

    error!(
        target: LOG_TARGET,
        "REFUSED a deep reorg (GHSA-3qmx-q9pv-f3m4). A fork chain of {} block(s) starting at height {} ({}) would \
         have replaced our chain from there upwards, but the advisory's proof of work rules only become consensus \
         at height {} and our tip is already at {}. Blocks below that height are still validated with the pre-fork \
         verifiers the advisory describes as forgeable, so a chain rooted there can be minted at no proof of work \
         cost no matter how much accumulated difficulty it claims ({}). This fork is being discarded, not queued: \
         it will never be accepted by this node. If you believe it is the honest chain, this node's database is on \
         the wrong side of the fork and has to be resynced from scratch.",
        reorg_chain.len(),
        lowest_new_block.height(),
        lowest_new_block.hash(),
        activation_height,
        tip_header.height(),
        best_fork_header.accumulated_data().total_accumulated_difficulty,
    );

    // Drop the whole fork out of the orphan pool rather than leaving it to win `find_strongest_orphan_tip` again on
    // the next block: if it stayed, it would mask every legitimate fork behind it and the node would stop following
    // the chain at all.
    //
    // What has to go is the orphan *subtree* rooted at the lowest new block, not just `reorg_chain`.
    // `reorg_chain` is the single linking chain `get_orphan_link_main_chain` walked back to the main chain, but
    // `find_orphan_descendant_tips_of` registers a tip for *every* descendant, so the same fork can carry several
    // tips - an attacker announcing two near-equal tips on the refused fork costs nothing under exactly the
    // premise this rule exists for. Deleting only the linking chain would leave the sibling branches in
    // `orphans_db` and `orphan_chain_tips_db` with their parent gone, and the next `swap_to_highest_pow_chain`
    // that picked one of them would fail `get_orphan_link_main_chain` with `InvalidOperation` - an error that
    // propagates out of `add_block` before `cleanup_orphans` runs, so the node stops accepting blocks entirely
    // until it is restarted. The subtree is deleted in reverse discovery order, so every child goes before its
    // parent: each delete then either removes a tip or promotes a parent that is itself deleted later, and the
    // root's parent is in the main chain rather than the orphan pool, so the last delete promotes nothing and no
    // stale tip is left behind. (`LMDBDatabase::delete_orphan` promotes with `lmdb_replace` for the same reason:
    // a parent with two tip children is promoted once per child.)
    //
    // Nothing is recorded in `bad_blocks` here. It looks like the obvious anti-replay, and it was tried, but it
    // cannot work at this call site: the refusal runs *before* any rewind, so `best_block_height` is still our
    // tip, while the record would be at a height below the activation height - and `insert_bad_block_and_cleanup`
    // sweeps every record below `best_block_height - CLEAN_BAD_BLOCKS_BEFORE_REL_HEIGHT`, which is
    // `best_block_height` itself in a non-test build, in the same write transaction. The record would be deleted
    // the moment it was written. Making it survive would mean either a record the sweep never reclaims - an
    // unbounded, remotely triggered table, since minting distinct pre-activation forks is free to the attacker -
    // or widening the sweep for every other caller. Neither is worth it: re-announcing the fork just has it
    // orphaned, refused and dropped again, which is the same cost as any other orphan spam and is bounded by the
    // orphan pool's own capacity.
    let mut txn = DbTransaction::new();
    for hash in orphan_subtree_hashes(db, *lowest_new_block.hash())?.iter().rev() {
        txn.delete_orphan(*hash);
    }
    db.write(txn)?;
    Ok(true)
}

/// Every orphan in the subtree rooted at `root_hash`, parents before children.
///
/// `fetch_orphan_children_of` is the only index that answers "what is hanging off this orphan", and a fork can
/// branch, so the walk has to be transitive rather than a single chain. Returned in breadth first order; a
/// caller deleting the subtree reverses it, which puts every child before its parent.
fn orphan_subtree_hashes<T: BlockchainBackend>(
    db: &T,
    root_hash: HashOutput,
) -> Result<Vec<HashOutput>, ChainStorageError> {
    let mut ordered = Vec::new();
    let mut to_visit = VecDeque::new();
    to_visit.push_back(root_hash);
    while let Some(hash) = to_visit.pop_front() {
        ordered.push(hash);
        for child in db.fetch_orphan_children_of(hash)? {
            to_visit.push_back(child.header.hash());
        }
    }
    Ok(ordered)
}

fn swap_to_highest_pow_chain<T: BlockchainBackend>(
    db: &mut T,
    config: &BlockchainDatabaseConfig,
    consensus_manager: &BaseNodeConsensusManager,
    block_validator: &dyn CandidateBlockValidator<T>,
    chain_strength_comparer: &dyn ChainStrengthComparer,
    // smt_writer: &mut LmdbTreeWriter,
) -> Result<BlockAddResult, ChainStorageError> {
    let strongest_orphan_tips = db.fetch_strongest_orphan_chain_tips()?;
    if strongest_orphan_tips.is_empty() {
        // we have no orphan chain tips, we are on the best tip we have, so lets return ok
        remove_non_canonical_headers(db)?;
        return Ok(BlockAddResult::OrphanBlock);
    }
    // Check the accumulated difficulty of the best fork chain compared to the main chain.
    let best_fork_header =
        find_strongest_orphan_tip(strongest_orphan_tips, chain_strength_comparer).ok_or_else(|| {
            // This should never happen because a block is always added to the orphan pool before
            // checking, but just in case
            warn!(
                target: LOG_TARGET,
                "Unable to find strongest orphan tip`. This should never happen.",
            );
            ChainStorageError::InvalidOperation("No chain tips found in orphan pool".to_string())
        })?;
    let tip_header = db.fetch_tip_header()?;
    match chain_strength_comparer.compare(&best_fork_header, &tip_header) {
        Ordering::Greater => {
            debug!(
                target: LOG_TARGET,
                "Fork chain (accum_diff:{}, hash:{}) is stronger than the current tip (#{} ({})).",
                best_fork_header.accumulated_data().total_accumulated_difficulty,
                best_fork_header.accumulated_data().hash,
                tip_header.height(),
                tip_header.hash(),
            );
        },
        Ordering::Less | Ordering::Equal => {
            debug!(
                target: LOG_TARGET,
                "Fork chain (accum_diff:{}, hash:{}) with block {} ({}) has a weaker difficulty.",
                best_fork_header.accumulated_data().total_accumulated_difficulty,
                best_fork_header.accumulated_data().hash,
                tip_header.header().height,
                tip_header.hash(),
            );
            remove_non_canonical_headers(db)?;
            return Ok(BlockAddResult::OrphanBlock);
        },
    }

    let reorg_chain = get_orphan_link_main_chain(db, best_fork_header.hash())?;
    let lowest_new_block = reorg_chain
        .front()
        .expect("The new orphan block should be in the queue")
        .clone();
    let fork_hash = lowest_new_block.header().prev_hash;

    // GHSA-3qmx-q9pv-f3m4 deep reorg anchor. This is the choke point for the gossip driven reorg path: it is the
    // first place where both halves of the question are known - our own tip, and the full set of blocks the swap
    // would add - and it is still *before* `reorganize_chain`, so nothing has been rewound and the pruned-node
    // `prune_past_horizon` rewind has not been reached. The header sync path does not come through here and is
    // anchored separately, in `HeaderSynchronizer::determine_sync_status`.
    if refuse_reorg_below_ghsa_activation(db, consensus_manager, &tip_header, &best_fork_header, &reorg_chain)? {
        remove_non_canonical_headers(db)?;
        return Ok(BlockAddResult::OrphanBlock);
    }

    let num_added_blocks = reorg_chain.len();
    // Note: This will also remove ay surplus headers (i.e. headers that are not linked to any blocks)
    let removed_blocks = reorganize_chain(db, block_validator, fork_hash, &reorg_chain)?;
    let num_removed_blocks = removed_blocks.len();

    // reorg is required when any blocks are removed or more than one are added
    // see https://github.com/tari-project/tari/issues/2101
    if num_removed_blocks > 0 || num_added_blocks > 1 {
        if config.track_reorgs {
            let mut txn = DbTransaction::new();
            txn.insert_reorg(Reorg::from_reorged_blocks(&reorg_chain, &removed_blocks));
            if let Err(e) = db.write(txn) {
                error!(target: LOG_TARGET, "Failed to track reorg: {e}");
            }
        }

        log!(
            target: LOG_TARGET,
            if num_removed_blocks > 1 {
                Level::Warn
            } else {
                Level::Info
            }, // We want a warning if the number of removed blocks is at least 2.
            "Chain reorg required from {} to {} (accum_diff:{}, hash:{}) to (accum_diff:{}, hash:{}). Number of \
             blocks to remove: {}, to add: {}.",
            tip_header.header().height,
            best_fork_header.header().height,
            tip_header.accumulated_data().total_accumulated_difficulty,
            tip_header.accumulated_data().hash,
            best_fork_header.accumulated_data().total_accumulated_difficulty,
            best_fork_header.accumulated_data().hash,
            num_removed_blocks,
            num_added_blocks,
        );
        Ok(BlockAddResult::ChainReorg {
            removed: removed_blocks,
            added: reorg_chain.into(),
        })
    } else {
        trace!(
            target: LOG_TARGET,
            "No reorg required. Number of blocks to remove: {num_removed_blocks}, to add: {num_added_blocks}."
        );
        // NOTE: panic is not possible because get_orphan_link_main_chain cannot return an empty Vec (reorg_chain)
        Ok(BlockAddResult::Ok(reorg_chain.front().unwrap().clone()))
    }
}

// Trim non-canonical headers above best-block height, but keep aligned banked headers.
// Safety:
// - Deletes only headers above best-block height that do not have a canonical predecessor.
// - Never deletes blocks.
fn remove_non_canonical_headers<T: BlockchainBackend>(db: &mut T) -> Result<usize, ChainStorageError> {
    let metadata = db.fetch_chain_metadata()?;
    let best_block_height = metadata.best_block_height();
    let mut expected_prev_hash = *metadata.best_block_hash();

    // Find the first mismatch above best-block height
    let mut height = best_block_height.saturating_add(1);
    loop {
        let next_chain_header = match db.fetch_chain_header_by_height(height) {
            Ok(hdr) => hdr,
            Err(ChainStorageError::ValueNotFound { .. }) => break, // no more headers stored
            Err(e) => return Err(e),
        };

        if next_chain_header.header().prev_hash != expected_prev_hash {
            let last_chain_header_height = db.fetch_last_chain_header()?.height();
            // let's clear out all remaining headers that don't have a canonical predecessor
            // rewind to height will first delete the headers, then try to delete blocks, but if we call this to a
            // height above the current best block height, it will only trim the extra headers with no blocks
            rewind_to_height(db, max(height.saturating_sub(1), best_block_height))?;
            let removed =
                usize::try_from(last_chain_header_height.saturating_sub(height).saturating_add(1)).unwrap_or(0);
            debug!(target: LOG_TARGET, "Trimmed {removed} non-canonical header(s) starting at height {height}");
            return Ok(removed);
        }

        expected_prev_hash = *next_chain_header.hash();
        height = height.saturating_add(1);
    }

    // All banked headers align with the current best chain; nothing to do
    Ok(0)
}

fn restore_reorged_chain<T: BlockchainBackend>(
    db: &mut T,
    to_hash: HashOutput,
    previous_chain: Vec<Arc<ChainBlock>>,
) -> Result<(), ChainStorageError> {
    let invalid_chain = rewind_to_hash(db, to_hash)?;
    debug!(
        target: LOG_TARGET,
        "Removed {} blocks during chain restore: {:?}.",
        invalid_chain.len(),
        invalid_chain
            .iter()
            .map(|block| block.accumulated_data().hash)
            .collect::<Vec<_>>(),
    );
    let mut txn = DbTransaction::new();

    for block in previous_chain.into_iter().rev() {
        txn.delete_orphan(block.accumulated_data().hash);
        insert_best_block(&mut txn, block)?;
    }
    db.write(txn)?;
    Ok(())
}

// this is tricky as we need to find the vm_key hash for the candidate block, but it might not be in the current chain
// so we need to search for it.
fn get_vm_key_for_candidate_block<T: BlockchainBackend>(
    db: &mut T,
    candidate_block: Arc<Block>,
) -> Result<(FixedHash, u64), ChainStorageError> {
    get_vm_key_for_candidate_header(db, candidate_block.header.clone())
}

/// Returns the Tari RandomX VM key hash for a candidate header, and the highest height at which the candidate's
/// chain and the chain in the database are known to be the same chain.
///
/// The candidate may be an orphan that forks below the current tip, so both answers have to be found by walking the
/// candidate's own ancestry back until it meets the main chain. That meeting height is what any chain dependent
/// index in the database may be trusted up to (see [`HeaderChainContext::candidate_chain`]); 0 is returned when the
/// walk never meets the main chain, which trusts nothing.
fn get_vm_key_for_candidate_header<T: BlockchainBackend>(
    db: &T,
    header: BlockHeader,
) -> Result<(FixedHash, u64), ChainStorageError> {
    let vm_height = tari_rx_vm_key_height(header.height);
    let mut current_header = header.clone();
    while current_header.height != vm_height {
        let h = db.fetch_chain_header_in_all_chains(&current_header.prev_hash)?;
        let chain_header = db.fetch_chain_header_by_height(h.height())?;
        if *h.header() == *chain_header.header() {
            // Now we now the orphan links back to the main chain here
            return Ok((*db.fetch_chain_header_by_height(vm_height)?.hash(), h.height()));
        }
        current_header = h.header().clone();
    }
    Ok((FixedHash::from(*current_header.hash()), 0))
}

/// Insert the provided block into the orphan pool and returns any new tips that were created.
#[allow(clippy::too_many_lines)]
fn insert_orphan_and_find_new_tips<T: BlockchainBackend>(
    db: &mut T,
    candidate_block: Arc<Block>,
    validator: &dyn HeaderChainLinkedValidator<T>,
    rules: &BaseNodeConsensusManager,
) -> Result<(), ChainStorageError> {
    let hash = candidate_block.hash();

    // There cannot be any _new_ tips if we've seen this orphan block before
    if db.contains(&DbKey::OrphanBlock(hash))? {
        return Ok(());
    }

    let mut txn = DbTransaction::new();
    let parent = match db.fetch_orphan_chain_tip_by_hash(&candidate_block.header.prev_hash)? {
        Some(curr_parent) => {
            txn.remove_orphan_chain_tip(candidate_block.header.prev_hash);
            info!(
                target: LOG_TARGET,
                "New orphan ({hash}) extends a chain in the current candidate tip set"
            );
            curr_parent
        },
        None => match db
            .fetch_chain_header_in_all_chains(&candidate_block.header.prev_hash)
            .optional()?
        {
            Some(curr_parent) => {
                debug!(
                    target: LOG_TARGET,
                    "New orphan #{} ({}) does not have a parent in the current tip set. Parent is {}",
                    candidate_block.header.height,
                    hash,
                    curr_parent.hash(),
                );
                curr_parent
            },
            None => {
                if db.contains(&DbKey::OrphanBlock(hash))? {
                    info!(
                        target: LOG_TARGET,
                        "Orphan #{} ({}) already found in orphan database", candidate_block.header.height, hash
                    );
                } else {
                    info!(
                        target: LOG_TARGET,
                        "Orphan #{} ({}) was not connected to any previous headers. Inserting as true orphan",
                        candidate_block.header.height,
                        hash
                    );

                    txn.insert_orphan(candidate_block);
                }
                db.write(txn)?;
                return Ok(());
            },
        },
    };

    // validate the block header
    let mut prev_timestamps = get_previous_timestamps(db, &candidate_block.header, rules)?;
    let (vm_key, fork_height) = get_vm_key_for_candidate_block(db, candidate_block.clone())?;
    // The candidate is an orphan: the database holds the main chain, which is only the candidate's chain up to the
    // fork point. Anything recorded above it belongs to a chain this candidate is not on.
    //
    // No pending seeds are tracked for the orphan chain itself, so the seed age rule is deliberately best effort
    // here: it catches re-use of a seed from the shared history and misses re-use within the orphan chain. That
    // fails open, and block body validation applies the same rule with an exact view once the chain is committed.
    let chain_context = HeaderChainContext::candidate_chain(vm_key, fork_height, None);
    let result = validator.validate(
        db,
        &candidate_block.header,
        parent.header(),
        &prev_timestamps,
        None,
        chain_context,
    );
    let achieved_target_diff = match result {
        Ok(validated) => validated.achieved_target,
        // future timelimit validation can succeed at a later time. As the block is not yet valid, we discard it
        // for now and ban the peer, but wont blacklist the block.
        Err(e @ ValidationError::BlockHeaderError(BlockHeaderValidationError::InvalidTimestampFutureTimeLimit)) |
        // The seed age rule is measured against the chain the header is on, so its verdict belongs to that chain and
        // not to the header alone: the same header can be stale on one chain and fine on another. Discard it and ban
        // the peer, but never memo it as bad. `BlockHeaderSyncValidator::blacklist_unless_verdict_can_change` does
        // the same for the other caller of this validator.
        Err(e @ ValidationError::BlockHeaderError(BlockHeaderValidationError::OldSeedHash)) |
        // We dont want to mark a block as bad for internal failures
        Err(
            e @ ValidationError::FatalStorageError(_) | e @ ValidationError::IncorrectNumberOfTimestampsProvided { .. },
        ) |
        // We dont have to mark the block twice
        Err(e @ ValidationError::BadBlockFound { .. }) => {
            db.write(txn)?;
            return Err(e.into());
        }

        Err(e) => {
            txn.insert_bad_block(candidate_block.header.hash(), candidate_block.header.height, e.to_string());
            db.write(txn)?;
            return Err(e.into());
        },
    };

    // Include the current block timestamp in the median window
    prev_timestamps.push(candidate_block.header.timestamp);

    let accumulated_data = BlockHeaderAccumulatedDataBuilder::from_previous(parent.accumulated_data())
        .with_hash(hash)
        .with_achieved_target_difficulty(achieved_target_diff)
        .with_total_kernel_offset(candidate_block.header.total_kernel_offset.clone())
        .build(rules.consensus_constants(candidate_block.header.height))?;

    let chain_block = ChainBlock::try_construct(candidate_block, accumulated_data).ok_or(
        ChainStorageError::UnexpectedResult("Somehow hash is missing from Chain block".to_string()),
    )?;
    let chain_header = chain_block.to_chain_header();

    // Extend orphan chain tip.

    txn.insert_orphan(chain_block.to_arc_block());

    txn.set_accumulated_data_for_orphan(chain_block.header().version, chain_block.accumulated_data().clone());
    db.write(txn)?;
    let height = chain_header.height();
    let tips = find_orphan_descendant_tips_of(
        db,
        chain_header,
        prev_timestamps,
        validator,
        rules.consensus_constants(height),
        fork_height,
    )?;
    let mut txn = DbTransaction::new();
    debug!(target: LOG_TARGET, "Found {} new orphan tips", tips.len());
    for new_tip in &tips {
        txn.insert_orphan_chain_tip(
            *new_tip.hash(),
            chain_block.accumulated_data().total_accumulated_difficulty,
        );
    }

    db.write(txn)?;
    Ok(())
}

// Find the tip set of any orphans that have hash as an ancestor
fn find_orphan_descendant_tips_of<T: BlockchainBackend>(
    db: &mut T,
    prev_chain_header: ChainHeader,
    prev_timestamps: RollingVec<EpochTime>,
    validator: &dyn HeaderChainLinkedValidator<T>,
    consensus_constants: &ConsensusConstants,
    // The highest height at which this orphan chain and the chain in the database are the same chain. Every
    // descendant found here is on the same orphan chain, so they all share it.
    fork_height: u64,
) -> Result<Vec<ChainHeader>, ChainStorageError> {
    let children = db.fetch_orphan_children_of(*prev_chain_header.hash())?;
    if children.is_empty() {
        debug!(
            target: LOG_TARGET,
            "Found new orphan tip {} ({})",
            prev_chain_header.height(),
            prev_chain_header.hash(),
        );
        return Ok(vec![prev_chain_header]);
    }

    debug!(
        target: LOG_TARGET,
        "Found {} children of orphan {} ({})",
        children.len(),
        prev_chain_header.height(),
        prev_chain_header.hash()
    );

    let mut res = vec![];
    for child in children {
        debug!(
            target: LOG_TARGET,
            "Validating header #{} ({}), descendant of #{} ({})",
            child.header.height,
            child.hash(),
            prev_chain_header.height(),
            prev_chain_header.hash(),
        );

        // we need to validate the header here because it may never have been validated.
        // TODO: this takes the Tari RandomX VM key from the main chain, while `fork_height` next to it is derived
        // from the candidate's own chain. For an orphan that forks below its VM key band boundary the two disagree,
        // which can fail a legitimate deep fork. Pre-existing behaviour, tracked separately; fixing it means
        // reworking orphan path VM key derivation.
        let vm_key = *db
            .fetch_chain_header_by_height(tari_rx_vm_key_height(child.header.height))?
            .hash();
        match validator.validate(
            db,
            &child.header,
            prev_chain_header.header(),
            &prev_timestamps,
            None,
            // As in `insert_orphan_and_find_new_tips`: no pending seeds for the orphan chain, so the seed age rule
            // is deliberately best effort on this path and fails open, with body validation as the backstop.
            HeaderChainContext::candidate_chain(vm_key, fork_height, None),
        ) {
            Ok(validated) => {
                let achieved_target = validated.achieved_target;
                // Append the child timestamp - a RollingVec ensures that the number of timestamps can never be more
                // than the median timestamp window size.
                let mut prev_timestamps_for_children = prev_timestamps.clone();
                prev_timestamps_for_children.push(child.header.timestamp);

                let child_hash = child.hash();
                let accum_data = BlockHeaderAccumulatedDataBuilder::from_previous(prev_chain_header.accumulated_data())
                    .with_hash(child_hash)
                    .with_achieved_target_difficulty(achieved_target)
                    .with_total_kernel_offset(child.header.total_kernel_offset.clone())
                    .build(consensus_constants)?;

                let chain_header = ChainHeader::try_construct(child.header, accum_data).ok_or_else(|| {
                    ChainStorageError::InvalidOperation(format!(
                        "Attempt to create mismatched ChainHeader with hash {child_hash}"
                    ))
                })?;

                // Set/overwrite accumulated data for this orphan block
                let mut txn = DbTransaction::new();
                txn.set_accumulated_data_for_orphan(
                    chain_header.header().version,
                    chain_header.accumulated_data().clone(),
                );
                db.write(txn)?;
                let children = find_orphan_descendant_tips_of(
                    db,
                    chain_header,
                    prev_timestamps_for_children,
                    validator,
                    consensus_constants,
                    fork_height,
                )?;
                res.extend(children);
            },
            Err(e) => {
                // Warn for now, idk might lower to debug later.
                warn!(
                    target: LOG_TARGET,
                    "Discarding orphan {} because it has an invalid header: {:?}",
                    child.hash(),
                    e
                );
                let mut txn = DbTransaction::new();
                txn.delete_orphan(child.hash());
                db.write(txn)?;
            },
        };
    }
    Ok(res)
}
fn get_previous_timestamps<T: BlockchainBackend>(
    db: &T,
    header: &BlockHeader,
    rules: &BaseNodeConsensusManager,
) -> Result<RollingVec<EpochTime>, ChainStorageError> {
    let median_timestamp_window_size = rules.consensus_constants(header.height).median_timestamp_count();
    let prev_height = usize::try_from(header.height)
        .map_err(|_| ChainStorageError::ConversionError("Block height overflowed usize".to_string()))?;

    let prev_timestamps_count = cmp::min(median_timestamp_window_size, prev_height);

    let mut timestamps = RollingVec::new(median_timestamp_window_size);
    let mut curr_header = header.prev_hash;
    for _ in 0..prev_timestamps_count {
        let h = db.fetch_chain_header_in_all_chains(&curr_header)?;
        curr_header = h.header().prev_hash;
        timestamps.push(EpochTime::from(h.timestamp()));
    }

    // median calculation requires timestamps to be sorted
    timestamps.sort_unstable();

    Ok(timestamps)
}

/// Gets all blocks ordered from the block that connects (via prev_hash) to the main chain, to the orphan tip.
#[allow(clippy::ptr_arg)]
fn get_orphan_link_main_chain<T: BlockchainBackend>(
    db: &T,
    orphan_tip: &HashOutput,
) -> Result<VecDeque<Arc<ChainBlock>>, ChainStorageError> {
    let mut chain: VecDeque<Arc<ChainBlock>> = VecDeque::new();
    let mut curr_hash = *orphan_tip;
    loop {
        let curr_block = db.fetch_orphan_chain_block(curr_hash)?.ok_or_else(|| {
            ChainStorageError::InvalidOperation(format!(
                "get_orphan_link_main_chain: Failed to fetch orphan chain block by hash {curr_hash}"
            ))
        })?;
        curr_hash = curr_block.header().prev_hash;
        chain.push_front(Arc::new(curr_block));

        // If this hash is part of the main chain, we're done - since curr_hash has already been set to the previous
        // hash, the chain Vec does not include the fork block in common with both chains
        if db.contains(&DbKey::HeaderHash(curr_hash))? {
            break;
        }
    }
    Ok(chain)
}

/// Find and return the orphan chain tip with the highest accumulated difficulty.
fn find_strongest_orphan_tip(
    orphan_chain_tips: Vec<ChainHeader>,
    chain_strength_comparer: &dyn ChainStrengthComparer,
) -> Option<ChainHeader> {
    let mut best_block_header: Option<ChainHeader> = None;
    for tip in orphan_chain_tips {
        best_block_header = match best_block_header {
            Some(current_best) => match chain_strength_comparer.compare(&current_best, &tip) {
                Ordering::Less => Some(tip),
                Ordering::Greater | Ordering::Equal => Some(current_best),
            },
            None => Some(tip),
        };
    }

    best_block_header
}

// Perform a comprehensive search to remove all the minimum height orphans to maintain the configured orphan pool
// storage limit. If the node is configured to run in pruned mode then orphan blocks with heights lower than the horizon
// block height will also be discarded.
fn cleanup_orphans<T: BlockchainBackend>(db: &mut T, orphan_storage_capacity: usize) -> Result<(), ChainStorageError> {
    let metadata = db.fetch_chain_metadata()?;
    let horizon_height = metadata.pruned_height_at_given_chain_tip(metadata.best_block_height());

    db.delete_oldest_orphans(horizon_height, orphan_storage_capacity)
}

fn prune_database_if_needed<T: BlockchainBackend>(
    db: &mut T,
    pruning_horizon: u64,
    pruning_interval: u64,
) -> Result<(), ChainStorageError> {
    let metadata = db.fetch_chain_metadata()?;
    if !metadata.is_pruned_node() {
        return Ok(());
    }

    let prune_to_height_target = metadata.best_block_height().saturating_sub(pruning_horizon);
    debug!(
        target: LOG_TARGET,
        "Blockchain height: {}, pruning horizon: {}, pruned height: {}, prune to height target: {}, pruning interval: {}",
        metadata.best_block_height(),
        metadata.pruning_horizon(),
        metadata.pruned_height(),
        prune_to_height_target,
        pruning_interval,
    );
    if metadata.pruned_height() < prune_to_height_target.saturating_sub(pruning_interval) {
        prune_to_height(db, prune_to_height_target)?;
    }

    Ok(())
}

fn prune_to_height<T: BlockchainBackend>(db: &mut T, target_horizon_height: u64) -> Result<(), ChainStorageError> {
    let metadata = db.fetch_chain_metadata()?;
    let last_pruned = metadata.pruned_height();
    if target_horizon_height < last_pruned {
        return Err(ChainStorageError::InvalidArguments {
            func: "prune_to_height",
            arg: "target_horizon_height",
            message: format!(
                "Target pruning horizon {target_horizon_height} is less than current pruning horizon {last_pruned}"
            ),
        });
    }

    if target_horizon_height == last_pruned {
        info!(
            target: LOG_TARGET,
            "Blockchain already pruned to height {target_horizon_height}"
        );
        return Ok(());
    }

    if target_horizon_height > metadata.best_block_height() {
        return Err(ChainStorageError::InvalidArguments {
            func: "prune_to_height",
            arg: "target_horizon_height",
            message: format!(
                "Target pruning horizon {} is greater than current block height {}",
                target_horizon_height,
                metadata.best_block_height()
            ),
        });
    }

    info!(
        target: LOG_TARGET,
        "Pruning blockchain database at height {target_horizon_height} (was={last_pruned})"
    );

    let mut txn = DbTransaction::new();
    for block_to_prune in last_pruned.saturating_add(1)..=target_horizon_height {
        let header = db.fetch_chain_header_by_height(block_to_prune)?;
        // Note, this could actually be done in one step instead of each block, since deleted is
        // accumulated

        txn.prune_outputs_spent_at_hash(*header.hash());
        txn.delete_all_inputs_in_block(*header.hash());
        // Write the transaction periodically so it wont run into the transaction size limit. 100 was a safe limit.
        if txn.operations().len() >= 100 {
            txn.set_pruned_height(block_to_prune);
            db.write(mem::take(&mut txn))?;
        }
    }

    txn.set_pruned_height(target_horizon_height);

    db.write(txn)?;
    Ok(())
}

fn log_error<T>(req: DbKey, err: ChainStorageError) -> Result<T, ChainStorageError> {
    error!(
        target: LOG_TARGET,
        "Database access error on request: {req}: {err}"
    );
    Err(err)
}

impl<T> Clone for BlockchainDatabase<T> {
    fn clone(&self) -> Self {
        BlockchainDatabase {
            db: self.db.clone(),
            validators: self.validators.clone(),
            config: self.config,
            consensus_manager: self.consensus_manager.clone(),
            difficulty_calculator: self.difficulty_calculator.clone(),
            disable_add_block_flag: self.disable_add_block_flag.clone(),
            is_background_pruning: self.is_background_pruning.clone(),
        }
    }
}

fn convert_to_option_bounds<T: RangeBounds<u64>>(bounds: T) -> (Option<u64>, Option<u64>) {
    let start = bounds.start_bound();
    let end = bounds.end_bound();
    use Bound::{Excluded, Included, Unbounded};
    let start = match start {
        Included(n) => Some(*n),
        Excluded(n) => Some(n.saturating_add(1)),
        Unbounded => None,
    };
    let end = match end {
        Included(n) => Some(*n),
        Excluded(n) => Some(n.saturating_sub(1)),
        // `(n..)` means fetch from the last block until `n`
        Unbounded => None,
    };

    (start, end)
}

// Process a batch of outputs in one block for PayRef migration
fn process_payref_for_height<B: BlockchainBackend>(
    db: Arc<RwLock<B>>,
    height: u64,
    metadata_at_start: ChainMetadata,
    initialize_stats: Option<u64>,
    finalize: bool,
) -> Result<PayrefRebuildStatus, ChainStorageError> {
    debug!(target: LOG_TARGET, "[PayRef] Processing index rebuilding for height {height}");

    let write_lock = db
        .write()
        .map_err(|_e| ChainStorageError::AccessError("Write lock on blockchain backend failed".into()))?;

    let status =
        write_lock.build_payref_indexes_for_height(height, metadata_at_start.clone(), initialize_stats, finalize)?;

    if finalize || status.is_rebuilt {
        debug!(
            target: LOG_TARGET,
            "[PayRef] Finalized index rebuilding for heights {} to {}",
            metadata_at_start.best_block_height(), height
        );
    }

    Ok(status)
}

// Process the burn commitment index rebuild for a single block height.
fn process_burn_commitment_index_for_height<B: BlockchainBackend>(
    db: Arc<RwLock<B>>,
    height: u64,
    finalize: bool,
) -> Result<BurnCommitmentRebuildStatus, ChainStorageError> {
    debug!(target: LOG_TARGET, "[BurnIndex] Processing burn commitment index rebuild for height {height}");

    let write_lock = db
        .write()
        .map_err(|_e| ChainStorageError::AccessError("Write lock on blockchain backend failed".into()))?;

    let status = write_lock.build_burn_commitment_index_for_height(height, finalize)?;

    if finalize || status.is_rebuilt {
        debug!(
            target: LOG_TARGET,
            "[BurnIndex] Finalized burn commitment index rebuild at height {height}"
        );
    }

    Ok(status)
}

/// Yield for one throttle interval, then process a single height off the blocking pool.
///
/// The throttle lets other tasks run: below the fork it pushes the rebuild out noticeably - 80,000 blocks take at
/// least 8,000 seconds longer, just over two hours - which is the accepted price of not monopolising the write
/// lock. The strict suffix is throttled far more lightly, because it is short in every case that exists today
/// (MainNet's tip is ~50 blocks above the fork), but not to zero: the strict path runs a RandomX-backed header
/// validation and holds the write lock to rewrite the data, so an Esmeralda-sized suffix (§5, if its tip ever
/// passes the activation height) would otherwise sit on the lock back to back for its whole duration.
///
/// `spawn_blocking` with `.await` rather than a plain blocking call, so the spawned task can still shut down when
/// the base node does.
async fn throttle_then_process_one_height<B: BlockchainBackend + 'static>(
    db_rw_lock: &Arc<RwLock<B>>,
    difficulty_calculator: &DifficultyCalculator,
    header_validator: &HeaderFullValidator,
    rules: &BaseNodeConsensusManager,
    height: u64,
    strict: Option<u64>,
) -> Result<Result<RebuildStep, ChainStorageError>, tokio::task::JoinError> {
    let throttle_ms = if strict.is_some() {
        STRICT_REBUILD_THROTTLE_MS
    } else {
        REBUILD_THROTTLE_MS
    };
    tokio::time::sleep(Duration::from_millis(throttle_ms)).await;

    let db = db_rw_lock.clone();
    let difficulty_calculator = difficulty_calculator.clone();
    let header_validator = header_validator.clone();
    let consensus_constants = rules.consensus_constants(height).clone();
    let rules = rules.clone();
    tokio::task::spawn_blocking(move || {
        process_accumulated_data_for_height(
            db,
            difficulty_calculator,
            &header_validator,
            &rules,
            height,
            &consensus_constants,
            strict,
        )
    })
    .await
}

/// The accumulated data rebuild walk itself, split out of `rebuild_accumulated_data_background_task` so that the
/// decision to spawn it and the walk it spawns can be read separately.
///
/// Climbs from the persisted watermark, one height per iteration, until a height reports itself as the last one.
/// The interesting part is what a single height can report - `RebuildStep::Processed`, `Retry`, `Resume`, or an
/// error - and the two bounds, `MAX_CONSECUTIVE_RETRIES` and `MAX_RESUMES`, on the two of those that can repeat.
/// Every way out other than `Processed` with `is_rebuilt: true` leaves `is_rebuilt: false`, so the next startup
/// resumes from the persisted watermark.
async fn run_accumulated_data_rebuild<B: BlockchainBackend + 'static>(
    db_rw_lock: Arc<RwLock<B>>,
    rules: BaseNodeConsensusManager,
    initial_status: AccumulatedDataRebuildStatus,
    strict_from_height: u64,
) {
    let difficulty_calculator = DifficultyCalculator::new(rules.clone(), RandomXFactory::new(1));
    let header_validator = HeaderFullValidator::new(rules.clone(), difficulty_calculator.clone());
    // The genesis block will not be at fault - start at height 1 if no data exists.
    let start_height = initial_status.last_rebuild_height.unwrap_or(1);
    let mut last_status = initial_status.clone();
    debug!(
        target: LOG_TARGET,
        "[AccData] Start rebuilding accumulated data from height {start_height} \
        (strict from height {strict_from_height})"
    );

    // The walk deliberately does not gate the add-block path while it runs, in either mode.
    // `disable_add_block_flag` would be the wrong instrument: the state machine sets it and clears it
    // around the whole header-sync to block-sync span, so two owners would silently overwrite each other -
    // this task's release would drop the state machine's gate mid-sync. And it is read in exactly one
    // place, the inbound propagated-block path; block sync and header sync never consult it.
    //
    // The write lock only makes a single operation atomic, so the strict path instead detects that the
    // chain moved under it and re-decides rather than proceeding - see
    // `process_accumulated_data_for_height_strict`, which compares both tips before it rewinds and comes
    // back as `RebuildStep::Retry` (a tip moved) or `RebuildStep::Resume` (this height's header moved) instead of
    // acting on a verdict it reached against a chain that is no longer there.
    let mut height = start_height;
    // A chain that is moving under the walk is normal (block sync appends, a reorg lands); a chain that
    // never settles long enough for one height to be validated against a stable state is not, and the
    // walk must not spin on it forever. After this many consecutive retries at the same height it stops,
    // leaving `is_rebuilt: false` so the next startup resumes from the persisted watermark.
    const MAX_CONSECUTIVE_RETRIES: u32 = 20;
    // A reorg that replaces a height the walk has already passed sends it back to the activation height,
    // because the walk is monotonic and cannot otherwise know that what it climbed is still there. A chain
    // reorganising that often is not one this walk can finish against, so the number of restarts is
    // bounded too; exceeding it stops with `is_rebuilt: false` and the next startup tries again.
    const MAX_RESUMES: u32 = 20;
    let mut retries: u32 = 0;
    let mut resumes: u32 = 0;
    loop {
        // Strict mode is decided per height, not per task. That is what keeps the pre-fork path intact: a
        // node walking 150,000 blocks from an earlier migration keeps its full throttle until it actually
        // reaches the fork.
        let strict = if height >= strict_from_height {
            Some(strict_from_height)
        } else {
            None
        };
        let res = throttle_then_process_one_height(
            &db_rw_lock,
            &difficulty_calculator,
            &header_validator,
            &rules,
            height,
            strict,
        )
        .await;
        match res {
            Ok(Ok(RebuildStep::Processed(current_status))) => {
                last_status = current_status;
                retries = 0;
            },
            // A tip moved while this height was being validated, but the height itself is untouched. The
            // height is deliberately *not* skipped - the verdict was simply reached against a chain that
            // has moved on, so it is reached again.
            Ok(Ok(RebuildStep::Retry)) => {
                retries = retries.saturating_add(1);
                if retries >= MAX_CONSECUTIVE_RETRIES {
                    error!(
                        target: LOG_TARGET,
                        "[AccData] Height {height} changed underneath the rebuild \
                        {MAX_CONSECUTIVE_RETRIES} times in a row; stopping. The next startup will resume \
                        from the persisted watermark. Last updated status: {last_status:?}"
                    );
                    break;
                }
                continue;
            },
            // A reorg replaced the header at this height, so it may have replaced any part of the prefix
            // the walk has already climbed. Climb it again from `from` rather than carrying on, so that
            // `is_rebuilt: true` is only ever written about a chain this walk has actually walked.
            Ok(Ok(RebuildStep::Resume { from })) => {
                resumes = resumes.saturating_add(1);
                if resumes >= MAX_RESUMES {
                    error!(
                        target: LOG_TARGET,
                        "[AccData] The chain reorganised underneath the rebuild {MAX_RESUMES} times; \
                        stopping. The next startup will resume from the persisted watermark. Last updated \
                        status: {last_status:?}"
                    );
                    break;
                }
                debug!(
                    target: LOG_TARGET,
                    "[AccData] Resuming the rebuild from height {from} (was at {height}), restart \
                    {resumes} of {MAX_RESUMES}"
                );
                height = from;
                retries = 0;
                continue;
            },
            // Any failure that is not a permanent-verdict validation failure - lock poisoning, an LMDB
            // error, a `spawn_blocking` join error, a verdict that can legitimately come out differently
            // later - leaves `is_rebuilt: false`, so the next startup resumes from the persisted
            // watermark. Deliberately not a rewind: that would turn a slow clock or a transient read into
            // a multi-hour resync. Until that restart the database holds corrected data below the failure
            // height and stale data above it, which is transient and self-healing.
            Ok(Err(e)) => {
                error!(
                    target: LOG_TARGET,
                    "[AccData] Rebuilding accumulated data failed. Initial status: {initial_status:?}. \
                    Last updated status: {last_status:?} ({e})"
                );
                break;
            },
            Err(e) => {
                error!(
                    target: LOG_TARGET,
                    "[AccData] Rebuilding accumulated data failed. Initial status: {initial_status:?}. \
                    Last updated status: {last_status:?} ({e})",
                );
                break;
            },
        }

        if last_status.is_rebuilt {
            debug!(
                target: LOG_TARGET,
                "[AccData] Rebuilding accumulated data from height {start_height} completed, Final status: {last_status:?}"
            );
            break;
        }
        height = height.saturating_add(1);
    }
}

/// What the rebuild walk should do next after one height.
#[derive(Debug)]
enum RebuildStep {
    /// The height was validated and its accumulated data rewritten. `status` carries the persisted watermark, and
    /// `is_rebuilt` on it tells the walk whether that was the last height.
    Processed(AccumulatedDataRebuildStatus),
    /// A tip moved underneath this height while it was being validated, but the header at this height and its
    /// parent are still the ones that were validated. Nothing was written, the watermark was not advanced, and the
    /// *same* height has to be read and validated again.
    Retry,
    /// The header at this height, or its parent, is not the one that was validated: a reorg replaced it. The walk
    /// goes back to `from` and climbs again from there.
    ///
    /// Deliberately not "skip and carry on", and deliberately not "retry this height in place" either.
    ///
    /// Not a skip, because the replacement header is not re-validated by the path that put it there:
    /// `rewind_to_height` seeds the orphan pool with `insert_chained_orphan`, which stores the pre-fork accumulated
    /// data verbatim, `reorganize_chain` feeds those back through `insert_best_block` which writes
    /// `header.accumulated_data()` as it found it and re-validates only the body, `insert_orphan_and_find_new_tips`
    /// returns early for a hash already in the orphan pool so the header never goes through the validator, and
    /// `restore_reorged_chain` does it with no validation at all. Skipping would therefore leave a stale row in
    /// place, and because the next height chains `from_previous(prev.accumulated_data())` it would carry that stale
    /// value forward over the whole rest of the suffix - and then mark the walk finished.
    ///
    /// Not a retry in place either, because the walk only ever moves up, so "this header changed" is evidence
    /// about the whole prefix and not just about this height. A reorg that splits *below* the walk's current
    /// height replaces every height from the split upwards, and `rewind_to_height` stores the heights it removed
    /// as chained orphans carrying their unrepaired accumulated data. Retrying in place would re-validate this one
    /// height on the new branch, run on to the tip and latch `is_rebuilt: true` over a prefix that was repaired on
    /// a branch that is no longer there - and a later reorg back onto it (`restore_reorged_chain` validates
    /// nothing) would re-insert exactly those stale rows into the main chain with the repair already marked
    /// finished. Going back to `from` and climbing again is what makes the finish line a statement about the chain
    /// the walk actually ended on.
    Resume { from: u64 },
}

/// Process the accumulated data rebuild for the given height.
///
/// `strict` selects between the two behaviours the walk needs, and is decided by the caller per height:
///
/// * `false` (below the fork) - recompute the target with `check_achieved_and_target_difficulty` and rewrite the
///   accumulated data. The stored data was computed under the rules in force at that height, so the only thing being
///   repaired is an earlier corruption; a failure here is the caller's to log and stop on.
/// * `true` (at or above the fork) - the stored data may have been computed under rules that no longer apply, so the
///   header is put through the full header validator first. A header that no longer validates *for a reason that is a
///   permanent property of the block* is not a corruption to repair but a block this node should never have accepted,
///   and the chain is rewound below it.
fn process_accumulated_data_for_height<B: BlockchainBackend>(
    db: Arc<RwLock<B>>,
    difficulty_calculator: DifficultyCalculator,
    header_validator: &HeaderFullValidator,
    rules: &BaseNodeConsensusManager,
    height: u64,
    consensus_constants: &ConsensusConstants,
    strict: Option<u64>,
) -> Result<RebuildStep, ChainStorageError> {
    debug!(target: LOG_TARGET, "[AccData] Processing accumulated data rebuilding for height {height} (strict: {strict:?})");

    if let Some(activation) = strict {
        return process_accumulated_data_for_height_strict(
            db,
            header_validator,
            rules,
            height,
            consensus_constants,
            activation,
        );
    }

    let write_lock = db
        .write()
        .map_err(|_e| ChainStorageError::AccessError("Write lock on blockchain backend failed".into()))?;
    let last_chain_header = write_lock.fetch_last_chain_header()?;
    // Safety check to ensure we do not rebuild accumulated data for a height that has been reorged out.
    let height = min(height, last_chain_header.height());

    // Rebuild the accumulated data for the given height
    let chain_header = write_lock.fetch_chain_header_by_height(height)?;
    let header = chain_header.header().clone();
    let prev_chain_header = write_lock.fetch_chain_header_by_height(height.saturating_sub(1))?;

    let achieved_difficulty = difficulty_calculator.check_achieved_and_target_difficulty(&*write_lock, &header)?;

    let accumulated_data = BlockHeaderAccumulatedDataBuilder::from_previous(prev_chain_header.accumulated_data())
        .with_hash(header.hash())
        .with_achieved_target_difficulty(achieved_difficulty)
        .with_total_kernel_offset(header.total_kernel_offset.clone())
        .build(consensus_constants)?;

    let status = write_lock.update_accumulated_difficulty(height, accumulated_data, last_chain_header, true)?;

    Ok(RebuildStep::Processed(status))
}

/// The strict half of `process_accumulated_data_for_height`, split out because it is the half that has to be
/// careful with the write lock, with what it is allowed to conclude, and with what it is allowed to leave behind.
///
/// The validation is the expensive part - `strict_validate_header` runs the full header validator, which spins up a
/// RandomX VM - and it is read-only, so it runs under the *read* lock. Holding the write lock across it would freeze
/// every gRPC query, RPC sync server and mempool read for the duration, per height, for the whole suffix.
///
/// Dropping the lock in between means the chain may move underneath us, so the write phase re-reads the state the
/// decision was taken against and compares it before acting. What has to match depends on the decision: rewriting
/// the accumulated data at `height` only needs `height` and its parent to be unchanged, while a *rewind* is a
/// statement about the whole chain above `height` and is only carried out against exactly the tips it was decided
/// against. A tip that moved under an otherwise stable height comes back as `RebuildStep::Retry` and the height is
/// validated again; a header that moved comes back as `RebuildStep::Resume` and the walk climbs the strict suffix
/// again, because the walk is monotonic and a reorg below it invalidates the prefix too.
fn process_accumulated_data_for_height_strict<B: BlockchainBackend>(
    db: Arc<RwLock<B>>,
    header_validator: &HeaderFullValidator,
    rules: &BaseNodeConsensusManager,
    height: u64,
    consensus_constants: &ConsensusConstants,
    activation: u64,
) -> Result<RebuildStep, ChainStorageError> {
    // Phase 1, under the read lock: pick the height, classify it, and validate the header there.
    let (header, prev_hash, snapshot, header_only, validation) = {
        let read_lock = db
            .read()
            .map_err(|_e| ChainStorageError::AccessError("Read lock on blockchain backend failed".into()))?;

        let snapshot = ChainTips::read(&*read_lock)?;

        // Which heights to repair, and where a rewind is legal, are two different questions. This is the first
        // one, and the answer is every height up to the *header* tip.
        //
        // Stopping at the block tip instead is wrong on the only path that matters here, the upgrade path. Header
        // sync running ahead of block sync is the ordinary steady state, not an edge case, and nothing ever goes
        // back over the headers it left behind: header sync neither re-validates nor rewrites a header it already
        // has, and takes the *stored* accumulated data at the split as its accumulation base; nothing at startup
        // rewinds headers above the block tip; and block sync writes the stored `BlockHeaderAccumulatedData` back
        // verbatim as each body arrives, promoting its `total_accumulated_difficulty` straight into chain
        // metadata. A node that upgraded with blocks at 350,020 and headers at 350,053 would therefore have
        // [350,000, 350,020] repaired and 350,021-350,053 left carrying the old binary's pre-fork-window targets -
        // chained off the repaired prefix, and with the walk marked finished so nothing ever returns to it.
        //
        // The second question is answered further down: a permanent verdict at a height above the block tip is
        // *not* rewound, because the guards that bound a rewind are all computed from the block tip. See
        // `StrictAction::StopHeaderOnly`.
        let header_only = height > snapshot.block_height;

        if height > snapshot.header_height {
            drop(read_lock);
            return resume_or_finish_above_the_header_tip(db, height, activation);
        }

        let header = read_lock.fetch_chain_header_by_height(height)?.header().clone();
        let prev_chain_header = read_lock.fetch_chain_header_by_height(height.saturating_sub(1))?;
        let validation = strict_validate_header(&*read_lock, header_validator, rules, &header, &prev_chain_header);
        (header, *prev_chain_header.hash(), snapshot, header_only, validation)
    };

    // Phase 2, under the write lock: confirm the state the decision was taken against still holds, then act.
    let mut write_lock = db
        .write()
        .map_err(|_e| ChainStorageError::AccessError("Write lock on blockchain backend failed".into()))?;
    let last_chain_header = write_lock.fetch_last_chain_header()?;
    let header_moved = height > last_chain_header.height() ||
        *write_lock.fetch_chain_header_by_height(height)?.hash() != header.hash() ||
        *write_lock
            .fetch_chain_header_by_height(height.saturating_sub(1))?
            .hash() !=
            prev_hash;
    let tips_moved = ChainTips::read(&*write_lock)? != snapshot;

    match decide_strict_action(header_moved, tips_moved, header_only, validation.as_ref().err()) {
        StrictAction::Retry => {
            info!(
                target: LOG_TARGET,
                "[AccData] A chain tip moved underneath height {height} while it was being validated; validating \
                it again rather than advancing past it."
            );
            return Ok(RebuildStep::Retry);
        },
        StrictAction::Resume => {
            info!(
                target: LOG_TARGET,
                "[AccData] The header at height {height} (or its parent) is no longer the one that was validated, \
                so a reorg replaced it while it was being validated. The split may be anywhere below this height, \
                and the heights the walk has already passed were repaired on a branch that may be gone, so the \
                strict suffix is walked again from the activation height {activation}."
            );
            return Ok(RebuildStep::Resume { from: activation });
        },
        StrictAction::StopHeaderOnly => {
            // Repairing a header-only height is well defined; rewinding at one is not. `rewind_to_height`
            // measures its block removals from the block tip, so every guard that bounds a rewind - the pruned
            // node refusal, the "how many blocks are about to go" accounting - is computed against a chain that
            // does not reach up here, while the header deletion it *would* perform is measured from the header
            // tip and could run tens of thousands of headers deep on one verdict. Stop instead, leaving
            // `is_rebuilt: false`. This is not a dead end: once block sync brings the body in, the same header is
            // reached again at a height where the rewind is a bounded, well defined operation.
            let e = validation.expect_err("StrictAction::StopHeaderOnly is only reached with a verdict");
            warn!(
                target: LOG_TARGET,
                "[AccData] The header at height {height} is above the block tip {} and no longer validates under \
                the current consensus rules ({e}). A rewind is not attempted at a header-only height, so the \
                rebuild stops here and resumes on the next startup.",
                snapshot.block_height
            );
            return Err(ChainStorageError::ValidationError { source: e });
        },
        StrictAction::Stop => {
            // Not a property of the block: a wall clock that is behind the network, a seed age rule that belongs
            // to the chain rather than to the header, a transient LMDB read, a missing row. Rewinding on these
            // would delete a perfectly good chain because a container had not finished talking to NTP yet. Stop
            // instead, leaving `is_rebuilt: false`, so the next startup resumes from the persisted watermark and
            // tries the same height again.
            let e = validation.expect_err("StrictAction::Stop is only reached with a verdict");
            warn!(
                target: LOG_TARGET,
                "[AccData] Validation of the block at height {height} could not be completed ({e}). This verdict \
                is not a permanent property of the block, so the rebuild stops here and resumes on the next \
                startup rather than rewinding the chain."
            );
            return Err(ChainStorageError::ValidationError { source: e });
        },
        StrictAction::Rewind => {
            let e = validation.expect_err("StrictAction::Rewind is only reached with a verdict");
            return rewind_below_invalid_header(&mut *write_lock, height, &e).map(RebuildStep::Processed);
        },
        StrictAction::Rewrite => {},
    }

    let achieved_difficulty = validation.expect("StrictAction::Rewrite is only reached with a valid header");
    let prev_chain_header = write_lock.fetch_chain_header_by_height(height.saturating_sub(1))?;
    let accumulated_data = BlockHeaderAccumulatedDataBuilder::from_previous(prev_chain_header.accumulated_data())
        .with_hash(header.hash())
        .with_achieved_target_difficulty(achieved_difficulty)
        .with_total_kernel_offset(header.total_kernel_offset.clone())
        .build(consensus_constants)?;

    write_lock
        .update_accumulated_difficulty(height, accumulated_data, last_chain_header, true)
        .map(RebuildStep::Processed)
}

/// What to do when the strict walk finds itself at a height above the header tip.
///
/// The walk only ever climbs, and the ordinary way it finishes is by rewriting the header tip itself, where
/// `update_accumulated_difficulty` records `is_rebuilt: height == last_chain_header.height()`. So arriving *above*
/// the header tip means the chain got shorter underneath the walk: a reorg replaced heights it had already
/// passed, and the branch it repaired them on is gone. Nothing may be concluded from that, least of all
/// "finished" - the strict suffix is walked again from the activation height.
///
/// The one exception is a chain that no longer reaches the fork at all. There is then no strict height left to
/// repair, and re-walking would only spin until the resume budget ran out.
///
/// The tip is re-read under the write lock rather than taken from the read phase's snapshot, so that the decision
/// to write `is_rebuilt: true` - a one-way latch - is never taken against a chain that has since grown back past
/// the activation height.
fn resume_or_finish_above_the_header_tip<B: BlockchainBackend>(
    db: Arc<RwLock<B>>,
    height: u64,
    activation: u64,
) -> Result<RebuildStep, ChainStorageError> {
    let write_lock = db
        .write()
        .map_err(|_e| ChainStorageError::AccessError("Write lock on blockchain backend failed".into()))?;
    let header_tip = write_lock.fetch_last_header()?.height;
    if header_tip >= activation {
        info!(
            target: LOG_TARGET,
            "[AccData] The strict rebuild reached height {height}, above the header tip {header_tip}, so the \
            chain grew shorter underneath it. Resuming from the activation height {activation} rather than \
            declaring the repair finished over a branch that is no longer there."
        );
        return Ok(RebuildStep::Resume { from: activation });
    }
    debug!(
        target: LOG_TARGET,
        "[AccData] The header tip {header_tip} is below the activation height {activation}; there is no post-fork \
        accumulated data left to repair."
    );
    let status = AccumulatedDataRebuildStatus {
        is_rebuilt: true,
        last_rebuild_height: Some(header_tip),
    };
    write_lock.set_accumulated_data_rebuild_status(status.clone())?;
    Ok(RebuildStep::Processed(status))
}

/// What the write phase of a strict height should do, given the state the decision in the read phase was taken
/// against and the state the database is actually in now.
#[derive(Debug, PartialEq, Eq)]
enum StrictAction {
    /// The header is unchanged and it validated: rewrite its accumulated data.
    Rewrite,
    /// Re-read and re-validate the same height. Nothing may be written and the watermark may not advance.
    Retry,
    /// The header at this height is not the one that was validated. Go back to the activation height and climb
    /// again, so the finish line is a statement about the chain the walk actually ended on.
    Resume,
    /// The header is unchanged, it failed on a permanent property of the block, and the chain is exactly the one
    /// that verdict was reached against: rewind below it.
    Rewind,
    /// Stop the walk with `is_rebuilt` still false, so the next startup resumes from the persisted watermark.
    Stop,
    /// The same, for a permanent verdict at a height above the block tip, where a rewind is not a bounded
    /// operation. Split out from `Stop` only so the two can be told apart in the log and in the tests.
    StopHeaderOnly,
}

/// The whole write-phase decision, as a pure function so that every branch of it can be tested directly rather
/// than raced into existence.
///
/// * `header_moved` - the header at this height, or its parent, is not the one that was validated. The verdict,
///   whatever it was, belongs to a header that is no longer there, so nothing may be concluded from it. Critically this
///   is neither a skip nor a retry in place but a `Resume`, for the reasons on `RebuildStep::Resume`: the walk is
///   monotonic, so a changed header is evidence that a reorg may have replaced any part of the prefix the walk has
///   already climbed, and only re-climbing it makes `is_rebuilt: true` a statement about the chain the walk ended on.
/// * `tips_moved` - either chain tip changed, but this height and its parent did not. Only a rewind cares: rewriting
///   one height's accumulated data is a statement about that height, while a rewind is a statement about every block
///   above it, and must not be executed against a chain that is no longer the one it was decided against. This is what
///   stands in for the add-block gate that is deliberately not reintroduced: sync is not held off, it is detected.
/// * `header_only` - this height is above the block tip. Its accumulated data is repaired like any other, but a
///   permanent verdict here stops the walk instead of rewinding, because a rewind at a header-only height is not a
///   bounded operation. Ordered *after* the `tips_moved` row on purpose: `header_only` was read from the phase 1
///   snapshot, and the only rows that consult it are the ones where the snapshot has been confirmed to still hold.
fn decide_strict_action(
    header_moved: bool,
    tips_moved: bool,
    header_only: bool,
    verdict: Option<&ValidationError>,
) -> StrictAction {
    if header_moved {
        return StrictAction::Resume;
    }
    match verdict {
        None => StrictAction::Rewrite,
        Some(e) if verdict_can_change(e) => StrictAction::Stop,
        Some(_) if tips_moved => StrictAction::Retry,
        Some(_) if header_only => StrictAction::StopHeaderOnly,
        Some(_) => StrictAction::Rewind,
    }
}

/// The block tip and the header tip, as one comparable snapshot.
///
/// Both are needed: the block tip is what a rewind removes blocks down to, and the header tip is what it removes
/// *headers* down from, which during header sync can be tens of thousands of heights higher.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ChainTips {
    block_height: u64,
    block_hash: HashOutput,
    header_height: u64,
    header_hash: HashOutput,
}

impl ChainTips {
    fn read<B: BlockchainBackend>(db: &B) -> Result<Self, ChainStorageError> {
        let metadata = db.fetch_chain_metadata()?;
        let last_header = db.fetch_last_header()?;
        Ok(Self {
            block_height: metadata.best_block_height(),
            block_hash: *metadata.best_block_hash(),
            header_height: last_header.height,
            header_hash: last_header.hash(),
        })
    }
}

/// Whether a validation verdict is one that can legitimately come out differently later, and therefore must never
/// be allowed to destroy blocks.
///
/// This is the same notion the rest of the codebase already uses to decide whether a failed header may be memoed in
/// `bad_blocks` - see `BlockHeaderSyncValidator::blacklist_unless_verdict_can_change` and the equivalent match in
/// `insert_orphan_and_find_new_tips` - and it is reused rather than restated so the three cannot drift apart. A
/// verdict that can change stops the walk with `is_rebuilt` still false, so the next startup tries again.
///
/// The two named cases are the ones that are about the *environment* or the *chain* rather than the header:
/// `InvalidTimestampFutureTimeLimit` is measured against `Utc::now()`, so a node whose clock is more than the
/// future time limit slow - a VM resumed from suspend, a container that has not finished talking to NTP - fails it
/// on every recent header, and `OldSeedHash` is measured against the chain the header is on.
///
/// Everything else defers to `ValidationError::get_ban_reason`, whose `None` arm is precisely the set of verdicts
/// that are not the block's fault: `FatalStorageError` (which is what *any* `ChainStorageError` becomes when it
/// crosses into validation, so a transient LMDB read failure lands here), `IncorrectNumberOfTimestampsProvided`,
/// the missing-row errors, and the hash/height mismatches that describe the query rather than the block.
fn verdict_can_change(err: &ValidationError) -> bool {
    match err {
        ValidationError::BlockHeaderError(BlockHeaderValidationError::InvalidTimestampFutureTimeLimit) |
        ValidationError::BlockHeaderError(BlockHeaderValidationError::OldSeedHash) => true,
        e => e.get_ban_reason().is_none(),
    }
}

/// Run the full header validator over a header that is already committed to the main chain.
///
/// `target_difficulty: None` routes the validator through the same `check_achieved_and_target_difficulty` the
/// non-strict path calls, so the returned `AchievedTargetDifficulty` is identical - but it *additionally* enforces
/// the activation rules the difficulty calculator cannot see on its own (`require_canonical_randomxt_pow_data`,
/// `aux_chain_merkle_proof_depth_binding` and `strict_merkle_tree_parameter_decoding`). Without that, a block above
/// the activation height that is invalid for one of those three would pass the rebuild, and the node would stay on
/// a chain it would reject if it ever re-synced from scratch.
///
/// The chain context is built the same way the orphan path builds it. The header is on the main chain, so its
/// ancestry meets the main chain at `height - 1` and that is what the database may be trusted up to; anything
/// recorded at `height` itself is this header's own contribution and must not be read back as evidence against it.
///
/// Everything here is read-only, hence `&B`: the caller runs it under the read lock, because the RandomX work
/// inside the validator is much too long to hold the write lock across.
fn strict_validate_header<B: BlockchainBackend>(
    db: &B,
    header_validator: &HeaderFullValidator,
    rules: &BaseNodeConsensusManager,
    header: &BlockHeader,
    prev_chain_header: &ChainHeader,
) -> Result<AchievedTargetDifficulty, ValidationError> {
    let prev_timestamps = get_previous_timestamps(db, header, rules)?;
    let (vm_key, fork_height) = get_vm_key_for_candidate_header(db, header.clone())?;
    let chain_context = HeaderChainContext::candidate_chain(vm_key, fork_height, None);
    let validated = header_validator.validate(
        db,
        header,
        prev_chain_header.header(),
        &prev_timestamps,
        None,
        chain_context,
    )?;
    Ok(validated.achieved_target)
}

/// Decide whether the rewind that is about to be performed is one this node is able to perform. Returns an error -
/// and rewinds nothing - if it is not.
///
/// The one refusal is a pruned node whose history no longer reaches the rewind target: `rewind_to_height` silently
/// turns a rewind deeper than `best_block_height - pruned_height` into a rewind to height 0, a full destructive
/// wipe, and in that branch it skips `insert_chained_orphan` but still calls `insert_orphan_chain_tip`, leaving a
/// tip whose hash resolves to nothing. A background task must never do that on its own.
///
/// Deliberately asked here, about the rewind actually being attempted, and deliberately *not* hoisted to the top of
/// the walk. An earlier revision hoisted it by expanding the condition for the deepest rewind the walk could ever
/// need, `activation - 1`: `tip - (activation - 1) > tip - pruned_height` reduces to `pruned_height >= activation`.
/// That predicate is independent of the tip, which is exactly what makes it useless as an up-front guard - on a
/// pruned node `pruned_height ~ tip - pruning_horizon`, so it is false at rollout and becomes *permanently* true
/// once the chain has advanced one pruning horizon past the fork. From then on every startup refused the repair at
/// its very first strict height, forever, logging that the node had to be resynced, on a node where in all
/// likelihood nothing failed validation and no rewind would ever have been attempted. Pruned nodes are the majority
/// of the network; excluding all of them permanently to pre-empt a rewind that is rare is the wrong trade.
///
/// What checking late costs is that `[activation, H-1]` is left with recomputed targets while `[H, tip]` still
/// carries the old ones. That is not a new state: `StrictAction::Stop`, `StrictAction::StopHeaderOnly` and the
/// consecutive-retry bound all produce it deliberately, and the up-front check produced it anyway whenever a pruner
/// crossed the activation height mid-walk. It is transient in all of those cases for the same reason - `is_rebuilt`
/// stays false, so the next startup resumes from the persisted watermark and walks the suffix again.
///
/// There is deliberately no cap on how many blocks the rewind may remove. An earlier revision had one, at 4,096
/// blocks, which is about five days of MainNet block time - so it would have refused precisely the late upgraders
/// the repair exists for, and been a wall-clock race against the fork.
fn refuse_if_rewind_is_impossible<B: BlockchainBackend>(db: &B, rewind_to: u64) -> Result<(), ChainStorageError> {
    let metadata = db.fetch_chain_metadata()?;
    let tip = metadata.best_block_height();
    let steps_back = tip.saturating_sub(rewind_to);
    let effective_pruning_horizon = tip.saturating_sub(metadata.pruned_height());
    if metadata.is_pruned_node() && steps_back > effective_pruning_horizon {
        error!(
            target: LOG_TARGET,
            "[AccData] The accumulated data repair needs to rewind this chain to height {rewind_to} \
            ({steps_back} blocks), which is past this pruned node's effective pruning horizon of \
            {effective_pruning_horizon}; `rewind_to_height` would turn that into a wipe to height 0. Refusing the \
            rewind: the chain is left as it is and the repair stops unfinished, so it is attempted again on the \
            next startup. The blocks it needs are genuinely gone, so this node has to be resynced to complete it."
        );
        return Err(ChainStorageError::InvalidOperation(format!(
            "Accumulated data rebuild needs to rewind {steps_back} blocks to height {rewind_to}, past this pruned \
             node's effective pruning horizon of {effective_pruning_horizon}"
        )));
    }
    Ok(())
}

/// Rewind the chain to just below `height`, because the block at `height` no longer validates under the rules that
/// are now in force, and finish the rebuild there.
///
/// There is deliberately nothing left for the walk to do afterwards - every height above the rewind point no longer
/// exists - so the status is written as rebuilt at `height - 1` and the caller terminates.
///
/// The outcome is a single one. `refuse_if_rewind_is_impossible` has already run, read-only, before this walk wrote
/// anything, so by the time execution reaches here the answer is "rewind", and the orphan cleanup and the status
/// that record it are committed in one transaction built without a single fallible call in it. That matters more
/// than it looks: the cleanup is the only thing standing between the repaired chain and an immediate reorg back
/// onto the blocks just removed. `rewind_to_height` inserts an orphan chain tip for the former tip carrying its
/// inflated pre-fork `total_accumulated_difficulty`, `swap_to_highest_pow_chain` ranks that above the repaired
/// (lower) main tip, and `reorganize_chain` validates only bodies while `insert_orphan_and_find_new_tips`
/// short-circuits on a hash already in the orphan pool - so the removed blocks would go straight back with their
/// stale accumulated data, and with the walk finished nothing would re-arm.
///
/// The orphan *chain tip* entries are removed with `remove_orphan_chain_tip_if_exists` rather than left to
/// `delete_orphan`, which returns early when the block is absent from `orphans_db`, and rather than probed first
/// with `fetch_orphan_chain_tip_by_hash`, which returns `Err(ValueNotFound)` rather than `Ok(None)` for exactly the
/// dangling-tip case that makes the explicit removal necessary.
fn rewind_below_invalid_header<B: BlockchainBackend>(
    write_lock: &mut B,
    height: u64,
    reason: &ValidationError,
) -> Result<AccumulatedDataRebuildStatus, ChainStorageError> {
    let rewind_to = height.saturating_sub(1);

    // Asked here, about this rewind, and nowhere else - see `refuse_if_rewind_is_impossible` for why hoisting it
    // to the top of the walk permanently excluded every pruned node.
    refuse_if_rewind_is_impossible(&*write_lock, rewind_to)?;

    let metadata = write_lock.fetch_chain_metadata()?;
    let tip = metadata.best_block_height();
    let header_tip = write_lock.fetch_last_header()?.height;
    warn!(
        target: LOG_TARGET,
        "[AccData] Block at height {height} no longer validates under the current consensus rules ({reason}); \
        rewinding the chain to height {rewind_to} ({} block(s), and {} header(s) from the header tip {header_tip})",
        tip.saturating_sub(rewind_to),
        header_tip.saturating_sub(cmp::max(tip, rewind_to)),
    );
    let removed_blocks = rewind_to_height(write_lock, rewind_to)?;

    // `rewind_to_height` keeps the blocks it removes as orphans, so an ordinary reorg can restore them. These are
    // being removed precisely because they do not validate, and their accumulated data was built from pre-fork
    // targets, so keeping them would leave an orphan tip whose inflated `total_accumulated_difficulty` can win an
    // `AccumulatedDifficultySquaredComparer` comparison it should lose. Delete them instead; anything that
    // genuinely mattered is re-fetched.
    //
    // One transaction, and not a single fallible call between the rewind and it: the chain being rewound, the
    // orphans being gone and the rebuild being finished are one outcome, never two thirds of one.
    let status = AccumulatedDataRebuildStatus {
        is_rebuilt: true,
        last_rebuild_height: Some(rewind_to),
    };
    let mut txn = DbTransaction::new();
    for block in &removed_blocks {
        let hash = *block.hash();
        txn.remove_orphan_chain_tip_if_exists(hash);
        txn.delete_orphan(hash);
    }
    txn.set_accumulated_data_rebuild_status(status.clone());
    write_lock.write(txn)?;
    Ok(status)
}

// Verify accumulated data for the given height, fixing it if needed
fn verify_accumulated_data_for_height<B: BlockchainBackend>(
    db: Arc<RwLock<B>>,
    difficulty_calculator: DifficultyCalculator,
    height: u64,
    consensus_constants: &ConsensusConstants,
    autocorrect: bool,
) -> Result<BlockchainCheckStatus, ChainStorageError> {
    debug!(target: LOG_TARGET, "[AccData check] Checking accumulated data for height {height}");

    let read_lock = db
        .read()
        .map_err(|_e| ChainStorageError::AccessError("Read lock on blockchain backend failed".into()))?;
    let last_chain_header = read_lock.fetch_last_chain_header()?;
    // Safety check to ensure we do not check accumulated data for a height that has been reorged out.
    let height = min(height, last_chain_header.height());

    // Check the accumulated data for the given height
    let chain_header = read_lock.fetch_chain_header_by_height(height)?;
    let header = chain_header.header().clone();
    let prev_chain_header = read_lock.fetch_chain_header_by_height(height.saturating_sub(1))?;

    let achieved_difficulty = difficulty_calculator.check_achieved_and_target_difficulty(&*read_lock, &header)?;
    drop(read_lock);

    let calculated_accumulated_data =
        BlockHeaderAccumulatedDataBuilder::from_previous(prev_chain_header.accumulated_data())
            .with_hash(header.hash())
            .with_achieved_target_difficulty(achieved_difficulty)
            .with_total_kernel_offset(header.total_kernel_offset.clone())
            .build(consensus_constants)?;

    let current_accumulated_data = chain_header.accumulated_data();

    if &calculated_accumulated_data == current_accumulated_data {
        trace!(
            target: LOG_TARGET,
            "[AccData check] Accumulated data for height {height} is correct. No update needed."
        );
    } else if autocorrect {
        let write_lock = db
            .write()
            .map_err(|_e| ChainStorageError::AccessError("Write lock on blockchain backend failed".into()))?;
        write_lock.update_accumulated_difficulty(height, calculated_accumulated_data, last_chain_header, false)?;
        info!(
            target: LOG_TARGET,
            "[AccData check] Accumulated data for height {height} was corrupted, but rebuilt."
        );
    } else {
        return Err(ChainStorageError::CorruptedDatabase(format!(
            "Accumulated data for height {height} is corrupted."
        )));
    }

    let write_lock = db
        .write()
        .map_err(|_e| ChainStorageError::AccessError("Write lock on blockchain backend failed".into()))?;
    let last_chain_header = write_lock.fetch_last_chain_header()?;
    let status = write_lock.update_accumulated_data_check_status(BlockchainCheckRequest::SetCheckResult {
        has_concluded: height == last_chain_header.height(),
        last_check_height: height,
        current_height: last_chain_header.height(),
    })?;

    Ok(status)
}

// Verify blockchain consistency for the given height
fn verify_blockchain_consistency_for_height<B: BlockchainBackend>(
    db: Arc<RwLock<B>>,
    validators: &Validators<B>,
    height: u64,
    full_validation: bool,
) -> Result<BlockchainCheckStatus, ChainStorageError> {
    debug!(target: LOG_TARGET, "[Blockchain check] Checking blockchain data for height {height} with full_validation({full_validation})");

    let read_lock = db
        .read()
        .map_err(|_e| ChainStorageError::AccessError("Read lock on blockchain backend failed".into()))?;
    let last_chain_header = read_lock.fetch_last_chain_header()?;
    // Safety check to ensure we do not check accumulated data for a height that has been reorged out.
    let height = min(height, last_chain_header.height());

    let block_data = {
        let metadata = read_lock.fetch_chain_metadata()?;
        let horizon_height = metadata.pruned_height_at_given_chain_tip(height);
        if height > horizon_height {
            let historical_block = fetch_block(&*read_lock, height, false).map_err(|e| {
                ChainStorageError::CorruptedDatabase(format!("Could not fetch block for height {height}: {e}"))
            })?;
            Some((
                historical_block.block().clone(),
                historical_block.accumulated_data().clone(),
            ))
        } else {
            None
        }
    };
    let prev_chain_header = read_lock.fetch_chain_header_by_height(height.saturating_sub(1))?;
    let this_block_header = if let Some((ref block, ref _accumulated_data)) = block_data {
        block.header.clone()
    } else {
        read_lock.fetch_chain_header_by_height(height)?.header().clone()
    };
    drop(read_lock);

    // Simple consistency checks
    if &this_block_header.prev_hash != prev_chain_header.hash() {
        return Err(ChainStorageError::CorruptedDatabase(format!(
            "Block at height {height} has invalid previous hash"
        )));
    }
    if this_block_header.height != prev_chain_header.height().saturating_add(1) {
        return Err(ChainStorageError::CorruptedDatabase(format!(
            "Block at height {height} does not follow previous header height"
        )));
    }

    // Full validation of block body and internal consistency if requested
    if full_validation && let Some((block, accumulated_data)) = block_data {
        let read_lock = db
            .read()
            .map_err(|_e| ChainStorageError::AccessError("Read lock on blockchain backend failed".into()))?;
        let block_hash = block.hash();
        let accumulated_data_hash = accumulated_data.hash;
        let chain_block = ChainBlock::try_construct(Arc::new(block), accumulated_data).ok_or_else(|| {
            ChainStorageError::CorruptedDatabase(format!(
                "Inconsistent hash in historical block: block hash {} vs. acc_data hash {}",
                block_hash, accumulated_data_hash
            ))
        })?;
        let block_validator = validators.block.clone();
        block_validator
            .validate_body_at_height(&read_lock, &chain_block)
            .map_err(|e| {
                ChainStorageError::CorruptedDatabase(format!("Block body validation failed for height {height}: {e}"))
            })?;

        let orphan_validator = validators.orphan.clone();
        orphan_validator
            .validate_internal_consistency(chain_block.block())
            .map_err(|e| {
                ChainStorageError::CorruptedDatabase(format!(
                    "Block internal consistency validation failed for height {height}: {e}"
                ))
            })?;
    }

    let write_lock = db
        .write()
        .map_err(|_e| ChainStorageError::AccessError("Write lock on blockchain backend failed".into()))?;
    let last_chain_header = write_lock.fetch_last_chain_header()?;
    let status = write_lock.update_blockchain_consistency_check_status(BlockchainCheckRequest::SetCheckResult {
        has_concluded: height == last_chain_header.height(),
        last_check_height: height,
        current_height: last_chain_header.height(),
    })?;

    Ok(status)
}

#[cfg(test)]
mod test {
    // Overflow in test code panics, which is the desired failure mode for a test.
    #![allow(clippy::arithmetic_side_effects)]
    #![allow(clippy::indexing_slicing)]
    use std::{collections::HashMap, sync};

    use rand::seq::SliceRandom;
    use tari_common::configuration::Network;
    use tari_test_utils::unpack_enum;
    use tari_transaction_components::{
        consensus::{
            ConsensusConstantsBuilder,
            consensus_constants::{POW_BACKOFF_DISABLED, PowAlgorithmConstants, UNSCHEDULED_ACTIVATION_HEIGHT},
        },
        tari_proof_of_work::Difficulty,
        transaction_components::RangeProofType,
    };

    use super::*;
    use crate::{
        block_specs,
        consensus::chain_strength_comparer::strongest_chain,
        proof_of_work::{AchievedTargetDifficulty, AdjustedTarget, sha3x_difficulty},
        test_helpers::{
            BlockSpecs,
            blockchain::{
                TempDatabase,
                create_chained_blocks,
                create_main_chain,
                create_new_blockchain,
                create_orphan_chain,
                create_test_blockchain_db,
            },
        },
        validation::{ValidatedHeader, header::HeaderFullValidator, mocks::MockValidator},
    };

    #[test]
    fn lmdb_fetch_monero_seeds() {
        let db = create_test_blockchain_db();
        let seed = b"test1";
        {
            let db_read = db.db_read_access().unwrap();
            assert_eq!(db_read.fetch_monero_seed_first_seen_height(&seed[..]).unwrap(), 0);
        }
        {
            let mut txn = DbTransaction::new();
            txn.insert_monero_seed_height(seed.to_vec(), 5);
            let mut db_write = db.test_db_write_access().unwrap();
            assert!(db_write.write(txn).is_ok());
        }
        {
            let db_read = db.db_read_access().unwrap();
            assert_eq!(db_read.fetch_monero_seed_first_seen_height(&seed[..]).unwrap(), 5);
        }

        {
            let mut txn = DbTransaction::new();
            txn.insert_monero_seed_height(seed.to_vec(), 2);
            let mut db_write = db.db_write_access().unwrap();
            assert!(db_write.write(txn).is_ok());
        }
        {
            let db_read = db.db_read_access().unwrap();
            assert_eq!(db_read.fetch_monero_seed_first_seen_height(&seed[..]).unwrap(), 2);
        }
    }

    mod get_orphan_link_main_chain {
        use super::*;

        #[tokio::test]
        async fn it_gets_a_simple_link_to_genesis() {
            let db = create_new_blockchain();
            let genesis = db
                .fetch_block(0, true)
                .unwrap()
                .try_into_chain_block()
                .map(Arc::new)
                .unwrap();
            let (_, chain) =
                create_orphan_chain(&db, &[("A->GB", 1, 120), ("B->A", 1, 120), ("C->B", 1, 120)], genesis);
            let access = db.db_read_access().unwrap();
            let orphan_chain = get_orphan_link_main_chain(&*access, chain.get("C").unwrap().hash()).unwrap();
            assert_eq!(orphan_chain[2].hash(), chain.get("C").unwrap().hash());
            assert_eq!(orphan_chain[1].hash(), chain.get("B").unwrap().hash());
            assert_eq!(orphan_chain[0].hash(), chain.get("A").unwrap().hash());
            assert_eq!(orphan_chain.len(), 3);
        }

        #[tokio::test]
        async fn it_selects_a_large_reorg_chain() {
            let db = create_new_blockchain();
            // Main chain
            let (_, mainchain) = create_main_chain(&db, &[
                ("A->GB", 1, 120),
                ("B->A", 1, 120),
                ("C->B", 1, 120),
                ("D->C", 1, 120),
            ]);
            // Create reorg chain
            let fork_root = mainchain.get("B").unwrap().clone();
            let (_, reorg_chain) = create_orphan_chain(
                &db,
                &[
                    ("C2->GB", 2, 120),
                    ("D2->C2", 1, 120),
                    ("E2->D2", 1, 120),
                    ("F2->E2", 1, 120),
                ],
                fork_root,
            );
            let access = db.db_read_access().unwrap();
            let orphan_chain = get_orphan_link_main_chain(&*access, reorg_chain.get("F2").unwrap().hash()).unwrap();

            assert_eq!(orphan_chain[3].hash(), reorg_chain.get("F2").unwrap().hash());
            assert_eq!(orphan_chain[2].hash(), reorg_chain.get("E2").unwrap().hash());
            assert_eq!(orphan_chain[1].hash(), reorg_chain.get("D2").unwrap().hash());
            assert_eq!(orphan_chain[0].hash(), reorg_chain.get("C2").unwrap().hash());
            assert_eq!(orphan_chain.len(), 4);
        }

        #[test]
        fn it_errors_if_orphan_not_exist() {
            let db = create_new_blockchain();
            let access = db.db_read_access().unwrap();
            let err = get_orphan_link_main_chain(&*access, &FixedHash::zero()).unwrap_err();
            assert!(matches!(err, ChainStorageError::InvalidOperation(_)));
        }
    }

    /// `fork_height` is the one thing keeping the orphan path's chain dependent checks honest: it says how far down
    /// the candidate's chain and the chain in the database are the same chain, and everything the database recorded
    /// above it has to be ignored. Overstating it is expensive: this path writes headers that fail validation to the
    /// bad block list, which keeps rejecting them for as long as the list holds them.
    mod fork_height {
        use std::sync::Mutex;

        use super::*;
        use crate::{
            proof_of_work::AdjustedTarget,
            validation::{HeaderChainContext, ValidatedHeader},
        };

        /// A block that extends the tip agrees with the database all the way down to its parent.
        #[tokio::test]
        async fn it_reports_the_parent_height_for_a_block_that_extends_the_tip() {
            let db = create_new_blockchain();
            let (_, main_chain) = create_main_chain(&db, &[("A->GB", 1, 120), ("B->A", 1, 120)]);
            let tip = main_chain.get("B").unwrap().clone();
            let (_, next) = create_chained_blocks(&db, &[("C->GB", 1, 120)], tip);
            let candidate = next.get("C").unwrap().clone();

            let access = db.db_write_access().unwrap();
            let (_, fork_height) = get_vm_key_for_candidate_header(&*access, candidate.header().clone()).unwrap();
            assert_eq!(candidate.height(), 3);
            assert_eq!(fork_height, 2);
        }

        /// An orphan that forks below the tip agrees with the database only up to its fork point, however long the
        /// orphan chain above it grows.
        #[tokio::test]
        async fn it_reports_the_fork_point_for_an_orphan_that_forks_below_the_tip() {
            let db = create_new_blockchain();
            let (_, main_chain) = create_main_chain(&db, &[("A->GB", 1, 120), ("B->A", 1, 120), ("C->B", 1, 120)]);
            let fork_root = main_chain.get("A").unwrap().clone();
            let (_, orphan_chain) = create_chained_blocks(&db, &[("B2->GB", 1, 120), ("C2->B2", 1, 120)], fork_root);

            let validator = MockValidator::new(true);
            let mut access = db.db_write_access().unwrap();
            for name in ["B2", "C2"] {
                let block = orphan_chain.get(name).unwrap().clone();
                insert_orphan_and_find_new_tips(&mut *access, block.to_arc_block(), &validator, &db.consensus_manager)
                    .unwrap();
            }

            let orphan_tip = orphan_chain.get("C2").unwrap().clone();
            assert_eq!(orphan_tip.height(), 3);
            let (_, fork_height) = get_vm_key_for_candidate_header(&*access, orphan_tip.header().clone()).unwrap();
            assert_eq!(fork_height, 1, "the orphan forks at A, not at its own parent");
        }

        /// When the walk ends without ever meeting the main chain, nothing about the database can be trusted, so it
        /// says so. (A candidate sitting on its own VM key band boundary is the reachable version of this: the walk
        /// has no headers to look at at all.)
        #[tokio::test]
        async fn it_trusts_nothing_when_the_walk_never_meets_the_main_chain() {
            let db = create_new_blockchain();
            create_main_chain(&db, &[("A->GB", 1, 120), ("B->A", 1, 120)]);

            let mut header = BlockHeader::new(0);
            header.height = 0;
            assert_eq!(tari_rx_vm_key_height(header.height), header.height);

            let access = db.db_write_access().unwrap();
            let (_, fork_height) = get_vm_key_for_candidate_header(&*access, header).unwrap();
            assert_eq!(fork_height, 0);
        }

        /// Captures the height a `HeaderChainContext` reports for a given Monero seed, so that a test can see what
        /// the orphan path actually handed to header validation.
        #[derive(Clone)]
        struct ContextSpy {
            seed: Vec<u8>,
            observed: Arc<Mutex<Vec<u64>>>,
        }

        impl ContextSpy {
            fn new(seed: Vec<u8>) -> Self {
                Self {
                    seed,
                    observed: Arc::new(Mutex::new(Vec::new())),
                }
            }

            fn observed(&self) -> Vec<u64> {
                self.observed.lock().unwrap().clone()
            }
        }

        impl<B: BlockchainBackend> HeaderChainLinkedValidator<B> for ContextSpy {
            fn validate(
                &self,
                db: &B,
                header: &BlockHeader,
                _: &BlockHeader,
                _: &[EpochTime],
                _: Option<AdjustedTarget>,
                chain_context: HeaderChainContext<'_>,
            ) -> Result<ValidatedHeader, ValidationError> {
                self.observed
                    .lock()
                    .unwrap()
                    .push(chain_context.monero_seed_first_seen_height(db, &self.seed)?);
                // Same as `MockValidator`: this assumes the consensus rules are the test rules.
                let difficulty_calculator = DifficultyCalculator::new(create_consensus_rules(), Default::default());
                let achieved_target = difficulty_calculator.check_achieved_and_target_difficulty(db, header)?;
                Ok(ValidatedHeader {
                    achieved_target,
                    monero_seed: None,
                })
            }
        }

        /// The regression this plumbing exists for: a seed the main chain first used *above* where an orphan forks
        /// is not that orphan's seed, so the seed age rule must not be able to see it. If it could, a legitimate
        /// fork would be rejected and - on this path - blacklisted forever.
        #[tokio::test]
        async fn an_orphan_cannot_see_a_seed_the_main_chain_first_used_above_the_fork_point() {
            let db = create_new_blockchain();
            let (_, main_chain) = create_main_chain(&db, &[("A->GB", 1, 120), ("B->A", 1, 120), ("C->B", 1, 120)]);

            // The main chain used this seed at height 2. Written straight to the index because that index, not the
            // block that filled it, is what the rule reads.
            let seed = vec![7u8; 32];
            {
                let mut txn = DbTransaction::new();
                txn.insert_monero_seed_height(seed.clone(), 2);
                let mut db_write = db.db_write_access().unwrap();
                db_write.write(txn).unwrap();
            }

            // The orphan forks at height 1, below that use of the seed.
            let fork_root = main_chain.get("A").unwrap().clone();
            let (_, orphan_chain) = create_chained_blocks(&db, &[("B2->GB", 1, 120), ("C2->B2", 1, 120)], fork_root);

            let spy = ContextSpy::new(seed.clone());
            let mut access = db.db_write_access().unwrap();
            for name in ["B2", "C2"] {
                let block = orphan_chain.get(name).unwrap().clone();
                insert_orphan_and_find_new_tips(&mut *access, block.to_arc_block(), &spy, &db.consensus_manager)
                    .unwrap();
            }

            let observed = spy.observed();
            assert!(!observed.is_empty(), "the orphan path must have validated a header");
            assert!(
                observed.iter().all(|height| *height == 0),
                "the orphan must not be able to see the main chain's seed, saw {observed:?}"
            );
            // The counterfactual: the index really does hold it, so a context that trusted the database outright
            // would have measured this orphan's headers against height 2.
            assert_eq!(access.fetch_monero_seed_first_seen_height(&seed).unwrap(), 2);
        }
    }

    mod insert_orphan_and_find_new_tips {
        use super::*;
        use crate::{
            proof_of_work::AdjustedTarget,
            validation::{HeaderChainContext, ValidatedHeader},
        };

        /// A validator that fails every header, so that a test can see what the orphan path does with the verdict.
        struct AlwaysFails {
            /// `true` for the chain dependent seed age verdict, `false` for a verdict about the header alone.
            seed_age: bool,
        }

        impl<B: BlockchainBackend> HeaderChainLinkedValidator<B> for AlwaysFails {
            fn validate(
                &self,
                _: &B,
                _: &BlockHeader,
                _: &BlockHeader,
                _: &[EpochTime],
                _: Option<AdjustedTarget>,
                _: HeaderChainContext<'_>,
            ) -> Result<ValidatedHeader, ValidationError> {
                if self.seed_age {
                    Err(ValidationError::BlockHeaderError(
                        BlockHeaderValidationError::OldSeedHash,
                    ))
                } else {
                    Err(ValidationError::BlockHeaderError(
                        BlockHeaderValidationError::InvalidNonce,
                    ))
                }
            }
        }

        fn orphan_of_genesis(db: &BlockchainDatabase<TempDatabase>) -> Arc<ChainBlock> {
            let genesis_block = db
                .fetch_block(0, true)
                .unwrap()
                .try_into_chain_block()
                .map(Arc::new)
                .unwrap();
            let (_, chain) = create_chained_blocks(db, &[("A->GB", 1u64, 120u64)], genesis_block);
            chain.get("A").unwrap().clone()
        }

        /// The seed age rule is measured against the chain a header is on, so the same header can be stale on one
        /// chain and fine on another. This path writes failed headers to the bad block list, which is consulted by
        /// `check_not_bad_block` on every later attempt, so a verdict that belongs to a chain rather than to the
        /// header must not be recorded there. `BlockHeaderSyncValidator` already behaves this way.
        #[tokio::test]
        async fn it_does_not_blacklist_a_seed_age_failure() {
            let db = create_new_blockchain();
            let block = orphan_of_genesis(&db);
            let mut access = db.db_write_access().unwrap();

            let err = insert_orphan_and_find_new_tips(
                &mut *access,
                block.to_arc_block(),
                &AlwaysFails { seed_age: true },
                &db.consensus_manager,
            )
            .unwrap_err();
            assert!(
                matches!(err, ChainStorageError::ValidationError {
                    source: ValidationError::BlockHeaderError(BlockHeaderValidationError::OldSeedHash)
                }),
                "expected the seed age failure to be returned, got {err:?}"
            );

            let (is_bad_block, reason) = access.bad_block_exists(*block.hash()).unwrap();
            assert!(!is_bad_block, "a seed age failure must not be blacklisted: {reason}");
        }

        /// The control for the test above: a verdict about the header alone is still recorded, so the assertion
        /// there is about `OldSeedHash` and not about this path having stopped blacklisting altogether.
        #[tokio::test]
        async fn it_still_blacklists_a_failure_that_is_about_the_header_alone() {
            let db = create_new_blockchain();
            let block = orphan_of_genesis(&db);
            let mut access = db.db_write_access().unwrap();

            insert_orphan_and_find_new_tips(
                &mut *access,
                block.to_arc_block(),
                &AlwaysFails { seed_age: false },
                &db.consensus_manager,
            )
            .unwrap_err();

            let (is_bad_block, _) = access.bad_block_exists(*block.hash()).unwrap();
            assert!(
                is_bad_block,
                "an invalid nonce is the header's own fault and must be blacklisted"
            );
        }

        #[tokio::test]
        async fn it_inserts_new_block_in_orphan_db_as_tip() {
            let db = create_new_blockchain();
            let validator = MockValidator::new(true);
            let genesis_block = db
                .fetch_block(0, true)
                .unwrap()
                .try_into_chain_block()
                .map(Arc::new)
                .unwrap();
            let (_, chain) = create_chained_blocks(&db, &[("A->GB", 1u64, 120u64)], genesis_block);
            let block = chain.get("A").unwrap().clone();
            let mut access = db.db_write_access().unwrap();
            insert_orphan_and_find_new_tips(&mut *access, block.to_arc_block(), &validator, &db.consensus_manager)
                .unwrap();

            let maybe_block = access.fetch_orphan_chain_tip_by_hash(block.hash()).unwrap();
            assert_eq!(maybe_block.unwrap().header(), block.header());
        }

        #[tokio::test]
        async fn it_inserts_true_orphan_chain() {
            let db = create_new_blockchain();
            let validator = MockValidator::new(true);
            let (_, main_chain) = create_main_chain(&db, &[("A->GB", 1, 120), ("B->A", 1, 120)]);

            let block_b = main_chain.get("B").unwrap().clone();
            let (_, orphan_chain) = create_chained_blocks(
                &db,
                &[("C2->GB", 1, 120), ("D2->C2", 1, 120), ("E2->D2", 1, 120)],
                block_b,
            );
            let mut access = db.db_write_access().unwrap();

            let block_d2 = orphan_chain.get("D2").unwrap().clone();
            insert_orphan_and_find_new_tips(&mut *access, block_d2.to_arc_block(), &validator, &db.consensus_manager)
                .unwrap();

            let block_e2 = orphan_chain.get("E2").unwrap().clone();
            insert_orphan_and_find_new_tips(&mut *access, block_e2.to_arc_block(), &validator, &db.consensus_manager)
                .unwrap();

            let maybe_block = access.fetch_orphan_children_of(*block_d2.hash()).unwrap();
            assert_eq!(maybe_block[0], *block_e2.to_arc_block());
        }

        #[tokio::test]
        async fn it_correctly_handles_duplicate_blocks() {
            let db = create_new_blockchain();
            let validator = MockValidator::new(true);
            let (_, main_chain) = create_main_chain(&db, &[("A->GB", 1, 120)]);

            let fork_root = main_chain.get("A").unwrap().clone();
            let (_, orphan_chain) = create_chained_blocks(&db, &[("B2->GB", 1, 120)], fork_root);
            let mut access = db.db_write_access().unwrap();

            let block = orphan_chain.get("B2").unwrap().clone();
            insert_orphan_and_find_new_tips(&mut *access, block.to_arc_block(), &validator, &db.consensus_manager)
                .unwrap();
            let fork_tip = access.fetch_orphan_chain_tip_by_hash(block.hash()).unwrap().unwrap();
            assert_eq!(fork_tip, block.to_chain_header());
            assert_eq!(fork_tip.accumulated_data().total_accumulated_difficulty, 3.into());
            let strongest_tips = access.fetch_strongest_orphan_chain_tips().unwrap().len();
            assert_eq!(strongest_tips, 1);

            // Insert again (block was received more than once), no new tips
            insert_orphan_and_find_new_tips(&mut *access, block.to_arc_block(), &validator, &db.consensus_manager)
                .unwrap();
            let strongest_tips = access.fetch_strongest_orphan_chain_tips().unwrap().len();
            assert_eq!(strongest_tips, 1);
        }

        #[ignore]
        #[tokio::test]
        async fn it_correctly_detects_strongest_orphan_tips() {
            let db = create_new_blockchain();
            let validator = MockValidator::new(true);
            let (_, main_chain) = create_main_chain(&db, &[
                ("A->GB", 1, 120),
                ("B->A", 2, 120),
                ("C->B", 1, 120),
                ("D->C", 1, 120),
                ("E->D", 1, 120),
                ("F->E", 1, 120),
                ("G->F", 1, 120),
            ]);

            // Fork 1 (with 3 blocks)
            let fork_root_1 = main_chain.get("A").unwrap().clone();

            let (_, orphan_chain_1) = create_chained_blocks(
                &db,
                &[("B2->GB", 1, 120), ("C2->B2", 1, 120), ("D2->C2", 1, 120)],
                fork_root_1,
            );

            // Fork 2 (with 1 block)
            let fork_root_2 = main_chain.get("GB").unwrap().clone();
            let (_, orphan_chain_2) = create_chained_blocks(&db, &[("B3->GB", 1, 120)], fork_root_2);

            // Fork 3 (with 1 block)
            let fork_root_3 = main_chain.get("B").unwrap().clone();
            let (_, orphan_chain_3) = create_chained_blocks(&db, &[("B4->GB", 1, 120)], fork_root_3);

            // Add blocks to db
            let mut access = db.db_write_access().unwrap();

            // Fork 1 (add 3 blocks)
            let block = orphan_chain_1.get("B2").unwrap().clone();
            insert_orphan_and_find_new_tips(&mut *access, block.to_arc_block(), &validator, &db.consensus_manager)
                .unwrap();
            let block = orphan_chain_1.get("C2").unwrap().clone();
            insert_orphan_and_find_new_tips(&mut *access, block.to_arc_block(), &validator, &db.consensus_manager)
                .unwrap();
            let block = orphan_chain_1.get("D2").unwrap().clone();
            insert_orphan_and_find_new_tips(&mut *access, block.to_arc_block(), &validator, &db.consensus_manager)
                .unwrap();
            let fork_tip_1 = access.fetch_orphan_chain_tip_by_hash(block.hash()).unwrap().unwrap();

            assert_eq!(fork_tip_1, block.to_chain_header());
            assert_eq!(fork_tip_1.accumulated_data().total_accumulated_difficulty, 5.into());

            // Fork 2 (add 1 block)
            let block = orphan_chain_2.get("B3").unwrap().clone();
            insert_orphan_and_find_new_tips(&mut *access, block.to_arc_block(), &validator, &db.consensus_manager)
                .unwrap();
            let fork_tip_2 = access.fetch_orphan_chain_tip_by_hash(block.hash()).unwrap().unwrap();

            assert_eq!(fork_tip_2, block.to_chain_header());
            assert_eq!(fork_tip_2.accumulated_data().total_accumulated_difficulty, 2.into());

            // Fork 3 (add 1 block)
            let block = orphan_chain_3.get("B4").unwrap().clone();
            insert_orphan_and_find_new_tips(&mut *access, block.to_arc_block(), &validator, &db.consensus_manager)
                .unwrap();
            let fork_tip_3 = access.fetch_orphan_chain_tip_by_hash(block.hash()).unwrap().unwrap();

            assert_eq!(fork_tip_3, block.to_chain_header());
            assert_eq!(fork_tip_3.accumulated_data().total_accumulated_difficulty, 5.into());

            assert_ne!(fork_tip_1, fork_tip_2);
            assert_ne!(fork_tip_1, fork_tip_3);

            // Test get strongest chain tips
            let strongest_tips = access.fetch_strongest_orphan_chain_tips().unwrap();
            assert_eq!(strongest_tips.len(), 2);
            let mut found_tip_1 = false;
            let mut found_tip_3 = false;
            for tip in &strongest_tips {
                if tip == &fork_tip_1 {
                    found_tip_1 = true;
                }
                if tip == &fork_tip_3 {
                    found_tip_3 = true;
                }
            }
            assert!(found_tip_1 && found_tip_3);

            // Insert again (block was received more than once), no new tips
            insert_orphan_and_find_new_tips(&mut *access, block.to_arc_block(), &validator, &db.consensus_manager)
                .unwrap();
            let strongest_tips = access.fetch_strongest_orphan_chain_tips().unwrap();
            assert_eq!(strongest_tips.len(), 2);
        }
    }

    mod handle_possible_reorg {
        use super::*;

        #[ignore]
        #[tokio::test]
        async fn it_links_many_orphan_branches_to_main_chain() {
            let test = TestHarness::setup();
            let (_, main_chain) =
                create_main_chain(&test.db, block_specs!(["1a->GB"], ["2a->1a"], ["3a->2a"], ["4a->3a"]));
            let genesis = main_chain.get("GB").unwrap().clone();

            let fork_root = main_chain.get("1a").unwrap().clone();
            let (_, orphan_chain_b) = create_chained_blocks(
                &test.db,
                block_specs!(["2b->GB"], ["3b->2b"], ["4b->3b"], ["5b->4b"], ["6b->5b"]),
                fork_root,
            );

            // Add orphans out of height order
            for name in ["5b", "3b", "4b", "6b"] {
                let block = orphan_chain_b.get(name).unwrap();
                let result = test.handle_possible_reorg(block.to_arc_block()).unwrap();
                assert!(result.is_orphaned());
            }

            // Add chain c orphans branching from chain b
            let fork_root = orphan_chain_b.get("3b").unwrap().clone();
            let (_, orphan_chain_c) = create_chained_blocks(
                &test.db,
                block_specs!(["4c->GB"], ["5c->4c"], ["6c->5c"], ["7c->6c"]),
                fork_root,
            );

            for name in ["7c", "5c", "6c", "4c"] {
                let block = orphan_chain_c.get(name).unwrap();
                let result = test.handle_possible_reorg(block.to_arc_block()).unwrap();
                assert!(result.is_orphaned());
            }

            let fork_root = orphan_chain_c.get("6c").unwrap().clone();
            let (_, orphan_chain_d) = create_chained_blocks(
                &test.db,
                block_specs!(["7d->GB", difficulty: Difficulty::from_u64(10).unwrap()]),
                fork_root,
            );

            let block = orphan_chain_d.get("7d").unwrap();
            let result = test.handle_possible_reorg(block.to_arc_block()).unwrap();
            assert!(result.is_orphaned());

            // REORG
            // Now, connect the chain and check that 7d branch is the tip
            let block = orphan_chain_b.get("2b").unwrap();
            let result = test.handle_possible_reorg(block.to_arc_block()).unwrap();
            result.assert_reorg(6, 3);

            {
                // Check 2b was added
                let access = test.db_write_access();
                let block = orphan_chain_b.get("2b").unwrap().clone();
                assert!(access.contains(&DbKey::HeaderHash(*block.hash())).unwrap());

                // Check 7d is the tip
                let block = orphan_chain_d.get("7d").unwrap().clone();
                let tip = access.fetch_tip_header().unwrap();
                assert_eq!(tip.hash(), block.hash());
                let metadata = access.fetch_chain_metadata().unwrap();
                assert_eq!(metadata.best_block_hash(), block.hash());
                assert_eq!(metadata.best_block_height(), block.height());
                assert!(access.contains(&DbKey::HeaderHash(*block.hash())).unwrap());

                let mut all_blocks = main_chain
                    .into_iter()
                    .chain(orphan_chain_b)
                    .chain(orphan_chain_c)
                    .chain(orphan_chain_d)
                    .collect::<HashMap<_, _>>();
                all_blocks.insert("GB".to_string(), genesis);
                // Check the chain heights
                let expected_chain = ["GB", "1a", "2b", "3b", "4c", "5c", "6c", "7d"];
                for (height, name) in expected_chain.iter().enumerate() {
                    let expected_block = all_blocks.get(*name).unwrap();
                    unpack_enum!(
                        DbValue::HeaderHeight(found_block) =
                            access.fetch(&DbKey::HeaderHeight(height as u64)).unwrap().unwrap()
                    );
                    assert_eq!(*found_block, *expected_block.header());
                }
            }
        }

        #[ignore]
        #[tokio::test]
        async fn it_links_many_orphan_branches_to_main_chain_with_greater_reorg_than_median_timestamp_window() {
            let test = TestHarness::setup();
            // This test assumes a MTC of 11
            assert_eq!(test.consensus.consensus_constants(0).median_timestamp_count(), 11);
            let (_, main_chain) = create_main_chain(
                &test.db,
                block_specs!(
                    ["1a->GB"],
                    ["2a->1a"],
                    ["3a->2a"],
                    ["4a->3a"],
                    ["5a->4a"],
                    ["6a->5a"],
                    ["7a->6a"],
                    ["8a->7a"],
                    ["9a->8a"],
                    ["10a->9a"],
                    ["11a->10a"],
                    ["12a->11a"],
                    ["13a->12a"],
                ),
            );
            let genesis = main_chain.get("GB").unwrap().clone();
            let fork_root = main_chain.get("1a").unwrap().clone();
            let (_, orphan_chain_b) = create_chained_blocks(
                &test.db,
                block_specs!(
                    ["2b->GB"],
                    ["3b->2b"],
                    ["4b->3b"],
                    ["5b->4b"],
                    ["6b->5b"],
                    ["7b->6b"],
                    ["8b->7b"],
                    ["9b->8b"],
                    ["10b->9b"],
                    ["11b->10b"],
                    ["12b->11b", difficulty: Difficulty::from_u64(5).unwrap()]
                ),
                fork_root,
            );

            // Add orphans out of height order
            let mut unordered = vec!["3b", "4b", "5b", "6b", "7b", "8b", "9b", "10b", "11b", "12b"];
            unordered.shuffle(&mut rand::rng());
            for name in unordered {
                let block = orphan_chain_b.get(name).unwrap().clone();
                let result = test.handle_possible_reorg(block.to_arc_block()).unwrap();
                assert!(result.is_orphaned());
            }

            // Now, connect the chain and check that 12b branch is the tip
            let block = orphan_chain_b.get("2b").unwrap().clone();
            let result = test.handle_possible_reorg(block.to_arc_block()).unwrap();
            result.assert_reorg(11, 12);

            {
                // Check 2b was added
                let access = test.db_write_access();
                let block = orphan_chain_b.get("2b").unwrap().clone();
                assert!(access.contains(&DbKey::HeaderHash(*block.hash())).unwrap());

                // Check 12b is the tip
                let block = orphan_chain_b.get("12b").unwrap().clone();
                let tip = access.fetch_tip_header().unwrap();
                assert_eq!(tip.hash(), block.hash());
                let metadata = access.fetch_chain_metadata().unwrap();
                assert_eq!(metadata.best_block_hash(), block.hash());
                assert_eq!(metadata.best_block_height(), block.height());
                assert!(access.contains(&DbKey::HeaderHash(*block.hash())).unwrap());

                let mut all_blocks = main_chain.into_iter().chain(orphan_chain_b).collect::<HashMap<_, _>>();
                all_blocks.insert("GB".to_string(), genesis);
                // Check the chain heights
                let expected_chain = [
                    "GB", "1a", "2b", "3b", "4b", "5b", "6b", "7b", "8b", "9b", "10b", "11b", "12b",
                ];
                for (height, name) in expected_chain.iter().enumerate() {
                    let expected_block = all_blocks.get(*name).unwrap();
                    unpack_enum!(
                        DbValue::HeaderHeight(found_block) =
                            access.fetch(&DbKey::HeaderHeight(height as u64)).unwrap().unwrap()
                    );
                    assert_eq!(*found_block, *expected_block.header());
                }
            }
        }

        #[tokio::test]
        async fn it_errors_if_reorging_to_an_invalid_height() {
            let test = TestHarness::setup();
            let (_, main_chain) =
                create_main_chain(&test.db, block_specs!(["1a->GB"], ["2a->1a"], ["3a->2a"], ["4a->3a"]));

            let fork_root = main_chain.get("1a").unwrap().clone();
            let (_, orphan_chain_b) = create_chained_blocks(
                &test.db,
                block_specs!(["2b->GB", height: 10, difficulty: Difficulty::from_u64(10).unwrap()]),
                fork_root,
            );

            let block = orphan_chain_b.get("2b").unwrap().clone();
            let err = test.handle_possible_reorg(block.to_arc_block()).unwrap_err();
            unpack_enum!(ChainStorageError::ValueNotFound { .. } = err);
        }

        #[tokio::test]
        async fn it_allows_orphan_blocks_with_any_height() {
            let test = TestHarness::setup();
            let (_, main_chain) = create_main_chain(
                &test.db,
                block_specs!(["1a->GB", difficulty: Difficulty::from_u64(2).unwrap()]),
            );

            let fork_root = main_chain.get("GB").unwrap().clone();
            let (_, orphan_chain_b) = create_orphan_chain(&test.db, block_specs!(["1b->GB", height: 10]), fork_root);

            let block = orphan_chain_b.get("1b").unwrap().clone();
            test.handle_possible_reorg(block.to_arc_block())
                .unwrap()
                .assert_orphaned();
        }
    }

    #[tokio::test]
    async fn test_handle_possible_reorg_case1() {
        // Normal chain
        let (result, _blocks) = test_case_handle_possible_reorg(&[("A->GB", 1, 120), ("B->A", 1, 120)]).unwrap();
        result[0].assert_added();
        result[1].assert_added();
    }

    #[ignore]
    #[tokio::test]
    async fn test_handle_possible_reorg_case2() {
        let (result, blocks) =
            test_case_handle_possible_reorg(&[("A->GB", 1, 120), ("B->A", 1, 120), ("A2->GB", 3, 120)]).unwrap();
        result[0].assert_added();
        result[1].assert_added();
        result[2].assert_reorg(1, 2);
        assert_added_hashes_eq(&result[2], vec!["A2"], &blocks);
    }

    #[ignore]
    #[tokio::test]
    async fn test_handle_possible_reorg_case3() {
        // Switch to new chain and then reorg back
        let (result, blocks) =
            test_case_handle_possible_reorg(&[("A->GB", 1, 120), ("A2->GB", 2, 120), ("B->A", 2, 120)]).unwrap();
        result[0].assert_added();
        result[1].assert_reorg(1, 1);
        result[2].assert_reorg(2, 1);
        assert_added_hashes_eq(&result[2], vec!["A", "B"], &blocks);
    }

    #[ignore]
    #[tokio::test]
    async fn test_handle_possible_reorg_case4() {
        let (result, blocks) = test_case_handle_possible_reorg(&[
            ("A->GB", 1, 120),
            ("A2->GB", 2, 120),
            ("B->A", 2, 120),
            ("A3->GB", 4, 120),
            ("C->B", 2, 120),
        ])
        .unwrap();
        result[0].assert_added();
        result[1].assert_reorg(1, 1);
        result[2].assert_reorg(2, 1);
        result[3].assert_reorg(1, 2);
        result[4].assert_reorg(3, 1);

        assert_added_hashes_eq(&result[4], vec!["A", "B", "C"], &blocks);
    }

    #[ignore]
    #[tokio::test]
    async fn test_handle_possible_reorg_case5() {
        let (result, blocks) = test_case_handle_possible_reorg(&[
            ("A->GB", 1, 120),
            ("B->A", 1, 120),
            ("A2->GB", 3, 120),
            ("C->B", 1, 120),
            ("D->C", 2, 120),
            ("B2->A", 5, 120),
            ("D2->C", 6, 120),
            ("D3->C", 7, 120),
            ("D4->C", 8, 120),
        ])
        .unwrap();
        result[0].assert_added();
        result[1].assert_added();
        result[2].assert_reorg(1, 2);
        result[3].assert_orphaned();
        result[4].assert_reorg(4, 1);
        result[5].assert_reorg(1, 3);
        result[6].assert_reorg(3, 1);
        result[7].assert_reorg(1, 1);
        result[8].assert_reorg(1, 1);

        assert_added_hashes_eq(&result[5], vec!["B2"], &blocks);
        assert_difficulty_eq(&result[5], vec![7.into()]);

        assert_added_hashes_eq(&result[6], vec!["B", "C", "D2"], &blocks);
        assert_difficulty_eq(&result[6], vec![3.into(), 4.into(), 10.into()]);

        assert_added_hashes_eq(&result[7], vec!["D3"], &blocks);
        assert_difficulty_eq(&result[7], vec![11.into()]);

        assert_added_hashes_eq(&result[8], vec!["D4"], &blocks);
        assert_difficulty_eq(&result[8], vec![12.into()]);
    }

    #[tokio::test]
    // #[ignore = "This test originally created an SMT in memory and not using a database, that is not possible with the
    // \ JMT"]
    async fn test_handle_possible_reorg_case6_orphan_chain_link() {
        let db = create_new_blockchain();
        let (_, mainchain) = create_main_chain(&db, &[
            ("A->GB", 1, 120),
            ("B->A", 1, 120),
            ("C->B", 1, 120),
            ("D->C", 1, 120),
        ]);

        let mock_validator = MockValidator::new(true);
        let chain_strength_comparer = strongest_chain().by_sha3x_difficulty().build();

        let fork_block = mainchain.get("B").unwrap().clone();
        let (_, reorg_chain) = create_chained_blocks(
            &db,
            &[("C2->GB", 1, 120), ("D2->C2", 1, 120), ("E2->D2", 1, 120)],
            fork_block,
        );

        // Add true orphans
        let mut access = db.db_write_access().unwrap();
        let result = handle_possible_reorg(
            &mut *access,
            &Default::default(),
            &db.consensus_manager,
            &mock_validator,
            &mock_validator,
            &*chain_strength_comparer,
            reorg_chain.get("E2").unwrap().to_arc_block(),
        )
        .unwrap();
        result.assert_orphaned();

        // Test adding a duplicate orphan
        let result = handle_possible_reorg(
            &mut *access,
            &Default::default(),
            &db.consensus_manager,
            &mock_validator,
            &mock_validator,
            &*chain_strength_comparer,
            reorg_chain.get("E2").unwrap().to_arc_block(),
        )
        .unwrap();
        result.assert_orphaned();

        let result = handle_possible_reorg(
            &mut *access,
            &Default::default(),
            &db.consensus_manager,
            &mock_validator,
            &mock_validator,
            &*chain_strength_comparer,
            reorg_chain.get("D2").unwrap().to_arc_block(),
        )
        .unwrap();
        result.assert_orphaned();

        let tip = access.fetch_last_header().unwrap();
        assert_eq!(&tip, mainchain.get("D").unwrap().header());

        let result = handle_possible_reorg(
            &mut *access,
            &Default::default(),
            &db.consensus_manager,
            &mock_validator,
            &mock_validator,
            &*chain_strength_comparer,
            reorg_chain.get("C2").unwrap().to_arc_block(),
        )
        .unwrap();
        result.assert_reorg(3, 2);

        let tip = access.fetch_last_header().unwrap();
        assert_eq!(&tip, reorg_chain.get("E2").unwrap().header());
        check_whole_chain(&mut access);
    }

    #[tokio::test]
    async fn test_handle_possible_reorg_case7_fail_reorg() {
        let db = create_new_blockchain();
        let (_, mainchain) = create_main_chain(&db, &[
            ("A->GB", 1, 120),
            ("B->A", 1, 120),
            ("C->B", 1, 120),
            ("D->C", 1, 120),
        ]);

        let mock_validator = MockValidator::new(true);
        let chain_strength_comparer = strongest_chain().by_sha3x_difficulty().build();
        // we only need a smt, this one will not be technically correct, but due to the use of mockvalidators(true),
        // they will pass all mr tests
        let fork_block = mainchain.get("C").unwrap().clone();
        let (_, reorg_chain) = create_chained_blocks(&db, &[("D2->GB", 1, 120), ("E2->D2", 2, 120)], fork_block);

        // Add true orphans
        let mut access = db.db_write_access().unwrap();
        let result = handle_possible_reorg(
            &mut *access,
            &Default::default(),
            &db.consensus_manager,
            &mock_validator,
            &mock_validator,
            &*chain_strength_comparer,
            reorg_chain.get("E2").unwrap().to_arc_block(),
        )
        .unwrap();
        result.assert_orphaned();

        let _error = handle_possible_reorg(
            &mut *access,
            &Default::default(),
            &db.consensus_manager,
            &MockValidator::new(false),
            &mock_validator,
            &*chain_strength_comparer,
            reorg_chain.get("D2").unwrap().to_arc_block(),
        )
        .unwrap_err();

        // Restored chain
        let tip = access.fetch_last_header().unwrap();
        assert_eq!(&tip, mainchain.get("D").unwrap().header());

        check_whole_chain(&mut access);
    }

    // ---------------------------------------------------------------------------------------------------------
    // GHSA-3qmx-q9pv-f3m4 deep reorg anchor
    // ---------------------------------------------------------------------------------------------------------

    /// The rule itself, in isolation. Everything below exercises it through the real reorg path; this pins the
    /// properties it has to have, including the ones that are about the rule *not* firing.
    #[test]
    fn the_deep_reorg_anchor_only_fires_when_it_should() {
        const ACTIVATION: u64 = 10_000;
        const WINDOW: u64 = GHSA_DEEP_REORG_CONFIRMATION_WINDOW;

        // Fires: our tip is a full confirmation window past the fork and the fork chain would add blocks from
        // below it. The first case is the exact engagement height, the second a genuinely deep fork.
        assert!(reorg_reintroduces_pre_ghsa_blocks(
            ACTIVATION + WINDOW,
            ACTIVATION - 1,
            ACTIVATION
        ));
        assert!(reorg_reintroduces_pre_ghsa_blocks(ACTIVATION + 5_000, 1, ACTIVATION));

        // Does not fire inside the confirmation window, even though the fork reaches below the activation
        // height. This is the case that partitions honest nodes if the rule is written as an absolute boundary:
        // at tip `activation + 1` a node one block ahead of its neighbour would otherwise permanently refuse a
        // fork the neighbour accepts, and the refusal costs a resync.
        assert!(!reorg_reintroduces_pre_ghsa_blocks(
            ACTIVATION + 1,
            ACTIVATION - 1,
            ACTIVATION
        ));
        assert!(!reorg_reintroduces_pre_ghsa_blocks(
            ACTIVATION + WINDOW - 1,
            ACTIVATION - 1,
            ACTIVATION
        ));
        // ...including the three block reorg across the fork that started all this: tip at the activation
        // height, fork point two below it.
        assert!(!reorg_reintroduces_pre_ghsa_blocks(
            ACTIVATION,
            ACTIVATION - 1,
            ACTIVATION
        ));

        // Does not fire below the fork, which is what keeps initial sync - and ordinary pre-fork operation -
        // working: the node reorgs freely inside the pre-fork range because that range's rules are the pre-fork
        // rules for everyone.
        assert!(!reorg_reintroduces_pre_ghsa_blocks(ACTIVATION - 1, 1, ACTIVATION));
        assert!(!reorg_reintroduces_pre_ghsa_blocks(0, 0, ACTIVATION));

        // Does not fire on a reorg that only adds blocks from the activation height upwards, however far past
        // the window we are: those blocks are subject to the new rules and had to carry real work.
        assert!(!reorg_reintroduces_pre_ghsa_blocks(
            ACTIVATION + WINDOW,
            ACTIVATION,
            ACTIVATION
        ));
        assert!(!reorg_reintroduces_pre_ghsa_blocks(
            ACTIVATION + 100_000,
            ACTIVATION,
            ACTIVATION
        ));

        // Inert where the advisory's fork is unscheduled: the engagement height saturates at `u64::MAX`, which
        // no reachable tip reaches.
        for tip in [0, 1_000, u64::MAX - 1] {
            assert!(!reorg_reintroduces_pre_ghsa_blocks(
                tip,
                0,
                UNSCHEDULED_ACTIVATION_HEIGHT
            ));
        }
        // ...and inert on a network like LocalNet that has the rules from height 0, because no block is below it.
        for tip in [0, 1_000, u64::MAX] {
            assert!(!reorg_reintroduces_pre_ghsa_blocks(tip, 0, 0));
        }
    }

    /// The property the confirmation window exists for, stated directly: *no* reorg shallower than the window is
    /// ever refused, wherever the node's tip happens to be. That is what stops two honest nodes at different
    /// heights from deciding an ordinary reorg differently - the only forks they can disagree about are ones far
    /// deeper than honest chain activity produces.
    ///
    /// The depth of a reorg is `tip - fork point`, and the lowest block it adds sits at `fork point + 1`
    /// (`HeaderFullValidator::check_height` makes that an identity, and header sync computes it as
    /// `split_header.height + 1`), so depth `d` means `lowest_new = tip - d + 1`.
    #[test]
    fn the_deep_reorg_anchor_never_refuses_a_reorg_shallower_than_the_window() {
        const ACTIVATION: u64 = 10_000;
        const WINDOW: u64 = GHSA_DEEP_REORG_CONFIRMATION_WINDOW;

        for tip in [
            0,
            1,
            ACTIVATION - WINDOW,
            ACTIVATION - 2,
            ACTIVATION - 1,
            ACTIVATION,
            ACTIVATION + 1,
            ACTIVATION + WINDOW - 1,
            ACTIVATION + WINDOW,
            ACTIVATION + WINDOW + 1,
            ACTIVATION + 10 * WINDOW,
        ] {
            for depth in 1..=WINDOW {
                let lowest_new = tip.saturating_sub(depth) + 1;
                assert!(
                    !reorg_reintroduces_pre_ghsa_blocks(tip, lowest_new, ACTIVATION),
                    "a {depth} block reorg was refused at tip {tip} (lowest new block {lowest_new})"
                );
            }
        }
    }

    /// LocalNet constants with the advisory's rules scheduled at `activation` instead of on from height 0, so the
    /// anchor has a boundary to guard. Everything else is the stock LocalNet rule set, including the default
    /// chain strength comparer - accumulated difficulty first, height only as a tiebreak - because the whole
    /// point of the fork these tests build is that it is *shorter and heavier* than the chain it replaces, and a
    /// height ordering would never pick it.
    fn rules_with_ghsa_activation_at(activation: u64) -> BaseNodeConsensusManager {
        BaseNodeConsensusManager::builder(Network::LocalNet)
            .add_consensus_constants(
                ConsensusConstantsBuilder::new(Network::LocalNet)
                    .with_effective_from_height(0)
                    .with_derive_monero_coinbase_hasher(false)
                    .build(),
            )
            .add_consensus_constants(
                ConsensusConstantsBuilder::new(Network::LocalNet)
                    .with_effective_from_height(activation)
                    .with_derive_monero_coinbase_hasher(true)
                    .build(),
            )
            .build()
            .unwrap()
    }

    /// The main chain the deep reorg fixture builds has to be longer than the confirmation window before the
    /// anchor can engage at all, which is the only reason it is this long. Four blocks past the window leaves
    /// room for an activation height that is both above the fork chain's lowest block and a full window below
    /// the tip.
    const DEEP_REORG_FIXTURE_CHAIN_LEN: u64 = GHSA_DEEP_REORG_CONFIRMATION_WINDOW + 4;
    /// Every main chain block in the fixture is mined at difficulty 1 - the default `MockValidator` credits the
    /// LocalNet LWMA target, which never leaves its minimum on a chain of evenly spaced blocks - so the fork has
    /// to out-accumulate one unit per main chain block. The fork's blocks carry real proof of work instead, and
    /// `apply_deep_reorg_fork` credits them with it. `mine_to_difficulty` searches for a hash of *exactly* the
    /// requested difficulty, which costs about `difficulty^2` hashes and gives up at 20 000, so this stays well
    /// clear of that ceiling and buys the accumulated difficulty with more blocks instead.
    const DEEP_REORG_FIXTURE_FORK_DIFFICULTY: u64 = 30;
    /// Enough blocks at that difficulty to beat the whole main chain with room to spare.
    const DEEP_REORG_FIXTURE_FORK_LEN: u64 = DEEP_REORG_FIXTURE_CHAIN_LEN / DEEP_REORG_FIXTURE_FORK_DIFFICULTY + 2;

    fn leaked(name: String) -> &'static str {
        Box::leak(name.into_boxed_str())
    }

    fn fixture_spec(name: &'static str, difficulty: u64, block_time: u64) -> crate::test_helpers::BlockSpec {
        crate::test_helpers::BlockSpec::builder()
            .with_name(name)
            .with_block_time(block_time)
            .with_difficulty(Difficulty::from_u64(difficulty).unwrap())
            .finish()
    }

    /// `M1 -> M2 -> ... -> M{DEEP_REORG_FIXTURE_CHAIN_LEN}`, with a much heavier fork rooted at `M2` that
    /// *branches* at its tip: `... -> F{n} -> {E2, D3}`. The branch is not decoration.
    /// `find_orphan_descendant_tips_of` registers a chain tip for every descendant, so a refused fork can have
    /// several tips while `get_orphan_link_main_chain` only ever returns the one linking chain - and an attacker
    /// announcing two near-equal tips on the same refused fork costs nothing under the premise this rule exists
    /// for.
    ///
    /// Returns the harness pieces plus both chains.
    #[allow(clippy::type_complexity)]
    fn deep_reorg_fixture(
        activation: u64,
    ) -> (
        BlockchainDatabase<TempDatabase>,
        HashMap<String, Arc<ChainBlock>>,
        HashMap<String, Arc<ChainBlock>>,
    ) {
        let rules = rules_with_ghsa_activation_at(activation);
        assert_eq!(rules.derived_monero_coinbase_activation_height(), activation);
        let db = crate::test_helpers::blockchain::create_custom_blockchain(rules);

        // Revealed value coinbases: the chain has to be hundreds of blocks long and every validator in this
        // harness is a mock, so paying for a bullet proof per block buys nothing.
        let main_chain_specs = (1..=DEEP_REORG_FIXTURE_CHAIN_LEN)
            .map(|height| {
                let name = if height == 1 {
                    leaked("M1->GB".to_string())
                } else {
                    leaked(format!("M{height}->M{}", height - 1))
                };
                fixture_spec(name, 1, 120)
            })
            .collect::<Vec<_>>();
        let (_, mainchain) = crate::test_helpers::blockchain::create_main_chain_with_range_proof_type(
            &db,
            main_chain_specs,
            Some(RangeProofType::RevealedValue),
        );
        // Initial sync across the boundary: every block above was added one at a time through the very same
        // `handle_possible_reorg` path, with the tip crossing `activation` on the way. The anchor must not have
        // touched any of them.
        assert_eq!(
            db.get_chain_metadata().unwrap().best_block_height(),
            DEEP_REORG_FIXTURE_CHAIN_LEN,
            "syncing through the activation height must be unaffected"
        );

        // The fork's lowest block is at height 3, two below the fork point's successor... and the specs are
        // emitted in non-decreasing height order so that the fixture's shared Merkle tree is written in order.
        // `D3` shares its height with `E2` and gets a different block time so that it cannot collide with it.
        let last_linear_height = DEEP_REORG_FIXTURE_FORK_LEN + 1;
        let mut fork_specs = (0..DEEP_REORG_FIXTURE_FORK_LEN - 1)
            .map(|index| {
                let height = index + 3;
                let name = match index {
                    0 => leaked("C2->GB".to_string()),
                    1 => leaked(format!("F{height}->C2")),
                    _ => leaked(format!("F{height}->F{}", height - 1)),
                };
                fixture_spec(name, DEEP_REORG_FIXTURE_FORK_DIFFICULTY, 120)
            })
            .collect::<Vec<_>>();
        fork_specs.push(fixture_spec(
            leaked(format!("D3->F{last_linear_height}")),
            DEEP_REORG_FIXTURE_FORK_DIFFICULTY - 1,
            121,
        ));
        fork_specs.push(fixture_spec(
            leaked(format!("E2->F{last_linear_height}")),
            DEEP_REORG_FIXTURE_FORK_DIFFICULTY,
            120,
        ));

        let fork_block = mainchain.get("M2").unwrap().clone();
        let (_, reorg_chain) = crate::test_helpers::blockchain::create_chained_blocks_with_range_proof_type(
            &db,
            fork_specs,
            fork_block,
            Some(RangeProofType::RevealedValue),
        );
        assert_eq!(reorg_chain.get("C2").unwrap().height(), 3);
        assert_eq!(
            reorg_chain.get("E2").unwrap().height(),
            reorg_chain.get("D3").unwrap().height()
        );
        assert_ne!(
            reorg_chain.get("D3").unwrap().hash(),
            reorg_chain.get("E2").unwrap().hash()
        );
        (db, mainchain, reorg_chain)
    }

    /// The names of the fork blocks, tip first: `E2`, then its sibling `D3`, then the linear chain back down to
    /// `C2`.
    fn deep_reorg_fork_names_tip_first() -> Vec<String> {
        let mut names = vec!["E2".to_string(), "D3".to_string()];
        for index in (0..DEEP_REORG_FIXTURE_FORK_LEN - 1).rev() {
            let height = index + 3;
            names.push(if index == 0 {
                "C2".to_string()
            } else {
                format!("F{height}")
            });
        }
        names
    }

    /// A header validator that accepts every header and credits it with the proof of work the header actually
    /// carries.
    ///
    /// The default `MockValidator` credits the LWMA *target* difficulty computed from the LocalNet rules, which
    /// sits at its minimum of 1 for every evenly spaced block in this harness - so with it a fork can only win
    /// by being longer than the chain it replaces, and the fork here has to reach hundreds of blocks below a tip
    /// it is only a few blocks taller than. This one reports what `mine_to_difficulty` actually mined, so a
    /// short heavy fork is expressible. It is not more permissive than the mock it replaces: it accepts exactly
    /// the same headers.
    struct PowCreditingHeaderValidator;

    impl<B: BlockchainBackend> HeaderChainLinkedValidator<B> for PowCreditingHeaderValidator {
        fn validate(
            &self,
            _db: &B,
            header: &BlockHeader,
            _prev_header: &BlockHeader,
            _prev_timestamps: &[EpochTime],
            _target_difficulty: Option<AdjustedTarget>,
            _chain_context: HeaderChainContext<'_>,
        ) -> Result<ValidatedHeader, ValidationError> {
            let achieved = sha3x_difficulty(header)?;
            Ok(ValidatedHeader {
                achieved_target: AchievedTargetDifficulty::try_construct(
                    PowAlgorithm::Sha3x,
                    achieved,
                    achieved,
                    achieved,
                )
                .expect("achieved == target"),
                // This harness mines Sha3x only, so there is never a Monero seed to report.
                monero_seed: None,
            })
        }
    }

    /// Feeds the fork in tip-first (so the intermediate blocks are plain orphans) and returns the result of the
    /// call that links it to the main chain - the one that would perform the swap.
    fn apply_deep_reorg_fork(
        db: &BlockchainDatabase<TempDatabase>,
        reorg_chain: &HashMap<String, Arc<ChainBlock>>,
    ) -> BlockAddResult {
        let mock_validator = MockValidator::new(true);
        let header_validator = PowCreditingHeaderValidator;
        let chain_strength_comparer = strongest_chain().by_sha3x_difficulty().build();
        let mut access = db.db_write_access().unwrap();
        let mut names = deep_reorg_fork_names_tip_first();
        let linking_block = names.pop().expect("the fork is not empty");
        assert_eq!(linking_block, "C2");
        for name in names {
            handle_possible_reorg(
                &mut *access,
                &Default::default(),
                &db.consensus_manager,
                &mock_validator,
                &header_validator,
                &*chain_strength_comparer,
                reorg_chain.get(&name).unwrap().to_arc_block(),
            )
            .unwrap()
            .assert_orphaned();
        }
        handle_possible_reorg(
            &mut *access,
            &Default::default(),
            &db.consensus_manager,
            &mock_validator,
            &header_validator,
            &*chain_strength_comparer,
            reorg_chain.get(&linking_block).unwrap().to_arc_block(),
        )
        .unwrap()
    }

    /// The activation height that makes the fixture's tip land exactly on the anchor's engagement height: the
    /// fork's lowest block is at 3, so an activation height of 4 is above it, and the tip is
    /// `4 + GHSA_DEEP_REORG_CONFIRMATION_WINDOW`.
    const DEEP_REORG_FIXTURE_ENGAGED_ACTIVATION: u64 = 4;

    /// The attack this exists for: our tip is a full confirmation window past the activation height, and a
    /// heavier fork wants to replace the chain from *below* it, where the forgeable pre-fork verifiers are still
    /// the rules. Refused, and the fork is dropped rather than left to win the tip selection again on the next
    /// block.
    #[tokio::test]
    async fn deep_reorg_anchor_refuses_a_fork_rooted_below_the_activation_height() {
        let (db, mainchain, reorg_chain) = deep_reorg_fixture(DEEP_REORG_FIXTURE_ENGAGED_ACTIVATION);
        assert_eq!(
            db.get_chain_metadata().unwrap().best_block_height(),
            DEEP_REORG_FIXTURE_ENGAGED_ACTIVATION + GHSA_DEEP_REORG_CONFIRMATION_WINDOW,
            "the fixture must put the tip exactly on the anchor's engagement height"
        );

        let result = apply_deep_reorg_fork(&db, &reorg_chain);
        result.assert_orphaned();

        let access = db.db_write_access().unwrap();
        // The chain did not move.
        let tip = access.fetch_last_header().unwrap();
        assert_eq!(
            &tip,
            mainchain
                .get(&format!("M{DEEP_REORG_FIXTURE_CHAIN_LEN}"))
                .unwrap()
                .header()
        );
        // The whole fork - both branches, not just the chain that linked it to the main chain - is gone from the
        // orphan pool, so it cannot mask a legitimate fork behind it forever.
        for name in deep_reorg_fork_names_tip_first() {
            let hash = *reorg_chain.get(&name).unwrap().hash();
            assert!(
                !access.contains(&DbKey::OrphanBlock(hash)).unwrap(),
                "{name} is still an orphan"
            );
            assert!(
                access.fetch_orphan_chain_tip_by_hash(&hash).unwrap().is_none(),
                "{name} is still an orphan chain tip"
            );
        }
        // Nothing is recorded in `bad_blocks`, deliberately: see `refuse_reorg_below_ghsa_activation`. The
        // record cannot survive `insert_bad_block_and_cleanup`'s sweep at this call site in a non-test build -
        // the refusal runs before any rewind, so the sweep's threshold is our own tip, which is above the height
        // being recorded - and making it survive would trade a no-op for an unbounded, remotely grown table.
        let (is_bad, _) = access.bad_block_exists(*reorg_chain.get("C2").unwrap().hash()).unwrap();
        assert!(!is_bad, "the refusal must not pretend to record a bad block");
    }

    /// The same fork and the same tip, with the activation height moved up by one so that the tip is one block
    /// short of the anchor's engagement height. The fork still reaches below the activation height, so an
    /// absolute height boundary would refuse it - and would therefore disagree with every node one block behind
    /// this one, permanently. Inside the window it is allowed.
    #[tokio::test]
    async fn deep_reorg_anchor_allows_a_fork_below_the_activation_height_inside_the_confirmation_window() {
        let activation = DEEP_REORG_FIXTURE_ENGAGED_ACTIVATION + 1;
        let (db, _mainchain, reorg_chain) = deep_reorg_fixture(activation);
        assert_eq!(
            db.get_chain_metadata().unwrap().best_block_height(),
            activation + GHSA_DEEP_REORG_CONFIRMATION_WINDOW - 1,
            "the fixture must put the tip one block below the anchor's engagement height"
        );
        assert!(
            reorg_chain.get("C2").unwrap().height() < activation,
            "the fork must still reach below the activation height, or this tests nothing"
        );

        apply_deep_reorg_fork(&db, &reorg_chain).assert_reorg(
            usize::try_from(DEEP_REORG_FIXTURE_FORK_LEN).unwrap(),
            usize::try_from(DEEP_REORG_FIXTURE_CHAIN_LEN).unwrap() - 2,
        );

        let access = db.db_write_access().unwrap();
        let tip = access.fetch_last_header().unwrap();
        assert_eq!(&tip, reorg_chain.get("E2").unwrap().header());
    }

    /// The whole schedule moved above our tip: the node is still entirely inside the pre-fork range and the
    /// anchor must not engage. This is the case that would break every node syncing history if the rule were
    /// written against the node's *software* rather than its tip.
    #[tokio::test]
    async fn deep_reorg_anchor_is_inert_below_the_activation_height() {
        let (db, _mainchain, reorg_chain) = deep_reorg_fixture(DEEP_REORG_FIXTURE_CHAIN_LEN + 1);
        apply_deep_reorg_fork(&db, &reorg_chain).assert_reorg(
            usize::try_from(DEEP_REORG_FIXTURE_FORK_LEN).unwrap(),
            usize::try_from(DEEP_REORG_FIXTURE_CHAIN_LEN).unwrap() - 2,
        );

        let access = db.db_write_access().unwrap();
        let tip = access.fetch_last_header().unwrap();
        assert_eq!(&tip, reorg_chain.get("E2").unwrap().header());
    }

    /// The boundary case the rule is deliberately phrased to allow: the fork adds blocks from exactly the
    /// activation height upwards, so every block it adds is subject to the new rules and had to do real work.
    /// The tip is far past the engagement height here, so it is the "lowest added block" phrasing doing the
    /// work, not the window.
    #[tokio::test]
    async fn deep_reorg_anchor_allows_a_reorg_that_starts_at_the_activation_height() {
        let (db, _mainchain, reorg_chain) = deep_reorg_fixture(3);
        apply_deep_reorg_fork(&db, &reorg_chain).assert_reorg(
            usize::try_from(DEEP_REORG_FIXTURE_FORK_LEN).unwrap(),
            usize::try_from(DEEP_REORG_FIXTURE_CHAIN_LEN).unwrap() - 2,
        );

        let access = db.db_write_access().unwrap();
        let tip = access.fetch_last_header().unwrap();
        assert_eq!(&tip, reorg_chain.get("E2").unwrap().header());
    }

    /// On a network where the advisory's fork is unscheduled the activation height is `u64::MAX`, so the anchor
    /// can never engage and reorg behaviour is exactly what it is today.
    #[tokio::test]
    async fn deep_reorg_anchor_is_inert_on_an_unscheduled_network() {
        let (db, _mainchain, reorg_chain) = deep_reorg_fixture(UNSCHEDULED_ACTIVATION_HEIGHT);
        apply_deep_reorg_fork(&db, &reorg_chain).assert_reorg(
            usize::try_from(DEEP_REORG_FIXTURE_FORK_LEN).unwrap(),
            usize::try_from(DEEP_REORG_FIXTURE_CHAIN_LEN).unwrap() - 2,
        );

        let access = db.db_write_access().unwrap();
        let tip = access.fetch_last_header().unwrap();
        assert_eq!(&tip, reorg_chain.get("E2").unwrap().header());
    }

    /// A refusal must not wedge the node.
    ///
    /// The refused fork branches at its tip, so `get_orphan_link_main_chain` returns only the chain ending in
    /// `E2` and the sibling `D3` is not in it. Deleting just that chain leaves `D3` in `orphans_db` *and* in
    /// `orphan_chain_tips_db` with its parent deleted out from under it, still claiming more accumulated
    /// difficulty than our chain. `swap_to_highest_pow_chain` then selects it, `get_orphan_link_main_chain`
    /// fails with `InvalidOperation` because the parent is gone, and that error propagates out of `add_block`
    /// before `cleanup_orphans` ever runs - so the node rejects blocks until it is restarted.
    #[tokio::test]
    async fn deep_reorg_anchor_refused_fork_leaves_no_orphan_behind_and_the_node_keeps_accepting_blocks() {
        let (db, mainchain, reorg_chain) = deep_reorg_fixture(DEEP_REORG_FIXTURE_ENGAGED_ACTIVATION);
        let tip_name = format!("M{DEEP_REORG_FIXTURE_CHAIN_LEN}");
        // The next two blocks on the main chain, built before the refusal so that they are the only thing the
        // node is asked to do afterwards.
        let (_, next_blocks) = crate::test_helpers::blockchain::create_chained_blocks_with_range_proof_type(
            &db,
            &[("NEXT->GB", 1, 120), ("NEXT2->NEXT", 1, 120)],
            mainchain.get(&tip_name).unwrap().clone(),
            Some(RangeProofType::RevealedValue),
        );
        let next_block = next_blocks.get("NEXT").unwrap().clone();
        let next_next_block = next_blocks.get("NEXT2").unwrap().clone();

        apply_deep_reorg_fork(&db, &reorg_chain).assert_orphaned();

        {
            let access = db.db_write_access().unwrap();
            for name in deep_reorg_fork_names_tip_first() {
                let hash = *reorg_chain.get(&name).unwrap().hash();
                assert!(
                    !access.contains(&DbKey::OrphanBlock(hash)).unwrap(),
                    "{name} survived the refusal in the orphan pool"
                );
                assert!(
                    access.fetch_orphan_chain_tip_by_hash(&hash).unwrap().is_none(),
                    "{name} survived the refusal as an orphan chain tip"
                );
            }
            assert!(
                access.fetch_strongest_orphan_chain_tips().unwrap().is_empty(),
                "the refused fork left an orphan chain tip behind"
            );
        }

        // And the node is still a working node. The true orphan goes first on purpose: a block whose parent we
        // do not have is the commonest thing on the wire, and it is the case a leftover orphan tip wedges. A
        // true orphan registers no tip of its own, so a stale tip is then the *only* thing
        // `swap_to_highest_pow_chain` can find - it outweighs our chain, `get_orphan_link_main_chain` cannot
        // reach the main chain from it because its parent was deleted, and the resulting `InvalidOperation`
        // comes back out of `add_block`.
        db.add_block(next_next_block.to_arc_block()).unwrap().assert_orphaned();
        db.add_block(next_block.to_arc_block()).unwrap();
        assert_eq!(
            db.get_chain_metadata().unwrap().best_block_height(),
            DEEP_REORG_FIXTURE_CHAIN_LEN + 2
        );
    }

    #[tokio::test]
    async fn test_handle_possible_reorg_target_difficulty_is_correct_case_1() {
        let (result, _blocks) = test_case_handle_possible_reorg(&[
            ("A->GB", 1, 12),
            ("B->A", 10, 40),
            ("C2->B", 20, 69),
            ("D2->C2", 40, 40),
        ])
        .unwrap();
        let mut expected_target_difficulties = vec![];
        expected_target_difficulties.extend(result[0].added_blocks());
        expected_target_difficulties.extend(result[1].added_blocks());
        expected_target_difficulties.extend(result[2].added_blocks());
        expected_target_difficulties.extend(result[3].added_blocks());

        let expected_target_difficulties: Vec<u64> = expected_target_difficulties
            .iter()
            .map(|b| b.accumulated_data().target_difficulty.as_u64())
            .collect();
        assert_eq!(expected_target_difficulties, vec![1, 10, 19, 24]);

        let (result, blocks) = test_case_handle_possible_reorg(&[
            ("A->GB", 1, 12),
            ("B->A", 10, 40),
            ("C->B", 30, 155),
            ("C2->B", 20, 69),
            ("D2->C2", 40, 40),
        ])
        .unwrap();

        result[0].assert_added();
        result[1].assert_added();
        result[2].assert_added();
        result[3].assert_orphaned();
        result[4].assert_reorg(2, 1);

        assert_added_hashes_eq(&result[4], vec!["C2", "D2"], &blocks);
        assert_target_difficulties_eq(&result[4], vec![19, 24]);
    }

    #[tokio::test]
    async fn test_handle_possible_reorg_banked_headers_not_aligned_with_propagated_block() {
        // env_logger::builder().filter_level(log::LevelFilter::Trace).init();  //  > ./target/output.log 2>&1
        // 1. Setup test harness and blockchain
        let test = TestHarness::setup();

        // 2. Create a chain: B1 -> B2 -> B3 -> H4 -> H5 (full blocks)
        let (_, main_chain) = create_main_chain(
            &test.db,
            block_specs!(
                ["B1->GB"],
                ["B2->B1"],
                ["B3->B2"],
                ["H4->B3"],
                ["H5->H4"],
                ["H6->H5"],
                ["H7->H6"]
            ),
        );

        // 3. Collect headers to "bank" (H4, H5, H6, H7)
        let banked_headers: Vec<_> = ["H4".to_string(), "H5".to_string(), "H6".to_string(), "H7".to_string()]
            .iter()
            .map(|n| main_chain.get(&n.clone()).unwrap().to_chain_header())
            .collect();

        // 4. Rewind to height 3 (removes H4, H5, H6, H7)
        let fork_root = main_chain.get("B3").unwrap().clone();
        assert!(
            banked_headers
                .iter()
                .all(|h| test.db.fetch_block_by_hash(*h.hash(), false).unwrap().is_some())
        );
        test.db.rewind_to_height(fork_root.height()).unwrap();
        test.db.cleanup_all_orphans().unwrap();
        assert!(
            banked_headers
                .iter()
                .all(|h| test.db.fetch_block_by_hash(*h.hash(), false).unwrap().is_none())
        );

        // 5. Add banked headers back in (headers only)
        test.db.insert_valid_headers(banked_headers.clone()).unwrap();
        assert!(
            banked_headers
                .iter()
                .all(|h| test.db.fetch_header_by_block_hash(*h.hash()).unwrap().is_some())
        );

        // 6. Create a new block that builds on the fork root (propagated block)
        let (_, reorg_chain) = create_chained_blocks(&test.db, block_specs!(["newB->GB"]), fork_root);
        let new_block = reorg_chain.get("newB").unwrap().clone().to_arc_block();

        // 7/ Reorg the blockchain to add the new block back in
        let result = test.handle_possible_reorg(new_block.clone());

        // 8. Assert that the new propagated block is in the db and banked headers are removed
        assert!(result.is_ok());
        assert!(test.db.fetch_block_by_hash(new_block.hash(), false).unwrap().is_some());
        assert!(
            banked_headers
                .iter()
                .all(|h| test.db.fetch_header_by_block_hash(*h.hash()).unwrap().is_none())
        );
    }

    #[ignore]
    #[tokio::test]
    async fn test_handle_possible_reorg_target_difficulty_is_correct_case_2() {
        // Test a straight chain to get the correct target difficulty. The block times must be reduced so that the
        // difficulty changes
        let (result, _blocks) = test_case_handle_possible_reorg(&[
            ("A->GB", 1, 12),
            ("B2->A", 10, 40),
            ("C2->B2", 20, 70),
            ("D2->C2", 25, 70),
            ("E2->D2", 30, 70),
        ])
        .unwrap();
        let mut expected_target_difficulties = vec![];
        expected_target_difficulties.extend(result[0].added_blocks());
        expected_target_difficulties.extend(result[1].added_blocks());
        expected_target_difficulties.extend(result[2].added_blocks());
        expected_target_difficulties.extend(result[3].added_blocks());
        expected_target_difficulties.extend(result[4].added_blocks());
        let expected_target_difficulties: Vec<u64> = expected_target_difficulties
            .iter()
            .map(|b| b.accumulated_data().target_difficulty.as_u64())
            .collect();
        assert_eq!(expected_target_difficulties, vec![1, 10, 19, 23, 26]);

        // Now do a reorg to make sure the target difficulties are the same
        let (result, blocks) = test_case_handle_possible_reorg(&[
            ("A->GB", 1, 12),
            ("B->A", 35, 200),
            ("C->B", 35, 200),
            ("B2->A", 10, 40),
            ("C2->B2", 20, 70),
            ("D2->C2", 25, 70),
            ("E2->D2", 30, 70),
        ])
        .unwrap();
        result[0].assert_added();
        result[1].assert_added();
        result[2].assert_added();
        result[3].assert_orphaned();
        result[4].assert_orphaned();
        result[5].assert_orphaned();
        result[6].assert_reorg(4, 2);

        assert_added_hashes_eq(&result[6], vec!["B2", "C2", "D2", "E2"], &blocks);
        assert_target_difficulties_eq(&result[6], vec![10, 19, 23, 26]);
    }

    #[ignore]
    #[tokio::test]
    async fn test_handle_possible_reorg_accum_difficulty_is_correct_case_1() {
        let (result, _blocks) = test_case_handle_possible_reorg(&[
            ("A0->GB", 1, 120), // Chain 0 at 2
            ("B0->A0", 1, 120), // Chain 0 at 3
            ("C0->B0", 1, 120), // Chain 0 at 4
            ("A1->C0", 2, 120), // Chain 1 at 6
            ("B1->A1", 2, 120), // Chain 1 at 8
            ("C1->B1", 2, 120), // Chain 1 at 10
            ("A2->C0", 2, 120), // Chain 2 at 6
            ("B2->A2", 2, 120), // Chain 2 at 8
            ("C2->B2", 2, 120), // Chain 2 at 10
            ("D2->C2", 1, 120), // Chain 2 at 11
            ("D1->C1", 1, 120), // Chain 1 at 11
            ("E1->D1", 1, 120), // Chain 1 at 12
            ("E2->D2", 1, 120), // Chain 2 at 12
        ])
        .unwrap();

        result[0].assert_added();
        result[1].assert_added();
        result[2].assert_added();

        assert_difficulty_eq(&result[0], vec![2.into()]);
        assert_difficulty_eq(&result[1], vec![3.into()]);
        assert_difficulty_eq(&result[2], vec![4.into()]);

        result[3].assert_added();
        result[4].assert_added();
        result[5].assert_added();

        assert_difficulty_eq(&result[3], vec![6.into()]);
        assert_difficulty_eq(&result[4], vec![8.into()]);
        assert_difficulty_eq(&result[5], vec![10.into()]);

        result[6].assert_orphaned();
        result[7].assert_orphaned();
        result[8].assert_orphaned();

        // ("D2->C2", 1, 120),   // Chain 2 at 11
        result[9].assert_reorg(4, 3);
        assert_difficulty_eq(&result[9], vec![6.into(), 8.into(), 10.into(), 11.into()]);

        // ("D1->C1", 1, 120),   // Chain 1 at 11
        result[10].assert_orphaned();

        // ("E1->D1", 1, 120),   // Chain 1 at 12
        result[11].assert_reorg(5, 4);
        assert_difficulty_eq(&result[11], vec![6.into(), 8.into(), 10.into(), 11.into(), 12.into()]);

        // ("E2->D2", 1, 120),   // Chain 2 at 12
        result[12].assert_orphaned();
    }

    fn check_whole_chain(db: &mut TempDatabase) {
        let mut h = db.fetch_chain_metadata().unwrap().best_block_height();
        while h > 0 {
            // fetch_chain_header_by_height will error if there are internal inconsistencies
            db.fetch_chain_header_by_height(h).unwrap();
            h -= 1;
        }
    }

    fn assert_added_hashes_eq(
        result: &BlockAddResult,
        block_names: Vec<&str>,
        blocks: &HashMap<String, Arc<ChainBlock>>,
    ) {
        let added = result.added_blocks();
        assert_eq!(
            added.iter().map(|b| b.hash()).collect::<Vec<_>>(),
            block_names
                .iter()
                .map(|b| blocks.get(*b).unwrap().hash())
                .collect::<Vec<_>>()
        );
    }

    fn assert_difficulty_eq(result: &BlockAddResult, values: Vec<U512>) {
        let accum_difficulty: Vec<U512> = result
            .added_blocks()
            .iter()
            .map(|cb| cb.accumulated_data().total_accumulated_difficulty)
            .collect();
        assert_eq!(accum_difficulty, values);
    }

    fn assert_target_difficulties_eq(result: &BlockAddResult, values: Vec<u64>) {
        let accum_difficulty: Vec<u64> = result
            .added_blocks()
            .iter()
            .map(|cb| cb.accumulated_data().target_difficulty.as_u64())
            .collect();
        assert_eq!(accum_difficulty, values);
    }

    struct TestHarness {
        db: BlockchainDatabase<TempDatabase>,
        config: BlockchainDatabaseConfig,
        consensus: BaseNodeConsensusManager,
        chain_strength_comparer: Box<dyn ChainStrengthComparer>,
        post_orphan_body_validator: Box<dyn CandidateBlockValidator<TempDatabase>>,
        header_validator: Box<dyn HeaderChainLinkedValidator<TempDatabase>>,
    }

    impl TestHarness {
        pub fn setup() -> Self {
            let consensus = create_consensus_rules();
            let db = create_new_blockchain();
            let difficulty_calculator = DifficultyCalculator::new(consensus.clone(), Default::default());
            let header_validator = Box::new(HeaderFullValidator::new(consensus.clone(), difficulty_calculator));
            let post_orphan_body_validator = Box::new(MockValidator::new(true));
            let chain_strength_comparer = strongest_chain().by_sha3x_difficulty().build();
            Self {
                db,
                config: Default::default(),
                consensus,
                chain_strength_comparer,
                header_validator,
                post_orphan_body_validator,
            }
        }

        pub fn db_write_access(&self) -> sync::RwLockWriteGuard<'_, TempDatabase> {
            self.db.db_write_access().unwrap()
        }

        pub fn handle_possible_reorg(&self, block: Arc<Block>) -> Result<BlockAddResult, ChainStorageError> {
            let mut access = self.db_write_access();
            handle_possible_reorg(
                &mut *access,
                &self.config,
                &self.consensus,
                &*self.post_orphan_body_validator,
                &*self.header_validator,
                &*self.chain_strength_comparer,
                block,
            )
        }
    }

    #[allow(clippy::type_complexity)]
    fn test_case_handle_possible_reorg<T: Into<BlockSpecs>>(
        blocks: T,
    ) -> Result<(Vec<BlockAddResult>, HashMap<String, Arc<ChainBlock>>), ChainStorageError> {
        let test = TestHarness::setup();
        let genesis_block = test
            .db
            .fetch_block(0, true)
            .unwrap()
            .try_into_chain_block()
            .map(Arc::new)
            .unwrap();
        let (block_names, chain) = { create_chained_blocks(&test.db, blocks, genesis_block) };

        let mut results = vec![];
        for name in block_names {
            let block = chain.get(&name.to_string()).unwrap();
            debug!(
                "Testing handle_possible_reorg for block {} ({}, parent = {})",
                block.height(),
                block.hash(),
                block.header().prev_hash,
            );
            results.push(test.handle_possible_reorg(block.to_arc_block()).unwrap());
        }
        Ok((results, chain))
    }

    // ---------------------------------------------------------------------------------------------------------
    // GHSA-3qmx-q9pv-f3m4 accumulated data repair: LMDB migration v8 plus the strict rebuild it arms
    // ---------------------------------------------------------------------------------------------------------

    /// These tests meet the migration the way the field meets it: a chain is built and brought to the state a
    /// pre-fork binary would have left it in, the database is closed, its migration version is put back, and it is
    /// reopened under the post-fork rules. Reopening is what runs the migration, and - when the migration arms the
    /// rebuild - what starts the background walk, so most of them are `#[tokio::test]`.
    mod c29_target_difficulty_repair {
        use std::{path::Path, time::Duration as StdDuration};

        use tari_transaction_components::consensus::ConsensusConstants;

        use super::*;
        use crate::{
            chain_storage::{AccumulatedDataRebuildStatus, lmdb_db::rewind_migration_version_for_test},
            test_helpers::blockchain::{
                create_custom_blockchain_at_path,
                create_main_chain_with_range_proof_type,
                create_orphan_chain,
            },
        };

        /// The window MainNet used before the fork, and the one its activation entry moves it to.
        const PRE_FORK_WINDOW: u64 = 90;
        const POST_FORK_WINDOW: u64 = 45;
        /// Above `PRE_FORK_WINDOW`, so the long window is full by the time the fork arrives. That is the only way
        /// the two windows can be averaging different samples, which is the whole mechanism under test.
        const ACTIVATION: u64 = 100;
        const CHAIN_LEN: u64 = 110;
        /// The version the migration must be put back to for the v8 block to run on reopen.
        const PRE_MIGRATION_VERSION: u64 = 8;

        /// Every block is mined to exactly this. `mine_to_difficulty` searches for an *exact* match against a
        /// 20,000 nonce budget, and the chance of hitting difficulty `d` on a given nonce is `1/(d(d+1))`, so the
        /// value has to stay low enough that 110 blocks in a row all land. It also has to stay above every target
        /// the LWMA can produce here, or a block would fail its own difficulty check during the rebuild.
        const ACHIEVED: u64 = 40;
        /// Keeps the LWMA off its lower clamp. Clamped targets are equal whatever the window length is, which
        /// would make the whole fixture vacuous.
        const MIN_TARGET: u64 = 15;
        /// A burst of fast blocks, placed so that it falls *inside* the 90 block window at the activation height
        /// and *outside* the 45 block one (which only reaches back to `ACTIVATION - POST_FORK_WINDOW`). Relying on
        /// a drift instead does not work: the LWMA damps a constant solve time away, and anything steep enough to
        /// survive the damping runs the target past `ACHIEVED` within a few blocks of the genesis block.
        const BURST: u64 = 40;
        const BURST_LEN: u64 = 14;

        fn constants(bipartite: bool, window: u64, from: u64) -> ConsensusConstants {
            ConsensusConstantsBuilder::new(Network::LocalNet)
                .clear_proof_of_work()
                // `BlockSpec` mines at a fixed difficulty regardless of the consensus target, so the backoff would
                // reject the second consecutive Sha3x block for not clearing its 2x target. MainNet leaves the cap
                // disabled across this fork in any case.
                .with_pow_backoff_cap(POW_BACKOFF_DISABLED)
                .add_proof_of_work(PowAlgorithm::Sha3x, PowAlgorithmConstants {
                    min_difficulty: Difficulty::from_u64(MIN_TARGET).unwrap(),
                    max_difficulty: Difficulty::max(),
                    target_time: 120,
                })
                .with_difficulty_block_window(window)
                .with_bipartite_cuckaroo_verification(bipartite)
                .with_effective_from_height(from)
                .build()
        }

        /// Two entries, the second carrying the fork: the bipartite verifier (which is what
        /// `bipartite_cuckaroo_activation_height` reads) and the narrower difficulty window, exactly as MainNet's
        /// `con_7` does.
        fn rules(activation: u64, post_fork_window: u64) -> BaseNodeConsensusManager {
            BaseNodeConsensusManager::builder(Network::LocalNet)
                .add_consensus_constants(constants(false, PRE_FORK_WINDOW, 0))
                .add_consensus_constants(constants(true, post_fork_window, activation))
                .build()
                .unwrap()
        }

        /// Rules in which nothing ever turns the bipartite verifier on, i.e. an unscheduled network.
        fn rules_unscheduled() -> BaseNodeConsensusManager {
            BaseNodeConsensusManager::builder(Network::LocalNet)
                .add_consensus_constants(constants(false, PRE_FORK_WINDOW, 0))
                .build()
                .unwrap()
        }

        /// Rules in which the base entry already carries the fix, i.e. LocalNet: activation height 0.
        fn rules_active_from_genesis() -> BaseNodeConsensusManager {
            BaseNodeConsensusManager::builder(Network::LocalNet)
                .add_consensus_constants(constants(true, POST_FORK_WINDOW, 0))
                .build()
                .unwrap()
        }

        /// A block time large enough that the block carrying it lands far beyond any plausible future time limit,
        /// whatever the genesis timestamp of the test fixture happens to be. A hundred years.
        const FAR_FUTURE_BLOCK_TIME: u64 = 100 * 365 * 24 * 60 * 60;

        /// `weak_height` is mined far below every target the LWMA produces, so it passes the mock validator that
        /// builds the chain and fails the real one the strict rebuild uses.
        ///
        /// `future_height` is given a block time far enough ahead that the block fails `check_timestamp_ftl`. The
        /// mock validator that builds the chain does not check the future time limit, so the block goes in
        /// regardless - which is exactly the shape of a node whose own clock is behind the network.
        fn chain_specs(weak_height: Option<u64>) -> Vec<crate::test_helpers::BlockSpec> {
            chain_specs_with(weak_height, None)
        }

        fn chain_specs_with(
            weak_height: Option<u64>,
            future_height: Option<u64>,
        ) -> Vec<crate::test_helpers::BlockSpec> {
            (1..=CHAIN_LEN)
                .map(|height| {
                    let name = if height == 1 {
                        leaked("M1->GB".to_string())
                    } else {
                        leaked(format!("M{height}->M{}", height - 1))
                    };
                    let block_time = if future_height == Some(height) {
                        FAR_FUTURE_BLOCK_TIME
                    } else if (BURST..BURST.saturating_add(BURST_LEN)).contains(&height) {
                        60
                    } else {
                        120
                    };
                    let difficulty = if weak_height == Some(height) { 2 } else { ACHIEVED };
                    fixture_spec(name, difficulty, block_time)
                })
                .collect()
        }

        fn stored_targets(db: &BlockchainDatabase<TempDatabase>) -> Vec<u64> {
            let access = db.db_read_access().unwrap();
            let tip = access.fetch_last_chain_header().unwrap().height();
            (0..=tip)
                .map(|h| {
                    access
                        .fetch_chain_header_by_height(h)
                        .unwrap()
                        .accumulated_data()
                        .target_difficulty
                        .as_u64()
                })
                .collect()
        }

        fn stored_accumulated_difficulties(db: &BlockchainDatabase<TempDatabase>) -> Vec<U512> {
            let access = db.db_read_access().unwrap();
            let tip = access.fetch_last_chain_header().unwrap().height();
            (0..=tip)
                .map(|h| {
                    access
                        .fetch_chain_header_by_height(h)
                        .unwrap()
                        .accumulated_data()
                        .total_accumulated_difficulty
                })
                .collect()
        }

        /// Walk the whole chain through the rebuild by hand, so the stored targets become the ones `rules` implies
        /// rather than the placeholders the mock header validator wrote while the chain was being built. Passing
        /// `u64::MAX` for `strict_from` keeps every height on the pre-fork path.
        fn rebuild_all(db: &BlockchainDatabase<TempDatabase>, rules: &BaseNodeConsensusManager, strict_from: u64) {
            let difficulty_calculator = DifficultyCalculator::new(rules.clone(), RandomXFactory::new(1));
            let validator = HeaderFullValidator::new(rules.clone(), difficulty_calculator.clone());
            for height in 1..=CHAIN_LEN {
                let cc = rules.consensus_constants(height).clone();
                process_accumulated_data_for_height(
                    db.db.clone(),
                    difficulty_calculator.clone(),
                    &validator,
                    rules,
                    height,
                    &cc,
                    (height >= strict_from).then_some(strict_from),
                )
                .unwrap_or_else(|e| panic!("rebuild failed at height {height}: {e}"));
            }
        }

        /// Build a chain across the fork and leave it exactly as a pre-fork binary would have: every stored target
        /// recomputed with the long window. The database is left on disk with its migration version put back, so
        /// the caller can reopen it under post-fork rules and meet the migration.
        fn build_pre_fork_chain(
            path: &Path,
            weak_height: Option<u64>,
        ) -> (Vec<u64>, Vec<U512>, HashMap<String, Arc<ChainBlock>>) {
            build_pre_fork_chain_from(path, chain_specs(weak_height), weak_height.is_none())
        }

        fn build_pre_fork_chain_from(
            path: &Path,
            specs: Vec<crate::test_helpers::BlockSpec>,
            rebuild: bool,
        ) -> (Vec<u64>, Vec<U512>, HashMap<String, Arc<ChainBlock>>) {
            let rules = rules(ACTIVATION, PRE_FORK_WINDOW);
            let db = create_custom_blockchain_at_path(path, rules.clone(), false);
            let (_, chain) = create_main_chain_with_range_proof_type(&db, specs, Some(RangeProofType::RevealedValue));
            assert_eq!(db.get_chain_metadata().unwrap().best_block_height(), CHAIN_LEN);
            if rebuild {
                rebuild_all(&db, &rules, u64::MAX);
            }
            let targets = stored_targets(&db);
            let accumulated = stored_accumulated_difficulties(&db);
            rewind_migration_version_for_test(db.db_read_access().unwrap().db(), PRE_MIGRATION_VERSION).unwrap();
            (targets, accumulated, chain)
        }

        fn mark_bad_block(db: &BlockchainDatabase<TempDatabase>, height: u64) {
            let access = db.db_read_access().unwrap();
            let header = access.fetch_chain_header_by_height(height).unwrap();
            let hash = *header.hash();
            drop(access);
            let mut txn = DbTransaction::new();
            txn.insert_bad_block(hash, height, "planted by a test".to_string());
            db.write(txn).unwrap();
        }

        /// Put the database into the ordinary upgrade-path shape: headers all the way to `CHAIN_LEN`, blocks
        /// stopping at `block_tip`. Header sync running ahead of block sync is the normal steady state on a node
        /// that is catching up, not an edge case, and it is the state in which the stale post-fork accumulated
        /// data this repair exists for is most likely to be sitting on disk.
        ///
        /// What makes a height "header only" to everything in the walk is `MetadataKey::ChainHeight`, which is
        /// what `set_best_block` moves; the bodies are left on disk because nothing under test reads them.
        fn set_block_tip_below_the_header_tip(db: &BlockchainDatabase<TempDatabase>, block_tip: u64) {
            let access = db.db_read_access().unwrap();
            let previous_best = *access.fetch_chain_metadata().unwrap().best_block_hash();
            let chain_header = access.fetch_chain_header_by_height(block_tip).unwrap();
            let hash = chain_header.accumulated_data().hash;
            let accumulated = chain_header.accumulated_data().total_accumulated_difficulty;
            let timestamp = chain_header.timestamp();
            drop(access);
            let mut txn = DbTransaction::new();
            txn.set_best_block(block_tip, hash, accumulated, previous_best, timestamp);
            db.write(txn).unwrap();
            assert_eq!(db.get_chain_metadata().unwrap().best_block_height(), block_tip);
        }

        fn rebuild_status(db: &BlockchainDatabase<TempDatabase>) -> AccumulatedDataRebuildStatus {
            db.db_read_access()
                .unwrap()
                .fetch_accumulated_data_rebuild_status()
                .unwrap()
        }

        /// Wait for the background rebuild to report itself finished. The pre-fork path sleeps 100 ms per height,
        /// so a walk that starts well below the activation height genuinely takes seconds.
        async fn await_rebuild(db: &BlockchainDatabase<TempDatabase>) {
            for _ in 0..600 {
                if rebuild_status(db).is_rebuilt {
                    return;
                }
                tokio::time::sleep(StdDuration::from_millis(50)).await;
            }
            panic!("background rebuild did not finish: {:?}", rebuild_status(db));
        }

        /// Recompute every target at and above the activation height under `rules`, and report the heights where
        /// the answer differs from what is stored.
        fn divergences_from_stored(
            db: &BlockchainDatabase<TempDatabase>,
            rules: &BaseNodeConsensusManager,
            from_height: u64,
        ) -> Vec<(u64, u64, u64)> {
            let tip = db.db_read_access().unwrap().fetch_last_chain_header().unwrap().height();
            divergences_in_range(db, rules, from_height, tip)
        }

        /// The same, bounded above. Needed wherever the chain carries a header that cannot be recomputed at all -
        /// a block mined below every target the LWMA can produce fails its own difficulty check here, which is a
        /// panic rather than a divergence.
        fn divergences_in_range(
            db: &BlockchainDatabase<TempDatabase>,
            rules: &BaseNodeConsensusManager,
            from_height: u64,
            to_height: u64,
        ) -> Vec<(u64, u64, u64)> {
            let stored = stored_targets(db);
            let calculator = DifficultyCalculator::new(rules.clone(), RandomXFactory::new(1));
            let access = db.db_read_access().unwrap();
            let mut diverged = vec![];
            for height in from_height..=to_height {
                let header = access.fetch_chain_header_by_height(height).unwrap().header().clone();
                let achieved = calculator
                    .check_achieved_and_target_difficulty(&*access, &header)
                    .unwrap_or_else(|e| panic!("recompute failed at height {height}: {e}"));
                let stored_target = stored[usize::try_from(height).unwrap()];
                if achieved.target().as_u64() != stored_target {
                    diverged.push((height, stored_target, achieved.target().as_u64()));
                }
            }
            diverged
        }

        /// §6.1 - the bug this whole change exists for.
        ///
        /// A chain synced on a pre-fork binary stored each target computed with the 90 block window. Under the
        /// post-fork constants the same headers recompute to different targets at and above the activation height,
        /// and only there: below it the rules at each height are unchanged, so the stored values are still right.
        /// `target_difficulty` is the only quantity that accumulates, so leaving this alone corrupts chain strength
        /// for every block above the fork.
        #[test]
        fn stored_targets_diverge_from_recomputed_ones_above_the_activation_height() {
            // Deliberately built and inspected in a single open: this test is about the divergence itself, not
            // about the migration, and reopening would arm the rebuild and spawn a task there is no runtime for.
            let path = tari_test_utils::paths::create_temporary_data_path();
            let pre_fork = rules(ACTIVATION, PRE_FORK_WINDOW);
            let db = create_custom_blockchain_at_path(&path, pre_fork.clone(), true);
            create_main_chain_with_range_proof_type(&db, chain_specs(None), Some(RangeProofType::RevealedValue));
            assert_eq!(db.get_chain_metadata().unwrap().best_block_height(), CHAIN_LEN);
            rebuild_all(&db, &pre_fork, u64::MAX);

            // Nothing diverges while the window is the one the targets were computed with.
            assert!(
                divergences_from_stored(&db, &rules(ACTIVATION, PRE_FORK_WINDOW), 1).is_empty(),
                "the pre-fork rules must reproduce their own stored targets"
            );

            // Narrowing the window at the activation height changes the answer above it...
            let post_fork = rules(ACTIVATION, POST_FORK_WINDOW);
            let diverged = divergences_from_stored(&db, &post_fork, ACTIVATION);
            assert!(
                diverged.len() >= 5,
                "expected the narrowed window to change several targets above the fork, got {diverged:?}"
            );

            // ...and nothing below it, because those heights keep the 90 block window.
            let below: Vec<_> = divergences_from_stored(&db, &post_fork, 1)
                .into_iter()
                .filter(|(height, _, _)| *height < ACTIVATION)
                .collect();
            assert!(below.is_empty(), "targets below the fork must not move, got {below:?}");
        }

        /// §6.2 - the repair, on a chain where every block still holds up.
        #[tokio::test]
        async fn the_rebuild_repairs_the_suffix_and_leaves_the_prefix_alone() {
            let path = tari_test_utils::paths::create_temporary_data_path();
            let (targets_before, accumulated_before, _) = build_pre_fork_chain(&path, None);

            let post_fork = rules(ACTIVATION, POST_FORK_WINDOW);
            let db = create_custom_blockchain_at_path(&path, post_fork.clone(), true);
            // Opening under the post-fork rules runs the migration, which arms the rebuild just below the fork.
            assert_eq!(rebuild_status(&db).last_rebuild_height, Some(ACTIVATION - 1));
            await_rebuild(&db).await;

            let targets_after = stored_targets(&db);
            let accumulated_after = stored_accumulated_difficulties(&db);

            // The tip is untouched: nothing here failed, so nothing was rewound.
            assert_eq!(db.get_chain_metadata().unwrap().best_block_height(), CHAIN_LEN);

            // Below the fork the rules at each height never changed, so the stored targets were already right.
            let prefix = usize::try_from(ACTIVATION).unwrap();
            assert_eq!(
                targets_before[..prefix],
                targets_after[..prefix],
                "targets below the fork must be byte identical"
            );

            // At and above it they have been rewritten to what the post-fork rules say...
            assert_ne!(targets_before[prefix..], targets_after[prefix..]);
            assert!(
                divergences_from_stored(&db, &post_fork, 1).is_empty(),
                "after the rebuild nothing may recompute to a different target"
            );

            // ...and because `target_difficulty` is what accumulates, the running total moved with them.
            assert_eq!(accumulated_before[..prefix], accumulated_after[..prefix]);
            assert_ne!(
                accumulated_before[usize::try_from(CHAIN_LEN).unwrap()],
                accumulated_after[usize::try_from(CHAIN_LEN).unwrap()],
                "total accumulated difficulty at the tip must be recomputed"
            );
        }

        /// §6.3 - a block above the fork whose proof of work no longer clears the corrected target.
        #[tokio::test]
        async fn a_block_that_no_longer_clears_its_target_rewinds_the_chain() {
            const WEAK: u64 = ACTIVATION + 3;

            let path = tari_test_utils::paths::create_temporary_data_path();
            let (_, _, chain) = build_pre_fork_chain(&path, Some(WEAK));
            let removed: Vec<_> = (WEAK..=CHAIN_LEN)
                .map(|h| *chain.get(&format!("M{h}")).unwrap().hash())
                .collect();

            let db = create_custom_blockchain_at_path(&path, rules(ACTIVATION, POST_FORK_WINDOW), true);
            await_rebuild(&db).await;

            assert_eq!(
                db.get_chain_metadata().unwrap().best_block_height(),
                WEAK - 1,
                "the chain must be rewound to exactly one below the offending block"
            );
            assert_eq!(
                db.db_read_access().unwrap().fetch_last_header().unwrap().height,
                WEAK - 1
            );
            assert_eq!(rebuild_status(&db), AccumulatedDataRebuildStatus {
                is_rebuilt: true,
                last_rebuild_height: Some(WEAK - 1),
            });

            // The rewind must not leave the blocks behind as orphans: their accumulated data was built from
            // pre-fork targets, and an orphan tip carrying that can win a comparison it should lose - which would
            // reorg the node straight back onto the chain it just rewound off, with the walk already finished and
            // nothing left to re-arm. The rewind, this cleanup and the status above are one committed outcome, so
            // observing any of them means observing all three.
            let access = db.db_read_access().unwrap();
            assert!(
                access.fetch_strongest_orphan_chain_tips().unwrap().is_empty(),
                "a strict rewind must not leave stale orphan tips behind"
            );
            for hash in &removed {
                assert!(
                    !access.contains(&DbKey::OrphanBlock(*hash)).unwrap(),
                    "a strict rewind must not leave the removed blocks behind as orphans"
                );
            }
        }

        /// §6.4 - the rewind has to be reachable by a rule the difficulty calculator cannot see.
        ///
        /// This is what §4.2 buys: the rebuild runs the *full* header validator, not just
        /// `check_achieved_and_target_difficulty`. Without it a block that is invalid for one of the activation
        /// rules would pass the repair, and the node would sit on a chain it would reject if it re-synced.
        #[tokio::test]
        async fn a_block_that_fails_a_non_difficulty_rule_rewinds_the_chain() {
            const BAD: u64 = ACTIVATION + 3;

            let path = tari_test_utils::paths::create_temporary_data_path();
            {
                let rules = rules(ACTIVATION, PRE_FORK_WINDOW);
                let db = create_custom_blockchain_at_path(&path, rules.clone(), false);
                create_main_chain_with_range_proof_type(&db, chain_specs(None), Some(RangeProofType::RevealedValue));
                rebuild_all(&db, &rules, u64::MAX);
                mark_bad_block(&db, BAD);

                // The difficulty calculator on its own is perfectly happy with this block - it clears its target.
                // Only the full validator's `check_not_bad_block` sees anything wrong, which is the point.
                let access = db.db_read_access().unwrap();
                let header = access.fetch_chain_header_by_height(BAD).unwrap().header().clone();
                DifficultyCalculator::new(rules.clone(), RandomXFactory::new(1))
                    .check_achieved_and_target_difficulty(&*access, &header)
                    .expect("the difficulty calculator alone cannot see this");
                drop(access);

                rewind_migration_version_for_test(db.db_read_access().unwrap().db(), PRE_MIGRATION_VERSION).unwrap();
            }

            let db = create_custom_blockchain_at_path(&path, rules(ACTIVATION, POST_FORK_WINDOW), true);
            await_rebuild(&db).await;

            assert_eq!(db.get_chain_metadata().unwrap().best_block_height(), BAD - 1);
            assert_eq!(rebuild_status(&db), AccumulatedDataRebuildStatus {
                is_rebuilt: true,
                last_rebuild_height: Some(BAD - 1),
            });
        }

        /// §6.5 - coexistence with a v5 rebuild that is still in flight.
        ///
        /// The `min` in the migration is load-bearing: overwriting a watermark that is below the activation height
        /// would silently abandon the rest of that repair. And strict mode is decided per height, not per task, so
        /// the part of the walk below the fork keeps its full throttle and its non-validating behaviour.
        #[tokio::test]
        async fn a_v5_rebuild_in_flight_is_not_abandoned_and_keeps_the_pre_fork_behaviour() {
            /// Well below the activation height, as a node part way through the earlier rebuild would be.
            const V5_WATERMARK: u64 = 50;

            let path = tari_test_utils::paths::create_temporary_data_path();
            let (targets_before, _, _) = build_pre_fork_chain(&path, None);
            {
                // Opened under rules the migration skips. An armed migration spawns the rebuild task, which holds
                // its own handle on the backend, so the LMDB environment would outlive this block and the next
                // open would fail on the file lock.
                let db = create_custom_blockchain_at_path(&path, rules_unscheduled(), false);
                db.db_read_access()
                    .unwrap()
                    .set_accumulated_data_rebuild_status(AccumulatedDataRebuildStatus {
                        is_rebuilt: false,
                        last_rebuild_height: Some(V5_WATERMARK),
                    })
                    .unwrap();
                // A header below the fork that the full validator would reject. Strict mode must not reach it: if
                // it engaged for the whole task rather than per height, this would rewind the chain to
                // `V5_WATERMARK + 2` instead of running to completion.
                mark_bad_block(&db, V5_WATERMARK + 3);
                rewind_migration_version_for_test(db.db_read_access().unwrap().db(), PRE_MIGRATION_VERSION).unwrap();
            }

            let db = create_custom_blockchain_at_path(&path, rules(ACTIVATION, POST_FORK_WINDOW), true);

            // The migration must not raise the watermark past where the earlier rebuild had got to.
            assert_eq!(rebuild_status(&db), AccumulatedDataRebuildStatus {
                is_rebuilt: false,
                last_rebuild_height: Some(V5_WATERMARK),
            });

            await_rebuild(&db).await;

            // It ran to completion: the bad block below the fork was never looked at, because strict mode only
            // engages at and above the activation height.
            assert_eq!(db.get_chain_metadata().unwrap().best_block_height(), CHAIN_LEN);

            // And it did the repair: the prefix is untouched, the suffix is now self-consistent.
            let targets_after = stored_targets(&db);
            let prefix = usize::try_from(ACTIVATION).unwrap();
            assert_eq!(targets_before[..prefix], targets_after[..prefix]);
            assert!(divergences_from_stored(&db, &rules(ACTIVATION, POST_FORK_WINDOW), 1).is_empty());
        }

        /// The skip path must not touch a rebuild that is still owed.
        ///
        /// Every migration block runs inside one `run_migrations` loop, so a database at version 5 arms the v5
        /// repair and then reaches this block in the same call. Marking the rebuild done here - because *this*
        /// fork has nothing to repair on this network - would latch `is_rebuilt: true` and silently abandon the
        /// v5 repair, on exactly the nodes that still need it.
        ///
        /// Each case needs its own database: leaving the in-flight status alone is the whole point, so the open
        /// under test arms the background walk, and that task holds the backend open past the end of the case.
        async fn skip_path_leaves_an_in_flight_rebuild_alone(case_rules: BaseNodeConsensusManager) {
            const IN_FLIGHT_WATERMARK: u64 = 50;

            let in_flight = AccumulatedDataRebuildStatus {
                is_rebuilt: false,
                last_rebuild_height: Some(IN_FLIGHT_WATERMARK),
            };

            let path = tari_test_utils::paths::create_temporary_data_path();
            build_pre_fork_chain(&path, None);
            {
                // Put the database into "the v5 repair is still in flight" and back to version 8. Opened under
                // rules the migration skips, and against a status that is already finished, so this open neither
                // arms nor spawns anything itself.
                let db = create_custom_blockchain_at_path(&path, rules_unscheduled(), false);
                db.db_read_access()
                    .unwrap()
                    .set_accumulated_data_rebuild_status(in_flight.clone())
                    .unwrap();
                rewind_migration_version_for_test(db.db_read_access().unwrap().db(), PRE_MIGRATION_VERSION).unwrap();
            }

            let db = create_custom_blockchain_at_path(&path, case_rules, true);
            // Read synchronously, before the test yields: `#[tokio::test]` is a current-thread runtime, so the
            // rebuild task this open spawned cannot have run a single height yet.
            assert_eq!(
                rebuild_status(&db),
                in_flight,
                "the skip path must leave both the watermark and `is_rebuilt: false` untouched"
            );
        }

        /// A NextNet node at MigrationVersion 5: the fork is unscheduled, so the v8 block skips - in the same
        /// `run_migrations` call that armed the v5 repair three iterations earlier.
        #[tokio::test]
        async fn the_skip_path_leaves_an_in_flight_rebuild_alone_when_the_fork_is_unscheduled() {
            skip_path_leaves_an_in_flight_rebuild_alone(rules_unscheduled()).await;
        }

        /// The same, for a MainNet node whose tip has not reached 350,000 yet.
        #[tokio::test]
        async fn the_skip_path_leaves_an_in_flight_rebuild_alone_when_the_tip_is_below_the_fork() {
            skip_path_leaves_an_in_flight_rebuild_alone(rules(CHAIN_LEN + 50, POST_FORK_WINDOW)).await;
        }

        /// A *finished* rebuild must not drag the walk back over its stale watermark.
        ///
        /// `update_accumulated_difficulty` records `is_rebuilt: height == last_chain_header.height()`, so a
        /// completed rebuild leaves the watermark at the tip *as it was when it completed*. A MainNet node that
        /// finished the v5 rebuild at height 200,000 and then synced to 350,053 carries
        /// `{is_rebuilt: true, last_rebuild_height: Some(200_000)}`; folding that into the `min` would re-walk
        /// 150,000 already-correct heights at 100 ms each - over four hours - before reaching the fork.
        #[tokio::test]
        async fn a_finished_rebuild_with_a_stale_watermark_arms_at_the_activation_height() {
            const STALE_WATERMARK: u64 = 50;

            let path = tari_test_utils::paths::create_temporary_data_path();
            build_pre_fork_chain(&path, None);
            {
                let db = create_custom_blockchain_at_path(&path, rules_unscheduled(), false);
                db.db_read_access()
                    .unwrap()
                    .set_accumulated_data_rebuild_status(AccumulatedDataRebuildStatus {
                        is_rebuilt: true,
                        last_rebuild_height: Some(STALE_WATERMARK),
                    })
                    .unwrap();
                rewind_migration_version_for_test(db.db_read_access().unwrap().db(), PRE_MIGRATION_VERSION).unwrap();
            }

            let db = create_custom_blockchain_at_path(&path, rules(ACTIVATION, POST_FORK_WINDOW), true);

            // Armed at the fork, not at the stale watermark: the finished rebuild is evidence about a tip that
            // has since moved, not about the prefix being unrepaired.
            assert_eq!(rebuild_status(&db), AccumulatedDataRebuildStatus {
                is_rebuilt: false,
                last_rebuild_height: Some(ACTIVATION - 1),
            });

            await_rebuild(&db).await;
            assert_eq!(db.get_chain_metadata().unwrap().best_block_height(), CHAIN_LEN);
            assert!(divergences_from_stored(&db, &rules(ACTIVATION, POST_FORK_WINDOW), 1).is_empty());
        }

        /// A background task must never wipe a pruned node's chain.
        ///
        /// `rewind_to_height` turns a rewind that reaches past a pruned node's effective pruning horizon into a
        /// rewind to height 0 - a full destructive wipe - and in that branch it also writes an orphan chain tip
        /// without the matching orphan block. `rewind_below_invalid_header` must recognise that case before it
        /// starts, refuse it, and leave the rebuild unfinished so the operator sees it again.
        #[tokio::test]
        async fn a_rewind_that_would_wipe_a_pruned_node_is_refused() {
            const INVALID_AT: u64 = ACTIVATION;

            let path = tari_test_utils::paths::create_temporary_data_path();
            build_pre_fork_chain(&path, None);
            let db = create_custom_blockchain_at_path(&path, rules_unscheduled(), true);

            // Make it a pruned node with only a few blocks of history: `is_pruned_node()` reads the pruning
            // horizon, and the effective horizon is `tip - pruned_height`, so the rewind to `INVALID_AT - 1`
            // reaches well past it.
            let mut txn = DbTransaction::new();
            txn.set_pruning_horizon(5);
            txn.set_pruned_height(CHAIN_LEN - 5);
            db.write(txn).unwrap();

            let status_before = rebuild_status(&db);
            let result = {
                let mut access = db.db_write_access().unwrap();
                rewind_below_invalid_header(
                    &mut *access,
                    INVALID_AT,
                    &ValidationError::ConsensusError("planted by a test".to_string()),
                )
            };

            assert!(
                result.is_err(),
                "the rewind must be refused, got {:?}",
                result.map(|s| format!("{s:?}"))
            );
            // Nothing was removed, and nothing claims the repair is finished.
            assert_eq!(
                db.get_chain_metadata().unwrap().best_block_height(),
                CHAIN_LEN,
                "a refused rewind must not touch the chain"
            );
            assert_eq!(
                db.db_read_access().unwrap().fetch_last_header().unwrap().height,
                CHAIN_LEN
            );
            assert_eq!(
                rebuild_status(&db),
                status_before,
                "a refused rewind must not mark the rebuild done"
            );
            assert!(
                db.db_read_access()
                    .unwrap()
                    .fetch_strongest_orphan_chain_tips()
                    .unwrap()
                    .is_empty(),
                "a refused rewind must not leave an orphan chain tip behind"
            );
        }

        /// Header sync ahead of block sync: the heights that have a header but no block yet must be armed for,
        /// and repaired, exactly like the ones that do.
        ///
        /// This is the shape of the upgrade path, not an exotic one. Nothing ever goes back over a header that is
        /// already on disk - header sync neither re-validates nor rewrites one, and takes the *stored* accumulated
        /// data at the split as its accumulation base - and block sync writes that stored value back verbatim as
        /// each body arrives, promoting it into chain metadata. So a post-fork header left holding the old
        /// binary's pre-fork-window target is not a transient state that block sync cleans up; it is the exact
        /// corruption this change exists to remove, and it would be chained off a repaired prefix.
        ///
        /// Two separate defects are covered here, because they compound. The block tip is below the activation
        /// height, so a migration that arms off `MetadataKey::ChainHeight` never arms at all; and once it does
        /// arm, a walk that stops at the block tip repairs nothing above it and latches `is_rebuilt: true`.
        #[tokio::test]
        async fn header_only_heights_above_the_block_tip_are_armed_for_and_repaired() {
            /// Below the fork, so the arming decision cannot be made from the block tip either.
            const BLOCK_TIP: u64 = ACTIVATION - 10;

            let path = tari_test_utils::paths::create_temporary_data_path();
            let (targets_before, _, _) = build_pre_fork_chain(&path, None);
            {
                // Opened under rules the migration skips, so this open neither arms nor spawns anything.
                let db = create_custom_blockchain_at_path(&path, rules_unscheduled(), false);
                set_block_tip_below_the_header_tip(&db, BLOCK_TIP);
                rewind_migration_version_for_test(db.db_read_access().unwrap().db(), PRE_MIGRATION_VERSION).unwrap();
            }

            let post_fork = rules(ACTIVATION, POST_FORK_WINDOW);
            let db = create_custom_blockchain_at_path(&path, post_fork.clone(), true);

            // Armed, even though the block tip is ten heights below the fork.
            assert_eq!(
                rebuild_status(&db).last_rebuild_height,
                Some(ACTIVATION - 1),
                "the arming decision must be taken on the header tip, not the block tip"
            );
            await_rebuild(&db).await;

            // Every post-fork height was repaired, including the eleven that have no block.
            assert!(
                divergences_from_stored(&db, &post_fork, 1).is_empty(),
                "header-only heights must be repaired, not skipped"
            );
            assert_eq!(rebuild_status(&db), AccumulatedDataRebuildStatus {
                is_rebuilt: true,
                last_rebuild_height: Some(CHAIN_LEN),
            });

            // Repairing a header-only height rewrites it; it never removes it, and it never moves the block tip.
            assert_eq!(
                db.db_read_access().unwrap().fetch_last_header().unwrap().height,
                CHAIN_LEN
            );
            assert_eq!(db.get_chain_metadata().unwrap().best_block_height(), BLOCK_TIP);

            // And the prefix below the fork is still byte identical.
            let prefix = usize::try_from(ACTIVATION).unwrap();
            assert_eq!(targets_before[..prefix], stored_targets(&db)[..prefix]);
        }

        /// A header above the block tip that no longer validates must stop the walk, not rewind it.
        ///
        /// Repairing a header-only height is well defined; rewinding at one is not. `rewind_to_height` measures
        /// its block removals from the block tip, so the pruned-node refusal and the "how many blocks are about
        /// to go" accounting are all computed against a chain that does not reach up here, while the headers it
        /// would delete are counted from the *header* tip and could run tens of thousands deep on one verdict.
        #[tokio::test]
        async fn a_permanent_verdict_above_the_block_tip_stops_instead_of_rewinding() {
            const WEAK: u64 = ACTIVATION + 3;
            /// Above the fork but below `WEAK`, so the walk enters strict mode on heights that have blocks and
            /// then meets the offending header at a height that does not.
            const BLOCK_TIP: u64 = ACTIVATION + 1;

            let path = tari_test_utils::paths::create_temporary_data_path();
            build_pre_fork_chain(&path, Some(WEAK));
            {
                let db = create_custom_blockchain_at_path(&path, rules_unscheduled(), false);
                set_block_tip_below_the_header_tip(&db, BLOCK_TIP);
                rewind_migration_version_for_test(db.db_read_access().unwrap().db(), PRE_MIGRATION_VERSION).unwrap();
            }

            let db = create_custom_blockchain_at_path(&path, rules(ACTIVATION, POST_FORK_WINDOW), true);
            for _ in 0..600 {
                if rebuild_status(&db).last_rebuild_height == Some(WEAK - 1) {
                    break;
                }
                tokio::time::sleep(StdDuration::from_millis(50)).await;
            }
            // Give it long enough to do the wrong thing if it were going to.
            tokio::time::sleep(StdDuration::from_millis(500)).await;

            assert_eq!(
                db.db_read_access().unwrap().fetch_last_header().unwrap().height,
                CHAIN_LEN,
                "a header-only height must not delete a single header"
            );
            assert_eq!(
                db.get_chain_metadata().unwrap().best_block_height(),
                BLOCK_TIP,
                "a header-only height must not move the block tip"
            );
            assert_eq!(
                rebuild_status(&db),
                AccumulatedDataRebuildStatus {
                    is_rebuilt: false,
                    last_rebuild_height: Some(WEAK - 1),
                },
                "the walk must stop unfinished, so it is tried again once block sync has brought the body in"
            );
        }

        /// A pruned node whose pruned height has advanced past the fork must still be repaired.
        ///
        /// An earlier revision hoisted the pruned-node rewind refusal to the top of the walk, where the predicate
        /// reduces to `pruned_height >= activation`. That is independent of the tip, so on a pruned node - where
        /// `pruned_height ~ tip - pruning_horizon` - it is false at rollout and *permanently* true once the chain
        /// has advanced one pruning horizon past the fork. Every startup from then on refused the repair at its
        /// first strict height and left the node with its stale post-fork data, although nothing here fails
        /// validation and no rewind is ever attempted.
        #[tokio::test]
        async fn a_pruned_node_past_the_activation_height_is_still_repaired() {
            let path = tari_test_utils::paths::create_temporary_data_path();
            let (targets_before, _, _) = build_pre_fork_chain(&path, None);
            let post_fork = rules(ACTIVATION, POST_FORK_WINDOW);
            // Opened under rules the migration skips, so the walk does not start on its own and the test drives
            // it. It has to be driven from the same open the pruned-node state is set up in: `start_new`
            // rewrites `pruning_horizon` from the config on every open, so a reopen would quietly turn this back
            // into an archival node and the test would pass without testing anything.
            let db = create_custom_blockchain_at_path(&path, rules_unscheduled(), true);
            let mut txn = DbTransaction::new();
            txn.set_pruning_horizon(5);
            txn.set_pruned_height(CHAIN_LEN - 5);
            db.write(txn).unwrap();
            let metadata = db.get_chain_metadata().unwrap();
            assert!(metadata.is_pruned_node());
            assert!(
                metadata.pruned_height() >= ACTIVATION,
                "the point of the fixture is a pruned height that has advanced past the fork"
            );

            let difficulty_calculator = DifficultyCalculator::new(post_fork.clone(), RandomXFactory::new(1));
            let validator = HeaderFullValidator::new(post_fork.clone(), difficulty_calculator.clone());
            for height in ACTIVATION..=CHAIN_LEN {
                let step = process_accumulated_data_for_height(
                    db.db.clone(),
                    difficulty_calculator.clone(),
                    &validator,
                    &post_fork,
                    height,
                    &post_fork.consensus_constants(height).clone(),
                    Some(ACTIVATION),
                )
                .unwrap_or_else(|e| {
                    panic!("a pruned node must not be refused the repair, but height {height} failed: {e}")
                });
                assert!(
                    matches!(step, RebuildStep::Processed(_)),
                    "height {height} was not repaired, got {step:?}"
                );
            }

            assert!(
                divergences_from_stored(&db, &post_fork, 1).is_empty(),
                "a pruned node's post-fork targets must be repaired like any other node's"
            );
            assert_eq!(
                db.get_chain_metadata().unwrap().best_block_height(),
                CHAIN_LEN,
                "nothing here fails validation, so nothing may be rewound"
            );
            let prefix = usize::try_from(ACTIVATION).unwrap();
            assert_eq!(targets_before[..prefix], stored_targets(&db)[..prefix]);
        }

        /// A walk that finds itself above the header tip has been overtaken by a reorg that shortened the chain.
        /// It must not call that "finished".
        ///
        /// The walk only ever climbs, so the ordinary way it ends is by rewriting the header tip itself. Arriving
        /// *above* the header tip therefore means the branch it repaired the heights below on is gone, and the
        /// heights that replaced them were not re-validated by whatever put them there. Latching
        /// `is_rebuilt: true` here would mark the repair finished over a suffix that was never repaired, one-way,
        /// with nothing left to re-arm it.
        #[tokio::test]
        async fn a_walk_that_climbs_above_the_header_tip_resumes_instead_of_finishing() {
            let path = tari_test_utils::paths::create_temporary_data_path();
            build_pre_fork_chain(&path, None);
            let post_fork = rules(ACTIVATION, POST_FORK_WINDOW);
            // Opened under rules the migration skips, so the walk does not start on its own and the test drives it.
            let db = create_custom_blockchain_at_path(&path, rules_unscheduled(), true);
            let status_before = rebuild_status(&db);

            let difficulty_calculator = DifficultyCalculator::new(post_fork.clone(), RandomXFactory::new(1));
            let validator = HeaderFullValidator::new(post_fork.clone(), difficulty_calculator.clone());
            let step = process_accumulated_data_for_height(
                db.db.clone(),
                difficulty_calculator.clone(),
                &validator,
                &post_fork,
                CHAIN_LEN + 1,
                &post_fork.consensus_constants(CHAIN_LEN + 1).clone(),
                Some(ACTIVATION),
            )
            .unwrap();

            match step {
                RebuildStep::Resume { from } => assert_eq!(
                    from, ACTIVATION,
                    "the whole strict suffix has to be climbed again, because the split may be anywhere in it"
                ),
                other => panic!("expected the walk to resume, got {other:?}"),
            }
            assert_eq!(
                rebuild_status(&db),
                status_before,
                "a resume must not write a status, least of all a finished one"
            );

            // The one case where there is genuinely nothing left: the chain no longer reaches the fork at all.
            // Re-walking there would only spin until the resume budget ran out, so this really is finished.
            let far_fork = rules(CHAIN_LEN + 5, POST_FORK_WINDOW);
            let step = process_accumulated_data_for_height(
                db.db.clone(),
                DifficultyCalculator::new(far_fork.clone(), RandomXFactory::new(1)),
                &validator,
                &far_fork,
                CHAIN_LEN + 6,
                &far_fork.consensus_constants(CHAIN_LEN + 6).clone(),
                Some(CHAIN_LEN + 5),
            )
            .unwrap();
            match step {
                RebuildStep::Processed(status) => assert_eq!(status, AccumulatedDataRebuildStatus {
                    is_rebuilt: true,
                    last_rebuild_height: Some(CHAIN_LEN),
                }),
                other => panic!("expected the walk to finish, got {other:?}"),
            }
        }

        /// The write-phase decision, branch by branch.
        ///
        /// Raced into existence it would be untestable, so it is a pure function and this is the whole truth
        /// table. The rows that matter most are the ones earlier revisions got wrong: a moved header is never a
        /// skip and never a retry in place, a permanent verdict reached against tips that have since moved is a
        /// `Retry` and never a rewind, and a permanent verdict above the block tip is never a rewind at all.
        #[test]
        fn the_write_phase_decision_covers_every_branch() {
            let permanent = ValidationError::ConsensusError("a permanent property of the block".to_string());
            let can_change = ValidationError::FatalStorageError("a transient read".to_string());

            // Nothing moved and the header validated: rewrite its accumulated data. Whether the height has a
            // block or only a header makes no difference - repairing accumulated data is the same operation.
            assert_eq!(decide_strict_action(false, false, false, None), StrictAction::Rewrite);
            assert_eq!(decide_strict_action(false, false, true, None), StrictAction::Rewrite);

            // The header at this height (or its parent) is not the one that was validated. Whatever the verdict
            // was, it belongs to a header that is no longer there. Never a skip: the replacement is not
            // re-validated by the reorg paths that put it there, so skipping would leave a stale row in place and
            // chain it forward over the rest of the suffix. And never a retry in place either: the walk is
            // monotonic, so a reorg that split below this height replaced heights it has already passed, and only
            // climbing them again makes the finish line true of the chain the walk ended on.
            assert_eq!(decide_strict_action(true, false, false, None), StrictAction::Resume);
            assert_eq!(
                decide_strict_action(true, false, false, Some(&permanent)),
                StrictAction::Resume
            );
            assert_eq!(
                decide_strict_action(true, true, false, Some(&permanent)),
                StrictAction::Resume
            );
            assert_eq!(
                decide_strict_action(true, true, false, Some(&can_change)),
                StrictAction::Resume
            );
            assert_eq!(
                decide_strict_action(true, true, true, Some(&permanent)),
                StrictAction::Resume
            );

            // A permanent verdict against exactly the chain it was reached on, at a height that has a block:
            // rewind.
            assert_eq!(
                decide_strict_action(false, false, false, Some(&permanent)),
                StrictAction::Rewind
            );

            // The same verdict, but sync or a reorg moved a tip while the header was being validated. A rewind is
            // a statement about every block above this height, so it must not be executed against a chain that is
            // no longer the one it was decided against. This is what stands in for the add-block gate that is
            // deliberately not reintroduced. Checked before `header_only`, because `header_only` was read from
            // the same snapshot and a moved tip is exactly what invalidates it.
            assert_eq!(
                decide_strict_action(false, true, false, Some(&permanent)),
                StrictAction::Retry
            );
            assert_eq!(
                decide_strict_action(false, true, true, Some(&permanent)),
                StrictAction::Retry
            );

            // The same verdict again, at a height above the block tip. The repair applies there, but a rewind
            // does not: `rewind_to_height` measures its block removals from the block tip, so every guard that
            // bounds one is computed against a chain that does not reach up here, while the headers it would
            // delete are counted from the header tip. Stop and let block sync bring the body in.
            assert_eq!(
                decide_strict_action(false, false, true, Some(&permanent)),
                StrictAction::StopHeaderOnly
            );

            // A verdict that can come out differently later never destroys blocks, moved tips or not, block or
            // no block.
            assert_eq!(
                decide_strict_action(false, false, false, Some(&can_change)),
                StrictAction::Stop
            );
            assert_eq!(
                decide_strict_action(false, true, false, Some(&can_change)),
                StrictAction::Stop
            );
            assert_eq!(
                decide_strict_action(false, false, true, Some(&can_change)),
                StrictAction::Stop
            );
        }

        /// Which verdicts are a permanent property of the block, and which are not.
        ///
        /// The discriminator is deliberately the one the codebase already has - `get_ban_reason() == None`, plus
        /// the two chain/environment dependent cases `BlockHeaderSyncValidator::blacklist_unless_verdict_can_change`
        /// and `insert_orphan_and_find_new_tips` already special-case - so that the three cannot drift apart.
        #[test]
        fn only_permanent_verdicts_are_allowed_to_destroy_blocks() {
            // A wall clock behind the network: a VM resumed from suspend, a container before NTP settles. This
            // fails on every recent header, and rewinding on it would delete the whole post-fork suffix for a
            // clock problem.
            assert!(verdict_can_change(&ValidationError::BlockHeaderError(
                BlockHeaderValidationError::InvalidTimestampFutureTimeLimit
            )));
            // Measured against the chain the header is on, not against the header.
            assert!(verdict_can_change(&ValidationError::BlockHeaderError(
                BlockHeaderValidationError::OldSeedHash
            )));
            // Every `ChainStorageError` becomes this when it crosses into validation, so a transient LMDB read
            // failure inside `get_previous_timestamps` or `check_not_bad_block` lands here.
            assert!(verdict_can_change(&ValidationError::FatalStorageError(
                "transient".to_string()
            )));
            assert!(verdict_can_change(
                &ValidationError::IncorrectNumberOfTimestampsProvided {
                    expected: 11,
                    actual: 3
                }
            ));
            assert!(verdict_can_change(&ValidationError::HeaderHashMismatch(
                "mismatch".to_string()
            )));
            assert!(verdict_can_change(&ValidationError::HeaderHeightMismatch(
                "7 != 8".to_string()
            )));

            // ...and the ones that really are about this block and cannot come out differently later.
            assert!(!verdict_can_change(&ValidationError::ConsensusError(
                "nope".to_string()
            )));
            assert!(!verdict_can_change(&ValidationError::BadBlockFound {
                hash: FixedHash::zero().to_hex(),
                reason: "already known bad".to_string(),
            }));
            assert!(!verdict_can_change(&ValidationError::BlockHeaderError(
                BlockHeaderValidationError::InvalidHeight { expected: 5, actual: 9 }
            )));
        }

        /// A verdict that can change must stop the walk, not rewind the chain.
        ///
        /// The block above the fork is dated a century into the future, which is what a node whose own clock is
        /// far behind the network sees on every recent header. Before this, *every* `ValidationError` rewound -
        /// including this one - so such a node would boot, walk to the fork, delete its whole post-fork suffix and
        /// latch `is_rebuilt: true`, leaving it to resync across a fork most of its peers have not taken yet.
        #[tokio::test]
        async fn a_verdict_that_can_change_stops_the_walk_instead_of_rewinding() {
            const FUTURE: u64 = ACTIVATION + 3;

            let path = tari_test_utils::paths::create_temporary_data_path();
            build_pre_fork_chain_from(&path, chain_specs_with(None, Some(FUTURE)), true);

            let db = create_custom_blockchain_at_path(&path, rules(ACTIVATION, POST_FORK_WINDOW), true);
            // The walk repairs up to the block below the offending one and then stops.
            for _ in 0..600 {
                if rebuild_status(&db).last_rebuild_height == Some(FUTURE - 1) {
                    break;
                }
                tokio::time::sleep(StdDuration::from_millis(50)).await;
            }
            // Give it long enough to do the wrong thing if it were going to.
            tokio::time::sleep(StdDuration::from_millis(500)).await;

            assert_eq!(
                db.get_chain_metadata().unwrap().best_block_height(),
                CHAIN_LEN,
                "a verdict that can change must not rewind a single block"
            );
            assert_eq!(
                db.db_read_access().unwrap().fetch_last_header().unwrap().height,
                CHAIN_LEN,
                "a verdict that can change must not delete a single header"
            );
            assert_eq!(
                rebuild_status(&db),
                AccumulatedDataRebuildStatus {
                    is_rebuilt: false,
                    last_rebuild_height: Some(FUTURE - 1),
                },
                "the walk must stop unfinished so the next startup retries the same height"
            );
        }

        /// A pruned node that genuinely cannot perform the rewind repairs everything up to it and then stops.
        ///
        /// The refusal is asked about the rewind actually being attempted, so a pruned node is not excluded from
        /// the repair for a rewind that may never happen - it gets the whole repair up to the height that needs
        /// one. What it does not get is the rewind, and the walk stops there with `is_rebuilt` still false.
        ///
        /// Leaving `[activation, H-1]` rewritten and `[H, tip]` stale is the accepted outcome here, not a new
        /// one: `StrictAction::Stop`, `StrictAction::StopHeaderOnly` and the consecutive-retry bound all produce
        /// it deliberately, and it is transient for the same reason - the next startup resumes from the
        /// persisted watermark.
        #[tokio::test]
        async fn a_pruned_node_that_cannot_rewind_repairs_up_to_the_refusal_and_stops() {
            const WEAK: u64 = ACTIVATION + 3;

            let path = tari_test_utils::paths::create_temporary_data_path();
            build_pre_fork_chain(&path, Some(WEAK));
            let post_fork = rules(ACTIVATION, POST_FORK_WINDOW);
            // Driven by hand, and from the same open the pruned-node state is set up in - see
            // `a_pruned_node_past_the_activation_height_is_still_repaired`.
            let db = create_custom_blockchain_at_path(&path, rules_unscheduled(), true);
            // A pruned node whose history no longer reaches `WEAK - 1`.
            let mut txn = DbTransaction::new();
            txn.set_pruning_horizon(5);
            txn.set_pruned_height(CHAIN_LEN - 5);
            db.write(txn).unwrap();

            let difficulty_calculator = DifficultyCalculator::new(post_fork.clone(), RandomXFactory::new(1));
            let validator = HeaderFullValidator::new(post_fork.clone(), difficulty_calculator.clone());
            let step_at = |height: u64| {
                process_accumulated_data_for_height(
                    db.db.clone(),
                    difficulty_calculator.clone(),
                    &validator,
                    &post_fork,
                    height,
                    &post_fork.consensus_constants(height).clone(),
                    Some(ACTIVATION),
                )
            };
            for height in ACTIVATION..WEAK {
                step_at(height).unwrap_or_else(|e| {
                    panic!("the repair must not be refused before the rewind, got {e} at {height}")
                });
            }
            let refused = step_at(WEAK);
            assert!(
                refused.is_err(),
                "the rewind this pruned node cannot perform must be refused, got {:?}",
                refused.map(|s| format!("{s:?}"))
            );

            // The refusal is about the rewind, so everything below it was still repaired.
            let diverged_below_the_refusal = divergences_in_range(&db, &post_fork, ACTIVATION, WEAK - 1);
            assert!(
                diverged_below_the_refusal.is_empty(),
                "a pruned node must still be repaired up to the height that needs a rewind, got \
                 {diverged_below_the_refusal:?}"
            );

            // And the rewind itself was refused, so not a block and not a header went.
            assert_eq!(db.get_chain_metadata().unwrap().best_block_height(), CHAIN_LEN);
            assert_eq!(
                db.db_read_access().unwrap().fetch_last_header().unwrap().height,
                CHAIN_LEN
            );
            assert_eq!(
                rebuild_status(&db),
                AccumulatedDataRebuildStatus {
                    is_rebuilt: false,
                    last_rebuild_height: Some(WEAK - 1),
                },
                "a refused rewind must leave the repair unfinished"
            );
            assert!(
                db.db_read_access()
                    .unwrap()
                    .fetch_strongest_orphan_chain_tips()
                    .unwrap()
                    .is_empty(),
                "a refused rewind must not leave an orphan chain tip behind"
            );
        }

        /// The walk climbs to the header tip, and a height that has only a header is repaired like any other.
        ///
        /// The companion to `header_only_heights_above_the_block_tip_are_armed_for_and_repaired`, driven by hand
        /// so that the single step at the header-only height can be inspected directly: it must come back
        /// `Processed`, with the accumulated data rewritten, and with the header still there.
        #[tokio::test]
        async fn the_strict_walk_repairs_up_to_the_header_tip() {
            let path = tari_test_utils::paths::create_temporary_data_path();
            build_pre_fork_chain(&path, None);
            let post_fork = rules(ACTIVATION, POST_FORK_WINDOW);
            // Opened under rules the migration skips, so the walk does not start on its own and the test drives it.
            let db = create_custom_blockchain_at_path(&path, rules_unscheduled(), true);

            // Drop the tip *block* but keep its header, which is exactly the shape header sync leaves behind.
            {
                let access = db.db_read_access().unwrap();
                let tip = access.fetch_chain_header_by_height(CHAIN_LEN).unwrap();
                let new_tip = access.fetch_chain_header_by_height(CHAIN_LEN - 1).unwrap();
                let expected_best = *access.fetch_chain_metadata().unwrap().best_block_hash();
                drop(access);
                let mut txn = DbTransaction::new();
                txn.delete_tip_block(*tip.hash());
                txn.set_best_block(
                    new_tip.height(),
                    new_tip.accumulated_data().hash,
                    new_tip.accumulated_data().total_accumulated_difficulty,
                    expected_best,
                    new_tip.timestamp(),
                );
                db.write(txn).unwrap();
            }
            assert_eq!(db.get_chain_metadata().unwrap().best_block_height(), CHAIN_LEN - 1);
            assert_eq!(
                db.db_read_access().unwrap().fetch_last_header().unwrap().height,
                CHAIN_LEN,
                "the header must survive so that this really is a header-only height"
            );

            let difficulty_calculator = DifficultyCalculator::new(post_fork.clone(), RandomXFactory::new(1));
            let validator = HeaderFullValidator::new(post_fork.clone(), difficulty_calculator.clone());
            let step = process_accumulated_data_for_height(
                db.db.clone(),
                difficulty_calculator,
                &validator,
                &post_fork,
                CHAIN_LEN,
                &post_fork.consensus_constants(CHAIN_LEN).clone(),
                Some(ACTIVATION),
            )
            .unwrap();

            match step {
                RebuildStep::Processed(status) => assert_eq!(
                    status,
                    AccumulatedDataRebuildStatus {
                        is_rebuilt: true,
                        last_rebuild_height: Some(CHAIN_LEN),
                    },
                    "the walk must repair the header-only height and finish at the header tip"
                ),
                other => panic!("expected the header-only height to be repaired, got {other:?}"),
            }
            // Repairing it rewrites it; it does not remove it, and it does not move the block tip.
            assert_eq!(
                db.db_read_access().unwrap().fetch_last_header().unwrap().height,
                CHAIN_LEN,
                "repairing a header-only height must not delete it"
            );
            assert_eq!(db.get_chain_metadata().unwrap().best_block_height(), CHAIN_LEN - 1);
            assert!(
                divergences_from_stored(&db, &post_fork, CHAIN_LEN).is_empty(),
                "the header-only height's target must have been recomputed under the post-fork rules"
            );
        }

        /// The orphan cleanup after a rewind may not be able to fail part way, so it may not probe first.
        ///
        /// `remove_orphan_chain_tip` fails the whole transaction on a hash that is not a tip, and the obvious
        /// guard - `fetch_orphan_chain_tip_by_hash` - returns `Err(ValueNotFound)` rather than `Ok(None)` for a
        /// tip entry whose orphan block is missing, which is the very case the guard was there for.
        #[tokio::test]
        async fn removing_an_orphan_chain_tip_that_is_not_there_is_not_an_error() {
            let path = tari_test_utils::paths::create_temporary_data_path();
            build_pre_fork_chain(&path, None);
            let db = create_custom_blockchain_at_path(&path, rules_unscheduled(), true);

            let mut txn = DbTransaction::new();
            txn.remove_orphan_chain_tip(FixedHash::zero());
            assert!(
                db.write(txn).is_err(),
                "the strict variant must still fail, or the `if_exists` one would be pointless"
            );

            let mut txn = DbTransaction::new();
            txn.remove_orphan_chain_tip_if_exists(FixedHash::zero());
            db.write(txn).unwrap();
        }

        /// §6.6 - the three cases where there is nothing to repair.
        #[tokio::test]
        async fn the_migration_is_a_no_op_where_there_is_nothing_to_repair() {
            let path = tari_test_utils::paths::create_temporary_data_path();
            let (targets_before, _, _) = build_pre_fork_chain(&path, None);

            let cases: Vec<(&str, BaseNodeConsensusManager)> = vec![
                // Igor / Stagenet / NextNet: no entry ever turns the verifier on.
                ("unscheduled", rules_unscheduled()),
                // LocalNet: the base entry carries the rule, so nothing was mined under the old rules.
                ("active from genesis", rules_active_from_genesis()),
                // Esmeralda today: the chain has not reached the fork.
                ("tip below activation", rules(CHAIN_LEN + 50, POST_FORK_WINDOW)),
            ];

            // What `build_pre_fork_chain` left behind: a rebuild that ran to completion at the tip.
            let status_before = AccumulatedDataRebuildStatus {
                is_rebuilt: true,
                last_rebuild_height: Some(CHAIN_LEN),
            };

            for (name, case_rules) in cases {
                let db = create_custom_blockchain_at_path(&path, case_rules, false);
                assert_eq!(
                    rebuild_status(&db),
                    status_before,
                    "{name}: the migration must arm nothing and leave the status exactly as it found it"
                );
                assert_eq!(
                    db.get_chain_metadata().unwrap().best_block_height(),
                    CHAIN_LEN,
                    "{name}"
                );
                assert_eq!(stored_targets(&db), targets_before, "{name}: nothing may be rewritten");
                rewind_migration_version_for_test(db.db_read_access().unwrap().db(), PRE_MIGRATION_VERSION).unwrap();
            }

            // The control: with the fork actually scheduled and reached, the same database *is* armed.
            let db = create_custom_blockchain_at_path(&path, rules(ACTIVATION, POST_FORK_WINDOW), true);
            assert_eq!(rebuild_status(&db).last_rebuild_height, Some(ACTIVATION - 1));
            assert!(!rebuild_status(&db).is_rebuilt);
            await_rebuild(&db).await;
        }

        /// §6.7 - orphans at or above the fork are purged, orphans below it are left alone.
        ///
        /// A stale orphan tip carries a `total_accumulated_difficulty` built from pre-fork targets and can win an
        /// accumulated difficulty comparison it should lose, reorging the node onto a stale-target side chain the
        /// moment the main chain is repaired.
        #[tokio::test]
        async fn orphans_at_or_above_the_activation_height_are_purged() {
            let path = tari_test_utils::paths::create_temporary_data_path();
            let (_, _, chain) = build_pre_fork_chain(&path, None);

            let (below_hashes, above_hashes) = {
                // See the note in the v5 coexistence test: an armed migration would keep this environment open.
                let db = create_custom_blockchain_at_path(&path, rules_unscheduled(), false);

                // Forks off height 50 and off height 105, so the orphans themselves sit at 51/52 and 106/107.
                let (_, below) = create_orphan_chain(
                    &db,
                    &[("LO1->GB", 1, 120), ("LO2->LO1", 1, 120)][..],
                    chain.get("M50").unwrap().clone(),
                );
                let (_, above) = create_orphan_chain(
                    &db,
                    &[("HO1->GB", 1, 120), ("HO2->HO1", 1, 120)][..],
                    chain.get("M105").unwrap().clone(),
                );
                let below_hashes: Vec<_> = ["LO1", "LO2"].iter().map(|n| *below.get(*n).unwrap().hash()).collect();
                let above_hashes: Vec<_> = ["HO1", "HO2"].iter().map(|n| *above.get(*n).unwrap().hash()).collect();

                // `create_orphan_chain` chains the orphans but does not record either branch as a tip, and the tip
                // entry is the half that actually matters here - it is what carries the stale
                // `total_accumulated_difficulty` into a chain strength comparison.
                let mut txn = DbTransaction::new();
                for (name, chain) in [("LO2", &below), ("HO2", &above)] {
                    let block = chain.get(name).unwrap();
                    txn.insert_orphan_chain_tip(*block.hash(), block.accumulated_data().total_accumulated_difficulty);
                }
                db.write(txn).unwrap();

                let access = db.db_read_access().unwrap();
                for hash in below_hashes.iter().chain(above_hashes.iter()) {
                    assert!(access.contains(&DbKey::OrphanBlock(*hash)).unwrap());
                }
                assert!(
                    access
                        .fetch_orphan_chain_tip_by_hash(&below_hashes[1])
                        .unwrap()
                        .is_some()
                );
                assert!(
                    access
                        .fetch_orphan_chain_tip_by_hash(&above_hashes[1])
                        .unwrap()
                        .is_some()
                );
                drop(access);

                rewind_migration_version_for_test(db.db_read_access().unwrap().db(), PRE_MIGRATION_VERSION).unwrap();
                (below_hashes, above_hashes)
            };

            let db = create_custom_blockchain_at_path(&path, rules(ACTIVATION, POST_FORK_WINDOW), true);
            {
                let access = db.db_read_access().unwrap();
                for hash in &above_hashes {
                    assert!(
                        !access.contains(&DbKey::OrphanBlock(*hash)).unwrap(),
                        "an orphan at or above the fork must be purged"
                    );
                }
                for hash in &below_hashes {
                    assert!(
                        access.contains(&DbKey::OrphanBlock(*hash)).unwrap(),
                        "an orphan below the fork must be left alone"
                    );
                }
                // The stale tip is gone and the pre-fork one is untouched. `delete_orphan` promotes a parent when
                // it removes a tip, but both purged orphans are above the fork, so the promotion is itself purged
                // and the branch leaves no tip behind at all.
                assert!(
                    access
                        .fetch_orphan_chain_tip_by_hash(&above_hashes[1])
                        .unwrap()
                        .is_none(),
                    "the stale orphan chain tip above the fork must be purged"
                );
                for hash in &above_hashes {
                    assert!(access.fetch_orphan_chain_tip_by_hash(hash).unwrap().is_none());
                }
                assert!(
                    access
                        .fetch_orphan_chain_tip_by_hash(&below_hashes[1])
                        .unwrap()
                        .is_some(),
                    "the pre-fork orphan chain tip must be left alone"
                );
            }
            await_rebuild(&db).await;
        }
    }

    fn create_consensus_rules() -> BaseNodeConsensusManager {
        BaseNodeConsensusManager::builder(Network::LocalNet)
            .add_consensus_constants(
                ConsensusConstantsBuilder::new(Network::LocalNet)
                    .clear_proof_of_work()
                    // `BlockSpec` mines every block at a hardcoded difficulty (1 by default) regardless of the
                    // consensus target, so under TIP-RFC-MT-0004 the second consecutive Sha3x block would correctly
                    // be rejected for not clearing its 2x target. That is a property of the test harness, not of the
                    // reorg behaviour under test, so switch the backoff off here.
                    .with_pow_backoff_cap(POW_BACKOFF_DISABLED)
                    .add_proof_of_work(PowAlgorithm::Sha3x, PowAlgorithmConstants {
                        min_difficulty: Difficulty::min(),
                        max_difficulty: Difficulty::from_u64(100).expect("valid difficulty"),
                        target_time: 120,
                    })
                    .build(),
            )
            .build()
            .unwrap()
    }
}
