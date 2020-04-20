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
        consts::BLOCKCHAIN_DATABASE_ORPHAN_STORAGE_CAPACITY,
        db_transaction::{DbKey, DbKeyValuePair, DbTransaction, DbValue, MetadataKey, MetadataValue, MmrTree},
        error::ChainStorageError,
        ChainMetadata,
        HistoricalBlock,
    },
    consensus::ConsensusManager,
    proof_of_work::{Difficulty, ProofOfWork},
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
use tari_crypto::tari_utilities::{hex::Hex, Hashable};
use tari_mmr::{Hash, MerkleCheckPoint, MerkleProof, MutableMmrLeafNodes};

const LOG_TARGET: &str = "c::cs::database";

/// Configuration for the BlockchainDatabase.
#[derive(Clone, Copy)]
pub struct BlockchainDatabaseConfig {
    pub orphan_storage_capacity: usize,
}

impl Default for BlockchainDatabaseConfig {
    fn default() -> Self {
        Self {
            orphan_storage_capacity: BLOCKCHAIN_DATABASE_ORPHAN_STORAGE_CAPACITY,
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
pub struct Validators<B: BlockchainBackend> {
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

impl<B: BlockchainBackend> Clone for Validators<B> {
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
    /// Fetches the checkpoint corresponding to the provided height, the checkpoint consist of the list of nodes
    /// added & deleted for the given Merkle tree.
    fn fetch_checkpoint(&self, tree: MmrTree, height: u64) -> Result<MerkleCheckPoint, ChainStorageError>;
    /// Fetches the leaf node hash and its deletion status for the nth leaf node in the given MMR tree.
    fn fetch_mmr_node(&self, tree: MmrTree, pos: u32) -> Result<(Hash, bool), ChainStorageError>;
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
    fn fetch_metadata(&self) -> Result<ChainMetadata, ChainStorageError>;
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
pub struct BlockchainDatabase<T>
where T: BlockchainBackend
{
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
        if blockchain_db.get_height()?.is_none() {
            let genesis_block = consensus_manager.get_genesis_block();
            blockchain_db.store_new_block(genesis_block)?;
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
        Ok(db.fetch_metadata()?.clone())
    }

    /// Returns the transaction kernel with the given hash.
    pub fn fetch_kernel(&self, hash: HashOutput) -> Result<TransactionKernel, ChainStorageError> {
        let db = self.db_read_access()?;
        fetch_kernel(&*db, hash)
    }

    /// Returns the block header at the given block height.
    pub fn fetch_header(&self, block_num: u64) -> Result<BlockHeader, ChainStorageError> {
        let db = self.db_read_access()?;
        fetch_header(&*db, block_num)
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

    /// Returns the orphan block with the given hash.
    pub fn fetch_orphan(&self, hash: HashOutput) -> Result<Block, ChainStorageError> {
        let db = self.db_read_access()?;
        fetch_orphan(&*db, hash)
    }

    /// Returns true if the given UTXO, represented by its hash exists in the UTXO set.
    pub fn is_utxo(&self, hash: HashOutput) -> Result<bool, ChainStorageError> {
        let db = self.db_read_access()?;
        is_utxo(&*db, hash)
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

    /// Tries to add a block to the longest chain.
    ///
    /// The block is added to the longest chain if and only if
    ///   * Block block is not already in the database, AND
    ///   * The block is next in the chain, AND
    ///   * The Validator passes
    ///   * There are no problems with the database backend (e.g. disk full)
    ///
    /// If the block is _not_ next in the chain, the block will be added to the orphan pool if the orphan validator
    /// passes, and then the database is checked for whether there has been a chain re-organisation.
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
        self.validators.orphan.validate(&block)?;

        let mut db = self.db_write_access()?;
        add_block(
            &mut db,
            &self.validators.block,
            &self.validators.accum_difficulty,
            block,
            self.config.orphan_storage_capacity,
        )
    }

    fn store_new_block(&self, block: Block) -> Result<(), ChainStorageError> {
        let mut db = self.db_write_access()?;
        store_new_block(&mut db, block)
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
    pub fn fetch_block_with_hash(&self, hash: HashOutput) -> Result<Option<HistoricalBlock>, ChainStorageError> {
        let db = self.db_read_access()?;
        fetch_block_with_hash(&*db, hash)
    }

    /// Atomically commit the provided transaction to the database backend. This function does not update the metadata.
    pub fn commit(&self, txn: DbTransaction) -> Result<(), ChainStorageError> {
        let mut db = self.db_write_access()?;
        commit(&mut db, txn)
    }

    /// Rewind the blockchain state to the block height given and return the blocks that were removed and orphaned.
    ///
    /// The operation will fail if
    /// * The block height is in the future
    /// * The block height is before pruning horizon
    pub fn rewind_to_height(&self, height: u64) -> Result<Vec<Block>, ChainStorageError> {
        let mut db = self.db_write_access()?;
        rewind_to_height(&mut db, height)
    }
}

fn unexpected_result<T>(req: DbKey, res: DbValue) -> Result<T, ChainStorageError> {
    let msg = format!("Unexpected result for database query {}. Response: {}", req, res);
    error!(target: LOG_TARGET, "{}", msg);
    Err(ChainStorageError::UnexpectedResult(msg))
}

fn fetch_kernel<T: BlockchainBackend>(db: &T, hash: HashOutput) -> Result<TransactionKernel, ChainStorageError> {
    fetch!(db, hash, TransactionKernel)
}

pub fn fetch_header<T: BlockchainBackend>(db: &T, block_num: u64) -> Result<BlockHeader, ChainStorageError> {
    fetch!(db, block_num, BlockHeader)
}

fn fetch_header_with_block_hash<T: BlockchainBackend>(
    db: &T,
    hash: HashOutput,
) -> Result<BlockHeader, ChainStorageError>
{
    fetch!(db, hash, BlockHash)
}

fn fetch_tip_header<T: BlockchainBackend>(db: &T) -> Result<BlockHeader, ChainStorageError> {
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

fn fetch_orphan<T: BlockchainBackend>(db: &T, hash: HashOutput) -> Result<Block, ChainStorageError> {
    fetch!(db, hash, OrphanBlock)
}

pub fn is_utxo<T: BlockchainBackend>(db: &T, hash: HashOutput) -> Result<bool, ChainStorageError> {
    let key = DbKey::UnspentOutput(hash);
    db.contains(&key)
}

pub fn is_stxo<T: BlockchainBackend>(db: &T, hash: HashOutput) -> Result<bool, ChainStorageError> {
    let key = DbKey::SpentOutput(hash);
    db.contains(&key)
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
    db: &mut RwLockWriteGuard<T>,
    block_validator: &Arc<Validator<Block, T>>,
    accum_difficulty_validator: &Arc<Validator<Difficulty, T>>,
    block: Block,
    orphan_storage_capacity: usize,
) -> Result<BlockAddResult, ChainStorageError>
{
    let block_hash = block.hash();
    if db.contains(&DbKey::BlockHash(block_hash))? {
        return Ok(BlockAddResult::BlockExists);
    }
    let block_add_result = handle_possible_reorg(db, block_validator, accum_difficulty_validator, block)?;
    // Cleanup orphan block pool
    match block_add_result {
        BlockAddResult::Ok => {},
        BlockAddResult::BlockExists => {},
        BlockAddResult::OrphanBlock => cleanup_orphans_single(db, orphan_storage_capacity)?,
        BlockAddResult::ChainReorg(_) => cleanup_orphans_comprehensive(db, orphan_storage_capacity)?,
    }
    Ok(block_add_result)
}

// Adds a new block onto the chain tip.
fn store_new_block<T: BlockchainBackend>(db: &mut RwLockWriteGuard<T>, block: Block) -> Result<(), ChainStorageError> {
    let (header, inputs, outputs, kernels) = block.dissolve();
    let height = header.height;
    let best_block = header.hash();
    let accumulated_difficulty =
        ProofOfWork::new_from_difficulty(&header.pow, ProofOfWork::achieved_difficulty(&header))
            .total_accumulated_difficulty();
    // Build all the DB queries needed to add the block and the add it atomically
    let mut txn = DbTransaction::new();
    // Update metadata
    txn.insert(DbKeyValuePair::Metadata(
        MetadataKey::ChainHeight,
        MetadataValue::ChainHeight(Some(height)),
    ));
    txn.insert(DbKeyValuePair::Metadata(
        MetadataKey::BestBlock,
        MetadataValue::BestBlock(Some(best_block.clone())),
    ));
    txn.insert(DbKeyValuePair::Metadata(
        MetadataKey::AccumulatedWork,
        MetadataValue::AccumulatedWork(Some(accumulated_difficulty)),
    ));
    // Insert block
    txn.insert_header(header);
    txn.spend_inputs(&inputs);
    outputs.iter().for_each(|utxo| txn.insert_utxo(utxo.clone(), true));
    kernels.iter().for_each(|k| txn.insert_kernel(k.clone(), true));
    txn.commit_block();
    commit(db, txn)?;
    Ok(())
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

fn fetch_block_with_hash<T: BlockchainBackend>(
    db: &T,
    hash: HashOutput,
) -> Result<Option<HistoricalBlock>, ChainStorageError>
{
    if let Ok(header) = fetch_header_with_block_hash(db, hash.clone()) {
        return Ok(Some(fetch_block(db, header.height)?));
    }
    if let Ok(block) = fetch_orphan(db, hash) {
        return Ok(Some(HistoricalBlock::new(block, 0, vec![])));
    }
    Ok(None)
}

fn check_for_valid_height<T: BlockchainBackend>(db: &T, height: u64) -> Result<u64, ChainStorageError> {
    let db_height = db.fetch_metadata()?.height_of_longest_chain.unwrap_or(0);
    if height > db_height {
        return Err(ChainStorageError::InvalidQuery(format!(
            "Cannot get block at height {}. Chain tip is at {}",
            height, db_height
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
            db.fetch_mmr_node(MmrTree::Utxo, pos)
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

fn fetch_checkpoint<T: BlockchainBackend>(
    db: &T,
    tree: MmrTree,
    height: u64,
) -> Result<MerkleCheckPoint, ChainStorageError>
{
    db.fetch_checkpoint(tree, height)
}

pub fn commit<T: BlockchainBackend>(db: &mut RwLockWriteGuard<T>, txn: DbTransaction) -> Result<(), ChainStorageError> {
    db.write(txn)
}

fn rewind_to_height<T: BlockchainBackend>(
    db: &mut RwLockWriteGuard<T>,
    height: u64,
) -> Result<Vec<Block>, ChainStorageError>
{
    let chain_height = check_for_valid_height(&**db, height)?;
    let mut removed_blocks = Vec::<Block>::new();
    if height == chain_height {
        return Ok(removed_blocks); // Rewind unnecessary, already on correct height
    }
    let steps_back = (chain_height - height) as usize;
    let mut txn = DbTransaction::new();
    // Rewind operation must be performed in reverse from tip to height+1.
    for rewind_height in ((height + 1)..=chain_height).rev() {
        // Reconstruct block at height and add to orphan block pool
        let orphaned_block = fetch_block(&**db, rewind_height)?.block().clone();
        removed_blocks.push(orphaned_block.clone());
        txn.insert_orphan(orphaned_block);

        // Remove Header and block hash
        txn.delete(DbKey::BlockHeader(rewind_height)); // Will also delete the blockhash

        // Remove Kernels
        fetch_checkpoint(&**db, MmrTree::Kernel, rewind_height)?
            .nodes_added()
            .iter()
            .for_each(|hash_output| {
                txn.delete(DbKey::TransactionKernel(hash_output.clone()));
            });

        // Remove UTXOs and move STXOs back to UTXO set
        let (nodes_added, nodes_deleted) = fetch_checkpoint(&**db, MmrTree::Utxo, rewind_height)?.into_parts();
        nodes_added.iter().for_each(|hash_output| {
            txn.delete(DbKey::UnspentOutput(hash_output.clone()));
        });
        for pos in nodes_deleted.iter() {
            db.fetch_mmr_node(MmrTree::Utxo, pos).and_then(|(stxo_hash, deleted)| {
                assert!(deleted);
                txn.unspend_stxo(stxo_hash);
                Ok(())
            })?;
        }
    }
    // Rewind MMRs
    txn.rewind_kernel_mmr(steps_back);
    txn.rewind_utxo_mmr(steps_back);
    txn.rewind_rp_mmr(steps_back);
    // Update metadata
    let last_header = fetch_header(&**db, height)?;
    let accumulated_work =
        ProofOfWork::new_from_difficulty(&last_header.pow, ProofOfWork::achieved_difficulty(&last_header))
            .total_accumulated_difficulty();
    txn.insert(DbKeyValuePair::Metadata(
        MetadataKey::ChainHeight,
        MetadataValue::ChainHeight(Some(last_header.height)),
    ));
    txn.insert(DbKeyValuePair::Metadata(
        MetadataKey::BestBlock,
        MetadataValue::BestBlock(Some(last_header.hash())),
    ));
    txn.insert(DbKeyValuePair::Metadata(
        MetadataKey::AccumulatedWork,
        MetadataValue::AccumulatedWork(Some(accumulated_work)),
    ));
    commit(db, txn)?;

    Ok(removed_blocks)
}

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
    insert_orphan(db, block.clone())?;
    info!(
        target: LOG_TARGET,
        "Added new orphan block to the database. Current best height is {}. Orphan block height is {}",
        db_height,
        block.header.height
    );
    trace!(target: LOG_TARGET, "{}", block);
    // Trigger a reorg check for all blocks in the orphan block pool
    debug!(target: LOG_TARGET, "Checking for chain re-org.");
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
    block_validator: &Arc<Validator<Block, T>>,
    accum_difficulty_validator: &Arc<Validator<Difficulty, T>>,
    new_block: Block,
) -> Result<BlockAddResult, ChainStorageError>
{
    // We can assume that the new block is part of the re-org chain if it exists, otherwise the re-org would have
    // happened on the previous call to this function.
    // Try and construct a path from `new_block` to the main chain:
    let mut reorg_chain = try_construct_fork(db, new_block.clone())?;
    if reorg_chain.is_empty() {
        trace!(
            target: LOG_TARGET,
            "Could not find complete chain to orphan. No need to reorg at this time"
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
    trace!(
        target: LOG_TARGET,
        "Comparing fork diff: ({}) with hash ({}) to main chain diff: ({}) with hash ({}) for possible reorg",
        fork_accum_difficulty,
        fork_tip_hash.to_hex(),
        tip_header.total_accumulated_difficulty_inclusive(),
        tip_header.hash().to_hex()
    );
    if accum_difficulty_validator.validate(&fork_accum_difficulty, db).is_ok() {
        // We've built the strongest orphan chain we can by going backwards and forwards from the new orphan block
        // that is linked with the main chain.
        let fork_tip_block = fetch_orphan(&**db, fork_tip_hash.clone())?;
        let fork_tip_header = fork_tip_block.header.clone();
        if fork_tip_hash != new_block_hash {
            // New block is not the tip, find complete chain from tip to main chain.
            reorg_chain = try_construct_fork(db, fork_tip_block)?;
        }
        let added_blocks: Vec<Block> = reorg_chain.iter().map(Clone::clone).collect();
        let fork_height = reorg_chain
            .front()
            .expect("The new orphan block should be in the queue")
            .header
            .height -
            1;
        let removed_blocks = reorganize_chain(db, block_validator, fork_height, reorg_chain)?;
        if removed_blocks.is_empty() {
            return Ok(BlockAddResult::Ok);
        } else {
            warn!(
                target: LOG_TARGET,
                "Chain reorg happened from difficulty: ({}) to difficulty: ({})", tip_header.pow, fork_tip_header.pow
            );
            debug!(
                target: LOG_TARGET,
                "Reorg from ({}) to ({})", tip_header, fork_tip_header
            );
            return Ok(BlockAddResult::ChainReorg((
                Box::new(removed_blocks),
                Box::new(added_blocks),
            )));
        }
    }
    debug!(target: LOG_TARGET, "Orphan block received: {}", new_block);
    Ok(BlockAddResult::OrphanBlock)
}

// Reorganize the main chain with the provided fork chain, starting at the specified height.
fn reorganize_chain<T: BlockchainBackend>(
    db: &mut RwLockWriteGuard<T>,
    block_validator: &Arc<Validator<Block, T>>,
    height: u64,
    chain: VecDeque<Block>,
) -> Result<Vec<Block>, ChainStorageError>
{
    let removed_blocks = rewind_to_height(db, height)?;
    trace!(target: LOG_TARGET, "Validate and add chain blocks.",);
    let mut validation_result: Result<(), ValidationError> = Ok(());
    let mut orphan_hashes = Vec::<BlockHash>::with_capacity(chain.len());
    for block in chain {
        let block_hash = block.hash();
        orphan_hashes.push(block_hash.clone());
        validation_result = block_validator.validate(&block, db);
        if validation_result.is_err() {
            debug!(
                target: LOG_TARGET,
                "Orphan block {} failed validation during chain reorganization",
                block_hash.to_hex(),
            );
            remove_orphan(db, block.hash())?;
            break;
        }
        store_new_block(db, block)?;
    }

    match validation_result {
        Ok(_) => {
            trace!(target: LOG_TARGET, "Removing reorged orphan blocks.",);
            if !orphan_hashes.is_empty() {
                let mut txn = DbTransaction::new();
                for orphan_hash in orphan_hashes {
                    txn.delete(DbKey::OrphanBlock(orphan_hash));
                }
                commit(db, txn)?;
            }
            Ok(removed_blocks)
        },
        Err(e) => {
            trace!(target: LOG_TARGET, "Restoring previous chain after failed reorg.",);
            let invalid_chain = rewind_to_height(db, height)?;
            debug!(
                target: LOG_TARGET,
                "Removed incomplete chain of blocks during chain restore: {:?}.",
                invalid_chain
                    .iter()
                    .map(|block| block.hash().to_hex())
                    .collect::<Vec<_>>(),
            );
            let mut txn = DbTransaction::new();
            for block in removed_blocks {
                txn.delete(DbKey::OrphanBlock(block.hash()));
                store_new_block(db, block)?;
            }
            commit(db, txn)?;
            Err(e.into())
        },
    }
}

// Insert the provided block into the orphan pool.
fn insert_orphan<T: BlockchainBackend>(db: &mut RwLockWriteGuard<T>, block: Block) -> Result<(), ChainStorageError> {
    let mut txn = DbTransaction::new();
    txn.insert_orphan(block);
    commit(db, txn)
}

// Discard the the orphan block from the orphan pool that corresponds to the provided block hash.
fn remove_orphan<T: BlockchainBackend>(
    db: &mut RwLockWriteGuard<T>,
    hash: HashOutput,
) -> Result<(), ChainStorageError>
{
    let mut txn = DbTransaction::new();
    txn.delete(DbKey::OrphanBlock(hash));
    commit(db, txn)
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
    let mut hash = new_block.header.prev_hash.clone();
    let mut height = new_block.header.height;
    fork_chain.push_front(new_block);

    loop {
        let fork_start_header = fork_chain
            .front()
            .expect("The new orphan block should be in the queue")
            .header
            .clone();
        trace!(
            target: LOG_TARGET,
            "Checking if block {} ({}) is connected to the main chain.",
            fork_start_header.height,
            fork_start_header.hash().to_hex(),
        );
        if let Ok(header) = fetch_header_with_block_hash(&**db, fork_start_header.prev_hash) {
            if header.height + 1 == fork_start_header.height {
                trace!(
                    target: LOG_TARGET,
                    "Connection with main chain found at block {} ({}).",
                    header.height,
                    header.hash().to_hex(),
                );
                return Ok(fork_chain);
            }
        }

        trace!(
            target: LOG_TARGET,
            "Not connected, checking if fork chain can be extended.",
        );
        match fetch_orphan(&**db, hash.clone()) {
            Ok(prev_block) => {
                trace!(
                    target: LOG_TARGET,
                    "Checking if block {} ({}) forms a sequence with next block.",
                    prev_block.header.height,
                    hash.to_hex(),
                );
                if prev_block.header.height + 1 != height {
                    // Well now. The block heights don't form a sequence, which means that we should not only stop now,
                    // but remove one or both of these orphans from the pool because the blockchain is broken at this
                    // point.
                    info!(
                        target: LOG_TARGET,
                        "A broken blockchain sequence was detected, removing block {} ({}).",
                        prev_block.header.height,
                        hash.to_hex()
                    );
                    remove_orphan(db, hash)?;
                    return Err(ChainStorageError::InvalidBlock);
                }
                trace!(
                    target: LOG_TARGET,
                    "Fork chain extended with block {} ({}).",
                    prev_block.header.height,
                    hash.to_hex(),
                );
                hash = prev_block.header.prev_hash.clone();
                height -= 1;
                fork_chain.push_front(prev_block);
            },
            Err(ChainStorageError::ValueNotFound(_)) => {
                trace!(
                    target: LOG_TARGET,
                    "Fork chain extension not found and it isn't connected to the main chain.",
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
    db.for_each_orphan(|pair| {
        let (_, block) = pair.unwrap();
        if (block.header.prev_hash == parent_hash) && (block.header.height == parent_height + 1) {
            // we found a match, let save to call later
            parents.push((block.hash(), block.header.height));
        }
    })
    .expect("Unexpected result for database query");
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

// Discards the orphan block with the minimum height from the block orphan pool to maintain the configured orphan pool
// storage limit.
fn cleanup_orphans_single<T: BlockchainBackend>(
    db: &mut RwLockWriteGuard<T>,
    orphan_storage_capacity: usize,
) -> Result<(), ChainStorageError>
{
    if db.get_orphan_count()? > orphan_storage_capacity {
        trace!(
            target: LOG_TARGET,
            "Orphan block storage limit reached, performing simple cleanup.",
        );
        let mut min_height: u64 = u64::max_value();
        let mut remove_hash: Option<BlockHash> = None;
        db.for_each_orphan(|pair| {
            let (_, block) = pair.unwrap();
            if block.header.height < min_height {
                min_height = block.header.height;
                remove_hash = Some(block.hash());
            }
        })
        .expect("Unexpected result for database query");
        if let Some(hash) = remove_hash {
            trace!(target: LOG_TARGET, "Discarding orphan block ({}).", hash.to_hex());
            remove_orphan(db, hash)?;
        }
    }
    Ok(())
}

// Perform a comprehensive search to remove all the minimum height orphans to maintain the configured orphan pool
// storage limit.
fn cleanup_orphans_comprehensive<T: BlockchainBackend>(
    db: &mut RwLockWriteGuard<T>,
    orphan_storage_capacity: usize,
) -> Result<(), ChainStorageError>
{
    let orphan_count = db.get_orphan_count()?;
    if orphan_count > orphan_storage_capacity {
        trace!(
            target: LOG_TARGET,
            "Orphan block storage limit reached, performing comprehensive cleanup.",
        );
        let remove_count = orphan_count - orphan_storage_capacity;

        let mut orphans = Vec::<(u64, BlockHash)>::with_capacity(orphan_count);
        db.for_each_orphan(|pair| {
            let (_, block) = pair.unwrap();
            orphans.push((block.header.height, block.hash()));
        })
        .expect("Unexpected result for database query");
        orphans.sort_by(|a, b| a.0.cmp(&b.0));

        let mut txn = DbTransaction::new();
        for i in 0..remove_count {
            trace!(
                target: LOG_TARGET,
                "Discarding orphan block ({}).",
                orphans[i].1.to_hex()
            );
            txn.delete(DbKey::OrphanBlock(orphans[i].1.clone()));
        }
        commit(db, txn)?;
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

impl<T> Clone for BlockchainDatabase<T>
where T: BlockchainBackend
{
    fn clone(&self) -> Self {
        BlockchainDatabase {
            db: self.db.clone(),
            validators: self.validators.clone(),
            config: self.config.clone(),
        }
    }
}
