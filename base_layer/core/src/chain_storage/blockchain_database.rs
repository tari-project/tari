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
    blocks::{blockheader::BlockHash, Block, BlockHeader, NewBlockTemplate},
    chain_storage::{
        consts::{
            BLOCKCHAIN_DATABASE_ORPHAN_STORAGE_CAPACITY,
            BLOCKCHAIN_DATABASE_PRUNED_MODE_CLEANUP_INTERVAL,
            BLOCKCHAIN_DATABASE_PRUNING_HORIZON,
        },
        db_transaction::{DbKey, DbValue, MmrTree},
        error::ChainStorageError,
        ChainMetadata,
        HistoricalBlock,
        MetadataKey,
    },
    consensus::ConsensusManager,
    proof_of_work::{Difficulty, PowAlgorithm, ProofOfWork},
    transactions::{
        transaction::{TransactionInput, TransactionKernel, TransactionOutput},
        types::{Commitment, HashOutput},
    },
    validation::{StatelessValidation, StatelessValidator, Validation, ValidationError, Validator},
};
use croaring::Bitmap;
use log::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard},
};
use strum_macros::Display;
use tari_crypto::tari_utilities::{epoch_time::EpochTime, hex::Hex, Hashable};
use tari_mmr::{Hash, MutableMmrLeafNodes};

const LOG_TARGET: &str = "c::cs::database";

/// Configuration for the BlockchainDatabase.
#[derive(Clone, Copy)]
pub struct BlockchainDatabaseConfig {
    pub orphan_storage_capacity: usize,
    pub pruning_horizon: u64,
    pub pruned_mode_cleanup_interval: u64,
}

