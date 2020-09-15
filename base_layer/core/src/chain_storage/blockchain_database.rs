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
            BLOCKCHAIN_DATABASE_PRUNED_MODE_PRUNING_INTERVAL,
            BLOCKCHAIN_DATABASE_PRUNING_HORIZON,
        },
        db_transaction::{DbKey, DbTransaction, DbValue, MetadataKey, MetadataValue, MmrTree},
        error::ChainStorageError,
        ChainMetadata,
        HistoricalBlock,
        InProgressHorizonSyncState,
    },
    consensus::ConsensusManager,
    proof_of_work::{Difficulty, PowAlgorithm, ProofOfWork},
    transactions::{
        transaction::{TransactionInput, TransactionKernel, TransactionOutput},
        types::{Commitment, HashOutput, PublicKey, Signature},
    },
    validation::{StatelessValidation, StatelessValidator, Validation, ValidationError, Validator},
};
use croaring::Bitmap;
use log::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    convert::TryFrom,
    sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard},
};
use strum_macros::Display;
use tari_crypto::tari_utilities::{epoch_time::EpochTime, hex::Hex, Hashable};
use tari_mmr::{Hash, MerkleCheckPoint, MerkleProof, MutableMmrLeafNodes};

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
/// Data is passed to and from the backend via the [DbKey], [DbValue], and [DbValueKey] enums. This strategy allows
/// us to keep the reading and writing API extremely simple. Extending the types of data that the back ends can handle
/// will entail adding to those enums, and the back ends, while this trait can remain unchanged.
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
    /// Fetches the merklish root for the MMR tree identified by the key. This function should only fail if there is an
    /// access or integrity issue with the back end.
    fn fetch_mmr_root(&self, tree: MmrTree) -> Result<HashOutput, ChainStorageError>;
    /// Returns only the MMR merkle root without the state of the roaring bitmap.
    fn fetch_mmr_only_root(&self, tree: MmrTree) -> Result<HashOutput, ChainStorageError>;
    /// Fetches the merklish root for the MMR tree identified by the key after the current additions and deletions have
    /// temporarily been applied. Deletions of hashes from the MMR can only be applied for UTXOs.
    fn calculate_mmr_root(
        &self,
        tree: MmrTree,
        additions: Vec<HashOutput>,
        deletions: Vec<HashOutput>,
    ) -> Result<HashOutput, ChainStorageError>;
    /// Constructs a merkle proof for the specified merkle mountain range and the given leaf position.
    fn fetch_mmr_proof(&self, tree: MmrTree, pos: usize) -> Result<MerkleProof, ChainStorageError>;
    /// Fetches the `MerkleCheckPoint` corresponding to the given height. In pruned mode, the underlying database may
    /// not be able to provide the checkpoint as it has been merged into the base checkpoint. In this case a
    /// `BeyondPruningHorizon` error is returned.
    fn fetch_checkpoint_at_height(&self, tree: MmrTree, height: u64) -> Result<MerkleCheckPoint, ChainStorageError>;
    /// Fetches the `MerkleCheckPoint` at the given index
    fn fetch_checkpoint_at_index(
        &self,
        tree: MmrTree,
        index: usize,
    ) -> Result<Option<MerkleCheckPoint>, ChainStorageError>;
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
    fn delete_mmr_node(&mut self, tree: MmrTree, hash: &Hash) -> Result<(), ChainStorageError>;
    /// Fetches the leaf index of the provided leaf node hash in the given MMR tree.
    fn fetch_mmr_leaf_index(&self, tree: MmrTree, hash: &Hash) -> Result<Option<u32>, ChainStorageError>;
    /// Performs the function F for each orphan block in the orphan pool.
    fn for_each_orphan<F>(&self, f: F) -> Result<(), ChainStorageError>
    where
        Self: Sized,
        F: FnMut(Result<(HashOutput, Block), ChainStorageError>);
    /// Returns the number of blocks in the block orphan pool.
    fn get_orphan_count(&self) -> Result<usize, ChainStorageError>;
    /// Performs the function F for each transaction kernel.
    fn for_each_kernel<F>(&self, f: F) -> Result<(), ChainStorageError>
    where
        Self: Sized,
        F: FnMut(Result<(HashOutput, TransactionKernel), ChainStorageError>);
    /// Performs the function F for each block header.
    fn for_each_header<F>(&self, f: F) -> Result<(), ChainStorageError>
    where
        Self: Sized,
        F: FnMut(Result<(u64, BlockHeader), ChainStorageError>);
    /// Performs the function F for each UTXO.
    fn for_each_utxo<F>(&self, f: F) -> Result<(), ChainStorageError>
    where
        Self: Sized,
        F: FnMut(Result<(HashOutput, TransactionOutput), ChainStorageError>);
    /// Returns the stored header with the highest corresponding height.
    fn fetch_last_header(&self) -> Result<Option<BlockHeader>, ChainStorageError>;
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
    /// Returns the checkpoint count for the given MmrTree
    fn count_checkpoints(&self, tree: MmrTree) -> Result<usize, ChainStorageError>;
    /// Validate the Merkle root for the given `MmrTree` matches the header at the given height
    fn validate_merkle_root(&self, tree: MmrTree, height: u64) -> Result<bool, ChainStorageError>;
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
pub struct BlockchainDatabase<B> {
    db: Arc<RwLock<B>>,
    validators: Validators<B>,
    config: BlockchainDatabaseConfig,
}

