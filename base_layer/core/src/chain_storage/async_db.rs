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
    blocks::{Block, BlockHeader, NewBlockTemplate},
    chain_storage::{
        accumulated_data::BlockHeaderAccumulatedData,
        blockchain_database::BlockAddResult,
        BlockAccumulatedData,
        BlockchainBackend,
        BlockchainDatabase,
        ChainBlock,
        ChainHeader,
        ChainStorageError,
        DbTransaction,
        HistoricalBlock,
        HorizonData,
        MmrTree,
        PrunedOutput,
        TargetDifficulties,
    },
    common::rolling_vec::RollingVec,
    proof_of_work::{PowAlgorithm, TargetDifficultyWindow},
    tari_utilities::epoch_time::EpochTime,
    transactions::{
        transaction::{TransactionKernel, TransactionOutput},
        types::{Commitment, HashOutput, Signature},
    },
};
use croaring::Bitmap;
use log::*;
use rand::{rngs::OsRng, RngCore};
use std::{mem, ops::RangeBounds, sync::Arc, time::Instant};
use tari_common_types::{chain_metadata::ChainMetadata, types::BlockHash};
use tari_mmr::pruned_hashset::PrunedHashSet;

const LOG_TARGET: &str = "c::bn::async_db";

fn trace_log<F, R>(name: &str, f: F) -> R
where F: FnOnce() -> R {
    let start = Instant::now();
    let trace_id = OsRng.next_u32();
    trace!(
        target: LOG_TARGET,
        "[{}] Entered blocking thread. trace_id: {}",
        name,
        trace_id
    );
    let ret = f();
    trace!(
        target: LOG_TARGET,
        "[{}] Exited blocking thread after {}ms. trace_id: {}",
        name,
        start.elapsed().as_millis(),
        trace_id
    );
    ret
}

