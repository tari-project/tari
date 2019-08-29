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

//! This is a memory-based blockchain database, generally only useful for testing purposes

use crate::{
    blocks::{block::Block, blockheader::BlockHeader},
    chain_storage::{
        blockchain_database::{BlockchainBackend, MmrTree},
        error::ChainStorageError,
        transaction::{DbKey, DbKeyValuePair, DbTransaction, DbValue, MetadataKey, MetadataValue, WriteOperation},
        ChainMetadata,
    },
    transaction::{TransactionKernel, TransactionOutput},
    types::HashOutput,
};
use digest::Digest;
use std::{
    collections::HashMap,
    sync::{Arc, RwLock, RwLockWriteGuard},
};
use tari_mmr::{Hash as MmrHash, MerkleChangeTracker, MerkleCheckPoint, MutableMmr};
use tari_utilities::hash::Hashable;

struct InnerDatabase<D>
where D: Digest
{
    metadata: ChainMetadata,
    headers: HashMap<u64, BlockHeader>,
    utxos: HashMap<HashOutput, TransactionOutput>,
    stxos: HashMap<HashOutput, TransactionOutput>,
    kernels: HashMap<HashOutput, TransactionKernel>,
    orphans: HashMap<HashOutput, Block>,
    // Define MMRs to use both a memory-backed base and a memory-backed pruned MMR
    utxo_mmr: MerkleChangeTracker<D, Vec<MmrHash>, Vec<MerkleCheckPoint>>,
    header_mmr: MerkleChangeTracker<D, Vec<MmrHash>, Vec<MerkleCheckPoint>>,
    kernel_mmr: MerkleChangeTracker<D, Vec<MmrHash>, Vec<MerkleCheckPoint>>,
    range_proof_mmr: MerkleChangeTracker<D, Vec<MmrHash>, Vec<MerkleCheckPoint>>,
}

/// A memory-backed blockchain database. The data is stored in RAM; and so all data will be lost when the program
/// terminates. Thus this DB is intended for testing purposes. It's also not very efficient since a single Mutex
/// protects the entire database. Again: testing.
#[derive(Default)]
pub struct MemoryDatabase<D>
where D: Digest
{
    db: Arc<RwLock<InnerDatabase<D>>>,
}

