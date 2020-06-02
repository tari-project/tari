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
        transaction::{TransactionKernel, TransactionOutput},
        types::{HashDigest, HashOutput},
    },
};
use croaring::Bitmap;
use digest::Digest;
use lmdb_zero::{Database, Environment, WriteTransaction};
use log::*;
use std::{cmp, collections::VecDeque, fmt::Display, path::Path, sync::Arc};
use tari_crypto::tari_utilities::{epoch_time::EpochTime, hash::Hashable};
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
}

impl<D> LMDBDatabase<D>
where D: Digest + Send + Sync
{
    pub fn new(store: LMDBStore, mmr_cache_config: MmrCacheConfig) -> Result<Self, ChainStorageError> {
        let utxo_checkpoints = LMDBVec::new(
            store.env(),
            store
                .get_handle(LMDB_DB_UTXO_MMR_CP_BACKEND)
                .ok_or_else(|| ChainStorageError::CriticalError("Could not create UTXO MMR backend".to_string()))?
                .db()
                .clone(),
        );
        let kernel_checkpoints = LMDBVec::new(
            store.env(),
            store
                .get_handle(LMDB_DB_KERNEL_MMR_CP_BACKEND)
                .ok_or_else(|| ChainStorageError::CriticalError("Could not create kernel MMR backend".to_string()))?
                .db()
                .clone(),
        );
        let range_proof_checkpoints = LMDBVec::new(
            store.env(),
            store
                .get_handle(LMDB_DB_RANGE_PROOF_MMR_CP_BACKEND)
                .ok_or_else(|| {
                    ChainStorageError::CriticalError("Could not create range proof MMR backend".to_string())
                })?
                .db()
                .clone(),
        );
        // Restore memory metadata
        let env = store.env();
        let metadata_db = store
            .get_handle(LMDB_DB_METADATA)
            .ok_or_else(|| ChainStorageError::CriticalError("Could not create metadata backend".to_string()))?
            .db()
            .clone();
        let metadata = ChainMetadata {
            height_of_longest_chain: fetch_chain_height(&env, &metadata_db)?,
            best_block: fetch_best_block(&env, &metadata_db)?,
            pruning_horizon: fetch_pruning_horizon(&env, &metadata_db)?,
            accumulated_difficulty: fetch_accumulated_work(&env, &metadata_db)?,
        };

        Ok(Self {
            metadata_db,
            mem_metadata: metadata,
            headers_db: store
                .get_handle(LMDB_DB_HEADERS)
                .ok_or_else(|| ChainStorageError::CriticalError("Could not get handle to headers DB".to_string()))?
                .db()
                .clone(),
            block_hashes_db: store
                .get_handle(LMDB_DB_BLOCK_HASHES)
                .ok_or_else(|| {
                    ChainStorageError::CriticalError("Could not create handle to block hashes DB".to_string())
                })?
                .db()
                .clone(),
            utxos_db: store
                .get_handle(LMDB_DB_UTXOS)
                .ok_or_else(|| ChainStorageError::CriticalError("Could not create handle to UTXOs DB".to_string()))?
                .db()
                .clone(),
            stxos_db: store
                .get_handle(LMDB_DB_STXOS)
                .ok_or_else(|| ChainStorageError::CriticalError("Could not create handle to STXOs DB".to_string()))?
                .db()
                .clone(),
            txos_hash_to_index_db: store
                .get_handle(LMDB_DB_TXOS_HASH_TO_INDEX)
                .ok_or_else(|| ChainStorageError::CriticalError("Could not create handle to TXOs DB".to_string()))?
                .db()
                .clone(),
            kernels_db: store
                .get_handle(LMDB_DB_KERNELS)
                .ok_or_else(|| ChainStorageError::CriticalError("Could not create handle to kernels DB".to_string()))?
                .db()
                .clone(),
            orphans_db: store
                .get_handle(LMDB_DB_ORPHANS)
                .ok_or_else(|| ChainStorageError::CriticalError("Could not create handle to orphans DB".to_string()))?
                .db()
                .clone(),
            utxo_mmr: MmrCache::new(MemDbVec::new(), utxo_checkpoints.clone(), mmr_cache_config)?,
            curr_utxo_checkpoint: {
                let acc_count = fetch_last_mmr_node_added_count(&utxo_checkpoints)?;
                MerkleCheckPoint::new(Vec::new(), Bitmap::create(), acc_count)
            },
            utxo_checkpoints,
            kernel_mmr: MmrCache::new(MemDbVec::new(), kernel_checkpoints.clone(), mmr_cache_config)?,
            curr_kernel_checkpoint: {
                let acc_count = fetch_last_mmr_node_added_count(&kernel_checkpoints)?;
                MerkleCheckPoint::new(Vec::new(), Bitmap::create(), acc_count)
            },
            kernel_checkpoints,
            range_proof_mmr: MmrCache::new(MemDbVec::new(), range_proof_checkpoints.clone(), mmr_cache_config)?,
            curr_range_proof_checkpoint: {
                let acc_count = fetch_last_mmr_node_added_count(&range_proof_checkpoints)?;
                MerkleCheckPoint::new(Vec::new(), Bitmap::create(), acc_count)
            },
            range_proof_checkpoints,
            env,
        })
    }

    // Perform the RewindMmr and CreateMmrCheckpoint operations after MMR txns and storage txns have been applied.
    fn commit_mmrs(&mut self, tx: DbTransaction) -> Result<(), ChainStorageError> {
        for op in tx.operations.into_iter() {
            match op {
                WriteOperation::RewindMmr(tree, steps_back) => match tree {
                    MmrTree::Kernel => {
                        let last_cp = rewind_checkpoints(&mut self.kernel_checkpoints, steps_back)?;
                        self.kernel_mmr
                            .update()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
                        self.curr_kernel_checkpoint.reset_to(&last_cp);
                    },
                    MmrTree::Utxo => {
                        let last_cp = rewind_checkpoints(&mut self.utxo_checkpoints, steps_back)?;
                        self.utxo_mmr
                            .update()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
                        self.curr_utxo_checkpoint.reset_to(&last_cp);
                    },
                    MmrTree::RangeProof => {
                        let last_cp = rewind_checkpoints(&mut self.range_proof_checkpoints, steps_back)?;
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
                _ => {},
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
        let mut update_mem_metadata = false;
        let txn = WriteTransaction::new(self.env.clone()).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        {
            for op in tx.operations.iter() {
                match op {
                    WriteOperation::Insert(insert) => match insert {
                        DbKeyValuePair::Metadata(k, v) => {
                            lmdb_replace(&txn, &self.metadata_db, &(k.clone() as u32), &v)?;
                            update_mem_metadata = true;
                        },
                        DbKeyValuePair::BlockHeader(k, v) => {
                            if lmdb_exists(&self.env, &self.headers_db, &k)? {
                                return Err(ChainStorageError::InvalidOperation("Duplicate key".to_string()));
                            }
                            let hash = v.hash();
                            lmdb_insert(&txn, &self.block_hashes_db, &hash, &k)?;
                            lmdb_insert(&txn, &self.headers_db, &k, &v)?;
                        },
                        DbKeyValuePair::UnspentOutput(k, v, update_mmr) => {
                            if lmdb_exists(&self.env, &self.utxos_db, &k)? {
                                return Err(ChainStorageError::InvalidOperation("Duplicate key".to_string()));
                            }
                            let proof_hash = v.proof().hash();
                            if *update_mmr {
                                self.curr_utxo_checkpoint.push_addition(k.clone());
                                self.curr_range_proof_checkpoint.push_addition(proof_hash.clone());
                            }

                            lmdb_insert(&txn, &self.utxos_db, &k, &v)?;
                            let index = self.curr_range_proof_checkpoint.accumulated_nodes_added_count() - 1;
                            lmdb_insert(&txn, &self.txos_hash_to_index_db, &k, &index)?;
                        },
                        DbKeyValuePair::TransactionKernel(k, v, update_mmr) => {
                            if lmdb_exists(&self.env, &self.kernels_db, &k)? {
                                return Err(ChainStorageError::InvalidOperation("Duplicate key".to_string()));
                            }
                            if *update_mmr {
                                self.curr_kernel_checkpoint.push_addition(k.clone());
                            }
                            lmdb_insert(&txn, &self.kernels_db, &k, &v)?;
                        },
                        DbKeyValuePair::OrphanBlock(k, v) => {
                            lmdb_replace(&txn, &self.orphans_db, &k, &v)?;
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
                            let index_result: Option<u32> = lmdb_get(&self.env, &self.txos_hash_to_index_db, &hash)?;
                            match index_result {
                                Some(index) => {
                                    self.curr_utxo_checkpoint.push_deletion(index as u32);
                                },
                                None => return Err(ChainStorageError::UnspendableInput),
                            }

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
        txn.commit()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;

        if update_mem_metadata {
            self.mem_metadata = ChainMetadata {
                height_of_longest_chain: fetch_chain_height(&self.env, &self.metadata_db)?,
                best_block: fetch_best_block(&self.env, &self.metadata_db)?,
                pruning_horizon: fetch_pruning_horizon(&self.env, &self.metadata_db)?,
                accumulated_difficulty: fetch_accumulated_work(&self.env, &self.metadata_db)?,
            };
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

    fn fetch_checkpoint(&self, tree: MmrTree, height: u64) -> Result<MerkleCheckPoint, ChainStorageError> {
        match tree {
            MmrTree::Kernel => self.kernel_checkpoints.get(height as usize),
            MmrTree::Utxo => self.utxo_checkpoints.get(height as usize),
            MmrTree::RangeProof => self.range_proof_checkpoints.get(height as usize),
        }
        .map_err(|e| ChainStorageError::AccessError(format!("Checkpoint error: {}", e.to_string())))?
        .ok_or_else(|| ChainStorageError::OutOfRange)
    }

    fn fetch_mmr_node_count(&self, tree: MmrTree, height: u64) -> Result<u32, ChainStorageError> {
        match tree {
            MmrTree::Kernel => fetch_mmr_nodes_added_count(&self.kernel_checkpoints, height),
            MmrTree::Utxo => fetch_mmr_nodes_added_count(&self.utxo_checkpoints, height),
            MmrTree::RangeProof => fetch_mmr_nodes_added_count(&self.range_proof_checkpoints, height),
        }
    }

    fn fetch_mmr_node(&self, tree: MmrTree, pos: u32) -> Result<(Vec<u8>, bool), ChainStorageError> {
        let (hash, deleted) = match tree {
            MmrTree::Kernel => self.kernel_mmr.fetch_mmr_node(pos)?,
            MmrTree::Utxo => self.utxo_mmr.fetch_mmr_node(pos)?,
            MmrTree::RangeProof => self.range_proof_mmr.fetch_mmr_node(pos)?,
        };
        let hash = hash.ok_or_else(|| {
            ChainStorageError::UnexpectedResult(format!("A leaf node hash in the {} MMR tree was not found", tree))
        })?;
        Ok((hash, deleted))
    }

    fn fetch_mmr_nodes(&self, tree: MmrTree, pos: u32, count: u32) -> Result<Vec<(Vec<u8>, bool)>, ChainStorageError> {
        let mut leaf_nodes = Vec::<(Vec<u8>, bool)>::with_capacity(count as usize);
        for pos in pos..pos + count {
            leaf_nodes.push(self.fetch_mmr_node(tree.clone(), pos)?);
        }
        Ok(leaf_nodes)
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
            let k = header_count - 1;
            lmdb_get(&self.env, &self.headers_db, &k)
        } else {
            Ok(None)
        }
    }

    /// Returns the metadata of the chain.
    fn fetch_metadata(&self) -> Result<ChainMetadata, ChainStorageError> {
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
}

// Fetches the chain height from the provided metadata db.
fn fetch_chain_height(env: &Environment, db: &Database) -> Result<Option<u64>, ChainStorageError> {
    let k = MetadataKey::ChainHeight;
    let val: Option<MetadataValue> = lmdb_get(&env, &db, &(k as u32))?;
    let val: Option<DbValue> = val.map(DbValue::Metadata);
    Ok(
        if let Some(DbValue::Metadata(MetadataValue::ChainHeight(height))) = val {
            height
        } else {
            None
        },
    )
}

// Fetches the best block hash from the provided metadata db.
fn fetch_best_block(env: &Environment, db: &Database) -> Result<Option<BlockHash>, ChainStorageError> {
    let k = MetadataKey::BestBlock;
    let val: Option<MetadataValue> = lmdb_get(&env, &db, &(k as u32))?;
    let val: Option<DbValue> = val.map(DbValue::Metadata);
    Ok(
        if let Some(DbValue::Metadata(MetadataValue::BestBlock(best_block))) = val {
            best_block
        } else {
            None
        },
    )
}

// Fetches the accumulated work from the provided metadata db.
fn fetch_accumulated_work(env: &Environment, db: &Database) -> Result<Option<Difficulty>, ChainStorageError> {
    let k = MetadataKey::AccumulatedWork;
    let val: Option<MetadataValue> = lmdb_get(&env, &db, &(k as u32))?;
    let val: Option<DbValue> = val.map(DbValue::Metadata);
    Ok(
        if let Some(DbValue::Metadata(MetadataValue::AccumulatedWork(accumulated_work))) = val {
            accumulated_work
        } else {
            None
        },
    )
}

// Fetches the pruning horizon from the provided metadata db.
fn fetch_pruning_horizon(env: &Environment, db: &Database) -> Result<u64, ChainStorageError> {
    let k = MetadataKey::PruningHorizon;
    let val: Option<MetadataValue> = lmdb_get(&env, &db, &(k as u32))?;
    let val: Option<DbValue> = val.map(DbValue::Metadata);
    Ok(
        if let Some(DbValue::Metadata(MetadataValue::PruningHorizon(pruning_horizon))) = val {
            pruning_horizon
        } else {
            0
        },
    )
}

/// Calculate the total leaf node count upto a specified height.
fn fetch_mmr_nodes_added_count<T>(checkpoints: &T, height: u64) -> Result<u32, ChainStorageError>
where
    T: ArrayLike<Value = MerkleCheckPoint>,
    T::Error: Display,
{
    let len = checkpoints
        .len()
        .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;

    let last_index = cmp::min(len - 1, height as usize);
    let count = checkpoints
        .get(last_index)
        .map_err(|e| ChainStorageError::AccessError(format!("Checkpoint error: {}", e.to_string())))?
        .map(|cp| cp.accumulated_nodes_added_count())
        .unwrap_or(0);

    Ok(count as u32)
}

fn fetch_last_mmr_node_added_count<T>(checkpoints: &T) -> Result<u32, ChainStorageError>
where
    T: ArrayLike<Value = MerkleCheckPoint>,
    T::Error: Display,
{
    let cp_len = checkpoints
        .len()
        .map_err(|e| ChainStorageError::AccessError(format!("Failed to fetch range proof checkpoint length: {}", e)))?;

    if cp_len == 0 {
        return Ok(0);
    }

    fetch_mmr_nodes_added_count(checkpoints, cp_len as u64)
}

// Calculated the new checkpoint count after rewinding a set number of steps back.
fn rewind_checkpoint_index(cp_count: usize, steps_back: usize) -> usize {
    if cp_count > steps_back {
        cp_count - steps_back
    } else {
        1
    }
}
/// Rewinds checkpoints by `steps_back` elements and returns the last checkpoint.
fn rewind_checkpoints(
    checkpoints: &mut LMDBVec<MerkleCheckPoint>,
    steps_back: usize,
) -> Result<MerkleCheckPoint, ChainStorageError>
{
    let cp_count = checkpoints
        .len()
        .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;

    let rewind_len = rewind_checkpoint_index(cp_count, steps_back);
    checkpoints
        .truncate(rewind_len)
        .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;

    let last_cp = checkpoints
        .get(rewind_len - 1)
        .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
        .expect("rewind_checkpoint_index should ensure that all checkpoints cannot be removed");

    Ok(last_cp)
}