macro_rules! make_async_fn {
    (
     $(#[$outer:meta])*
     $fn:ident() -> $rtype:ty, $name:expr) => {
        $(#[$outer])*
        pub async fn $fn(&self) -> Result<$rtype, ChainStorageError> {
            let db = self.db.clone();
            tokio::task::spawn_blocking(move || {
                trace_log($name, move || db.$fn())
            })
            .await?
        }
    };


    (
     $(#[$outer:meta])*
     $fn:ident$(< $( $lt:tt $( : $clt:path )? ),+ >)?($($param:ident:$ptype:ty),+) -> $rtype:ty, $name:expr) => {
        $(#[$outer])*
        pub async fn $fn$(< $( $lt $( : $clt )? ),+ +Sync+Send + 'static >)?(&self, $($param: $ptype),+) -> Result<$rtype, ChainStorageError> {
            let db = self.db.clone();
            tokio::task::spawn_blocking(move || {
                trace_log($name, move || db.$fn($($param),+))
            })
            .await?
        }
    };
}

/// Asynchronous version of the BlockchainDatabase.
/// This component proxies all functions within BlockchainDatabase, executing each on tokio's blocking thread pool.
pub struct AsyncBlockchainDb<B> {
    db: BlockchainDatabase<B>,
}

impl<B: BlockchainBackend + 'static> AsyncBlockchainDb<B> {
    pub fn new(db: BlockchainDatabase<B>) -> Self {
        Self { db }
    }

    pub fn write_transaction(&self) -> AsyncDbTransaction<'_, B> {
        AsyncDbTransaction::new(self)
    }

    pub fn into_inner(self) -> BlockchainDatabase<B> {
        self.db
    }

    pub fn inner(&self) -> &BlockchainDatabase<B> {
        &self.db
    }
}

impl<B: BlockchainBackend + 'static> AsyncBlockchainDb<B> {
    make_async_fn!(write(transaction: DbTransaction) -> (), "write");

    //---------------------------------- Metadata --------------------------------------------//
    make_async_fn!(get_chain_metadata() -> ChainMetadata, "get_chain_metadata");

    make_async_fn!(fetch_horizon_data() -> Option<HorizonData>, "fetch_horizon_data");

    //---------------------------------- TXO --------------------------------------------//
    make_async_fn!(fetch_utxo(hash: HashOutput) -> Option<TransactionOutput>, "fetch_utxo");

    make_async_fn!(fetch_utxos(hashes: Vec<HashOutput>, is_spent_as_of: Option<HashOutput>) -> Vec<Option<(TransactionOutput, bool)>>, "fetch_utxos");

    make_async_fn!(fetch_utxos_by_mmr_position(start: u64, end: u64, end_header_hash: HashOutput) -> (Vec<PrunedOutput>, Vec<Bitmap>), "fetch_utxos_by_mmr_position");

    //---------------------------------- Kernel --------------------------------------------//
    make_async_fn!(fetch_kernel_by_excess_sig(excess_sig: Signature) -> Option<(TransactionKernel, HashOutput)>, "fetch_kernel_by_excess_sig");

    make_async_fn!(fetch_kernels_by_mmr_position(start: u64, end: u64) -> Vec<TransactionKernel>, "fetch_kernels_by_mmr_position");

    //---------------------------------- MMR --------------------------------------------//
    make_async_fn!(prepare_block_merkle_roots(template: NewBlockTemplate) -> Block, "create_block");

    make_async_fn!(fetch_mmr_size(tree: MmrTree) -> u64, "fetch_mmr_node_count");

    make_async_fn!(rewind_to_height(height: u64) -> Vec<Arc<ChainBlock>>, "rewind_to_height");

    make_async_fn!(rewind_to_hash(hash: BlockHash) -> Vec<Arc<ChainBlock>>, "rewind_to_hash");

    //---------------------------------- Headers --------------------------------------------//
    make_async_fn!(fetch_header(height: u64) -> Option<BlockHeader>, "fetch_header");

    make_async_fn!(fetch_chain_header(height: u64) -> ChainHeader, "fetch_chain_header");

    make_async_fn!(fetch_chain_headers<T: RangeBounds<u64>>(bounds: T) -> Vec<ChainHeader>, "fetch_chain_headers");

    make_async_fn!(fetch_header_and_accumulated_data(height: u64) -> (BlockHeader, BlockHeaderAccumulatedData), "fetch_header_and_accumulated_data");

    make_async_fn!(fetch_header_accumulated_data(hash: HashOutput) -> Option<BlockHeaderAccumulatedData>, "fetch_header_accumulated_data");

    make_async_fn!(fetch_headers<T: RangeBounds<u64>>(bounds: T) -> Vec<BlockHeader>, "fetch_headers");

    make_async_fn!(fetch_header_by_block_hash(hash: HashOutput) -> Option<BlockHeader>, "fetch_header_by_block_hash");

    make_async_fn!(fetch_header_containing_kernel_mmr(mmr_position: u64) -> ChainHeader, "fetch_header_containing_kernel_mmr");

    make_async_fn!(fetch_header_containing_utxo_mmr(mmr_position: u64) -> ChainHeader, "fetch_header_containing_utxo_mmr");

    make_async_fn!(fetch_chain_header_by_block_hash(hash: HashOutput) -> Option<ChainHeader>, "fetch_chain_header_by_block_hash");

    make_async_fn!(
         /// Find the first matching header in a list of block hashes, returning the index of the match and the BlockHeader. Or None if not found.
        find_headers_after_hash<I: IntoIterator<Item = HashOutput>>(ordered_hashes: I, count: u64) -> Option<(usize, Vec<BlockHeader>)>,
        "find_headers_after_hash"
    );

    make_async_fn!(fetch_last_header() -> BlockHeader, "fetch_last_header");

    make_async_fn!(fetch_tip_header() -> ChainHeader, "fetch_tip_header");

    make_async_fn!(insert_valid_headers(headers: Vec<(BlockHeader, BlockHeaderAccumulatedData)>) -> (), "insert_valid_headers");

    //---------------------------------- Block --------------------------------------------//
    make_async_fn!(add_block(block: Arc<Block>) -> BlockAddResult, "add_block");

    make_async_fn!(cleanup_orphans() -> (), "cleanup_orphans");

    make_async_fn!(cleanup_all_orphans() -> (), "cleanup_all_orphans");

    make_async_fn!(block_exists(block_hash: BlockHash) -> bool, "block_exists");

    make_async_fn!(fetch_block(height: u64) -> HistoricalBlock, "fetch_block");

    make_async_fn!(fetch_blocks<T: RangeBounds<u64>>(bounds: T) -> Vec<HistoricalBlock>, "fetch_blocks");

    make_async_fn!(fetch_orphan(hash: HashOutput) -> Block, "fetch_orphan");

    make_async_fn!(fetch_block_by_hash(hash: HashOutput) -> Option<HistoricalBlock>, "fetch_block_by_hash");

    make_async_fn!(fetch_block_with_kernel(excess_sig: Signature) -> Option<HistoricalBlock>, "fetch_block_with_kernel");

    make_async_fn!(fetch_block_with_stxo(commitment: Commitment) -> Option<HistoricalBlock>, "fetch_block_with_stxo");

    make_async_fn!(fetch_block_with_utxo(commitment: Commitment) -> Option<HistoricalBlock>, "fetch_block_with_utxo");

    make_async_fn!(fetch_block_accumulated_data(hash: HashOutput) -> BlockAccumulatedData, "fetch_block_accumulated_data");

    make_async_fn!(fetch_block_accumulated_data_by_height(height: u64) -> BlockAccumulatedData, "fetch_block_accumulated_data_by_height");

    //---------------------------------- Misc. --------------------------------------------//
    make_async_fn!(fetch_block_timestamps(start_hash: HashOutput) -> RollingVec<EpochTime>, "fetch_block_timestamps");

    make_async_fn!(fetch_target_difficulty(pow_algo: PowAlgorithm,height: u64) -> TargetDifficultyWindow, "fetch_target_difficulty");

    make_async_fn!(fetch_target_difficulties(start_hash: HashOutput) -> TargetDifficulties, "fetch_target_difficulties");

    make_async_fn!(fetch_block_hashes_from_header_tip(n: usize, offset: usize) -> Vec<HashOutput>, "fetch_block_hashes_from_header_tip");
}

impl<B: BlockchainBackend + 'static> From<BlockchainDatabase<B>> for AsyncBlockchainDb<B> {
    fn from(db: BlockchainDatabase<B>) -> Self {
        Self::new(db)
    }
}

