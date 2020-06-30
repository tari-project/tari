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
        db_transaction::{DbKey, DbValue, MetadataKey, MetadataValue, MmrTree},
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
    proof_of_work::{Difficulty, PowAlgorithm, ProofOfWork},
    transactions::{
        transaction::{TransactionInput, TransactionKernel, TransactionOutput},
        types::{HashDigest, HashOutput},
    },
};
use croaring::Bitmap;
use digest::Digest;
use lmdb_zero::{Database, Environment, WriteTransaction};
use log::*;
use std::{collections::VecDeque, fmt::Display, path::Path, sync::Arc};
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
    // fn commit_mmrs(&mut self, tx: DbTransaction) -> Result<(), ChainStorageError> {
    //     for op in tx.operations.into_iter() {
    //         match op {
    //             WriteOperation::RewindMmr(tree, steps_back) => match tree {},
    //             WriteOperation::CreateMmrCheckpoint(tree) => match tree {
    //                 MmrTree::Kernel => {
    //                     let curr_checkpoint = self.curr_kernel_checkpoint.clone();
    //                     self.kernel_checkpoints
    //                         .push(curr_checkpoint)
    //                         .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
    //                     self.curr_kernel_checkpoint.reset();

    //                     self.kernel_mmr
    //                         .update()
    //                         .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
    //                 },
    //                 MmrTree::Utxo => {
    //                     let curr_checkpoint = self.curr_utxo_checkpoint.clone();
    //                     self.utxo_checkpoints
    //                         .push(curr_checkpoint)
    //                         .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
    //                     self.curr_utxo_checkpoint.reset();

    //                     self.utxo_mmr
    //                         .update()
    //                         .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
    //                 },
    //                 MmrTree::RangeProof => {
    //                     let curr_checkpoint = self.curr_range_proof_checkpoint.clone();
    //                     self.range_proof_checkpoints
    //                         .push(curr_checkpoint)
    //                         .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
    //                     self.curr_range_proof_checkpoint.reset();

    //                     self.range_proof_mmr
    //                         .update()
    //                         .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
    //                 },
    //             },
    //             WriteOperation::MergeMmrCheckpoints(tree, max_cp_count) => match tree {
    //                 MmrTree::Kernel => {
    //                     let (num_cps_merged, _) = merge_checkpoints(&mut self.kernel_checkpoints, max_cp_count)?;
    //                     self.kernel_mmr
    //                         .checkpoints_merged(num_cps_merged)
    //                         .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
    //                     trace!(target: LOG_TARGET, "Merged {} kernel checkpoints", num_cps_merged);
    //                 },
    //                 MmrTree::Utxo => {
    //                     let (num_cps_merged, stxo_leaf_indices) =
    //                         merge_checkpoints(&mut self.utxo_checkpoints, max_cp_count)?;
    //                     self.utxo_mmr
    //                         .checkpoints_merged(num_cps_merged)
    //                         .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
    //                     trace!(target: LOG_TARGET, "Merged {} utxo checkpoints", num_cps_merged);
    //                     let num_stxo_leaf_indices = stxo_leaf_indices.len();
    //                     let num_stxos_discarded = self.discard_stxos(stxo_leaf_indices)?;
    //                     trace!(
    //                         target: LOG_TARGET,
    //                         "Discarded {} of {} STXOs",
    //                         num_stxo_leaf_indices,
    //                         num_stxos_discarded
    //                     );
    //                 },
    //                 MmrTree::RangeProof => {
    //                     let (num_cps_merged, _) = merge_checkpoints(&mut self.range_proof_checkpoints,
    // max_cp_count)?;                     self.range_proof_mmr
    //                         .checkpoints_merged(num_cps_merged)
    //                         .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
    //                     trace!(target: LOG_TARGET, "Merged {} range proof checkpoints", num_cps_merged);
    //                 },
    //             },
    //             _ => {},
    //         }
    //     }
    //     Ok(())
    // }

    // This will reconstruct the blocks and returns a copy
    fn reconstruct_block(&self, height: u64) -> Result<Block, ChainStorageError> {
        // get header
        let header: BlockHeader = lmdb_get(&self.env, &self.headers_db, &height)?
            .ok_or_else(|| ChainStorageError::ValueNotFound(DbKey::BlockHeader(height)))?;
        // get the checkpoint
        let kernel_cp = self.fetch_checkpoint(MmrTree::Kernel, height)?;
        let (kernel_hashes, _) = kernel_cp.into_parts();
        let mut kernels = Vec::new();
        // get kernels
        for hash in kernel_hashes {
            let kernel: TransactionKernel = lmdb_get(&self.env, &self.kernels_db, &hash)?
                .ok_or_else(|| ChainStorageError::ValueNotFound(DbKey::TransactionKernel(hash)))?;
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
                    let val: TransactionOutput = lmdb_get(&self.env, &self.stxos_db, hash)?
                        .ok_or_else(|| ChainStorageError::ValueNotFound(DbKey::SpentOutput(hash.clone())))?;
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
            let val: Option<TransactionOutput> = lmdb_get(&self.env, &self.utxos_db, &hash)?;
            if val.is_some() {
                outputs.push(val.unwrap());
                continue;
            }
            // Check the STXO set
            let val: Option<TransactionOutput> = lmdb_get(&self.env, &self.stxos_db, &hash)?;
            match val {
                Some(v) => {
                    spent.push(v.commitment.clone());
                    outputs.push(v);
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

    // Reset any mmr txns that have been applied.
    fn reset_mmrs(&mut self) -> Result<(), ChainStorageError> {
        trace!(target: LOG_TARGET, "Reset mmrs called");
        self.kernel_mmr.reset()?;
        self.utxo_mmr.reset()?;
        self.range_proof_mmr.reset()?;
        Ok(())
    }

    fn rewind_mmrs(&mut self, steps_back: usize) -> Result<(), ChainStorageError> {
        // rewind kernel
        let last_cp = rewind_checkpoints(&mut self.kernel_checkpoints, steps_back)?;
        self.kernel_mmr
            .update()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        self.curr_kernel_checkpoint.reset_to(&last_cp);
        // rewind utxo
        let last_cp = rewind_checkpoints(&mut self.utxo_checkpoints, steps_back)?;
        self.utxo_mmr
            .update()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        self.curr_utxo_checkpoint.reset_to(&last_cp);
        // rewind range proof
        let last_cp = rewind_checkpoints(&mut self.range_proof_checkpoints, steps_back)?;
        self.range_proof_mmr
            .update()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        self.curr_range_proof_checkpoint.reset_to(&last_cp);
        Ok(())
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
                            let cp = self.fetch_checkpoint(MmrTree::Utxo, height)?;
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

    // This function will remove a block from the orphan pool, deconstruct the block and add the block to the
    // block_header, utxo, kernel databases
    fn move_block_from_orphan_to_main_chain(&mut self, block_hash: HashOutput) -> Result<(), ChainStorageError> {
        let val: Block = lmdb_get(&self.env, &self.orphans_db, &block_hash)?
            .ok_or_else(|| ChainStorageError::ValueNotFound(DbKey::OrphanBlock(block_hash)))?;

        let txn = WriteTransaction::new(self.env.clone()).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        let (header, inputs, outputs, kernels) = val.dissolve();
        // lets insert the headers
        let k = header.height;
        if lmdb_exists(&self.env, &self.headers_db, &k)? {
            return Err(ChainStorageError::InvalidOperation(format!(
                "Duplicate `BlockHeader` key `{}`",
                k
            )));
        }
        let hash = header.hash();
        lmdb_insert(&txn, &self.block_hashes_db, &hash, &k)?;
        lmdb_insert(&txn, &self.headers_db, &k, &header)?;

        // lets spend the inputs
        for input in inputs {
            let k = input.hash();
            lmdb_delete(&txn, &self.stxos_db, &k)?;
            lmdb_delete(&txn, &self.txos_hash_to_index_db, &k)?;
        }

        // lets the insert the new utxo's

        for utxo in outputs {
            let k = utxo.hash();
            if lmdb_exists(&self.env, &self.utxos_db, &k)? {
                return Err(ChainStorageError::InvalidOperation(format!(
                    "Duplicate `UnspentOutput` key `{}`",
                    k.to_hex()
                )));
            }
            self.curr_utxo_checkpoint.push_addition(k.clone());
            self.curr_range_proof_checkpoint.push_addition(utxo.proof().hash());

            lmdb_insert(&txn, &self.utxos_db, &k, &utxo)?;
            let index = self.curr_range_proof_checkpoint.accumulated_nodes_added_count() - 1;
            lmdb_insert(&txn, &self.txos_hash_to_index_db, &k, &index)?;
        }
        // lets insert the kernels
        for kernel in kernels {
            let k = kernel.hash();
            if lmdb_exists(&self.env, &self.kernels_db, &k)? {
                return Err(ChainStorageError::InvalidOperation(format!(
                    "Duplicate `TransactionKernel` key `{}`",
                    k.to_hex()
                )));
            }
            self.curr_kernel_checkpoint.push_addition(k.clone());
            lmdb_insert(&txn, &self.kernels_db, &k, &kernel)?;
        }
        // lets remove the orphan
        lmdb_delete(&txn, &self.orphans_db, &k)?;
        // lets update the meta data
        let accumulated_difficulty =
            ProofOfWork::new_from_difficulty(&header.pow, ProofOfWork::achieved_difficulty(&header))
                .total_accumulated_difficulty();
        lmdb_replace(
            &txn,
            &self.metadata_db,
            &(MetadataKey::ChainHeight.clone() as u32),
            &Some(header.height),
        )?;
        lmdb_replace(
            &txn,
            &self.metadata_db,
            &(MetadataKey::BestBlock.clone() as u32),
            &Some(header.hash()),
        )?;
        lmdb_replace(
            &txn,
            &self.metadata_db,
            &(MetadataKey::AccumulatedWork.clone() as u32),
            &Some(accumulated_difficulty),
        )?;

        txn.commit()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        self.mem_metadata = ChainMetadata {
            height_of_longest_chain: fetch_chain_height(&self.env, &self.metadata_db)?,
            best_block: fetch_best_block(&self.env, &self.metadata_db)?,
            pruning_horizon: fetch_pruning_horizon(&self.env, &self.metadata_db)?,
            accumulated_difficulty: fetch_accumulated_work(&self.env, &self.metadata_db)?,
        };
        // let update the mmrs
        // kernels
        let curr_checkpoint = self.curr_kernel_checkpoint.clone();
        self.kernel_checkpoints
            .push(curr_checkpoint)
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        self.curr_kernel_checkpoint.reset();

        self.kernel_mmr
            .update()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        // utxos
        let curr_checkpoint = self.curr_utxo_checkpoint.clone();
        self.utxo_checkpoints
            .push(curr_checkpoint)
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        self.curr_utxo_checkpoint.reset();

        self.utxo_mmr
            .update()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        // rangeproofs
        let curr_checkpoint = self.curr_range_proof_checkpoint.clone();
        self.range_proof_checkpoints
            .push(curr_checkpoint)
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        self.curr_range_proof_checkpoint.reset();

        self.range_proof_mmr
            .update()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;

        Ok(())
    }

    // Perform all the storage txns and all MMR transactions excluding CreateMmrCheckpoint and RewindMmr on the
    // header_mmr, utxo_mmr, range_proof_mmr and kernel_mmr. Only when all the txns can successfully be applied is the
    // changes committed to the backend databases. CreateMmrCheckpoint and RewindMmr txns will be performed after these
    // txns have been successfully applied.
    // fn apply_mmr_and_storage_txs(&mut self, tx: &DbTransaction) -> Result<(), ChainStorageError> {
    //     let mut update_mem_metadata = false;
    //     let txn = WriteTransaction::new(self.env.clone()).map_err(|e|
    // ChainStorageError::AccessError(e.to_string()))?;     for op in tx.operations.iter() {
    //         match op {
    //             WriteOperation::Insert(insert) => match insert {
    //                 DbKeyValuePair::Metadata(k, v) => {
    //                     lmdb_replace(&txn, &self.metadata_db, &(k.clone() as u32), &v)?;
    //                     update_mem_metadata = true;
    //                 },
    //             },
    //             WriteOperation::Delete(delete) => match delete {
    //                 DbKey::Metadata(_) => {}, // no-op
    //             },
    //             WriteOperation::Spend(key) => match key {
    //                 DbKey::UnspentOutput(hash) => {},
    //                 _ => return Err(ChainStorageError::InvalidOperation("Only UTXOs can be spent".into())),
    //             },
    //             WriteOperation::UnSpend(key) => match key {
    //                 DbKey::SpentOutput(hash) => {
    //                     let stxo: TransactionOutput = lmdb_get(&self.env, &self.stxos_db, &hash)?.ok_or_else(|| {
    //                         error!(
    //                             target: LOG_TARGET,
    //                             "STXO could not be unspent: Hash `{}` not found in the STXO db",
    //                             hash.to_hex()
    //                         );
    //                         ChainStorageError::UnspendError
    //                     })?;
    //                     lmdb_delete(&txn, &self.stxos_db, &hash)?;
    //                     lmdb_insert(&txn, &self.utxos_db, &hash, &stxo)?;
    //                 },
    //                 _ => return Err(ChainStorageError::InvalidOperation("Only STXOs can be unspent".into())),
    //             },
    //             _ => {},
    //         }
    //     }
    //     txn.commit()
    //         .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;

    //     Ok(())
    // }

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

    // Retrieves the checkpoint corresponding to the provided height, if the checkpoint is part of the horizon state
    // then a BeyondPruningHorizon error will be produced.
    fn fetch_checkpoint(&self, tree: MmrTree, height: u64) -> Result<MerkleCheckPoint, ChainStorageError> {
        let tip_height = lmdb_len(&self.env, &self.headers_db)?.saturating_sub(1) as u64;
        let pruned_mode = self.mem_metadata.is_pruned_node();
        match tree {
            MmrTree::Kernel => tree_fetch_checkpoint(&self.kernel_checkpoints, pruned_mode, tip_height, height),
            MmrTree::Utxo => tree_fetch_checkpoint(&self.utxo_checkpoints, pruned_mode, tip_height, height),
            MmrTree::RangeProof => {
                tree_fetch_checkpoint(&self.range_proof_checkpoints, pruned_mode, tip_height, height)
            },
        }
        .map_err(|e| ChainStorageError::AccessError(format!("Checkpoint error: {}", e.to_string())))?
        .ok_or_else(|| ChainStorageError::OutOfRange)
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
    fn add_orphan_block(&mut self, block: Block) -> Result<(), ChainStorageError> {
        let hash = block.hash();
        let txn = WriteTransaction::new(self.env.clone()).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        lmdb_insert(&txn, &self.orphans_db, &hash, &block)?;
        txn.commit().map_err(|e| ChainStorageError::AccessError(e.to_string()))
    }

    fn accept_block(&mut self, block_hash: HashOutput) -> Result<(), ChainStorageError> {
        match self.move_block_from_orphan_to_main_chain(block_hash) {
            Ok(_) => Ok(()),
            Err(e) => {
                self.reset_mmrs()?;
                Err(e)
            },
        }
    }

    fn force_meta_data(&mut self, metadata: ChainMetadata) -> Result<(), ChainStorageError> {
        let txn = WriteTransaction::new(self.env.clone()).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        lmdb_replace(
            &txn,
            &self.metadata_db,
            &(MetadataKey::ChainHeight.clone() as u32),
            &metadata.height_of_longest_chain,
        )?;
        lmdb_replace(
            &txn,
            &self.metadata_db,
            &(MetadataKey::BestBlock.clone() as u32),
            &metadata.best_block,
        )?;
        lmdb_replace(
            &txn,
            &self.metadata_db,
            &(MetadataKey::AccumulatedWork.clone() as u32),
            &metadata.accumulated_difficulty,
        )?;

        lmdb_replace(
            &txn,
            &self.metadata_db,
            &(MetadataKey::PruningHorizon.clone() as u32),
            &metadata.pruning_horizon,
        )?;
        txn.commit().map_err(|e| ChainStorageError::AccessError(e.to_string()))
    }

    // rewinds the database to the specified height. It will move every block that was rewound to the orphan pool
    fn rewind_to_height(&mut self, height: u64) -> Result<Vec<BlockHeader>, ChainStorageError> {
        let hashes: Vec<BlockHash> = Vec::new();
        let txn = WriteTransaction::new(self.env.clone()).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        let chain_height = self.mem_metadata.height_of_longest_chain.unwrap_or(0);
        let steps_back = (chain_height - height) as usize;
        let mut removed_blocks = Vec::new();
        for rewind_height in ((height + 1)..=chain_height).rev() {
            // Reconstruct block at height and add to orphan block pool

            let orphaned_block = self.reconstruct_block(rewind_height)?; // fetch_block(&**db, rewind_height)?.block().clone();
                                                                         // 1st we add the removed block back to the orphan pool.
            let hash = orphaned_block.hash();
            lmdb_insert(&txn, &self.orphans_db, &hash, &orphaned_block)?;
            removed_blocks.push(orphaned_block.header.clone());

            // Now we need to remove that block
            // Remove Header and block hash
            lmdb_delete(&txn, &self.block_hashes_db, &hash)?;
            lmdb_delete(&txn, &self.headers_db, &rewind_height)?;

            // lets get the checkpoint
            let hashes = self
                .fetch_checkpoint(MmrTree::Kernel, rewind_height)?
                .nodes_added()
                .clone();
            for hash in hashes {
                lmdb_delete(&txn, &self.kernels_db, &hash)?;
            }
            // Remove UTXOs and move STXOs back to UTXO set
            let (nodes_added, nodes_deleted) = self.fetch_checkpoint(MmrTree::Utxo, rewind_height)?.into_parts();
            for hash in nodes_added {
                lmdb_delete(&txn, &self.utxos_db, &hash)?;
                lmdb_delete(&txn, &self.txos_hash_to_index_db, &hash)?;
            }
            // lets unspend utxos
            for pos in nodes_deleted.iter() {
                self.fetch_mmr_nodes(MmrTree::Utxo, pos, 1, None).and_then(|nodes| {
                    let (stxo_hash, deleted) = &nodes[0];
                    assert!(deleted);

                    let utxo: TransactionOutput = lmdb_get(&self.env, &self.utxos_db, stxo_hash)?.ok_or_else(|| {
                        error!(
                            target: LOG_TARGET,
                            "Could spend UTXO: hash `{}` not found in UTXO db",
                            hash.to_hex()
                        );
                        ChainStorageError::UnspendableInput
                    })?;

                    let index = lmdb_get(&self.env, &self.txos_hash_to_index_db, stxo_hash)?.ok_or_else(|| {
                        error!(
                            target: LOG_TARGET,
                            "** Blockchain DB out of sync! ** Hash `{}` was found in utxo_db but could not be found \
                             in txos_hash_to_index db!",
                            hash.to_hex()
                        );
                        ChainStorageError::UnspendableInput
                    })?;
                    self.curr_utxo_checkpoint.push_deletion(index);

                    lmdb_delete(&txn, &self.utxos_db, &stxo_hash)?;
                    lmdb_insert(&txn, &self.stxos_db, &stxo_hash, &utxo)?;
                    Ok(())
                })?;
            }
        }
        self.rewind_mmrs(steps_back)?;

        match txn.commit().map_err(|e| ChainStorageError::AccessError(e.to_string())) {
            Ok(_) => Ok(removed_blocks),
            Err(e) => {
                self.reset_mmrs()?;
                Err(e)
            },
        }
    }

    /// This is used when synchronising. Adds in the list of headers provided to the main chain
    fn add_block_headers(&mut self, headers: Vec<BlockHeader>) -> Result<(), ChainStorageError> {
        let txn = WriteTransaction::new(self.env.clone()).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        for header in headers {
            // lets insert the headers
            let k = header.height;
            if lmdb_exists(&self.env, &self.headers_db, &k)? {
                return Err(ChainStorageError::InvalidOperation(format!(
                    "Duplicate `BlockHeader` key `{}`",
                    k
                )));
            }
            let hash = header.hash();
            lmdb_insert(&txn, &self.block_hashes_db, &hash, &k)?;
            lmdb_insert(&txn, &self.headers_db, &k, &header)?;
        }
        txn.commit().map_err(|e| ChainStorageError::AccessError(e.to_string()))
    }

    /// This is used when synchronising. Adds in the list of kernels provided to the main chain
    fn add_kernels(&mut self, kernels: Vec<TransactionKernel>) -> Result<(), ChainStorageError> {
        let txn = WriteTransaction::new(self.env.clone()).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        for kernel in kernels {
            let k = kernel.hash();
            if lmdb_exists(&self.env, &self.kernels_db, &k)? {
                return Err(ChainStorageError::InvalidOperation(format!(
                    "Duplicate `TransactionKernel` key `{}`",
                    k.to_hex()
                )));
            }
            self.curr_kernel_checkpoint.push_addition(k.clone());
            lmdb_insert(&txn, &self.kernels_db, &k, &kernel)?;
        }

        txn.commit().map_err(|e| ChainStorageError::AccessError(e.to_string()))
    }

    /// This is used when synchronising. Adds in the list of utxos provided to the main chain
    fn add_utxos(&mut self, utxos: Vec<TransactionOutput>) -> Result<(), ChainStorageError> {
        let txn = WriteTransaction::new(self.env.clone()).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        for utxo in utxos {
            let k = utxo.hash();
            if lmdb_exists(&self.env, &self.utxos_db, &k)? {
                return Err(ChainStorageError::InvalidOperation(format!(
                    "Duplicate `UnspentOutput` key `{}`",
                    k.to_hex()
                )));
            }
            self.curr_utxo_checkpoint.push_addition(k.clone());
            self.curr_range_proof_checkpoint.push_addition(utxo.proof().hash());

            lmdb_insert(&txn, &self.utxos_db, &k, &utxo)?;
            let index = self.curr_range_proof_checkpoint.accumulated_nodes_added_count() - 1;
            lmdb_insert(&txn, &self.txos_hash_to_index_db, &k, &index)?;
        }
        Ok(())
    }

    /// This is used when synchronising. Adds in the mmrs provided to the main chain
    fn add_mmr(&mut self, tree: MmrTree, hashes: Vec<HashOutput>) -> Result<(), ChainStorageError> {
        Ok(())
    }

    /// This function is used to remove orphan blocks
    /// This function will return ok if it did not encounter an error. If a orphan block was not found, it should return
    /// Ok(false)
    fn remove_orphan_blocks(&mut self, block_hashes: Vec<BlockHash>) -> Result<bool, ChainStorageError> {
        let mut results = true;
        for hash in block_hashes {
            if !lmdb_exists(&self.env, &self.orphans_db, &hash)? {
                results = false;
            }
            let txn =
                WriteTransaction::new(self.env.clone()).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
            lmdb_delete(&txn, &self.orphans_db, &hash)?;
            txn.commit()
                .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        }
        Ok(results)
    }

    /// returns a list of orphan block headers that are parents to the named hash
    fn fetch_parent_orphan_headers(
        &self,
        hash: HashOutput,
        height: u64,
    ) -> Result<Vec<BlockHeader>, ChainStorageError>
    {
        let mut headers = Vec::new();

        lmdb_for_each::<_, HashOutput, Block>(&self.env, &self.orphans_db, |pair| {
            let (_, block) = pair.unwrap();
            if (block.header.prev_hash == hash) && (block.header.height == height + 1) {
                // we found a match, let save to call later
                headers.push(block.header);
            }
        })?;
        Ok(headers)
    }

    /// Returns a list of all orphan block headers
    fn fetch_all_orphan_headers(&self) -> Result<Vec<BlockHeader>, ChainStorageError> {
        let mut headers = Vec::new();
        lmdb_for_each::<_, HashOutput, Block>(&self.env, &self.orphans_db, |pair| {
            let (_, block) = pair.unwrap();
            // we found a match, let save to call later
            headers.push(block.header);
        })?;
        Ok(headers)
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
            DbKey::Block(k) => {
                let block = self.reconstruct_block(*k)?;
                Some(DbValue::OrphanBlock(Box::new(block)))
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
            DbKey::Block(k) => lmdb_exists(&self.env, &self.headers_db, k)?,
        })
    }

    // fn fetch_mmr_root(&self, tree: MmrTree) -> Result<Vec<u8>, ChainStorageError> {
    //     let pruned_mmr = self.get_pruned_mmr(&tree)?;
    //     Ok(pruned_mmr.get_merkle_root()?)
    // }

    // fn fetch_mmr_only_root(&self, tree: MmrTree) -> Result<Vec<u8>, ChainStorageError> {
    //     let pruned_mmr = self.get_pruned_mmr(&tree)?;
    //     Ok(pruned_mmr.get_mmr_only_root()?)
    // }

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
    // fn fetch_mmr_proof(&self, tree: MmrTree, leaf_pos: usize) -> Result<MerkleProof, ChainStorageError> {
    //     let pruned_mmr = self.get_pruned_mmr(&tree)?;
    //     Ok(match tree {
    //         MmrTree::Utxo => MerkleProof::for_leaf_node(&pruned_mmr.mmr(), leaf_pos)?,
    //         MmrTree::Kernel => MerkleProof::for_leaf_node(&pruned_mmr.mmr(), leaf_pos)?,
    //         MmrTree::RangeProof => MerkleProof::for_leaf_node(&pruned_mmr.mmr(), leaf_pos)?,
    //     })
    // }

    fn fetch_mmr_node_count(&self, tree: MmrTree, height: u64) -> Result<u32, ChainStorageError> {
        let tip_height = lmdb_len(&self.env, &self.headers_db)?.saturating_sub(1) as u64;
        match tree {
            MmrTree::Kernel => fetch_mmr_nodes_added_count(&self.kernel_checkpoints, tip_height, height),
            MmrTree::Utxo => fetch_mmr_nodes_added_count(&self.utxo_checkpoints, tip_height, height),
            MmrTree::RangeProof => fetch_mmr_nodes_added_count(&self.range_proof_checkpoints, tip_height, height),
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

    // /// Iterate over all the stored orphan blocks and execute the function `f` for each block.
    // fn for_each_orphan<F>(&self, f: F) -> Result<(), ChainStorageError>
    // where F: FnMut(Result<(HashOutput, Block), ChainStorageError>) {
    //     lmdb_for_each::<F, HashOutput, Block>(&self.env, &self.orphans_db, f)
    // }

    /// Returns the number of blocks in the block orphan pool.
    fn get_orphan_count(&self) -> Result<usize, ChainStorageError> {
        lmdb_len(&self.env, &self.orphans_db)
    }

    /// Iterate over all the stored transaction kernels and execute the function `f` for each kernel.
    // fn for_each_kernel<F>(&self, f: F) -> Result<(), ChainStorageError>
    // where F: FnMut(Result<(HashOutput, TransactionKernel), ChainStorageError>) {
    //     lmdb_for_each::<F, HashOutput, TransactionKernel>(&self.env, &self.kernels_db, f)
    // }

    // /// Iterate over all the stored block headers and execute the function `f` for each header.
    // fn for_each_header<F>(&self, f: F) -> Result<(), ChainStorageError>
    // where F: FnMut(Result<(u64, BlockHeader), ChainStorageError>) {
    //     lmdb_for_each::<F, u64, BlockHeader>(&self.env, &self.headers_db, f)
    // }

    /// Iterate over all the stored unspent transaction outputs and execute the function `f` for each kernel.
    // fn for_each_utxo<F>(&self, f: F) -> Result<(), ChainStorageError>
    // where F: FnMut(Result<(HashOutput, TransactionOutput), ChainStorageError>) {
    //     lmdb_for_each::<F, HashOutput, TransactionOutput>(&self.env, &self.utxos_db, f)
    // }

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

// Retrieves the checkpoint corresponding to the provided height, if the checkpoint is part of the horizon state then a
// BeyondPruningHorizon error will be produced.
fn tree_fetch_checkpoint<T>(
    checkpoints: &T,
    pruned_mode: bool,
    tip_height: u64,
    height: u64,
) -> Result<Option<MerkleCheckPoint>, ChainStorageError>
where
    T: ArrayLike<Value = MerkleCheckPoint>,
    T::Error: Display,
{
    let last_cp_index = checkpoints
        .len()
        .map_err(|e| ChainStorageError::AccessError(e.to_string()))?
        .saturating_sub(1);
    let offset = tip_height
        .checked_sub(height)
        .ok_or_else(|| ChainStorageError::OutOfRange)?;
    let index = last_cp_index
        .checked_sub(offset as usize)
        .ok_or_else(|| ChainStorageError::BeyondPruningHorizon)?;
    if pruned_mode && index == 0 {
        // In pruned mode the first checkpoint is an accumulation of all checkpoints from the genesis block to horizon
        // block height.
        return Err(ChainStorageError::BeyondPruningHorizon);
    }
    checkpoints
        .get(index as usize)
        .map_err(|e| ChainStorageError::AccessError(e.to_string()))
}

// Calculate the total leaf node count upto a specified height.
fn fetch_mmr_nodes_added_count<T>(checkpoints: &T, tip_height: u64, height: u64) -> Result<u32, ChainStorageError>
where
    T: ArrayLike<Value = MerkleCheckPoint>,
    T::Error: Display,
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

// Returns the accumulated node added count.
fn fetch_last_mmr_node_added_count<T>(checkpoints: &T) -> Result<u32, ChainStorageError>
where
    T: ArrayLike<Value = MerkleCheckPoint>,
    T::Error: Display,
{
    let cp_count = checkpoints
        .len()
        .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
    Ok(match cp_count.checked_sub(1) {
        Some(last_index) => checkpoints
            .get(last_index)
            .map_err(|e| ChainStorageError::AccessError(format!("Checkpoint error: {}", e.to_string())))?
            .map(|cp| cp.accumulated_nodes_added_count())
            .unwrap_or(0),
        None => 0,
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

// Rewinds checkpoints by `steps_back` elements and returns the last checkpoint.
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

// Attempt to merge the set of oldest checkpoints into the horizon state and return the number of checkpoints that have
// been merged.
fn merge_checkpoints(
    checkpoints: &mut LMDBVec<MerkleCheckPoint>,
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
