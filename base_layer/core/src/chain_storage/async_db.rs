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
    ops::RangeBounds,
    sync::{Arc, RwLock},
    time::Instant,
};

use log::*;
use primitive_types::U256;
use rand::{rngs::OsRng, RngCore};
use tari_common_types::{
    chain_metadata::ChainMetadata,
    types::{BlockHash, Commitment, HashOutput, PublicKey, Signature},
};
use tari_utilities::epoch_time::EpochTime;

use super::TemplateRegistrationEntry;
use crate::{
    blocks::{
        Block,
        BlockAccumulatedData,
        BlockHeader,
        BlockHeaderAccumulatedData,
        ChainBlock,
        ChainHeader,
        HistoricalBlock,
        NewBlockTemplate,
        UpdateBlockAccumulatedData,
    },
    chain_storage::{
        blockchain_database::MmrRoots,
        utxo_mined_info::{InputMinedInfo, OutputMinedInfo},
        BlockAddResult,
        BlockchainBackend,
        BlockchainDatabase,
        ChainStorageError,
        DbBasicStats,
        DbTotalSizeStats,
        DbTransaction,
        HorizonData,
        MmrTree,
        TargetDifficulties,
    },
    common::rolling_vec::RollingVec,
    proof_of_work::{PowAlgorithm, TargetDifficultyWindow},
    transactions::transaction_components::{OutputType, TransactionInput, TransactionKernel, TransactionOutput},
    OutputSmt,
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
        pub async fn $fn$(< $( $lt $( : $clt )? ),+ + Sync + Send + 'static >)?(&self, $($param: $ptype),+) -> Result<$rtype, ChainStorageError> {
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

    pub fn fetch_genesis_block(&self) -> ChainBlock {
        self.db.fetch_genesis_block()
    }
}

impl<B: BlockchainBackend + 'static> AsyncBlockchainDb<B> {
    make_async_fn!(write(transaction: DbTransaction) -> (), "write");

    //---------------------------------- Metadata --------------------------------------------//
    make_async_fn!(get_chain_metadata() -> ChainMetadata, "get_chain_metadata");

    make_async_fn!(fetch_horizon_data() -> HorizonData, "fetch_horizon_data");

    //---------------------------------- TXO --------------------------------------------//

    make_async_fn!(fetch_output(output_hash: HashOutput) -> Option<OutputMinedInfo>, "fetch_output");

    make_async_fn!(fetch_input(output_hash: HashOutput) -> Option<InputMinedInfo>, "fetch_input");

    make_async_fn!(fetch_unspent_output_hash_by_commitment(commitment: Commitment) -> Option<HashOutput>, "fetch_unspent_output_by_commitment");

    make_async_fn!(fetch_outputs_with_spend_status_at_tip(hashes: Vec<HashOutput>) -> Vec<Option<(TransactionOutput, bool)>>, "fetch_outputs_with_spend_status_at_tip");

    make_async_fn!(fetch_outputs_mined_info(hashes: Vec<HashOutput>) -> Vec<Option<OutputMinedInfo>>, "fetch_outputs_mined_info");

    make_async_fn!(fetch_inputs_mined_info(hashes: Vec<HashOutput>) -> Vec<Option<InputMinedInfo>>, "fetch_inputs_mined_info");

    make_async_fn!(fetch_outputs_in_block_with_spend_state(header_hash: HashOutput, spend_status_at_header: Option<HashOutput>) -> Vec<(TransactionOutput, bool)>, "fetch_outputs_in_block_with_spend_state");

    make_async_fn!(fetch_outputs_in_block(header_hash: HashOutput) -> Vec<TransactionOutput>, "fetch_outputs_in_block");

    make_async_fn!(fetch_inputs_in_block(header_hash: HashOutput) -> Vec<TransactionInput>, "fetch_inputs_in_block");

    make_async_fn!(utxo_count() -> usize, "utxo_count");

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

    make_async_fn!(chain_block_or_orphan_block_exists(block_hash: BlockHash) -> bool, "block_exists");

    make_async_fn!(chain_header_or_orphan_exists(block_hash: BlockHash) -> bool, "header_exists");

    make_async_fn!(bad_block_exists(block_hash: BlockHash) -> (bool, String), "bad_block_exists");

    make_async_fn!(add_bad_block(hash: BlockHash, height: u64, reason: String) -> (), "add_bad_block");

    make_async_fn!(fetch_block(height: u64, compact: bool) -> HistoricalBlock, "fetch_block");

    make_async_fn!(fetch_blocks<T: RangeBounds<u64>>(bounds: T, compact: bool) -> Vec<HistoricalBlock>, "fetch_blocks");

    make_async_fn!(fetch_orphan(hash: HashOutput) -> Block, "fetch_orphan");

    make_async_fn!(fetch_block_by_hash(hash: HashOutput, compact: bool) -> Option<HistoricalBlock>, "fetch_block_by_hash");

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

    make_async_fn!(get_stats() -> DbBasicStats, "get_stats");

    make_async_fn!(fetch_total_size_stats() -> DbTotalSizeStats, "fetch_total_size_stats");

    make_async_fn!(fetch_active_validator_nodes(height: u64) -> Vec<(PublicKey, [u8;32])>, "fetch_active_validator_nodes");

    make_async_fn!(get_shard_key(height:u64, public_key: PublicKey) -> Option<[u8;32]>, "get_shard_key");

    make_async_fn!(fetch_template_registrations<T: RangeBounds<u64>>(range: T) -> Vec<TemplateRegistrationEntry>, "fetch_template_registrations");

    make_async_fn!(swap_to_highest_pow_chain() -> (), "swap to highest proof-of-work chain");
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
        accumulated_difficulty: U256,
        expected_prev_best_block: HashOutput,
        timestamp: u64,
    ) -> &mut Self {
        self.transaction.set_best_block(
            height,
            hash,
            accumulated_difficulty,
            expected_prev_best_block,
            timestamp,
        );
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
        mmr_position: u64,
    ) -> &mut Self {
        self.transaction.insert_kernel(kernel, header_hash, mmr_position);
        self
    }

    pub fn insert_output_via_horizon_sync(
        &mut self,
        output: TransactionOutput,
        header_hash: HashOutput,
        header_height: u64,
        timestamp: u64,
    ) -> &mut Self {
        self.transaction
            .insert_utxo(output, header_hash, header_height, timestamp);
        self
    }

    pub fn prune_output_from_all_dbs(
        &mut self,
        output_hash: HashOutput,
        commitment: Commitment,
        output_type: OutputType,
    ) -> &mut Self {
        self.transaction
            .prune_output_from_all_dbs(output_hash, commitment, output_type);
        self
    }

    pub fn delete_all_kernerls_in_block(&mut self, block_hash: BlockHash) -> &mut Self {
        self.transaction.delete_all_kernerls_in_block(block_hash);
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

    pub fn insert_chain_header(&mut self, chain_header: ChainHeader) -> &mut Self {
        self.transaction.insert_chain_header(chain_header);
        self
    }

    pub fn insert_tip_block_body(&mut self, block: Arc<ChainBlock>, smt: Arc<RwLock<OutputSmt>>) -> &mut Self {
        self.transaction.insert_tip_block_body(block, smt);
        self
    }

    pub fn delete_orphan(&mut self, hash: HashOutput) -> &mut Self {
        self.transaction.delete_orphan(hash);
        self
    }

    pub fn insert_bad_block(&mut self, hash: HashOutput, height: u64, reason: String) -> &mut Self {
        self.transaction.insert_bad_block(hash, height, reason);
        self
    }

    pub async fn commit(&mut self) -> Result<(), ChainStorageError> {
        let transaction = mem::take(&mut self.transaction);
        self.db.write(transaction).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::blockchain::{create_new_blockchain, TempDatabase};

    impl AsyncBlockchainDb<TempDatabase> {
        pub fn sample() -> Self {
            Self {
                db: create_new_blockchain(),
            }
        }
    }

    #[tokio::test]
    async fn coverage_async_blockchain_db() {
        let obj = AsyncBlockchainDb::sample();
        obj.clone().into_inner();
        obj.fetch_horizon_data().await.unwrap();
        obj.fetch_chain_header(0).await.unwrap();
        obj.fetch_last_header().await.unwrap();
        obj.clear_all_pending_headers().await.unwrap();
        obj.fetch_last_chain_header().await.unwrap();
        obj.cleanup_orphans().await.unwrap();
        obj.cleanup_all_orphans().await.unwrap();
        obj.prune_to_height(0).await.unwrap();
        obj.get_stats().await.unwrap();
        obj.fetch_total_size_stats().await.unwrap();
        let _trans = obj.write_transaction();
    }
}
