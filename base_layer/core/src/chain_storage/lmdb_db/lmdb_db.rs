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
    blocks::{
        blockheader::{BlockHash, BlockHeader},
        Block,
    },
    chain_storage::{
        blockchain_database::BlockchainBackend,
        checkpoint_utils,
        db_transaction::{
            DbKey,
            DbKeyValuePair,
            DbTransaction,
            DbValue,
            MetadataKey,
            MetadataValue,
            MmrTree,
            WriteOperation,
        },
        error::ChainStorageError,
        lmdb_db::{
            lmdb::{lmdb_delete, lmdb_exists, lmdb_for_each, lmdb_get, lmdb_insert, lmdb_len, lmdb_replace},
            LMDBVec,
            LMDB_DB_BLOCK_HASHES,
            LMDB_DB_HEADERS,
            LMDB_DB_KERNELS,
            LMDB_DB_KERNEL_MMR_CP_BACKEND,
            LMDB_DB_METADATA,
            LMDB_DB_ORPHANS,
            LMDB_DB_RANGE_PROOF_MMR_CP_BACKEND,
            LMDB_DB_STXOS,
            LMDB_DB_TXOS_HASH_TO_INDEX,
            LMDB_DB_UTXOS,
            LMDB_DB_UTXO_MMR_CP_BACKEND,
        },
        memory_db::MemDbVec,
        ChainMetadata,
    },
    proof_of_work::{Difficulty, PowAlgorithm},
    transactions::{
        transaction::TransactionKernel,
        types::{HashDigest, HashOutput},
        TransactionOutput,
    },
};
use croaring::Bitmap;
use digest::Digest;
use lmdb_zero::{Database, Environment, WriteTransaction};
use log::*;
use std::{collections::VecDeque, path::Path, sync::Arc};
use tari_crypto::tari_utilities::{epoch_time::EpochTime, hash::Hashable, hex::Hex};
use tari_mmr::{
    functions::{calculate_pruned_mmr_root, prune_mutable_mmr, PrunedMutableMmr},
    ArrayLike,
    Hash as MmrHash,
    Hash,
    MerkleCheckPoint,
    MerkleProof,
    MmrCache,
    MmrCacheConfig,
    MutableMmr,
};
use tari_storage::lmdb_store::{db, LMDBBuilder, LMDBStore};

type DatabaseRef = Arc<Database<'static>>;

pub const LOG_TARGET: &str = "c::cs::lmdb_db::lmdb_db";

/// This is a lmdb-based blockchain database for persistent storage of the chain state.
pub struct LMDBDatabase<D>
where D: Digest
{
    env: Arc<Environment>,
    metadata_db: DatabaseRef,
    mem_metadata: ChainMetadata, // Memory copy of stored metadata
    headers_db: DatabaseRef,
    block_hashes_db: DatabaseRef,
    utxos_db: DatabaseRef,
    stxos_db: DatabaseRef,
    txos_hash_to_index_db: DatabaseRef,
    kernels_db: DatabaseRef,
    orphans_db: DatabaseRef,
    utxo_mmr: MmrCache<D, MemDbVec<MmrHash>, LMDBVec<MerkleCheckPoint>>,
    utxo_checkpoints: LMDBVec<MerkleCheckPoint>,
    curr_utxo_checkpoint: MerkleCheckPoint,
    kernel_mmr: MmrCache<D, MemDbVec<MmrHash>, LMDBVec<MerkleCheckPoint>>,
    kernel_checkpoints: LMDBVec<MerkleCheckPoint>,
    curr_kernel_checkpoint: MerkleCheckPoint,
    range_proof_mmr: MmrCache<D, MemDbVec<MmrHash>, LMDBVec<MerkleCheckPoint>>,
    range_proof_checkpoints: LMDBVec<MerkleCheckPoint>,
    curr_range_proof_checkpoint: MerkleCheckPoint,
    is_mem_metadata_dirty: bool,
}

