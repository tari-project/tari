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
    blocks::{Block, BlockHeader, NewBlockTemplate},
    chain_storage::{
        accumulated_data::{BlockAccumulatedData, BlockHeaderAccumulatedData},
        consts::{
            BLOCKCHAIN_DATABASE_ORPHAN_STORAGE_CAPACITY,
            BLOCKCHAIN_DATABASE_PRUNED_MODE_PRUNING_INTERVAL,
            BLOCKCHAIN_DATABASE_PRUNING_HORIZON,
        },
        db_transaction::{DbKey, DbTransaction, DbValue},
        error::ChainStorageError,
        pruned_output::PrunedOutput,
        BlockchainBackend,
        ChainBlock,
        ChainHeader,
        HistoricalBlock,
        HorizonData,
        MmrTree,
        OrNotFound,
        TargetDifficulties,
    },
    common::rolling_vec::RollingVec,
    consensus::{chain_strength_comparer::ChainStrengthComparer, ConsensusManager},
    proof_of_work::{monero_rx::MoneroData, PowAlgorithm, TargetDifficultyWindow},
    tari_utilities::epoch_time::EpochTime,
    transactions::{
        transaction::{TransactionKernel, TransactionOutput},
        types::{Commitment, HashDigest, HashOutput, Signature},
    },
    validation::{HeaderValidation, OrphanValidation, PostOrphanBodyValidation, ValidationError},
};
use croaring::Bitmap;
use log::*;
use std::{
    cmp,
    cmp::Ordering,
    collections::VecDeque,
    mem,
    ops::Bound,
    sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard},
    time::Instant,
};
use strum_macros::Display;
use tari_common_types::{chain_metadata::ChainMetadata, types::BlockHash};
use tari_crypto::tari_utilities::{hex::Hex, Hashable};
use tari_mmr::{MerkleMountainRange, MutableMmr};
use uint::static_assertions::_core::ops::RangeBounds;

const LOG_TARGET: &str = "c::cs::database";

/// Configuration for the BlockchainDatabase.
#[derive(Clone, Copy)]
pub struct BlockchainDatabaseConfig {
    pub orphan_storage_capacity: usize,
    pub pruning_horizon: u64,
    pub pruning_interval: u64,
}

impl Default for BlockchainDatabaseConfig {
    fn default() -> Self {
        Self {
            orphan_storage_capacity: BLOCKCHAIN_DATABASE_ORPHAN_STORAGE_CAPACITY,
            pruning_horizon: BLOCKCHAIN_DATABASE_PRUNING_HORIZON,
            pruning_interval: BLOCKCHAIN_DATABASE_PRUNED_MODE_PRUNING_INTERVAL,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Display)]
pub enum BlockAddResult {
    Ok(Arc<ChainBlock>),
    BlockExists,
    OrphanBlock,
    /// Indicates the new block caused a chain reorg. This contains removed blocks followed by added blocks.
    ChainReorg(Vec<Arc<ChainBlock>>, Vec<Arc<ChainBlock>>),
}

impl BlockAddResult {
    pub fn is_added(&self) -> bool {
        matches!(self, BlockAddResult::Ok(_))
    }

    pub fn assert_added(&self) -> ChainBlock {
        match self {
            BlockAddResult::ChainReorg(added, removed) => panic!(
                "Expected added result, but was reorg ({} added, {} removed)",
                added.len(),
                removed.len()
            ),
            BlockAddResult::Ok(b) => b.as_ref().clone(),
            BlockAddResult::BlockExists => panic!("Expected added result, but was BlockExists"),
            BlockAddResult::OrphanBlock => panic!("Expected added result, but was OrphanBlock"),
        }
    }

    pub fn assert_orphaned(&self) {
        match self {
            BlockAddResult::OrphanBlock => (),
            _ => panic!("Result was not orphaned"),
        }
    }

