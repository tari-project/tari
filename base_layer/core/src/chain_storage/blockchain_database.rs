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
        consts::{
            BLOCKCHAIN_DATABASE_ORPHAN_STORAGE_CAPACITY,
            BLOCKCHAIN_DATABASE_PRUNED_MODE_PRUNING_INTERVAL,
            BLOCKCHAIN_DATABASE_PRUNING_HORIZON,
        },
        db_transaction::{DbKey, DbTransaction, DbValue, MetadataKey, MetadataValue, MmrTree},
        error::ChainStorageError,
        BlockAccumulatedData,
        BlockHeaderAccumulatedData,
        HistoricalBlock,
        InProgressHorizonSyncState,
    },
    consensus::{chain_strength_comparer::ChainStrengthComparer, ConsensusManager},
    proof_of_work::{Difficulty, PowAlgorithm, ProofOfWork},
    transactions::{
        transaction::{TransactionInput, TransactionKernel, TransactionOutput},
        types::{Commitment, HashOutput, Signature},
    },
    validation::{StatefulValidation, StatefulValidator, Validation, Validator},
};
use croaring::Bitmap;
use digest::Input;
use log::*;
use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    collections::VecDeque,
    mem,
    sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard},
    time::Instant,
};
use strum_macros::Display;
use tari_common_types::{chain_metadata::ChainMetadata, types::BlockHash};
use tari_crypto::{
    hash::blake2::Blake256,
    tari_utilities::{epoch_time::EpochTime, hex::Hex, Hashable},
};
use tari_mmr::{pruned_hashset::PrunedHashSet, Hash, MerkleMountainRange, MutableMmr, MutableMmrLeafNodes};
use uint::static_assertions::_core::convert::Infallible;

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
    Ok,
    BlockExists,
    OrphanBlock,
    /// Indicates the new block caused a chain reorg. This contains removed blocks followed by added blocks.
    ChainReorg(Vec<Arc<Block>>, Vec<Arc<Block>>),
}

/// MutableMmrState provides the total number of leaf nodes in the base MMR and the requested leaf nodes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MutableMmrState {
    pub total_leaf_count: usize,
    pub leaf_nodes: MutableMmrLeafNodes,
}

/// A placeholder struct that contains the two validators that the database uses to decide whether or not a block is
/// eligible to be added to the database. The `block` validator should perform a full consensus check. The `orphan`
/// validator needs to check that the block is internally consistent, but can't know whether the PoW is sufficient,
/// for example.
/// The `GenesisBlockValidator` is used to check that the chain builds on the correct genesis block.
/// The `ChainTipValidator` is used to check that the accounting balance and MMR states of the chain state is valid.
pub struct Validators<B> {
    block: Arc<StatefulValidator<Block, B>>,
    orphan: Arc<Validator<Block>>,
}

impl<B: BlockchainBackend> Validators<B> {
    pub fn new(block: impl StatefulValidation<Block, B> + 'static, orphan: impl Validation<Block> + 'static) -> Self {
        Self {
            block: Arc::new(Box::new(block)),
            orphan: Arc::new(Box::new(orphan)),
        }
    }
}

impl<B> Clone for Validators<B> {
    fn clone(&self) -> Self {
        Validators {
            block: Arc::clone(&self.block),
            orphan: Arc::clone(&self.orphan),
        }
    }
}

/// Identify behaviour for Blockchain database back ends. Implementations must support `Send` and `Sync` so that
/// `BlockchainDatabase` can be thread-safe. The backend *must* also execute transactions atomically; i.e., every
/// operation within it must succeed, or they all fail. Failure to support this contract could lead to
/// synchronisation issues in your database backend.
///
/// Data is passed to and from the backend via the [DbKey], [DbValue], and [DbValueKey] enums. This strategy allows
/// us to keep the reading and writing API extremely simple. Extending the types of data that the back ends can handle
/// will entail adding to those enums, and the back ends, while this trait can remain unchanged.
#[allow(clippy::ptr_arg)]
pub trait BlockchainBackend: Send + Sync {
    /// Commit the transaction given to the backend. If there is an error, the transaction must be rolled back, and
    /// the error condition returned. On success, every operation in the transaction will have been committed, and
    /// the function will return `Ok(())`.
    fn write(&mut self, tx: DbTransaction) -> Result<(), ChainStorageError>;
    /// Fetch a value from the back end corresponding to the given key. If the value is not found, `get` must return
    /// `Ok(None)`. It should only error if there is an access or integrity issue with the underlying back end.
    fn fetch(&self, key: &DbKey) -> Result<Option<DbValue>, ChainStorageError>;
    /// Checks to see whether the given key exists in the back end. This function should only fail if there is an
    /// access or integrity issue with the back end.
    fn contains(&self, key: &DbKey) -> Result<bool, ChainStorageError>;

    /// Fetches data that is calculated and accumulated for blocks that have been
    /// added to a chain of headers
    fn fetch_header_accumulated_data(
        &self,
        hash: &HashOutput,
    ) -> Result<Option<BlockHeaderAccumulatedData>, ChainStorageError>;

    /// Used to determine if the database is empty, i.e. a brand new database.
    /// This is called to decide if the genesis block should be created.
    fn is_empty(&self) -> Result<bool, ChainStorageError>;

    /// Fetch accumulated data like MMR peaks and deleted hashmap
    fn fetch_block_accumulated_data(&self, header_hash: &HashOutput)
        -> Result<BlockAccumulatedData, ChainStorageError>;

    /// Fetch all the kernels in a block
    fn fetch_kernels_in_block(&self, header_hash: &HashOutput) -> Result<Vec<TransactionKernel>, ChainStorageError>;

    /// Fetch a specific output. Returns the output and the leaf index in the output MMR
    fn fetch_output(&self, output_hash: &HashOutput) -> Result<Option<(TransactionOutput, u32)>, ChainStorageError>;

    /// Fetch all outputs in a block
    fn fetch_outputs_in_block(&self, header_hash: &HashOutput) -> Result<Vec<TransactionOutput>, ChainStorageError>;

    /// Fetch all inputs in a block
    fn fetch_inputs_in_block(&self, header_hash: &HashOutput) -> Result<Vec<TransactionInput>, ChainStorageError>;