impl<B> BlockchainDatabase<B>
where B: BlockchainBackend
{
    /// Creates a new `BlockchainDatabase` using the provided backend.
    pub fn new(
        db: B,
        consensus_manager: &ConsensusManager,
        validators: Validators<B>,
        config: BlockchainDatabaseConfig,
    ) -> Result<Self, ChainStorageError>
    {
        debug!(
            target: LOG_TARGET,
            "Initializing database pj={}", config.pruning_horizon
        );
        let blockchain_db = BlockchainDatabase {
            db: Arc::new(RwLock::new(db)),
            validators,
            config,
        };
        let metadata = blockchain_db.get_chain_metadata()?;
        if metadata.height_of_longest_chain.is_none() {
            let genesis_block = consensus_manager.get_genesis_block();
            blockchain_db.store_new_block(genesis_block)?;
            blockchain_db.store_pruning_horizon(config.pruning_horizon)?;
        }
        if config.pruning_horizon != metadata.pruning_horizon {
            debug!(
                target: LOG_TARGET,
                "Updating pruning horizon from {} to {}.", metadata.pruning_horizon, config.pruning_horizon,
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
    pub fn get_height(&self) -> Result<Option<u64>, ChainStorageError> {
        let db = self.db_read_access()?;
        Ok(db.fetch_chain_metadata()?.height_of_longest_chain)
    }

    /// Return the geometric mean of the proof of work of the longest chain.
    /// The proof of work is returned as the geometric mean of all difficulties
    pub fn get_accumulated_difficulty(&self) -> Result<Option<Difficulty>, ChainStorageError> {
        let db = self.db_read_access()?;
        Ok(db.fetch_chain_metadata()?.accumulated_difficulty)
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

    /// Store the provided headers. This function does not do any validation and assumes the inserted header has already
    /// been validated.
    pub fn insert_valid_headers(&self, headers: Vec<BlockHeader>) -> Result<(), ChainStorageError> {
        let mut db = self.db_write_access()?;
        insert_headers(&mut *db, headers)
    }

    /// Returns the set of block headers specified by the block numbers.
    pub fn fetch_headers(&self, block_nums: Vec<u64>) -> Result<Vec<BlockHeader>, ChainStorageError> {
        let db = self.db_read_access()?;
        fetch_headers(&*db, block_nums)
    }

    /// Returns the block header corresponding` to the provided BlockHash
    pub fn fetch_header_by_block_hash(&self, hash: HashOutput) -> Result<BlockHeader, ChainStorageError> {
        let db = self.db_read_access()?;
        fetch_header_by_block_hash(&*db, hash)
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

    /// Spends the UTXO with the given hash
    pub fn spend_utxo(&self, hash: HashOutput) -> Result<(), ChainStorageError> {
        let mut db = self.db_write_access()?;
        spend_utxo(&mut *db, hash)
    }

    /// Returns the sum of all UTXO commitments
    pub fn fetch_utxo_commitment_sum(&self) -> Result<Commitment, ChainStorageError> {
        let db = self.db_read_access()?;
        let mut sum = Commitment::from_public_key(&PublicKey::default());
        db.for_each_utxo(|utxo| {
            if let Ok((_, output)) = utxo {
                sum = &sum + output.commitment();
            }
        })?;

        Ok(sum)
    }

    /// Returns the sum of all kernels
    pub fn fetch_kernel_commitment_sum(&self) -> Result<Commitment, ChainStorageError> {
        let db = self.db_read_access()?;
        let mut sum = Commitment::from_public_key(&PublicKey::default());
        db.for_each_kernel(|kernel| {
            if let Ok((_, kernel)) = kernel {
                sum = &sum + &kernel.excess
            }
        })?;

        Ok(sum)
    }

    /// Store the provided UTXO.
    pub fn insert_utxo(&self, utxo: TransactionOutput) -> Result<(), ChainStorageError> {
        let mut db = self.db_write_access()?;
        insert_utxo(&mut *db, utxo)
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
        db.fetch_target_difficulties(pow_algo, height, block_window)
    }

    /// Returns true if the given UTXO, represented by its hash exists in the UTXO set.
    pub fn is_utxo(&self, hash: HashOutput) -> Result<bool, ChainStorageError> {
        let key = DbKey::UnspentOutput(hash);
        let db = self.db_read_access()?;
        db.contains(&key)
    }

    /// Calculate the Merklish root of the specified merkle mountain range.
    pub fn fetch_mmr_root(&self, tree: MmrTree) -> Result<HashOutput, ChainStorageError> {
        let db = self.db_read_access()?;
        fetch_mmr_root(&*db, tree)
    }

    /// Returns only the MMR merkle root without the state of the roaring bitmap.
    pub fn fetch_mmr_only_root(&self, tree: MmrTree) -> Result<HashOutput, ChainStorageError> {
        let db = self.db_read_access()?;
        fetch_mmr_only_root(&*db, tree)
    }

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
    pub fn fetch_mmr_proof(&self, tree: MmrTree, pos: usize) -> Result<MerkleProof, ChainStorageError> {
        let db = self.db_read_access()?;
        fetch_mmr_proof(&*db, tree, pos)
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

    /// Validates the merkle root against the header at the given height
    pub fn validate_merkle_root(&self, tree: MmrTree, height: u64) -> Result<bool, ChainStorageError> {
        let db = self.db_read_access()?;
        db.validate_merkle_root(tree, height)
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
            &*self.validators.accum_difficulty,
            block,
        )?;

        // Cleanup orphan block pool
        match block_add_result {
            BlockAddResult::OrphanBlock | BlockAddResult::ChainReorg(_) => {
                cleanup_orphans(&mut *db, self.config.orphan_storage_capacity)?
            },
            _ => {},
        }

        // Cleanup of backend when in pruned mode.
        match block_add_result {
            BlockAddResult::Ok | BlockAddResult::ChainReorg(_) => prune_database(
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

    fn store_new_block(&self, block: Block) -> Result<(), ChainStorageError> {
        let mut txn = DbTransaction::new();
        store_new_block(&mut txn, block);
        let mut db = self.db_write_access()?;
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
        block_exists(&*db, hash)
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
    pub fn rewind_to_height(&self, height: u64) -> Result<Vec<Block>, ChainStorageError> {
        let mut db = self.db_write_access()?;
        rewind_to_height(&mut *db, height)
    }

    /// Prepares the database for horizon sync. This function sets the PendingHorizonSyncState for the database
    /// and sets the chain metadata to indicate that this node can not provide any sync data until sync is complete.
    pub fn horizon_sync_begin(&self) -> Result<InProgressHorizonSyncState, ChainStorageError> {
        let mut db = self.db_write_access()?;
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
                let metadata = db.fetch_chain_metadata()?;

                let state = InProgressHorizonSyncState {
                    metadata,
                    initial_kernel_checkpoint_count: db.count_checkpoints(MmrTree::Kernel)? as u64,
                    initial_utxo_checkpoint_count: db.count_checkpoints(MmrTree::Utxo)? as u64,
                    initial_rangeproof_checkpoint_count: db.count_checkpoints(MmrTree::Utxo)? as u64,
                };
                debug!(target: LOG_TARGET, "Preparing database for horizon sync. ({})", state);

                let mut txn = DbTransaction::new();

                txn.set_metadata(
                    MetadataKey::HorizonSyncState,
                    MetadataValue::HorizonSyncState(state.clone()),
                );

                // During horizon state syncing the blockchain backend will be in an inconsistent state until the entire
                // horizon state has been synced. Reset the local chain metadata will limit other nodes and
                // local service from requesting data while the horizon sync is in progress.
                txn.set_metadata(MetadataKey::ChainHeight, MetadataValue::ChainHeight(Some(0)));
                txn.set_metadata(
                    MetadataKey::EffectivePrunedHeight,
                    MetadataValue::EffectivePrunedHeight(0),
                );
                txn.set_metadata(MetadataKey::AccumulatedWork, MetadataValue::AccumulatedWork(None));
                commit(&mut *db, txn)?;

                Ok(state)
            },
        }
    }

    /// Commit the current synced horizon state.
    pub fn horizon_sync_commit(&self) -> Result<(), ChainStorageError> {
        let mut db = self.db_write_access()?;
        let tip_header = db
            .fetch_last_header()?
            .ok_or_else(|| ChainStorageError::InvalidQuery("Cannot retrieve header. Blockchain DB is empty".into()))?;

        let mut txn = DbTransaction::new();

        // Update metadata
        txn.set_metadata(
            MetadataKey::ChainHeight,
            MetadataValue::ChainHeight(Some(tip_header.height)),
        );

        let best_block = tip_header.hash();
        txn.set_metadata(MetadataKey::BestBlock, MetadataValue::BestBlock(Some(best_block)));

        let accumulated_difficulty =
            ProofOfWork::new_from_difficulty(&tip_header.pow, ProofOfWork::achieved_difficulty(&tip_header))
                .total_accumulated_difficulty();
        txn.set_metadata(
            MetadataKey::AccumulatedWork,
            MetadataValue::AccumulatedWork(Some(accumulated_difficulty)),
        );

        txn.set_metadata(
            MetadataKey::EffectivePrunedHeight,
            MetadataValue::EffectivePrunedHeight(tip_header.height),
        );

        // Merge all MMR checkpoints created during horizon sync into a single checkpoint
        txn.merge_checkpoints(1);

        // Remove pending horizon sync state
        txn.delete_metadata(MetadataKey::HorizonSyncState);

        commit(&mut *db, txn)
    }

    /// Rollback the current synced horizon state to a consistent state.
    pub fn horizon_sync_rollback(&self) -> Result<(), ChainStorageError> {
        let mut db = self.db_write_access()?;
        let sync_state = match get_horizon_sync_state(&*db)? {
            Some(state) => state,
            None => {
                debug!(target: LOG_TARGET, "Horizon sync: Nothing to roll back");
                return Ok(());
            },
        };

        let mut txn = DbTransaction::new();

        // Rollback added kernels
        let first_tmp_checkpoint_index =
            usize::try_from(sync_state.initial_kernel_checkpoint_count).map_err(|_| ChainStorageError::OutOfRange)?;
        let cp_count = db.count_checkpoints(MmrTree::Kernel)?;
        for i in first_tmp_checkpoint_index..cp_count {
            let cp = db.fetch_checkpoint_at_index(MmrTree::Kernel, i)?.expect(&format!(
                "Database is corrupt: Failed to fetch kernel checkpoint at index {}",
                i
            ));
            let (nodes_added, _) = cp.into_parts();
            for hash in nodes_added {
                txn.delete(DbKey::TransactionKernel(hash));
            }
        }

        txn.rewind_kernel_mmr(cp_count - first_tmp_checkpoint_index);

        // Rollback UTXO changes
        let first_tmp_checkpoint_index =
            usize::try_from(sync_state.initial_utxo_checkpoint_count).map_err(|_| ChainStorageError::OutOfRange)?;
        let cp_count = db.count_checkpoints(MmrTree::Utxo)?;
        for i in first_tmp_checkpoint_index..cp_count {
            let cp = db.fetch_checkpoint_at_index(MmrTree::Utxo, i)?.expect(&format!(
                "Database is corrupt: Failed to fetch UTXO checkpoint at index {}",
                i
            ));
            let (nodes_added, deleted) = cp.into_parts();
            for hash in nodes_added {
                txn.delete(DbKey::UnspentOutput(hash));
            }
            for pos in deleted.iter() {
                let (stxo_hash, is_deleted) = db.fetch_mmr_node(MmrTree::Utxo, pos, None)?;
                debug_assert!(is_deleted);
                txn.unspend_stxo(stxo_hash);
            }
        }

        txn.rewind_utxo_mmr(cp_count - first_tmp_checkpoint_index);

        // Rollback Rangeproof checkpoints
        let first_tmp_checkpoint_index = usize::try_from(sync_state.initial_rangeproof_checkpoint_count)
            .map_err(|_| ChainStorageError::OutOfRange)?;
        let rp_checkpoint_count = db.count_checkpoints(MmrTree::RangeProof)?;
        txn.rewind_rangeproof_mmr(rp_checkpoint_count - first_tmp_checkpoint_index);

        // Rollback metadata
        let metadata = sync_state.metadata;
        txn.set_metadata(
            MetadataKey::ChainHeight,
            MetadataValue::ChainHeight(metadata.height_of_longest_chain),
        );
        txn.set_metadata(MetadataKey::BestBlock, MetadataValue::BestBlock(metadata.best_block));
        txn.set_metadata(
            MetadataKey::AccumulatedWork,
            MetadataValue::AccumulatedWork(metadata.accumulated_difficulty),
        );
        txn.set_metadata(
            MetadataKey::EffectivePrunedHeight,
            MetadataValue::EffectivePrunedHeight(metadata.effective_pruned_height),
        );

        // Remove pending horizon sync state
        txn.delete_metadata(MetadataKey::HorizonSyncState);

        commit(&mut *db, txn)
    }

    /// Store the provided set of kernels and persists a checkpoint
    pub fn horizon_sync_insert_kernels(&self, kernels: Vec<TransactionKernel>) -> Result<(), ChainStorageError> {
        let mut db = self.db_write_access()?;
        let mut txn = DbTransaction::new();
        kernels.into_iter().for_each(|kernel| txn.insert_kernel(kernel));
        txn.create_mmr_checkpoint(MmrTree::Kernel);
        commit(&mut *db, txn)
    }

    /// Spends the UTXOs with the given hashes
    pub fn horizon_sync_spend_utxos(&self, hashes: Vec<HashOutput>) -> Result<(), ChainStorageError> {
        let mut db = self.db_write_access()?;
        let mut txn = DbTransaction::new();
        hashes.into_iter().for_each(|hash| txn.spend_utxo(hash));
        txn.create_mmr_checkpoint(MmrTree::Utxo);
        commit(&mut *db, txn)
    }

    /// Create a MMR checkpoint for the given `MmrTree`
    pub fn horizon_sync_create_mmr_checkpoint(&self, tree: MmrTree) -> Result<(), ChainStorageError> {
        let mut db = self.db_write_access()?;
        let mut txn = DbTransaction::new();
        txn.create_mmr_checkpoint(tree);
        commit(&mut *db, txn)
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
        MetadataValue::ChainHeight(metadata.height_of_longest_chain),
    );
    txn.set_metadata(MetadataKey::BestBlock, MetadataValue::BestBlock(metadata.best_block));
    txn.set_metadata(
        MetadataKey::AccumulatedWork,
        MetadataValue::AccumulatedWork(metadata.accumulated_difficulty),
    );
    commit(db, txn)
}

fn fetch_kernel<T: BlockchainBackend>(db: &T, hash: HashOutput) -> Result<TransactionKernel, ChainStorageError> {
    fetch!(db, hash, TransactionKernel)
}

pub fn fetch_header<T: BlockchainBackend>(db: &T, block_num: u64) -> Result<BlockHeader, ChainStorageError> {
    fetch!(db, block_num, BlockHeader)
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

fn insert_headers<T: BlockchainBackend>(db: &mut T, headers: Vec<BlockHeader>) -> Result<(), ChainStorageError> {
    let mut txn = DbTransaction::new();
    headers.into_iter().for_each(|header| {
        txn.insert_header(header);
    });
    commit(db, txn)
}

fn fetch_header_by_block_hash<T: BlockchainBackend>(db: &T, hash: BlockHash) -> Result<BlockHeader, ChainStorageError> {
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

fn insert_utxo<T: BlockchainBackend>(db: &mut T, utxo: TransactionOutput) -> Result<(), ChainStorageError> {
    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo);
    commit(db, txn)
}

fn fetch_stxo<T: BlockchainBackend>(db: &T, hash: HashOutput) -> Result<TransactionOutput, ChainStorageError> {
    fetch!(db, hash, SpentOutput)
}

fn spend_utxo<T: BlockchainBackend>(db: &mut T, hash: HashOutput) -> Result<(), ChainStorageError> {
    let mut txn = DbTransaction::new();
    txn.spend_utxo(hash);
    commit(db, txn)
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

fn is_stxo<T: BlockchainBackend>(db: &T, hash: HashOutput) -> Result<bool, ChainStorageError> {
    // Check if the UTXO MMR contains the specified deleted UTXO hash, the backend stxo_db is not used for this task as
    // archival nodes and pruning nodes might have different STXOs in their stxo_db as horizon state STXOs are
    // discarded by pruned nodes.
    if let Some(leaf_index) = db.fetch_mmr_leaf_index(MmrTree::Utxo, &hash)? {
        let (_, deleted) = db.fetch_mmr_node(MmrTree::Utxo, leaf_index, None)?;
        return Ok(deleted);
    }
    Ok(false)
}

fn fetch_mmr_root<T: BlockchainBackend>(db: &T, tree: MmrTree) -> Result<HashOutput, ChainStorageError> {
    db.fetch_mmr_root(tree)
}

fn fetch_mmr_only_root<T: BlockchainBackend>(db: &T, tree: MmrTree) -> Result<HashOutput, ChainStorageError> {
    db.fetch_mmr_only_root(tree)
}

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

/// Fetch a Merklish proof for the given hash, tree and position in the MMR
fn fetch_mmr_proof<T: BlockchainBackend>(db: &T, tree: MmrTree, pos: usize) -> Result<MerkleProof, ChainStorageError> {
    db.fetch_mmr_proof(tree, pos)
}

fn add_block<T: BlockchainBackend>(
    db: &mut T,
    block_validator: &Validator<Block, T>,
    accum_difficulty_validator: &Validator<Difficulty, T>,
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
fn store_new_block(txn: &mut DbTransaction, block: Block) {
    debug!(
        target: LOG_TARGET,
        "Storing new block #{} `{}`",
        block.header.height,
        block.hash().to_hex()
    );
    let (header, inputs, outputs, kernels) = block.dissolve();
    let height = header.height;
    let best_block = header.hash();
    let accumulated_difficulty =
        ProofOfWork::new_from_difficulty(&header.pow, ProofOfWork::achieved_difficulty(&header))
            .total_accumulated_difficulty();
    // Build all the DB queries needed to add the block and the add it atomically

    // Update metadata
    txn.set_metadata(MetadataKey::ChainHeight, MetadataValue::ChainHeight(Some(height)));
    txn.set_metadata(MetadataKey::BestBlock, MetadataValue::BestBlock(Some(best_block)));
    txn.set_metadata(
        MetadataKey::AccumulatedWork,
        MetadataValue::AccumulatedWork(Some(accumulated_difficulty)),
    );
    // Insert block
    txn.insert_header(header);
    txn.spend_inputs(&inputs);
    outputs.into_iter().for_each(|utxo| txn.insert_utxo(utxo));
    kernels.into_iter().for_each(|k| txn.insert_kernel(k));
    txn.commit_block();
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
    let tip_height = check_for_valid_height(&*db, height)?;
    let header = fetch_header(db, height)?;
    let kernel_cp = fetch_checkpoint(db, MmrTree::Kernel, height)?;
    let (kernel_hashes, _) = kernel_cp.into_parts();
    let kernels = fetch_kernels(db, kernel_hashes)?;
    let utxo_cp = fetch_checkpoint(db, MmrTree::Utxo, height)?;
    let (utxo_hashes, deleted_nodes) = utxo_cp.into_parts();
    let inputs = fetch_inputs(db, deleted_nodes)?;
    let (outputs, spent) = fetch_outputs(db, utxo_hashes)?;
    let block = header
        .into_builder()
        .add_inputs(inputs)
        .add_outputs(outputs)
        .add_kernels(kernels)
        .build();
    Ok(HistoricalBlock::new(block, tip_height - height + 1, spent))
}

fn fetch_block_with_kernel<T: BlockchainBackend>(
    db: &T,
    excess_sig: Signature,
) -> Result<Option<HistoricalBlock>, ChainStorageError>
{
    let metadata = db.fetch_chain_metadata()?;
    let db_height = metadata.height_of_longest_chain.unwrap_or(0);
    let horizon_height = metadata.horizon_block(db_height);
    for i in (horizon_height..db_height).rev() {
        let kernel_cp = fetch_checkpoint(db, MmrTree::Kernel, i)?;
        let (kernel_hashes, _) = kernel_cp.into_parts();
        let kernels = fetch_kernels(db, kernel_hashes)?;
        for kernel in kernels {
            if kernel.excess_sig == excess_sig {
                return Ok(Some(fetch_block(db, i)?));
            }
        }
    }
    // data is not in the pruning horizon, let's check behind that but only if there is a pruning horizon
    if horizon_height > 0 {
        let kernel_cp = fetch_checkpoint(db, MmrTree::Kernel, horizon_height - 1)?;
        let (kernel_hashes, _) = kernel_cp.into_parts();
        let kernels = fetch_kernels(db, kernel_hashes)?;
        for kernel in kernels {
            if kernel.excess_sig == excess_sig {
                return Ok(None);
            }
        }
    }
    Err(ChainStorageError::ValueNotFound {
        entity: "Kernel".to_string(),
        field: "Excess sig".to_string(),
        value: excess_sig.get_signature().to_hex(),
    })
}

fn fetch_block_with_utxo<T: BlockchainBackend>(
    db: &T,
    commitment: Commitment,
) -> Result<Option<HistoricalBlock>, ChainStorageError>
{
    let metadata = db.fetch_chain_metadata()?;
    let db_height = metadata.height_of_longest_chain.unwrap_or(0);
    let horizon_height = metadata.horizon_block(db_height);
    for i in (horizon_height..db_height).rev() {
        let utxo_cp = fetch_checkpoint(db, MmrTree::Utxo, i)?;
        let (utxo_hashes, _) = utxo_cp.into_parts();
        let utxos = fetch_outputs(db, utxo_hashes)?;
        for utxo in utxos.0 {
            if utxo.commitment == commitment {
                return Ok(Some(fetch_block(db, i)?));
            }
        }
        for comm in utxos.1 {
            if comm == commitment {
                return Ok(Some(fetch_block(db, i)?));
            }
        }
    }
    // data is not in the pruning horizon, let's check behind that but only if there is a pruning horizon
    if horizon_height > 0 {
        let utxo_cp = fetch_checkpoint(db, MmrTree::Utxo, horizon_height - 1)?;
        let (utxo_hashes, _) = utxo_cp.into_parts();
        let utxos = fetch_outputs(db, utxo_hashes)?;
        for utxo in utxos.0 {
            if utxo.commitment == commitment {
                return Ok(None);
            }
        }
        for comm in utxos.1 {
            if comm == commitment {
                return Ok(None);
            }
        }
    }
    Err(ChainStorageError::ValueNotFound {
        entity: "Utxo".to_string(),
        field: "Commitment".to_string(),
        value: commitment.to_hex(),
    })
}

fn fetch_block_with_stxo<T: BlockchainBackend>(
    db: &T,
    commitment: Commitment,
) -> Result<Option<HistoricalBlock>, ChainStorageError>
{
    let metadata = db.fetch_chain_metadata()?;
    let db_height = metadata.height_of_longest_chain.unwrap_or(0);
    let horizon_height = metadata.horizon_block(db_height);
    for i in (horizon_height..db_height).rev() {
        let utxo_cp = fetch_checkpoint(db, MmrTree::Utxo, i)?;
        let (_, deleted) = utxo_cp.into_parts();
        let inputs = fetch_inputs(db, deleted)?;
        for input in inputs {
            if input.commitment == commitment {
                return Ok(Some(fetch_block(db, i)?));
            }
        }
    }
    // data is not in the pruning horizon, we cannot check stxo's behind pruning horizon
    Err(ChainStorageError::ValueNotFound {
        entity: "Utxo".to_string(),
        field: "Commitment".to_string(),
        value: commitment.to_hex(),
    })
}

fn fetch_block_with_hash<T: BlockchainBackend>(
    db: &T,
    hash: BlockHash,
) -> Result<Option<HistoricalBlock>, ChainStorageError>
{
    if let Ok(header) = fetch_header_by_block_hash(db, hash.clone()) {
        return Ok(Some(fetch_block(db, header.height)?));
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
            db.fetch_mmr_node(MmrTree::Utxo, pos, None)
                .and_then(|(hash, deleted)| {
                    assert!(deleted);
                    fetch_stxo(db, hash)
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
            Err(ChainStorageError::ValueNotFound { .. }) => {}, // Check STXO set below
            Err(e) => return Err(e),                            // Something bad happened. Abort.
        }
        // Check the STXO set
        let stxo = fetch_stxo(db, hash)?;
        spent.push(stxo.commitment.clone());
        outputs.push(stxo);
    }
    Ok((outputs, spent))
}

fn fetch_checkpoint<T: BlockchainBackend>(
    db: &T,
    tree: MmrTree,
    height: u64,
) -> Result<MerkleCheckPoint, ChainStorageError>
{
    db.fetch_checkpoint_at_height(tree, height)
}

#[inline]
fn commit<T: BlockchainBackend>(db: &mut T, txn: DbTransaction) -> Result<(), ChainStorageError> {
    db.write(txn)
}

fn rewind_to_height<T: BlockchainBackend>(db: &mut T, height: u64) -> Result<Vec<Block>, ChainStorageError> {
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
        let orphaned_block = fetch_block(db, rewind_height)?.block().clone();
        removed_blocks.push(orphaned_block.clone());
        txn.insert_orphan(orphaned_block);

        // Remove Header and block hash
        txn.delete(DbKey::BlockHeader(rewind_height));

        // Remove Kernels
        let (nodes_added, _) = fetch_checkpoint(db, MmrTree::Kernel, rewind_height)?.into_parts();
        nodes_added.into_iter().for_each(|hash_output| {
            txn.delete(DbKey::TransactionKernel(hash_output));
        });

        // Remove UTXOs and move STXOs back to UTXO set
        let checkpoint = fetch_checkpoint(db, MmrTree::Utxo, rewind_height)?;
        let (nodes_added, nodes_deleted) = checkpoint.into_parts();
        for pos in nodes_deleted.iter() {
            let (stxo_hash, deleted) = db.fetch_mmr_node(MmrTree::Utxo, pos, None)?;
            if !deleted {
                warn!(
                    target: LOG_TARGET,
                    "**Database corruption detected** An MMR checkpoint at height {} indicated that a node {} was \
                     spent but the corresponding MMR node did not.",
                    rewind_height,
                    pos
                );
            }
            txn.unspend_stxo(stxo_hash);
        }

        // Delete nodes from the UTXO set
        nodes_added.iter().for_each(|hash_output| {
            txn.delete(DbKey::UnspentOutput(hash_output.clone()));
        });
    }
    // Rewind MMRs
    txn.rewind_kernel_mmr(steps_back);
    txn.rewind_utxo_mmr(steps_back);
    txn.rewind_rangeproof_mmr(steps_back);
    // Update metadata
    let last_header = fetch_header(db, height)?;
    let accumulated_work =
        ProofOfWork::new_from_difficulty(&last_header.pow, ProofOfWork::achieved_difficulty(&last_header))
            .total_accumulated_difficulty();
    txn.set_metadata(
        MetadataKey::ChainHeight,
        MetadataValue::ChainHeight(Some(last_header.height)),
    );
    txn.set_metadata(
        MetadataKey::BestBlock,
        MetadataValue::BestBlock(Some(last_header.hash())),
    );
    txn.set_metadata(
        MetadataKey::AccumulatedWork,
        MetadataValue::AccumulatedWork(Some(accumulated_work)),
    );
    commit(db, txn)?;

    Ok(removed_blocks)
}

// Checks whether we should add the block as an orphan. If it is the case, the orphan block is added and the chain
// is reorganised if necessary.
fn handle_possible_reorg<T: BlockchainBackend>(
    db: &mut T,
    block_validator: &Validator<Block, T>,
    accum_difficulty_validator: &Validator<Difficulty, T>,
    block: Block,
) -> Result<BlockAddResult, ChainStorageError>
{
    let db_height = db
        .fetch_chain_metadata()?
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
    db: &mut T,
    block_validator: &Validator<Block, T>,
    accum_difficulty_validator: &Validator<Difficulty, T>,
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
    let orphan_chain_tips = find_orphan_chain_tips(db, new_block.header.height, new_block_hash.clone());
    trace!(
        target: LOG_TARGET,
        "Search for orphan tips linked to block #{} complete.",
        new_block.header.height
    );
    // Check the accumulated difficulty of the best fork chain compared to the main chain.
    let (fork_accum_difficulty, fork_tip_hash) = find_strongest_orphan_tip(db, orphan_chain_tips)?;
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
    let fork_tip_block = fetch_orphan(db, fork_tip_hash.clone())?;
    let fork_tip_header = fork_tip_block.header.clone();
    if fork_tip_hash != new_block_hash {
        // New block is not the tip, find complete chain from tip to main chain.
        reorg_chain = try_construct_fork(db, fork_tip_block)?;
    }
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
            fork_tip_header,
            tip_header.pow,
            tip_header.hash().to_hex(),
            fork_tip_header.pow,
            fork_tip_hash.to_hex(),
            num_removed_blocks,
            num_added_blocks,
        );
        Ok(BlockAddResult::ChainReorg((
            Box::new(removed_blocks),
            Box::new(added_blocks),
        )))
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
    db: &mut T,
    block_validator: &Validator<Block, T>,
    height: u64,
    chain: VecDeque<Block>,
) -> Result<Vec<Block>, ChainStorageError>
{
    let removed_blocks = rewind_to_height(db, height)?;
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
        if let Err(e) = block_validator.validate(&block, db) {
            warn!(
                target: LOG_TARGET,
                "Orphan block {} ({}) failed validation during chain reorg: {}", block.header.height, block_hash_hex, e
            );
            remove_orphan(db, block.hash())?;

            info!(target: LOG_TARGET, "Restoring previous chain after failed reorg.");
            restore_reorged_chain(db, height, removed_blocks)?;
            return Err(e.into());
        }

        store_new_block(&mut txn, block);
        // Failed to store the block - this should typically never happen unless there is a bug in the validator
        // (e.g. does not catch a double spend). In any case, we still need to restore the chain to a
        // good state before returning.
        if let Err(e) = commit(db, txn) {
            warn!(
                target: LOG_TARGET,
                "Failed to commit reorg chain: {}. Restoring last chain.", e
            );

            restore_reorged_chain(db, height, removed_blocks)?;
            return Err(e.into());
        }
    }

    Ok(removed_blocks)
}

fn restore_reorged_chain<T: BlockchainBackend>(
    db: &mut T,
    height: u64,
    previous_chain: Vec<Block>,
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
        store_new_block(&mut txn, block);
    }
    commit(db, txn)?;
    Ok(())
}

// Insert the provided block into the orphan pool.
fn insert_orphan<T: BlockchainBackend>(db: &mut T, block: Block) -> Result<(), ChainStorageError> {
    let mut txn = DbTransaction::new();
    txn.insert_orphan(block);
    commit(db, txn)
}

// Discard the the orphan block from the orphan pool that corresponds to the provided block hash.
fn remove_orphan<T: BlockchainBackend>(db: &mut T, hash: HashOutput) -> Result<(), ChainStorageError> {
    let mut txn = DbTransaction::new();
    txn.delete(DbKey::OrphanBlock(hash));
    commit(db, txn)
}

/// We try and build a chain from this block to the main chain. If we can't do that we can stop.
/// We start with the current, newly received block, and look for a blockchain sequence (via `prev_hash`).
/// Each successful link is pushed to the front of the queue. An empty queue is returned if the fork chain did not
/// link to the main chain.
fn try_construct_fork<T: BlockchainBackend>(
    db: &mut T,
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
        if let Ok(header) = fetch_header_by_block_hash(db, fork_start_header.prev_hash) {
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
        match fetch_orphan(db, hash.clone()) {
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
            Err(ChainStorageError::ValueNotFound { .. }) => {
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
    let mut count = 0;
    let start = std::time::Instant::now();
    db.for_each_orphan(|pair| {
        count += 1;
        let (_, block) = pair.unwrap();
        if (block.header.prev_hash == parent_hash) && (block.header.height == parent_height + 1) {
            // we found a match, let save to call later
            parents.push((block.hash(), block.header.height));
        }
    })
    .expect("Unexpected result for database query");

    debug!(
        target: LOG_TARGET,
        "Searched {} orphan(s), found {} parent(s) in {:.0?}",
        count,
        parents.len(),
        start.elapsed()
    );
    // we need two for loops so that we ensure we release the db read lock as this iterative call can saturate all db
    // read locks. This ensures the call only uses one read lock.
    for (parent_hash, parent_height) in parents {
        let mut orphan_chain_tips = find_orphan_chain_tips(db, parent_height, parent_hash.clone());
        if !orphan_chain_tips.is_empty() {
            tip_hashes.append(&mut orphan_chain_tips);
        } else {
            tip_hashes.push(parent_hash);
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
fn cleanup_orphans<T: BlockchainBackend>(db: &mut T, orphan_storage_capacity: usize) -> Result<(), ChainStorageError> {
    let orphan_count = db.get_orphan_count()?;
    let num_over_limit = orphan_count.saturating_sub(orphan_storage_capacity);
    if num_over_limit > 0 {
        info!(
            target: LOG_TARGET,
            "Orphan block storage limit reached, performing cleanup.",
        );

        let mut orphans = Vec::<(u64, BlockHash)>::with_capacity(orphan_count);
        db.for_each_orphan(|pair| {
            let (block_hash, block) = pair.unwrap();
            orphans.push((block.header.height, block_hash));
        })
        .expect("Unexpected result for database query");
        orphans.sort_by(|a, b| a.0.cmp(&b.0));

        let metadata = db.fetch_chain_metadata()?;
        let horizon_height = metadata.horizon_block(metadata.height_of_longest_chain.unwrap_or(0));
        let mut txn = DbTransaction::new();
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
            txn.delete(DbKey::OrphanBlock(block_hash.clone()));
        }
        commit(db, txn)?;
    }
    Ok(())
}

fn prune_database<T: BlockchainBackend>(
    db: &mut T,
    pruning_height_interval: u64,
    pruning_horizon: u64,
    height: u64,
) -> Result<(), ChainStorageError>
{
    let metadata = db.fetch_chain_metadata()?;
    if metadata.is_pruned_node() {
        let db_height = metadata.height_of_longest_chain();
        if db_height % pruning_height_interval == 0 {
            info!(target: LOG_TARGET, "Pruning interval reached. Pruning the database.");
            let abs_pruning_horizon = height.saturating_sub(pruning_horizon);

            let mut txn = DbTransaction::new();
            let max_cp_count = pruning_horizon + 1; // Include accumulated checkpoint
            txn.merge_checkpoints(max_cp_count as usize);

            if abs_pruning_horizon > metadata.effective_pruned_height {
                txn.set_metadata(
                    MetadataKey::EffectivePrunedHeight,
                    MetadataValue::EffectivePrunedHeight(abs_pruning_horizon),
                );
            }
            commit(db, txn)?;
        }
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
        }
    }
}
