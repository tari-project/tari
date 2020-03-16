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
    blocks::{blockheader::BlockHeader, Block},
    chain_storage::{
        blockchain_database::BlockchainBackend,
        db_transaction::{DbKey, DbKeyValuePair, DbTransaction, DbValue, MetadataValue, MmrTree, WriteOperation},
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
    },
    transactions::{
        transaction::{TransactionKernel, TransactionOutput},
        types::{HashDigest, HashOutput},
    },
};
use croaring::Bitmap;
use digest::Digest;
use lmdb_zero::{Database, Environment, WriteTransaction};
use log::*;
use std::{
    path::Path,
    sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard},
};
use tari_crypto::tari_utilities::{hash::Hashable, hex::Hex};
use tari_mmr::{
    functions::{prune_mutable_mmr, PrunedMutableMmr},
    ArrayLike,
    ArrayLikeExt,
    Hash as MmrHash,
    MerkleCheckPoint,
    MerkleProof,
    MmrCache,
    MmrCacheConfig,
};
use tari_storage::lmdb_store::{db, LMDBBuilder, LMDBStore};

type DatabaseRef = Arc<Database<'static>>;

pub const LOG_TARGET: &str = "c::cs::lmdb_db::lmdb_db";

/// This is a lmdb-based blockchain database for persistent storage of the chain state.
pub struct LMDBDatabase<D>
where D: Digest
{
    env: Arc<Environment>,
    // Lock used to ensure there aren't two threads altering
    // or reading data at the same time
    transaction_write_lock: RwLock<u32>,
    metadata_db: DatabaseRef,
    headers_db: DatabaseRef,
    block_hashes_db: DatabaseRef,
    utxos_db: DatabaseRef,
    stxos_db: DatabaseRef,
    txos_hash_to_index_db: DatabaseRef,
    kernels_db: DatabaseRef,
    orphans_db: DatabaseRef,
    utxo_mmr: RwLock<MmrCache<D, MemDbVec<MmrHash>, LMDBVec<MerkleCheckPoint>>>,
    utxo_checkpoints: RwLock<LMDBVec<MerkleCheckPoint>>,
    curr_utxo_checkpoint: RwLock<MerkleCheckPoint>,
    kernel_mmr: RwLock<MmrCache<D, MemDbVec<MmrHash>, LMDBVec<MerkleCheckPoint>>>,
    kernel_checkpoints: RwLock<LMDBVec<MerkleCheckPoint>>,
    curr_kernel_checkpoint: RwLock<MerkleCheckPoint>,
    range_proof_mmr: RwLock<MmrCache<D, MemDbVec<MmrHash>, LMDBVec<MerkleCheckPoint>>>,
    range_proof_checkpoints: RwLock<LMDBVec<MerkleCheckPoint>>,
    curr_range_proof_checkpoint: RwLock<MerkleCheckPoint>,
}

