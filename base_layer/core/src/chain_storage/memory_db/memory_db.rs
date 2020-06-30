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
    blocks::{blockheader::BlockHash, Block, BlockHeader},
    chain_storage::{
        blockchain_database::BlockchainBackend,
        db_transaction::{DbKey, DbValue, MetadataKey, MetadataValue, MmrTree},
        error::ChainStorageError,
        memory_db::MemDbVec,
        ChainMetadata,
    },
    proof_of_work::{Difficulty, PowAlgorithm, ProofOfWork},
    transactions::{
        transaction::{TransactionInput, TransactionKernel, TransactionOutput},
        types::HashOutput,
    },
};
use croaring::Bitmap;
use digest::Digest;
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard},
};
use tari_crypto::tari_utilities::{epoch_time::EpochTime, hash::Hashable, hex::Hex};
use tari_mmr::{
    functions::{prune_mutable_mmr, PrunedMutableMmr},
    ArrayLike,
    ArrayLikeExt,
    Hash as MmrHash,
    Hash,
    MerkleCheckPoint,
    MmrCache,
    MmrCacheConfig,
};

/// A generic struct for storing node objects in the BlockchainDB that also form part of an MMR. The index field makes
/// reverse lookups (find by hash) possible.
#[derive(Debug)]
struct MerkleNode<T> {
    index: usize,
    value: T,
}

#[derive(Debug)]
struct InnerDatabase<D>
where D: Digest
{
    metadata: HashMap<u32, MetadataValue>,
    headers: HashMap<u64, BlockHeader>,
    block_hashes: HashMap<HashOutput, u64>,
    utxos: HashMap<HashOutput, MerkleNode<TransactionOutput>>,
    stxos: HashMap<HashOutput, MerkleNode<TransactionOutput>>,
    kernels: HashMap<HashOutput, TransactionKernel>,
    orphans: HashMap<HashOutput, Block>,
    // Define MMRs to use both a memory-backed base and a memory-backed pruned MMR
    utxo_mmr: MmrCache<D, MemDbVec<MmrHash>, MemDbVec<MerkleCheckPoint>>,
    utxo_checkpoints: MemDbVec<MerkleCheckPoint>,
    curr_utxo_checkpoint: MerkleCheckPoint,
    kernel_mmr: MmrCache<D, MemDbVec<MmrHash>, MemDbVec<MerkleCheckPoint>>,
    kernel_checkpoints: MemDbVec<MerkleCheckPoint>,
    curr_kernel_checkpoint: MerkleCheckPoint,
    range_proof_mmr: MmrCache<D, MemDbVec<MmrHash>, MemDbVec<MerkleCheckPoint>>,
    range_proof_checkpoints: MemDbVec<MerkleCheckPoint>,
    curr_range_proof_checkpoint: MerkleCheckPoint,
}

/// A memory-backed blockchain database. The data is stored in RAM; and so all data will be lost when the program
/// terminates. Thus this DB is intended for testing purposes. It's also not very efficient since a single Mutex
/// protects the entire database. Again: testing.
#[derive(Default, Debug)]
pub struct MemoryDatabase<D>
where D: Digest
{
    db: Arc<RwLock<InnerDatabase<D>>>,
}