    /// Fetches the total merkle mountain range node count upto the specified height.
    fn fetch_mmr_node_count(&self, tree: MmrTree, height: u64) -> Result<u32, ChainStorageError>;
    /// Fetches the leaf node hash and its deletion status for the nth leaf node in the given MMR tree. The height
    /// parameter is used to select the point in history used for the node deletion status.
    fn fetch_mmr_node(
        &self,
        tree: MmrTree,
        pos: u32,
        hist_height: Option<u64>,
    ) -> Result<(Hash, bool), ChainStorageError>;
    /// Fetches the set of leaf node hashes and their deletion status' for the nth to nth+count leaf node index in the
    /// given MMR tree. The height parameter is used to select the point in history used for the node deletion status.
    fn fetch_mmr_nodes(
        &self,
        tree: MmrTree,
        pos: u32,
        count: u32,
        hist_height: Option<u64>,
    ) -> Result<Vec<(Hash, bool)>, ChainStorageError>;
    /// Inserts an MMR node consisting of a leaf node hash and its deletion status into the given MMR tree.
    fn insert_mmr_node(&mut self, tree: MmrTree, hash: Hash, deleted: bool) -> Result<(), ChainStorageError>;
    /// Marks the MMR node corresponding to the provided hash as deleted.
    #[allow(clippy::ptr_arg)]
    fn delete_mmr_node(&mut self, tree: MmrTree, hash: &Hash) -> Result<(), ChainStorageError>;
    /// Fetches the leaf index of the provided leaf node hash in the given MMR tree.
    #[allow(clippy::ptr_arg)]
    fn fetch_mmr_leaf_index(&self, tree: MmrTree, hash: &Hash) -> Result<Option<u32>, ChainStorageError>;
    /// Returns the number of blocks in the block orphan pool.
    fn get_orphan_count(&self) -> Result<usize, ChainStorageError>;
    /// Returns the stored header with the highest corresponding height.
    fn fetch_last_header(&self) -> Result<BlockHeader, ChainStorageError>;
    /// Returns the stored chain metadata.
    fn fetch_chain_metadata(&self) -> Result<ChainMetadata, ChainStorageError>;
    /// Returns the set of target difficulties for the specified proof of work algorithm.
    fn fetch_target_difficulties(
        &self,
        pow_algo: PowAlgorithm,
        height: u64,
        block_window: usize,
    ) -> Result<Vec<(EpochTime, Difficulty)>, ChainStorageError>;

    /// Returns the UTXO count
    fn count_utxos(&self) -> Result<usize, ChainStorageError>;
    /// Returns the kernel count
    fn count_kernels(&self) -> Result<usize, ChainStorageError>;

    /// Fetches all of the orphans (hash) that are currently at the tip of an alternate chain
    fn fetch_orphan_chain_tips(&self) -> Result<Vec<HashOutput>, ChainStorageError>;
    /// Fetch all orphans that have `hash` as a previous hash
    fn fetch_orphan_children_of(&self, hash: HashOutput) -> Result<Vec<HashOutput>, ChainStorageError>;
    /// Delete orphans according to age. Used to keep the orphan pool at a certain capacity
    fn delete_oldest_orphans(
        &mut self,
        horizon_height: u64,
        orphan_storage_capacity: usize,
    ) -> Result<(), ChainStorageError>;
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
///
/// ```
/// use tari_core::{
///     chain_storage::{BlockchainDatabase, BlockchainDatabaseConfig, MemoryDatabase, Validators},
///     consensus::{ConsensusManagerBuilder, Network},
///     transactions::types::HashDigest,
///     validation::{mocks::MockValidator, StatefulValidation},
/// };
/// let db_backend = MemoryDatabase::<HashDigest>::default();
/// let validators = Validators::new(MockValidator::new(true), MockValidator::new(true));
/// let db = MemoryDatabase::<HashDigest>::default();
/// let network = Network::LocalNet;
/// let rules = ConsensusManagerBuilder::new(network).build();
/// let db = BlockchainDatabase::new(
///     db_backend,
///     &rules,
///     validators,
///     BlockchainDatabaseConfig::default(),
///     false,
/// )
/// .unwrap();
/// // Do stuff with db
/// ```
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
            let genesis_block = consensus_manager.get_genesis_block();
            blockchain_db.store_new_block(genesis_block)?;
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

    fn db_write_access(&self) -> Result<RwLockWriteGuard<B>, ChainStorageError> {
        self.db.write().map_err(|e| {
            error!(
                target: LOG_TARGET,
                "An attempt to get a write lock on the blockchain backend failed. {:?}", e
            );
            ChainStorageError::AccessError("Write lock on blockchain backend failed".into())
        })
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

    /// Sets and stores the chain metadata, overwriting previous metadata.
    pub fn set_chain_metadata(&self, metadata: ChainMetadata) -> Result<(), ChainStorageError> {
        let mut db = self.db_write_access()?;
        set_chain_metadata(&mut *db, metadata)
    }

    /// Returns the transaction kernel with the given hash.
    pub fn fetch_kernel(&self, hash: HashOutput) -> Result<TransactionKernel, ChainStorageError> {
        let db = self.db_read_access()?;
        fetch_kernel(&*db, hash)
    }

    // Fetch the utxo
    pub fn fetch_utxo(&self, hash: HashOutput) -> Result<Option<TransactionOutput>, ChainStorageError> {
        let db = self.db_read_access()?;
        Ok(db.fetch_output(&hash)?.map(|(out, _index)| out))
    }

    // Return a list of matching utxos, with each being `None` if not found. If found, the transaction
    // output, and a boolean indicating if the UTXO was spent as of the block hash specified or the tip if not
    // specified.
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
        let data = db.fetch_block_accumulated_data(&is_spent_as_of)?;
        let mut result = vec![];
        for hash in hashes {
            let output = db.fetch_output(&hash)?;
            result.push(output.map(|(out, mmr_index)| (out, data.deleted.contains(mmr_index))));
        }
        Ok(result)
    }

