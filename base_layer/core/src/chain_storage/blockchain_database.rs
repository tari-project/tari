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
//

use crate::{
    blocks::{Block, BlockBuilder, BlockHeader},
    chain_storage::{
        db_transaction::{DbKey, DbTransaction, DbValue, MetadataKey, MetadataValue, MmrTree},
        error::ChainStorageError,
        ChainMetadata,
        HistoricalBlock,
    },
    proof_of_work::Difficulty,
    transaction::{TransactionInput, TransactionKernel, TransactionOutput},
    types::{Commitment, HashOutput},
};
use croaring::Bitmap;
use log::*;
use std::sync::{Arc, RwLock, RwLockReadGuard};
use tari_mmr::{Hash, MerkleCheckPoint, MerkleProof};
use tari_utilities::Hashable;

const LOG_TARGET: &str = "core::chain_storage::database";

#[derive(Clone, Debug, PartialEq)]
pub enum BlockAddResult {
    Ok,
    BlockExists,
    OrphanBlock,
    ChainReorg,
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
    fn write(&self, tx: DbTransaction) -> Result<(), ChainStorageError>;
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
    /// Constructs a merkle proof for the specified merkle mountain range and the given leaf position.
    fn fetch_mmr_proof(&self, tree: MmrTree, pos: usize) -> Result<MerkleProof, ChainStorageError>;
    /// The nth MMR checkpoint (the list of nodes added & deleted) for the given Merkle tree. The index is the n-th
    /// checkpoint (block) from the pruning horizon block.
    fn fetch_mmr_checkpoint(&self, tree: MmrTree, index: u64) -> Result<MerkleCheckPoint, ChainStorageError>;
    /// Fetches the leaf node hash and its deletion status for the nth leaf node in the given MMR tree.
    fn fetch_mmr_node(&self, tree: MmrTree, pos: u32) -> Result<(Hash, bool), ChainStorageError>;
}