impl<D> MemoryDatabase<D>
where D: Digest + Send + Sync
{
    pub fn new(mmr_cache_config: MmrCacheConfig) -> Self {
        Self {
            db: Arc::new(RwLock::new(InnerDatabase::new(mmr_cache_config))),
        }
    }

    // This will reconstruct the blocks and returns a copy
    fn reconstruct_block(&self, height: u64) -> Result<Block, ChainStorageError> {
        let db = self
            .db
            .read()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        // get header
        let header: BlockHeader = db
            .headers
            .get(&height)
            .ok_or_else(|| ChainStorageError::ValueNotFound(DbKey::BlockHeader(height)))?
            .clone();
        // get the checkpoint
        let kernel_cp = self.fetch_checkpoint(MmrTree::Kernel, height)?;
        let (kernel_hashes, _) = kernel_cp.into_parts();
        let mut kernels = Vec::new();
        // get kernels
        for hash in kernel_hashes {
            let kernel: TransactionKernel = db
                .kernels
                .get(&hash)
                .ok_or_else(|| ChainStorageError::ValueNotFound(DbKey::TransactionKernel(hash)))?
                .clone();
            kernels.push(kernel);
        }
        let utxo_cp = self.fetch_checkpoint(MmrTree::Utxo, height)?;
        let (utxo_hashes, deleted_nodes) = utxo_cp.into_parts();
        // lets get the inputs
        let inputs: Result<Vec<TransactionInput>, ChainStorageError> = deleted_nodes
            .iter()
            .map(|pos| {
                self.fetch_mmr_nodes(MmrTree::Utxo, pos, 1, None).and_then(|node| {
                    let (hash, deleted) = &node[0];
                    assert!(deleted);
                    let val: TransactionOutput = db
                        .stxos
                        .get(hash)
                        .ok_or_else(|| ChainStorageError::ValueNotFound(DbKey::SpentOutput(hash.clone())))?
                        .value
                        .clone();
                    Ok(TransactionInput::from(val))
                })
            })
            .collect();
        let inputs = inputs?;
        // lets get the outputs
        let mut outputs = Vec::with_capacity(utxo_hashes.len());
        let mut spent = Vec::with_capacity(utxo_hashes.len());
        for hash in utxo_hashes.into_iter() {
            // The outputs could come from either the UTXO or STXO set
            let val: Option<&MerkleNode<TransactionOutput>> = db.utxos.get(&hash);
            if val.is_some() {
                outputs.push(val.unwrap().value.clone());
                continue;
            }
            // Check the STXO set
            let val: Option<&MerkleNode<TransactionOutput>> = db.stxos.get(&hash);
            match val {
                Some(v) => {
                    spent.push(v.value.commitment.clone());
                    outputs.push(v.value.clone());
                },
                None => return Err(ChainStorageError::ValueNotFound(DbKey::SpentOutput(hash))),
            }
        }
        let block = header
            .into_builder()
            .add_inputs(inputs)
            .add_outputs(outputs)
            .add_kernels(kernels)
            .build();
        Ok(block)
    }

    fn fetch_mmr_node(
        &self,
        tree: MmrTree,
        pos: u32,
        hist_height: Option<u64>,
    ) -> Result<(Vec<u8>, bool), ChainStorageError>
    {
        let db = self.db_access()?;
        let (hash, deleted) = match tree {
            MmrTree::Kernel => db.kernel_mmr.fetch_mmr_node(pos)?,
            MmrTree::Utxo => {
                let (hash, mut deleted) = db.utxo_mmr.fetch_mmr_node(pos)?;
                // Check if the MMR node was deleted after the historic height then its deletion status should change.
                // TODO: Find a more efficient way to query the historic deletion status of an MMR node.
                if deleted {
                    if let Some(hist_height) = hist_height {
                        let tip_height = db.headers.len().saturating_sub(1) as u64;
                        for height in hist_height + 1..=tip_height {
                            let cp = self.fetch_checkpoint(MmrTree::Utxo, height)?;
                            if cp.nodes_deleted().contains(pos) {
                                deleted = false;
                            }
                        }
                    }
                }
                (hash, deleted)
            },
            MmrTree::RangeProof => db.range_proof_mmr.fetch_mmr_node(pos)?,
        };
        let hash = hash.ok_or_else(|| {
            ChainStorageError::UnexpectedResult(format!("A leaf node hash in the {} MMR tree was not found", tree))
        })?;
        Ok((hash, deleted))
    }

    pub(self) fn db_access(&self) -> Result<RwLockReadGuard<InnerDatabase<D>>, ChainStorageError> {
        self.db
            .read()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))
    }

    // Fetches the chain metadata chain height.
    fn fetch_chain_height(&self) -> Result<Option<u64>, ChainStorageError> {
        Ok(
            if let Some(DbValue::Metadata(MetadataValue::ChainHeight(height))) =
                self.fetch(&DbKey::Metadata(MetadataKey::ChainHeight))?
            {
                height
            } else {
                None
            },
        )
    }

    // Fetches the chain metadata best block hash.
    fn fetch_best_block(&self) -> Result<Option<BlockHash>, ChainStorageError> {
        Ok(
            if let Some(DbValue::Metadata(MetadataValue::BestBlock(best_block))) =
                self.fetch(&DbKey::Metadata(MetadataKey::BestBlock))?
            {
                best_block
            } else {
                None
            },
        )
    }

    // Fetches the chain metadata accumulated work.
    fn fetch_accumulated_work(&self) -> Result<Option<Difficulty>, ChainStorageError> {
        Ok(
            if let Some(DbValue::Metadata(MetadataValue::AccumulatedWork(accumulated_work))) =
                self.fetch(&DbKey::Metadata(MetadataKey::AccumulatedWork))?
            {
                accumulated_work
            } else {
                None
            },
        )
    }

    // Fetches the chain metadata pruning horizon.
    fn fetch_pruning_horizon(&self) -> Result<u64, ChainStorageError> {
        Ok(
            if let Some(DbValue::Metadata(MetadataValue::PruningHorizon(pruning_horizon))) =
                self.fetch(&DbKey::Metadata(MetadataKey::PruningHorizon))?
            {
                pruning_horizon
            } else {
                0
            },
        )
    }

    fn rewind_mmrs(db: &mut RwLockWriteGuard<InnerDatabase<D>>, steps_back: usize) -> Result<(), ChainStorageError> {
        // rewind kernel
        let last_cp = rewind_checkpoints(&mut db.kernel_checkpoints, steps_back)?;
        db.kernel_mmr
            .update()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        db.curr_kernel_checkpoint.reset_to(&last_cp);
        // rewind utxo
        let last_cp = rewind_checkpoints(&mut db.utxo_checkpoints, steps_back)?;
        db.utxo_mmr
            .update()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        db.curr_utxo_checkpoint.reset_to(&last_cp);
        // rewind range proof
        let last_cp = rewind_checkpoints(&mut db.range_proof_checkpoints, steps_back)?;
        db.range_proof_mmr
            .update()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        db.curr_range_proof_checkpoint.reset_to(&last_cp);
        Ok(())
    }

    // fn write(&mut self, tx: DbTransaction) -> Result<(), ChainStorageError> {
    //     if tx.operations.is_empty() {
    //         return Ok(());
    //     }

    //     let mut db = self
    //         .db
    //         .write()
    //         .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
    //     // Not **really** atomic, but..
    //     // Hashmap insertions don't typically fail and b) MemoryDB should not be used for production anyway.
    //     for op in tx.operations.into_iter() {
    //         match op {
    //             WriteOperation::Insert(insert) => match insert {
    //                 DbKeyValuePair::Metadata(k, v) => {
    //                     let key = k as u32;
    //                     db.metadata.insert(key, v);
    //                 },
    //             },
    //             WriteOperation::Delete(delete) => match delete {
    //                 DbKey::Metadata(_) => {}, // no-op
    //                 DbKey::BlockHeader(k) => {
    //                     db.headers.remove(&k).and_then(|v| db.block_hashes.remove(&v.hash()));
    //                 },
    //                 DbKey::BlockHash(hash) => {
    //                     db.block_hashes.remove(&hash).and_then(|i| db.headers.remove(&i));
    //                 },
    //                 DbKey::UnspentOutput(k) => {
    //                     db.utxos.remove(&k);
    //                 },
    //                 DbKey::SpentOutput(k) => {
    //                     db.stxos.remove(&k);
    //                 },
    //                 DbKey::TransactionKernel(k) => {
    //                     db.kernels.remove(&k);
    //                 },
    //                 DbKey::OrphanBlock(k) => {
    //                     db.orphans.remove(&k);
    //                 },
    //             },
    //             WriteOperation::UnSpend(key) => match key {
    //                 DbKey::SpentOutput(hash) => {
    //                     let moved = unspend_stxo(&mut db, hash);
    //                     if !moved {
    //                         return Err(ChainStorageError::UnspendError);
    //                     }
    //                 },
    //                 _ => return Err(ChainStorageError::InvalidOperation("Only STXOs can be unspent".into())),
    //             },
    //             WriteOperation::CreateMmrCheckpoint(tree) => match tree {
    //                 MmrTree::Kernel => {
    //                     let curr_checkpoint = db.curr_kernel_checkpoint.clone();
    //                     db.kernel_checkpoints.push(curr_checkpoint)?;
    //                     db.curr_kernel_checkpoint.reset();

    //                     db.kernel_mmr
    //                         .update()
    //                         .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
    //                 },
    //                 MmrTree::Utxo => {
    //                     let curr_checkpoint = db.curr_utxo_checkpoint.clone();
    //                     db.utxo_checkpoints.push(curr_checkpoint)?;
    //                     db.curr_utxo_checkpoint.reset();

    //                     db.utxo_mmr
    //                         .update()
    //                         .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
    //                 },
    //                 MmrTree::RangeProof => {
    //                     let curr_checkpoint = db.curr_range_proof_checkpoint.clone();
    //                     db.range_proof_checkpoints.push(curr_checkpoint)?;
    //                     db.curr_range_proof_checkpoint.reset();

    //                     db.range_proof_mmr
    //                         .update()
    //                         .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
    //                 },
    //             },
    //             WriteOperation::RewindMmr(tree, steps_back) => match tree {
    //                 MmrTree::Kernel => {
    //                     let last_cp = rewind_checkpoints(&mut db.kernel_checkpoints, steps_back)?;
    //                     db.kernel_mmr
    //                         .update()
    //                         .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
    //                     db.curr_kernel_checkpoint.reset_to(&last_cp);
    //                 },
    //                 MmrTree::Utxo => {
    //                     let last_cp = rewind_checkpoints(&mut db.utxo_checkpoints, steps_back)?;
    //                     db.utxo_mmr
    //                         .update()
    //                         .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
    //                     db.curr_utxo_checkpoint.reset_to(&last_cp);
    //                 },
    //                 MmrTree::RangeProof => {
    //                     let last_cp = rewind_checkpoints(&mut db.range_proof_checkpoints, steps_back)?;
    //                     db.range_proof_mmr
    //                         .update()
    //                         .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
    //                     db.curr_range_proof_checkpoint.reset_to(&last_cp);
    //                 },
    //             },
    //             WriteOperation::MergeMmrCheckpoints(tree, max_cp_count) => match tree {
    //                 MmrTree::Kernel => {
    //                     let (num_cps_merged, _) = merge_checkpoints(&mut db.kernel_checkpoints, max_cp_count)?;
    //                     db.kernel_mmr
    //                         .checkpoints_merged(num_cps_merged)
    //                         .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
    //                 },
    //                 MmrTree::Utxo => {
    //                     let (num_cps_merged, stxo_leaf_indices) =
    //                         merge_checkpoints(&mut db.utxo_checkpoints, max_cp_count)?;
    //                     db.utxo_mmr
    //                         .checkpoints_merged(num_cps_merged)
    //                         .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
    //                     discard_stxos(&mut db, stxo_leaf_indices)?;
    //                 },
    //                 MmrTree::RangeProof => {
    //                     let (num_cps_merged, _) = merge_checkpoints(&mut db.range_proof_checkpoints, max_cp_count)?;
    //                     db.range_proof_mmr
    //                         .checkpoints_merged(num_cps_merged)
    //                         .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
    //                 },
    //             },
    //         }
    //     }
    //     Ok(())
    // }

    fn fetch_checkpoint(&self, tree: MmrTree, height: u64) -> Result<MerkleCheckPoint, ChainStorageError> {
        let db = self.db_access()?;
        let tip_height = db.headers.len().saturating_sub(1) as u64;
        let pruned_mode = self.fetch_metadata()?.is_pruned_node();
        match tree {
            MmrTree::Kernel => fetch_checkpoint(&db.kernel_checkpoints, pruned_mode, tip_height, height),
            MmrTree::Utxo => fetch_checkpoint(&db.utxo_checkpoints, pruned_mode, tip_height, height),
            MmrTree::RangeProof => fetch_checkpoint(&db.range_proof_checkpoints, pruned_mode, tip_height, height),
        }?
        .ok_or_else(|| ChainStorageError::OutOfRange)
    }
}

