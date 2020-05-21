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
use crate::grpc::{
    blocks::{block_fees, block_heights, block_size, GET_BLOCKS_MAX_HEIGHTS, GET_BLOCKS_PAGE_SIZE},
    helpers::{mean, median},
};
use base_node_grpc::*;
use log::*;
use prost_types::Timestamp;
use std::{cmp, convert::TryFrom};
use tari_common::GlobalConfig;
use tari_core::{
    base_node::LocalNodeCommsInterface,
    blocks::{Block, BlockHeader},
    chain_storage::HistoricalBlock,
    consensus::{emission::EmissionSchedule, ConsensusConstants, Network},
    proof_of_work::PowAlgorithm,
};
use tari_crypto::tari_utilities::{epoch_time::EpochTime, ByteArray, Hashable};
use tokio::{runtime, sync::mpsc};
use tonic::{Request, Response, Status};

const VERSION: &'static str = env!("CARGO_PKG_VERSION");

const LOG_TARGET: &str = "base_node::grpc";
// The maximum number of difficulty ints that can be requested at a time. These will be streamed to the
// client, so memory is not really a concern here, but a malicious client could request a large
// number here to keep the node busy
const GET_DIFFICULTY_MAX_HEIGHTS: usize = 10_000;
const GET_DIFFICULTY_PAGE_SIZE: usize = 1_000;
// The maximum number of headers a client can request at a time. If the client requests more than
// this, this is the maximum that will be returned.
const LIST_HEADERS_MAX_NUM_HEADERS: u64 = 10_000;
// The number of headers to request via the local interface at a time. These are then streamed to
// client.
const LIST_HEADERS_PAGE_SIZE: usize = 10;
// The `num_headers` value if none is provided.
const LIST_HEADERS_DEFAULT_NUM_HEADERS: u64 = 10;

pub(crate) mod base_node_grpc {
    tonic::include_proto!("tari.base_node");
}

pub struct BaseNodeGrpcServer {
    executor: runtime::Handle,
    node_service: LocalNodeCommsInterface,
    node_config: GlobalConfig,
}

impl BaseNodeGrpcServer {
    pub fn new(executor: runtime::Handle, local_node: LocalNodeCommsInterface, node_config: GlobalConfig) -> Self {
        Self {
            executor,
            node_service: local_node,
            node_config,
        }
    }
}

#[tonic::async_trait]
impl base_node_grpc::base_node_server::BaseNode for BaseNodeGrpcServer {
    type GetBlocksStream = mpsc::Receiver<Result<base_node_grpc::HistoricalBlock, Status>>;
    type GetNetworkDifficultyStream = mpsc::Receiver<Result<base_node_grpc::NetworkDifficultyResponse, Status>>;
    type ListHeadersStream = mpsc::Receiver<Result<base_node_grpc::BlockHeader, Status>>;