impl Default for BlockchainDatabaseConfig {
    fn default() -> Self {
        Self {
            orphan_storage_capacity: BLOCKCHAIN_DATABASE_ORPHAN_STORAGE_CAPACITY,
            pruning_horizon: BLOCKCHAIN_DATABASE_PRUNING_HORIZON,
            pruned_mode_cleanup_interval: BLOCKCHAIN_DATABASE_PRUNED_MODE_CLEANUP_INTERVAL,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Display)]
pub enum BlockAddResult {
    Ok,
    BlockExists,
    OrphanBlock,
    ChainReorg((Box<Vec<Block>>, Box<Vec<Block>>)), // Set of removed blocks and set of added blocks
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
    block: Arc<Validator<Block, B>>,
    orphan: Arc<StatelessValidator<Block>>,
    accum_difficulty: Arc<Validator<Difficulty, B>>,
}

impl<B: BlockchainBackend> Validators<B> {
    pub fn new(
        block: impl Validation<Block, B> + 'static,
        orphan: impl StatelessValidation<Block> + 'static,
        accum_difficulty: impl Validation<Difficulty, B> + 'static,
    ) -> Self
    {
        Self {
            block: Arc::new(Box::new(block)),
            orphan: Arc::new(Box::new(orphan)),
            accum_difficulty: Arc::new(Box::new(accum_difficulty)),
        }
    }
}

impl<B> Clone for Validators<B> {
    fn clone(&self) -> Self {
        Validators {
            block: Arc::clone(&self.block),
            orphan: Arc::clone(&self.orphan),
            accum_difficulty: Arc::clone(&self.accum_difficulty),
        }
    }
}

/// Identify behaviour for Blockchain database back ends. Implementations must support `Send` and `Sync` so that
/// `BlockchainDatabase` can be thread-safe. The backend *must* also execute transactions atomically; i.e., every
/// operation within it must succeed, or they all fail. Failure to support this contract could lead to
/// synchronisation issues in your database backend.
///
/// This trait is written so that every it is assumed that every function call should be atomic, e.i. if some part of
/// the instruction fails, the entire call should be reversed
pub trait BlockchainBackend: Send + Sync {
    /// Adds a block to the orphan database. This function assumes that the stateless validation has passed on the
    /// block.
    fn add_orphan_block(&mut self, block: Block) -> Result<(), ChainStorageError>;
    /// This function will move a block from the orphan pool to the main chain. It assumes that the block has passed
    /// full stateful validation. It will spend all inputs and store utxo, headers and kernels. It will also update the
    /// mmr's
    fn accept_block(&mut self, block_hash: HashOutput) -> Result<(), ChainStorageError>;
    // rewinds the database to the specified height. It will move every block that was rewound to the orphan pool
    // This will return the hashes of every block that was moved to the orphan pool
    fn rewind_to_height(&mut self, height: u64) -> Result<Vec<BlockHeader>, ChainStorageError>;
    /// This is used when synchronising. Adds in the list of headers provided to the main chain
    fn add_block_headers(&mut self, headers: Vec<BlockHeader>) -> Result<(), ChainStorageError>;
    /// This is used when synchronising. Adds in the list of kernels provided to the main chain
    fn add_kernels(&mut self, kernels: Vec<TransactionKernel>) -> Result<(), ChainStorageError>;
    /// This is used when synchronising. Adds in the list of utxos provided to the main chain
    fn add_utxos(&mut self, utxos: Vec<TransactionOutput>) -> Result<(), ChainStorageError>;
    /// This is used when synchronising. Adds in the list of mmr leafs provided to the main chain
    fn add_mmr(&mut self, tree: MmrTree, hashes: Vec<HashOutput>) -> Result<(), ChainStorageError>;
    /// This function will force the chain_meta_data inside the database to a certain value
    fn force_meta_data(&mut self, metadata: ChainMetadata) -> Result<(), ChainStorageError>;
    /// This function is used to remove orphan blocks
    /// This function will return ok if it did not encounter an error. If a orphan block was not found, it should return
    /// Ok(false)
    fn remove_orphan_blocks(&mut self, block_hashes: Vec<BlockHash>) -> Result<bool, ChainStorageError>;
    /// Fetch a value from the back end corresponding to the given key. If the value is not found, `get` must return
    /// `Ok(None)`. It should only error if there is an access or integrity issue with the underlying back end.
    fn fetch(&self, key: &DbKey) -> Result<Option<DbValue>, ChainStorageError>;
    /// Checks to see whether the given key exists in the back end. This function should only fail if there is an
    /// access or integrity issue with the back end.
    fn contains(&self, key: &DbKey) -> Result<bool, ChainStorageError>;
    // DELETE? fn fetch_mmr_root(&self, tree: MmrTree) -> Result<HashOutput, ChainStorageError>;
    /// Returns only the MMR merkle root without the state of the roaring bitmap.
    // DELETE? fn fetch_mmr_only_root(&self, tree: MmrTree) -> Result<HashOutput, ChainStorageError>;
    /// Fetches the merklish root for the MMR tree identified by the key after the current additions and deletions have
    /// temporarily been applied. Deletions of hashes from the MMR can only be applied for UTXOs.
    fn calculate_mmr_root(
        &self,
        tree: MmrTree,
        additions: Vec<HashOutput>,
        deletions: Vec<HashOutput>,
    ) -> Result<HashOutput, ChainStorageError>;
    /// Constructs a merkle proof for the specified merkle mountain range and the given leaf position.
    // DELETE? fn fetch_mmr_proof(&self, tree: MmrTree, pos: usize) -> Result<MerkleProof, ChainStorageError>;
    /// Fetches the total merkle mountain range node count upto the specified height.
    fn fetch_mmr_node_count(&self, tree: MmrTree, height: u64) -> Result<u32, ChainStorageError>;
    /// Fetches the leaf node hash and its deletion status for the nth leaf node in the given MMR tree.
    // delete, can use function below with count 1 fn fetch_mmr_node(&self, tree: MmrTree, pos: u32) -> Result<(Hash,
    // bool), ChainStorageError>;
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
    fn delete_mmr_node(&mut self, tree: MmrTree, hash: &Hash) -> Result<(), ChainStorageError>;
    /// Fetches the leaf index of the provided leaf node hash in the given MMR tree.
    fn fetch_mmr_leaf_index(&self, tree: MmrTree, hash: &Hash) -> Result<Option<u32>, ChainStorageError>;
    /// Performs the function F for each orphan block in the orphan pool.
    // replace with 2 fn below. // fn for_each_orphan<F>(&self, f: F) -> Result<(), ChainStorageError>
    // where
    //     Self: Sized,
    //     F: FnMut(Result<(HashOutput, Block), ChainStorageError>);
    /// returns a list of orphan block headers that are parents to the named hash
    fn fetch_parent_orphan_headers(&self, hash: HashOutput, height: u64)
        -> Result<Vec<BlockHeader>, ChainStorageError>;
    /// Returns a list of all orphan block headers
    fn fetch_all_orphan_headers(&self) -> Result<Vec<BlockHeader>, ChainStorageError>;
    /// Returns the number of blocks in the block orphan pool.
    fn get_orphan_count(&self) -> Result<usize, ChainStorageError>;
    /// Performs the function F for each transaction kernel.
    // DELETE? not used// fn for_each_kernel<F>(&self, f: F) -> Result<(), ChainStorageError>
    // where
    //     Self: Sized,
    //     F: FnMut(Result<(HashOutput, TransactionKernel), ChainStorageError>);
    /// Performs the function F for each block header.
    // DELETE? not used// fn for_each_header<F>(&self, f: F) -> Result<(), ChainStorageError>
    // where
    //     Self: Sized,
    //     F: FnMut(Result<(u64, BlockHeader), ChainStorageError>);
    /// Performs the function F for each UTXO.
    // DELETE? not used// fn for_each_utxo<F>(&self, f: F) -> Result<(), ChainStorageError>
    // where
    //     Self: Sized,
    //     F: FnMut(Result<(HashOutput, TransactionOutput), ChainStorageError>);
    /// Returns the stored header with the highest corresponding height on the main chain.
    fn fetch_last_header(&self) -> Result<Option<BlockHeader>, ChainStorageError>;
    /// Returns the stored chain metadata.
    fn fetch_metadata(&self) -> Result<ChainMetadata, ChainStorageError>;
    /// Returns the set of target difficulties for the specified proof of work algorithm.
    fn fetch_target_difficulties(
        &self,
        pow_algo: PowAlgorithm,
        height: u64,
        block_window: usize,
    ) -> Result<Vec<(EpochTime, Difficulty)>, ChainStorageError>;
}

// Private macro that pulls out all the boiler plate of extracting a DB query result from its variants
macro_rules! fetch {
    ($db:ident, $key_val:expr, $key_var:ident) => {{
        let key = DbKey::$key_var($key_val);
        match $db.fetch(&key) {
            Ok(None) => Err(ChainStorageError::ValueNotFound(key)),
            Ok(Some(DbValue::$key_var(k))) => Ok(*k),
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
///     validation::{accum_difficulty_validators::AccumDifficultyValidator, mocks::MockValidator, Validation},
/// };
/// let db_backend = MemoryDatabase::<HashDigest>::default();
/// let validators = Validators::new(
///     MockValidator::new(true),
///     MockValidator::new(true),
///     AccumDifficultyValidator {},
/// );
/// let db = MemoryDatabase::<HashDigest>::default();
/// let network = Network::LocalNet;
/// let rules = ConsensusManagerBuilder::new(network).build();
/// let db = BlockchainDatabase::new(db_backend, &rules, validators, BlockchainDatabaseConfig::default()).unwrap();
/// // Do stuff with db
/// ```
pub struct BlockchainDatabase<T> {
    db: Arc<RwLock<T>>,
    validators: Validators<T>,
    config: BlockchainDatabaseConfig,
}

impl<T> BlockchainDatabase<T>
where T: BlockchainBackend
{
    /// Creates a new `BlockchainDatabase` using the provided backend.
    pub fn new(
        db: T,
        consensus_manager: &ConsensusManager,
        validators: Validators<T>,
        config: BlockchainDatabaseConfig,
    ) -> Result<Self, ChainStorageError>
    {
        let blockchain_db = BlockchainDatabase {
            db: Arc::new(RwLock::new(db)),
            validators,
            config,
        };
        let metadata = blockchain_db.get_metadata()?;
        if metadata.height_of_longest_chain.is_none() {
            let genesis_block = consensus_manager.get_genesis_block();
            blockchain_db.store_new_block(genesis_block)?;
        // blockchain_db.store_pruning_horizon(config.pruning_horizon)?;
        } else if (metadata.is_archival_node() && (config.pruning_horizon != metadata.pruning_horizon)) ||
            (metadata.is_pruned_node() && (config.pruning_horizon < metadata.pruning_horizon))
        {
            debug!(
                target: LOG_TARGET,
                "Updating pruning horizon from {} to {}.", metadata.pruning_horizon, config.pruning_horizon,
            );
            // blockchain_db.store_pruning_horizon(config.pruning_horizon)?;
        }
        Ok(blockchain_db)
    }

    // Be careful about making this method public. Rather use `db_and_metadata_read_access`
    // so that metadata and db are read in the correct order so that deadlocks don't occur
    pub fn db_read_access(&self) -> Result<RwLockReadGuard<T>, ChainStorageError> {
        self.db.read().map_err(|e| {
            error!(
                target: LOG_TARGET,
                "An attempt to get a read lock on the blockchain backend failed. {:?}", e
            );
            ChainStorageError::AccessError("Read lock on blockchain backend failed".into())
        })
    }

    pub fn db_write_access(&self) -> Result<RwLockWriteGuard<T>, ChainStorageError> {
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
    pub fn get_height(&self) -> Result<Option<u64>, ChainStorageError> {
        let db = self.db_read_access()?;
        Ok(db.fetch_metadata()?.height_of_longest_chain)
    }

    /// Return the geometric mean of the proof of work of the longest chain.
    /// The proof of work is returned as the geometric mean of all difficulties
    pub fn get_accumulated_difficulty(&self) -> Result<Option<Difficulty>, ChainStorageError> {
        let db = self.db_read_access()?;
        Ok(db.fetch_metadata()?.accumulated_difficulty)
    }

    /// Returns a copy of the current blockchain database metadata
    pub fn get_metadata(&self) -> Result<ChainMetadata, ChainStorageError> {
        let db = self.db_read_access()?;
        db.fetch_metadata()
    }

    /// Sets the stored chain metadata to the provided metadata.
    pub fn write_metadata(&self, metadata: ChainMetadata) -> Result<(), ChainStorageError> {
        let mut db = self.db_write_access()?;
        write_metadata(&mut db, metadata)
    }

    /// Returns the transaction kernel with the given hash.
    pub fn fetch_kernel(&self, hash: HashOutput) -> Result<TransactionKernel, ChainStorageError> {
        let db = self.db_read_access()?;
        fetch_kernel(&*db, hash)
    }

    /// Returns the set of transaction kernels with the given hashes.
    pub fn fetch_kernels(&self, hashes: Vec<HashOutput>) -> Result<Vec<TransactionKernel>, ChainStorageError> {
        let db = self.db_read_access()?;
        fetch_kernels(&*db, hashes)
    }

    /// Returns the block header at the given block height.
    pub fn fetch_header(&self, block_num: u64) -> Result<BlockHeader, ChainStorageError> {
        let db = self.db_read_access()?;
        fetch_header(&*db, block_num)
    }

    /// Returns the set of block headers specified by the block numbers.
    pub fn fetch_headers(&self, block_nums: Vec<u64>) -> Result<Vec<BlockHeader>, ChainStorageError> {
        let db = self.db_read_access()?;
        fetch_headers(&*db, block_nums)
    }

    /// Returns the block header corresponding` to the provided BlockHash
    pub fn fetch_header_with_block_hash(&self, hash: HashOutput) -> Result<BlockHeader, ChainStorageError> {
        let db = self.db_read_access()?;
        fetch_header_with_block_hash(&*db, hash)
    }

    pub fn fetch_tip_header(&self) -> Result<BlockHeader, ChainStorageError> {
        let db = self.db_read_access()?;
        fetch_tip_header(&*db)
    }

    /// Returns the UTXO with the given hash.
    pub fn fetch_utxo(&self, hash: HashOutput) -> Result<TransactionOutput, ChainStorageError> {
        let db = self.db_read_access()?;
        fetch_utxo(&*db, hash)
    }

    /// Returns the STXO with the given hash.
    pub fn fetch_stxo(&self, hash: HashOutput) -> Result<TransactionOutput, ChainStorageError> {
        let db = self.db_read_access()?;
        fetch_stxo(&*db, hash)
    }

    /// Returns the STXO with the given hash.
    pub fn is_stxo(&self, hash: HashOutput) -> Result<bool, ChainStorageError> {
        let db = self.db_read_access()?;
        is_stxo(&*db, hash)
    }

    /// Returns the UTXO or STXO with the given hash, it will return none if not found.
    pub fn fetch_txo(&self, hash: HashOutput) -> Result<Option<TransactionOutput>, ChainStorageError> {
        let db = self.db_read_access()?;
        fetch_txo(&*db, hash)
    }

    /// Returns the orphan block with the given hash.
    pub fn fetch_orphan(&self, hash: HashOutput) -> Result<Block, ChainStorageError> {
        let db = self.db_read_access()?;
        fetch_orphan(&*db, hash)
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
        fetch_target_difficulties(&*db, pow_algo, height, block_window)
    }

    /// Returns true if the given UTXO, represented by its hash exists in the UTXO set.
    pub fn is_utxo(&self, hash: HashOutput) -> Result<bool, ChainStorageError> {
        let db = self.db_read_access()?;
        is_utxo(&*db, hash)
    }

    // /// Calculate the Merklish root of the specified merkle mountain range.
    // pub fn fetch_mmr_root(&self, tree: MmrTree) -> Result<HashOutput, ChainStorageError> {
    //     let db = self.db_read_access()?;
    //     fetch_mmr_root(&*db, tree)
    // }

    // /// Returns only the MMR merkle root without the state of the roaring bitmap.
    // pub fn fetch_mmr_only_root(&self, tree: MmrTree) -> Result<HashOutput, ChainStorageError> {
    //     let db = self.db_read_access()?;
    //     fetch_mmr_only_root(&*db, tree)
    // }

    /// Apply the current change set to a pruned copy of the merkle mountain range and calculate the resulting Merklish
    /// root of the specified merkle mountain range. Deletions of hashes from the MMR can only be applied for UTXOs.
    pub fn calculate_mmr_root(
        &self,
        tree: MmrTree,
        additions: Vec<HashOutput>,
        deletions: Vec<HashOutput>,
    ) -> Result<HashOutput, ChainStorageError>
    {
        let db = self.db_read_access()?;
        db.calculate_mmr_root(tree, additions, deletions)
    }

    /// `calculate_mmr_roots` takes a block template and calculates the MMR roots for a hypothetical new block that
    /// would be built onto the chain tip. Note that _no checks_ are made to determine whether the template would
    /// actually be a valid extension to the chain; only the new MMR roots are calculated
    pub fn calculate_mmr_roots(&self, template: NewBlockTemplate) -> Result<Block, ChainStorageError> {
        let db = self.db_read_access()?;
        calculate_mmr_roots(&*db, template)
    }

    /// Fetch a Merklish proof for the given hash, tree and position in the MMR
    // pub fn fetch_mmr_proof(&self, tree: MmrTree, pos: usize) -> Result<MerkleProof, ChainStorageError> {
    //     let db = self.db_read_access()?;
    //     fetch_mmr_proof(&*db, tree, pos)
    // }

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
    ) -> Result<Vec<(Vec<u8>, bool)>, ChainStorageError>
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
    pub fn add_block(&self, block: Block) -> Result<BlockAddResult, ChainStorageError> {
        // Perform orphan block validation.
        // lets check orphan validation time
        // Todo  move this down after exist check as this can be slow
        if let Err(e) = self.validators.orphan.validate(&block) {
            warn!(
                target: LOG_TARGET,
                "Block #{} ({}) failed validation - {}",
                block.header.height,
                block.hash().to_hex(),
                e.to_string()
            );
            return Err(e.into());
        }
        let mut db = self.db_write_access()?;
        let block_add_result = add_block(
            &mut db,
            &self.validators.block,
            &self.validators.accum_difficulty,
            block,
        )?;

        // Cleanup orphan block pool
        match block_add_result {
            BlockAddResult::OrphanBlock | BlockAddResult::ChainReorg(_) => {
                cleanup_orphans(&mut db, self.config.orphan_storage_capacity)?
            },
            _ => {},
        }

        // Cleanup of backend when in pruned mode.
        match block_add_result {
            BlockAddResult::Ok | BlockAddResult::ChainReorg(_) => cleanup_pruned_mode(
                &mut db,
                self.config.pruned_mode_cleanup_interval,
                self.config.pruning_horizon,
            )?,
            _ => {},
        }

        Ok(block_add_result)
    }

    fn store_new_block(&self, block: Block) -> Result<(), ChainStorageError> {
        let mut db = self.db_write_access()?;
        let hash = block.hash();
        db.add_orphan_block(block)?;
        db.accept_block(hash)
    }

    // fn store_pruning_horizon(&self, pruning_horizon: u64) -> Result<(), ChainStorageError> {
    //     let mut db = self.db_write_access()?;
    //     store_pruning_horizon(&mut db, pruning_horizon)
    // }

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
    pub fn fetch_block_with_height(&self, height: u64) -> Result<HistoricalBlock, ChainStorageError> {
        let db = self.db_read_access()?;
        fetch_historic_block(&*db, height)
    }

    /// Attempt to fetch the block corresponding to the provided hash from the main chain, if it cannot be found then
    /// the block will be searched in the orphan block pool.
    pub fn fetch_block_with_hash(&self, hash: BlockHash) -> Result<Option<HistoricalBlock>, ChainStorageError> {
        let db = self.db_read_access()?;
        fetch_block_with_hash(&*db, hash)
    }

    /// Returns true if this block exists in the chain, or is orphaned.
    pub fn block_exists(&self, hash: BlockHash) -> Result<bool, ChainStorageError> {
        let db = self.db_read_access()?;
        block_exists(&*db, hash)
    }

    /// Atomically commit the provided transaction to the database backend. This function does not update the metadata.
    // pub fn commit(&self, txn: DbTransaction) -> Result<(), ChainStorageError> {
    //     let mut db = self.db_write_access()?;
    //     commit(&mut db, txn)
    // }

    /// Rewind the blockchain state to the block height given and return the blocks that were removed and orphaned.
    ///
    /// The operation will fail if
    /// * The block height is in the future
    /// * The block height is before the horizon block height determined by the pruning horizon
    pub fn rewind_to_height(&self, height: u64) -> Result<Vec<BlockHeader>, ChainStorageError> {
        let mut db = self.db_write_access()?;
        db.rewind_to_height(height)
    }

    /// Commit the current synced horizon state.
    pub fn commit_horizon_state(&self) -> Result<(), ChainStorageError> {
        let mut db = self.db_write_access()?;
        commit_horizon_state(&mut db)
    }

    /// This is used when synchronising. Adds in the list of headers provided to the main chain
    pub fn add_block_headers(&self, headers: Vec<BlockHeader>) -> Result<(), ChainStorageError> {
        let mut db = self.db_write_access()?;
        db.add_block_headers(headers)
    }

    /// This is used when synchronising. Adds in the list of kernels provided to the main chain
    pub fn add_kernels(&self, kernels: Vec<TransactionKernel>) -> Result<(), ChainStorageError> {
        let mut db = self.db_write_access()?;
        db.add_kernels(kernels)
    }

    /// This is used when synchronising. Adds in the list of utxos provided to the main chain
    pub fn add_utxos(&self, utxos: Vec<TransactionOutput>) -> Result<(), ChainStorageError> {
        let mut db = self.db_write_access()?;
        db.add_utxos(utxos)
    }

    /// This is used when synchronising. Adds in the list of utxos provided to the main chain
    pub fn add_orphan_block(&self, orphan: Block) -> Result<(), ChainStorageError> {
        if let Err(e) = self.validators.orphan.validate(&orphan) {
            warn!(
                target: LOG_TARGET,
                "Block #{} ({}) failed validation - {}",
                orphan.header.height,
                orphan.hash().to_hex(),
                e.to_string()
            );
            return Err(e.into());
        }
        let mut db = self.db_write_access()?;
        db.add_orphan_block(orphan)
    }
}

fn unexpected_result<T>(req: DbKey, res: DbValue) -> Result<T, ChainStorageError> {
    let msg = format!("Unexpected result for database query {}. Response: {}", req, res);
    error!(target: LOG_TARGET, "{}", msg);
    Err(ChainStorageError::UnexpectedResult(msg))
}

fn write_metadata<T: BlockchainBackend>(
    db: &mut RwLockWriteGuard<T>,
    metadata: ChainMetadata,
) -> Result<(), ChainStorageError>
{
    db.force_meta_data(metadata)
}

fn fetch_kernel<T: BlockchainBackend>(db: &T, hash: HashOutput) -> Result<TransactionKernel, ChainStorageError> {
    fetch!(db, hash, TransactionKernel)
}

pub fn fetch_header<T: BlockchainBackend>(db: &T, block_num: u64) -> Result<BlockHeader, ChainStorageError> {
    fetch!(db, block_num, BlockHeader)
}

pub fn fetch_block<T: BlockchainBackend>(db: &T, block_num: u64) -> Result<Block, ChainStorageError> {
    fetch!(db, block_num, Block)
}

pub fn fetch_headers<T: BlockchainBackend>(
    db: &T,
    block_nums: Vec<u64>,
) -> Result<Vec<BlockHeader>, ChainStorageError>
{
    let mut headers = Vec::<BlockHeader>::with_capacity(block_nums.len());
    for block_num in block_nums {
        headers.push(fetch_header(db, block_num)?);
    }
    Ok(headers)
}

fn fetch_header_with_block_hash<T: BlockchainBackend>(
    db: &T,
    hash: BlockHash,
) -> Result<BlockHeader, ChainStorageError>
{
    fetch!(db, hash, BlockHash)
}

pub fn fetch_tip_header<T: BlockchainBackend>(db: &T) -> Result<BlockHeader, ChainStorageError> {
    db.fetch_last_header()
        .or_else(|e| {
            error!(target: LOG_TARGET, "Could not fetch the tip header of the db. {:?}", e);
            Err(e)
        })?
        .ok_or_else(|| ChainStorageError::InvalidQuery("Cannot retrieve header. Blockchain DB is empty".into()))
}

fn fetch_utxo<T: BlockchainBackend>(db: &T, hash: HashOutput) -> Result<TransactionOutput, ChainStorageError> {
    fetch!(db, hash, UnspentOutput)
}

fn fetch_stxo<T: BlockchainBackend>(db: &T, hash: HashOutput) -> Result<TransactionOutput, ChainStorageError> {
    fetch!(db, hash, SpentOutput)
}

fn fetch_txo<T: BlockchainBackend>(db: &T, hash: HashOutput) -> Result<Option<TransactionOutput>, ChainStorageError> {
    if let Some(DbValue::SpentOutput(output)) = db.fetch(&DbKey::SpentOutput(hash.clone()))? {
        return Ok(Some(*output));
    }
    let key = DbKey::UnspentOutput(hash);
    match db.fetch(&key)? {
        Some(DbValue::UnspentOutput(output)) => Ok(Some(*output)),
        Some(other) => unexpected_result(key, other),
        None => Ok(None),
    }
}

fn fetch_orphan<T: BlockchainBackend>(db: &T, hash: BlockHash) -> Result<Block, ChainStorageError> {
    fetch!(db, hash, OrphanBlock)
}

pub fn fetch_target_difficulties<T: BlockchainBackend>(
    db: &T,
    pow_algo: PowAlgorithm,
    height: u64,
    block_window: usize,
) -> Result<Vec<(EpochTime, Difficulty)>, ChainStorageError>
{
    db.fetch_target_difficulties(pow_algo, height, block_window)
}

pub fn is_utxo<T: BlockchainBackend>(db: &T, hash: HashOutput) -> Result<bool, ChainStorageError> {
    let key = DbKey::UnspentOutput(hash);
    db.contains(&key)
}

pub fn is_stxo<T: BlockchainBackend>(db: &T, hash: HashOutput) -> Result<bool, ChainStorageError> {
    // Check if the UTXO MMR contains the specified deleted UTXO hash, the backend stxo_db is not used for this task as
    // archival nodes and pruning nodes might have different STXOs in their stxo_db as horizon state STXOs are
    // discarded by pruned nodes.
    if let Some(leaf_index) = db.fetch_mmr_leaf_index(MmrTree::Utxo, &hash)? {
        let (_, deleted) = db.fetch_mmr_nodes(MmrTree::Utxo, leaf_index, 1, None)?[0];
        return Ok(deleted);
    }
    Ok(false)
}

// fn fetch_mmr_root<T: BlockchainBackend>(db: &T, tree: MmrTree) -> Result<HashOutput, ChainStorageError> {
//     db.fetch_mmr_root(tree)
// }

// fn fetch_mmr_only_root<T: BlockchainBackend>(db: &T, tree: MmrTree) -> Result<HashOutput, ChainStorageError> {
//     db.fetch_mmr_only_root(tree)
// }

pub fn calculate_mmr_roots<T: BlockchainBackend>(
    db: &T,
    template: NewBlockTemplate,
) -> Result<Block, ChainStorageError>
{
    let NewBlockTemplate { header, mut body } = template;
    // Make sure the body components are sorted. If they already are, this is a very cheap call.
    body.sort();
    let kernel_hashes: Vec<HashOutput> = body.kernels().iter().map(|k| k.hash()).collect();
    let out_hashes: Vec<HashOutput> = body.outputs().iter().map(|out| out.hash()).collect();
    let rp_hashes: Vec<HashOutput> = body.outputs().iter().map(|out| out.proof().hash()).collect();
    let inp_hashes: Vec<HashOutput> = body.inputs().iter().map(|inp| inp.hash()).collect();

    let mut header = BlockHeader::from(header);
    header.kernel_mr = db.calculate_mmr_root(MmrTree::Kernel, kernel_hashes, vec![])?;
    header.output_mr = db.calculate_mmr_root(MmrTree::Utxo, out_hashes, inp_hashes)?;
    header.range_proof_mr = db.calculate_mmr_root(MmrTree::RangeProof, rp_hashes, vec![])?;
    Ok(Block { header, body })
}

// /// Fetch a Merklish proof for the given hash, tree and position in the MMR
// fn fetch_mmr_proof<T: BlockchainBackend>(db: &T, tree: MmrTree, pos: usize) -> Result<MerkleProof, ChainStorageError>
// {     db.fetch_mmr_proof(tree, pos)
// }

fn add_block<T: BlockchainBackend>(
    db: &mut RwLockWriteGuard<T>,
    block_validator: &Arc<Validator<Block, T>>,
    accum_difficulty_validator: &Arc<Validator<Difficulty, T>>,
    block: Block,
) -> Result<BlockAddResult, ChainStorageError>
{
    let block_hash = block.hash();
    if db.contains(&DbKey::BlockHash(block_hash))? {
        return Ok(BlockAddResult::BlockExists);
    }
    handle_possible_reorg(db, block_validator, accum_difficulty_validator, block)
}

// Adds a new block onto the chain tip.
// fn store_new_block<T: BlockchainBackend>(db: &mut RwLockWriteGuard<T>, block: Block) -> Result<(), ChainStorageError>
// {     let hash = block.hash();
//     db.accept_block(hash)
//     // let (header, inputs, outputs, kernels) = block.dissolve();
//     // let height = header.height;
//     // let best_block = header.hash();
//     // let accumulated_difficulty =
//     //     ProofOfWork::new_from_difficulty(&header.pow, ProofOfWork::achieved_difficulty(&header))
//     //         .total_accumulated_difficulty();
//     // // Build all the DB queries needed to add the block and the add it atomically
//     // let mut txn = DbTransaction::new();
//     // // Update metadata
//     // txn.insert(DbKeyValuePair::Metadata(
//     //     MetadataKey::ChainHeight,
//     //     MetadataValue::ChainHeight(Some(height)),
//     // ));
//     // txn.insert(DbKeyValuePair::Metadata(
//     //     MetadataKey::BestBlock,
//     //     MetadataValue::BestBlock(Some(best_block)),
//     // ));
//     // txn.insert(DbKeyValuePair::Metadata(
//     //     MetadataKey::AccumulatedWork,
//     //     MetadataValue::AccumulatedWork(Some(accumulated_difficulty)),
//     // ));
//     // // Insert block
//     // txn.insert_header(header);
//     // txn.spend_inputs(&inputs);
//     // outputs.iter().for_each(|utxo| txn.insert_utxo(utxo.clone()));
//     // kernels.iter().for_each(|k| txn.insert_kernel(k.clone()));
//     // txn.commit_block();
//     // commit(db, txn)?;
//     // Ok(())
// }

// fn store_pruning_horizon<T: BlockchainBackend>(
//     db: &mut RwLockWriteGuard<T>,
//     pruning_horizon: u64,
// ) -> Result<(), ChainStorageError>
// {
//     let mut txn = DbTransaction::new();
//     txn.insert(DbKeyValuePair::Metadata(
//         MetadataKey::PruningHorizon,
//         MetadataValue::PruningHorizon(pruning_horizon),
//     ));
//     commit(db, txn)
// }

fn fetch_historic_block<T: BlockchainBackend>(db: &T, height: u64) -> Result<HistoricalBlock, ChainStorageError> {
    let tip_height = check_for_valid_height(&*db, height)?;
    // let header = fetch_header(db, height)?;
    // let kernel_cp = fetch_checkpoint(db, MmrTree::Kernel, height)?;
    // let (kernel_hashes, _) = kernel_cp.into_parts();
    // let kernels = fetch_kernels(db, kernel_hashes)?;
    // let utxo_cp = fetch_checkpoint(db, MmrTree::Utxo, height)?;
    // let (utxo_hashes, deleted_nodes) = utxo_cp.into_parts();
    // // let inputs = fetch_inputs(db, deleted_nodes)?;
    // let (outputs, spent) = fetch_outputs(db, utxo_hashes)?;
    // let block = header
    //     .into_builder()
    //     .add_inputs(inputs)
    //     .add_outputs(outputs)
    //     .add_kernels(kernels)
    //     .build();
    let block = fetch_block(db, height)?;
    let mut spent_hashes = Vec::new();
    for output in block.body.outputs() {
        spent_hashes.push(output.hash());
    }
    let spent = fetch_spent_outputs(db, spent_hashes)?;
    Ok(HistoricalBlock::new(block, tip_height - height + 1, spent))
}

fn fetch_block_with_hash<T: BlockchainBackend>(
    db: &T,
    hash: BlockHash,
) -> Result<Option<HistoricalBlock>, ChainStorageError>
{
    if let Ok(header) = fetch_header_with_block_hash(db, hash.clone()) {
        return Ok(Some(fetch_historic_block(db, header.height)?));
    }
    if let Ok(block) = fetch_orphan(db, hash) {
        return Ok(Some(HistoricalBlock::new(block, 0, vec![])));
    }
    Ok(None)
}

fn block_exists<T: BlockchainBackend>(db: &T, hash: BlockHash) -> Result<bool, ChainStorageError> {
    let exists = db.contains(&DbKey::BlockHash(hash.clone()))? || db.contains(&DbKey::OrphanBlock(hash))?;
    Ok(exists)
}

fn check_for_valid_height<T: BlockchainBackend>(db: &T, height: u64) -> Result<u64, ChainStorageError> {
    let metadata = db.fetch_metadata()?;
    let db_height = metadata.height_of_longest_chain.unwrap_or(0);
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

fn fetch_kernels<T: BlockchainBackend>(db: &T, hashes: Vec<Hash>) -> Result<Vec<TransactionKernel>, ChainStorageError> {
    hashes.into_iter().map(|hash| fetch_kernel(db, hash)).collect()
}

fn fetch_inputs<T: BlockchainBackend>(
    db: &T,
    deleted_nodes: Bitmap,
) -> Result<Vec<TransactionInput>, ChainStorageError>
{
    // The inputs must all be in the current STXO set
    let inputs: Result<Vec<TransactionInput>, ChainStorageError> = deleted_nodes
        .iter()
        .map(|pos| {
            db.fetch_mmr_nodes(MmrTree::Utxo, pos, 1, None)
                .and_then(|node| {
                    let (hash, deleted) = &node[0];
                    assert!(deleted);
                    fetch_stxo(db, hash.clone())
                })
                .and_then(|stxo| Ok(TransactionInput::from(stxo)))
        })
        .collect();
    inputs
}

fn fetch_outputs<T: BlockchainBackend>(
    db: &T,
    hashes: Vec<Hash>,
) -> Result<(Vec<TransactionOutput>, Vec<Commitment>), ChainStorageError>
{
    let mut outputs = Vec::with_capacity(hashes.len());
    let mut spent = Vec::with_capacity(hashes.len());
    for hash in hashes.into_iter() {
        // The outputs could come from either the UTXO or STXO set
        match fetch_utxo(db, hash.clone()) {
            Ok(utxo) => {
                outputs.push(utxo);
                continue;
            },
            Err(ChainStorageError::ValueNotFound(_)) => {}, // Check STXO set below
            Err(e) => return Err(e),                        // Something bad happened. Abort.
        }
        // Check the STXO set
        let stxo = fetch_stxo(db, hash)?;
        spent.push(stxo.commitment.clone());
        outputs.push(stxo);
    }
    Ok((outputs, spent))
}

// This searches the db for the matches of stxo's.
fn fetch_spent_outputs<T: BlockchainBackend>(db: &T, hashes: Vec<Hash>) -> Result<Vec<Commitment>, ChainStorageError> {
    let mut spent = Vec::with_capacity(hashes.len());
    for hash in hashes.into_iter() {
        // Check the STXO set
        match fetch_stxo(db, hash) {
            Ok(v) => {
                spent.push(v.commitment.clone());
            },
            Err(ChainStorageError::ValueNotFound(_)) => {},
            Err(e) => return Err(e),
        }
    }
    Ok(spent)
}

// fn fetch_checkpoint<T: BlockchainBackend>(
//     db: &T,
//     tree: MmrTree,
//     height: u64,
// ) -> Result<MerkleCheckPoint, ChainStorageError>
// {
//     db.fetch_checkpoint(tree, height)
// }

// pub fn commit<T: BlockchainBackend>(db: &mut RwLockWriteGuard<T>, txn: DbTransaction) -> Result<(),
// ChainStorageError> {     db.write(txn)
// }

// Checks whether we should add the block as an orphan. If it is the case, the orphan block is added and the chain
// is reorganised if necessary.
fn handle_possible_reorg<T: BlockchainBackend>(
    db: &mut RwLockWriteGuard<T>,
    block_validator: &Arc<Validator<Block, T>>,
    accum_difficulty_validator: &Arc<Validator<Difficulty, T>>,
    block: Block,
) -> Result<BlockAddResult, ChainStorageError>
{
    let db_height = db
        .fetch_metadata()?
        .height_of_longest_chain
        .ok_or_else(|| ChainStorageError::InvalidQuery("Cannot retrieve block. Blockchain DB is empty".into()))
        .or_else(|e| {
            error!(
                target: LOG_TARGET,
                "Could not retrieve block, block chain is empty {:?}", e
            );
            Err(e)
        })?;
    let block_hash = block.hash().to_hex();
    insert_orphan(db, block.clone())?;
    debug!(
        target: LOG_TARGET,
        "Added candidate block #{} ({}) to the orphan database. Best height is {}.",
        block.header.height,
        block_hash,
        db_height,
    );
    // Trigger a reorg check for all blocks in the orphan block pool
    handle_reorg(db, block_validator, accum_difficulty_validator, block)
}

// The handle_reorg function is triggered by the adding of orphaned blocks. Reorg chains are constructed by
// finding the orphan chain tip with the highest accumulated difficulty that can be linked to the newly added
// orphan block and then building a chain from the strongest orphan tip back to the main chain. The newly added
// orphan block is considered to be a orphan tip if no better tips can be found that link to it. When a valid
// reorg chain is constructed with a higher accumulated difficulty, then the main chain is rewound and updated
// with the newly un-orphaned blocks from the reorg chain.
fn handle_reorg<T: BlockchainBackend>(
    db: &mut RwLockWriteGuard<T>,
    block_validator: &Validator<Block, T>,
    accum_difficulty_validator: &Arc<Validator<Difficulty, T>>,
    new_block: Block,
) -> Result<BlockAddResult, ChainStorageError>
{
    // We can assume that the new block is part of the reorg chain if it exists, otherwise the reorg would have
    // happened on the previous call to this function.
    // Try and construct a path from `new_block` to the main chain:
    let mut reorg_chain = try_construct_fork(db, new_block.clone())?;
    if reorg_chain.is_empty() {
        debug!(
            target: LOG_TARGET,
            "No reorg required, could not construct complete chain using block #{} ({}).",
            new_block.header.height,
            new_block.hash().to_hex()
        );
        return Ok(BlockAddResult::OrphanBlock);
    }
    // Try and find all orphaned chain tips that can be linked to the new orphan block, if no better orphan chain
    // tips can be found then the new_block is a tip.
    let new_block_hash = new_block.hash();
    let orphan_chain_tips = find_orphan_chain_tips(&**db, new_block.header.height, new_block_hash.clone());
    // Check the accumulated difficulty of the best fork chain compared to the main chain.
    let (fork_accum_difficulty, fork_tip_hash) = find_strongest_orphan_tip(&**db, orphan_chain_tips)?;
    let tip_header = db
        .fetch_last_header()?
        .ok_or_else(|| ChainStorageError::InvalidQuery("Cannot retrieve header. Blockchain DB is empty".into()))?;
    if fork_tip_hash == new_block_hash {
        debug!(
            target: LOG_TARGET,
            "Comparing candidate block #{} (accum_diff:{}, hash:{}) to main chain #{} (accum_diff: {}, hash: ({})).",
            new_block.header.height,
            fork_accum_difficulty,
            fork_tip_hash.to_hex(),
            tip_header.height,
            tip_header.total_accumulated_difficulty_inclusive(),
            tip_header.hash().to_hex()
        );
    } else {
        debug!(
            target: LOG_TARGET,
            "Comparing fork (accum_diff:{}, hash:{}) with block #{} ({}) to main chain #{} (accum_diff: {}, hash: \
             ({})).",
            fork_accum_difficulty,
            fork_tip_hash.to_hex(),
            new_block.header.height,
            new_block_hash.to_hex(),
            tip_header.height,
            tip_header.total_accumulated_difficulty_inclusive(),
            tip_header.hash().to_hex()
        );
    }

    match accum_difficulty_validator.validate(&fork_accum_difficulty, db) {
        Ok(_) => {
            debug!(
                target: LOG_TARGET,
                "Accumulated difficulty validation PASSED for block #{} ({})",
                new_block.header.height,
                new_block_hash.to_hex()
            );
        },
        Err(ValidationError::WeakerAccumulatedDifficulty) => {
            debug!(
                target: LOG_TARGET,
                "Fork chain (accum_diff:{}, hash:{}) with block {} ({}) has a weaker accumulated difficulty.",
                fork_accum_difficulty,
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
        Err(err) => {
            error!(
                target: LOG_TARGET,
                "Failed to validate accumulated difficulty on forked chain (accum_diff:{}, hash:{}) with block {} \
                 ({}): {:?}.",
                fork_accum_difficulty,
                fork_tip_hash.to_hex(),
                new_block.header.height,
                new_block_hash.to_hex(),
                err
            );
            return Err(err.into());
        },
    }

    // We've built the strongest orphan chain we can by going backwards and forwards from the new orphan block
    // that is linked with the main chain.
    let fork_tip_block = fetch_orphan(&**db, fork_tip_hash.clone())?;
    let fork_tip_header = fork_tip_block.header.clone();
    if fork_tip_hash != new_block_hash {
        // New block is not the tip, find complete chain from tip to main chain.
        reorg_chain = try_construct_fork(db, fork_tip_block)?;
    }
    let added_blocks: Vec<Block> = reorg_chain.iter().cloned().collect();
    let fork_height = reorg_chain
        .front()
        .expect("The new orphan block should be in the queue")
        .header
        .height -
        1;
    let removed_blocks = reorganize_chain(db, block_validator, fork_height, reorg_chain)?;
    if removed_blocks.is_empty() {
        Ok(BlockAddResult::Ok)
    } else {
        debug!(
            target: LOG_TARGET,
            "Chain reorg processed from (accum_diff:{}, hash:{}) to (accum_diff:{}, hash:{})",
            tip_header.pow,
            tip_header.hash().to_hex(),
            fork_tip_header.pow,
            fork_tip_hash.to_hex()
        );
        info!(
            target: LOG_TARGET,
            "Reorg from ({}) to ({})", tip_header, fork_tip_header
        );
        Ok(BlockAddResult::ChainReorg((
            Box::new(removed_blocks),
            Box::new(added_blocks),
        )))
    }
}

// Reorganize the main chain with the provided fork chain, starting at the specified height.
fn reorganize_chain<T: BlockchainBackend>(
    db: &mut RwLockWriteGuard<T>,
    block_validator: &Validator<Block, T>,
    height: u64,
    chain: VecDeque<Block>,
) -> Result<Vec<Block>, ChainStorageError>
{
    let removed_blocks_headers = db.rewind_to_height(height)?;
    let mut removed_blocks = Vec::new();
    for header in &removed_blocks_headers {
        removed_blocks.push(fetch_orphan(&**db, header.hash())?);
    }

    debug!(
        target: LOG_TARGET,
        "Validate and add {} chain blocks from height {}.",
        chain.len(),
        height
    );
    let mut validation_result: Result<(), ValidationError> = Ok(());
    let mut orphan_hashes = Vec::<BlockHash>::with_capacity(chain.len());
    for block in chain {
        let block_hash = block.hash();
        orphan_hashes.push(block_hash.clone());
        validation_result = block_validator.validate(&block, db);
        if validation_result.is_err() {
            remove_orphan(db, block.hash())?;
            break;
        }
        db.accept_block(block_hash)?;
        // store_new_block(db, block)?;
    }

    match validation_result {
        Ok(_) => {
            debug!(target: LOG_TARGET, "Removing orphan blocks used for reorg.",);
            // if !orphan_hashes.is_empty() {
            //     let mut txn = DbTransaction::new();
            //     for orphan_hash in orphan_hashes {
            //         txn.delete(DbKey::OrphanBlock(orphan_hash));
            //     }
            //     commit(db, txn)?;
            // }
            Ok(removed_blocks)
        },
        Err(e) => {
            info!(target: LOG_TARGET, "Restoring previous chain after failed reorg.",);
            let invalid_chain = db.rewind_to_height(height)?;
            debug!(
                target: LOG_TARGET,
                "Removed incomplete chain of blocks during chain restore: {:?}.",
                invalid_chain
                    .iter()
                    .map(|block| block.hash().to_hex())
                    .collect::<Vec<_>>(),
            );
            // let mut txn = DbTransaction::new();
            // for block in removed_blocks {
            //     txn.delete(DbKey::OrphanBlock(block.hash()));
            //     store_new_block(db, block)?;
            // }
            // commit(db, txn)?;
            for header in removed_blocks_headers {
                db.accept_block(header.hash())?;
            }
            Err(e.into())
        },
    }
}

// Insert the provided block into the orphan pool.
fn insert_orphan<T: BlockchainBackend>(db: &mut RwLockWriteGuard<T>, block: Block) -> Result<(), ChainStorageError> {
    db.add_orphan_block(block)
}

// Discard the the orphan block from the orphan pool that corresponds to the provided block hash.
fn remove_orphan<T: BlockchainBackend>(
    db: &mut RwLockWriteGuard<T>,
    hash: HashOutput,
) -> Result<(), ChainStorageError>
{
    //     let mut txn = DbTransaction::new();
    //     txn.delete(DbKey::OrphanBlock(hash));
    //     commit(db, txn)
    db.remove_orphan_blocks(vec![hash]);
    Ok(())
}

/// We try and build a chain from this block to the main chain. If we can't do that we can stop.
/// We start with the current, newly received block, and look for a blockchain sequence (via `prev_hash`).
/// Each successful link is pushed to the front of the queue. An empty queue is returned if the fork chain did not
/// link to the main chain.
fn try_construct_fork<T: BlockchainBackend>(
    db: &mut RwLockWriteGuard<T>,
    new_block: Block,
) -> Result<VecDeque<Block>, ChainStorageError>
{
    let mut fork_chain = VecDeque::new();
    let new_block_hash = new_block.hash();
    let new_block_height = new_block.header.height;
    let mut hash = new_block.header.prev_hash.clone();
    let mut height = new_block_height;
    fork_chain.push_front(new_block);

    loop {
        let fork_start_header = fork_chain
            .front()
            .expect("The new orphan block should be in the queue")
            .header
            .clone();
        debug!(
            target: LOG_TARGET,
            "Checking if block #{} ({}) is connected to the main chain.",
            fork_start_header.height,
            fork_start_header.hash().to_hex(),
        );
        if let Ok(header) = fetch_header_with_block_hash(&**db, fork_start_header.prev_hash) {
            if header.height + 1 == fork_start_header.height {
                debug!(
                    target: LOG_TARGET,
                    "Connection with main chain found at block #{} ({}) from block #{} ({}).",
                    header.height,
                    header.hash().to_hex(),
                    new_block_height,
                    new_block_hash.to_hex(),
                );
                return Ok(fork_chain);
            }
        }

        debug!(
            target: LOG_TARGET,
            "Not connected, checking if fork chain can be extended.",
        );
        match fetch_orphan(&**db, hash.clone()) {
            Ok(prev_block) => {
                debug!(
                    target: LOG_TARGET,
                    "Checking if block #{} ({}) forms a sequence with next block.",
                    prev_block.header.height,
                    hash.to_hex(),
                );
                if prev_block.header.height + 1 != height {
                    // Well now. The block heights don't form a sequence, which means that we should not only stop now,
                    // but remove one or both of these orphans from the pool because the blockchain is broken at this
                    // point.
                    debug!(
                        target: LOG_TARGET,
                        "A broken blockchain sequence was detected, removing block #{} ({}).",
                        prev_block.header.height,
                        hash.to_hex()
                    );
                    remove_orphan(db, hash)?;
                    return Err(ChainStorageError::InvalidBlock);
                }
                debug!(
                    target: LOG_TARGET,
                    "Fork chain extended with block #{} ({}).",
                    prev_block.header.height,
                    hash.to_hex(),
                );
                hash = prev_block.header.prev_hash.clone();
                height -= 1;
                fork_chain.push_front(prev_block);
            },
            Err(ChainStorageError::ValueNotFound(_)) => {
                debug!(
                    target: LOG_TARGET,
                    "Fork chain extension not found, block #{} ({}) not connected to main chain.",
                    new_block_height,
                    new_block_hash.to_hex(),
                );
                break;
            },
            Err(e) => return Err(e),
        }
    }
    Ok(VecDeque::new())
}

/// Try to find all orphan chain tips that originate from the current orphan parent block.
fn find_orphan_chain_tips<T: BlockchainBackend>(db: &T, parent_height: u64, parent_hash: BlockHash) -> Vec<BlockHash> {
    let mut tip_hashes = Vec::<BlockHash>::new();
    let mut parents = Vec::<(BlockHash, u64)>::new();
    let parents_headers = db
        .fetch_parent_orphan_headers(parent_hash.clone(), parent_height)
        .unwrap_or(Vec::new());
    for header in parents_headers {
        parents.push((header.hash(), header.height));
    }
    // we need two for loops so that we ensure we release the db read lock as this iterative call can saturate all db
    // read locks. This ensures the call only uses one read lock.
    for (parent_hash, parent_height) in parents {
        let mut orphan_chain_tips = find_orphan_chain_tips(db, parent_height, parent_hash.clone());
        if !orphan_chain_tips.is_empty() {
            tip_hashes.append(&mut orphan_chain_tips);
        } else {
            tip_hashes.push(parent_hash.clone());
        }
    }
    if tip_hashes.is_empty() {
        // No chain tips found, then parent must be the tip.
        tip_hashes.push(parent_hash);
    }
    tip_hashes
}

/// Find and return the orphan chain tip with the highest accumulated difficulty.
fn find_strongest_orphan_tip<T: BlockchainBackend>(
    db: &T,
    orphan_chain_tips: Vec<BlockHash>,
) -> Result<(Difficulty, BlockHash), ChainStorageError>
{
    let mut best_accum_difficulty = Difficulty::min();
    let mut best_tip_hash: Vec<u8> = vec![0; 32];
    for tip_hash in orphan_chain_tips {
        let header = fetch_orphan(db, tip_hash.clone())?.header;
        let accum_difficulty = header.total_accumulated_difficulty_inclusive();
        if accum_difficulty >= best_accum_difficulty {
            best_tip_hash = tip_hash;
            best_accum_difficulty = accum_difficulty;
        }
    }
    Ok((best_accum_difficulty, best_tip_hash))
}

// Perform a comprehensive search to remove all the minimum height orphans to maintain the configured orphan pool
// storage limit. If the node is configured to run in pruned mode then orphan blocks with heights lower than the horizon
// block height will also be discarded.
fn cleanup_orphans<T: BlockchainBackend>(
    db: &mut RwLockWriteGuard<T>,
    orphan_storage_capacity: usize,
) -> Result<(), ChainStorageError>
{
    let orphan_count = db.get_orphan_count()?;
    let num_over_limit = orphan_count.saturating_sub(orphan_storage_capacity);
    if num_over_limit > 0 {
        info!(
            target: LOG_TARGET,
            "Orphan block storage limit reached, performing cleanup.",
        );
        let orphan_headers = db.fetch_all_orphan_headers()?;
        let mut orphans = Vec::new();
        for orphan in orphan_headers {
            orphans.push((orphan.height, orphan.hash()));
        }
        orphans.sort_by(|a, b| a.0.cmp(&b.0));

        let metadata = db.fetch_metadata()?;
        let horizon_height = metadata.horizon_block(metadata.height_of_longest_chain.unwrap_or(0));
        // let mut txn = DbTransaction::new();
        let mut blocks_to_remove = Vec::new();
        for (removed_count, (height, block_hash)) in orphans.into_iter().enumerate() {
            if height > horizon_height && removed_count >= num_over_limit {
                break;
            }
            debug!(
                target: LOG_TARGET,
                "Discarding orphan block #{} ({}).",
                height,
                block_hash.to_hex()
            );
            blocks_to_remove.push(block_hash);
            // txn.delete(DbKey::OrphanBlock(block_hash.clone()));
        }
        db.remove_orphan_blocks(blocks_to_remove)?;
        // commit(db, txn)?;
    }
    Ok(())
}

fn cleanup_pruned_mode<T: BlockchainBackend>(
    db: &mut RwLockWriteGuard<T>,
    pruned_mode_cleanup_interval: u64,
    pruning_horizon: u64,
) -> Result<(), ChainStorageError>
{
    // let metadata = db.fetch_metadata()?;
    // if metadata.is_pruned_node() {
    //     let db_height = metadata.height_of_longest_chain.unwrap_or(0);
    //     if db_height % pruned_mode_cleanup_interval == 0 {
    //         info!(
    //             target: LOG_TARGET,
    //             "Pruned mode cleanup interval reached, performing cleanup.",
    //         );
    //         let max_cp_count = pruning_horizon + 1; // Include accumulated checkpoint
    //         let mut txn = DbTransaction::new();
    //         txn.merge_checkpoints(max_cp_count as usize);
    //         return commit(db, txn);
    //     }
    // }
    Ok(())
}

fn commit_horizon_state<T: BlockchainBackend>(db: &mut RwLockWriteGuard<T>) -> Result<(), ChainStorageError> {
    let mut metadata = db.fetch_metadata()?;
    let tip_header = db
        .fetch_last_header()?
        .ok_or_else(|| ChainStorageError::InvalidQuery("Cannot retrieve header. Blockchain DB is empty".into()))?;
    metadata.best_block = Some(tip_header.hash());
    metadata.accumulated_difficulty = Some(
        ProofOfWork::new_from_difficulty(&tip_header.pow, ProofOfWork::achieved_difficulty(&tip_header))
            .total_accumulated_difficulty(),
    );
    metadata.height_of_longest_chain = Some(tip_header.height);
    db.force_meta_data(metadata)
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
        }
    }
}