impl<D> LMDBDatabase<D>
where D: Digest + Send + Sync
{
    pub fn new(store: LMDBStore, mmr_cache_config: MmrCacheConfig) -> Result<Self, ChainStorageError> {
        let utxo_checkpoints = LMDBVec::new(
            store.env(),
            store
                .get_handle(LMDB_DB_UTXO_MMR_CP_BACKEND)
                .ok_or_else(|| ChainStorageError::CriticalError)?
                .db()
                .clone(),
        );
        let kernel_checkpoints = LMDBVec::new(
            store.env(),
            store
                .get_handle(LMDB_DB_KERNEL_MMR_CP_BACKEND)
                .ok_or_else(|| ChainStorageError::CriticalError)?
                .db()
                .clone(),
        );
        let range_proof_checkpoints = LMDBVec::new(
            store.env(),
            store
                .get_handle(LMDB_DB_RANGE_PROOF_MMR_CP_BACKEND)
                .ok_or_else(|| ChainStorageError::CriticalError)?
                .db()
                .clone(),
        );
        Ok(Self {
            metadata_db: store
                .get_handle(LMDB_DB_METADATA)
                .ok_or_else(|| ChainStorageError::CriticalError)?
                .db()
                .clone(),
            headers_db: store
                .get_handle(LMDB_DB_HEADERS)
                .ok_or_else(|| ChainStorageError::CriticalError)?
                .db()
                .clone(),
            block_hashes_db: store
                .get_handle(LMDB_DB_BLOCK_HASHES)
                .ok_or_else(|| ChainStorageError::CriticalError)?
                .db()
                .clone(),
            utxos_db: store
                .get_handle(LMDB_DB_UTXOS)
                .ok_or_else(|| ChainStorageError::CriticalError)?
                .db()
                .clone(),
            stxos_db: store
                .get_handle(LMDB_DB_STXOS)
                .ok_or_else(|| ChainStorageError::CriticalError)?
                .db()
                .clone(),
            txos_hash_to_index_db: store
                .get_handle(LMDB_DB_TXOS_HASH_TO_INDEX)
                .ok_or_else(|| ChainStorageError::CriticalError)?
                .db()
                .clone(),
            kernels_db: store
                .get_handle(LMDB_DB_KERNELS)
                .ok_or_else(|| ChainStorageError::CriticalError)?
                .db()
                .clone(),
            orphans_db: store
                .get_handle(LMDB_DB_ORPHANS)
                .ok_or_else(|| ChainStorageError::CriticalError)?
                .db()
                .clone(),
            utxo_mmr: RwLock::new(MmrCache::new(
                MemDbVec::new(),
                utxo_checkpoints.clone(),
                mmr_cache_config,
            )?),
            utxo_checkpoints: RwLock::new(utxo_checkpoints),
            curr_utxo_checkpoint: RwLock::new(MerkleCheckPoint::new(Vec::new(), Bitmap::create())),
            kernel_mmr: RwLock::new(MmrCache::new(
                MemDbVec::new(),
                kernel_checkpoints.clone(),
                mmr_cache_config,
            )?),
            kernel_checkpoints: RwLock::new(kernel_checkpoints),
            curr_kernel_checkpoint: RwLock::new(MerkleCheckPoint::new(Vec::new(), Bitmap::create())),
            range_proof_mmr: RwLock::new(MmrCache::new(
                MemDbVec::new(),
                range_proof_checkpoints.clone(),
                mmr_cache_config,
            )?),
            range_proof_checkpoints: RwLock::new(range_proof_checkpoints),
            curr_range_proof_checkpoint: RwLock::new(MerkleCheckPoint::new(Vec::new(), Bitmap::create())),
            env: store.env(),
            transaction_write_lock: Default::default(),
        })
    }

    // Applies all MMR transactions excluding CreateMmrCheckpoint and RewindMmr on the header_mmr, utxo_mmr,
    // range_proof_mmr and kernel_mmr. CreateMmrCheckpoint and RewindMmr txns will be performed after the the storage
    // txns have been successfully applied.
    // NOTE: Do not call this without having a lock on self.transaction_write_lock
    fn apply_mmr_txs(&self, tx: &DbTransaction) -> Result<(), ChainStorageError> {
        for op in tx.operations.iter() {
            match op {
                WriteOperation::Insert(insert) => match insert {
                    DbKeyValuePair::BlockHeader(_, _) => {},
                    DbKeyValuePair::UnspentOutput(k, v, update_mmr) => {
                        if *update_mmr {
                            self.curr_utxo_checkpoint
                                .write()
                                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                                .push_addition(k.clone());
                            let proof_hash = v.proof().hash();
                            self.curr_range_proof_checkpoint
                                .write()
                                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                                .push_addition(proof_hash.clone());
                        }
                    },
                    DbKeyValuePair::TransactionKernel(k, _, update_mmr) => {
                        if *update_mmr {
                            self.curr_kernel_checkpoint
                                .write()
                                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                                .push_addition(k.clone());
                        }
                    },
                    _ => {},
                },
                WriteOperation::Spend(key) => match key {
                    DbKey::UnspentOutput(hash) => {
                        let index_result: Option<usize> = lmdb_get(&self.env, &self.txos_hash_to_index_db, &hash)?;
                        match index_result {
                            Some(index) => {
                                self.curr_utxo_checkpoint
                                    .write()
                                    .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                                    .push_deletion(index as u32);
                            },
                            None => return Err(ChainStorageError::UnspendableInput),
                        }
                    },
                    _ => return Err(ChainStorageError::InvalidOperation("Only UTXOs can be spent".into())),
                },
                _ => {},
            }
        }
        Ok(())
    }

    // Perform the RewindMmr and CreateMmrCheckpoint operations after MMR txns and storage txns have been applied.
    // NOTE: Make sure you have a write lock on transaction_write_lock
    fn commit_mmrs(&self, tx: DbTransaction) -> Result<(), ChainStorageError> {
        for op in tx.operations.into_iter() {
            match op {
                WriteOperation::RewindMmr(tree, steps_back) => match tree {
                    MmrTree::Kernel => {
                        self.curr_kernel_checkpoint
                            .write()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                            .clear();
                        let cp_count = self
                            .kernel_checkpoints
                            .read()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                            .len()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
                        self.kernel_checkpoints
                            .write()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                            .truncate(rewind_checkpoint_index(cp_count, steps_back))
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
                        self.kernel_mmr
                            .write()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                            .update()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
                    },
                    MmrTree::Utxo => {
                        self.curr_utxo_checkpoint
                            .write()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                            .clear();
                        let cp_count = self
                            .utxo_checkpoints
                            .read()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                            .len()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
                        self.utxo_checkpoints
                            .write()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                            .truncate(rewind_checkpoint_index(cp_count, steps_back))
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
                        self.utxo_mmr
                            .write()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                            .update()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
                    },
                    MmrTree::RangeProof => {
                        self.curr_range_proof_checkpoint
                            .write()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                            .clear();
                        let cp_count = self
                            .range_proof_checkpoints
                            .read()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                            .len()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
                        self.range_proof_checkpoints
                            .write()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                            .truncate(rewind_checkpoint_index(cp_count, steps_back))
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
                        self.range_proof_mmr
                            .write()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                            .update()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
                    },
                },
                WriteOperation::CreateMmrCheckpoint(tree) => match tree {
                    MmrTree::Kernel => {
                        let curr_checkpoint = self
                            .curr_kernel_checkpoint
                            .read()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                            .clone();
                        self.kernel_checkpoints
                            .write()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                            .push(curr_checkpoint)
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;

                        self.curr_kernel_checkpoint
                            .write()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                            .clear();
                        self.kernel_mmr
                            .write()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                            .update()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
                    },
                    MmrTree::Utxo => {
                        let curr_checkpoint = self
                            .curr_utxo_checkpoint
                            .read()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                            .clone();
                        self.utxo_checkpoints
                            .write()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                            .push(curr_checkpoint)
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;

                        self.curr_utxo_checkpoint
                            .write()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                            .clear();
                        self.utxo_mmr
                            .write()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                            .update()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
                    },
                    MmrTree::RangeProof => {
                        let curr_checkpoint = self
                            .curr_range_proof_checkpoint
                            .read()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                            .clone();
                        self.range_proof_checkpoints
                            .write()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                            .push(curr_checkpoint)
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;

                        self.curr_range_proof_checkpoint
                            .write()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                            .clear();
                        self.range_proof_mmr
                            .write()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                            .update()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
                    },
                },
                _ => {},
            }
        }
        Ok(())
    }

    // Reset any mmr txns that have been applied.
    // Note: Make sure you have a write lock on transaction_write_lock
    fn reset_mmrs(&self) -> Result<(), ChainStorageError> {
        debug!(target: LOG_TARGET, "Reset mmrs called");
        self.kernel_mmr
            .write()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
            .reset()?;
        self.utxo_mmr
            .write()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
            .reset()?;
        self.range_proof_mmr
            .write()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
            .reset()?;
        Ok(())
    }

    // Perform all the storage txns, excluding any MMR operations. Only when all the txns can successfully be applied is
    // the changes committed to the backend databases.
    fn apply_storage_txs(&self, tx: &DbTransaction) -> Result<(), ChainStorageError> {
        let txn = WriteTransaction::new(self.env.clone()).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        {
            for op in tx.operations.iter() {
                match op {
                    WriteOperation::Insert(insert) => match insert {
                        DbKeyValuePair::Metadata(k, v) => {
                            lmdb_replace(&txn, &self.metadata_db, &(k.clone() as u32), &v)?;
                        },
                        DbKeyValuePair::BlockHeader(k, v) => {
                            let hash = v.hash();
                            lmdb_insert(&txn, &self.block_hashes_db, &hash, &k)?;
                            lmdb_insert(&txn, &self.headers_db, &k, &v)?;
                        },
                        DbKeyValuePair::UnspentOutput(k, v, _) => {
                            let proof_hash = v.proof().hash();
                            if let Some(index) = self.find_range_proof_leaf_index(&proof_hash)? {
                                lmdb_insert(&txn, &self.utxos_db, &k, &v)?;
                                lmdb_insert(&txn, &self.txos_hash_to_index_db, &k, &index)?;
                            } else {
                                warn!(
                                    target: LOG_TARGET,
                                    "Could not find range proof leaf index:{}",
                                    proof_hash.to_hex()
                                );
                            }
                        },
                        DbKeyValuePair::TransactionKernel(k, v, _) => {
                            lmdb_insert(&txn, &self.kernels_db, &k, &v)?;
                        },
                        DbKeyValuePair::OrphanBlock(k, v) => {
                            lmdb_insert(&txn, &self.orphans_db, &k, &v)?;
                        },
                    },
                    WriteOperation::Delete(delete) => match delete {
                        DbKey::Metadata(_) => {}, // no-op
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
                    },
                    WriteOperation::Spend(key) => match key {
                        DbKey::UnspentOutput(hash) => {
                            let utxo_result: Option<TransactionOutput> = lmdb_get(&self.env, &self.utxos_db, &hash)?;
                            match utxo_result {
                                Some(utxo) => {
                                    lmdb_delete(&txn, &self.utxos_db, &hash)?;
                                    lmdb_insert(&txn, &self.stxos_db, &hash, &utxo)?;
                                },
                                None => return Err(ChainStorageError::UnspendableInput),
                            }
                        },
                        _ => return Err(ChainStorageError::InvalidOperation("Only UTXOs can be spent".into())),
                    },
                    WriteOperation::UnSpend(key) => match key {
                        DbKey::SpentOutput(hash) => {
                            let stxo_result: Option<TransactionOutput> = lmdb_get(&self.env, &self.stxos_db, &hash)?;
                            match stxo_result {
                                Some(stxo) => {
                                    lmdb_delete(&txn, &self.stxos_db, &hash)?;
                                    lmdb_insert(&txn, &self.utxos_db, &hash, &stxo)?;
                                },
                                None => return Err(ChainStorageError::UnspendError),
                            }
                        },
                        _ => return Err(ChainStorageError::InvalidOperation("Only STXOs can be unspent".into())),
                    },
                    _ => {},
                }
            }
        }
        txn.commit().map_err(|e| ChainStorageError::AccessError(e.to_string()))
    }

    // Construct a pruned mmr for the specified MMR tree based on the checkpoint state and new additions and deletions.
    fn get_pruned_mmr(&self, tree: &MmrTree) -> Result<PrunedMutableMmr<D>, ChainStorageError> {
        let _lock = self.lock_for_read()?;
        Ok(match tree {
            MmrTree::Utxo => {
                let mut pruned_mmr = prune_mutable_mmr(
                    &*self
                        .utxo_mmr
                        .read()
                        .map_err(|e| ChainStorageError::AccessError(e.to_string()))?,
                )?;
                for hash in self
                    .curr_utxo_checkpoint
                    .read()
                    .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                    .nodes_added()
                {
                    pruned_mmr.push(&hash)?;
                }
                for index in self
                    .curr_utxo_checkpoint
                    .read()
                    .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                    .nodes_deleted()
                    .to_vec()
                {
                    pruned_mmr.delete_and_compress(index, false);
                }
                pruned_mmr.compress();
                pruned_mmr
            },
            MmrTree::Kernel => {
                let mut pruned_mmr = prune_mutable_mmr(
                    &*self
                        .kernel_mmr
                        .read()
                        .map_err(|e| ChainStorageError::AccessError(e.to_string()))?,
                )?;
                for hash in self
                    .curr_kernel_checkpoint
                    .read()
                    .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                    .nodes_added()
                {
                    pruned_mmr.push(&hash)?;
                }
                pruned_mmr
            },
            MmrTree::RangeProof => {
                let mut pruned_mmr = prune_mutable_mmr(
                    &*self
                        .range_proof_mmr
                        .read()
                        .map_err(|e| ChainStorageError::AccessError(e.to_string()))?,
                )?;
                for hash in self
                    .curr_range_proof_checkpoint
                    .read()
                    .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                    .nodes_added()
                {
                    pruned_mmr.push(&hash)?;
                }
                pruned_mmr
            },
        })
    }

    fn lock_for_write(&self) -> Result<RwLockWriteGuard<u32>, ChainStorageError> {
        Ok(self.transaction_write_lock.write().map_err(|e| {
            ChainStorageError::AccessError(format!(
                "Could not exclusively gain write access to DB: {}",
                e.to_string()
            ))
        })?)
    }

    fn lock_for_read(&self) -> Result<RwLockReadGuard<u32>, ChainStorageError> {
        Ok(self.transaction_write_lock.read().map_err(|e| {
            ChainStorageError::AccessError(format!(
                "Could not exclusively gain write access to DB: {}",
                e.to_string()
            ))
        })?)
    }
}