    async fn get_network_difficulty(
        &self,
        request: Request<HeightRequest>,
    ) -> Result<Response<Self::GetNetworkDifficultyStream>, Status>
    {
        let request = request.into_inner();
        debug!(
            target: LOG_TARGET,
            "Incoming GRPC request for GetNetworkDifficulty: from_tip: {:?} start_height: {:?} end_height: {:?}",
            request.from_tip,
            request.start_height,
            request.end_height
        );
        let mut handler = self.node_service.clone();
        let mut heights: Vec<u64> = request.get_heights(handler.clone()).await?;
        heights = heights
            .drain(..cmp::min(heights.len(), GET_DIFFICULTY_MAX_HEIGHTS))
            .collect();
        let (mut tx, rx) = mpsc::channel(GET_DIFFICULTY_MAX_HEIGHTS);

        self.executor.spawn(async move {
            let mut page: Vec<u64> = heights
                .drain(..cmp::min(heights.len(), GET_DIFFICULTY_PAGE_SIZE))
                .collect();
            while page.len() > 0 {
                let difficulties = match handler.get_headers(page.clone()).await {
                    Err(err) => {
                        warn!(
                            target: LOG_TARGET,
                            "Error communicating with local base node: {:?}", err,
                        );
                        return;
                    },
                    Ok(data) => {
                        let mut iter = data.iter().peekable();
                        let mut result = Vec::new();
                        while let Some(next) = iter.next() {
                            let current_difficulty = next.pow.accumulated_blake_difficulty.as_u64();
                            let current_timestamp = next.timestamp.as_u64();
                            let current_height = next.height;
                            let estimated_hash_rate = if let Some(peek) = iter.peek() {
                                let peeked_timestamp = peek.timestamp.as_u64();
                                let estimated_hash_rate = current_difficulty / (current_timestamp - peeked_timestamp);
                                estimated_hash_rate
                            } else {
                                0
                            };

                            result.push((current_height, current_difficulty, estimated_hash_rate))
                        }
                        result
                    },
                };

                let result_size = difficulties.len();
                for difficulty in difficulties {
                    match tx
                        .send(Ok({
                            NetworkDifficultyResponse {
                                height: difficulty.0,
                                difficulty: difficulty.1,
                                estimated_hash_rate: difficulty.2,
                            }
                        }))
                        .await
                    {
                        Ok(_) => (),
                        Err(err) => {
                            warn!(target: LOG_TARGET, "Error sending difficulty via GRPC:  {}", err);
                            match tx.send(Err(Status::unknown("Error sending data"))).await {
                                Ok(_) => (),
                                Err(send_err) => {
                                    warn!(target: LOG_TARGET, "Error sending error to GRPC client: {}", send_err)
                                },
                            }
                            return;
                        },
                    }
                }
                if result_size < GET_DIFFICULTY_PAGE_SIZE {
                    break;
                }
                page = heights
                    .drain(..cmp::min(heights.len(), GET_DIFFICULTY_PAGE_SIZE))
                    .collect();
            }
        });

        debug!(
            target: LOG_TARGET,
            "Sending GetNetworkDifficulty response stream to client"
        );
        Ok(Response::new(rx))
    }

    async fn list_headers(
        &self,
        request: Request<ListHeadersRequest>,
    ) -> Result<Response<Self::ListHeadersStream>, Status>
    {
        let request = request.into_inner();
        debug!(
            target: LOG_TARGET,
            "Incoming GRPC request for ListHeaders: from_height: {}, num_headers:{}, sorting:{}",
            request.from_height,
            request.num_headers,
            request.sorting
        );

        let mut handler = self.node_service.clone();
        let tip = match handler.get_metadata().await {
            Err(err) => {
                warn!(target: LOG_TARGET, "Error communicating with base node: {}", err,);
                return Err(Status::internal(err.to_string()));
            },
            Ok(data) => data.height_of_longest_chain.unwrap_or(0),
        };

        let sorting: Sorting = request.sorting();
        let num_headers = match request.num_headers {
            0 => LIST_HEADERS_DEFAULT_NUM_HEADERS,
            _ => request.num_headers,
        };

        let num_headers = cmp::min(num_headers, LIST_HEADERS_MAX_NUM_HEADERS);
        let (mut tx, rx) = mpsc::channel(LIST_HEADERS_PAGE_SIZE);

        let headers: Vec<u64> = if request.from_height != 0 {
            match sorting {
                Sorting::Desc => ((cmp::max(0, request.from_height as i64 - num_headers as i64 + 1) as u64)..=
                    request.from_height)
                    .rev()
                    .collect(),
                Sorting::Asc => (request.from_height..(request.from_height + num_headers)).collect(),
            }
        } else {
            match sorting {
                Sorting::Desc => ((cmp::max(0, tip as i64 - num_headers as i64 + 1) as u64)..=tip)
                    .rev()
                    .collect(),
                Sorting::Asc => (0..num_headers).collect(),
            }
        };

        self.executor.spawn(async move {
            trace!(target: LOG_TARGET, "Starting base node request");
            let mut headers = headers;
            trace!(target: LOG_TARGET, "Headers:{:?}", headers);
            let mut page: Vec<u64> = headers
                .drain(..cmp::min(headers.len(), LIST_HEADERS_PAGE_SIZE))
                .collect();
            while page.len() > 0 {
                trace!(target: LOG_TARGET, "Page: {:?}", page);
                let result_headers = match handler.get_headers(page).await {
                    Err(err) => {
                        warn!(target: LOG_TARGET, "Error communicating with base node: {}", err,);
                        return;
                    },
                    Ok(data) => data,
                };
                trace!(target: LOG_TARGET, "Result headers: {}", result_headers.len());
                let result_size = result_headers.len();

                for header in result_headers {
                    trace!(target: LOG_TARGET, "Sending block header: {}", header.height);
                    match tx.send(Ok(header.into())).await {
                        Ok(_) => (),
                        Err(err) => {
                            warn!(target: LOG_TARGET, "Error sending block header via GRPC:  {}", err);
                            match tx.send(Err(Status::unknown("Error sending data"))).await {
                                Ok(_) => (),
                                Err(send_err) => {
                                    warn!(target: LOG_TARGET, "Error sending error to GRPC client: {}", send_err)
                                },
                            }
                            return;
                        },
                    }
                }
                if result_size < LIST_HEADERS_PAGE_SIZE {
                    break;
                }
                page = headers
                    .drain(..cmp::min(headers.len(), LIST_HEADERS_PAGE_SIZE))
                    .collect();
            }
        });

        debug!(target: LOG_TARGET, "Sending ListHeaders response stream to client");
        Ok(Response::new(rx))
    }