impl<D> LMDBDatabase<D>
where D: Digest + Send + Sync
{
    pub fn new(store: LMDBStore, mmr_cache_config: MmrCacheConfig) -> Result<Self, ChainStorageError> {
        let utxo_checkpoints = LMDBVec::new(store.env(), get_database(&store, LMDB_DB_UTXO_MMR_CP_BACKEND)?);
        let kernel_checkpoints = LMDBVec::new(store.env(), get_database(&store, LMDB_DB_KERNEL_MMR_CP_BACKEND)?);
        let range_proof_checkpoints =
            LMDBVec::new(store.env(), get_database(&store, LMDB_DB_RANGE_PROOF_MMR_CP_BACKEND)?);
        // Restore memory metadata
        let env = store.env();
        let metadata_db = get_database(&store, LMDB_DB_METADATA)?;
        let metadata = fetch_metadata(&env, &metadata_db)?;

        Ok(Self {
            metadata_db,
            mem_metadata: metadata,
            headers_db: get_database(&store, LMDB_DB_HEADERS)?,
            block_hashes_db: get_database(&store, LMDB_DB_BLOCK_HASHES)?,
            utxos_db: get_database(&store, LMDB_DB_UTXOS)?,
            stxos_db: get_database(&store, LMDB_DB_STXOS)?,
            txos_hash_to_index_db: get_database(&store, LMDB_DB_TXOS_HASH_TO_INDEX)?,
            kernels_db: get_database(&store, LMDB_DB_KERNELS)?,
            orphans_db: get_database(&store, LMDB_DB_ORPHANS)?,
            utxo_mmr: MmrCache::new(MemDbVec::new(), utxo_checkpoints.clone(), mmr_cache_config)?,
            curr_utxo_checkpoint: {
                let acc_count = checkpoint_utils::fetch_last_mmr_node_added_count(&utxo_checkpoints)?;
                MerkleCheckPoint::new(Vec::new(), Bitmap::create(), acc_count)
            },
            utxo_checkpoints,
            kernel_mmr: MmrCache::new(MemDbVec::new(), kernel_checkpoints.clone(), mmr_cache_config)?,
            curr_kernel_checkpoint: {
                let acc_count = checkpoint_utils::fetch_last_mmr_node_added_count(&kernel_checkpoints)?;
                MerkleCheckPoint::new(Vec::new(), Bitmap::create(), acc_count)
            },
            kernel_checkpoints,
            range_proof_mmr: MmrCache::new(MemDbVec::new(), range_proof_checkpoints.clone(), mmr_cache_config)?,
            curr_range_proof_checkpoint: {
                let acc_count = checkpoint_utils::fetch_last_mmr_node_added_count(&range_proof_checkpoints)?;
                MerkleCheckPoint::new(Vec::new(), Bitmap::create(), acc_count)
            },
            range_proof_checkpoints,
            env,
            is_mem_metadata_dirty: false,
        })
    }

    // Perform the RewindMmr and CreateMmrCheckpoint operations after MMR txns and storage txns have been applied.
    fn commit_mmrs(&mut self, tx: DbTransaction) -> Result<(), ChainStorageError> {
        for op in tx.operations.into_iter() {
            match op {
                WriteOperation::RewindMmr(tree, steps_back) => match tree {
                    MmrTree::Kernel => {
                        let last_cp = checkpoint_utils::rewind_checkpoints(&mut self.kernel_checkpoints, steps_back)?;
                        self.kernel_mmr
                            .update()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
                        self.curr_kernel_checkpoint.reset_to(&last_cp);
                    },
                    MmrTree::Utxo => {
                        let last_cp = checkpoint_utils::rewind_checkpoints(&mut self.utxo_checkpoints, steps_back)?;
                        self.utxo_mmr
                            .update()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
                        self.curr_utxo_checkpoint.reset_to(&last_cp);
                    },
                    MmrTree::RangeProof => {
                        let last_cp =
                            checkpoint_utils::rewind_checkpoints(&mut self.range_proof_checkpoints, steps_back)?;
                        self.range_proof_mmr
                            .update()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
                        self.curr_range_proof_checkpoint.reset_to(&last_cp);
                    },
                },
                WriteOperation::CreateMmrCheckpoint(tree) => match tree {
                    MmrTree::Kernel => {
                        let curr_checkpoint = self.curr_kernel_checkpoint.clone();
                        self.kernel_checkpoints
                            .push(curr_checkpoint)
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
                        self.curr_kernel_checkpoint.reset();

                        self.kernel_mmr
                            .update()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
                    },
                    MmrTree::Utxo => {
                        let curr_checkpoint = self.curr_utxo_checkpoint.clone();
                        self.utxo_checkpoints
                            .push(curr_checkpoint)
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
                        self.curr_utxo_checkpoint.reset();

                        self.utxo_mmr
                            .update()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
                    },
                    MmrTree::RangeProof => {
                        let curr_checkpoint = self.curr_range_proof_checkpoint.clone();
                        self.range_proof_checkpoints
                            .push(curr_checkpoint)
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
                        self.curr_range_proof_checkpoint.reset();

                        self.range_proof_mmr
                            .update()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
                    },
                },
                WriteOperation::MergeMmrCheckpoints(tree, max_cp_count) => match tree {
                    MmrTree::Kernel => {
                        let (num_cps_merged, _) =
                            checkpoint_utils::merge_checkpoints(&mut self.kernel_checkpoints, max_cp_count)?;
                        self.kernel_mmr
                            .checkpoints_merged(num_cps_merged)
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
                        trace!(target: LOG_TARGET, "Merged {} kernel checkpoints", num_cps_merged);
                    },
                    MmrTree::Utxo => {
                        let (num_cps_merged, stxo_leaf_indices) =
                            checkpoint_utils::merge_checkpoints(&mut self.utxo_checkpoints, max_cp_count)?;
                        self.utxo_mmr
                            .checkpoints_merged(num_cps_merged)
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
                        trace!(target: LOG_TARGET, "Merged {} utxo checkpoints", num_cps_merged);
                        let num_stxo_leaf_indices = stxo_leaf_indices.len();
                        let num_stxos_discarded = self.discard_stxos(stxo_leaf_indices)?;
                        trace!(
                            target: LOG_TARGET,
                            "Discarded {} of {} STXOs",
                            num_stxo_leaf_indices,
                            num_stxos_discarded
                        );
                    },
                    MmrTree::RangeProof => {
                        let (num_cps_merged, _) =
                            checkpoint_utils::merge_checkpoints(&mut self.range_proof_checkpoints, max_cp_count)?;
                        self.range_proof_mmr
                            .checkpoints_merged(num_cps_merged)
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
                        trace!(target: LOG_TARGET, "Merged {} range proof checkpoints", num_cps_merged);
                    },
                },
                _ => { /* ignore operations which do not apply */ },
            }
        }
        Ok(())
    }

    // Reset any mmr txns that have been applied.
    fn reset_mmrs(&mut self) -> Result<(), ChainStorageError> {
        trace!(target: LOG_TARGET, "Reset mmrs called");
        self.kernel_mmr.reset()?;
        self.utxo_mmr.reset()?;
        self.range_proof_mmr.reset()?;
        Ok(())
    }

    // Perform all the storage txns and all MMR transactions excluding CreateMmrCheckpoint and RewindMmr on the
    // header_mmr, utxo_mmr, range_proof_mmr and kernel_mmr. Only when all the txns can successfully be applied is the
    // changes committed to the backend databases. CreateMmrCheckpoint and RewindMmr txns will be performed after these
    // txns have been successfully applied.
    fn apply_mmr_and_storage_txs(&mut self, tx: &DbTransaction) -> Result<(), ChainStorageError> {
        let txn = WriteTransaction::new(self.env.clone()).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        for op in tx.operations.iter() {
            match op {
                WriteOperation::Insert(insert) => self.op_insert(&txn, insert)?,
                WriteOperation::Delete(delete) => self.op_delete(&txn, delete)?,
                WriteOperation::Spend(key) => self.op_spend(&txn, key)?,
                WriteOperation::UnSpend(key) => self.op_unspend(&txn, key)?,
                _ => {},
            }
        }
        txn.commit()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;

        if self.is_mem_metadata_dirty {
            self.mem_metadata = fetch_metadata(&self.env, &self.metadata_db)?;
            self.is_mem_metadata_dirty = false;
        }
        Ok(())
    }

    fn op_insert(&mut self, txn: &WriteTransaction<'_>, kv_pair: &DbKeyValuePair) -> Result<(), ChainStorageError> {
        match kv_pair {
            DbKeyValuePair::Metadata(k, v) => {
                lmdb_replace(&txn, &self.metadata_db, &(k.clone() as u32), &v)?;
                self.is_mem_metadata_dirty = true;
            },
            DbKeyValuePair::BlockHeader(k, v) => {
                if lmdb_exists(&self.env, &self.headers_db, &k)? {
                    return Err(ChainStorageError::InvalidOperation(format!(
                        "Duplicate `BlockHeader` key `{}`",
                        k
                    )));
                }
                let hash = v.hash();
                lmdb_insert(&txn, &self.block_hashes_db, &hash, &k)?;
                lmdb_insert(&txn, &self.headers_db, &k, &v)?;
            },
            DbKeyValuePair::UnspentOutput(k, v) => {
                if lmdb_exists(&self.env, &self.utxos_db, &k)? {
                    return Err(ChainStorageError::InvalidOperation(format!(
                        "Duplicate `UnspentOutput` key `{}`",
                        k.to_hex()
                    )));
                }
                self.curr_utxo_checkpoint.push_addition(k.clone());
                self.curr_range_proof_checkpoint.push_addition(v.proof().hash());

                lmdb_insert(&txn, &self.utxos_db, &k, &v)?;
                let index = self.curr_range_proof_checkpoint.accumulated_nodes_added_count() - 1;
                lmdb_insert(&txn, &self.txos_hash_to_index_db, &k, &index)?;
            },
            DbKeyValuePair::TransactionKernel(k, v) => {
                if lmdb_exists(&self.env, &self.kernels_db, &k)? {
                    return Err(ChainStorageError::InvalidOperation(format!(
                        "Duplicate `TransactionKernel` key `{}`",
                        k.to_hex()
                    )));
                }
                self.curr_kernel_checkpoint.push_addition(k.clone());
                lmdb_insert(&txn, &self.kernels_db, &k, &v)?;
            },
            DbKeyValuePair::OrphanBlock(k, v) => {
                lmdb_replace(&txn, &self.orphans_db, &k, &v)?;
            },
        }
        Ok(())
    }

    fn op_delete(&mut self, txn: &WriteTransaction<'_>, key: &DbKey) -> Result<(), ChainStorageError> {
        match key {
            DbKey::Metadata(key) => {
                match lmdb_delete(&txn, &self.metadata_db, &(*key as u32)) {
                    Ok(_) => {},
                    // Ignore if the value didn't exist in the first place
                    Err(ChainStorageError::LmdbValueNotFound(_)) => {},
                    Err(err) => return Err(err),
                }
            },
            DbKey::BlockHeader(k) => {
                let val: Option<BlockHeader> = lmdb_get(&self.env, &self.headers_db, &k)?;
                if let Some(v) = val {
                    let hash = v.hash();
                    lmdb_delete(&txn, &self.block_hashes_db, &hash)?;
                    lmdb_delete(&txn, &self.headers_db, &k)?;
                }
            },
            DbKey::BlockHash(hash) => {
                let result: Option<u64> = lmdb_get(&self.env, &self.block_hashes_db, &hash)?;
                if let Some(k) = result {
                    lmdb_delete(&txn, &self.block_hashes_db, &hash)?;
                    lmdb_delete(&txn, &self.headers_db, &k)?;
                }
            },
            DbKey::UnspentOutput(k) => {
                lmdb_delete(&txn, &self.utxos_db, &k)?;
                lmdb_delete(&txn, &self.txos_hash_to_index_db, &k)?;
            },
            DbKey::SpentOutput(k) => {
                lmdb_delete(&txn, &self.stxos_db, &k)?;
                lmdb_delete(&txn, &self.txos_hash_to_index_db, &k)?;
            },
            DbKey::TransactionKernel(k) => {
                lmdb_delete(&txn, &self.kernels_db, &k)?;
            },
            DbKey::OrphanBlock(k) => {
                lmdb_delete(&txn, &self.orphans_db, &k)?;
            },
        }

        Ok(())
    }

    fn op_spend(&mut self, txn: &WriteTransaction<'_>, key: &DbKey) -> Result<(), ChainStorageError> {
        match key {
            DbKey::UnspentOutput(hash) => {
                let utxo: TransactionOutput = lmdb_get(&self.env, &self.utxos_db, &hash)?.ok_or_else(|| {
                    error!(
                        target: LOG_TARGET,
                        "Could spend UTXO: hash `{}` not found in UTXO db",
                        hash.to_hex()
                    );
                    ChainStorageError::UnspendableInput
                })?;

                let index = lmdb_get(&self.env, &self.txos_hash_to_index_db, &hash)?.ok_or_else(|| {
                    error!(
                        target: LOG_TARGET,
                        "** Blockchain DB out of sync! ** Hash `{}` was found in utxo_db but could not be found in \
                         txos_hash_to_index db!",
                        hash.to_hex()
                    );
                    ChainStorageError::UnspendableInput
                })?;
                self.curr_utxo_checkpoint.push_deletion(index);

                lmdb_delete(&txn, &self.utxos_db, &hash)?;
                lmdb_insert(&txn, &self.stxos_db, &hash, &utxo)?;
            },
            _ => return Err(ChainStorageError::InvalidOperation("Only UTXOs can be spent".into())),
        }
        Ok(())
    }

    fn op_unspend(&mut self, txn: &WriteTransaction<'_>, key: &DbKey) -> Result<(), ChainStorageError> {
        match key {
            DbKey::SpentOutput(hash) => {
                let stxo: TransactionOutput = lmdb_get(&self.env, &self.stxos_db, &hash)?.ok_or_else(|| {
                    error!(
                        target: LOG_TARGET,
                        "STXO could not be unspent: Hash `{}` not found in the STXO db",
                        hash.to_hex()
                    );
                    ChainStorageError::UnspendError
                })?;
                lmdb_delete(&txn, &self.stxos_db, &hash)?;
                lmdb_insert(&txn, &self.utxos_db, &hash, &stxo)?;
            },
            _ => return Err(ChainStorageError::InvalidOperation("Only STXOs can be unspent".into())),
        }
        Ok(())
    }

    // Construct a pruned mmr for the specified MMR tree based on the checkpoint state and new additions and deletions.
    fn get_pruned_mmr(&self, tree: &MmrTree) -> Result<PrunedMutableMmr<D>, ChainStorageError> {
        Ok(match tree {
            MmrTree::Utxo => {
                let mut pruned_mmr = prune_mutable_mmr(&*self.utxo_mmr)?;
                for hash in self.curr_utxo_checkpoint.nodes_added() {
                    pruned_mmr.push(&hash)?;
                }
                for index in self.curr_utxo_checkpoint.nodes_deleted().to_vec() {
                    pruned_mmr.delete_and_compress(index, false);
                }
                pruned_mmr.compress();
                pruned_mmr
            },
            MmrTree::Kernel => {
                let mut pruned_mmr = prune_mutable_mmr(&*self.kernel_mmr)?;
                for hash in self.curr_kernel_checkpoint.nodes_added() {
                    pruned_mmr.push(&hash)?;
                }
                pruned_mmr
            },
            MmrTree::RangeProof => {
                let mut pruned_mmr = prune_mutable_mmr(&*self.range_proof_mmr)?;
                for hash in self.curr_range_proof_checkpoint.nodes_added() {
                    pruned_mmr.push(&hash)?;
                }
                pruned_mmr
            },
        })
    }

    // Discard the STXOs of the checkpoints that have been merged into the horizon state and return the number of
    // removed STXOs.
    fn discard_stxos(&mut self, leaf_indices: Vec<u32>) -> Result<usize, ChainStorageError> {
        let mut num_removed = 0;
        let txn = WriteTransaction::new(self.env.clone()).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        {
            for leaf_index in leaf_indices {
                if let (Some(hash), _) = self.utxo_mmr.fetch_mmr_node(leaf_index)? {
                    if lmdb_exists(&self.env, &self.stxos_db, &hash)? {
                        lmdb_delete(&txn, &self.stxos_db, &hash)?;
                        num_removed += 1;
                    }
                }
            }
        }
        txn.commit()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        Ok(num_removed)
    }
}

