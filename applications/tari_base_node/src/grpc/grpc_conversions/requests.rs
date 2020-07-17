// Copyright 2020. The Tari Project
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

use crate::grpc::{
    blocks::{block_fees, block_heights, block_size, GET_BLOCKS_MAX_HEIGHTS, GET_BLOCKS_PAGE_SIZE},
    helpers::{mean, median},
    server::{base_node_grpc as grpc, base_node_grpc::*},
};
use prost_types::Timestamp;
use std::convert::{TryFrom, TryInto};
use tari_core::{
    base_node::{comms_interface::Broadcast, LocalNodeCommsInterface},
    blocks::{Block, BlockHeader, NewBlockHeaderTemplate, NewBlockTemplate},
    chain_storage::{ChainMetadata, HistoricalBlock},
    consensus::{
        emission::EmissionSchedule,
        ConsensusConstants,
        Network,
        KERNEL_WEIGHT,
        WEIGHT_PER_INPUT,
        WEIGHT_PER_OUTPUT,
    },
    proof_of_work::{Difficulty, PowAlgorithm, ProofOfWork},
    proto::utils::try_convert_all,
    transactions::{
        aggregated_body::AggregateBody,
        bullet_rangeproofs::BulletRangeProof,
        tari_amount::MicroTari,
        transaction::{
            KernelFeatures,
            OutputFeatures,
            OutputFlags,
            TransactionInput,
            TransactionKernel,
            TransactionOutput,
        },
        types::{BlindingFactor, Commitment, PrivateKey, PublicKey, Signature},
    },
};
use tari_crypto::tari_utilities::{epoch_time::EpochTime, ByteArray, Hashable};
use tonic::Status;

/// Utility function that converts a `chrono::DateTime` to a `prost::Timestamp`
fn datetime_to_timestamp(datetime: EpochTime) -> Timestamp {
    Timestamp {
        seconds: datetime.as_u64() as i64,
        nanos: 0,
    }
}

pub(crate) fn timestamp_to_datetime(timestamp: Timestamp) -> EpochTime {
    (timestamp.seconds as u64).into()
}

impl From<u64> for grpc::IntegerValue {
    fn from(value: u64) -> Self {
        Self { value }
    }
}

impl From<String> for grpc::StringValue {
    fn from(value: String) -> Self {
        Self { value }
    }
}

impl grpc::HeightRequest {
    pub async fn get_heights(&self, handler: LocalNodeCommsInterface) -> Result<Vec<u64>, Status> {
        block_heights(handler, self.start_height, self.end_height, self.from_tip).await
    }
}

impl From<grpc::BlockGroupRequest> for grpc::HeightRequest {
    fn from(b: BlockGroupRequest) -> Self {
        Self {
            from_tip: b.from_tip,
            start_height: b.start_height,
            end_height: b.end_height,
        }
    }
}

impl From<ConsensusConstants> for grpc::ConsensusConstants {
    fn from(cc: ConsensusConstants) -> Self {
        let (emission_initial, emission_decay, emission_tail) = cc.emission_amounts();
        Self {
            coinbase_lock_height: cc.coinbase_lock_height(),
            blockchain_version: cc.blockchain_version().into(),
            future_time_limit: cc.ftl().as_u64(),
            target_block_interval: cc.get_target_block_interval(),
            difficulty_block_window: cc.get_difficulty_block_window(),
            difficulty_max_block_interval: cc.get_difficulty_max_block_interval(),
            max_block_transaction_weight: cc.get_max_block_transaction_weight(),
            pow_algo_count: cc.get_pow_algo_count(),
            median_timestamp_count: u64::try_from(cc.get_median_timestamp_count()).unwrap_or(0),
            emission_initial: emission_initial.into(),
            emission_decay: emission_decay.into(),
            emission_tail: emission_tail.into(),
            min_blake_pow_difficulty: cc.min_pow_difficulty(PowAlgorithm::Blake).into(),
            block_weight_inputs: WEIGHT_PER_INPUT,
            block_weight_outputs: WEIGHT_PER_OUTPUT,
            block_weight_kernels: KERNEL_WEIGHT,
        }
    }
}

impl From<ChainMetadata> for grpc::MetaData {
    fn from(meta: ChainMetadata) -> Self {
        let diff = meta.accumulated_difficulty.unwrap_or_else(Difficulty::min);
        Self {
            height_of_longest_chain: meta.height_of_longest_chain.unwrap_or(0),
            best_block: meta.best_block.unwrap_or(vec![]),
            pruning_horizon: meta.pruning_horizon,
            accumulated_difficulty: diff.as_u64(),
        }
    }
}