    async fn get_blocks(&self, request: Request<GetBlocksRequest>) -> Result<Response<Self::GetBlocksStream>, Status> {
        let request = request.into_inner();
        debug!(
            target: LOG_TARGET,
            "Incoming GRPC request for GetBlocks: {:?}", request.heights
        );
        let mut heights = request.heights;
        heights = heights
            .drain(..cmp::min(heights.len(), GET_BLOCKS_MAX_HEIGHTS))
            .collect();

        let mut handler = self.node_service.clone();
        let (mut tx, rx) = mpsc::channel(GET_BLOCKS_PAGE_SIZE);
        self.executor.spawn(async move {
            let mut page: Vec<u64> = heights.drain(..cmp::min(heights.len(), GET_BLOCKS_PAGE_SIZE)).collect();

            while page.len() > 0 {
                let blocks = match handler.get_blocks(page.clone()).await {
                    Err(err) => {
                        warn!(
                            target: LOG_TARGET,
                            "Error communicating with local base node: {:?}", err,
                        );
                        return;
                    },
                    Ok(data) => data,
                };
                let result_size = blocks.len();
                for block in blocks {
                    match tx.send(Ok(block.into())).await {
                        Ok(_) => (),
                        Err(err) => {
                            warn!(target: LOG_TARGET, "Error sending header via GRPC:  {}", err);
                            match tx.send(Err(Status::unknown("Error sending data"))).await {
                                Ok(_) => (),
                                Err(send_err) => {
                                    warn!(target: LOG_TARGET, "Error sending error to GRPC client: {}", send_err)
                                },
                            }
                            return;
                        },
                    }
                }
                if result_size < GET_BLOCKS_PAGE_SIZE {
                    break;
                }
                page = heights.drain(..cmp::min(heights.len(), GET_BLOCKS_PAGE_SIZE)).collect();
            }
        });

        debug!(target: LOG_TARGET, "Sending GetBlocks response stream to client");
        Ok(Response::new(rx))
    }

    async fn get_calc_timing(&self, request: Request<HeightRequest>) -> Result<Response<CalcTimingResponse>, Status> {
        let request = request.into_inner();
        debug!(
            target: LOG_TARGET,
            "Incoming GRPC request for GetCalcTiming: from_tip: {:?} start_height: {:?} end_height: {:?}",
            request.from_tip,
            request.start_height,
            request.end_height
        );

        let mut handler = self.node_service.clone();
        let heights: Vec<u64> = request.get_heights(handler.clone()).await?;

        let headers = match handler.get_headers(heights).await {
            Ok(headers) => headers,
            Err(err) => {
                warn!(target: LOG_TARGET, "Error getting headers for GRPC client: {}", err);
                Vec::new()
            },
        };
        let (max, min, avg) = BlockHeader::timing_stats(&headers);

        let response: base_node_grpc::CalcTimingResponse = base_node_grpc::CalcTimingResponse { max, min, avg };
        debug!(target: LOG_TARGET, "Sending GetCalcTiming response to client");
        Ok(Response::new(response))
    }

