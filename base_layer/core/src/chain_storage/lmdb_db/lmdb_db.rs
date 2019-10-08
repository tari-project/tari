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
            LMDB_DB_UTXOS,
            LMDB_DB_UTXO_MMR_BASE_BACKEND,
            LMDB_DB_UTXO_MMR_CP_BACKEND,
        },
    },
    transaction::{TransactionKernel, TransactionOutput},
    types::{HashDigest, HashOutput},
};
use digest::Digest;
use lmdb_zero::{Database, Environment, WriteTransaction};
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    sync::{Arc, RwLock},
};
use tari_mmr::{Hash as MmrHash, MerkleChangeTracker, MerkleCheckPoint, MerkleProof, MutableMmr};
use tari_storage::lmdb_store::{db, LMDBBuilder, LMDBStore};
use tari_utilities::hash::Hashable;

type DatabaseRef = Arc<Database<'static>>;

/// A generic struct for storing node objects in the BlockchainDB that also form part of an MMR. The index field makes
/// reverse lookups (find by hash) possible.
#[derive(Serialize, Deserialize)]
struct MerkleNode<T> {
    index: usize,
    value: T,
}

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
    kernels_db: DatabaseRef,
    orphans_db: DatabaseRef,
    utxo_mmr: RwLock<MerkleChangeTracker<D, LMDBVec<MmrHash>, Vec<MerkleCheckPoint> /* LMDBVec<MerkleCheckPoint> */>>,
    header_mmr: RwLock<MerkleChangeTracker<D, LMDBVec<MmrHash>, Vec<MerkleCheckPoint> /* LMDBVec<MerkleCheckPoint> */>>,
    kernel_mmr: RwLock<MerkleChangeTracker<D, LMDBVec<MmrHash>, Vec<MerkleCheckPoint> /* LMDBVec<MerkleCheckPoint> */>>,
    range_proof_mmr:
        RwLock<MerkleChangeTracker<D, LMDBVec<MmrHash>, Vec<MerkleCheckPoint> /* LMDBVec<MerkleCheckPoint> */>>,
}

impl<D> LMDBDatabase<D>
where D: Digest + Send + Sync
{
    pub fn new(store: LMDBStore) -> Result<Self, ChainStorageError> {
        let utxo_mmr_base_backend = LMDBVec::new(
            store.env(),
            store
                .get_handle(LMDB_DB_UTXO_MMR_BASE_BACKEND)
                .ok_or(ChainStorageError::CriticalError)?
                .db()
                .clone(),
        );
        // let utxo_mmr_cp_backend = LMDBVec::new(
        // store.env(),
        // store
        // .get_handle(LMDB_DB_UTXO_MMR_CP_BACKEND)
        // .ok_or(ChainStorageError::CriticalError)?
        // .db()
        // .clone(),
        // );
        let header_mmr_base_backend = LMDBVec::new(
            store.env(),
            store
                .get_handle(LMDB_DB_HEADER_MMR_BASE_BACKEND)
                .ok_or(ChainStorageError::CriticalError)?
                .db()
                .clone(),
        );
        // let header_mmr_cp_backend = LMDBVec::new(store.env(),
        // store
        // .get_handle(LMDB_DB_HEADER_MMR_CP_BACKEND)
        // .ok_or(ChainStorageError::CriticalError)?.db()
        //                .clone(),
        // );
        let kernel_mmr_base_backend = LMDBVec::new(
            store.env(),
            store
                .get_handle(LMDB_DB_KERNEL_MMR_BASE_BACKEND)
                .ok_or(ChainStorageError::CriticalError)?
                .db()
                .clone(),
        );
        // let kernel_mmr_cp_backend = LMDBVec::new(store.env(),
        // store
        // .get_handle(LMDB_DB_KERNEL_MMR_CP_BACKEND)
        // .ok_or(ChainStorageError::CriticalError)?.db()
        //                .clone(),
        // );
        let range_proof_mmr_base_backend = LMDBVec::new(
            store.env(),
            store
                .get_handle(LMDB_DB_RANGE_PROOF_MMR_BASE_BACKEND)
                .ok_or(ChainStorageError::CriticalError)?
                .db()
                .clone(),
        );
        // let range_proof_mmr_cp_backend = LMDBVec::new(store.env(),
        // store
        // .get_handle(LMDB_DB_RANGE_PROOF_MMR_CP_BACKEND)
        // .ok_or(ChainStorageError::CriticalError)?.db()
        //                .clone(),
        // );
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
                Vec::new(), // utxo_mmr_cp_backend,
            )?),
            header_mmr: RwLock::new(MerkleChangeTracker::new(
                MutableMmr::new(header_mmr_base_backend),
                Vec::new(), // header_mmr_cp_backend
            )?),
            kernel_mmr: RwLock::new(MerkleChangeTracker::new(
                MutableMmr::new(kernel_mmr_base_backend),
                Vec::new(), // kernel_mmr_cp_backend
            )?),
            range_proof_mmr: RwLock::new(MerkleChangeTracker::new(
                MutableMmr::new(range_proof_mmr_base_backend),
                Vec::new(), // range_proof_mmr_cp_backend
            )?),
            env: store.env(),
        })
    }
}

