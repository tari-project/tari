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
        blockchain_database::BlockAddResult,
        BlockchainBackend,
        BlockchainDatabase,
        ChainStorageError,
        DbTransaction,
        HistoricalBlock,
        InProgressHorizonSyncState,
        MetadataKey,
        MetadataValue,
        TargetDifficulties,
    },
    common::rolling_vec::RollingVec,
    proof_of_work::{PowAlgorithm, TargetDifficultyWindow},
    tari_utilities::epoch_time::EpochTime,
    transactions::{
        transaction::{TransactionKernel, TransactionOutput},
        types::{Commitment, HashOutput, Signature},
    },
    types::MmrTree,
};
use log::*;
use rand::{rngs::OsRng, RngCore};
use std::{mem, ops::RangeBounds, sync::Arc, time::Instant};
use tari_common_types::{chain_metadata::ChainMetadata, types::BlockHash};
use tari_mmr::Hash;

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

    make_async_fn!(set_chain_metadata(metadata: ChainMetadata) -> (), "set_chain_metadata");

    //---------------------------------- Kernels --------------------------------------------//
    make_async_fn!(fetch_kernel(hash: HashOutput) -> TransactionKernel, "fetch_kernel");

    //---------------------------------- TXO --------------------------------------------//
    make_async_fn!(fetch_utxo(hash: HashOutput) -> Option<TransactionOutput>, "fetch_utxo");

    make_async_fn!(fetch_utxos(hashes: Vec<HashOutput>, is_spent_as_of: Option<HashOutput>) -> Vec<Option<(TransactionOutput, bool)>>, "fetch_utxos");

    //---------------------------------- MMR --------------------------------------------//
    make_async_fn!(prepare_block_merkle_roots(template: NewBlockTemplate) -> Block, "create_block");

    make_async_fn!(fetch_mmr_node_count(tree: MmrTree, height: u64) -> u32, "fetch_mmr_node_count");

    make_async_fn!(fetch_mmr_nodes(tree: MmrTree, pos: u32, count: u32, hist_height:Option<u64>) -> Vec<(Vec<u8>, bool)>, "fetch_mmr_nodes");

    make_async_fn!(insert_mmr_node(tree: MmrTree, hash: Hash, deleted: bool) -> (), "insert_mmr_node");

    make_async_fn!(rewind_to_height(height: u64) -> Vec<Arc<Block>>, "rewind_to_height");

    //---------------------------------- Headers --------------------------------------------//
    make_async_fn!(fetch_header(height: u64) -> Option<BlockHeader>, "fetch_header");

    make_async_fn!(fetch_headers<T: RangeBounds<u64>>(bounds: T) -> Vec<BlockHeader>, "fetch_headers");

    make_async_fn!(fetch_header_by_block_hash(hash: HashOutput) -> Option<BlockHeader>, "fetch_header_by_block_hash");

    make_async_fn!(
         /// Find the first matching header in a list of block hashes, returning the index of the match and the BlockHeader. Or None if not found.
        find_headers_after_hash<I: IntoIterator<Item = HashOutput>>(ordered_hashes: I, count: u64) -> Option<(usize, Vec<BlockHeader>)>,
        "find_headers_after_hash"
    );

    make_async_fn!(fetch_tip_header() -> BlockHeader, "fetch_header");

    make_async_fn!(insert_valid_headers(headers: Vec<BlockHeader>) -> (), "insert_valid_headers");

    make_async_fn!(fetch_target_difficulty(pow_algo: PowAlgorithm, height: u64) -> TargetDifficultyWindow, "fetch_target_difficulty");

    //---------------------------------- Block --------------------------------------------//
    make_async_fn!(add_block(block: Arc<Block>) -> BlockAddResult, "add_block");

    make_async_fn!(cleanup_all_orphans() -> (), "cleanup_all_orphans");

    make_async_fn!(block_exists(block_hash: BlockHash) -> bool, "block_exists");

    make_async_fn!(fetch_block(height: u64) -> HistoricalBlock, "fetch_block");

    make_async_fn!(fetch_blocks<T: RangeBounds<u64>>(bounds: T) -> Vec<HistoricalBlock>, "fetch_blocks");

    make_async_fn!(fetch_orphan(hash: HashOutput) -> Block, "fetch_orphan");

    make_async_fn!(fetch_block_by_hash(hash: HashOutput) -> Option<HistoricalBlock>, "fetch_block_by_hash");

    make_async_fn!(fetch_block_with_kernel(excess_sig: Signature) -> Option<HistoricalBlock>, "fetch_block_with_kernel");

    make_async_fn!(fetch_block_with_stxo(commitment: Commitment) -> Option<HistoricalBlock>, "fetch_block_with_stxo");

    make_async_fn!(fetch_block_with_utxo(commitment: Commitment) -> Option<HistoricalBlock>, "fetch_block_with_utxo");

    //---------------------------------- Horizon Sync --------------------------------------------//
    make_async_fn!(get_horizon_sync_state() -> Option<InProgressHorizonSyncState>, "get_horizon_sync_state");

    make_async_fn!(set_horizon_sync_state(state: InProgressHorizonSyncState) -> (), "set_horizon_sync_state");

    make_async_fn!(horizon_sync_begin() -> InProgressHorizonSyncState, "horizon_sync_begin");

    make_async_fn!(horizon_sync_commit() -> (), "horizon_sync_commit");

    make_async_fn!(horizon_sync_rollback() -> (), "horizon_sync_rollback");

    make_async_fn!(horizon_sync_insert_kernels(kernels: Vec<TransactionKernel>) -> (), "horizon_sync_insert_kernels");

    make_async_fn!(horizon_sync_spend_utxos(hash: Vec<HashOutput>) -> (), "horizon_sync_spend_utxos");

    //---------------------------------- Misc. --------------------------------------------//
    make_async_fn!(fetch_block_timestamps(start_hash: HashOutput) -> RollingVec<EpochTime>, "fetch_block_timestamps");

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

    pub fn insert_header(&mut self, header: BlockHeader) -> &mut Self {
        self.transaction.insert_header(header);
        self
    }

    /// Add the BlockHeader and contents of a `Block` (i.e. inputs, outputs and kernels) to the database.
    /// If the `BlockHeader` already exists, then just the contents are updated along with the relevant accumulated
    /// data.
    pub fn insert_block(&mut self, block: Arc<Block>) -> &mut Self {
        self.transaction.insert_block(block);
        self
    }

    pub fn set_metadata(&mut self, key: MetadataKey, value: MetadataValue) -> &mut Self {
        self.transaction.set_metadata(key, value);
        self
    }

    pub async fn commit(&mut self) -> Result<(), ChainStorageError> {
        let transaction = mem::take(&mut self.transaction);
        self.db.write(transaction).await
    }
}