pub fn create_lmdb_database<P: AsRef<Path>>(
    path: P,
    mmr_cache_config: MmrCacheConfig,
) -> Result<LMDBDatabase<HashDigest>, ChainStorageError>
{
    let flags = db::CREATE;
    let _ = std::fs::create_dir_all(&path);
    let lmdb_store = LMDBBuilder::new()
        .set_path(path)
        .set_environment_size(1_000)
        .set_max_number_of_databases(15)
        .add_database(LMDB_DB_METADATA, flags)
        .add_database(LMDB_DB_HEADERS, flags)
        .add_database(LMDB_DB_BLOCK_HASHES, flags)
        .add_database(LMDB_DB_UTXOS, flags)
        .add_database(LMDB_DB_STXOS, flags)
        .add_database(LMDB_DB_TXOS_HASH_TO_INDEX, flags)
        .add_database(LMDB_DB_KERNELS, flags)
        .add_database(LMDB_DB_ORPHANS, flags)
        .add_database(LMDB_DB_UTXO_MMR_CP_BACKEND, flags)
        .add_database(LMDB_DB_KERNEL_MMR_CP_BACKEND, flags)
        .add_database(LMDB_DB_RANGE_PROOF_MMR_CP_BACKEND, flags)
        .build()
        .map_err(|err| ChainStorageError::CriticalError(format!("Could not create LMDB store:{}", err)))?;
    LMDBDatabase::<HashDigest>::new(lmdb_store, mmr_cache_config)
}

