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
        blockchain_database::{BlockchainBackend, MutableMmrState},
        db_transaction::{DbKey, DbKeyValuePair, DbTransaction, DbValue, MetadataValue, MmrTree, WriteOperation},
        error::ChainStorageError,
        lmdb_db::{
            lmdb::{lmdb_delete, lmdb_exists, lmdb_for_each, lmdb_get, lmdb_insert},
            LMDBVec,
            LMDB_DB_BLOCK_HASHES,
            LMDB_DB_HEADERS,
            LMDB_DB_HEADER_MMR_BASE_BACKEND,
            LMDB_DB_HEADER_MMR_CP_BACKEND,
            LMDB_DB_KERNELS,
            LMDB_DB_KERNEL_MMR_BASE_BACKEND,
            LMDB_DB_KERNEL_MMR_CP_BACKEND,
            LMDB_DB_METADATA,
            LMDB_DB_ORPHANS,
            LMDB_DB_RANGE_PROOF_MMR_BASE_BACKEND,
            LMDB_DB_RANGE_PROOF_MMR_CP_BACKEND,
            LMDB_DB_STXOS,
            LMDB_DB_TXOS_HASH_TO_INDEX,
            LMDB_DB_UTXOS,
            LMDB_DB_UTXO_MMR_BASE_BACKEND,
            LMDB_DB_UTXO_MMR_CP_BACKEND,
        },
    },
};
use digest::Digest;
use lmdb_zero::{Database, Environment, WriteTransaction};
use std::{
    path::Path,
    sync::{Arc, RwLock},
};
use tari_mmr::{
    functions::prune_mutable_mmr,
    Hash as MmrHash,
    MerkleChangeTracker,
    MerkleChangeTrackerConfig,
    MerkleCheckPoint,
    MerkleProof,
    MutableMmr,
    MutableMmrLeafNodes,
};
use tari_storage::lmdb_store::{db, LMDBBuilder, LMDBStore};
use tari_transactions::{
    transaction::{TransactionKernel, TransactionOutput},
    types::{HashDigest, HashOutput},
};
use tari_utilities::hash::Hashable;

type DatabaseRef = Arc<Database<'static>>;

/// This is a lmdb-based blockchain database for persistent storage of the chain state.
pub struct LMDBDatabase<D>
where D: Digest
{
    env: Arc<Environment>,
    metadata_db: DatabaseRef,
    headers_db: DatabaseRef,
    block_hashes_db: DatabaseRef,
    utxos_db: DatabaseRef,
    stxos_db: DatabaseRef,
    txos_hash_to_index_db: DatabaseRef,
    kernels_db: DatabaseRef,
    orphans_db: DatabaseRef,
    utxo_mmr: RwLock<MerkleChangeTracker<D, LMDBVec<MmrHash>, LMDBVec<MerkleCheckPoint>>>,
    header_mmr: RwLock<MerkleChangeTracker<D, LMDBVec<MmrHash>, LMDBVec<MerkleCheckPoint>>>,
    kernel_mmr: RwLock<MerkleChangeTracker<D, LMDBVec<MmrHash>, LMDBVec<MerkleCheckPoint>>>,
    range_proof_mmr: RwLock<MerkleChangeTracker<D, LMDBVec<MmrHash>, LMDBVec<MerkleCheckPoint>>>,
}