    async fn get_constants(
        &self,
        _request: Request<base_node_grpc::Empty>,
    ) -> Result<Response<base_node_grpc::ConsensusConstants>, Status>
    {
        debug!(target: LOG_TARGET, "Incoming GRPC request for GetConstants",);
        let network: Network = self.node_config.network.into();
        debug!(target: LOG_TARGET, "Sending GetConstants response to client");
        Ok(Response::new(network.create_consensus_constants().into()))
    }

    async fn get_block_size(
        &self,
        request: Request<BlockGroupRequest>,
    ) -> Result<Response<BlockGroupResponse>, Status>
    {
        get_block_group(self.node_service.clone(), request, BlockGroupType::BlockSize).await
    }

    async fn get_block_fees(
        &self,
        request: Request<BlockGroupRequest>,
    ) -> Result<Response<BlockGroupResponse>, Status>
    {
        get_block_group(self.node_service.clone(), request, BlockGroupType::BlockFees).await
    }

    async fn get_version(&self, _request: Request<base_node_grpc::Empty>) -> Result<Response<StringValue>, Status> {
        Ok(Response::new(VERSION.to_string().into()))
    }

    async fn get_tokens_in_circulation(
        &self,
        request: Request<base_node_grpc::IntegerValue>,
    ) -> Result<Response<IntegerValue>, Status>
    {
        debug!(target: LOG_TARGET, "Incoming GRPC request for GetTokensInCirculation",);
        let request = request.into_inner();
        let network: Network = self.node_config.network.into();
        let constants = network.create_consensus_constants();
        let (initial, decay, tail) = constants.emission_amounts();
        let schedule = EmissionSchedule::new(initial, decay, tail);
        let value: u64 = schedule.supply_at_block(request.value).into();
        debug!(
            target: LOG_TARGET,
            "Sending GetTokensInCirculation response {} to client", value
        );
        Ok(Response::new(IntegerValue { value }))
    }
}

enum BlockGroupType {
    BlockFees,
    BlockSize,
}
async fn get_block_group(
    mut handler: LocalNodeCommsInterface,
    request: Request<BlockGroupRequest>,
    block_group_type: BlockGroupType,
) -> Result<Response<BlockGroupResponse>, Status>
{
    let request = request.into_inner();
    let calc_type_response = request.calc_type;
    let calc_type: CalcType = request.calc_type();
    let height_request: HeightRequest = request.into();

    debug!(
        target: LOG_TARGET,
        "Incoming GRPC request for GetBlockSize: from_tip: {:?} start_height: {:?} end_height: {:?}",
        height_request.from_tip,
        height_request.start_height,
        height_request.end_height
    );

    let heights = height_request.get_heights(handler.clone()).await?;

    let blocks = match handler.get_blocks(heights).await {
        Err(err) => {
            warn!(
                target: LOG_TARGET,
                "Error communicating with local base node: {:?}", err,
            );
            vec![]
        },
        Ok(data) => data,
    };
    let extractor = match block_group_type {
        BlockGroupType::BlockFees => block_fees,
        BlockGroupType::BlockSize => block_size,
    };
    let values = blocks.iter().map(extractor).collect::<Vec<u64>>();
    let value = match calc_type {
        CalcType::Median => median(values).map(|v| vec![v]),
        CalcType::Mean => mean(values).map(|v| vec![v]),
        CalcType::Quantile => return Err(Status::unimplemented("Quantile has not been implemented")),
        CalcType::Quartile => return Err(Status::unimplemented("Quartile has not been implemented")),
        _ => median(values).map(|v| vec![v]),
    }
    .unwrap_or(vec![]);
    debug!(
        target: LOG_TARGET,
        "Sending GetBlockSize response to client: {:?}", value
    );
    Ok(Response::new(BlockGroupResponse {
        value,
        calc_type: calc_type_response,
    }))
}
/// Utility function that converts a `chrono::DateTime` to a `prost::Timestamp`
fn datetime_to_timestamp(datetime: EpochTime) -> Timestamp {
    Timestamp {
        seconds: datetime.as_u64() as i64,
        nanos: 0,
    }
}

impl From<u64> for base_node_grpc::IntegerValue {
    fn from(value: u64) -> Self {
        Self { value }
    }
}

