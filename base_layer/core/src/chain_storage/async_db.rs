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
    blocks::{Block, BlockHash, BlockHeader, NewBlockTemplate},
    chain_storage::{
        blockchain_database::BlockAddResult,
        metadata::ChainMetadata,
        BlockchainBackend,
        BlockchainDatabase,
        ChainStorageError,
        HistoricalBlock,
        InProgressHorizonSyncState,
        MmrTree,
    },
    proof_of_work::{Difficulty, PowAlgorithm},
    tari_utilities::epoch_time::EpochTime,
    transactions::{
        transaction::{TransactionKernel, TransactionOutput},
        types::{Commitment, HashOutput, Signature},
    },
};
use log::*;
use rand::{rngs::OsRng, RngCore};
use std::{sync::Arc, time::Instant};
use tari_mmr::{Hash, MerkleProof};

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

macro_rules! make_async {
    ($fn:ident() -> $rtype:ty, $name:expr) => {
        pub async fn $fn<T>(db: BlockchainDatabase<T>) -> Result<$rtype, ChainStorageError>
        where T: BlockchainBackend + 'static {
            tokio::task::spawn_blocking(move || {
                trace_log($name, move || db.$fn())
            })
            .await?
        }
    };

    ($fn:ident($($param:ident:$ptype:ty),+) -> $rtype:ty, $name:expr) => {
        pub async fn $fn<T>(db: BlockchainDatabase<T>, $($param: $ptype),+) -> Result<$rtype, ChainStorageError>
        where T: BlockchainBackend + 'static {
            tokio::task::spawn_blocking(move || {
                trace_log($name, move || db.$fn($($param),+))
            })
                .await?
        }
    };
}

//---------------------------------- Metadata --------------------------------------------//
make_async!(get_chain_metadata() -> ChainMetadata, "get_chain_metadata");
make_async!(set_chain_metadata(metadata: ChainMetadata) -> (), "set_chain_metadata");

//---------------------------------- Kernels --------------------------------------------//
make_async!(fetch_kernel(hash: HashOutput) -> TransactionKernel, "fetch_kernel");

//---------------------------------- TXO --------------------------------------------//
make_async!(fetch_stxo(hash: HashOutput) -> TransactionOutput, "fetch_stxo");
make_async!(fetch_txo(hash: HashOutput) -> Option<TransactionOutput>, "fetch_txo");
make_async!(fetch_utxo(hash: HashOutput) -> TransactionOutput, "fetch_utxo");
make_async!(spend_utxo(hash: HashOutput) -> (), "spend_utxo");
make_async!(insert_utxo(utxo: TransactionOutput) -> (), "insert_utxo");
make_async!(is_stxo(hash: HashOutput) -> bool, "is_stxo");
make_async!(is_utxo(hash: HashOutput) -> bool, "is_utxo");

//---------------------------------- Headers --------------------------------------------//
make_async!(fetch_header(block_num: u64) -> BlockHeader, "fetch_header");
make_async!(fetch_header_by_block_hash(hash: HashOutput) -> BlockHeader, "fetch_header_by_block_hash");
make_async!(fetch_tip_header() -> BlockHeader, "fetch_header");
make_async!(insert_valid_headers(headers: Vec<BlockHeader>) -> (), "insert_valid_headers");
make_async!(fetch_target_difficulties(pow_algo: PowAlgorithm, height: u64, block_window: usize) -> Vec<(EpochTime, Difficulty)>, "fetch_target_difficulties");

//---------------------------------- MMR --------------------------------------------//
make_async!(calculate_mmr_root(tree: MmrTree,additions: Vec<HashOutput>,deletions: Vec<HashOutput>) -> HashOutput, "calculate_mmr_root");
make_async!(calculate_mmr_roots(template: NewBlockTemplate) -> Block, "calculate_mmr_roots");
make_async!(fetch_mmr_node_count(tree: MmrTree, height: u64) -> u32, "fetch_mmr_node_count");
make_async!(fetch_mmr_nodes(tree: MmrTree, pos: u32, count: u32, hist_height:Option<u64>) -> Vec<(Vec<u8>, bool)>, "fetch_mmr_nodes");
make_async!(fetch_mmr_only_root(tree: MmrTree) -> HashOutput, "fetch_mmr_only_root");
make_async!(fetch_mmr_proof(tree: MmrTree, pos: usize) -> MerkleProof, "fetch_mmr_proof");
make_async!(fetch_mmr_root(tree: MmrTree) -> HashOutput, "fetch_mmr_root");
make_async!(insert_mmr_node(tree: MmrTree, hash: Hash, deleted: bool) -> (), "insert_mmr_node");
make_async!(validate_merkle_root(tree: MmrTree, height: u64) -> bool, "validate_merkle_root");
make_async!(rewind_to_height(height: u64) -> Vec<Arc<Block>>, "rewind_to_height");

//---------------------------------- Block --------------------------------------------//
make_async!(add_block(block: Arc<Block>) -> BlockAddResult, "add_block");
make_async!(cleanup_all_orphans() -> (), "cleanup_all_orphans");
make_async!(block_exists(block_hash: BlockHash) -> bool, "block_exists");
make_async!(fetch_block(height: u64) -> HistoricalBlock, "fetch_block");
make_async!(fetch_orphan(hash: HashOutput) -> Block, "fetch_orphan");
make_async!(fetch_block_with_hash(hash: HashOutput) -> Option<HistoricalBlock>, "fetch_block_with_hash");
make_async!(fetch_block_with_kernel(excess_sig: Signature) -> Option<HistoricalBlock>, "fetch_block_with_kernel");
make_async!(fetch_block_with_stxo(commitment: Commitment) -> Option<HistoricalBlock>, "fetch_block_with_stxo");
make_async!(fetch_block_with_utxo(commitment: Commitment) -> Option<HistoricalBlock>, "fetch_block_with_utxo");

//---------------------------------- Horizon Sync --------------------------------------------//
make_async!(get_horizon_sync_state() -> Option<InProgressHorizonSyncState>, "get_horizon_sync_state");
make_async!(set_horizon_sync_state(state: InProgressHorizonSyncState) -> (), "set_horizon_sync_state");
make_async!(horizon_sync_begin() -> InProgressHorizonSyncState, "horizon_sync_begin");
make_async!(horizon_sync_commit() -> (), "horizon_sync_commit");
make_async!(horizon_sync_rollback() -> (), "horizon_sync_rollback");
make_async!(horizon_sync_insert_kernels(kernels: Vec<TransactionKernel>) -> (), "horizon_sync_insert_kernels");
make_async!(horizon_sync_spend_utxos(hash: Vec<HashOutput>) -> (), "horizon_sync_spend_utxos");
make_async!(horizon_sync_create_mmr_checkpoint(tree: MmrTree) -> (), "horizon_sync_create_mmr_checkpoint");