    pub fn assert_reorg(&self, num_added: usize, num_removed: usize) {
        match self {
            BlockAddResult::ChainReorg(a, r) => {
                assert_eq!(num_added, a.len(), "Number of added reorged blocks was different");
                assert_eq!(num_removed, r.len(), "Number of removed reorged blocks was different");
            },
            BlockAddResult::Ok(_) => panic!("Expected reorg result, but was Ok()"),
            BlockAddResult::BlockExists => panic!("Expected reorg result, but was BlockExists"),
            BlockAddResult::OrphanBlock => panic!("Expected reorg result, but was OrphanBlock"),
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
    pub block: Arc<Box<dyn PostOrphanBodyValidation<B>>>,
    pub header: Arc<Box<dyn HeaderValidation<B>>>,
    pub orphan: Arc<Box<dyn OrphanValidation>>,
}

impl<B: BlockchainBackend> Validators<B> {
    pub fn new(
        block: impl PostOrphanBodyValidation<B> + 'static,
        header: impl HeaderValidation<B> + 'static,
        orphan: impl OrphanValidation + 'static,
    ) -> Self
    {
        Self {
            block: Arc::new(Box::new(block)),
            header: Arc::new(Box::new(header)),
            orphan: Arc::new(Box::new(orphan)),
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
    consensus_manager: ConsensusManager,
}

#[allow(clippy::ptr_arg)]
impl<B> BlockchainDatabase<B>
where B: BlockchainBackend
{
    /// Creates a new `BlockchainDatabase` using the provided backend.
    pub fn new(
        db: B,
        consensus_manager: &ConsensusManager,
        validators: Validators<B>,
        config: BlockchainDatabaseConfig,
        cleanup_orphans_at_startup: bool,
    ) -> Result<Self, ChainStorageError>
    {
        debug!(
            target: LOG_TARGET,
            "Initializing database pruning horizon={}", config.pruning_horizon
        );
        let is_empty = db.is_empty()?;
        let blockchain_db = BlockchainDatabase {
            db: Arc::new(RwLock::new(db)),
            validators,
            config,
            consensus_manager: consensus_manager.clone(),
        };
        if is_empty {
            info!(target: LOG_TARGET, "Blockchain db is empty. Adding genesis block.");
            let genesis_block = consensus_manager.get_genesis_block();
            blockchain_db.insert_block(Arc::new(genesis_block))?;
            blockchain_db.store_pruning_horizon(config.pruning_horizon)?;
        }
        if cleanup_orphans_at_startup {
            match blockchain_db.cleanup_all_orphans() {
                Ok(_) => info!(target: LOG_TARGET, "Orphan database cleaned out at startup.",),
                Err(e) => warn!(
                    target: LOG_TARGET,
                    "Orphan database could not be cleaned out at startup: ({}).", e
                ),
            }
        }

        let pruning_horizon = blockchain_db.get_chain_metadata()?.pruning_horizon();
        if config.pruning_horizon != pruning_horizon {
            debug!(
                target: LOG_TARGET,
                "Updating pruning horizon from {} to {}.", pruning_horizon, config.pruning_horizon,
            );
            blockchain_db.store_pruning_horizon(config.pruning_horizon)?;
        }
        Ok(blockchain_db)
    }

    pub fn create_chain_header_if_valid(
        &self,
        header: BlockHeader,
        prev: &ChainHeader,
    ) -> Result<ChainHeader, ChainStorageError>
    {
        let db = self.db_read_access()?;
        // let prev  = db.fetch_chain_header_in_all_chains(&header.prev_hash)?.ok_or_else(||
        // ChainStorageError::ValueNotFound {     entity: "Header".to_string(),
        //     field: "hash".to_string(),
        //     value:  header.prev_hash.to_hex()
        // })?;
        let accum_data = self.validators.header.validate(&*db, &header, &prev.accumulated_data)?;
        Ok(ChainHeader {
            header,
            accumulated_data: accum_data.build()?,
        })
    }

    // Be careful about making this method public. Rather use `db_and_metadata_read_access`
    // so that metadata and db are read in the correct order so that deadlocks don't occur
    pub fn db_read_access(&self) -> Result<RwLockReadGuard<B>, ChainStorageError> {
        self.db.read().map_err(|e| {
            error!(
                target: LOG_TARGET,
                "An attempt to get a read lock on the blockchain backend failed. {:?}", e
            );
            ChainStorageError::AccessError("Read lock on blockchain backend failed".into())
        })
    }

    #[cfg(test)]
    pub fn test_db_write_access(&self) -> Result<RwLockWriteGuard<B>, ChainStorageError> {
        self.db.write().map_err(|e| {
            error!(
                target: LOG_TARGET,
                "An attempt to get a write lock on the blockchain backend failed. {:?}", e
            );
            ChainStorageError::AccessError("Write lock on blockchain backend failed".into())
        })
    }

    fn db_write_access(&self) -> Result<RwLockWriteGuard<B>, ChainStorageError> {
        self.db.write().map_err(|e| {
            error!(
                target: LOG_TARGET,
                "An attempt to get a write lock on the blockchain backend failed. {:?}", e
            );
            ChainStorageError::AccessError("Write lock on blockchain backend failed".into())
        })
    }

    pub fn write(&self, transaction: DbTransaction) -> Result<(), ChainStorageError> {
        let mut db = self.db_write_access()?;
        db.write(transaction)
    }

    /// Returns the height of the current longest chain. This method will only fail if there's a fairly serious
    /// synchronisation problem on the database. You can try calling [BlockchainDatabase::try_recover_metadata] in
    /// that case to re-sync the metadata; or else just exit the program.
    ///
    /// If the chain is empty (the genesis block hasn't been added yet), this function returns `None`
    pub fn get_height(&self) -> Result<u64, ChainStorageError> {
        let db = self.db_read_access()?;
        Ok(db.fetch_chain_metadata()?.height_of_longest_chain())
    }

    /// Return the geometric mean of the proof of work of the longest chain.
    /// The proof of work is returned as the geometric mean of all difficulties
    pub fn get_accumulated_difficulty(&self) -> Result<u128, ChainStorageError> {
        let db = self.db_read_access()?;
        Ok(db.fetch_chain_metadata()?.accumulated_difficulty())
    }

    /// Returns a copy of the current blockchain database metadata
    pub fn get_chain_metadata(&self) -> Result<ChainMetadata, ChainStorageError> {
        let db = self.db_read_access()?;
        db.fetch_chain_metadata()
    }

    // Fetch the utxo
    pub fn fetch_utxo(&self, hash: HashOutput) -> Result<Option<TransactionOutput>, ChainStorageError> {
        let db = self.db_read_access()?;
        Ok(db.fetch_output(&hash)?.map(|(out, _index)| out))
    }

    /// Return a list of matching utxos, with each being `None` if not found. If found, the transaction
    /// output, and a boolean indicating if the UTXO was spent as of the block hash specified or the tip if not
    /// specified.
    pub fn fetch_utxos(
        &self,
        hashes: Vec<HashOutput>,
        is_spent_as_of: Option<HashOutput>,
    ) -> Result<Vec<Option<(TransactionOutput, bool)>>, ChainStorageError>
    {
        let db = self.db_read_access()?;
        let is_spent_as_of = match is_spent_as_of {
            Some(hash) => hash,
            None => db.fetch_chain_metadata()?.best_block().clone(),
        };
        let data =
            db.fetch_block_accumulated_data(&is_spent_as_of)?
                .ok_or_else(|| ChainStorageError::ValueNotFound {
                    entity: "BlockAccumulatedData".to_string(),
                    field: "header_hash".to_string(),
                    value: is_spent_as_of.to_hex(),
                })?;
        let mut result = vec![];
        for hash in hashes {
            let output = db.fetch_output(&hash)?;
            result.push(output.map(|(out, mmr_index)| (out, data.deleted().contains(mmr_index))));
        }
        Ok(result)
    }

    pub fn fetch_kernel_by_excess(
        &self,
        excess: &[u8],
    ) -> Result<Option<(TransactionKernel, HashOutput)>, ChainStorageError>
    {
        let db = self.db_read_access()?;
        db.fetch_kernel_by_excess(excess)
    }

    pub fn fetch_kernel_by_excess_sig(
        &self,
        excess_sig: Signature,
    ) -> Result<Option<(TransactionKernel, HashOutput)>, ChainStorageError>
    {
        let db = self.db_read_access()?;
        db.fetch_kernel_by_excess_sig(&excess_sig)
    }

    pub fn fetch_kernels_by_mmr_position(
        &self,
        start: u64,
        end: u64,
    ) -> Result<Vec<TransactionKernel>, ChainStorageError>
    {
        let db = self.db_read_access()?;
        db.fetch_kernels_by_mmr_position(start, end)
    }

    pub fn fetch_utxos_by_mmr_position(
        &self,
        start: u64,
        end: u64,
        end_header_hash: HashOutput,
    ) -> Result<(Vec<PrunedOutput>, Vec<Bitmap>), ChainStorageError>
    {
        let db = self.db_read_access()?;
        let accum_data = db.fetch_block_accumulated_data(&end_header_hash).or_not_found(
            "BlockAccumulatedData",
            "hash",
            end_header_hash.to_hex(),
        )?;
        db.fetch_utxos_by_mmr_position(start, end, accum_data.deleted())
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
    pub fn fetch_header_and_accumulated_data(
        &self,
        height: u64,
    ) -> Result<(BlockHeader, BlockHeaderAccumulatedData), ChainStorageError>
    {
        let db = self.db_read_access()?;
        db.fetch_header_and_accumulated_data(height)
    }

    /// Returns the block header at the given block height.
    pub fn fetch_chain_header(&self, height: u64) -> Result<ChainHeader, ChainStorageError> {
        let db = self.db_read_access()?;
        let (header, accumulated_data) = db.fetch_header_and_accumulated_data(height)?;
        Ok(ChainHeader {
            header,
            accumulated_data,
        })
    }

    pub fn fetch_header_containing_kernel_mmr(&self, mmr_position: u64) -> Result<ChainHeader, ChainStorageError> {
        let db = self.db_read_access()?;
        db.fetch_header_containing_kernel_mmr(mmr_position)
    }

    pub fn fetch_header_containing_utxo_mmr(&self, mmr_position: u64) -> Result<ChainHeader, ChainStorageError> {
        let db = self.db_read_access()?;
        db.fetch_header_containing_utxo_mmr(mmr_position)
    }

    /// Find the first matching header in a list of block hashes, returning the index of the match and the BlockHeader.
    /// Or None if not found.
    pub fn find_headers_after_hash<I: IntoIterator<Item = HashOutput>>(
        &self,
        ordered_hashes: I,
        count: u64,
    ) -> Result<Option<(usize, Vec<BlockHeader>)>, ChainStorageError>
    {
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
                    let headers = fetch_headers(&*db, header.height + 1, end_height)?;
                    return Ok(Some((i, headers)));
                },
                None => continue,
            };
        }
        Ok(None)
    }

    pub fn fetch_block_timestamps(&self, start_hash: HashOutput) -> Result<RollingVec<EpochTime>, ChainStorageError> {
        let start_header =
            self.fetch_header_by_block_hash(start_hash.clone())?
                .ok_or_else(|| ChainStorageError::ValueNotFound {
                    entity: "BlockHeader".to_string(),
                    field: "start_hash".to_string(),
                    value: start_hash.to_hex(),
                })?;
        let constants = self.consensus_manager.consensus_constants(start_header.height);
        let timestamp_window = constants.get_median_timestamp_count();
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
    ) -> Result<Option<BlockHeaderAccumulatedData>, ChainStorageError>
    {
        let db = self.db_read_access()?;
        db.fetch_header_accumulated_data(&hash)
    }

    /// Store the provided headers. This function does not do any validation and assumes the inserted header has already
    /// been validated.
    pub fn insert_valid_headers(
        &self,
        headers: Vec<(BlockHeader, BlockHeaderAccumulatedData)>,
    ) -> Result<(), ChainStorageError>
    {
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

    /// Returns the block header corresponding to the provided BlockHash
    pub fn fetch_header_by_block_hash(&self, hash: HashOutput) -> Result<Option<BlockHeader>, ChainStorageError> {
        let db = self.db_read_access()?;
        fetch_header_by_block_hash(&*db, hash)
    }

    /// Returns a connected header in the main chain by block hash
    pub fn fetch_chain_header_by_block_hash(&self, hash: HashOutput) -> Result<Option<ChainHeader>, ChainStorageError> {
        let db = self.db_read_access()?;

        if let Some(header) = fetch_header_by_block_hash(&*db, hash.clone())? {
            let accumulated_data =
                db.fetch_header_accumulated_data(&hash)?
                    .ok_or_else(|| ChainStorageError::ValueNotFound {
                        entity: "BlockHeaderAccumulatedData".to_string(),
                        field: "hash".to_string(),
                        value: hash.to_hex(),
                    })?;
            Ok(Some(ChainHeader {
                header,
                accumulated_data,
            }))
        } else {
            Ok(None)
        }
    }

    /// Returns the header at the tip
    pub fn fetch_tip_header(&self) -> Result<ChainHeader, ChainStorageError> {
        let db = self.db_read_access()?;
        db.fetch_tip_header()
    }

    pub fn fetch_last_header(&self) -> Result<BlockHeader, ChainStorageError> {
        let db = self.db_read_access()?;
        db.fetch_last_header()
    }

    /// Returns the sum of all kernels
    pub fn fetch_kernel_commitment_sum(&self, at_hash: &HashOutput) -> Result<Commitment, ChainStorageError> {
        Ok(self.fetch_block_accumulated_data(at_hash.clone())?.kernel_sum)
    }

    /// Returns `n` hashes from height _h - offset_ where _h_ is the tip header height back to `h - n - offset`.
    pub fn fetch_block_hashes_from_header_tip(
        &self,
        n: usize,
        offset: usize,
    ) -> Result<Vec<HashOutput>, ChainStorageError>
    {
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
        let start = end_height.saturating_sub(n as u64 - 1);
        let headers = fetch_headers(&*db, start, end_height)?;
        Ok(headers.into_iter().map(|h| h.hash()).rev().collect())
    }

    pub fn fetch_block_accumulated_data(&self, at_hash: HashOutput) -> Result<BlockAccumulatedData, ChainStorageError> {
        let db = self.db_read_access()?;
        db.fetch_block_accumulated_data(&at_hash)?
            .ok_or_else(|| ChainStorageError::ValueNotFound {
                entity: "BlockAccumulatedData".to_string(),
                field: "at_hash".to_string(),
                value: at_hash.to_hex(),
            })
    }

    pub fn fetch_block_accumulated_data_by_height(
        &self,
        height: u64,
    ) -> Result<BlockAccumulatedData, ChainStorageError>
    {
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
    pub fn fetch_target_difficulty(
        &self,
        pow_algo: PowAlgorithm,
        height: u64,
    ) -> Result<TargetDifficultyWindow, ChainStorageError>
    {
        let db = self.db_read_access()?;
        fetch_target_difficulty(&*db, &self.consensus_manager, pow_algo, height)
    }

    pub fn fetch_target_difficulties(&self, start_hash: HashOutput) -> Result<TargetDifficulties, ChainStorageError> {
        let db = self.db_read_access()?;
        let start_header =
            fetch_header_by_block_hash(&*db, start_hash.clone())?.ok_or_else(|| ChainStorageError::ValueNotFound {
                entity: "fetch_target_difficulties".to_string(),
                field: "start_hash".to_string(),
                value: start_hash.to_hex(),
            })?;
        let start_height = start_header.height;
        let mut targets = TargetDifficulties::new(&self.consensus_manager, start_height);
        let accum_data =
            db.fetch_header_accumulated_data(&start_hash)?
                .ok_or_else(|| ChainStorageError::ValueNotFound {
                    entity: "BlockHeaderAccumulatedData".to_string(),
                    field: "hash".to_string(),
                    value: start_hash.to_hex(),
                })?;
        // Add start header since we have it on hand
        targets.add_front(&start_header, accum_data.target_difficulty);

        for h in (0..start_height).rev() {
            // TODO: this can be optimized by retrieving the accumulated data and header at the same time, or even
            // better by retrieving only the epoch and target difficulty in the same lmdb transaction
            let header = fetch_header(&*db, h)?;
            if !targets.is_algo_full(header.pow_algo()) {
                let accum_data = db.fetch_header_accumulated_data(&header.hash())?.ok_or_else(|| {
                    ChainStorageError::ValueNotFound {
                        entity: "BlockHeaderAccumulatedData".to_string(),
                        field: "hash".to_string(),
                        value: header.hash().to_hex(),
                    }
                })?;

                targets.add_front(&header, accum_data.target_difficulty);
            }
            if targets.is_full() {
                break;
            }
        }

        Ok(targets)
    }

    pub fn prepare_block_merkle_roots(&self, template: NewBlockTemplate) -> Result<Block, ChainStorageError> {
        let NewBlockTemplate { header, mut body, .. } = template;
        body.sort();
        let header = BlockHeader::from(header);
        let mut block = Block { header, body };
        let roots = self.calculate_mmr_roots(&block)?;
        block.header.kernel_mr = roots.kernel_mr;
        block.header.kernel_mmr_size = roots.kernel_mmr_size;
        block.header.output_mr = roots.output_mr;
        block.header.range_proof_mr = roots.range_proof_mr;
        block.header.output_mmr_size = roots.output_mmr_size;
        Ok(block)
    }

    /// `calculate_mmr_roots` takes a _pre-sorted_ block body and calculates the MMR roots for it.
    ///
    /// ## Panic
    /// This function will panic if the block body is not sorted
    pub fn calculate_mmr_roots(&self, block: &Block) -> Result<MmrRoots, ChainStorageError> {
        let db = self.db_read_access()?;
        assert!(
            block.body.is_sorted(),
            "calculate_mmr_roots expected a sorted block body, however the block body was not sorted"
        );
        calculate_mmr_roots(&*db, &block)
    }

    /// Fetches the total merkle mountain range node count up to the specified height.
    pub fn fetch_mmr_size(&self, tree: MmrTree) -> Result<u64, ChainStorageError> {
        let db = self.db_read_access()?;
        db.fetch_mmr_size(tree)
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
    pub fn add_block(&self, block: Arc<Block>) -> Result<BlockAddResult, ChainStorageError> {
        let new_height = block.header.height;
        // Perform orphan block validation.
        if let Err(e) = self.validators.orphan.validate(&block) {
            warn!(
                target: LOG_TARGET,
                "Block #{} ({}) failed validation - {}",
                &new_height,
                block.hash().to_hex(),
                e.to_string()
            );
            return Err(e.into());
        }

        trace!(
            target: LOG_TARGET,
            "[add_block] acquired write access db lock for block #{} ",
            &new_height
        );
        let mut db = self.db_write_access()?;
        let block_add_result = add_block(
            &mut *db,
            &**self.validators.block,
            &**self.validators.header,
            &*self.consensus_manager.chain_strength_comparer(),
            block,
        )?;

        // Cleanup of backend when in pruned mode.
        match block_add_result {
            BlockAddResult::Ok(_) | BlockAddResult::ChainReorg(_, _) => {
                prune_database_if_needed(&mut *db, self.config.pruning_interval, self.config.pruning_horizon)?
            },
            _ => {},
        }

        trace!(
            target: LOG_TARGET,
            "[add_block] released write access db lock for block #{} ",
            &new_height
        );
        Ok(block_add_result)
    }

    /// Clean out the entire orphan pool
    pub fn cleanup_orphans(&self) -> Result<(), ChainStorageError> {
        let mut db = self.db_write_access()?;
        let _ = cleanup_orphans(&mut *db, self.config.orphan_storage_capacity)?;
        Ok(())
    }

    /// Clean out the entire orphan pool
    pub fn cleanup_all_orphans(&self) -> Result<(), ChainStorageError> {
        let mut db = self.db_write_access()?;
        let _ = cleanup_orphans(&mut *db, 0)?;
        Ok(())
    }

    fn insert_block(&self, block: Arc<ChainBlock>) -> Result<(), ChainStorageError> {
        let mut db = self.db_write_access()?;
        let mut txn = DbTransaction::new();
        insert_block(&mut txn, block)?;
        db.write(txn)
    }

    fn store_pruning_horizon(&self, pruning_horizon: u64) -> Result<(), ChainStorageError> {
        let mut db = self.db_write_access()?;
        store_pruning_horizon(&mut *db, pruning_horizon)
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
    pub fn fetch_block(&self, height: u64) -> Result<HistoricalBlock, ChainStorageError> {
        let db = self.db_read_access()?;
        fetch_block(&*db, height)
    }

    /// Returns the set of blocks according to the bounds
    pub fn fetch_blocks<T: RangeBounds<u64>>(&self, bounds: T) -> Result<Vec<HistoricalBlock>, ChainStorageError> {
        let db = self.db_read_access()?;
        let (mut start, mut end) = convert_to_option_bounds(bounds);

        let metadata = db.fetch_chain_metadata()?;

        if start.is_none() {
            // `(..n)` means fetch blocks with the lowest height possible until `n`
            start = Some(metadata.pruned_height());
        }
        if end.is_none() {
            // `(n..)` means fetch blocks until this node's tip
            end = Some(metadata.height_of_longest_chain());
        }

        let (start, end) = (start.unwrap(), end.unwrap());

        if end > metadata.height_of_longest_chain() {
            return Err(ChainStorageError::ValueNotFound {
                entity: "Block".to_string(),
                field: "end height".to_string(),
                value: end.to_string(),
            });
        }

        debug!(target: LOG_TARGET, "Fetching blocks {}-{}", start, end);
        let blocks = fetch_blocks(&*db, start, end)?;
        debug!(target: LOG_TARGET, "Fetched {} block(s)", blocks.len());

        Ok(blocks)
    }

    /// Attempt to fetch the block corresponding to the provided hash from the main chain, if it cannot be found then
    /// the block will be searched in the orphan block pool.
    pub fn fetch_block_by_hash(&self, hash: BlockHash) -> Result<Option<HistoricalBlock>, ChainStorageError> {
        let db = self.db_read_access()?;
        fetch_block_by_hash(&*db, hash)
    }

    /// Attempt to fetch the block corresponding to the provided kernel hash from the main chain, if the block is past
    /// pruning horizon, it will return Ok<None>
    pub fn fetch_block_with_kernel(&self, excess_sig: Signature) -> Result<Option<HistoricalBlock>, ChainStorageError> {
        let db = self.db_read_access()?;
        fetch_block_with_kernel(&*db, excess_sig)
    }

    /// Attempt to fetch the block corresponding to the provided stxo hash from the main chain, if the block is past
    /// pruning horizon, it will return Ok<None>
    pub fn fetch_block_with_stxo(&self, commitment: Commitment) -> Result<Option<HistoricalBlock>, ChainStorageError> {
        let db = self.db_read_access()?;
        fetch_block_with_stxo(&*db, commitment)
    }

    /// Attempt to fetch the block corresponding to the provided utxo hash from the main chain, if the block is past
    /// pruning horizon, it will return Ok<None>
    pub fn fetch_block_with_utxo(&self, commitment: Commitment) -> Result<Option<HistoricalBlock>, ChainStorageError> {
        let db = self.db_read_access()?;
        fetch_block_with_utxo(&*db, commitment)
    }

    /// Returns true if this block exists in the chain, or is orphaned.
    pub fn block_exists(&self, hash: BlockHash) -> Result<bool, ChainStorageError> {
        let db = self.db_read_access()?;
        Ok(db.contains(&DbKey::BlockHash(hash.clone()))? || db.contains(&DbKey::OrphanBlock(hash))?)
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
    /// * The block height is before the horizon block height determined by the pruning horizon
    pub fn rewind_to_height(&self, height: u64) -> Result<Vec<Arc<ChainBlock>>, ChainStorageError> {
        let mut db = self.db_write_access()?;
        rewind_to_height(&mut *db, height)
    }

    pub fn fetch_horizon_data(&self) -> Result<Option<HorizonData>, ChainStorageError> {
        let db = self.db_read_access()?;
        db.fetch_horizon_data()
    }
}

fn unexpected_result<T>(req: DbKey, res: DbValue) -> Result<T, ChainStorageError> {
    let msg = format!("Unexpected result for database query {}. Response: {}", req, res);
    error!(target: LOG_TARGET, "{}", msg);
    Err(ChainStorageError::UnexpectedResult(msg))
}

/// Container struct for MMR roots
#[derive(Debug, Clone)]
pub struct MmrRoots {
    pub kernel_mr: BlockHash,
    pub kernel_mmr_size: u64,
    pub output_mr: BlockHash,
    pub range_proof_mr: BlockHash,
    pub output_mmr_size: u64,
}

pub fn calculate_mmr_roots<T: BlockchainBackend>(db: &T, block: &Block) -> Result<MmrRoots, ChainStorageError> {
    let header = &block.header;
    let body = &block.body;

    let BlockAccumulatedData {
        kernels,
        outputs,
        range_proofs,
        deleted,
        ..
    } = db
        .fetch_block_accumulated_data(&header.prev_hash)?
        .ok_or_else(|| ChainStorageError::ValueNotFound {
            entity: "BlockAccumulatedData".to_string(),
            field: "header_hash".to_string(),
            value: header.prev_hash.to_hex(),
        })?;

    let deleted = deleted.deleted;
    let mut kernel_mmr = MerkleMountainRange::<HashDigest, _>::new(kernels);
    let mut output_mmr = MutableMmr::<HashDigest, _>::new(outputs, deleted)?;
    let mut proof_mmr = MerkleMountainRange::<HashDigest, _>::new(range_proofs);

    for kernel in body.kernels().iter() {
        kernel_mmr.push(kernel.hash())?;
    }

    for output in body.outputs().iter() {
        output_mmr.push(output.hash())?;
        proof_mmr.push(output.proof().hash())?;
    }

    for input in body.inputs().iter() {
        let index =
            db.fetch_mmr_leaf_index(MmrTree::Utxo, &input.hash())?
                .ok_or_else(|| ChainStorageError::ValueNotFound {
                    entity: "UTXO".to_string(),
                    field: "hash".to_string(),
                    value: input.hash().to_hex(),
                })?;

        if !output_mmr.delete(index) {
            let len = output_mmr.len();
            return Err(ChainStorageError::InvalidOperation(format!(
                "Could not delete index {} from the output MMR (length is {})",
                index, len
            )));
        }
    }

    output_mmr.compress();

    let mmr_roots = MmrRoots {
        kernel_mr: kernel_mmr.get_merkle_root()?,
        kernel_mmr_size: kernel_mmr.get_leaf_count()? as u64,
        output_mr: output_mmr.get_merkle_root()?,
        output_mmr_size: proof_mmr.get_leaf_count()? as u64,
        range_proof_mr: proof_mmr.get_merkle_root()?,
    };
    Ok(mmr_roots)
}

pub fn fetch_header<T: BlockchainBackend>(db: &T, block_num: u64) -> Result<BlockHeader, ChainStorageError> {
    fetch!(db, block_num, BlockHeader)
}

pub fn fetch_headers<T: BlockchainBackend>(
    db: &T,
    mut start: u64,
    mut end_inclusive: u64,
) -> Result<Vec<BlockHeader>, ChainStorageError>
{
    let is_reversed = start > end_inclusive;

    if is_reversed {
        mem::swap(&mut end_inclusive, &mut start);
    }

    // Allow the headers to be returned in reverse order
    let mut headers = Vec::with_capacity((end_inclusive - start) as usize);
    for h in start..=end_inclusive {
        match db.fetch(&DbKey::BlockHeader(h))? {
            Some(DbValue::BlockHeader(header)) => {
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

fn insert_headers<T: BlockchainBackend>(
    db: &mut T,
    headers: Vec<(BlockHeader, BlockHeaderAccumulatedData)>,
) -> Result<(), ChainStorageError>
{
    let mut txn = DbTransaction::new();
    headers.into_iter().for_each(|(header, accum_data)| {
        txn.insert_header(header, accum_data);
    });
    db.write(txn)
}

fn fetch_header_by_block_hash<T: BlockchainBackend>(
    db: &T,
    hash: BlockHash,
) -> Result<Option<BlockHeader>, ChainStorageError>
{
    try_fetch!(db, hash, BlockHash)
}

fn fetch_orphan<T: BlockchainBackend>(db: &T, hash: BlockHash) -> Result<Block, ChainStorageError> {
    fetch!(db, hash, OrphanBlock)
}

fn add_block<T: BlockchainBackend>(
    db: &mut T,
    block_validator: &dyn PostOrphanBodyValidation<T>,
    header_validator: &dyn HeaderValidation<T>,
    chain_strength_comparer: &dyn ChainStrengthComparer,
    block: Arc<Block>,
) -> Result<BlockAddResult, ChainStorageError>
{
    let block_hash = block.hash();
    if db.contains(&DbKey::BlockHash(block_hash))? {
        return Ok(BlockAddResult::BlockExists);
    }
    handle_possible_reorg(db, block_validator, header_validator, chain_strength_comparer, block)
}

// Adds a new block onto the chain tip.
fn insert_block(txn: &mut DbTransaction, block: Arc<ChainBlock>) -> Result<(), ChainStorageError> {
    let block_hash = block.accumulated_data.hash.clone();
    debug!(
        target: LOG_TARGET,
        "Storing new block #{} `{}`",
        block.block.header.height,
        block_hash.to_hex()
    );
    if block.block.header.pow.pow_algo == PowAlgorithm::Monero {
        let monero_seed = MoneroData::from_header(&block.block.header)
            .map_err(|e| ValidationError::CustomError(e.to_string()))?
            .key;
        txn.insert_monero_seed_height(&monero_seed, block.block.header.height);
    }

    // Update metadata
    let accumulated_difficulty = block.accumulated_data.total_accumulated_difficulty;
    txn.set_best_block(block.block.header.height, block_hash, accumulated_difficulty)
        .insert_block(block);

    Ok(())
}

fn store_pruning_horizon<T: BlockchainBackend>(db: &mut T, pruning_horizon: u64) -> Result<(), ChainStorageError> {
    let mut txn = DbTransaction::new();
    txn.set_pruning_horizon(pruning_horizon);
    db.write(txn)
}

pub fn fetch_target_difficulty<T: BlockchainBackend>(
    db: &T,
    consensus_manager: &ConsensusManager,
    pow_algo: PowAlgorithm,
    height: u64,
) -> Result<TargetDifficultyWindow, ChainStorageError>
{
    let mut target_difficulties = consensus_manager.new_target_difficulty(pow_algo, height);
    for height in (0..height).rev() {
        // TODO: this can be optimized by retrieving the accumulated data and header at the same time, or even
        // better by retrieving only the epoch and target difficulty in the same lmdb transaction
        let header = fetch_header(&*db, height)?;

        if header.pow.pow_algo == pow_algo {
            let accum_data =
                db.fetch_header_accumulated_data(&header.hash())?
                    .ok_or_else(|| ChainStorageError::ValueNotFound {
                        entity: "BlockHeaderAccumulatedData".to_string(),
                        field: "hash".to_string(),
                        value: header.hash().to_hex(),
                    })?;

            target_difficulties.add_front(header.timestamp(), accum_data.target_difficulty);
            if target_difficulties.is_full() {
                break;
            }
        }
    }

    Ok(target_difficulties)
}

fn fetch_block<T: BlockchainBackend>(db: &T, height: u64) -> Result<HistoricalBlock, ChainStorageError> {
    let mark = Instant::now();
    let (tip_height, is_pruned) = check_for_valid_height(&*db, height)?;
    let (header, accumulated_data) = db.fetch_header_and_accumulated_data(height)?;
    let kernels = db.fetch_kernels_in_block(&accumulated_data.hash)?;
    let outputs = db.fetch_outputs_in_block(&accumulated_data.hash)?;
    let inputs = db.fetch_inputs_in_block(&accumulated_data.hash)?;
    let mut unpruned = vec![];
    let mut pruned = vec![];
    for output in outputs {
        match output {
            PrunedOutput::Pruned {
                output_hash,
                range_proof_hash,
            } => {
                pruned.push((output_hash, range_proof_hash));
            },
            PrunedOutput::NotPruned { output } => unpruned.push(output),
        }
    }

    let mut pruned_input_count = 0;

    if is_pruned {
        let mut deleted = db
            .fetch_block_accumulated_data_by_height(height)
            .or_not_found("BlockAccumulatedData", "height", height.to_string())?
            .deleted()
            .clone();
        if height > 0 {
            let prev = db
                .fetch_block_accumulated_data_by_height(height - 1)
                .or_not_found("BlockAccumulatedData", "height", (height - 1).to_string())?
                .deleted()
                .clone();
            deleted -= prev;
        }

        pruned_input_count = deleted.cardinality();
    }

    let block = header
        .into_builder()
        .add_inputs(inputs)
        .add_outputs(unpruned)
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
        tip_height - height + 1,
        accumulated_data,
        pruned,
        pruned_input_count,
    ))
}

fn fetch_blocks<T: BlockchainBackend>(
    db: &T,
    start: u64,
    end_inclusive: u64,
) -> Result<Vec<HistoricalBlock>, ChainStorageError>
{
    (start..=end_inclusive).map(|i| fetch_block(db, i)).collect()
}

fn fetch_block_with_kernel<T: BlockchainBackend>(
    _db: &T,
    _excess_sig: Signature,
) -> Result<Option<HistoricalBlock>, ChainStorageError>
{
    unimplemented!()
    // let metadata = db.fetch_chain_metadata()?;
    // let db_height = metadata.height_of_longest_chain.unwrap_or(0);
    // let horizon_height = metadata.horizon_block(db_height);
    // for i in (horizon_height..db_height).rev() {
    //     let kernel_cp = fetch_checkpoint(db, MmrTree::Kernel, i)?;
    //     let (kernel_hashes, _) = kernel_cp.into_parts();
    //     let kernels = fetch_kernels(db, kernel_hashes)?;
    //     for kernel in kernels {
    //         if kernel.excess_sig == excess_sig {
    //             return Ok(Some(fetch_block(db, i)?));
    //         }
    //     }
    // }
    // // data is not in the pruning horizon, let's check behind that but only if there is a pruning horizon
    // if horizon_height > 0 {
    //     let kernel_cp = fetch_checkpoint(db, MmrTree::Kernel, horizon_height - 1)?;
    //     let (kernel_hashes, _) = kernel_cp.into_parts();
    //     let kernels = fetch_kernels(db, kernel_hashes)?;
    //     for kernel in kernels {
    //         if kernel.excess_sig == excess_sig {
    //             return Ok(None);
    //         }
    //     }
    // }
    // Err(ChainStorageError::ValueNotFound {
    //     entity: "Kernel".to_string(),
    //     field: "Excess sig".to_string(),
    //     value: excess_sig.get_signature().to_hex(),
    // })
}

fn fetch_block_with_utxo<T: BlockchainBackend>(
    _db: &T,
    _commitment: Commitment,
) -> Result<Option<HistoricalBlock>, ChainStorageError>
{
    unimplemented!()
    // let metadata = db.fetch_chain_metadata()?;
    // let db_height = metadata.height_of_longest_chain.unwrap_or(0);
    // let horizon_height = metadata.horizon_block(db_height);
    // for i in (horizon_height..db_height).rev() {
    //     let utxo_cp = fetch_checkpoint(db, MmrTree::Utxo, i)?;
    //     let (utxo_hashes, _) = utxo_cp.into_parts();
    //     let utxos = fetch_outputs(db, utxo_hashes)?;
    //     for utxo in utxos.0 {
    //         if utxo.commitment == commitment {
    //             return Ok(Some(fetch_block(db, i)?));
    //         }
    //     }
    //     for comm in utxos.1 {
    //         if comm == commitment {
    //             return Ok(Some(fetch_block(db, i)?));
    //         }
    //     }
    // }
    // // data is not in the pruning horizon, let's check behind that but only if there is a pruning horizon
    // if horizon_height > 0 {
    //     let utxo_cp = fetch_checkpoint(db, MmrTree::Utxo, horizon_height - 1)?;
    //     let (utxo_hashes, _) = utxo_cp.into_parts();
    //     let utxos = fetch_outputs(db, utxo_hashes)?;
    //     for utxo in utxos.0 {
    //         if utxo.commitment == commitment {
    //             return Ok(None);
    //         }
    //     }
    //     for comm in utxos.1 {
    //         if comm == commitment {
    //             return Ok(None);
    //         }
    //     }
    // }
    // Err(ChainStorageError::ValueNotFound {
    //     entity: "Utxo".to_string(),
    //     field: "Commitment".to_string(),
    //     value: commitment.to_hex(),
    // })
}

fn fetch_block_with_stxo<T: BlockchainBackend>(
    _db: &T,
    _commitment: Commitment,
) -> Result<Option<HistoricalBlock>, ChainStorageError>
{
    unimplemented!()
    // let metadata = db.fetch_chain_metadata()?;
    // let db_height = metadata.height_of_longest_chain.unwrap_or(0);
    // let horizon_height = metadata.horizon_block(db_height);
    // for i in (horizon_height..db_height).rev() {
    //     let utxo_cp = fetch_checkpoint(db, MmrTree::Utxo, i)?;
    //     let (_, deleted) = utxo_cp.into_parts();
    //     let inputs = fetch_inputs(db, deleted)?;
    //     for input in inputs {
    //         if input.commitment == commitment {
    //             return Ok(Some(fetch_block(db, i)?));
    //         }
    //     }
    // }
    // // data is not in the pruning horizon, we cannot check stxo's behind pruning horizon
    // Err(ChainStorageError::ValueNotFound {
    //     entity: "Utxo".to_string(),
    //     field: "Commitment".to_string(),
    //     value: commitment.to_hex(),
    // })
}

fn fetch_block_by_hash<T: BlockchainBackend>(
    db: &T,
    hash: BlockHash,
) -> Result<Option<HistoricalBlock>, ChainStorageError>
{
    if let Some(header) = fetch_header_by_block_hash(db, hash)? {
        return Ok(Some(fetch_block(db, header.height)?));
    }
    Ok(None)
}

fn check_for_valid_height<T: BlockchainBackend>(db: &T, height: u64) -> Result<(u64, bool), ChainStorageError> {
    let metadata = db.fetch_chain_metadata()?;
    let tip_height = metadata.height_of_longest_chain();
    if height > tip_height {
        return Err(ChainStorageError::InvalidQuery(format!(
            "Cannot get block at height {}. Chain tip is at {}",
            height, tip_height
        )));
    }
    let pruned_height = metadata.pruned_height();
    Ok((tip_height, height < pruned_height))
}

fn rewind_to_height<T: BlockchainBackend>(db: &mut T, height: u64) -> Result<Vec<Arc<ChainBlock>>, ChainStorageError> {
    let last_header = db.fetch_last_header()?;

    let mut txn = DbTransaction::new();

    // Delete headers
    let last_header_height = last_header.height;
    let metadata = db.fetch_chain_metadata()?;
    let last_block_height = metadata.height_of_longest_chain();
    let steps_back = last_header_height
        .checked_sub(cmp::max(last_block_height, height))
        .ok_or_else(|| {
            ChainStorageError::InvalidQuery(format!(
                "Cannot rewind to height ({}) that is greater than the tip header height {}.",
                cmp::max(height, last_block_height),
                last_header_height
            ))
        })?;

    debug!(
        target: LOG_TARGET,
        "Rewinding headers from height {} to {}",
        last_header_height,
        last_header_height - steps_back
    );
    (0..steps_back).for_each(|h| {
        txn.delete_header(last_header_height - h);
    });

    // Delete blocks

    let steps_back = last_block_height.saturating_sub(height);
    // No blocks to remove
    if steps_back == 0 {
        db.write(txn)?;
        return Ok(vec![]);
    }

    let mut removed_blocks = Vec::with_capacity(steps_back as usize);
    debug!(
        target: LOG_TARGET,
        "Rewinding blocks from height {} to {}",
        last_block_height,
        last_block_height - steps_back
    );
    for h in 0..steps_back {
        let block = fetch_block(db, last_block_height - h)?;
        if block.is_pruned() {
            unimplemented!("Rewinding past the pruning horizon not implemented.");
        }
        let block = Arc::new(block.clone().try_into_chain_block()?);
        txn.delete_block(block.block.hash());
        txn.delete_header(last_block_height - h);
        txn.insert_chained_orphan(block.clone());
        removed_blocks.push(block);
    }

    // Update metadata
    let (last_header, header_accumulated_data) = db.fetch_header_and_accumulated_data(height)?;

    txn.set_best_block(
        last_header.height,
        header_accumulated_data.hash.clone(),
        header_accumulated_data.total_accumulated_difficulty,
    );
    db.write(txn)?;

    Ok(removed_blocks)
}

// Checks whether we should add the block as an orphan. If it is the case, the orphan block is added and the chain
// is reorganised if necessary.
fn handle_possible_reorg<T: BlockchainBackend>(
    db: &mut T,
    block_validator: &dyn PostOrphanBodyValidation<T>,
    header_validator: &dyn HeaderValidation<T>,
    chain_strength_comparer: &dyn ChainStrengthComparer,
    new_block: Arc<Block>,
) -> Result<BlockAddResult, ChainStorageError>
{
    let db_height = db.fetch_chain_metadata()?.height_of_longest_chain();

    let new_tips = insert_orphan_and_find_new_tips(db, new_block.clone(), header_validator)?;
    debug!(
        target: LOG_TARGET,
        "Added candidate block #{} ({}) to the orphan database. Best height is {}. New tips found:{} ",
        new_block.header.height,
        new_block.hash().to_hex(),
        db_height,
        new_tips.len()
    );

    if new_tips.is_empty() {
        debug!(
            target: LOG_TARGET,
            "No reorg required, could not construct complete chain using block #{} ({}).",
            new_block.header.height,
            new_block.hash().to_hex()
        );
        return Ok(BlockAddResult::OrphanBlock);
    }

    let new_block_hash = new_block.hash();

    // Check the accumulated difficulty of the best fork chain compared to the main chain.
    let fork_header = find_strongest_orphan_tip(new_tips, chain_strength_comparer)?;
    if fork_header.is_none() {
        // This should never happen because a block is always added to the orphan pool before
        // checking, but just in case
        return Err(ChainStorageError::InvalidOperation(
            "No chain tips found in orphan pool".to_string(),
        ));
    }
    let fork_header = fork_header.unwrap();

    let tip_header = db.fetch_tip_header()?;
    if fork_header.accumulated_data.hash == new_block_hash {
        debug!(
            target: LOG_TARGET,
            "Comparing candidate block #{} (accum_diff:{}, hash:{}) to main chain #{} (accum_diff: {}, hash: ({})).",
            new_block.header.height,
            fork_header.accumulated_data.total_accumulated_difficulty,
            fork_header.accumulated_data.hash.to_hex(),
            tip_header.header.height,
            tip_header.accumulated_data.total_accumulated_difficulty,
            tip_header.accumulated_data.hash.to_hex()
        );
    } else {
        debug!(
            target: LOG_TARGET,
            "Comparing fork (accum_diff:{}, hash:{}) with block #{} ({}) to main chain #{} (accum_diff: {}, hash: \
             ({})).",
            fork_header.accumulated_data.total_accumulated_difficulty,
            fork_header.accumulated_data.hash.to_hex(),
            new_block.header.height,
            new_block_hash.to_hex(),
            tip_header.header.height,
            tip_header.accumulated_data.total_accumulated_difficulty,
            tip_header.accumulated_data.hash.to_hex()
        );
    }

    match chain_strength_comparer.compare(&fork_header, &tip_header) {
        Ordering::Greater => {
            debug!(
                target: LOG_TARGET,
                "Fork chain (accum_diff:{}, hash:{}) is stronger than the current tip.",
                fork_header.header.height,
                fork_header.accumulated_data.hash.to_hex()
            );
        },
        Ordering::Less | Ordering::Equal => {
            debug!(
                target: LOG_TARGET,
                "Fork chain (accum_diff:{}, hash:{}) with block {} ({}) has a weaker difficulty.",
                fork_header.accumulated_data.total_accumulated_difficulty,
                fork_header.accumulated_data.hash.to_hex(),
                new_block.header.height,
                new_block_hash.to_hex(),
            );
            debug!(
                target: LOG_TARGET,
                "Orphan block received: #{} ", new_block.header.height
            );
            return Ok(BlockAddResult::OrphanBlock);
        },
    }

    // TODO: We already have the first link in this chain, can be optimized to exclude it
    let reorg_chain = get_orphan_link_main_chain(db, &fork_header.accumulated_data.hash)?;
    // }
    let fork_height = reorg_chain
        .front()
        .expect("The new orphan block should be in the queue")
        .block
        .header
        .height -
        1;

    let num_added_blocks = reorg_chain.len();
    let removed_blocks = reorganize_chain(db, block_validator, fork_height, tip_header.height(), &reorg_chain)?;

    let num_removed_blocks = removed_blocks.len();

    // reorg is required when any blocks are removed or more than one are added
    // see https://github.com/tari-project/tari/issues/2101
    if num_removed_blocks > 0 || num_added_blocks > 1 {
        info!(
            target: LOG_TARGET,
            "Chain reorg required from {} to {} (accum_diff:{}, hash:{}) to (accum_diff:{}, hash:{}). Number of \
             blocks to remove: {}, to add: {}.
            ",
            tip_header.header,
            fork_header.header,
            tip_header.accumulated_data.total_accumulated_difficulty,
            tip_header.accumulated_data.hash.to_hex(),
            fork_header.accumulated_data.total_accumulated_difficulty,
            fork_header.accumulated_data.hash.to_hex(),
            num_removed_blocks,
            num_added_blocks,
        );
        Ok(BlockAddResult::ChainReorg(removed_blocks, reorg_chain.into()))
    } else {
        trace!(
            target: LOG_TARGET,
            "No reorg required. Number of blocks to remove: {}, to add: {}.",
            num_removed_blocks,
            num_added_blocks,
        );
        Ok(BlockAddResult::Ok(reorg_chain.front().unwrap().clone()))
    }
}

// Reorganize the main chain with the provided fork chain, starting at the specified height.
fn reorganize_chain<T: BlockchainBackend>(
    backend: &mut T,
    block_validator: &dyn PostOrphanBodyValidation<T>,
    height: u64,
    current_tip_height: u64,
    chain: &VecDeque<Arc<ChainBlock>>,
) -> Result<Vec<Arc<ChainBlock>>, ChainStorageError>
{
    let removed_blocks = if height <= current_tip_height {
        rewind_to_height(backend, height)?
    } else {
        vec![]
    };
    debug!(
        target: LOG_TARGET,
        "Validate and add {} chain block(s) from height {}. Rewound blocks: [{}]",
        chain.len(),
        height,
        removed_blocks
            .iter()
            .map(|b| b.block.header.height.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );

    for block in chain {
        let mut txn = DbTransaction::new();
        let block_hash_hex = block.accumulated_data.hash.to_hex();
        txn.delete(DbKey::OrphanBlock(block.accumulated_data.hash.clone()));
        if let Err(e) = block_validator.validate_body_for_valid_orphan(&block, backend) {
            warn!(
                target: LOG_TARGET,
                "Orphan block {} ({}) failed validation during chain reorg: {}",
                block.block.header.height,
                block_hash_hex,
                e
            );
            remove_orphan(backend, block.accumulated_data.hash.clone())?;

            info!(target: LOG_TARGET, "Restoring previous chain after failed reorg.");
            restore_reorged_chain(backend, height, removed_blocks)?;
            return Err(e.into());
        }

        insert_block(&mut txn, block.clone())?;
        // Failed to store the block - this should typically never happen unless there is a bug in the validator
        // (e.g. does not catch a double spend). In any case, we still need to restore the chain to a
        // good state before returning.
        if let Err(e) = backend.write(txn) {
            warn!(
                target: LOG_TARGET,
                "Failed to commit reorg chain: {}. Restoring last chain.", e
            );

            restore_reorged_chain(backend, height, removed_blocks)?;
            return Err(e);
        }
    }

    Ok(removed_blocks)
}

fn restore_reorged_chain<T: BlockchainBackend>(
    db: &mut T,
    height: u64,
    previous_chain: Vec<Arc<ChainBlock>>,
) -> Result<(), ChainStorageError>
{
    let invalid_chain = rewind_to_height(db, height)?;
    debug!(
        target: LOG_TARGET,
        "Removed {} blocks during chain restore: {:?}.",
        invalid_chain.len(),
        invalid_chain
            .iter()
            .map(|block| block.accumulated_data.hash.to_hex())
            .collect::<Vec<_>>(),
    );
    let mut txn = DbTransaction::new();
    // Add removed blocks in the reverse order that they were removed
    // See: https://github.com/tari-project/tari/issues/2182
    for block in previous_chain.into_iter().rev() {
        txn.delete(DbKey::OrphanBlock(block.accumulated_data.hash.clone()));
        insert_block(&mut txn, block)?;
    }
    db.write(txn)?;
    Ok(())
}

// Insert the provided block into the orphan pool and returns any new tips that were created
fn insert_orphan_and_find_new_tips<T: BlockchainBackend>(
    db: &mut T,
    block: Arc<Block>,
    validator: &dyn HeaderValidation<T>,
) -> Result<Vec<ChainHeader>, ChainStorageError>
{
    let hash = block.hash();

    let mut txn = DbTransaction::new();

    let mut new_tips_found = vec![];
    let parent;
    if let Some(curr_parent) = db.fetch_orphan_chain_tip_by_hash(&block.hash())? {
        parent = curr_parent;
        txn.remove_orphan_chain_tip(block.header.prev_hash.clone());
        debug!(
            target: LOG_TARGET,
            "New orphan extends a chain in the current candidate tip set"
        );
    } else {
        let best_chain_connection = db.fetch_chain_header_in_all_chains(&block.header.prev_hash.clone())?;
        if let Some(curr_parent) = best_chain_connection {
            debug!(
                target: LOG_TARGET,
                "New orphan does not have a parent in the current tip set"
            );
            parent = curr_parent;
        } else {
            debug!(
                target: LOG_TARGET,
                "Orphan {} was not connected to any previous headers. Inserting as true orphan",
                hash.to_hex()
            );

            txn.insert_orphan(block.clone());
            db.write(txn)?;
            return Ok(vec![]);
        }
    }

    let accum_builder = validator.validate(db, &block.header, &parent.accumulated_data)?;
    let accumulated_data = accum_builder
        .total_kernel_offset(
            &parent.accumulated_data.total_kernel_offset,
            &block.header.total_kernel_offset,
        )
        .build()?;

    // Extend tip

    txn.insert_chained_orphan(
        ChainBlock {
            accumulated_data: accumulated_data.clone(),
            block: block.as_ref().clone(),
        }
        .into(),
    );

    for new_tip in find_orphan_descendant_tips_of(&*db, &block.header, &accumulated_data, validator, &mut txn)? {
        txn.insert_orphan_chain_tip(new_tip.accumulated_data.hash.clone());
        new_tips_found.push(new_tip);
    }

    db.write(txn)?;
    Ok(new_tips_found)
}

// Find the tip set of any orphans that have hash as an ancestor
fn find_orphan_descendant_tips_of<T: BlockchainBackend>(
    db: &T,
    prev_header: &BlockHeader,
    prev_accumulated_data: &BlockHeaderAccumulatedData,
    validator: &dyn HeaderValidation<T>,
    txn: &mut DbTransaction,
) -> Result<Vec<ChainHeader>, ChainStorageError>
{
    let children = db.fetch_orphan_children_of(prev_accumulated_data.hash.clone())?;
    if children.is_empty() {
        return Ok(vec![ChainHeader {
            header: prev_header.clone(),
            accumulated_data: prev_accumulated_data.clone(),
        }]);
    }
    let mut res = vec![];
    for child in children {
        match validator.validate(db, &child.header, prev_accumulated_data) {
            Ok(builder) => {
                let accum_data = builder
                    .total_kernel_offset(
                        &prev_accumulated_data.total_kernel_offset,
                        &child.header.total_kernel_offset,
                    )
                    .build()?;
                txn.insert_chained_orphan(
                    ChainBlock {
                        block: child.clone(),
                        accumulated_data: accum_data.clone(),
                    }
                    .into(),
                );
                res.extend(find_orphan_descendant_tips_of(
                    db,
                    &child.header,
                    &accum_data,
                    validator,
                    txn,
                )?);
            },
            Err(e) => {
                // Warn for now, idk might lower to debug later.
                warn!(
                    target: LOG_TARGET,
                    "Discarding orphan {} because it has an invalid header: {}",
                    child.hash().to_hex(),
                    e
                );
                txn.delete_orphan(child.hash());
            },
        };
    }
    Ok(res)
}

// Discard the the orphan block from the orphan pool that corresponds to the provided block hash.
fn remove_orphan<T: BlockchainBackend>(db: &mut T, hash: HashOutput) -> Result<(), ChainStorageError> {
    let mut txn = DbTransaction::new();
    txn.delete(DbKey::OrphanBlock(hash));
    db.write(txn)
}

/// Gets all blocks from the orphan to the point where it connects to the best chain
// TODO: this would probably perform better if it reused the db transaction
#[allow(clippy::ptr_arg)]
fn get_orphan_link_main_chain<T: BlockchainBackend>(
    db: &mut T,
    orphan_tip: &HashOutput,
) -> Result<VecDeque<Arc<ChainBlock>>, ChainStorageError>
{
    let mut chain: VecDeque<Arc<ChainBlock>> = VecDeque::new();
    let mut curr_hash = orphan_tip.to_owned();
    loop {
        let curr_block = fetch!(db, curr_hash.clone(), OrphanBlock)?;
        let accumulated_data = db.fetch_orphan_header_accumulated_data(curr_hash.clone())?;
        curr_hash = curr_block.header.prev_hash.clone();
        chain.push_front(Arc::new(ChainBlock {
            block: curr_block,
            accumulated_data,
        }));

        if db.contains(&DbKey::BlockHash(curr_hash.clone()))? {
            break;
        }
    }
    Ok(chain)
}

/// Find and return the orphan chain tip with the highest accumulated difficulty.
fn find_strongest_orphan_tip(
    orphan_chain_tips: Vec<ChainHeader>,
    chain_strength_comparer: &dyn ChainStrengthComparer,
) -> Result<Option<ChainHeader>, ChainStorageError>
{
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

    Ok(best_block_header)
}

// Perform a comprehensive search to remove all the minimum height orphans to maintain the configured orphan pool
// storage limit. If the node is configured to run in pruned mode then orphan blocks with heights lower than the horizon
// block height will also be discarded.
fn cleanup_orphans<T: BlockchainBackend>(db: &mut T, orphan_storage_capacity: usize) -> Result<(), ChainStorageError> {
    let metadata = db.fetch_chain_metadata()?;
    let horizon_height = metadata.horizon_block(metadata.height_of_longest_chain());

    db.delete_oldest_orphans(horizon_height, orphan_storage_capacity)
}
fn prune_database_if_needed<T: BlockchainBackend>(
    db: &mut T,
    pruning_horizon: u64,
    pruning_interval: u64,
) -> Result<(), ChainStorageError>
{
    let metadata = db.fetch_chain_metadata()?;
    if metadata.is_pruned_node() &&
        metadata.pruned_height() <
            metadata
                .height_of_longest_chain()
                .saturating_sub(pruning_horizon + pruning_interval)
    {
        prune_database(db, pruning_horizon, metadata)
    } else {
        Ok(())
    }
}

fn prune_database<T: BlockchainBackend>(
    db: &mut T,
    pruning_horizon: u64,
    metadata: ChainMetadata,
) -> Result<(), ChainStorageError>
{
    if metadata.is_pruned_node() {
        let db_height = metadata.height_of_longest_chain();
        let abs_pruning_horizon = db_height.saturating_sub(pruning_horizon);

        let last_pruned = metadata.pruned_height();
        let mut last_block = db.fetch_block_accumulated_data_by_height(last_pruned).or_not_found(
            "BlockAccumulatedData",
            "height",
            last_pruned.to_string(),
        )?;
        let mut txn = DbTransaction::new();
        for block_to_prune in (last_pruned + 1)..abs_pruning_horizon {
            let curr_block = db.fetch_block_accumulated_data_by_height(block_to_prune).or_not_found(
                "BlockAccumulatedData",
                "height",
                block_to_prune.to_string(),
            )?;
            // Note, this could actually be done in one step instead of each block, since deleted is
            // accumulated
            let inputs_to_prune = curr_block.deleted.deleted.clone() - last_block.deleted.deleted;
            last_block = curr_block;
            txn.prune_outputs_and_update_horizon(inputs_to_prune.to_vec(), block_to_prune);
        }

        db.write(txn)?;
    }

    Ok(())
}

fn log_error<T>(req: DbKey, err: ChainStorageError) -> Result<T, ChainStorageError> {
    error!(
        target: LOG_TARGET,
        "Database access error on request: {}: {}",
        req,
        err.to_string()
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
        }
    }
}

fn convert_to_option_bounds<T: RangeBounds<u64>>(bounds: T) -> (Option<u64>, Option<u64>) {
    let start = bounds.start_bound();
    let end = bounds.end_bound();
    use Bound::*;
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

#[cfg(test)]
mod test {
    use super::*;
    use crate::test_helpers::blockchain::create_test_blockchain_db;

    #[test]
    fn lmdb_fetch_monero_seeds() {
        // Perform test
        {
            let db = create_test_blockchain_db();
            let seed = "test1".to_string();
            {
                let db_read = db.db_read_access().unwrap();
                assert_eq!(db_read.fetch_monero_seed_first_seen_height(&seed).unwrap(), 0);
            }
            {
                let mut txn = DbTransaction::new();
                txn.insert_monero_seed_height(&seed, 5);
                let mut db_write = db.test_db_write_access().unwrap();
                assert!(db_write.write(txn).is_ok());
            }
            {
                let db_read = db.db_read_access().unwrap();
                assert_eq!(db_read.fetch_monero_seed_first_seen_height(&seed).unwrap(), 5);
            }

            {
                let mut txn = DbTransaction::new();
                txn.insert_monero_seed_height(&seed, 2);
                let mut db_write = db.db_write_access().unwrap();
                assert!(db_write.write(txn).is_ok());
            }
            {
                let db_read = db.db_read_access().unwrap();
                assert_eq!(db_read.fetch_monero_seed_first_seen_height(&seed).unwrap(), 2);
            }
        }
    }
}