#[allow(dead_code)]
pub fn create_lmdb_database(path: &Path) -> Result<LMDBDatabase<HashDigest>, ChainStorageError> {
    let _ = std::fs::create_dir(&path).unwrap_or_default();
    let lmdb_store = LMDBBuilder::new()
        .set_path(path.to_str().unwrap())
        .set_environment_size(15)
        .set_max_number_of_databases(15)
        .add_database(LMDB_DB_METADATA, db::CREATE)
        .add_database(LMDB_DB_HEADERS, db::CREATE)
        .add_database(LMDB_DB_BLOCK_HASHES, db::CREATE)
        .add_database(LMDB_DB_UTXOS, db::CREATE)
        .add_database(LMDB_DB_STXOS, db::CREATE)
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
    LMDBDatabase::<HashDigest>::new(lmdb_store)
}

impl<D> BlockchainBackend for LMDBDatabase<D>
where D: Digest + Send + Sync
{
    fn write(&self, tx: DbTransaction) -> Result<(), ChainStorageError> {
        let txn = WriteTransaction::new(self.env.clone()).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        {
            for op in tx.operations.into_iter() {
                match op {
                    WriteOperation::Insert(insert) => match insert {
                        DbKeyValuePair::Metadata(k, v) => {
                            lmdb_insert(&txn, &self.metadata_db, &(k as u32), &v)?;
                        },
                        DbKeyValuePair::BlockHeader(k, v) => {
                            let hash = v.hash();
                            lmdb_insert(&txn, &self.block_hashes_db, &hash, &k)?;
                            let index = self
                                .header_mmr
                                .write()
                                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                                .push(&hash)? -
                                1 as usize;
                            let v = MerkleNode { index, value: *v };
                            lmdb_insert(&txn, &self.headers_db, &k, &v)?;
                        },
                        DbKeyValuePair::UnspentOutput(k, v) => {
                            self.utxo_mmr
                                .write()
                                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                                .push(&k)?;
                            let proof_hash = v.proof().hash();
                            let index = self
                                .range_proof_mmr
                                .write()
                                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                                .push(&proof_hash)? -
                                1;
                            let v = MerkleNode { index, value: *v };
                            lmdb_insert(&txn, &self.utxos_db, &k, &v)?;
                        },
                        DbKeyValuePair::TransactionKernel(k, v) => {
                            let index = self
                                .kernel_mmr
                                .write()
                                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                                .push(&k)? -
                                1;
                            let v = MerkleNode { index, value: *v };
                            lmdb_insert(&txn, &self.kernels_db, &k, &v)?;
                        },
                        DbKeyValuePair::OrphanBlock(k, v) => {
                            lmdb_insert(&txn, &self.orphans_db, &k, &v)?;
                        },
                        DbKeyValuePair::CommitBlock => {
                            self.utxo_mmr
                                .write()
                                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                                .commit()
                                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
                            self.header_mmr
                                .write()
                                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                                .commit()
                                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
                            self.range_proof_mmr
                                .write()
                                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                                .commit()
                                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
                            self.kernel_mmr
                                .write()
                                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                                .commit()
                                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
                        },
                    },
                    WriteOperation::Delete(delete) => match delete {
                        DbKey::Metadata(_) => {}, // no-op
                        DbKey::BlockHeader(k) => {
                            lmdb_delete(&txn, &self.headers_db, &k)?;
                            // TODO: shouldn't blockhash also be deleted
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
                        },
                        DbKey::SpentOutput(k) => {
                            lmdb_delete(&txn, &self.stxos_db, &k)?;
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
                            let utxo_result: Option<MerkleNode<TransactionOutput>> =
                                lmdb_get(&self.env, &self.utxos_db, &hash)?;
                            match utxo_result {
                                Some(utxo) => {
                                    lmdb_delete(&txn, &self.utxos_db, &hash)?;
                                    self.utxo_mmr
                                        .write()
                                        .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                                        .delete(utxo.index as u32);
                                    lmdb_insert(&txn, &self.stxos_db, &hash, &utxo)?;
                                },
                                None => return Err(ChainStorageError::UnspendableInput),
                            }
                        },
                        _ => return Err(ChainStorageError::InvalidOperation("Only UTXOs can be spent".into())),
                    },
                    WriteOperation::UnSpend(key) => match key {
                        DbKey::SpentOutput(hash) => {
                            let stxo_result: Option<MerkleNode<TransactionOutput>> =
                                lmdb_get(&self.env, &self.stxos_db, &hash)?;
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
                    WriteOperation::RewindMmr(tree, steps_back) => match tree {
                        MmrTree::Header => {
                            self.header_mmr
                                .write()
                                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                                .rewind(steps_back)
                                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
                        },
                        MmrTree::Kernel => {
                            self.kernel_mmr
                                .write()
                                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                                .rewind(steps_back)
                                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
                        },
                        MmrTree::Utxo => {
                            self.utxo_mmr
                                .write()
                                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                                .rewind(steps_back)
                                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
                        },
                        MmrTree::RangeProof => {
                            self.range_proof_mmr
                                .write()
                                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                                .rewind(steps_back)
                                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
                        },
                    },
                }
            }
        }
        txn.commit().map_err(|e| ChainStorageError::AccessError(e.to_string()))
    }

    fn fetch(&self, key: &DbKey) -> Result<Option<DbValue>, ChainStorageError> {
        let result = match key {
            DbKey::Metadata(k) => {
                let val: Option<MetadataValue> = lmdb_get(&self.env, &self.metadata_db, &(k.clone() as u32))?;
                val.map(|val| DbValue::Metadata(val))
            },
            DbKey::BlockHeader(k) => {
                let val: Option<MerkleNode<BlockHeader>> = lmdb_get(&self.env, &self.headers_db, k)?;
                val.map(|val| DbValue::BlockHeader(Box::new(val.value)))
            },
            DbKey::BlockHash(hash) => {
                let k: Option<u64> = lmdb_get(&self.env, &self.block_hashes_db, hash)?;
                match k {
                    Some(k) => {
                        let val: Option<MerkleNode<BlockHeader>> = lmdb_get(&self.env, &self.headers_db, &k)?;
                        val.map(|val| DbValue::BlockHash(Box::new(val.value)))
                    },
                    None => None,
                }
            },
            DbKey::UnspentOutput(k) => {
                let val: Option<MerkleNode<TransactionOutput>> = lmdb_get(&self.env, &self.utxos_db, k)?;
                val.map(|val| DbValue::UnspentOutput(Box::new(val.value)))
            },
            DbKey::SpentOutput(k) => {
                let val: Option<MerkleNode<TransactionOutput>> = lmdb_get(&self.env, &self.stxos_db, k)?;
                val.map(|val| DbValue::SpentOutput(Box::new(val.value)))
            },
            DbKey::TransactionKernel(k) => {
                let val: Option<MerkleNode<TransactionKernel>> = lmdb_get(&self.env, &self.kernels_db, k)?;
                val.map(|val| DbValue::TransactionKernel(Box::new(val.value)))
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

    fn fetch_mmr_checkpoint(&self, tree: MmrTree, index: u64) -> Result<MerkleCheckPoint, ChainStorageError> {
        let index = index as usize;
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

    /// Iterate over all the stored orphan blocks and execute the function `f` for each block.
    fn for_each_orphan<F>(&self, f: F) -> Result<(), ChainStorageError>
    where F: FnMut(Result<(HashOutput, Block), ChainStorageError>) {
        lmdb_for_each::<F, HashOutput, Block>(&self.env, &self.orphans_db, f)
    }
}