impl<B> Clone for AsyncBlockchainDb<B> {
    fn clone(&self) -> Self {
        Self { db: self.db.clone() }
    }
}

pub struct AsyncDbTransaction<'a, B> {
    db: &'a AsyncBlockchainDb<B>,
    transaction: DbTransaction,
}

impl<'a, B: BlockchainBackend + 'static> AsyncDbTransaction<'a, B> {
    pub fn new(db: &'a AsyncBlockchainDb<B>) -> Self {
        Self {
            db,
            transaction: DbTransaction::new(),
        }
    }

    pub fn set_best_block(&mut self, height: u64, hash: HashOutput, accumulated_data: u128) -> &mut Self {
        self.transaction.set_best_block(height, hash, accumulated_data);
        self
    }

    pub fn set_pruned_height(&mut self, height: u64, kernel_sum: Commitment, utxo_sum: Commitment) -> &mut Self {
        self.transaction.set_pruned_height(height, kernel_sum, utxo_sum);
        self
    }

    pub fn insert_kernel_via_horizon_sync(
        &mut self,
        kernel: TransactionKernel,
        header_hash: HashOutput,
        mmr_position: u32,
    ) -> &mut Self
    {
        self.transaction.insert_kernel(kernel, header_hash, mmr_position);
        self
    }

    pub fn insert_output_via_horizon_sync(
        &mut self,
        output: TransactionOutput,
        header_hash: HashOutput,
        header_height: u64,
        mmr_position: u32,
    ) -> &mut Self
    {
        self.transaction
            .insert_utxo(output, header_hash, header_height, mmr_position);
        self
    }

    pub fn insert_pruned_output_via_horizon_sync(
        &mut self,
        output_hash: HashOutput,
        proof_hash: HashOutput,
        header_hash: HashOutput,
        header_height: u64,
        mmr_position: u32,
    ) -> &mut Self
    {
        self.transaction
            .insert_pruned_utxo(output_hash, proof_hash, header_hash, header_height, mmr_position);
        self
    }

    pub fn update_pruned_hash_set(
        &mut self,
        mmr_tree: MmrTree,
        header_hash: HashOutput,
        pruned_hash_set: PrunedHashSet,
    ) -> &mut Self
    {
        self.transaction
            .update_pruned_hash_set(mmr_tree, header_hash, pruned_hash_set);
        self
    }

    pub fn update_deleted_with_diff(&mut self, header_hash: HashOutput, deleted: Bitmap) -> &mut Self {
        self.transaction.update_deleted_with_diff(header_hash, deleted);
        self
    }

    pub fn insert_header(&mut self, header: BlockHeader, accum_data: BlockHeaderAccumulatedData) -> &mut Self {
        self.transaction.insert_header(header, accum_data);
        self
    }

    /// Add the BlockHeader and contents of a `Block` (i.e. inputs, outputs and kernels) to the database.
    /// If the `BlockHeader` already exists, then just the contents are updated along with the relevant accumulated
    /// data.
    pub fn insert_block(&mut self, block: Arc<ChainBlock>) -> &mut Self {
        self.transaction.insert_block(block);
        self
    }

    pub async fn commit(&mut self) -> Result<(), ChainStorageError> {
        let transaction = mem::take(&mut self.transaction);
        self.db.write(transaction).await
    }
}