impl From<String> for base_node_grpc::StringValue {
    fn from(value: String) -> Self {
        Self { value }
    }
}

impl base_node_grpc::HeightRequest {
    pub async fn get_heights(&self, handler: LocalNodeCommsInterface) -> Result<Vec<u64>, Status> {
        block_heights(handler, self.start_height, self.end_height, self.from_tip).await
    }
}

impl From<base_node_grpc::BlockGroupRequest> for base_node_grpc::HeightRequest {
    fn from(b: BlockGroupRequest) -> Self {
        Self {
            from_tip: b.from_tip,
            start_height: b.start_height,
            end_height: b.end_height,
        }
    }
}

impl From<ConsensusConstants> for base_node_grpc::ConsensusConstants {
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
        }
    }
}
impl From<HistoricalBlock> for base_node_grpc::HistoricalBlock {
    fn from(hb: HistoricalBlock) -> Self {
        Self {
            confirmations: hb.confirmations,
            spent_commitments: hb.spent_commitments.iter().map(|c| Vec::from(c.as_bytes())).collect(),
            block: Some(hb.block.into()),
        }
    }
}

impl From<tari_core::blocks::Block> for base_node_grpc::Block {
    fn from(block: Block) -> Self {
        Self {
            body: Some(base_node_grpc::AggregateBody {
                inputs: block
                    .body
                    .inputs()
                    .iter()
                    .map(|input| base_node_grpc::TransactionInput {
                        features: Some(base_node_grpc::OutputFeatures {
                            flags: input.features.flags.bits() as u32,
                            maturity: input.features.maturity,
                        }),
                        commitment: Vec::from(input.commitment.as_bytes()),
                    })
                    .collect(),
                outputs: block
                    .body
                    .outputs()
                    .iter()
                    .map(|output| base_node_grpc::TransactionOutput {
                        features: Some(base_node_grpc::OutputFeatures {
                            flags: output.features.flags.bits() as u32,
                            maturity: output.features.maturity,
                        }),
                        commitment: Vec::from(output.commitment.as_bytes()),
                        range_proof: Vec::from(output.proof.as_bytes()),
                    })
                    .collect(),
                kernels: block
                    .body
                    .kernels()
                    .iter()
                    .map(|kernel| base_node_grpc::TransactionKernel {
                        features: kernel.features.bits() as u32,
                        fee: kernel.fee.0,
                        lock_height: kernel.lock_height,
                        meta_info: kernel.meta_info.as_ref().map(|info| info.clone()).unwrap_or(vec![]),
                        linked_kernel: kernel.linked_kernel.as_ref().map(|link| link.clone()).unwrap_or(vec![]),
                        excess: Vec::from(kernel.excess.as_bytes()),
                        excess_sig: Some(base_node_grpc::Signature {
                            public_nonce: Vec::from(kernel.excess_sig.get_public_nonce().as_bytes()),
                            signature: Vec::from(kernel.excess_sig.get_signature().as_bytes()),
                        }),
                    })
                    .collect(),
            }),
            header: Some(block.header.into()),
        }
    }
}

impl From<BlockHeader> for base_node_grpc::BlockHeader {
    fn from(h: BlockHeader) -> Self {
        Self {
            hash: h.hash(),
            version: h.version as u32,
            height: h.height,
            prev_hash: h.prev_hash.clone(),
            timestamp: Some(datetime_to_timestamp(h.timestamp)),
            output_mr: h.output_mr.clone(),
            range_proof_mr: h.range_proof_mr.clone(),
            kernel_mr: h.kernel_mr.clone(),
            total_kernel_offset: Vec::from(h.total_kernel_offset.as_bytes()),
            nonce: h.nonce,
            pow: Some(base_node_grpc::ProofOfWork {
                pow_algo: match h.pow.pow_algo {
                    PowAlgorithm::Monero => 0,
                    PowAlgorithm::Blake => 1,
                },
                accumulated_monero_difficulty: h.pow.accumulated_monero_difficulty.into(),
                accumulated_blake_difficulty: h.pow.accumulated_blake_difficulty.into(),
                pow_data: h.pow.pow_data,
            }),
        }
    }
}
