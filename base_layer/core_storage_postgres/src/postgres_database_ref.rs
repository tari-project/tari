use std::sync::{Arc, RwLock};
use crate::postgres_database::PostgresDatabase;
use tari_core::chain_storage::{BlockchainBackend, DbTransaction, ChainStorageError, DbKey, DbValue, MmrTree};
use digest::Digest;
use tari_core::transactions::types::HashOutput;
use tari_mmr::{MerkleProof, MerkleCheckPoint, Hash};
use tari_core::blocks::{BlockHeader, Block};
use tari_core::transactions::transaction::{TransactionOutput, TransactionKernel};
use crate::error::PostgresChainStorageError::PoisonedLockError;
use snafu::ResultExt;

pub struct PostgresDatabaseRef<D>
    where D: Digest{
    db: Arc<RwLock<PostgresDatabase<D>>>
}

impl<D> PostgresDatabaseRef<D>
where D: Digest
{
    pub fn new(db: PostgresDatabase<D>) -> Self{
        Self{
            db: Arc::new(RwLock::new(db))
        }
    }
}

impl<D> BlockchainBackend for PostgresDatabaseRef<D>
    where D: Digest + Send + Sync {
    fn write(&self, tx: DbTransaction) -> Result<(), ChainStorageError> {
        self.db.write().map_err(|_| PoisonedLockError)?.write(tx)
    }

    fn fetch(&self, key: &DbKey) -> Result<Option<DbValue>, ChainStorageError> {
        self.db.read().map_err(|_| PoisonedLockError)?.fetch(key)
    }

    fn contains(&self, key: &DbKey) -> Result<bool, ChainStorageError> {
        self.db.read().map_err(|_| PoisonedLockError)?.contains(key)
    }

    fn fetch_mmr_root(&self, tree: MmrTree) -> Result<HashOutput, ChainStorageError> {
        self.db.read().map_err(|_| PoisonedLockError)?.fetch_mmr_root(tree)
    }

    fn fetch_mmr_only_root(&self, tree: MmrTree) -> Result<HashOutput, ChainStorageError> {
        self.db.read().map_err(|_| PoisonedLockError)?.fetch_mmr_only_root(tree)
    }

    fn calculate_mmr_root(&self, tree: MmrTree, additions: Vec<HashOutput>, deletions: Vec<HashOutput>) -> Result<HashOutput, ChainStorageError> {
        self.db.read().map_err(|_| PoisonedLockError)?.calculate_mmr_root(tree, additions, deletions)
    }

    fn fetch_mmr_proof(&self, tree: MmrTree, pos: usize) -> Result<MerkleProof, ChainStorageError> {
        self.db.read().map_err(|_| PoisonedLockError)?.fetch_mmr_proof(tree, pos)
    }

    fn fetch_checkpoint(&self, tree: MmrTree, height: u64) -> Result<MerkleCheckPoint, ChainStorageError> {
        self.db.read().map_err(|_| PoisonedLockError)?.fetch_checkpoint(tree, height)
    }

    fn fetch_mmr_node(&self, tree: MmrTree, pos: u32) -> Result<(Hash, bool), ChainStorageError> {
        self.db.read().map_err(|_| PoisonedLockError)?.fetch_mmr_node(tree, pos)
    }

    fn for_each_orphan<F>(&self, f: F) -> Result<(), ChainStorageError> where
        Self: Sized,
        F: FnMut(Result<(HashOutput, Block), ChainStorageError>) {
        self.db.read().map_err(|_| PoisonedLockError)?.for_each_orphan(f)
    }

    fn for_each_kernel<F>(&self, f: F) -> Result<(), ChainStorageError> where
        Self: Sized,
        F: FnMut(Result<(HashOutput, TransactionKernel), ChainStorageError>) {
        self.db.read().map_err(|_| PoisonedLockError)?.for_each_kernel(f)
    }

    fn for_each_header<F>(&self, f: F) -> Result<(), ChainStorageError> where
        Self: Sized,
        F: FnMut(Result<(u64, BlockHeader), ChainStorageError>) {
        self.db.read().map_err(|_| PoisonedLockError)?.for_each_header(f)
    }

    fn for_each_utxo<F>(&self, f: F) -> Result<(), ChainStorageError> where
        Self: Sized,
        F: FnMut(Result<(HashOutput, TransactionOutput), ChainStorageError>) {
        self.db.read().map_err(|_| PoisonedLockError)?.for_each_utxo(f)
    }

    fn fetch_last_header(&self) -> Result<Option<BlockHeader>, ChainStorageError> {
        self.db.read().map_err(|_| PoisonedLockError)?.fetch_last_header()
    }

    fn range_proof_checkpoints_len(&self) -> Result<usize, ChainStorageError> {
        self.db.read().map_err(|_| PoisonedLockError)?.range_proof_checkpoints_len()
    }

    fn get_range_proof_checkpoints(&self, cp_index: usize) -> Result<Option<MerkleCheckPoint>, ChainStorageError> {
        self.db.read().map_err(|_| PoisonedLockError)?.get_range_proof_checkpoints(cp_index)
    }

    fn curr_range_proof_checkpoint_get_added_position(&self, hash: &HashOutput) -> Result<Option<usize>, ChainStorageError> {
        self.db.read().map_err(|_| PoisonedLockError)?.curr_range_proof_checkpoint_get_added_position(hash)
    }
}