    /// Returns the block header at the given block height.
    pub fn fetch_header(&self, block_num: u64) -> Result<BlockHeader, ChainStorageError> {
        let db = self.db_read_access()?;
        fetch_header(&*db, block_num)
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
    pub fn insert_valid_headers(&self, headers: Vec<BlockHeader>) -> Result<(), ChainStorageError> {
        let mut db = self.db_write_access()?;
        insert_headers(&mut *db, headers)
    }

    /// Returns the set of block headers between `start` and up to and including `end_inclusive`
    pub fn fetch_headers(&self, start: u64, end_inclusive: u64) -> Result<Vec<BlockHeader>, ChainStorageError> {
        let db = self.db_read_access()?;
        fetch_headers(&*db, start, end_inclusive)
    }

    /// Returns the block header corresponding` to the provided BlockHash
    pub fn fetch_header_by_block_hash(&self, hash: HashOutput) -> Result<Option<BlockHeader>, ChainStorageError> {
        let db = self.db_read_access()?;
        fetch_header_by_block_hash(&*db, hash)
    }

    /// Returns the header at the tip
    pub fn fetch_tip_header(&self) -> Result<BlockHeader, ChainStorageError> {
        let db = self.db_read_access()?;
        fetch_tip_header(&*db)
    }

    /// Returns the sum of all UTXO commitments
    pub fn fetch_utxo_commitment_sum(&self, at_hash: &HashOutput) -> Result<Commitment, ChainStorageError> {
        let db = self.db_read_access()?;
        Ok(db.fetch_block_accumulated_data(at_hash)?.total_utxo_sum)
    }

    /// Returns the sum of all kernels
    pub fn fetch_kernel_commitment_sum(&self, at_hash: &HashOutput) -> Result<Commitment, ChainStorageError> {
        let db = self.db_read_access()?;
        Ok(db.fetch_block_accumulated_data(at_hash)?.total_kernel_sum)
    }

    /// Returns the orphan block with the given hash.
    pub fn fetch_orphan(&self, hash: HashOutput) -> Result<Block, ChainStorageError> {
        let db = self.db_read_access()?;
        fetch_orphan(&*db, hash)
    }

    pub fn fetch_all_orphans(&self) -> Result<Vec<Block>, ChainStorageError> {
        unimplemented!()
        // let db = self.db_read_access()?;
        // let mut result = vec![];
        // // TODO: this is a bit clumsy in order to safely handle the results. There should be a cleaner way
        // db.for_each_orphan(|o| result.push(o))?;
        // let mut orphans = vec![];
        // for o in result {
        //     // check each result
        //     orphans.push(o?.1);
        // }
        // Ok(orphans)
    }

    /// Returns the set of target difficulties for the specified proof of work algorithm.
    pub fn fetch_target_difficulties(
        &self,
        pow_algo: PowAlgorithm,
        height: u64,
        block_window: usize,
    ) -> Result<Vec<(EpochTime, Difficulty)>, ChainStorageError>
    {
        let db = self.db_read_access()?;
        db.fetch_target_difficulties(pow_algo, height, block_window)
    }

    /// `calculate_mmr_roots` takes a block template and calculates the MMR roots for a hypothetical new block that
    /// would be built onto the chain tip. Note that _no checks_ are made to determine whether the template would
    /// actually be a valid extension to the chain; only the new MMR roots are calculated
    pub fn calculate_mmr_roots(&self, template: NewBlockTemplate) -> Result<Block, ChainStorageError> {
        let db = self.db_read_access()?;
        calculate_mmr_roots(&*db, template)
    }

    /// Fetches the total merkle mountain range node count upto the specified height.
    pub fn fetch_mmr_node_count(&self, tree: MmrTree, height: u64) -> Result<u32, ChainStorageError> {
        let db = self.db_read_access()?;
        db.fetch_mmr_node_count(tree, height)
    }

    /// Fetches the set of leaf node hashes and their deletion status' for the given MMR tree.
    pub fn fetch_mmr_nodes(
        &self,
        tree: MmrTree,
        pos: u32,
        count: u32,
        hist_height: Option<u64>,
    ) -> Result<Vec<(Hash, bool)>, ChainStorageError>
    {
        let db = self.db_read_access()?;
        db.fetch_mmr_nodes(tree, pos, count, hist_height)
    }

    /// Inserts an MMR node consisting of a leaf node hash and its deletion status into the given MMR tree.
    pub fn insert_mmr_node(&self, tree: MmrTree, hash: Hash, deleted: bool) -> Result<(), ChainStorageError> {
        let mut db = self.db_write_access()?;
        db.insert_mmr_node(tree, hash, deleted)
    }

    /// Marks the MMR node corresponding to the provided hash as deleted.
    #[allow(clippy::ptr_arg)]
    pub fn delete_mmr_node(&self, tree: MmrTree, hash: &Hash) -> Result<(), ChainStorageError> {
        let mut db = self.db_write_access()?;
        db.delete_mmr_node(tree, hash)
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
            &*self.validators.block,
            &*self.consensus_manager.chain_strength_comparer(),
            block,
        )?;

        // Cleanup orphan block pool
        match block_add_result {
            BlockAddResult::OrphanBlock | BlockAddResult::ChainReorg(_, _) => {
                cleanup_orphans(&mut *db, self.config.orphan_storage_capacity)?
            },
            _ => {},
        }

        // Cleanup of backend when in pruned mode.
        match block_add_result {
            BlockAddResult::Ok | BlockAddResult::ChainReorg(_, _) => prune_database(
                &mut *db,
                self.config.pruning_interval,
                self.config.pruning_horizon,
                new_height,
            )?,
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
    pub fn cleanup_all_orphans(&self) -> Result<(), ChainStorageError> {
        let mut db = self.db_write_access()?;
        let _ = cleanup_orphans(&mut *db, 0)?;
        Ok(())
    }

    fn store_new_block(&self, block: Block) -> Result<(), ChainStorageError> {
        let mut txn = DbTransaction::new();
        let mut db = self.db_write_access()?;
        store_new_block(&*db, &mut txn, block)?;
        commit(&mut *db, txn)
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

    /// Returns the set of block between `start` and up to and including `end_inclusive`
    pub fn fetch_blocks(&self, start: u64, end_inclusive: u64) -> Result<Vec<HistoricalBlock>, ChainStorageError> {
        let db = self.db_read_access()?;
        fetch_blocks(&*db, start, end_inclusive)
    }

    /// Attempt to fetch the block corresponding to the provided hash from the main chain, if it cannot be found then
    /// the block will be searched in the orphan block pool.
    pub fn fetch_block_with_hash(&self, hash: BlockHash) -> Result<Option<HistoricalBlock>, ChainStorageError> {
        let db = self.db_read_access()?;
        fetch_block_with_hash(&*db, hash)
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
        commit(&mut *db, txn)
    }

    pub fn get_horizon_sync_state(&self) -> Result<Option<InProgressHorizonSyncState>, ChainStorageError> {
        let db = self.db_read_access()?;
        get_horizon_sync_state(&*db)
    }

    pub fn set_horizon_sync_state(&self, state: InProgressHorizonSyncState) -> Result<(), ChainStorageError> {
        let mut txn = DbTransaction::new();
        txn.set_metadata(MetadataKey::HorizonSyncState, MetadataValue::HorizonSyncState(state));
        self.commit(txn)
    }

    /// Rewind the blockchain state to the block height given and return the blocks that were removed and orphaned.
    ///
    /// The operation will fail if
    /// * The block height is in the future
    /// * The block height is before the horizon block height determined by the pruning horizon
    pub fn rewind_to_height(&self, height: u64) -> Result<Vec<Arc<Block>>, ChainStorageError> {
        let mut db = self.db_write_access()?;
        rewind_to_height(&mut *db, height)
    }

    /// Prepares the database for horizon sync. This function sets the PendingHorizonSyncState for the database
    /// and sets the chain metadata to indicate that this node can not provide any sync data until sync is complete.
    pub fn horizon_sync_begin(&self) -> Result<InProgressHorizonSyncState, ChainStorageError> {
        let db = self.db_write_access()?;
        match get_horizon_sync_state(&*db)? {
            Some(state) => {
                info!(
                    target: LOG_TARGET,
                    "Previous horizon sync was interrupted. Attempting to recover."
                );
                debug!(target: LOG_TARGET, "Existing PendingHorizonSyncState = ({})", state);
                Ok(state)
            },
            None => {
                unimplemented!();
                // let metadata = db.fetch_chain_metadata()?;
                //
                // let state = InProgressHorizonSyncState {
                //     metadata,
                //     initial_kernel_checkpoint_count: db.count_checkpoints(MmrTree::Kernel)? as u64,
                //     initial_utxo_checkpoint_count: db.count_checkpoints(MmrTree::Utxo)? as u64,
                //     initial_rangeproof_checkpoint_count: db.count_checkpoints(MmrTree::Utxo)? as u64,
                // };
                // debug!(target: LOG_TARGET, "Preparing database for horizon sync. ({})", state);
                //
                // let mut txn = DbTransaction::new();
                //
                // txn.set_metadata(
                //     MetadataKey::HorizonSyncState,
                //     MetadataValue::HorizonSyncState(state.clone()),
                // );
                //
                // // During horizon state syncing the blockchain backend will be in an inconsistent state until the
                // entire // horizon state has been synced. Reset the local chain metadata will limit
                // other nodes and // local service from requesting data while the horizon sync is in
                // progress. txn.set_metadata(MetadataKey::ChainHeight,
                // MetadataValue::ChainHeight(Some(0))); txn.set_metadata(
                //     MetadataKey::EffectivePrunedHeight,
                //     MetadataValue::EffectivePrunedHeight(0),
                // );
                // txn.set_metadata(MetadataKey::AccumulatedWork, MetadataValue::AccumulatedWork(None));
                // commit(&mut *db, txn)?;
                //
                // Ok(state)
            },
        }
    }

    /// Commit the current synced horizon state.
    pub fn horizon_sync_commit(&self) -> Result<(), ChainStorageError> {
        let mut db = self.db_write_access()?;
        let tip_header = db.fetch_last_header()?;

        let mut txn = DbTransaction::new();

        // Update metadata
        txn.set_metadata(MetadataKey::ChainHeight, MetadataValue::ChainHeight(tip_header.height));

        let best_block = tip_header.hash();
        txn.set_metadata(MetadataKey::BestBlock, MetadataValue::BestBlock(best_block));

        let accumulated_difficulty =
            ProofOfWork::new_from_difficulty(&tip_header.pow, ProofOfWork::achieved_difficulty(&tip_header)?)
                .total_accumulated_difficulty();
        txn.set_metadata(
            MetadataKey::AccumulatedWork,
            MetadataValue::AccumulatedWork(accumulated_difficulty),
        );

        txn.set_metadata(
            MetadataKey::EffectivePrunedHeight,
            MetadataValue::EffectivePrunedHeight(tip_header.height),
        );

        // Remove pending horizon sync state
        // txn.delete_metadata(MetadataKey::HorizonSyncState);

        let _res = commit(&mut *db, txn);
        unimplemented!();
    }

    /// Rollback the current synced horizon state to a consistent state.
    pub fn horizon_sync_rollback(&self) -> Result<(), ChainStorageError> {
        unimplemented!()
        // let mut db = self.db_write_access()?;
        // let sync_state = match get_horizon_sync_state(&*db)? {
        //     Some(state) => state,
        //     None => {
        //         debug!(target: LOG_TARGET, "Horizon sync: Nothing to roll back");
        //         return Ok(());
        //     },
        // };
        //
        // let mut txn = DbTransaction::new();
        //
        // // Rollback added kernels
        // let first_tmp_checkpoint_index =
        //     usize::try_from(sync_state.initial_kernel_checkpoint_count).map_err(|_| ChainStorageError::OutOfRange)?;
        // let cp_count = db.count_checkpoints(MmrTree::Kernel)?;
        // for i in first_tmp_checkpoint_index..cp_count {
        //     let cp = db
        //         .fetch_checkpoint_at_index(MmrTree::Kernel, i)?
        //         .unwrap_or_else(|| panic!("Database is corrupt: Failed to fetch kernel checkpoint at index {}", i));
        //     let (nodes_added, _) = cp.into_parts();
        //     for hash in nodes_added {
        //         txn.delete(DbKey::TransactionKernel(hash));
        //     }
        // }
        //
        // txn.rewind_kernel_mmr(cp_count - first_tmp_checkpoint_index);
        //
        // // Rollback UTXO changes
        // let first_tmp_checkpoint_index =
        //     usize::try_from(sync_state.initial_utxo_checkpoint_count).map_err(|_| ChainStorageError::OutOfRange)?;
        // let cp_count = db.count_checkpoints(MmrTree::Utxo)?;
        // for i in first_tmp_checkpoint_index..cp_count {
        //     let cp = db
        //         .fetch_checkpoint_at_index(MmrTree::Utxo, i)?
        //         .unwrap_or_else(|| panic!("Database is corrupt: Failed to fetch UTXO checkpoint at index {}", i));
        //     let (nodes_added, deleted) = cp.into_parts();
        //     for hash in nodes_added {
        //         txn.delete(DbKey::UnspentOutput(hash));
        //     }
        //     for pos in deleted.iter() {
        //         let (stxo_hash, is_deleted) = db.fetch_mmr_node(MmrTree::Utxo, pos, None)?;
        //         debug_assert!(is_deleted);
        //         txn.unspend_stxo(stxo_hash);
        //     }
        // }
        //
        // txn.rewind_utxo_mmr(cp_count - first_tmp_checkpoint_index);
        //
        // // Rollback Rangeproof checkpoints
        // let first_tmp_checkpoint_index = usize::try_from(sync_state.initial_rangeproof_checkpoint_count)
        //     .map_err(|_| ChainStorageError::OutOfRange)?;
        // let rp_checkpoint_count = db.count_checkpoints(MmrTree::RangeProof)?;
        // txn.rewind_rangeproof_mmr(rp_checkpoint_count - first_tmp_checkpoint_index);
        //
        // // Rollback metadata
        // let metadata = sync_state.metadata;
        // txn.set_metadata(
        //     MetadataKey::ChainHeight,
        //     MetadataValue::ChainHeight(metadata.height_of_longest_chain),
        // );
        // txn.set_metadata(MetadataKey::BestBlock, MetadataValue::BestBlock(metadata.best_block));
        // txn.set_metadata(
        //     MetadataKey::AccumulatedWork,
        //     MetadataValue::AccumulatedWork(metadata.accumulated_difficulty),
        // );
        // txn.set_metadata(
        //     MetadataKey::EffectivePrunedHeight,
        //     MetadataValue::EffectivePrunedHeight(metadata.effective_pruned_height),
        // );
        //
        // // Remove pending horizon sync state
        // txn.delete_metadata(MetadataKey::HorizonSyncState);
        //
        // commit(&mut *db, txn)
    }

    /// Store the provided set of kernels and persists a checkpoint
    pub fn horizon_sync_insert_kernels(&self, _kernels: Vec<TransactionKernel>) -> Result<(), ChainStorageError> {
        // let mut db = self.db_write_access()?;
        // let mut txn = DbTransaction::new();
        // // kernels.into_iter().for_each(|kernel| txn.insert_kernel(kernel));
        // txn.create_mmr_checkpoint(MmrTree::Kernel);
        // commit(&mut *db, txn)
        unimplemented!()
    }

    /// Spends the UTXOs with the given hashes
    pub fn horizon_sync_spend_utxos(&self, _hashes: Vec<HashOutput>) -> Result<(), ChainStorageError> {
        // let mut db = self.db_write_access()?;
        // let mut txn = DbTransaction::new();
        // hashes.into_iter().for_each(|hash| txn.spend_utxo(hash));
        // txn.create_mmr_checkpoint(MmrTree::Utxo);
        // commit(&mut *db, txn)
        unimplemented!();
    }
}

fn unexpected_result<T>(req: DbKey, res: DbValue) -> Result<T, ChainStorageError> {
    let msg = format!("Unexpected result for database query {}. Response: {}", req, res);
    error!(target: LOG_TARGET, "{}", msg);
    Err(ChainStorageError::UnexpectedResult(msg))
}

fn set_chain_metadata<T: BlockchainBackend>(db: &mut T, metadata: ChainMetadata) -> Result<(), ChainStorageError> {
    let mut txn = DbTransaction::new();
    txn.set_metadata(
        MetadataKey::ChainHeight,
        MetadataValue::ChainHeight(metadata.height_of_longest_chain()),
    );
    txn.set_metadata(
        MetadataKey::BestBlock,
        MetadataValue::BestBlock(metadata.best_block().clone()),
    );
    txn.set_metadata(
        MetadataKey::AccumulatedWork,
        MetadataValue::AccumulatedWork(metadata.accumulated_difficulty()),
    );
    commit(db, txn)
}

pub fn calculate_mmr_roots<T: BlockchainBackend>(
    db: &T,
    template: NewBlockTemplate,
) -> Result<Block, ChainStorageError>
{
    let NewBlockTemplate { header, mut body } = template;
    body.sort();

    let BlockAccumulatedData {
        kernels,
        outputs,
        range_proofs,
        deleted,
        total_kernel_sum: _,
        total_utxo_sum: _,
    } = db.fetch_block_accumulated_data(&header.prev_hash)?;

    let mut kernel_mmr: MerkleMountainRange<Blake256, _> = MerkleMountainRange::new(kernels);
    let mut output_mmr: MutableMmr<Blake256, _> = MutableMmr::new(outputs, deleted)?;
    let mut proof_mmr: MerkleMountainRange<Blake256, _> = MerkleMountainRange::new(range_proofs);

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

        if !output_mmr.delete_and_compress(index, false) {
            return Err(ChainStorageError::InvalidOperation(format!(
                "Could not delete index {} from the output MMR",
                index
            )));
        }
    }

    output_mmr.compress();

    let mut header = BlockHeader::from(header);
    header.kernel_mr = include_legacy_deleted_hash(kernel_mmr.get_merkle_root()?);
    header.output_mr = output_mmr.get_merkle_root()?;
    header.range_proof_mr = include_legacy_deleted_hash(proof_mmr.get_merkle_root()?);
    Ok(Block { header, body })
}

fn fetch_kernel<T: BlockchainBackend>(db: &T, hash: HashOutput) -> Result<TransactionKernel, ChainStorageError> {
    fetch!(db, hash, TransactionKernel)
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

fn insert_headers<T: BlockchainBackend>(db: &mut T, headers: Vec<BlockHeader>) -> Result<(), ChainStorageError> {
    let mut txn = DbTransaction::new();
    headers.into_iter().for_each(|header| {
        txn.insert_header(header);
    });
    commit(db, txn)
}

fn fetch_header_by_block_hash<T: BlockchainBackend>(
    db: &T,
    hash: BlockHash,
) -> Result<Option<BlockHeader>, ChainStorageError>
{
    try_fetch!(db, hash, BlockHash)
}

pub fn fetch_tip_header<T: BlockchainBackend>(db: &T) -> Result<BlockHeader, ChainStorageError> {
    db.fetch_last_header().map_err(|e| {
        error!(target: LOG_TARGET, "Could not fetch the tip header of the db. {:?}", e);
        e
    })
}

fn fetch_orphan<T: BlockchainBackend>(db: &T, hash: BlockHash) -> Result<Block, ChainStorageError> {
    fetch!(db, hash, OrphanBlock)
}

fn add_block<T: BlockchainBackend>(
    db: &mut T,
    block_validator: &StatefulValidator<Block, T>,
    chain_strength_comparer: &dyn ChainStrengthComparer,
    block: Arc<Block>,
) -> Result<BlockAddResult, ChainStorageError>
{
    let block_hash = block.hash();
    if db.contains(&DbKey::BlockHash(block_hash))? {
        return Ok(BlockAddResult::BlockExists);
    }
    handle_possible_reorg(db, block_validator, chain_strength_comparer, block)
}

// Adds a new block onto the chain tip.
fn store_new_block<T: BlockchainBackend>(
    db: &T,
    txn: &mut DbTransaction,
    block: Block,
) -> Result<(), ChainStorageError>
{
    debug!(
        target: LOG_TARGET,
        "Storing new block #{} `{}`",
        block.header.height,
        block.hash().to_hex()
    );
    // Try to take ownership of the Block if this is the only reference, otherwise take ownership of a clone
    let (header, inputs, outputs, kernels) = block.dissolve();
    let height = header.height;
    let header_hash = header.hash();
    let accumulated_difficulty = header.get_proof_of_work()?.total_accumulated_difficulty();
    // Build all the DB queries needed to add the block and the add it atomically

    // Update metadata
    txn.set_metadata(MetadataKey::ChainHeight, MetadataValue::ChainHeight(height));
    txn.set_metadata(MetadataKey::BestBlock, MetadataValue::BestBlock(header_hash.clone()));
    txn.set_metadata(
        MetadataKey::AccumulatedWork,
        MetadataValue::AccumulatedWork(accumulated_difficulty),
    );
    txn.insert_header(header.clone());
    let prev_accumulated_data_total_kernel_offset = if height == 0 {
        0.into()
    } else {
        db.fetch_header_accumulated_data(&header.prev_hash)?
            .ok_or_else(|| ChainStorageError::ValueNotFound {
                entity: "HeaderAccumulatedData".to_string(),
                field: "header hash".to_string(),
                value: header.prev_hash.to_hex(),
            })?
            .total_kernel_offset
    };
    txn.insert_header_accumulated_data(BlockHeaderAccumulatedData {
        hash: header_hash.clone(),
        total_kernel_offset: header.total_kernel_offset + prev_accumulated_data_total_kernel_offset,
    });
    let mut kernel_mmr: MerkleMountainRange<Blake256, _>;
    let mut output_mmr: MutableMmr<Blake256, _>;
    let mut proof_mmr: MerkleMountainRange<Blake256, _>;
    let mut total_utxo;
    let mut total_kernel;

    if header.height > 0 {
        let BlockAccumulatedData {
            kernels: kernel_set,
            outputs: output_set,
            range_proofs: range_proof_set,
            deleted,
            total_utxo_sum,
            total_kernel_sum,
        } = db.fetch_block_accumulated_data(&header.prev_hash)?;

        total_utxo = Some(total_utxo_sum);
        total_kernel = Some(total_kernel_sum);
        kernel_mmr = MerkleMountainRange::new(kernel_set);
        output_mmr = MutableMmr::new(output_set, deleted)?;
        proof_mmr = MerkleMountainRange::new(range_proof_set);
    } else {
        total_utxo = None;
        total_kernel = None;
        kernel_mmr = MerkleMountainRange::new(PrunedHashSet::default());

        output_mmr = MutableMmr::new(PrunedHashSet::default(), Bitmap::create())?;
        proof_mmr = MerkleMountainRange::new(PrunedHashSet::default());
    }

    for kernel in kernels.iter() {
        let pos = kernel_mmr.push(kernel.hash())?;
        txn.insert_kernel(kernel.clone(), header_hash.clone(), pos as u32);

        if let Some(t) = total_kernel {
            total_kernel = Some(&t + &kernel.excess);
        } else {
            total_kernel = Some(kernel.excess.clone());
        }
    }

    for output in outputs.iter() {
        output_mmr.push(output.hash())?;

        proof_mmr.push(output.proof().hash())?;

        txn.insert_utxo(
            output.clone(),
            header_hash.clone(),
            (proof_mmr.get_leaf_count()? - 1) as u32,
        );

        if let Some(t) = total_utxo {
            total_utxo = Some(&t + &output.commitment);
        } else {
            total_utxo = Some(output.commitment.clone());
        }
    }

    for input in inputs.iter() {
        let index = db.fetch_mmr_leaf_index(MmrTree::Utxo, &input.hash())?.unwrap();
        if !output_mmr.delete_and_compress(index, false) {
            return Err(ChainStorageError::InvalidOperation(format!(
                "Could not delete index {} from the output MMR",
                index
            )));
        }
        txn.insert_input(input.clone(), header_hash.clone(), index);

        if let Some(t) = total_utxo {
            total_utxo = Some(&t - &input.commitment);
        }
    }
    output_mmr.compress();

    // Check that MMR roots match
    // TODO: To remove later. This is just a sanity check
    let kernel_root = include_legacy_deleted_hash(kernel_mmr.get_merkle_root()?);
    let output_root = output_mmr.get_merkle_root()?;
    let range_proof_root = include_legacy_deleted_hash(proof_mmr.get_merkle_root()?);
    if output_root != header.output_mr {
        return Err(ChainStorageError::InvalidOperation(
            "Calculated output MMR root did not match header mmr".to_string(),
        ));
    }
    if kernel_root != header.kernel_mr {
        return Err(ChainStorageError::InvalidOperation(
            "Calculated Kernel MMR root did not match header mmr".to_string(),
        ));
    }
    if range_proof_root != header.range_proof_mr {
        return Err(ChainStorageError::InvalidOperation(
            "Calculated range proof MMR root did not match header mmr".to_string(),
        ));
    }

    txn.set_block_accumulated_data(
        header_hash,
        BlockAccumulatedData::new(
            kernel_mmr.get_pruned_hash_set()?,
            output_mmr.mmr().get_pruned_hash_set()?,
            proof_mmr.get_pruned_hash_set()?,
            output_mmr.deleted().clone(),
            total_kernel.unwrap(),
            total_utxo.unwrap(),
        ),
    );

    Ok(())
}

fn include_legacy_deleted_hash(mmr_root: HashOutput) -> HashOutput {
    // TODO: Remove this function. It is here because previous
    // versions of the code would include this hashss
    let bitmap_ser = Bitmap::create().serialize();
    let mut hasher = Blake256::new();
    hasher.input(mmr_root);
    hasher.input(&bitmap_ser);
    hasher.result().to_vec()
}

fn store_pruning_horizon<T: BlockchainBackend>(db: &mut T, pruning_horizon: u64) -> Result<(), ChainStorageError> {
    let mut txn = DbTransaction::new();
    txn.set_metadata(
        MetadataKey::PruningHorizon,
        MetadataValue::PruningHorizon(pruning_horizon),
    );
    commit(db, txn)
}

fn fetch_block<T: BlockchainBackend>(db: &T, height: u64) -> Result<HistoricalBlock, ChainStorageError> {
    let mark = Instant::now();
    let tip_height = check_for_valid_height(&*db, height)?;
    let header = fetch_header(db, height)?;
    let header_hash = header.hash();
    let kernels = db.fetch_kernels_in_block(&header_hash)?;
    let outputs = db.fetch_outputs_in_block(&header_hash)?;
    let inputs = db.fetch_inputs_in_block(&header_hash)?;
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
    Ok(HistoricalBlock::new(block, tip_height - height + 1))
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

fn fetch_block_with_hash<T: BlockchainBackend>(
    db: &T,
    hash: BlockHash,
) -> Result<Option<HistoricalBlock>, ChainStorageError>
{
    if let Some(header) = fetch_header_by_block_hash(db, hash.clone())? {
        return Ok(Some(fetch_block(db, header.height)?));
    }
    if let Ok(block) = fetch_orphan(db, hash) {
        return Ok(Some(HistoricalBlock::new(block, 0)));
    }
    Ok(None)
}

fn check_for_valid_height<T: BlockchainBackend>(db: &T, height: u64) -> Result<u64, ChainStorageError> {
    let metadata = db.fetch_chain_metadata()?;
    let db_height = metadata.height_of_longest_chain();
    if height > db_height {
        return Err(ChainStorageError::InvalidQuery(format!(
            "Cannot get block at height {}. Chain tip is at {}",
            height, db_height
        )));
    }
    let horizon_height = metadata.horizon_block(db_height);
    if height < horizon_height {
        return Err(ChainStorageError::InvalidQuery(format!(
            "Cannot get block at height {}. Horizon height is at {}",
            height, horizon_height
        )));
    }
    Ok(db_height)
}

#[inline]
fn commit<T: BlockchainBackend>(db: &mut T, txn: DbTransaction) -> Result<(), ChainStorageError> {
    db.write(txn)
}

fn rewind_to_height<T: BlockchainBackend>(db: &mut T, height: u64) -> Result<Vec<Arc<Block>>, ChainStorageError> {
    let chain_height = check_for_valid_height(db, height)?;
    if height == chain_height {
        return Ok(Vec::new()); // Rewind unnecessary, already on correct height
    }
    debug!(
        target: LOG_TARGET,
        "Rewinding from height {} to {}", chain_height, height
    );
    let steps_back = (chain_height - height) as usize;
    let mut removed_blocks = Vec::with_capacity(steps_back);
    let mut txn = DbTransaction::new();
    // Rewind operation must be performed in reverse from tip to height+1.
    for rewind_height in ((height + 1)..=chain_height).rev() {
        // Reconstruct block at height and add to orphan block pool
        let orphaned_block = Arc::new(fetch_block(db, rewind_height)?.into_block());
        removed_blocks.push(orphaned_block.clone());
        txn.delete_block(orphaned_block.hash());
        txn.insert_orphan(orphaned_block);
        txn.delete(DbKey::BlockHeader(rewind_height));
    }
    // Update metadata
    let last_header = fetch_header(db, height)?;
    let accumulated_work =
        ProofOfWork::new_from_difficulty(&last_header.pow, ProofOfWork::achieved_difficulty(&last_header)?)
            .total_accumulated_difficulty();
    txn.set_metadata(MetadataKey::ChainHeight, MetadataValue::ChainHeight(last_header.height));
    txn.set_metadata(MetadataKey::BestBlock, MetadataValue::BestBlock(last_header.hash()));
    txn.set_metadata(
        MetadataKey::AccumulatedWork,
        MetadataValue::AccumulatedWork(accumulated_work),
    );
    commit(db, txn)?;

    Ok(removed_blocks)
}

// Checks whether we should add the block as an orphan. If it is the case, the orphan block is added and the chain
// is reorganised if necessary.
fn handle_possible_reorg<T: BlockchainBackend>(
    db: &mut T,
    block_validator: &StatefulValidator<Block, T>,
    chain_strength_comparer: &dyn ChainStrengthComparer,
    new_block: Arc<Block>,
) -> Result<BlockAddResult, ChainStorageError>
{
    let db_height = db.fetch_chain_metadata()?.height_of_longest_chain();

    let new_tips = insert_orphan_and_find_new_tips(db, new_block.clone())?;
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
    let orphan_chain_tips = db.fetch_orphan_chain_tips()?;
    trace!(
        target: LOG_TARGET,
        "Search for orphan tips linked to block #{} complete. {} tips found",
        new_block.header.height,
        orphan_chain_tips.len()
    );
    // Check the accumulated difficulty of the best fork chain compared to the main chain.
    let fork_header = find_strongest_orphan_tip(db, orphan_chain_tips, chain_strength_comparer)?;
    if fork_header.is_none() {
        // This should never happen because a block is always added to the orphan pool before
        // checking, but just in case
        return Err(ChainStorageError::InvalidOperation(
            "No chain tips found in orphan pool".to_string(),
        ));
    }
    let fork_header = fork_header.unwrap();
    let fork_tip_hash = fork_header.hash();

    let tip_header = db.fetch_last_header()?;
    if fork_tip_hash == new_block_hash {
        debug!(
            target: LOG_TARGET,
            "Comparing candidate block #{} (accum_diff:{}, hash:{}) to main chain #{} (accum_diff: {}, hash: ({})).",
            new_block.header.height,
            fork_header.total_accumulated_difficulty_inclusive_squared()?,
            fork_tip_hash.to_hex(),
            tip_header.height,
            tip_header.total_accumulated_difficulty_inclusive_squared()?,
            tip_header.hash().to_hex()
        );
    } else {
        debug!(
            target: LOG_TARGET,
            "Comparing fork (accum_diff:{}, hash:{}) with block #{} ({}) to main chain #{} (accum_diff: {}, hash: \
             ({})).",
            fork_header.total_accumulated_difficulty_inclusive_squared()?,
            fork_tip_hash.to_hex(),
            new_block.header.height,
            new_block_hash.to_hex(),
            tip_header.height,
            tip_header.total_accumulated_difficulty_inclusive_squared()?,
            tip_header.hash().to_hex()
        );
    }

    match chain_strength_comparer.compare(&fork_header, &tip_header) {
        Ordering::Greater => {
            debug!(
                target: LOG_TARGET,
                "Accumulated difficulty validation PASSED for block #{} ({})",
                new_block.header.height,
                new_block_hash.to_hex()
            );
        },
        Ordering::Less | Ordering::Equal => {
            debug!(
                target: LOG_TARGET,
                "Fork chain (accum_diff:{}, hash:{}) with block {} ({}) has a weaker difficulty.",
                fork_header.total_accumulated_difficulty_inclusive_squared()?,
                fork_tip_hash.to_hex(),
                new_block.header.height,
                new_block_hash.to_hex(),
            );
            debug!(
                target: LOG_TARGET,
                "Orphan block received: #{}", new_block.header.height
            );
            return Ok(BlockAddResult::OrphanBlock);
        },
    }

    // We've built the strongest orphan chain we can by going backwards and forwards from the new orphan block
    // that is linked with the main chain.
    // New block is not the tip, find complete chain from tip to main chain.
    let reorg_chain = get_orphan_link_main_chain(db, &fork_tip_hash)?;
    // }
    let added_blocks = reorg_chain.iter().cloned().collect::<Vec<_>>();
    let fork_height = reorg_chain
        .front()
        .expect("The new orphan block should be in the queue")
        .header
        .height -
        1;
    let removed_blocks = reorganize_chain(db, block_validator, fork_height, reorg_chain)?;
    let num_removed_blocks = removed_blocks.len();
    let num_added_blocks = added_blocks.len();

    // reorg is required when any blocks are removed or more than one are added
    // see https://github.com/tari-project/tari/issues/2101
    if num_removed_blocks > 0 || num_added_blocks > 1 {
        info!(
            target: LOG_TARGET,
            "Chain reorg required from {} to {} (accum_diff:{}, hash:{}) to (accum_diff:{}, hash:{}). Number of \
             blocks to remove: {}, to add: {}.
            ",
            tip_header,
            fork_header,
            tip_header.pow,
            tip_header.hash().to_hex(),
            fork_header.pow,
            fork_tip_hash.to_hex(),
            num_removed_blocks,
            num_added_blocks,
        );
        Ok(BlockAddResult::ChainReorg(removed_blocks, added_blocks))
    } else {
        trace!(
            target: LOG_TARGET,
            "No reorg required. Number of blocks to remove: {}, to add: {}.",
            num_removed_blocks,
            num_added_blocks,
        );
        Ok(BlockAddResult::Ok)
    }
}

// Reorganize the main chain with the provided fork chain, starting at the specified height.
fn reorganize_chain<T: BlockchainBackend>(
    backend: &mut T,
    block_validator: &StatefulValidator<Block, T>,
    height: u64,
    chain: VecDeque<Arc<Block>>,
) -> Result<Vec<Arc<Block>>, ChainStorageError>
{
    let removed_blocks = rewind_to_height(backend, height)?;
    debug!(
        target: LOG_TARGET,
        "Validate and add {} chain block(s) from height {}. Rewound blocks: [{}]",
        chain.len(),
        height,
        removed_blocks
            .iter()
            .map(|b| b.header.height.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );

    for block in chain {
        let mut txn = DbTransaction::new();
        let block_hash = block.hash();
        let block_hash_hex = block_hash.to_hex();
        txn.delete(DbKey::OrphanBlock(block_hash));
        if let Err(e) = block_validator.validate(&block, backend) {
            warn!(
                target: LOG_TARGET,
                "Orphan block {} ({}) failed validation during chain reorg: {}", block.header.height, block_hash_hex, e
            );
            remove_orphan(backend, block.hash())?;

            info!(target: LOG_TARGET, "Restoring previous chain after failed reorg.");
            restore_reorged_chain(backend, height, removed_blocks)?;
            return Err(e.into());
        }

        store_new_block(backend, &mut txn, unwrap_or_clone(block))?;
        // Failed to store the block - this should typically never happen unless there is a bug in the validator
        // (e.g. does not catch a double spend). In any case, we still need to restore the chain to a
        // good state before returning.
        if let Err(e) = commit(backend, txn) {
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
    previous_chain: Vec<Arc<Block>>,
) -> Result<(), ChainStorageError>
{
    let invalid_chain = rewind_to_height(db, height)?;
    debug!(
        target: LOG_TARGET,
        "Removed {} blocks during chain restore: {:?}.",
        invalid_chain.len(),
        invalid_chain
            .iter()
            .map(|block| block.hash().to_hex())
            .collect::<Vec<_>>(),
    );
    let mut txn = DbTransaction::new();
    // Add removed blocks in the reverse order that they were removed
    // See: https://github.com/tari-project/tari/issues/2182
    for block in previous_chain.into_iter().rev() {
        txn.delete(DbKey::OrphanBlock(block.hash()));
        store_new_block(db, &mut txn, unwrap_or_clone(block))?;
    }
    commit(db, txn)?;
    Ok(())
}

// Insert the provided block into the orphan pool and returns any new tips that were created
fn insert_orphan_and_find_new_tips<T: BlockchainBackend>(
    db: &mut T,
    block: Arc<Block>,
) -> Result<Vec<HashOutput>, ChainStorageError>
{
    let hash = block.hash();

    let mut txn = DbTransaction::new();
    txn.insert_orphan(block.clone());

    let mut new_tips_found = vec![];
    let tips = db.fetch_orphan_chain_tips()?;
    if tips.contains(&block.header.prev_hash) {
        // Extend tip
        txn.remove_orphan_chain_tip(block.header.prev_hash.clone());

        for new_tip in find_orphan_descendant_tips_of(&*db, hash)? {
            txn.insert_orphan_chain_tip(new_tip.clone());
            new_tips_found.push(new_tip);
        }
    } else {
        // Find in connected
        let best_chain_connection = fetch_header_by_block_hash(&*db, block.header.prev_hash.clone())?;
        if let Some(connected) = best_chain_connection {
            debug!(
                target: LOG_TARGET,
                "New orphan connects to existing chain at height: {}", connected.height
            );
            for new_tip in find_orphan_descendant_tips_of(&*db, hash)? {
                txn.insert_orphan_chain_tip(new_tip.clone());
                new_tips_found.push(new_tip);
            }
        } else {
            debug!(
                target: LOG_TARGET,
                "Orphan {} was not connected to any previous headers. Inserting as true orphan",
                hash.to_hex()
            );
        }
    }

    commit(db, txn)?;
    Ok(new_tips_found)
}

// Find the tip set of any orphans that have hash as an ancestor
fn find_orphan_descendant_tips_of<T: BlockchainBackend>(
    db: &T,
    hash: HashOutput,
) -> Result<Vec<HashOutput>, ChainStorageError>
{
    let children = db.fetch_orphan_children_of(hash.clone())?;
    if children.is_empty() {
        return Ok(vec![hash]);
    }
    let mut res = vec![];
    for child in children {
        res.extend(find_orphan_descendant_tips_of(db, child)?);
    }
    Ok(res)
}

// Discard the the orphan block from the orphan pool that corresponds to the provided block hash.
fn remove_orphan<T: BlockchainBackend>(db: &mut T, hash: HashOutput) -> Result<(), ChainStorageError> {
    let mut txn = DbTransaction::new();
    txn.delete(DbKey::OrphanBlock(hash));
    commit(db, txn)
}

/// Gets all blocks from the orphan to the point where it connects to the best chain
#[allow(clippy::ptr_arg)]
fn get_orphan_link_main_chain<T: BlockchainBackend>(
    db: &mut T,
    orphan_tip: &HashOutput,
) -> Result<VecDeque<Arc<Block>>, ChainStorageError>
{
    let mut chain: VecDeque<Arc<Block>> = VecDeque::new();
    let mut curr_hash = orphan_tip.to_owned();
    loop {
        let curr_block = fetch!(db, curr_hash, OrphanBlock)?;
        curr_hash = curr_block.header.prev_hash.clone();
        chain.push_front(Arc::new(curr_block));

        if db.contains(&DbKey::BlockHash(curr_hash.clone()))? {
            break;
        }
    }
    Ok(chain)
}
/// Find and return the orphan chain tip with the highest accumulated difficulty.
fn find_strongest_orphan_tip<T: BlockchainBackend>(
    db: &T,
    orphan_chain_tips: Vec<BlockHash>,
    chain_strength_comparer: &dyn ChainStrengthComparer,
) -> Result<Option<BlockHeader>, ChainStorageError>
{
    let mut best_block_header: Option<BlockHeader> = None;
    for tip_hash in orphan_chain_tips {
        let header = fetch_orphan(db, tip_hash.clone())?.header;
        best_block_header = match best_block_header {
            Some(current_best) => match chain_strength_comparer.compare(&current_best, &header) {
                Ordering::Less => Some(header),
                Ordering::Greater | Ordering::Equal => Some(current_best),
            },
            None => Some(header),
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

fn prune_database<T: BlockchainBackend>(
    db: &mut T,
    _pruning_height_interval: u64,
    _pruning_horizon: u64,
    _height: u64,
) -> Result<(), ChainStorageError>
{
    let metadata = db.fetch_chain_metadata()?;
    if metadata.is_pruned_node() {
        unimplemented!();
        //     let db_height = metadata.height_of_longest_chain();
        //     if db_height % pruning_height_interval == 0 {
        //         info!(target: LOG_TARGET, "Pruning interval reached. Pruning the database.");
        //         let abs_pruning_horizon = height.saturating_sub(pruning_horizon);
        //
        //         let mut txn = DbTransaction::new();
        //         let max_cp_count = pruning_horizon + 1; // Include accumulated checkpoint
        //         unimplemented!()
        //        // txn.merge_checkpoints(max_cp_count as usize);
        //
        //         if abs_pruning_horizon > metadata.effective_pruned_height {
        //             txn.set_metadata(
        //                 MetadataKey::EffectivePrunedHeight,
        //                 MetadataValue::EffectivePrunedHeight(abs_pruning_horizon),
        //             );
        //         }
        //         commit(db, txn)?;
        //     }
    }

    Ok(())
}

fn get_horizon_sync_state<T: BlockchainBackend>(
    db: &T,
) -> Result<Option<InProgressHorizonSyncState>, ChainStorageError> {
    match db.fetch(&DbKey::Metadata(MetadataKey::HorizonSyncState))? {
        Some(DbValue::Metadata(MetadataValue::HorizonSyncState(val))) => Ok(Some(val)),
        _ => Ok(None),
    }
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

fn unwrap_or_clone<T: Clone>(arc: Arc<T>) -> T {
    Arc::try_unwrap(arc)
        .or_else(|t| Result::<_, Infallible>::Ok(Clone::clone(&*t)))
        .expect("or_else with Ok")
}