impl<D> BlockchainBackend for MemoryDatabase<D>
where D: Digest + Send + Sync
{
    fn add_orphan_block(&mut self, block: Block) -> Result<(), ChainStorageError> {
        let mut db = self
            .db
            .write()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        let hash = block.hash();
        db.orphans.insert(hash, block);
        Ok(())
    }

    fn accept_block(&mut self, block_hash: HashOutput) -> Result<(), ChainStorageError> {
        let mut db = self
            .db
            .write()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        // lets get the block
        let block = db
            .orphans
            .get(&block_hash)
            .ok_or_else(|| ChainStorageError::ValueNotFound(DbKey::OrphanBlock(block_hash)))?;

        let (header, inputs, outputs, kernels) = block.clone().dissolve();
        // insert headers
        let k = header.height;
        if db.headers.contains_key(&k) {
            return Err(ChainStorageError::InvalidOperation(format!(
                "Duplicate `BlockHeader` key `{}`",
                k
            )));
        };
        db.block_hashes.insert(header.hash(), k);
        let accumulated_difficulty = Some(
            ProofOfWork::new_from_difficulty(&header.pow, ProofOfWork::achieved_difficulty(&header))
                .total_accumulated_difficulty(),
        );
        let height = Some(header.height);
        let hash = Some(header.hash());
        db.headers.insert(k, header);

        // lets add the kernels
        for kernel in kernels {
            let k = kernel.hash();
            if db.kernels.contains_key(&k) {
                return Err(ChainStorageError::InvalidOperation(format!(
                    "Duplicate `TransactionKernel` key `{}`",
                    k.to_hex()
                )));
            }
            db.curr_kernel_checkpoint.push_addition(k.clone());
            db.kernels.insert(k, kernel);
        }
        // lets add the utxos
        for utxo in outputs {
            let k = utxo.hash();
            if db.utxos.contains_key(&k) {
                return Err(ChainStorageError::InvalidOperation(format!(
                    "Duplicate `UnspentOutput` key `{}`",
                    k.to_hex()
                )));
            }
            db.curr_utxo_checkpoint.push_addition(k.clone());
            db.curr_range_proof_checkpoint.push_addition(utxo.proof().hash());
            let index = db.curr_range_proof_checkpoint.accumulated_nodes_added_count() - 1;
            let v = MerkleNode {
                index: index as usize,
                value: utxo,
            };
            db.utxos.insert(k, v);
        }

        // lets spend the utxo's
        for utxo in inputs {
            let k = utxo.hash();
            if !spend_utxo(&mut db, k) {
                return Err(ChainStorageError::UnspendableInput);
            }
        }
        // lets update the metadata
        db.metadata
            .insert(MetadataKey::ChainHeight as u32, MetadataValue::ChainHeight(height));
        db.metadata
            .insert(MetadataKey::BestBlock as u32, MetadataValue::BestBlock(hash));
        db.metadata.insert(
            MetadataKey::AccumulatedWork as u32,
            MetadataValue::AccumulatedWork(accumulated_difficulty),
        );

        // lets update the checkpoints of and the mmrs
        // kernels
        let curr_checkpoint = db.curr_kernel_checkpoint.clone();
        db.kernel_checkpoints.push(curr_checkpoint)?;
        db.curr_kernel_checkpoint.reset();

        db.kernel_mmr
            .update()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        // utxos
        let curr_checkpoint = db.curr_utxo_checkpoint.clone();
        db.utxo_checkpoints.push(curr_checkpoint)?;
        db.curr_utxo_checkpoint.reset();

        db.utxo_mmr
            .update()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;

        // range proofs
        let curr_checkpoint = db.curr_range_proof_checkpoint.clone();
        db.range_proof_checkpoints.push(curr_checkpoint)?;
        db.curr_range_proof_checkpoint.reset();

        db.range_proof_mmr
            .update()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;

        Ok(())
    }

    fn force_meta_data(&mut self, metadata: ChainMetadata) -> Result<(), ChainStorageError> {
        let mut db = self
            .db
            .write()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        db.metadata.insert(
            MetadataKey::ChainHeight as u32,
            MetadataValue::ChainHeight(metadata.height_of_longest_chain),
        );
        db.metadata.insert(
            MetadataKey::BestBlock as u32,
            MetadataValue::BestBlock(metadata.best_block),
        );
        db.metadata.insert(
            MetadataKey::AccumulatedWork as u32,
            MetadataValue::AccumulatedWork(metadata.accumulated_difficulty),
        );
        db.metadata.insert(
            MetadataKey::PruningHorizon as u32,
            MetadataValue::PruningHorizon(metadata.pruning_horizon),
        );
        Ok(())
    }

    // rewinds the database to the specified height. It will move every block that was rewound to the orphan pool
    fn rewind_to_height(&mut self, height: u64) -> Result<Vec<BlockHeader>, ChainStorageError> {
        let mut headers: Vec<BlockHeader> = Vec::new();
        let mut db = self
            .db
            .write()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        let chain_height = self.fetch_chain_height()?.unwrap_or(0);
        let steps_back = (chain_height - height) as usize;
        let mut removed_blocks = Vec::new();
        for rewind_height in ((height + 1)..=chain_height).rev() {
            // Reconstruct block at height and add to orphan block pool

            let orphaned_block = self.reconstruct_block(rewind_height)?; // fetch_block(&**db, rewind_height)?.block().clone();
                                                                         // 1st we add the removed block back to the orphan pool.
            let hash = orphaned_block.hash();
            db.orphans.insert(hash.clone(), orphaned_block);
            removed_blocks.push(hash);

            // Now we need to remove that block
            // Remove Header and block hash
            db.headers.remove(&rewind_height).and_then(|v| {
                headers.push(v.clone());
                db.block_hashes.remove(&v.hash())
            });

            // lets get the checkpoint
            let hashes = self
                .fetch_checkpoint(MmrTree::Kernel, rewind_height)?
                .nodes_added()
                .clone();
            for hash in hashes {
                db.kernels.remove(&hash);
            }
            // Remove UTXOs and move STXOs back to UTXO set
            let (nodes_added, nodes_deleted) = self.fetch_checkpoint(MmrTree::Utxo, rewind_height)?.into_parts();
            for hash in nodes_added {
                db.utxos.remove(&hash);
            }
            // lets unspend utxos
            for pos in nodes_deleted.iter() {
                self.fetch_mmr_nodes(MmrTree::Utxo, pos, 1, None).and_then(|nodes| {
                    let (stxo_hash, deleted) = &nodes[0];
                    assert!(deleted);

                    unspend_stxo(&mut db, stxo_hash.clone());
                    Ok(())
                })?;
            }
        }
        MemoryDatabase::rewind_mmrs(&mut db, steps_back)?;
        Ok(headers)
    }

    /// This is used when synchronising. Adds in the list of headers provided to the main chain
    fn add_block_headers(&mut self, headers: Vec<BlockHeader>) -> Result<(), ChainStorageError> {
        let mut db = self
            .db
            .write()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        for header in headers {
            let k = header.height;
            if db.headers.contains_key(&k) {
                return Err(ChainStorageError::InvalidOperation(format!(
                    "Duplicate `BlockHeader` key `{}`",
                    k
                )));
            };
            db.block_hashes.insert(header.hash(), k);
            db.headers.insert(k, header);
        }
        Ok(())
    }

    /// This is used when synchronising. Adds in the list of kernels provided to the main chain
    fn add_kernels(&mut self, kernels: Vec<TransactionKernel>) -> Result<(), ChainStorageError> {
        let mut db = self
            .db
            .write()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        for kernel in kernels {
            let k = kernel.hash();
            if db.kernels.contains_key(&k) {
                return Err(ChainStorageError::InvalidOperation(format!(
                    "Duplicate `TransactionKernel` key `{}`",
                    k.to_hex()
                )));
            }
            db.curr_kernel_checkpoint.push_addition(k.clone());
            db.kernels.insert(k, kernel);
        }
        Ok(())
    }

    /// This is used when synchronising. Adds in the list of utxos provided to the main chain
    fn add_utxos(&mut self, utxos: Vec<TransactionOutput>) -> Result<(), ChainStorageError> {
        let mut db = self
            .db
            .write()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        for utxo in utxos {
            let k = utxo.hash();
            if db.utxos.contains_key(&k) {
                return Err(ChainStorageError::InvalidOperation(format!(
                    "Duplicate `UnspentOutput` key `{}`",
                    k.to_hex()
                )));
            }
            db.curr_utxo_checkpoint.push_addition(k.clone());
            db.curr_range_proof_checkpoint.push_addition(utxo.proof().hash());
            let index = db.curr_range_proof_checkpoint.accumulated_nodes_added_count() - 1;
            let v = MerkleNode {
                index: index as usize,
                value: utxo,
            };
            db.utxos.insert(k, v);
        }
        Ok(())
    }

    fn remove_orphan_blocks(&mut self, block_hashes: Vec<BlockHash>) -> Result<bool, ChainStorageError> {
        let mut db = self
            .db
            .write()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        let mut results = true;
        for hash in block_hashes {
            if !db.orphans.contains_key(&hash) {
                results = false;
            }
            db.orphans.remove(&hash);
        }
        Ok(results)
    }

    fn fetch(&self, key: &DbKey) -> Result<Option<DbValue>, ChainStorageError> {
        let db = self.db_access()?;
        let result = match key {
            DbKey::Metadata(k) => db
                .metadata
                .get(&(k.clone() as u32))
                .map(|v| DbValue::Metadata(v.clone())),
            DbKey::BlockHeader(k) => db.headers.get(k).map(|v| DbValue::BlockHeader(Box::new(v.clone()))),
            DbKey::BlockHash(hash) => db
                .block_hashes
                .get(hash)
                .and_then(|i| db.headers.get(i))
                .map(|v| DbValue::BlockHash(Box::new(v.clone()))),
            DbKey::UnspentOutput(k) => db
                .utxos
                .get(k)
                .map(|v| DbValue::UnspentOutput(Box::new(v.value.clone()))),
            DbKey::SpentOutput(k) => db.stxos.get(k).map(|v| DbValue::SpentOutput(Box::new(v.value.clone()))),
            DbKey::TransactionKernel(k) => db
                .kernels
                .get(k)
                .map(|v| DbValue::TransactionKernel(Box::new(v.clone()))),
            DbKey::OrphanBlock(k) => db.orphans.get(k).map(|v| DbValue::OrphanBlock(Box::new(v.clone()))),
            DbKey::Block(k) => {
                let block = self.reconstruct_block(*k)?;
                Some(DbValue::OrphanBlock(Box::new(block)))
            },
        };
        Ok(result)
    }

    fn contains(&self, key: &DbKey) -> Result<bool, ChainStorageError> {
        let db = self.db_access()?;
        let result = match key {
            DbKey::Metadata(_) => true,
            DbKey::BlockHeader(k) => db.headers.contains_key(k),
            DbKey::BlockHash(h) => db.block_hashes.contains_key(h),
            DbKey::UnspentOutput(k) => db.utxos.contains_key(k),
            DbKey::SpentOutput(k) => db.stxos.contains_key(k),
            DbKey::TransactionKernel(k) => db.kernels.contains_key(k),
            DbKey::OrphanBlock(k) => db.orphans.contains_key(k),
            DbKey::Block(k) => db.headers.contains_key(k),
        };
        Ok(result)
    }

    // fn fetch_mmr_root(&self, tree: MmrTree) -> Result<Vec<u8>, ChainStorageError> {
    //     let db = self.db_access()?;
    //     let pruned_mmr = get_pruned_mmr(&db, &tree)?;
    //     Ok(pruned_mmr.get_merkle_root()?)
    // }

    // fn fetch_mmr_only_root(&self, tree: MmrTree) -> Result<Vec<u8>, ChainStorageError> {
    //     let db = self.db_access()?;
    //     let pruned_mmr = get_pruned_mmr(&db, &tree)?;
    //     Ok(pruned_mmr.get_mmr_only_root()?)
    // }

    fn calculate_mmr_root(
        &self,
        tree: MmrTree,
        additions: Vec<HashOutput>,
        deletions: Vec<HashOutput>,
    ) -> Result<Vec<u8>, ChainStorageError>
    {
        let db = self.db_access()?;
        let mut pruned_mmr = get_pruned_mmr(&db, &tree)?;
        for hash in additions {
            pruned_mmr.push(&hash)?;
        }
        if tree == MmrTree::Utxo {
            deletions.iter().for_each(|hash| {
                if let Some(node) = db.utxos.get(hash) {
                    pruned_mmr.delete_and_compress(node.index as u32, false);
                }
            });
            pruned_mmr.compress();
        }
        Ok(pruned_mmr.get_merkle_root()?)
    }

    /// Returns an MMR proof extracted from the full Merkle mountain range without trimming the MMR using the roaring
    /// bitmap
    // fn fetch_mmr_proof(&self, tree: MmrTree, leaf_pos: usize) -> Result<MerkleProof, ChainStorageError> {
    //     let db = self.db_access()?;
    //     let pruned_mmr = get_pruned_mmr(&db, &tree)?;
    //     let proof = match tree {
    //         MmrTree::Utxo => MerkleProof::for_leaf_node(&pruned_mmr.mmr(), leaf_pos)?,
    //         MmrTree::Kernel => MerkleProof::for_leaf_node(&pruned_mmr.mmr(), leaf_pos)?,
    //         MmrTree::RangeProof => MerkleProof::for_leaf_node(&pruned_mmr.mmr(), leaf_pos)?,
    //     };
    //     Ok(proof)
    // }

    // fn fetch_checkpoint(&self, tree: MmrTree, height: u64) -> Result<MerkleCheckPoint, ChainStorageError> {
    //     let db = self.db_access()?;
    //     let tip_height = db.headers.len().saturating_sub(1) as u64;
    //     let pruned_mode = self.fetch_metadata()?.is_pruned_node();
    //     match tree {
    //         MmrTree::Kernel => fetch_checkpoint(&db.kernel_checkpoints, pruned_mode, tip_height, height),
    //         MmrTree::Utxo => fetch_checkpoint(&db.utxo_checkpoints, pruned_mode, tip_height, height),
    //         MmrTree::RangeProof => fetch_checkpoint(&db.range_proof_checkpoints, pruned_mode, tip_height, height),
    //     }?
    //     .ok_or_else(|| ChainStorageError::OutOfRange)
    // }

    fn fetch_mmr_node_count(&self, tree: MmrTree, height: u64) -> Result<u32, ChainStorageError> {
        let db = self.db_access()?;
        let tip_height = db.headers.len().saturating_sub(1) as u64;
        match tree {
            MmrTree::Kernel => fetch_mmr_nodes_added_count(&db.kernel_checkpoints, tip_height, height),
            MmrTree::Utxo => fetch_mmr_nodes_added_count(&db.utxo_checkpoints, tip_height, height),
            MmrTree::RangeProof => fetch_mmr_nodes_added_count(&db.range_proof_checkpoints, tip_height, height),
        }
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
            leaf_nodes.push(self.fetch_mmr_node(tree.clone(), pos, hist_height)?);
        }
        Ok(leaf_nodes)
    }

    fn insert_mmr_node(&mut self, tree: MmrTree, hash: Hash, deleted: bool) -> Result<(), ChainStorageError> {
        let mut db = self
            .db
            .write()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        match tree {
            MmrTree::Kernel => db.curr_kernel_checkpoint.push_addition(hash),
            MmrTree::Utxo => {
                db.curr_utxo_checkpoint.push_addition(hash);
                if deleted {
                    let leaf_index = db
                        .curr_utxo_checkpoint
                        .accumulated_nodes_added_count()
                        .saturating_sub(1);
                    db.curr_utxo_checkpoint.push_deletion(leaf_index);
                }
            },
            MmrTree::RangeProof => db.curr_range_proof_checkpoint.push_addition(hash),
        };
        Ok(())
    }

    fn delete_mmr_node(&mut self, tree: MmrTree, hash: &Hash) -> Result<(), ChainStorageError> {
        let mut db = self
            .db
            .write()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        match tree {
            MmrTree::Kernel | MmrTree::RangeProof => {},
            MmrTree::Utxo => {
                if let Some(leaf_index) = db.utxo_mmr.find_leaf_index(&hash)? {
                    db.curr_utxo_checkpoint.push_deletion(leaf_index);
                }
            },
        };
        Ok(())
    }

    /// Iterate over all the stored orphan blocks and execute the function `f` for each block.
    // fn for_each_orphan<F>(&self, mut f: F) -> Result<(), ChainStorageError>
    // where F: FnMut(Result<(HashOutput, Block), ChainStorageError>) {
    //     let db = self.db_access()?;
    //     for (key, val) in db.orphans.iter() {
    //         f(Ok((key.clone(), val.clone())));
    //     }
    //     Ok(())
    // }

    fn fetch_parent_orphan_headers(
        &self,
        hash: HashOutput,
        height: u64,
    ) -> Result<Vec<BlockHeader>, ChainStorageError>
    {
        let db = self.db_access()?;
        let mut headers = Vec::new();

        for (_, block) in db.orphans.iter() {
            if (block.header.prev_hash == hash) && (block.header.height == height + 1) {
                // we found a match, let save to call later
                headers.push(block.header.clone());
            }
        }
        Ok(headers)
    }

    /// Returns a list of all orphan block headers
    fn fetch_all_orphan_headers(&self) -> Result<Vec<BlockHeader>, ChainStorageError> {
        let db = self.db_access()?;
        let mut headers = Vec::new();
        for (_, val) in db.orphans.iter() {
            headers.push(val.header.clone());
        }
        Ok(headers)
    }

    fn fetch_mmr_leaf_index(&self, tree: MmrTree, hash: &Hash) -> Result<Option<u32>, ChainStorageError> {
        let db = self.db_access()?;
        Ok(match tree {
            MmrTree::Kernel => db.kernel_mmr.find_leaf_index(hash)?,
            MmrTree::Utxo => db.utxo_mmr.find_leaf_index(hash)?,
            MmrTree::RangeProof => db.range_proof_mmr.find_leaf_index(hash)?,
        })
    }

    /// Returns the number of blocks in the block orphan pool.
    fn get_orphan_count(&self) -> Result<usize, ChainStorageError> {
        let db = self.db_access()?;
        Ok(db.orphans.len())
    }

    /// Iterate over all the stored transaction kernels and execute the function `f` for each kernel.
    // fn for_each_kernel<F>(&self, mut f: F) -> Result<(), ChainStorageError>
    // where F: FnMut(Result<(HashOutput, TransactionKernel), ChainStorageError>) {
    //     let db = self.db_access()?;
    //     for (key, val) in db.kernels.iter() {
    //         f(Ok((key.clone(), val.clone())));
    //     }
    //     Ok(())
    // }

    // /// Iterate over all the stored block headers and execute the function `f` for each header.
    // fn for_each_header<F>(&self, mut f: F) -> Result<(), ChainStorageError>
    // where F: FnMut(Result<(u64, BlockHeader), ChainStorageError>) {
    //     let db = self.db_access()?;
    //     for (key, val) in db.headers.iter() {
    //         f(Ok((*key, val.clone())));
    //     }
    //     Ok(())
    // }

    /// Iterate over all the stored unspent transaction outputs and execute the function `f` for each UTXO.
    // fn for_each_utxo<F>(&self, mut f: F) -> Result<(), ChainStorageError>
    // where F: FnMut(Result<(HashOutput, TransactionOutput), ChainStorageError>) {
    //     let db = self.db_access()?;
    //     for (key, val) in db.utxos.iter() {
    //         f(Ok((key.clone(), val.value.clone())));
    //     }
    //     Ok(())
    // }

    /// Finds and returns the last stored header.
    fn fetch_last_header(&self) -> Result<Option<BlockHeader>, ChainStorageError> {
        let db = self.db_access()?;
        let header_count = db.headers.len() as u64;
        if header_count >= 1 {
            let k = header_count - 1;
            Ok(db.headers.get(&k).cloned())
        } else {
            Ok(None)
        }
    }

    /// Returns the metadata of the chain.
    fn fetch_metadata(&self) -> Result<ChainMetadata, ChainStorageError> {
        Ok(ChainMetadata {
            height_of_longest_chain: self.fetch_chain_height()?,
            best_block: self.fetch_best_block()?,
            pruning_horizon: self.fetch_pruning_horizon()?,
            accumulated_difficulty: self.fetch_accumulated_work()?,
        })
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
        let tip_height = self.fetch_chain_height()?.ok_or_else(|| {
            ChainStorageError::InvalidQuery("Cannot retrieve chain height. Blockchain DB is empty".into())
        })?;
        if height <= tip_height {
            let db = self.db_access()?;
            for height in (0..=height).rev() {
                let header = db
                    .headers
                    .get(&height)
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

impl<D> Clone for MemoryDatabase<D>
where D: Digest
{
    fn clone(&self) -> Self {
        MemoryDatabase { db: self.db.clone() }
    }
}

impl<D: Digest> InnerDatabase<D> {
    pub fn new(mmr_cache_config: MmrCacheConfig) -> Self {
        let utxo_checkpoints = MemDbVec::new();
        let utxo_mmr = MmrCache::<D, _, _>::new(MemDbVec::new(), utxo_checkpoints.clone(), mmr_cache_config).unwrap();
        let kernel_checkpoints = MemDbVec::new();
        let kernel_mmr =
            MmrCache::<D, _, _>::new(MemDbVec::new(), kernel_checkpoints.clone(), mmr_cache_config).unwrap();
        let range_proof_checkpoints = MemDbVec::new();
        let range_proof_mmr =
            MmrCache::<D, _, _>::new(MemDbVec::new(), range_proof_checkpoints.clone(), mmr_cache_config).unwrap();
        Self {
            metadata: HashMap::default(),
            headers: HashMap::default(),
            block_hashes: HashMap::default(),
            utxos: HashMap::default(),
            stxos: HashMap::default(),
            kernels: HashMap::default(),
            orphans: HashMap::default(),
            utxo_mmr,
            curr_utxo_checkpoint: MerkleCheckPoint::new(Vec::new(), Bitmap::create(), 0),
            utxo_checkpoints,
            kernel_mmr,
            curr_kernel_checkpoint: MerkleCheckPoint::new(Vec::new(), Bitmap::create(), 0),
            kernel_checkpoints,
            range_proof_mmr,
            curr_range_proof_checkpoint: MerkleCheckPoint::new(Vec::new(), Bitmap::create(), 0),
            range_proof_checkpoints,
        }
    }
}

impl<D> Default for InnerDatabase<D>
where D: Digest
{
    fn default() -> Self {
        Self::new(MmrCacheConfig::default())
    }
}

// This is a private helper function. When it is called, we are guaranteed to have a write lock on self.db
fn spend_utxo<D: Digest>(db: &mut RwLockWriteGuard<InnerDatabase<D>>, hash: HashOutput) -> bool {
    match db.utxos.remove(&hash) {
        None => false,
        Some(utxo) => {
            db.curr_utxo_checkpoint.push_deletion(utxo.index as u32);
            db.stxos.insert(hash, utxo);
            true
        },
    }
}

// This is a private helper function. When it is called, we are guaranteed to have a write lock on self.db. Unspend_stxo
// is only called for rewind operations and doesn't have to re-insert the utxo entry into the utxo_mmr as the MMR will
// be rolled back.
fn unspend_stxo<D: Digest>(db: &mut RwLockWriteGuard<InnerDatabase<D>>, hash: HashOutput) -> bool {
    match db.stxos.remove(&hash) {
        None => false,
        Some(stxo) => {
            db.utxos.insert(hash, stxo);
            true
        },
    }
}

// Construct a pruned mmr for the specified MMR tree based on the checkpoint state and new additions and deletions.
fn get_pruned_mmr<D: Digest>(
    db: &RwLockReadGuard<InnerDatabase<D>>,
    tree: &MmrTree,
) -> Result<PrunedMutableMmr<D>, ChainStorageError>
{
    Ok(match tree {
        MmrTree::Utxo => {
            let mut pruned_mmr = prune_mutable_mmr(&db.utxo_mmr)?;
            for hash in db.curr_utxo_checkpoint.nodes_added() {
                pruned_mmr.push(&hash)?;
            }
            db.curr_utxo_checkpoint
                .nodes_deleted()
                .to_vec()
                .iter()
                .for_each(|index| {
                    pruned_mmr.delete_and_compress(*index, false);
                });
            pruned_mmr.compress();
            pruned_mmr
        },
        MmrTree::Kernel => {
            let mut pruned_mmr = prune_mutable_mmr(&db.kernel_mmr)?;
            for hash in db.curr_kernel_checkpoint.nodes_added() {
                pruned_mmr.push(&hash)?;
            }
            pruned_mmr
        },
        MmrTree::RangeProof => {
            let mut pruned_mmr = prune_mutable_mmr(&db.range_proof_mmr)?;
            for hash in db.curr_range_proof_checkpoint.nodes_added() {
                pruned_mmr.push(&hash)?;
            }
            pruned_mmr
        },
    })
}

// Calculated the new checkpoint count after rewinding a set number of steps back.
fn rewind_checkpoint_index(cp_count: usize, steps_back: usize) -> usize {
    if cp_count > steps_back {
        cp_count - steps_back
    } else {
        1
    }
}

// Retrieves the checkpoint corresponding to the provided height, if the checkpoint is part of the horizon state then a
// BeyondPruningHorizon error will be produced.
fn fetch_checkpoint(
    checkpoints: &MemDbVec<MerkleCheckPoint>,
    pruned_mode: bool,
    tip_height: u64,
    height: u64,
) -> Result<Option<MerkleCheckPoint>, ChainStorageError>
{
    let last_cp_index = checkpoints.len()?.saturating_sub(1);
    let height_offset = tip_height
        .checked_sub(height)
        .ok_or_else(|| ChainStorageError::OutOfRange)?;
    let index = last_cp_index
        .checked_sub(height_offset as usize)
        .ok_or_else(|| ChainStorageError::BeyondPruningHorizon)?;
    if pruned_mode && index == 0 {
        // In pruned mode the first checkpoint is an accumulation of all checkpoints from the genesis block to horizon
        // block height.
        return Err(ChainStorageError::BeyondPruningHorizon);
    }
    checkpoints.get(index as usize)
}

// Calculate the total leaf node count upto a specified height.
fn fetch_mmr_nodes_added_count(
    checkpoints: &MemDbVec<MerkleCheckPoint>,
    tip_height: u64,
    height: u64,
) -> Result<u32, ChainStorageError>
{
    let cp_count = checkpoints
        .len()
        .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
    Ok(match cp_count.checked_sub(1) {
        Some(last_index) => {
            let index = last_index.saturating_sub(tip_height.saturating_sub(height) as usize);
            checkpoints
                .get(index)
                .map_err(|e| ChainStorageError::AccessError(format!("Checkpoint error: {}", e.to_string())))?
                .map(|cp| cp.accumulated_nodes_added_count())
                .unwrap_or(0)
        },
        None => 0,
    })
}

// Rewinds checkpoints by `steps_back` elements and returns the last checkpoint.
fn rewind_checkpoints(
    checkpoints: &mut MemDbVec<MerkleCheckPoint>,
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

// Attempt to merge the set of oldest checkpoints into the horizon state and return the number of checkpoints that have
// been merged.
fn merge_checkpoints(
    checkpoints: &mut MemDbVec<MerkleCheckPoint>,
    max_cp_count: usize,
) -> Result<(usize, Vec<u32>), ChainStorageError>
{
    let cp_count = checkpoints
        .len()
        .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
    let mut stxo_leaf_indices = Vec::<u32>::new();
    if let Some(num_cps_merged) = (cp_count + 1).checked_sub(max_cp_count) {
        if let Some(mut merged_cp) = checkpoints
            .get(0)
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
        {
            for index in 1..num_cps_merged {
                if let Some(cp) = checkpoints
                    .get(index)
                    .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
                {
                    stxo_leaf_indices.append(&mut cp.nodes_deleted().to_vec());
                    merged_cp.append(cp);
                }
            }
            checkpoints
                .shift(num_cps_merged)
                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
            checkpoints
                .push_front(merged_cp)
                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
            return Ok((num_cps_merged, stxo_leaf_indices));
        }
    }
    Ok((0, stxo_leaf_indices))
}

// Discard the STXOs of the checkpoints that have been merged into the horizon state and return the number of removed
// STXOs.
fn discard_stxos<D: Digest>(
    db: &mut RwLockWriteGuard<InnerDatabase<D>>,
    leaf_indices: Vec<u32>,
) -> Result<usize, ChainStorageError>
{
    let mut num_removed = 0;
    for leaf_index in leaf_indices {
        if let (Some(hash), _) = db.utxo_mmr.fetch_mmr_node(leaf_index)? {
            if db.stxos.remove(&hash).is_some() {
                num_removed += 1;
            }
        }
    }
    Ok(num_removed)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::transactions::{
        helpers::create_utxo,
        tari_amount::MicroTari,
        types::{CryptoFactories, HashDigest},
    };

    #[test]
    fn memory_spend_utxo_and_unspend_stxo() {
        let mut db = MemoryDatabase::<HashDigest>::default();

        let factories = CryptoFactories::default();
        let (utxo1, _) = create_utxo(MicroTari(10_000), &factories, None);
        let (utxo2, _) = create_utxo(MicroTari(15_000), &factories, None);
        let hash1 = utxo1.hash();
        let hash2 = utxo2.hash();

        assert!(db.add_utxos(vec![utxo1.clone(), utxo2.clone()]).is_ok());

        {
            let mut db_access = db.db.write().unwrap();
            assert!(spend_utxo(&mut db_access, hash1.clone()));
        }
        assert_eq!(db.contains(&DbKey::UnspentOutput(hash1.clone())), Ok(false));
        assert_eq!(db.contains(&DbKey::UnspentOutput(hash2.clone())), Ok(true));
        assert_eq!(db.contains(&DbKey::SpentOutput(hash1.clone())), Ok(true));
        assert_eq!(db.contains(&DbKey::SpentOutput(hash2.clone())), Ok(false));

        {
            let mut db_access = db.db.write().unwrap();
            assert!(spend_utxo(&mut db_access, hash2.clone()));
            assert!(unspend_stxo(&mut db_access, hash1.clone()));
        }
        assert_eq!(db.contains(&DbKey::UnspentOutput(hash1.clone())), Ok(true));
        assert_eq!(db.contains(&DbKey::UnspentOutput(hash2.clone())), Ok(false));
        assert_eq!(db.contains(&DbKey::SpentOutput(hash1.clone())), Ok(false));
        assert_eq!(db.contains(&DbKey::SpentOutput(hash2.clone())), Ok(true));

        if let Some(DbValue::UnspentOutput(retrieved_utxo)) = db.fetch(&DbKey::UnspentOutput(hash1.clone())).unwrap() {
            assert_eq!(*retrieved_utxo, utxo1);
        } else {
            assert!(false);
        }
        if let Some(DbValue::SpentOutput(retrieved_utxo)) = db.fetch(&DbKey::SpentOutput(hash2.clone())).unwrap() {
            assert_eq!(*retrieved_utxo, utxo2);
        } else {
            assert!(false);
        }
    }
}
