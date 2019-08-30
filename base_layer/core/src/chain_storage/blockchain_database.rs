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
    blocks::{block::Block, blockheader::BlockHeader},
    chain_storage::{
        error::ChainStorageError,
        transaction::{DbKey, DbTransaction, DbValue, MetadataKey, MetadataValue, MmrTree},
        ChainMetadata,
        HistoricalBlock,
    },
    proof_of_work::Difficulty,
    transaction::{TransactionKernel, TransactionOutput},
    types::HashOutput,
};
use log::*;
use std::sync::{Arc, RwLock, RwLockReadGuard};
use tari_mmr::{MerkleCheckPoint, MerkleProof};

const LOG_TARGET: &str = "core::chain_storage::database";

/// Identify behaviour for Blockchain database backends. Implementations must support `Send` and `Sync` so that
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
    fn fetch_mmr_proof(&self, tree: MmrTree, pos: u64) -> Result<MerkleProof, ChainStorageError>;
    fn fetch_mmr_checkpoint(&self, tree: MmrTree, height: u64) -> Result<MerkleCheckPoint, ChainStorageError>;
}

// Private macro that pulls out all the boiler plate of extracting a DB query result from its variants
macro_rules! fetch {
    ($self:ident, $key_val:expr, $key_var:ident) => {{
        let key = DbKey::$key_var($key_val);
        match $self.db.fetch(&key) {
            Ok(None) => Ok(None),
            Ok(Some(DbValue::$key_var(k))) => Ok(Some(*k)),
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
    fn read_metadata(db: &T) -> Result<ChainMetadata, ChainStorageError> {
        let height = fetch!(meta db, ChainHeight, 0);
        let work = fetch!(meta db, AccumulatedWork, 0);
        let horizon = fetch!(meta db, PruningHorizon, 0);
        Ok(ChainMetadata {
            height_of_longest_chain: height,
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

    fn get_metadata(&self) -> Result<RwLockReadGuard<ChainMetadata>, ChainStorageError> {
        self.metadata.read().map_err(|e| {
            error!(
                target: LOG_TARGET,
                "An attempt to get sa read lock on the blockchain metadata failed. {}",
                e.to_string()
            );
            ChainStorageError::AccessError("Read lock on blockchain metadata failed".into())
        })
    }

    /// Returns the height of the current longest chain. This method will only fail if there's a fairly serious
    /// synchronisation problem on the database. You can try calling [BlockchainDatabase::try_recover_metadata] in
    /// that case to re-sync the metadata; or else just exit the program.
    pub fn get_height(&self) -> Result<u64, ChainStorageError> {
        let metadata = self.get_metadata()?;
        Ok(metadata.height_of_longest_chain)
    }

    /// Returns the total accumulated work/difficulty of the longest chain.
    ///
    /// This method will only fail if there's a fairly serious synchronisation problem on the database. You can try
    /// calling [BlockchainDatabase::try_recover_metadata] in that case to re-sync the metadata; or else
    /// just exit the program.
    pub fn get_total_work(&self) -> Result<Difficulty, ChainStorageError> {
        let metadata = self.get_metadata()?;
        Ok(metadata.total_accumulated_difficulty.into())
    }

    /// Returns the transaction kernel with the given hash.
    pub fn fetch_kernel(&self, hash: HashOutput) -> Result<Option<TransactionKernel>, ChainStorageError> {
        fetch!(self, hash, TransactionKernel)
    }

    /// Returns the block header at the given block height.
    pub fn fetch_header(&self, block_num: u64) -> Result<Option<BlockHeader>, ChainStorageError> {
        fetch!(self, block_num, BlockHeader)
    }

    /// Returns the UTXO with the given hash.
    pub fn fetch_utxo(&self, hash: HashOutput) -> Result<Option<TransactionOutput>, ChainStorageError> {
        fetch!(self, hash, UnspentOutput)
    }

    /// Returns the STXO with the given hash.
    pub fn fetch_stxo(&self, hash: HashOutput) -> Result<Option<TransactionOutput>, ChainStorageError> {
        fetch!(self, hash, SpentOutput)
    }

    /// Returns the orphan block with the given hash.
    pub fn fetch_orphan(&self, hash: HashOutput) -> Result<Option<Block>, ChainStorageError> {
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

    /// Fetch a Merklish proof for the given hash, tree and position in the MMR
    pub fn fetch_mmr_proof(&self, tree: MmrTree, pos: u64) -> Result<MerkleProof, ChainStorageError> {
        self.db.fetch_mmr_proof(tree, pos)
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
        unimplemented!()
    }

    /// Atomically commit the provided transaction to the database backend. This function does not update the metadata.
    pub(crate) fn commit(&mut self, txn: DbTransaction) -> Result<(), ChainStorageError> {
        self.db.write(txn)
    }
}

fn unexpected_result<T>(req: DbKey, res: DbValue) -> Result<T, ChainStorageError> {
    error!(
        target: LOG_TARGET,
        "Unexpected result for database query {}. Response: {}", req, res
    );
    Err(ChainStorageError::UnexpectedResult)
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