impl<D> BlockchainBackend for LMDBDatabase<D>
where D: Digest + Send + Sync
{
    fn write(&mut self, tx: DbTransaction) -> Result<(), ChainStorageError> {
        if tx.operations.is_empty() {
            return Ok(());
        }
        match self.apply_mmr_and_storage_txs(&tx) {
            Ok(_) => self.commit_mmrs(tx),
            Err(e) => {
                self.reset_mmrs()?;
                Err(e)
            },
        }
    }

    fn fetch(&self, key: &DbKey) -> Result<Option<DbValue>, ChainStorageError> {
        Ok(match key {
            DbKey::Metadata(k) => {
                let val: Option<MetadataValue> = lmdb_get(&self.env, &self.metadata_db, &(k.clone() as u32))?;
                val.map(DbValue::Metadata)
            },
            DbKey::BlockHeader(k) => {
                let val: Option<BlockHeader> = lmdb_get(&self.env, &self.headers_db, k)?;
                val.map(|val| DbValue::BlockHeader(Box::new(val)))
            },
            DbKey::BlockHash(hash) => {
                let k: Option<u64> = lmdb_get(&self.env, &self.block_hashes_db, hash)?;
                match k {
                    Some(k) => {
                        let val: Option<BlockHeader> = lmdb_get(&self.env, &self.headers_db, &k)?;
                        val.map(|val| DbValue::BlockHash(Box::new(val)))
                    },
                    None => None,
                }
            },
            DbKey::UnspentOutput(k) => {
                let val: Option<TransactionOutput> = lmdb_get(&self.env, &self.utxos_db, k)?;
                val.map(|val| DbValue::UnspentOutput(Box::new(val)))
            },
            DbKey::SpentOutput(k) => {
                let val: Option<TransactionOutput> = lmdb_get(&self.env, &self.stxos_db, k)?;
                val.map(|val| DbValue::SpentOutput(Box::new(val)))
            },
            DbKey::TransactionKernel(k) => {
                let val: Option<TransactionKernel> = lmdb_get(&self.env, &self.kernels_db, k)?;
                val.map(|val| DbValue::TransactionKernel(Box::new(val)))
            },
            DbKey::OrphanBlock(k) => {
                let val: Option<Block> = lmdb_get(&self.env, &self.orphans_db, k)?;
                val.map(|val| DbValue::OrphanBlock(Box::new(val)))
            },
        })
    }

    fn contains(&self, key: &DbKey) -> Result<bool, ChainStorageError> {
        Ok(match key {
            DbKey::Metadata(k) => lmdb_exists(&self.env, &self.metadata_db, &(*k as u32))?,
            DbKey::BlockHeader(k) => lmdb_exists(&self.env, &self.headers_db, k)?,
            DbKey::BlockHash(h) => lmdb_exists(&self.env, &self.block_hashes_db, h)?,
            DbKey::UnspentOutput(k) => lmdb_exists(&self.env, &self.utxos_db, k)?,
            DbKey::SpentOutput(k) => lmdb_exists(&self.env, &self.stxos_db, k)?,
            DbKey::TransactionKernel(k) => lmdb_exists(&self.env, &self.kernels_db, k)?,
            DbKey::OrphanBlock(k) => lmdb_exists(&self.env, &self.orphans_db, k)?,
        })
    }

    fn fetch_mmr_root(&self, tree: MmrTree) -> Result<Vec<u8>, ChainStorageError> {
        let pruned_mmr = self.get_pruned_mmr(&tree)?;
        Ok(pruned_mmr.get_merkle_root()?)
    }

    fn fetch_mmr_only_root(&self, tree: MmrTree) -> Result<Vec<u8>, ChainStorageError> {
        let pruned_mmr = self.get_pruned_mmr(&tree)?;
        Ok(pruned_mmr.get_mmr_only_root()?)
    }

    fn calculate_mmr_root(
        &self,
        tree: MmrTree,
        additions: Vec<HashOutput>,
        deletions: Vec<HashOutput>,
    ) -> Result<Vec<u8>, ChainStorageError>
    {
        let mut pruned_mmr = self.get_pruned_mmr(&tree)?;
        for hash in additions {
            pruned_mmr.push(&hash)?;
        }
        if tree == MmrTree::Utxo {
            for hash in deletions {
                if let Some(index) = lmdb_get(&self.env, &self.txos_hash_to_index_db, &hash)? {
                    pruned_mmr.delete_and_compress(index, false);
                }
            }
            pruned_mmr.compress();
        }
        Ok(pruned_mmr.get_merkle_root()?)
    }

    /// Returns an MMR proof extracted from the full Merkle mountain range without trimming the MMR using the roaring
    /// bitmap
    fn fetch_mmr_proof(&self, tree: MmrTree, leaf_pos: usize) -> Result<MerkleProof, ChainStorageError> {
        let pruned_mmr = self.get_pruned_mmr(&tree)?;
        Ok(match tree {
            MmrTree::Utxo => MerkleProof::for_leaf_node(&pruned_mmr.mmr(), leaf_pos)?,
            MmrTree::Kernel => MerkleProof::for_leaf_node(&pruned_mmr.mmr(), leaf_pos)?,
            MmrTree::RangeProof => MerkleProof::for_leaf_node(&pruned_mmr.mmr(), leaf_pos)?,
        })
    }

    // Retrieves the checkpoint corresponding to the provided height, if the checkpoint is part of the horizon state
    // then a BeyondPruningHorizon error will be produced.
    fn fetch_checkpoint_at_height(&self, tree: MmrTree, height: u64) -> Result<MerkleCheckPoint, ChainStorageError> {
        let tip_height = lmdb_len(&self.env, &self.headers_db)? as u64;
        let pruned_mode = self.mem_metadata.is_pruned_node();
        match tree {
            MmrTree::Kernel => {
                checkpoint_utils::fetch_checkpoint(&self.kernel_checkpoints, pruned_mode, tip_height, height)
            },
            MmrTree::Utxo => {
                checkpoint_utils::fetch_checkpoint(&self.utxo_checkpoints, pruned_mode, tip_height, height)
            },
            MmrTree::RangeProof => {
                checkpoint_utils::fetch_checkpoint(&self.range_proof_checkpoints, pruned_mode, tip_height, height)
            },
        }
        .map_err(|e| ChainStorageError::AccessError(format!("Checkpoint error: {}", e.to_string())))?
        .ok_or_else(|| ChainStorageError::OutOfRange)
    }

    fn fetch_checkpoint_at_index(
        &self,
        tree: MmrTree,
        index: usize,
    ) -> Result<Option<MerkleCheckPoint>, ChainStorageError>
    {
        let checkpoints = match tree {
            MmrTree::Utxo => self.utxo_checkpoints.get(index),
            MmrTree::Kernel => self.kernel_checkpoints.get(index),
            MmrTree::RangeProof => self.range_proof_checkpoints.get(index),
        }?;
        Ok(checkpoints)
    }

    fn fetch_mmr_node_count(&self, tree: MmrTree, height: u64) -> Result<u32, ChainStorageError> {
        let tip_height = lmdb_len(&self.env, &self.headers_db)?.saturating_sub(1) as u64;
        match tree {
            MmrTree::Kernel => {
                checkpoint_utils::fetch_mmr_nodes_added_count(&self.kernel_checkpoints, tip_height, height)
            },
            MmrTree::Utxo => checkpoint_utils::fetch_mmr_nodes_added_count(&self.utxo_checkpoints, tip_height, height),
            MmrTree::RangeProof => {
                checkpoint_utils::fetch_mmr_nodes_added_count(&self.range_proof_checkpoints, tip_height, height)
            },
        }
    }

    fn fetch_mmr_node(
        &self,
        tree: MmrTree,
        pos: u32,
        hist_height: Option<u64>,
    ) -> Result<(Vec<u8>, bool), ChainStorageError>
    {
        let (hash, deleted) = match tree {
            MmrTree::Kernel => self.kernel_mmr.fetch_mmr_node(pos)?,
            MmrTree::Utxo => {
                let (hash, mut deleted) = self.utxo_mmr.fetch_mmr_node(pos)?;
                // Check if the MMR node was deleted after the historic height then its deletion status should change.
                // TODO: Find a more efficient way to query the historic deletion status of an MMR node.
                if deleted {
                    if let Some(hist_height) = hist_height {
                        let tip_height = lmdb_len(&self.env, &self.headers_db)?.saturating_sub(1) as u64;
                        for height in hist_height + 1..=tip_height {
                            let cp = self.fetch_checkpoint_at_height(MmrTree::Utxo, height)?;
                            if cp.nodes_deleted().contains(pos) {
                                deleted = false;
                            }
                        }
                    }
                }
                (hash, deleted)
            },
            MmrTree::RangeProof => self.range_proof_mmr.fetch_mmr_node(pos)?,
        };

        let hash = hash.ok_or_else(|| {
            ChainStorageError::UnexpectedResult(format!("A leaf node hash in the {} MMR tree was not found", tree))
        })?;

        Ok((hash, deleted))
    }

    fn fetch_mmr_nodes(
        &self,
        tree: MmrTree,
        pos: u32,
        count: u32,
        hist_height: Option<u64>,
    ) -> Result<Vec<(Vec<u8>, bool)>, ChainStorageError>
    {
        let mut leaf_nodes = Vec::<(Vec<u8>, bool)>::with_capacity(count as usize);
        for pos in pos..pos + count {
            leaf_nodes.push(self.fetch_mmr_node(tree, pos, hist_height)?);
        }
        Ok(leaf_nodes)
    }

    fn insert_mmr_node(&mut self, tree: MmrTree, hash: Hash, deleted: bool) -> Result<(), ChainStorageError> {
        match tree {
            MmrTree::Kernel => self.curr_kernel_checkpoint.push_addition(hash),
            MmrTree::Utxo => {
                self.curr_utxo_checkpoint.push_addition(hash);
                if deleted {
                    let leaf_index = self
                        .curr_utxo_checkpoint
                        .accumulated_nodes_added_count()
                        .saturating_sub(1);
                    self.curr_utxo_checkpoint.push_deletion(leaf_index);
                }
            },
            MmrTree::RangeProof => self.curr_range_proof_checkpoint.push_addition(hash),
        };
        Ok(())
    }

    fn delete_mmr_node(&mut self, tree: MmrTree, hash: &Hash) -> Result<(), ChainStorageError> {
        match tree {
            MmrTree::Kernel | MmrTree::RangeProof => {},
            MmrTree::Utxo => {
                if let Some(leaf_index) = self.utxo_mmr.find_leaf_index(&hash)? {
                    self.curr_utxo_checkpoint.push_deletion(leaf_index);
                }
            },
        };
        Ok(())
    }

    fn fetch_mmr_leaf_index(&self, tree: MmrTree, hash: &Hash) -> Result<Option<u32>, ChainStorageError> {
        Ok(match tree {
            MmrTree::Kernel => self.kernel_mmr.find_leaf_index(hash)?,
            MmrTree::Utxo => self.utxo_mmr.find_leaf_index(hash)?,
            MmrTree::RangeProof => self.range_proof_mmr.find_leaf_index(hash)?,
        })
    }

    /// Iterate over all the stored orphan blocks and execute the function `f` for each block.
    fn for_each_orphan<F>(&self, f: F) -> Result<(), ChainStorageError>
    where F: FnMut(Result<(HashOutput, Block), ChainStorageError>) {
        lmdb_for_each::<F, HashOutput, Block>(&self.env, &self.orphans_db, f)
    }

    /// Returns the number of blocks in the block orphan pool.
    fn get_orphan_count(&self) -> Result<usize, ChainStorageError> {
        lmdb_len(&self.env, &self.orphans_db)
    }

    /// Iterate over all the stored transaction kernels and execute the function `f` for each kernel.
    fn for_each_kernel<F>(&self, f: F) -> Result<(), ChainStorageError>
    where F: FnMut(Result<(HashOutput, TransactionKernel), ChainStorageError>) {
        lmdb_for_each::<F, HashOutput, TransactionKernel>(&self.env, &self.kernels_db, f)
    }

    /// Iterate over all the stored block headers and execute the function `f` for each header.
    fn for_each_header<F>(&self, f: F) -> Result<(), ChainStorageError>
    where F: FnMut(Result<(u64, BlockHeader), ChainStorageError>) {
        lmdb_for_each::<F, u64, BlockHeader>(&self.env, &self.headers_db, f)
    }

    /// Iterate over all the stored unspent transaction outputs and execute the function `f` for each kernel.
    fn for_each_utxo<F>(&self, f: F) -> Result<(), ChainStorageError>
    where F: FnMut(Result<(HashOutput, TransactionOutput), ChainStorageError>) {
        lmdb_for_each::<F, HashOutput, TransactionOutput>(&self.env, &self.utxos_db, f)
    }

    /// Finds and returns the last stored header.
    fn fetch_last_header(&self) -> Result<Option<BlockHeader>, ChainStorageError> {
        let header_count = lmdb_len(&self.env, &self.headers_db)?;
        if header_count >= 1 {
            let index = header_count as usize - 1;
            lmdb_get(&self.env, &self.headers_db, &index)
        } else {
            Ok(None)
        }
    }

    /// Returns the metadata of the chain.
    fn fetch_chain_metadata(&self) -> Result<ChainMetadata, ChainStorageError> {
        Ok(self.mem_metadata.clone())
    }

    /// Returns the set of target difficulties for the specified proof of work algorithm.
    fn fetch_target_difficulties(
        &self,
        pow_algo: PowAlgorithm,
        height: u64,
        block_window: usize,
    ) -> Result<Vec<(EpochTime, Difficulty)>, ChainStorageError>
    {
        let mut target_difficulties = VecDeque::<(EpochTime, Difficulty)>::with_capacity(block_window);
        let tip_height = self.mem_metadata.height_of_longest_chain.ok_or_else(|| {
            ChainStorageError::InvalidQuery("Cannot retrieve chain height. Blockchain DB is empty".into())
        })?;
        if height <= tip_height {
            for height in (0..=height).rev() {
                let header: BlockHeader = lmdb_get(&self.env, &self.headers_db, &height)?
                    .ok_or_else(|| ChainStorageError::InvalidQuery("Cannot retrieve header.".into()))?;
                if header.pow.pow_algo == pow_algo {
                    target_difficulties.push_front((header.timestamp, header.pow.target_difficulty));
                    if target_difficulties.len() >= block_window {
                        break;
                    }
                }
            }
        }
        Ok(target_difficulties
            .into_iter()
            .collect::<Vec<(EpochTime, Difficulty)>>())
    }

    fn count_utxos(&self) -> Result<usize, ChainStorageError> {
        lmdb_len(&self.env, &self.utxos_db)
    }

    fn count_kernels(&self) -> Result<usize, ChainStorageError> {
        lmdb_len(&self.env, &self.kernels_db)
    }

    fn count_checkpoints(&self, tree: MmrTree) -> Result<usize, ChainStorageError> {
        let len = match tree {
            MmrTree::Utxo => self.utxo_checkpoints.len()?,
            MmrTree::Kernel => self.kernel_checkpoints.len()?,
            MmrTree::RangeProof => self.range_proof_checkpoints.len()?,
        };

        Ok(len)
    }

    /// Validate the MMR root for the given `MmrTree` matches the header at the given height
    fn validate_merkle_root(&self, tree: MmrTree, height: u64) -> Result<bool, ChainStorageError> {
        let header: BlockHeader = lmdb_get(&self.env, &self.headers_db, &height)?.ok_or_else(|| {
            ChainStorageError::InvalidQuery(format!(
                "Header at height {} was not found when validating {} merkle root",
                height, tree
            ))
        })?;

        match tree {
            MmrTree::Utxo => validate_merkle_root(&self.utxo_mmr, &self.curr_utxo_checkpoint, &header.output_mr),
            MmrTree::Kernel => validate_merkle_root(&self.kernel_mmr, &self.curr_kernel_checkpoint, &header.kernel_mr),
            MmrTree::RangeProof => validate_merkle_root(
                &self.range_proof_mmr,
                &self.curr_range_proof_checkpoint,
                &header.range_proof_mr,
            ),
        }
    }
}