// Private macro that pulls out all the boiler plate of extracting a DB query result from its variants
macro_rules! fetch {
    ($self:ident, $key_val:expr, $key_var:ident) => {{
        let key = DbKey::$key_var($key_val);
        match $self.db.fetch(&key) {
            Ok(None) => Err(ChainStorageError::ValueNotFound(key)),
            Ok(Some(DbValue::$key_var(k))) => Ok(*k),
            Ok(Some(other)) => unexpected_result(key, other),
            Err(e) => log_error(key, e),
        }
    }};

    (meta $db:expr, $meta_key:ident, $default:expr) => {{
        match $db.fetch(&DbKey::Metadata(MetadataKey::$meta_key)) {
            Ok(None) => {
                warn!(
                    target: LOG_TARGET,
                    "The {} entry is not present in the database. Assuming the database is empty.",
                    DbKey::Metadata(MetadataKey::$meta_key)
                );
                $default
            },
            Ok(Some(DbValue::Metadata(MetadataValue::$meta_key(v)))) => v,
            Ok(Some(other)) => return unexpected_result(DbKey::Metadata(MetadataKey::$meta_key), other),
            Err(e) => return log_error(DbKey::Metadata(MetadataKey::$meta_key), e),
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
///     chain_storage::{BlockchainDatabase, MemoryDatabase},
///     types::HashDigest,
/// };
/// let db_backend = MemoryDatabase::<HashDigest>::default();
/// let mut db = BlockchainDatabase::new(db_backend).unwrap();
/// // Do stuff with db
/// ```
pub struct BlockchainDatabase<T>
where T: BlockchainBackend
{
    metadata: Arc<RwLock<ChainMetadata>>,
    db: Arc<T>,
}

impl<T> BlockchainDatabase<T>
where T: BlockchainBackend
{
    /// Creates a new `BlockchainDatabase` using the provided backend.
    pub fn new(db: T) -> Result<Self, ChainStorageError> {
        let metadata = Self::read_metadata(&db)?;
        Ok(BlockchainDatabase {
            metadata: Arc::new(RwLock::new(metadata)),
            db: Arc::new(db),
        })
    }

    /// Reads the blockchain metadata (block height etc) from the underlying backend and returns it.
    /// If the metadata values aren't in the database, (e.g. when running a node for the first time),
    /// then log as much and return a reasonable default.
    fn read_metadata(db: &T) -> Result<ChainMetadata, ChainStorageError> {
        let height = fetch!(meta db, ChainHeight, None);
        let hash = fetch!(meta db, BestBlock, None);
        let work = fetch!(meta db, AccumulatedWork, 0);
        // Set a default of 2880 blocks (2 days with 1min blocks)
        let horizon = fetch!(meta db, PruningHorizon, 2880);
        Ok(ChainMetadata {
            height_of_longest_chain: height,
            best_block: hash,
            total_accumulated_difficulty: work,
            pruning_horizon: horizon,
        })
    }

    /// If a call to any metadata function fails, you can try and force a re-sync with this function. If the RWLock
    /// is poisoned because a write attempt failed, this function will replace the old lock with a new one with data
    /// freshly read from the underlying database. If this still fails, there's probably something badly wrong.
    ///
    /// # Returns
    ///  Ok(true) - The lock was refreshed and data was successfully re-read from the database. Proceed with caution.
    ///             The database *may* be inconsistent.
    /// Ok(false) - Everything looks fine. Why did you call this function again?
    /// Err(ChainStorageError::CriticalError) - Refreshing the lock failed. We couldn't refresh the metadata from the DB
    ///             backend, so you should probably just shut things down and look at the logs.
    pub fn try_recover_metadata(&mut self) -> Result<bool, ChainStorageError> {
        if !self.metadata.is_poisoned() {
            // metadata is fine. Nothing to do here
            return Ok(false);
        }
        match BlockchainDatabase::read_metadata(self.db.as_ref()) {
            Ok(data) => {
                self.metadata = Arc::new(RwLock::new(data));
                Ok(true)
            },
            Err(e) => {
                error!(
                    target: LOG_TARGET,
                    "Could not read metadata from database. {}. We're going to panic here. Perhaps restarting will \
                     fix things",
                    e.to_string()
                );
                Err(ChainStorageError::CriticalError)
            },
        }
    }

    fn access_metadata(&self) -> Result<RwLockReadGuard<ChainMetadata>, ChainStorageError> {
        self.metadata.read().map_err(|e| {
            error!(
                target: LOG_TARGET,
                "An attempt to get sa read lock on the blockchain metadata failed. {}",
                e.to_string()
            );
            ChainStorageError::AccessError("Read lock on blockchain metadata failed".into())
        })
    }

    fn update_metadata(&self, new_height: u64, new_hash: Vec<u8>) -> Result<(), ChainStorageError> {
        let mut db = self.metadata.write().map_err(|_| {
            ChainStorageError::AccessError(
                "Could not obtain write access to blockchain metadata after storing block".into(),
            )
        })?;
        db.height_of_longest_chain = Some(new_height);
        db.best_block = Some(new_hash);
        Ok(())
    }

    /// Returns the height of the current longest chain. This method will only fail if there's a fairly serious
    /// synchronisation problem on the database. You can try calling [BlockchainDatabase::try_recover_metadata] in
    /// that case to re-sync the metadata; or else just exit the program.
    ///
    /// If the chain is empty (the genesis block hasn't been added yet), this function returns `None`
    pub fn get_height(&self) -> Result<Option<u64>, ChainStorageError> {
        let metadata = self.access_metadata()?;
        Ok(metadata.height_of_longest_chain)
    }

    /// Returns a copy of the current blockchain database metadata
    pub fn get_metadata(&self) -> Result<ChainMetadata, ChainStorageError> {
        let db = self.access_metadata()?;
        Ok(db.clone())
    }

    /// Returns the total accumulated work/difficulty of the longest chain.
    ///
    /// This method will only fail if there's a fairly serious synchronisation problem on the database. You can try
    /// calling [BlockchainDatabase::try_recover_metadata] in that case to re-sync the metadata; or else
    /// just exit the program.
    pub fn get_total_work(&self) -> Result<Difficulty, ChainStorageError> {
        let metadata = self.access_metadata()?;
        Ok(metadata.total_accumulated_difficulty.into())
    }

    /// Returns the transaction kernel with the given hash.
    pub fn fetch_kernel(&self, hash: HashOutput) -> Result<TransactionKernel, ChainStorageError> {
        fetch!(self, hash, TransactionKernel)
    }

    /// Returns the block header at the given block height.
    pub fn fetch_header(&self, block_num: u64) -> Result<BlockHeader, ChainStorageError> {
        fetch!(self, block_num, BlockHeader)
    }

    /// Returns the UTXO with the given hash.
    pub fn fetch_utxo(&self, hash: HashOutput) -> Result<TransactionOutput, ChainStorageError> {
        fetch!(self, hash, UnspentOutput)
    }

    /// Returns the STXO with the given hash.
    pub fn fetch_stxo(&self, hash: HashOutput) -> Result<TransactionOutput, ChainStorageError> {
        fetch!(self, hash, SpentOutput)
    }

    /// Returns the orphan block with the given hash.
    pub fn fetch_orphan(&self, hash: HashOutput) -> Result<Block, ChainStorageError> {
        fetch!(self, hash, OrphanBlock)
    }

    /// Returns true if the given UTXO, represented by its hash exists in the UTXO set.
    pub fn is_utxo(&self, hash: HashOutput) -> Result<bool, ChainStorageError> {
        let key = DbKey::UnspentOutput(hash);
        self.db.contains(&key)
    }

    /// Calculate the Merklish root of the specified merkle mountain range.
    pub fn fetch_mmr_root(&self, tree: MmrTree) -> Result<HashOutput, ChainStorageError> {
        self.db.fetch_mmr_root(tree)
    }

    /// Returns only the MMR merkle root without the state of the roaring bitmap.
    pub fn fetch_mmr_only_root(&self, tree: MmrTree) -> Result<HashOutput, ChainStorageError> {
        self.db.fetch_mmr_only_root(tree)
    }

    /// Fetch a Merklish proof for the given hash, tree and position in the MMR
    pub fn fetch_mmr_proof(&self, tree: MmrTree, pos: usize) -> Result<MerkleProof, ChainStorageError> {
        self.db.fetch_mmr_proof(tree, pos)
    }

    /// Add a block to the longest chain. This function does some basic checks to maintain the chain integrity, but
    /// does not perform a full block validation (this should have been done by this point).
    ///
    /// On completion, this function will have
    ///   * Checked that the previous block builds on the longest chain.
    ///       * If not - add orphan block and possibly re-org
    ///   * That the total accumulated work has increased.
    ///   * Mark all inputs in the block as spent.
    ///   * Updated the database metadata
    ///
    /// An error is returned if:
    ///   * the block has already been added
    ///   * any of the inputs were not in the UTXO set or were marked as spent already
    ///
    /// If an error does occur while writing the new block parts, all changes are reverted before returning.
    pub fn add_block(&self, block: Block) -> Result<BlockAddResult, ChainStorageError> {
        let block_hash = block.hash();
        let block_height = block.header.height;
        if self.db.contains(&DbKey::BlockHash(block_hash.clone()))? {
            return Ok(BlockAddResult::BlockExists);
        }
        if !self.is_new_best_block(&block)? {
            return self.handle_possible_reorg(block);
        }
        let mut txn = DbTransaction::new();
        let (header, inputs, outputs, kernels) = block.dissolve();
        txn.insert_header(header);
        txn.spend_inputs(&inputs);
        outputs.into_iter().for_each(|utxo| txn.insert_utxo(utxo));
        kernels.into_iter().for_each(|k| txn.insert_kernel(k));
        txn.commit_block();
        self.commit(txn)?;
        self.update_metadata(block_height, block_hash)?;
        Ok(BlockAddResult::Ok)
    }

    /// Returns true if the given block -- assuming everything else is valid -- would be added to the tip of the
    /// longest chain; i.e. the following conditions are met:
    ///   * The blockchain is empty,
    ///   * or ALL of:
    ///     * the block's parent hash is the hash of the block at the current chain tip,
    ///     * the block height is one greater than the parent block
    ///     * the total accumulated work has increased
    pub fn is_new_best_block(&self, block: &Block) -> Result<bool, ChainStorageError> {
        let (height, parent_hash) = {
            let db = self.access_metadata()?;
            if db.height_of_longest_chain.is_none() {
                return Ok(true);
            }
            (
                db.height_of_longest_chain.clone().unwrap(),
                db.best_block.clone().unwrap(),
            )
        };
        let best_block = self.fetch_header(height)?;
        let result = block.header.prev_hash == parent_hash &&
            block.header.height == best_block.height + 1 &&
            block.header.total_difficulty > best_block.total_difficulty;
        Ok(result)
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
        let metadata = self.check_for_valid_height(height)?;
        let header = self.fetch_header(height)?;
        let kernel_cp = self.fetch_mmr_checkpoint(MmrTree::Kernel, height)?;
        let (kernel_hashes, _) = kernel_cp.into_parts();
        let kernels = self.fetch_kernels(kernel_hashes)?;
        let utxo_cp = self.db.fetch_mmr_checkpoint(MmrTree::Utxo, height)?;
        let (utxo_hashes, deleted_nodes) = utxo_cp.into_parts();
        let inputs = self.fetch_inputs(deleted_nodes)?;
        let (outputs, spent) = self.fetch_outputs(utxo_hashes)?;
        let block = BlockBuilder::new()
            .with_header(header)
            .add_inputs(inputs)
            .add_outputs(outputs)
            .add_kernels(kernels)
            .build();
        Ok(HistoricalBlock::new(
            block,
            metadata.height_of_longest_chain.unwrap() - height + 1,
            spent,
        ))
    }

    fn check_for_valid_height(&self, height: u64) -> Result<ChainMetadata, ChainStorageError> {
        let metadata = self.get_metadata()?;
        if metadata.height_of_longest_chain.is_none() {
            return Err(ChainStorageError::InvalidQuery(
                "Cannot retrieve block. Blockchain DB is empty".into(),
            ));
        }
        if height > metadata.height_of_longest_chain.unwrap() {
            return Err(ChainStorageError::InvalidQuery(format!(
                "Cannot get block at height {}. Chain tip is at {}",
                height,
                metadata.height_of_longest_chain.unwrap()
            )));
        }
        // We can't actually provide full block beyond the pruning horizon
        if height < metadata.horizon_block().unwrap() {
            return Err(ChainStorageError::BeyondPruningHorizon);
        }
        Ok(metadata)
    }

    fn fetch_kernels(&self, hashes: Vec<Hash>) -> Result<Vec<TransactionKernel>, ChainStorageError> {
        hashes.into_iter().map(|hash| self.fetch_kernel(hash)).collect()
    }

    fn fetch_inputs(&self, deleted_nodes: Bitmap) -> Result<Vec<TransactionInput>, ChainStorageError> {
        // The inputs must all be in the current STXO set
        let inputs: Result<Vec<TransactionInput>, ChainStorageError> = deleted_nodes
            .iter()
            .map(|pos| {
                self.db
                    .fetch_mmr_node(MmrTree::Utxo, pos)
                    .and_then(|(hash, deleted)| {
                        assert!(deleted);
                        self.fetch_stxo(hash)
                    })
                    .and_then(|stxo| Ok(TransactionInput::from(stxo)))
            })
            .collect();
        inputs
    }

    fn fetch_outputs(&self, hashes: Vec<Hash>) -> Result<(Vec<TransactionOutput>, Vec<Commitment>), ChainStorageError> {
        let mut outputs = Vec::with_capacity(hashes.len());
        let mut spent = Vec::with_capacity(hashes.len());
        for hash in hashes.into_iter() {
            // The outputs could come from either the UTXO or STXO set
            match self.fetch_utxo(hash.clone()) {
                Ok(utxo) => {
                    outputs.push(utxo);
                    continue;
                },
                Err(ChainStorageError::ValueNotFound(_)) => {}, // Check STXO set below
                Err(e) => return Err(e),                        // Something bad happened. Abort.
            }
            // Check the STXO set
            let stxo = self.fetch_stxo(hash)?;
            spent.push(stxo.commitment.clone());
            outputs.push(stxo);
        }
        Ok((outputs, spent))
    }

    fn fetch_mmr_checkpoint(&self, tree: MmrTree, height: u64) -> Result<MerkleCheckPoint, ChainStorageError> {
        let metadata = self.get_metadata()?;
        let horizon_block = metadata
            .horizon_block()
            .ok_or(ChainStorageError::InvalidQuery("Blockchain database is empty".into()))?;
        let index = height
            .checked_sub(horizon_block)
            .ok_or(ChainStorageError::BeyondPruningHorizon)? as u64;
        self.db.fetch_mmr_checkpoint(tree, index)
    }

    /// Atomically commit the provided transaction to the database backend. This function does not update the metadata.
    pub(crate) fn commit(&self, txn: DbTransaction) -> Result<(), ChainStorageError> {
        self.db.write(txn)
    }

    /// Rewind the blockchain state to the block height given.
    ///
    /// The operation will fail if
    /// * The block height is in the future
    /// * The block height is before pruning horizon
    pub fn rewind_to_height(&self, height: u64) -> Result<(), ChainStorageError> {
        self.check_for_valid_height(height)?;

        let chain_height = self
            .get_height()?
            .ok_or(ChainStorageError::InvalidQuery("Blockchain database is empty".into()))?;
        if height == chain_height {
            return Ok(()); // Rewind unnecessary, already on correct height
        }

        let steps_back = (chain_height - height) as usize;
        let mut txn = DbTransaction::new();
        for rewind_height in (height + 1)..=chain_height {
            // Reconstruct block at height and add to orphan block pool
            let orphaned_block = self.fetch_block(rewind_height)?.block().clone();
            txn.insert_orphan(orphaned_block);

            // Remove Headers
            txn.delete(DbKey::BlockHeader(rewind_height));
            // Remove block_hashes
            self.fetch_mmr_checkpoint(MmrTree::Header, rewind_height)?
                .nodes_added()
                .iter()
                .for_each(|hash_output| {
                    txn.delete(DbKey::BlockHash(hash_output.clone()));
                });
            // Remove Kernels
            self.fetch_mmr_checkpoint(MmrTree::Kernel, rewind_height)?
                .nodes_added()
                .iter()
                .for_each(|hash_output| {
                    txn.delete(DbKey::TransactionKernel(hash_output.clone()));
                });

            // Remove UTXOs and move STXOs back to UTXO set
            let (nodes_added, nodes_deleted) = self.fetch_mmr_checkpoint(MmrTree::Utxo, rewind_height)?.into_parts();
            nodes_added.iter().for_each(|hash_output| {
                txn.delete(DbKey::UnspentOutput(hash_output.clone()));
            });
            for pos in nodes_deleted.iter() {
                self.db
                    .fetch_mmr_node(MmrTree::Utxo, pos)
                    .and_then(|(stxo_hash, deleted)| {
                        assert!(deleted);
                        txn.unspend_stxo(stxo_hash);
                        Ok(())
                    })?;
            }
        }
        // Rewind MMRs
        txn.rewind_header_mmr(steps_back);
        txn.rewind_kernel_mmr(steps_back);
        txn.rewind_utxo_mmr(steps_back);
        txn.rewind_rp_mmr(steps_back);

        self.commit(txn)
    }

    /// Checks whether we should add the block as an orphan. If it is the case, the orphan block is added and the chain
    /// is reorganised if necessary
    fn handle_possible_reorg(&self, _block: Block) -> Result<BlockAddResult, ChainStorageError> {
        // TODO - check if block height > pruning horizon
        // TODO - check if proof of work is valid and above some spam minimum??
        unimplemented!()
    }

    fn handle_reorg(&mut self, _orphan_hash: Hash) -> Result<(), ChainStorageError> {
        unimplemented!()
    }
}

fn unexpected_result<T>(req: DbKey, res: DbValue) -> Result<T, ChainStorageError> {
    let msg = format!("Unexpected result for database query {}. Response: {}", req, res);
    error!(target: LOG_TARGET, "{}", msg);
    Err(ChainStorageError::UnexpectedResult(msg))
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
            metadata: self.metadata.clone(),
            db: self.db.clone(),
        }
    }
}