pub fn create_lmdb_database(
    path: &Path,
    mmr_cache_config: MmrCacheConfig,
) -> Result<LMDBDatabase<HashDigest>, ChainStorageError>
{
    let flags = db::CREATE;
    std::fs::create_dir_all(&path).unwrap_or_default();
    let lmdb_store = LMDBBuilder::new()
        .set_path(path.to_str().unwrap())
        .set_environment_size(15)
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
        .map_err(|_| ChainStorageError::CriticalError)?;
    LMDBDatabase::<HashDigest>::new(lmdb_store, mmr_cache_config)
}

impl<D> BlockchainBackend for LMDBDatabase<D>
where D: Digest + Send + Sync
{
    fn write(&self, tx: DbTransaction) -> Result<(), ChainStorageError> {
        // Prevent other threads from running a transaction at the same time
        let _lock = self.lock_for_write()?;
        match self.apply_mmr_txs(&tx) {
            Ok(_) => match self.apply_storage_txs(&tx) {
                Ok(_) => self.commit_mmrs(tx),
                Err(e) => {
                    self.reset_mmrs()?;
                    Err(e)
                },
            },
            Err(e) => {
                self.reset_mmrs()?;
                Err(e)
            },
        }
    }

    fn fetch(&self, key: &DbKey) -> Result<Option<DbValue>, ChainStorageError> {
        let _lock = self.lock_for_read()?;
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
        let _lock = self.lock_for_read()?;
        Ok(match key {
            DbKey::Metadata(k) => lmdb_exists(&self.env, &self.metadata_db, &(k.clone() as u32))?,
            DbKey::BlockHeader(k) => lmdb_exists(&self.env, &self.headers_db, k)?,
            DbKey::BlockHash(h) => lmdb_exists(&self.env, &self.block_hashes_db, h)?,
            DbKey::UnspentOutput(k) => lmdb_exists(&self.env, &self.utxos_db, k)?,
            DbKey::SpentOutput(k) => lmdb_exists(&self.env, &self.stxos_db, k)?,
            DbKey::TransactionKernel(k) => lmdb_exists(&self.env, &self.kernels_db, k)?,
            DbKey::OrphanBlock(k) => lmdb_exists(&self.env, &self.orphans_db, k)?,
        })
    }

    fn fetch_mmr_root(&self, tree: MmrTree) -> Result<Vec<u8>, ChainStorageError> {
        let _lock = self.lock_for_read()?;
        let pruned_mmr = self.get_pruned_mmr(&tree)?;
        Ok(pruned_mmr.get_merkle_root()?)
    }

    fn fetch_mmr_only_root(&self, tree: MmrTree) -> Result<Vec<u8>, ChainStorageError> {
        let _lock = self.lock_for_read()?;
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
        let _lock = self.lock_for_read()?;
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
        let _lock = self.lock_for_read()?;
        let pruned_mmr = self.get_pruned_mmr(&tree)?;
        Ok(match tree {
            MmrTree::Utxo => MerkleProof::for_leaf_node(&pruned_mmr.mmr(), leaf_pos)?,
            MmrTree::Kernel => MerkleProof::for_leaf_node(&pruned_mmr.mmr(), leaf_pos)?,
            MmrTree::RangeProof => MerkleProof::for_leaf_node(&pruned_mmr.mmr(), leaf_pos)?,
        })
    }

    fn fetch_checkpoint(&self, tree: MmrTree, height: u64) -> Result<MerkleCheckPoint, ChainStorageError> {
        let _lock = self.lock_for_read()?;
        match tree {
            MmrTree::Kernel => self
                .kernel_checkpoints
                .read()
                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                .get(height as usize),
            MmrTree::Utxo => self
                .utxo_checkpoints
                .read()
                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                .get(height as usize),
            MmrTree::RangeProof => self
                .range_proof_checkpoints
                .read()
                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                .get(height as usize),
        }
        .map_err(|e| ChainStorageError::AccessError(format!("Checkpoint error: {}", e.to_string())))?
        .ok_or_else(|| ChainStorageError::OutOfRange)
    }

    fn fetch_mmr_node(&self, tree: MmrTree, pos: u32) -> Result<(Vec<u8>, bool), ChainStorageError> {
        let _lock = self.lock_for_read();
        let (hash, deleted) = match tree {
            MmrTree::Kernel => self
                .kernel_mmr
                .read()
                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                .fetch_mmr_node(pos)?,
            MmrTree::Utxo => self
                .utxo_mmr
                .read()
                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                .fetch_mmr_node(pos)?,
            MmrTree::RangeProof => self
                .range_proof_mmr
                .read()
                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                .fetch_mmr_node(pos)?,
        };
        let hash = hash.ok_or_else(|| {
            ChainStorageError::UnexpectedResult(format!("A leaf node hash in the {} MMR tree was not found", tree))
        })?;
        Ok((hash, deleted))
    }

    /// Iterate over all the stored orphan blocks and execute the function `f` for each block.
    fn for_each_orphan<F>(&self, f: F) -> Result<(), ChainStorageError>
    where F: FnMut(Result<(HashOutput, Block), ChainStorageError>) {
        let _lock = self.lock_for_read()?;
        lmdb_for_each::<_, HashOutput, Block>(&self.env, &self.orphans_db, f)
    }

    /// Iterate over all the stored transaction kernels and execute the function `f` for each kernel.
    fn for_each_kernel<F>(&self, f: F) -> Result<(), ChainStorageError>
    where F: FnMut(Result<(HashOutput, TransactionKernel), ChainStorageError>) {
        let _lock = self.lock_for_read()?;
        lmdb_for_each::<F, HashOutput, TransactionKernel>(&self.env, &self.kernels_db, f)
    }

    /// Iterate over all the stored block headers and execute the function `f` for each header.
    fn for_each_header<F>(&self, f: F) -> Result<(), ChainStorageError>
    where F: FnMut(Result<(u64, BlockHeader), ChainStorageError>) {
        let _lock = self.lock_for_read()?;
        lmdb_for_each::<F, u64, BlockHeader>(&self.env, &self.headers_db, f)
    }

    /// Iterate over all the stored unspent transaction outputs and execute the function `f` for each kernel.
    fn for_each_utxo<F>(&self, f: F) -> Result<(), ChainStorageError>
    where F: FnMut(Result<(HashOutput, TransactionOutput), ChainStorageError>) {
        let _lock = self.lock_for_read()?;
        lmdb_for_each::<F, HashOutput, TransactionOutput>(&self.env, &self.utxos_db, f)
    }

    /// Finds and returns the last stored header.
    fn fetch_last_header(&self) -> Result<Option<BlockHeader>, ChainStorageError> {
        let _lock = self.lock_for_read()?;
        let header_count = lmdb_len(&self.env, &self.headers_db)?;
        if header_count >= 1 {
            let k = header_count - 1;
            lmdb_get(&self.env, &self.headers_db, &k)
        } else {
            Ok(None)
        }
    }

    fn range_proof_checkpoints_len(&self) -> Result<usize, ChainStorageError> {
        self.range_proof_checkpoints
            .read()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
            .len()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))
    }

    fn get_range_proof_checkpoints(&self, cp_index: usize) -> Result<Option<MerkleCheckPoint>, ChainStorageError> {
        self.range_proof_checkpoints
            .read()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
            .get(cp_index)
            .map_err(|e| ChainStorageError::AccessError(format!("Checkpoint error: {}", e.to_string())))
    }

    fn curr_range_proof_checkpoint_get_added_position(
        &self,
        hash: &HashOutput,
    ) -> Result<Option<usize>, ChainStorageError>
    {
        Ok(self
            .curr_range_proof_checkpoint
            .read()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
            .nodes_added()
            .iter()
            .position(|h| h == hash))
    }
}

// Calculated the new checkpoint count after rewinding a set number of steps back.
fn rewind_checkpoint_index(cp_count: usize, steps_back: usize) -> usize {
    if cp_count > steps_back {
        cp_count - steps_back
    } else {
        1
    }
}