fn validate_merkle_root<D, B>(
    mmr: &MutableMmr<D, B>,
    current_cp: &MerkleCheckPoint,
    expected_mr: &BlockHash,
) -> Result<bool, ChainStorageError>
where
    D: Digest,
    B: ArrayLike<Value = Hash>,
{
    let mmr = prune_mutable_mmr(&mmr)?;
    let root = calculate_pruned_mmr_root(
        &mmr,
        current_cp.nodes_added().clone(),
        current_cp.nodes_deleted().to_vec(),
    )?;
    Ok(expected_mr == &root)
}

// Fetch the chain metadata
fn fetch_metadata(env: &Environment, db: &Database) -> Result<ChainMetadata, ChainStorageError> {
    Ok(ChainMetadata {
        height_of_longest_chain: fetch_chain_height(&env, &db)?,
        best_block: fetch_best_block(&env, &db)?,
        pruning_horizon: fetch_pruning_horizon(&env, &db)?,
        effective_pruned_height: fetch_effective_pruned_height(&env, &db)?,
        accumulated_difficulty: fetch_accumulated_work(&env, &db)?,
    })
}

// Fetches the chain height from the provided metadata db.
fn fetch_chain_height(env: &Environment, db: &Database) -> Result<Option<u64>, ChainStorageError> {
    let k = MetadataKey::ChainHeight;
    let val: Option<MetadataValue> = lmdb_get(&env, &db, &(k as u32))?;
    match val {
        Some(MetadataValue::ChainHeight(height)) => Ok(height),
        _ => Ok(None),
    }
}