impl<D> LMDBDatabase<D>
where D: Digest + Send + Sync
{
    pub fn new(store: LMDBStore, mct_config: MerkleChangeTrackerConfig) -> Result<Self, ChainStorageError> {
        let utxo_mmr_base_backend = LMDBVec::new(
            store.env(),
            store
                .get_handle(LMDB_DB_UTXO_MMR_BASE_BACKEND)
                .ok_or(ChainStorageError::CriticalError)?
                .db()
                .clone(),
        );
        let utxo_mmr_cp_backend = LMDBVec::new(
            store.env(),
            store
                .get_handle(LMDB_DB_UTXO_MMR_CP_BACKEND)
                .ok_or(ChainStorageError::CriticalError)?
                .db()
                .clone(),
        );
        let header_mmr_base_backend = LMDBVec::new(
            store.env(),
            store
                .get_handle(LMDB_DB_HEADER_MMR_BASE_BACKEND)
                .ok_or(ChainStorageError::CriticalError)?
                .db()
                .clone(),
        );
        let header_mmr_cp_backend = LMDBVec::new(
            store.env(),
            store
                .get_handle(LMDB_DB_HEADER_MMR_CP_BACKEND)
                .ok_or(ChainStorageError::CriticalError)?
                .db()
                .clone(),
        );
        let kernel_mmr_base_backend = LMDBVec::new(
            store.env(),
            store
                .get_handle(LMDB_DB_KERNEL_MMR_BASE_BACKEND)
                .ok_or(ChainStorageError::CriticalError)?
                .db()
                .clone(),
        );
        let kernel_mmr_cp_backend = LMDBVec::new(
            store.env(),
            store
                .get_handle(LMDB_DB_KERNEL_MMR_CP_BACKEND)
                .ok_or(ChainStorageError::CriticalError)?
                .db()
                .clone(),
        );
        let range_proof_mmr_base_backend = LMDBVec::new(
            store.env(),
            store
                .get_handle(LMDB_DB_RANGE_PROOF_MMR_BASE_BACKEND)
                .ok_or(ChainStorageError::CriticalError)?
                .db()
                .clone(),
        );
        let range_proof_mmr_cp_backend = LMDBVec::new(
            store.env(),
            store
                .get_handle(LMDB_DB_RANGE_PROOF_MMR_CP_BACKEND)
                .ok_or(ChainStorageError::CriticalError)?
                .db()
                .clone(),
        );
        Ok(Self {
            metadata_db: store
                .get_handle(LMDB_DB_METADATA)
                .ok_or(ChainStorageError::CriticalError)?
                .db()
                .clone(),
            headers_db: store
                .get_handle(LMDB_DB_HEADERS)
                .ok_or(ChainStorageError::CriticalError)?
                .db()
                .clone(),
            block_hashes_db: store
                .get_handle(LMDB_DB_BLOCK_HASHES)
                .ok_or(ChainStorageError::CriticalError)?
                .db()
                .clone(),
            utxos_db: store
                .get_handle(LMDB_DB_UTXOS)
                .ok_or(ChainStorageError::CriticalError)?
                .db()
                .clone(),
            stxos_db: store
                .get_handle(LMDB_DB_STXOS)
                .ok_or(ChainStorageError::CriticalError)?
                .db()
                .clone(),
            txos_hash_to_index_db: store
                .get_handle(LMDB_DB_TXOS_HASH_TO_INDEX)
                .ok_or(ChainStorageError::CriticalError)?
                .db()
                .clone(),
            kernels_db: store
                .get_handle(LMDB_DB_KERNELS)
                .ok_or(ChainStorageError::CriticalError)?
                .db()
                .clone(),
            orphans_db: store
                .get_handle(LMDB_DB_ORPHANS)
                .ok_or(ChainStorageError::CriticalError)?
                .db()
                .clone(),
            utxo_mmr: RwLock::new(MerkleChangeTracker::new(
                MutableMmr::new(utxo_mmr_base_backend),
                utxo_mmr_cp_backend,
                mct_config,
            )?),
            header_mmr: RwLock::new(MerkleChangeTracker::new(
                MutableMmr::new(header_mmr_base_backend),
                header_mmr_cp_backend,
                mct_config,
            )?),
            kernel_mmr: RwLock::new(MerkleChangeTracker::new(
                MutableMmr::new(kernel_mmr_base_backend),
                kernel_mmr_cp_backend,
                mct_config,
            )?),
            range_proof_mmr: RwLock::new(MerkleChangeTracker::new(
                MutableMmr::new(range_proof_mmr_base_backend),
                range_proof_mmr_cp_backend,
                mct_config,
            )?),
            env: store.env(),
        })
    }

    // Applies all MMR transactions excluding CreateMmrCheckpoint and RewindMmr on the header_mmr, utxo_mmr,
    // range_proof_mmr and kernel_mmr. CreateMmrCheckpoint and RewindMmr txns will be performed after the the storage
    // txns have been successfully applied.
    fn apply_mmr_txs(&self, tx: &DbTransaction) -> Result<(), ChainStorageError> {
        for op in tx.operations.iter() {
            match op {
                WriteOperation::Insert(insert) => match insert {
                    DbKeyValuePair::BlockHeader(_k, v) => {
                        let hash = v.hash();
                        self.header_mmr
                            .write()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                            .push(&hash)?;
                    },
                    DbKeyValuePair::UnspentOutput(k, v) => {
                        self.utxo_mmr
                            .write()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                            .push(&k)?;
                        let proof_hash = v.proof().hash();
                        self.range_proof_mmr
                            .write()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                            .push(&proof_hash)?;
                    },
                    DbKeyValuePair::TransactionKernel(k, _v) => {
                        self.kernel_mmr
                            .write()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                            .push(&k)?;
                    },
                    _ => {},
                },
                WriteOperation::Spend(key) => match key {
                    DbKey::UnspentOutput(hash) => {
                        let index_result: Option<usize> = lmdb_get(&self.env, &self.txos_hash_to_index_db, &hash)?;
                        match index_result {
                            Some(index) => {
                                self.utxo_mmr
                                    .write()
                                    .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                                    .delete(index as u32);
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
    fn commit_mmrs(&self, tx: DbTransaction) -> Result<(), ChainStorageError> {
        for op in tx.operations.into_iter() {
            match op {
                WriteOperation::RewindMmr(tree, steps_back) => match tree {
                    MmrTree::Header => {
                        if steps_back == 0 {
                            self.header_mmr
                                .write()
                                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                                .reset()
                                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
                        } else {
                            self.header_mmr
                                .write()
                                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                                .rewind(steps_back)
                                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
                        }
                    },
                    MmrTree::Kernel => {
                        if steps_back == 0 {
                            self.kernel_mmr
                                .write()
                                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                                .reset()
                                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
                        } else {
                            self.kernel_mmr
                                .write()
                                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                                .rewind(steps_back)
                                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
                        }
                    },
                    MmrTree::Utxo => {
                        if steps_back == 0 {
                            self.utxo_mmr
                                .write()
                                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                                .reset()
                                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
                        } else {
                            self.utxo_mmr
                                .write()
                                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                                .rewind(steps_back)
                                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
                        }
                    },
                    MmrTree::RangeProof => {
                        if steps_back == 0 {
                            self.range_proof_mmr
                                .write()
                                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                                .reset()
                                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
                        } else {
                            self.range_proof_mmr
                                .write()
                                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                                .rewind(steps_back)
                                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
                        }
                    },
                },
                WriteOperation::CreateMmrCheckpoint(tree) => match tree {
                    MmrTree::Header => {
                        self.header_mmr
                            .write()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                            .commit()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
                    },
                    MmrTree::Kernel => {
                        self.kernel_mmr
                            .write()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                            .commit()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
                    },
                    MmrTree::Utxo => {
                        self.utxo_mmr
                            .write()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                            .commit()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
                    },
                    MmrTree::RangeProof => {
                        self.range_proof_mmr
                            .write()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                            .commit()
                            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
                    },
                },
                _ => {},
            }
        }
        Ok(())
    }

    // Reset any mmr txns that have been applied.
    fn reset_mmrs(&self) -> Result<(), ChainStorageError> {
        self.header_mmr
            .write()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
            .reset()?;
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
                            lmdb_insert(&txn, &self.metadata_db, &(k.clone() as u32), &v)?;
                        },
                        DbKeyValuePair::BlockHeader(k, v) => {
                            let hash = v.hash();
                            lmdb_insert(&txn, &self.block_hashes_db, &hash, &k)?;
                            lmdb_insert(&txn, &self.headers_db, &k, &v)?;
                        },
                        DbKeyValuePair::UnspentOutput(k, v) => {
                            let proof_hash = v.proof().hash();
                            if let Some(index) = self
                                .range_proof_mmr
                                .read()
                                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                                .find_leaf_index(&proof_hash)?
                            {
                                lmdb_insert(&txn, &self.utxos_db, &k, &v)?;
                                lmdb_insert(&txn, &self.txos_hash_to_index_db, &k, &index)?;
                            }
                        },
                        DbKeyValuePair::TransactionKernel(k, v) => {
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
}

#[allow(dead_code)]
pub fn create_lmdb_database(
    path: &Path,
    mct_config: MerkleChangeTrackerConfig,
) -> Result<LMDBDatabase<HashDigest>, ChainStorageError>
{
    let _ = std::fs::create_dir_all(&path).unwrap_or_default();
    let lmdb_store = LMDBBuilder::new()
        .set_path(path.to_str().unwrap())
        .set_environment_size(15)
        .set_max_number_of_databases(15)
        .add_database(LMDB_DB_METADATA, db::CREATE)
        .add_database(LMDB_DB_HEADERS, db::CREATE)
        .add_database(LMDB_DB_BLOCK_HASHES, db::CREATE)
        .add_database(LMDB_DB_UTXOS, db::CREATE)
        .add_database(LMDB_DB_STXOS, db::CREATE)
        .add_database(LMDB_DB_TXOS_HASH_TO_INDEX, db::CREATE)
        .add_database(LMDB_DB_KERNELS, db::CREATE)
        .add_database(LMDB_DB_ORPHANS, db::CREATE)
        .add_database(LMDB_DB_UTXO_MMR_BASE_BACKEND, db::CREATE)
        .add_database(LMDB_DB_UTXO_MMR_CP_BACKEND, db::CREATE)
        .add_database(LMDB_DB_HEADER_MMR_BASE_BACKEND, db::CREATE)
        .add_database(LMDB_DB_HEADER_MMR_CP_BACKEND, db::CREATE)
        .add_database(LMDB_DB_KERNEL_MMR_BASE_BACKEND, db::CREATE)
        .add_database(LMDB_DB_KERNEL_MMR_CP_BACKEND, db::CREATE)
        .add_database(LMDB_DB_RANGE_PROOF_MMR_BASE_BACKEND, db::CREATE)
        .add_database(LMDB_DB_RANGE_PROOF_MMR_CP_BACKEND, db::CREATE)
        .build()
        .map_err(|_| ChainStorageError::CriticalError)?;
    LMDBDatabase::<HashDigest>::new(lmdb_store, mct_config)
}

impl<D> BlockchainBackend for LMDBDatabase<D>
where D: Digest + Send + Sync
{
    fn write(&self, tx: DbTransaction) -> Result<(), ChainStorageError> {
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
        let result = match key {
            DbKey::Metadata(k) => {
                let val: Option<MetadataValue> = lmdb_get(&self.env, &self.metadata_db, &(k.clone() as u32))?;
                val.map(|val| DbValue::Metadata(val))
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
        };
        Ok(result)
    }

    fn contains(&self, key: &DbKey) -> Result<bool, ChainStorageError> {
        let result = match key {
            DbKey::Metadata(k) => lmdb_exists(&self.env, &self.metadata_db, &(k.clone() as u32))?,
            DbKey::BlockHeader(k) => lmdb_exists(&self.env, &self.headers_db, k)?,
            DbKey::BlockHash(h) => lmdb_exists(&self.env, &self.block_hashes_db, h)?,
            DbKey::UnspentOutput(k) => lmdb_exists(&self.env, &self.utxos_db, k)?,
            DbKey::SpentOutput(k) => lmdb_exists(&self.env, &self.stxos_db, k)?,
            DbKey::TransactionKernel(k) => lmdb_exists(&self.env, &self.kernels_db, k)?,
            DbKey::OrphanBlock(k) => lmdb_exists(&self.env, &self.orphans_db, k)?,
        };
        Ok(result)
    }

    fn fetch_mmr_root(&self, tree: MmrTree) -> Result<Vec<u8>, ChainStorageError> {
        let root = match tree {
            MmrTree::Utxo => self
                .utxo_mmr
                .read()
                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                .get_merkle_root()?,
            MmrTree::Kernel => self
                .kernel_mmr
                .read()
                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                .get_merkle_root()?,
            MmrTree::RangeProof => self
                .range_proof_mmr
                .read()
                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                .get_merkle_root()?,
            MmrTree::Header => self
                .header_mmr
                .read()
                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                .get_merkle_root()?,
        };
        Ok(root)
    }

    fn fetch_mmr_only_root(&self, tree: MmrTree) -> Result<Vec<u8>, ChainStorageError> {
        let root = match tree {
            MmrTree::Utxo => self
                .utxo_mmr
                .read()
                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                .get_mmr_only_root()?,
            MmrTree::Kernel => self
                .kernel_mmr
                .read()
                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                .get_mmr_only_root()?,
            MmrTree::RangeProof => self
                .range_proof_mmr
                .read()
                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                .get_mmr_only_root()?,
            MmrTree::Header => self
                .header_mmr
                .read()
                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                .get_mmr_only_root()?,
        };
        Ok(root)
    }

    fn calculate_mmr_root(
        &self,
        tree: MmrTree,
        additions: Vec<HashOutput>,
        deletions: Vec<HashOutput>,
    ) -> Result<Vec<u8>, ChainStorageError>
    {
        let mut pruned_mmr = match tree {
            MmrTree::Utxo => prune_mutable_mmr(
                &*self
                    .utxo_mmr
                    .read()
                    .map_err(|e| ChainStorageError::AccessError(e.to_string()))?,
            )?,
            MmrTree::Kernel => prune_mutable_mmr(
                &*self
                    .kernel_mmr
                    .read()
                    .map_err(|e| ChainStorageError::AccessError(e.to_string()))?,
            )?,
            MmrTree::RangeProof => prune_mutable_mmr(
                &*self
                    .range_proof_mmr
                    .read()
                    .map_err(|e| ChainStorageError::AccessError(e.to_string()))?,
            )?,
            MmrTree::Header => prune_mutable_mmr(
                &*self
                    .header_mmr
                    .read()
                    .map_err(|e| ChainStorageError::AccessError(e.to_string()))?,
            )?,
        };
        for hash in additions {
            pruned_mmr.push(&hash)?;
        }
        if let MmrTree::Utxo = tree {
            for hash in deletions {
                if let Some(index) = lmdb_get(&self.env, &self.txos_hash_to_index_db, &hash)? {
                    pruned_mmr.delete(index);
                }
            }
        }
        Ok(pruned_mmr.get_merkle_root()?)
    }

    /// Returns an MMR proof extracted from the full Merkle mountain range without trimming the MMR using the roaring
    /// bitmap
    fn fetch_mmr_proof(&self, tree: MmrTree, leaf_pos: usize) -> Result<MerkleProof, ChainStorageError> {
        let proof = match tree {
            MmrTree::Utxo => MerkleProof::for_leaf_node(
                &self
                    .utxo_mmr
                    .read()
                    .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                    .mmr(),
                leaf_pos,
            )?,
            MmrTree::Kernel => MerkleProof::for_leaf_node(
                &self
                    .kernel_mmr
                    .read()
                    .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                    .mmr(),
                leaf_pos,
            )?,
            MmrTree::RangeProof => MerkleProof::for_leaf_node(
                &self
                    .range_proof_mmr
                    .read()
                    .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                    .mmr(),
                leaf_pos,
            )?,
            MmrTree::Header => MerkleProof::for_leaf_node(
                &self
                    .header_mmr
                    .read()
                    .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                    .mmr(),
                leaf_pos,
            )?,
        };
        Ok(proof)
    }

    fn fetch_mmr_checkpoint(&self, tree: MmrTree, height: u64) -> Result<MerkleCheckPoint, ChainStorageError> {
        let pruning_horizon = self.fetch_pruning_horizon()?;
        if height < pruning_horizon {
            return Err(ChainStorageError::BeyondPruningHorizon);
        }
        let index = (height - pruning_horizon) as usize;
        let cp = match tree {
            MmrTree::Kernel => self
                .kernel_mmr
                .read()
                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                .get_checkpoint(index),
            MmrTree::Utxo => self
                .utxo_mmr
                .read()
                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                .get_checkpoint(index),
            MmrTree::RangeProof => self
                .range_proof_mmr
                .read()
                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                .get_checkpoint(index),
            MmrTree::Header => self
                .header_mmr
                .read()
                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                .get_checkpoint(index),
        };
        cp.map_err(|e| ChainStorageError::AccessError(format!("MMR Checkpoint error: {}", e.to_string())))
    }

    fn fetch_mmr_node(&self, tree: MmrTree, pos: u32) -> Result<(Vec<u8>, bool), ChainStorageError> {
        let (hash, deleted) = match tree {
            MmrTree::Kernel => self
                .kernel_mmr
                .read()
                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                .get_leaf_status(pos)?,
            MmrTree::Header => self
                .header_mmr
                .read()
                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                .get_leaf_status(pos)?,
            MmrTree::Utxo => self
                .utxo_mmr
                .read()
                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                .get_leaf_status(pos)?,
            MmrTree::RangeProof => self
                .range_proof_mmr
                .read()
                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                .get_leaf_status(pos)?,
        };
        let hash = hash
            .ok_or(ChainStorageError::UnexpectedResult(format!(
                "A leaf node hash in the {} MMR tree was not found",
                tree
            )))?
            .clone();
        Ok((hash, deleted))
    }

    fn fetch_mmr_base_leaf_nodes(
        &self,
        tree: MmrTree,
        index: usize,
        count: usize,
    ) -> Result<MutableMmrState, ChainStorageError>
    {
        let mmr_state = match tree {
            MmrTree::Kernel => {
                let total_leaf_count = self
                    .kernel_mmr
                    .read()
                    .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                    .get_base_leaf_count();
                let leaf_nodes = self
                    .kernel_mmr
                    .read()
                    .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                    .to_base_leaf_nodes(index, count)?;
                MutableMmrState {
                    total_leaf_count,
                    leaf_nodes,
                }
            },
            MmrTree::Header => {
                let total_leaf_count = self
                    .header_mmr
                    .read()
                    .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                    .get_base_leaf_count();
                let leaf_nodes = self
                    .header_mmr
                    .read()
                    .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                    .to_base_leaf_nodes(index, count)?;
                MutableMmrState {
                    total_leaf_count,
                    leaf_nodes,
                }
            },
            MmrTree::Utxo => {
                let total_leaf_count = self
                    .utxo_mmr
                    .read()
                    .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                    .get_base_leaf_count();
                let leaf_nodes = self
                    .utxo_mmr
                    .read()
                    .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                    .to_base_leaf_nodes(index, count)?;
                MutableMmrState {
                    total_leaf_count,
                    leaf_nodes,
                }
            },
            MmrTree::RangeProof => {
                let total_leaf_count = self
                    .range_proof_mmr
                    .read()
                    .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                    .get_base_leaf_count();
                let leaf_nodes = self
                    .range_proof_mmr
                    .read()
                    .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                    .to_base_leaf_nodes(index, count)?;
                MutableMmrState {
                    total_leaf_count,
                    leaf_nodes,
                }
            },
        };
        Ok(mmr_state)
    }

    fn fetch_mmr_base_leaf_node_count(&self, tree: MmrTree) -> Result<usize, ChainStorageError> {
        let mmr_state = match tree {
            MmrTree::Kernel => self
                .kernel_mmr
                .read()
                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                .get_base_leaf_count(),
            MmrTree::Header => self
                .header_mmr
                .read()
                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                .get_base_leaf_count(),
            MmrTree::Utxo => self
                .utxo_mmr
                .read()
                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                .get_base_leaf_count(),
            MmrTree::RangeProof => self
                .range_proof_mmr
                .read()
                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                .get_base_leaf_count(),
        };
        Ok(mmr_state)
    }

    fn restore_mmr(&self, tree: MmrTree, base_state: MutableMmrLeafNodes) -> Result<(), ChainStorageError> {
        match tree {
            MmrTree::Kernel => self
                .kernel_mmr
                .write()
                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                .restore(base_state)?,
            MmrTree::Header => self
                .header_mmr
                .write()
                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                .restore(base_state)?,
            MmrTree::Utxo => self
                .utxo_mmr
                .write()
                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                .restore(base_state)?,
            MmrTree::RangeProof => self
                .range_proof_mmr
                .write()
                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                .restore(base_state)?,
        };
        Ok(())
    }

    /// Iterate over all the stored orphan blocks and execute the function `f` for each block.
    fn for_each_orphan<F>(&self, f: F) -> Result<(), ChainStorageError>
    where F: FnMut(Result<(HashOutput, Block), ChainStorageError>) {
        lmdb_for_each::<F, HashOutput, Block>(&self.env, &self.orphans_db, f)
    }

    fn fetch_pruning_horizon(&self) -> Result<u64, ChainStorageError> {
        Ok(self
            .header_mmr
            .read()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
            .get_base_leaf_count() as u64)
    }
}