impl<D> BlockchainBackend for MemoryDatabase<D>
where D: Digest + Send + Sync
{
    fn write(&self, tx: DbTransaction) -> Result<(), ChainStorageError> {
        let mut db = self
            .db
            .write()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        // Not **really** atomic, but..
        // Hashmap insertions don't typically fail and b) MemoryDB should not be used for production anyway.
        for op in tx.operations.into_iter() {
            match op {
                WriteOperation::Insert(insert) => match insert {
                    DbKeyValuePair::Metadata(_, MetadataValue::ChainHeight(h)) => {
                        db.metadata.height_of_longest_chain = h;
                    },
                    DbKeyValuePair::Metadata(_, MetadataValue::AccumulatedWork(w)) => {
                        db.metadata.total_accumulated_difficulty = w;
                    },
                    DbKeyValuePair::BlockHeader(k, v) => {
                        let hash = v.hash();
                        db.header_mmr.push(&hash).unwrap();
                        db.headers.insert(k, *v);
                    },
                    DbKeyValuePair::UnspentOutput(k, v) => {
                        db.utxo_mmr.push(&k).unwrap();
                        let proof_hash = v.proof().hash();
                        let _ = db.range_proof_mmr.push(&proof_hash);
                        db.utxos.insert(k, *v);
                    },
                    DbKeyValuePair::SpentOutput(k, v) => {
                        db.stxos.insert(k, *v);
                    },
                    DbKeyValuePair::TransactionKernel(k, v) => {
                        db.kernel_mmr.push(&k).unwrap();
                        db.kernels.insert(k, *v);
                    },
                    DbKeyValuePair::OrphanBlock(k, v) => {
                        db.orphans.insert(k, *v);
                    },
                },
                WriteOperation::Delete(delete) => match delete {
                    DbKey::Metadata(_) => {}, // no-op
                    DbKey::BlockHeader(k) => {
                        db.headers.remove(&k);
                    },
                    DbKey::UnspentOutput(k) => {
                        db.utxos.remove(&k);
                    },
                    DbKey::SpentOutput(k) => {
                        db.stxos.remove(&k);
                    },
                    DbKey::TransactionKernel(k) => {
                        db.kernels.remove(&k);
                    },
                    DbKey::OrphanBlock(k) => {
                        db.orphans.remove(&k);
                    },
                },
                WriteOperation::Move(key) => match key {
                    DbKey::UnspentOutput(hash) => {
                        let moved = spend_utxo(&mut db, hash);
                        if !moved {
                            return Err(ChainStorageError::UnspendableInput);
                        }
                    },
                    _ => return Err(ChainStorageError::InvalidOperation("Only UTXOs can be moved".into())),
                },
            }
        }
        Ok(())
    }

    fn fetch(&self, key: &DbKey) -> Result<Option<DbValue>, ChainStorageError> {
        let db = self
            .db
            .read()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        let result = match key {
            DbKey::Metadata(MetadataKey::ChainHeight) => Some(DbValue::Metadata(MetadataValue::ChainHeight(
                db.metadata.height_of_longest_chain,
            ))),
            DbKey::Metadata(MetadataKey::AccumulatedWork) => Some(DbValue::Metadata(MetadataValue::AccumulatedWork(
                db.metadata.total_accumulated_difficulty,
            ))),
            DbKey::BlockHeader(k) => db.headers.get(k).map(|v| DbValue::BlockHeader(Box::new(v.clone()))),
            DbKey::UnspentOutput(k) => db.utxos.get(k).map(|v| DbValue::UnspentOutput(Box::new(v.clone()))),
            DbKey::SpentOutput(k) => db.stxos.get(k).map(|v| DbValue::SpentOutput(Box::new(v.clone()))),
            DbKey::TransactionKernel(k) => db
                .kernels
                .get(k)
                .map(|v| DbValue::TransactionKernel(Box::new(v.clone()))),
            DbKey::OrphanBlock(k) => db.orphans.get(k).map(|v| DbValue::OrphanBlock(Box::new(v.clone()))),
        };
        Ok(result)
    }

    fn contains(&self, key: &DbKey) -> Result<bool, ChainStorageError> {
        let db = self
            .db
            .read()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        let result = match key {
            DbKey::Metadata(_) => true,
            DbKey::BlockHeader(k) => db.headers.contains_key(k),
            DbKey::UnspentOutput(k) => db.utxos.contains_key(k),
            DbKey::SpentOutput(k) => db.stxos.contains_key(k),
            DbKey::TransactionKernel(k) => db.kernels.contains_key(k),
            DbKey::OrphanBlock(k) => db.orphans.contains_key(k),
        };

        Ok(result)
    }

    fn fetch_mmr_root(&self, tree: MmrTree) -> Result<Vec<u8>, ChainStorageError> {
        let db = self
            .db
            .read()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        let root = match tree {
            MmrTree::Utxo => db.utxo_mmr.get_merkle_root(),
            MmrTree::Kernel => db.kernel_mmr.get_merkle_root(),
            MmrTree::RangeProof => db.range_proof_mmr.get_merkle_root(),
            MmrTree::Header => db.header_mmr.get_merkle_root(),
        };
        Ok(root)
    }
}

impl<D> Clone for MemoryDatabase<D>
where D: Digest
{
    fn clone(&self) -> Self {
        MemoryDatabase { db: self.db.clone() }
    }
}

impl<D> Default for InnerDatabase<D>
where D: Digest
{
    fn default() -> Self {
        let utxo_mmr = MerkleChangeTracker::<D, _, _>::new(MutableMmr::new(Vec::new()), Vec::new()).unwrap();
        let header_mmr = MerkleChangeTracker::<D, _, _>::new(MutableMmr::new(Vec::new()), Vec::new()).unwrap();
        let kernel_mmr = MerkleChangeTracker::<D, _, _>::new(MutableMmr::new(Vec::new()), Vec::new()).unwrap();
        let range_proof_mmr = MerkleChangeTracker::<D, _, _>::new(MutableMmr::new(Vec::new()), Vec::new()).unwrap();
        InnerDatabase {
            metadata: ChainMetadata::default(),
            headers: HashMap::default(),
            utxos: HashMap::default(),
            stxos: HashMap::default(),
            kernels: HashMap::default(),
            orphans: HashMap::default(),
            utxo_mmr,
            header_mmr,
            kernel_mmr,
            range_proof_mmr,
        }
    }
}

// This is a private helper function. When it is called, we are guaranteed to have a write lock on self.db
fn spend_utxo<D: Digest>(db: &mut RwLockWriteGuard<InnerDatabase<D>>, hash: HashOutput) -> bool {
    match db.utxos.remove(&hash) {
        None => false,
        Some(utxo) => {
            db.stxos.insert(hash, utxo);
            true
        },
    }
}