// Fetches the effective pruned height from the provided metadata db.
fn fetch_effective_pruned_height(env: &Environment, db: &Database) -> Result<u64, ChainStorageError> {
    let k = MetadataKey::EffectivePrunedHeight;
    let val: Option<MetadataValue> = lmdb_get(&env, &db, &(k as u32))?;
    match val {
        Some(MetadataValue::EffectivePrunedHeight(height)) => Ok(height),
        _ => Ok(0),
    }
}

// Fetches the best block hash from the provided metadata db.
fn fetch_best_block(env: &Environment, db: &Database) -> Result<Option<BlockHash>, ChainStorageError> {
    let k = MetadataKey::BestBlock;
    let val: Option<MetadataValue> = lmdb_get(&env, &db, &(k as u32))?;
    match val {
        Some(MetadataValue::BestBlock(best_block)) => Ok(best_block),
        _ => Ok(None),
    }
}

// Fetches the accumulated work from the provided metadata db.
fn fetch_accumulated_work(env: &Environment, db: &Database) -> Result<Option<Difficulty>, ChainStorageError> {
    let k = MetadataKey::AccumulatedWork;
    let val: Option<MetadataValue> = lmdb_get(&env, &db, &(k as u32))?;
    match val {
        Some(MetadataValue::AccumulatedWork(accumulated_difficulty)) => Ok(accumulated_difficulty),
        _ => Ok(None),
    }
}

// Fetches the pruning horizon from the provided metadata db.
fn fetch_pruning_horizon(env: &Environment, db: &Database) -> Result<u64, ChainStorageError> {
    let k = MetadataKey::PruningHorizon;
    let val: Option<MetadataValue> = lmdb_get(&env, &db, &(k as u32))?;
    match val {
        Some(MetadataValue::PruningHorizon(pruning_horizon)) => Ok(pruning_horizon),
        _ => Ok(0),
    }
}

fn get_database(store: &LMDBStore, name: &str) -> Result<DatabaseRef, ChainStorageError> {
    let handle = store
        .get_handle(name)
        .ok_or_else(|| ChainStorageError::CriticalError(format!("Could not get `{}` database", name)))?;
    Ok(handle.db())
}
