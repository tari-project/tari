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
use std::{
    mem,
    ops::{Range, RangeBounds},
    sync::Arc,
    time::Instant,
};

use croaring::Bitmap;
use log::*;
use rand::{rngs::OsRng, RngCore};
use tari_common_types::{
    chain_metadata::ChainMetadata,
    types::{BlockHash, Commitment, HashOutput, PublicKey, Signature},
};
use tari_utilities::epoch_time::EpochTime;

use crate::{
    blocks::{
        Block,
        BlockAccumulatedData,
        BlockHeader,
        BlockHeaderAccumulatedData,
        ChainBlock,
        ChainHeader,
        CompleteDeletedBitmap,
        DeletedBitmap,
        HistoricalBlock,
        NewBlockTemplate,
        UpdateBlockAccumulatedData,
    },
    chain_storage::{
        blockchain_database::MmrRoots,
        utxo_mined_info::UtxoMinedInfo,
        BlockAddResult,
        BlockchainBackend,
        BlockchainDatabase,
        ChainStorageError,
        DbBasicStats,
        DbTotalSizeStats,
        DbTransaction,
        HorizonData,
        MmrTree,
        PrunedOutput,
        TargetDifficulties,
    },
    common::rolling_vec::RollingVec,
    proof_of_work::{PowAlgorithm, TargetDifficultyWindow},
    transactions::transaction_components::{TransactionKernel, TransactionOutput},
};

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
            let mut mdc = vec![];
            log_mdc::iter(|k, v| mdc.push((k.to_owned(), v.to_owned())));
            tokio::task::spawn_blocking(move || {
                    log_mdc::extend(mdc.clone());
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
            let mut mdc = vec![];
            log_mdc::iter(|k, v| mdc.push((k.to_owned(), v.to_owned())));
            tokio::task::spawn_blocking(move || {
                log_mdc::extend(mdc.clone());
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

    make_async_fn!(fetch_horizon_data() -> HorizonData, "fetch_horizon_data");

    //---------------------------------- TXO --------------------------------------------//
    make_async_fn!(fetch_utxo(hash: HashOutput) -> Option<PrunedOutput>, "fetch_utxo");

    make_async_fn!(fetch_utxos(hashes: Vec<HashOutput>) -> Vec<Option<(PrunedOutput, bool)>>, "fetch_utxos");

    make_async_fn!(fetch_utxos_and_mined_info(hashes: Vec<HashOutput>) -> Vec<Option<UtxoMinedInfo>>, "fetch_utxos_and_mined_info");

    make_async_fn!(fetch_utxos_in_block(hash: HashOutput, deleted: Option<Arc<Bitmap>>) -> (Vec<PrunedOutput>, Bitmap), "fetch_utxos_in_block");

    make_async_fn!(fetch_utxo_by_unique_id(parent_public_key: Option<PublicKey>,unique_id: HashOutput, deleted_at: Option<u64>) -> Option<UtxoMinedInfo>, "fetch_utxo_by_unique_id");

    make_async_fn!(fetch_all_unspent_by_parent_public_key(
        parent_public_key: PublicKey,
        range: Range<usize>) -> Vec<UtxoMinedInfo>, "fetch_all_unspent_by_parent_public_key");

    //---------------------------------- Kernel --------------------------------------------//
    make_async_fn!(fetch_kernel_by_excess_sig(excess_sig: Signature) -> Option<(TransactionKernel, HashOutput)>, "fetch_kernel_by_excess_sig");

    make_async_fn!(fetch_kernels_in_block(hash: HashOutput) -> Vec<TransactionKernel>, "fetch_kernels_in_block");

    //---------------------------------- MMR --------------------------------------------//
    make_async_fn!(prepare_new_block(template: NewBlockTemplate) -> Block, "prepare_new_block");

    make_async_fn!(fetch_mmr_size(tree: MmrTree) -> u64, "fetch_mmr_size");

    make_async_fn!(calculate_mmr_roots(block: Block) -> (Block, MmrRoots), "calculate_mmr_roots");

    //---------------------------------- Headers --------------------------------------------//
    make_async_fn!(fetch_header(height: u64) -> Option<BlockHeader>, "fetch_header");

    make_async_fn!(fetch_chain_header(height: u64) -> ChainHeader, "fetch_chain_header");

    make_async_fn!(fetch_chain_headers<T: RangeBounds<u64>>(bounds: T) -> Vec<ChainHeader>, "fetch_chain_headers");

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

    make_async_fn!(clear_all_pending_headers() -> usize, "clear_all_pending_headers");

    make_async_fn!(fetch_last_chain_header() -> ChainHeader, "fetch_last_chain_header");

    make_async_fn!(fetch_tip_header() -> ChainHeader, "fetch_tip_header");

    make_async_fn!(insert_valid_headers(headers: Vec<ChainHeader>) -> (), "insert_valid_headers");

    //---------------------------------- Block --------------------------------------------//
    make_async_fn!(add_block(block: Arc<Block>) -> BlockAddResult, "add_block");

    make_async_fn!(cleanup_orphans() -> (), "cleanup_orphans");

    make_async_fn!(cleanup_all_orphans() -> (), "cleanup_all_orphans");

    make_async_fn!(block_exists(block_hash: BlockHash) -> bool, "block_exists");

    make_async_fn!(bad_block_exists(block_hash: BlockHash) -> bool, "bad_block_exists");

    make_async_fn!(fetch_block(height: u64) -> HistoricalBlock, "fetch_block");

    make_async_fn!(fetch_blocks<T: RangeBounds<u64>>(bounds: T) -> Vec<HistoricalBlock>, "fetch_blocks");

    make_async_fn!(fetch_orphan(hash: HashOutput) -> Block, "fetch_orphan");

    make_async_fn!(fetch_block_by_hash(hash: HashOutput) -> Option<HistoricalBlock>, "fetch_block_by_hash");

    make_async_fn!(fetch_block_with_kernel(excess_sig: Signature) -> Option<HistoricalBlock>, "fetch_block_with_kernel");

    make_async_fn!(fetch_block_with_utxo(commitment: Commitment) -> Option<HistoricalBlock>, "fetch_block_with_utxo");

    make_async_fn!(fetch_block_accumulated_data(hash: HashOutput) -> BlockAccumulatedData, "fetch_block_accumulated_data");

    make_async_fn!(fetch_block_accumulated_data_by_height(height: u64) -> BlockAccumulatedData, "fetch_block_accumulated_data_by_height");

    //---------------------------------- Misc. --------------------------------------------//

    make_async_fn!(prune_to_height(height: u64) -> (), "prune_to_height");

    make_async_fn!(rewind_to_height(height: u64) -> Vec<Arc<ChainBlock>>, "rewind_to_height");

    make_async_fn!(rewind_to_hash(hash: BlockHash) -> Vec<Arc<ChainBlock>>, "rewind_to_hash");

    make_async_fn!(fetch_block_timestamps(start_hash: HashOutput) -> RollingVec<EpochTime>, "fetch_block_timestamps");

    make_async_fn!(fetch_target_difficulty_for_next_block(pow_algo: PowAlgorithm, current_block_hash: HashOutput) -> TargetDifficultyWindow, "fetch_target_difficulty");

    make_async_fn!(fetch_target_difficulties_for_next_block(current_block_hash: HashOutput) -> TargetDifficulties, "fetch_target_difficulties_for_next_block");

    make_async_fn!(fetch_block_hashes_from_header_tip(n: usize, offset: usize) -> Vec<HashOutput>, "fetch_block_hashes_from_header_tip");

    make_async_fn!(fetch_complete_deleted_bitmap_at(hash: HashOutput) -> CompleteDeletedBitmap, "fetch_deleted_bitmap");

    make_async_fn!(fetch_deleted_bitmap_at_tip() -> DeletedBitmap, "fetch_deleted_bitmap_at_tip");

    make_async_fn!(fetch_header_hash_by_deleted_mmr_positions(mmr_positions: Vec<u32>) -> Vec<Option<(u64, HashOutput)>>, "fetch_headers_of_deleted_positions");

    make_async_fn!(get_stats() -> DbBasicStats, "get_stats");

    make_async_fn!(fetch_total_size_stats() -> DbTotalSizeStats, "fetch_total_size_stats");
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

    pub fn set_best_block(
        &mut self,
        height: u64,
        hash: HashOutput,
        accumulated_difficulty: u128,
        expected_prev_best_block: HashOutput,
    ) -> &mut Self {
        self.transaction
            .set_best_block(height, hash, accumulated_difficulty, expected_prev_best_block);
        self
    }

    pub fn set_pruned_height(&mut self, height: u64) -> &mut Self {
        self.transaction.set_pruned_height(height);
        self
    }

    pub fn set_horizon_data(&mut self, kernel_sum: Commitment, utxo_sum: Commitment) -> &mut Self {
        self.transaction.set_horizon_data(kernel_sum, utxo_sum);
        self
    }

    pub fn insert_kernel_via_horizon_sync(
        &mut self,
        kernel: TransactionKernel,
        header_hash: HashOutput,
        mmr_position: u32,
    ) -> &mut Self {
        self.transaction.insert_kernel(kernel, header_hash, mmr_position);
        self
    }

    pub fn insert_output_via_horizon_sync(
        &mut self,
        output: TransactionOutput,
        header_hash: HashOutput,
        header_height: u64,
        mmr_position: u32,
    ) -> &mut Self {
        self.transaction
            .insert_utxo(output, header_hash, header_height, mmr_position);
        self
    }

    pub fn insert_pruned_output_via_horizon_sync(
        &mut self,
        output_hash: HashOutput,
        witness_hash: HashOutput,
        header_hash: HashOutput,
        header_height: u64,
        mmr_position: u32,
    ) -> &mut Self {
        self.transaction
            .insert_pruned_utxo(output_hash, witness_hash, header_hash, header_height, mmr_position);
        self
    }

    pub fn update_block_accumulated_data_via_horizon_sync(
        &mut self,
        header_hash: HashOutput,
        values: UpdateBlockAccumulatedData,
    ) -> &mut Self {
        self.transaction.update_block_accumulated_data(header_hash, values);
        self
    }

    /// Updates the deleted tip bitmap with the indexes of the given bitmap.
    pub fn update_deleted_bitmap(&mut self, deleted: Bitmap) -> &mut Self {
        self.transaction.update_deleted_bitmap(deleted);
        self
    }

    pub fn insert_chain_header(&mut self, chain_header: ChainHeader) -> &mut Self {
        self.transaction.insert_chain_header(chain_header);
        self
    }

    pub fn insert_block_body(&mut self, block: Arc<ChainBlock>) -> &mut Self {
        self.transaction.insert_block_body(block);
        self
    }

    pub fn insert_bad_block(&mut self, hash: HashOutput, height: u64) -> &mut Self {
        self.transaction.insert_bad_block(hash, height);
        self
    }

    pub fn prune_outputs_at_positions(&mut self, positions: Vec<u32>) -> &mut Self {
        self.transaction.prune_outputs_at_positions(positions);
        self
    }

    pub async fn commit(&mut self) -> Result<(), ChainStorageError> {
        let transaction = mem::take(&mut self.transaction);
        self.db.write(transaction).await
    }
}
