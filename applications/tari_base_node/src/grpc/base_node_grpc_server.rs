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

use log::*;
use std::{cmp, convert::TryInto};
use tari_common::GlobalConfig;
use tari_core::{
    base_node::{comms_interface::Broadcast, LocalNodeCommsInterface},
    blocks::{Block, BlockHeader, NewBlockTemplate},
    consensus::{emission::EmissionSchedule, ConsensusManagerBuilder, Network},
    proof_of_work::PowAlgorithm,
};

use tari_app_grpc::{
    tari_rpc,
    tari_rpc::{CalcType, Sorting},
};
use tari_crypto::tari_utilities::Hashable;
use tokio::{runtime, sync::mpsc};
use tonic::{Request, Response, Status};

const VERSION: &str = env!("CARGO_PKG_VERSION");

const LOG_TARGET: &str = "base_node::grpc";
const GET_TOKENS_IN_CIRCULATION_MAX_HEIGHTS: usize = 1_000_000;
const GET_TOKENS_IN_CIRCULATION_PAGE_SIZE: usize = 1_000;
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

pub async fn get_heights(
    request: &tari_rpc::HeightRequest,
    handler: LocalNodeCommsInterface,
) -> Result<Vec<u64>, Status>
{
    block_heights(handler, request.start_height, request.end_height, request.from_tip).await
}

#[tonic::async_trait]
impl tari_rpc::base_node_server::BaseNode for BaseNodeGrpcServer {
    type GetBlocksStream = mpsc::Receiver<Result<tari_rpc::HistoricalBlock, Status>>;
    type GetNetworkDifficultyStream = mpsc::Receiver<Result<tari_rpc::NetworkDifficultyResponse, Status>>;
    type GetTokensInCirculationStream = mpsc::Receiver<Result<tari_rpc::ValueAtHeightResponse, Status>>;
    type ListHeadersStream = mpsc::Receiver<Result<tari_rpc::BlockHeader, Status>>;
    type SearchKernelsStream = mpsc::Receiver<Result<tari_rpc::HistoricalBlock, Status>>;

    async fn get_network_difficulty(
        &self,
        request: Request<tari_rpc::HeightRequest>,
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
        let mut heights: Vec<u64> = get_heights(&request, handler.clone()).await?;
        heights = heights
            .drain(..cmp::min(heights.len(), GET_DIFFICULTY_MAX_HEIGHTS))
            .collect();
        let (mut tx, rx) = mpsc::channel(GET_DIFFICULTY_MAX_HEIGHTS);

        self.executor.spawn(async move {
            let mut page: Vec<u64> = heights
                .drain(..cmp::min(heights.len(), GET_DIFFICULTY_PAGE_SIZE))
                .collect();
            while !page.is_empty() {
                let mut difficulties = match handler.get_headers(page.clone()).await {
                    Err(err) => {
                        warn!(
                            target: LOG_TARGET,
                            "Error communicating with local base node: {:?}", err,
                        );
                        return;
                    },
                    Ok(mut data) => {
                        data.sort_by(|a, b| a.height.cmp(&b.height));
                        let mut iter = data.iter().peekable();
                        let mut result = Vec::new();
                        while let Some(next) = iter.next() {
                            let current_difficulty = next.pow.target_difficulty.as_u64();
                            let current_timestamp = next.timestamp.as_u64();
                            let current_height = next.height;
                            let estimated_hash_rate = if let Some(peek) = iter.peek() {
                                let peeked_timestamp = peek.timestamp.as_u64();
                                // Sometimes blocks can have the same timestamp, lucky miner and some clock drift.
                                if peeked_timestamp > current_timestamp {
                                    current_difficulty / (peeked_timestamp - current_timestamp)
                                } else {
                                    0
                                }
                            } else {
                                0
                            };

                            result.push((
                                current_height,
                                current_difficulty,
                                estimated_hash_rate,
                                current_timestamp,
                            ))
                        }

                        result
                    },
                };
                difficulties.sort_by(|a, b| b.0.cmp(&a.0));
                let result_size = difficulties.len();
                for difficulty in difficulties {
                    match tx
                        .send(Ok({
                            tari_rpc::NetworkDifficultyResponse {
                                height: difficulty.0,
                                difficulty: difficulty.1,
                                estimated_hash_rate: difficulty.2,
                                timestamp: difficulty.3,
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
        request: Request<tari_rpc::ListHeadersRequest>,
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
            while !page.is_empty() {
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

    async fn get_new_block_template(
        &self,
        request: Request<tari_rpc::PowAlgo>,
    ) -> Result<Response<tari_rpc::NewBlockTemplateResponse>, Status>
    {
        let request = request.into_inner();
        debug!(target: LOG_TARGET, "Incoming GRPC request for get new block template");
        let algo: PowAlgorithm = (request.pow_algo as u64)
            .try_into()
            .map_err(|_| Status::invalid_argument("No valid pow algo selected".to_string()))?;
        let mut handler = self.node_service.clone();

        let new_template = handler
            .get_new_block_template(algo)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let height = new_template.header.height;

        let cm = ConsensusManagerBuilder::new(self.node_config.network.into()).build();

        let response = tari_rpc::NewBlockTemplateResponse {
            new_block_template: Some(new_template.into()),
            block_reward: cm.emission_schedule().block_reward(height).0,
        };

        debug!(target: LOG_TARGET, "Sending GetNewBlockTemplate response to client");
        Ok(Response::new(response))
    }

    async fn get_new_block(
        &self,
        request: Request<tari_rpc::NewBlockTemplate>,
    ) -> Result<Response<tari_rpc::GetNewBlockResult>, Status>
    {
        let request = request.into_inner();
        debug!(target: LOG_TARGET, "Incoming GRPC request for get new block");
        let block_template: NewBlockTemplate = request
            .try_into()
            .map_err(|_| Status::internal("Failed to convert arguments".to_string()))?;

        let mut handler = self.node_service.clone();

        let new_block = handler
            .get_new_block(block_template)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        // construct response
        let cm = ConsensusManagerBuilder::new(self.node_config.network.into()).build();
        let block_hash = new_block.hash();
        let mining_hash = new_block.header.merged_mining_hash();
        let pow = match new_block.header.pow.pow_algo {
            PowAlgorithm::Monero => 0,
            PowAlgorithm::Blake => 1,
        };
        let target_difficulty = new_block.header.pow.target_difficulty;
        let reward = cm.calculate_coinbase_and_fees(&new_block);
        let block: Option<tari_rpc::Block> = Some(new_block.into());
        let mining_data = Some(tari_rpc::MinerData {
            algo: Some(tari_rpc::PowAlgo { pow_algo: pow }),
            target_difficulty: target_difficulty.as_u64(),
            reward: reward.0,
            mergemining_hash: mining_hash,
        });
        let response = tari_rpc::GetNewBlockResult {
            block_hash,
            block,
            mining_data,
        };
        debug!(target: LOG_TARGET, "Sending GetNewBlock response to client");
        Ok(Response::new(response))
    }

    async fn submit_block(&self, request: Request<tari_rpc::Block>) -> Result<Response<tari_rpc::Empty>, Status> {
        let request = request.into_inner();
        let block: Block = request
            .try_into()
            .map_err(|_| Status::internal("Failed to convert arguments".to_string()))?;

        let mut handler = self.node_service.clone();
        handler
            .submit_block(block, Broadcast::from(true))
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        let response: tari_rpc::Empty = tari_rpc::Empty {};

        debug!(target: LOG_TARGET, "Sending SubmitBlock response to client");
        Ok(Response::new(response))
    }

    async fn get_blocks(
        &self,
        request: Request<tari_rpc::GetBlocksRequest>,
    ) -> Result<Response<Self::GetBlocksStream>, Status>
    {
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

            while !page.is_empty() {
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

    async fn get_tip_info(
        &self,
        _request: Request<tari_rpc::Empty>,
    ) -> Result<Response<tari_rpc::TipInfoResponse>, Status>
    {
        debug!(target: LOG_TARGET, "Incoming GRPC request for BN tip data");

        let mut handler = self.node_service.clone();

        let meta = handler
            .get_metadata()
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let response = tari_rpc::TipInfoResponse {
            metadata: Some(meta.into()),
        };

        debug!(target: LOG_TARGET, "Sending MetaData response to client");
        Ok(Response::new(response))
    }

    async fn search_kernels(
        &self,
        request: Request<tari_rpc::SearchKernelsRequest>,
    ) -> Result<Response<Self::SearchKernelsStream>, Status>
    {
        debug!(target: LOG_TARGET, "Incoming GRPC request for SearchKernels");
        let request = request.into_inner();

        let converted: Result<Vec<_>, _> = request.signatures.into_iter().map(|s| s.try_into()).collect();
        let kernels = converted.map_err(|_| Status::internal("Failed to convert one or more arguments."))?;

        let mut handler = self.node_service.clone();

        let (mut tx, rx) = mpsc::channel(GET_BLOCKS_PAGE_SIZE);
        self.executor.spawn(async move {
            let blocks = match handler.get_blocks_with_kernels(kernels).await {
                Err(err) => {
                    warn!(
                        target: LOG_TARGET,
                        "Error communicating with local base node: {:?}", err,
                    );
                    return;
                },
                Ok(data) => data,
            };
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
        });

        debug!(target: LOG_TARGET, "Sending SearchKernels response stream to client");
        Ok(Response::new(rx))
    }

    async fn get_calc_timing(
        &self,
        request: Request<tari_rpc::HeightRequest>,
    ) -> Result<Response<tari_rpc::CalcTimingResponse>, Status>
    {
        let request = request.into_inner();
        debug!(
            target: LOG_TARGET,
            "Incoming GRPC request for GetCalcTiming: from_tip: {:?} start_height: {:?} end_height: {:?}",
            request.from_tip,
            request.start_height,
            request.end_height
        );

        let mut handler = self.node_service.clone();
        let heights: Vec<u64> = get_heights(&request, handler.clone()).await?;

        let headers = match handler.get_headers(heights).await {
            Ok(headers) => headers,
            Err(err) => {
                warn!(target: LOG_TARGET, "Error getting headers for GRPC client: {}", err);
                Vec::new()
            },
        };
        let (max, min, avg) = BlockHeader::timing_stats(&headers);

        let response: tari_rpc::CalcTimingResponse = tari_rpc::CalcTimingResponse { max, min, avg };
        debug!(target: LOG_TARGET, "Sending GetCalcTiming response to client");
        Ok(Response::new(response))
    }

    async fn get_constants(
        &self,
        _request: Request<tari_rpc::Empty>,
    ) -> Result<Response<tari_rpc::ConsensusConstants>, Status>
    {
        debug!(target: LOG_TARGET, "Incoming GRPC request for GetConstants",);
        let network: Network = self.node_config.network.into();
        debug!(target: LOG_TARGET, "Sending GetConstants response to client");
        Ok(Response::new(network.create_consensus_constants().into()))
    }

    async fn get_block_size(
        &self,
        request: Request<tari_rpc::BlockGroupRequest>,
    ) -> Result<Response<tari_rpc::BlockGroupResponse>, Status>
    {
        get_block_group(self.node_service.clone(), request, BlockGroupType::BlockSize).await
    }

    async fn get_block_fees(
        &self,
        request: Request<tari_rpc::BlockGroupRequest>,
    ) -> Result<Response<tari_rpc::BlockGroupResponse>, Status>
    {
        get_block_group(self.node_service.clone(), request, BlockGroupType::BlockFees).await
    }

    async fn get_version(&self, _request: Request<tari_rpc::Empty>) -> Result<Response<tari_rpc::StringValue>, Status> {
        Ok(Response::new(VERSION.to_string().into()))
    }

    async fn get_tokens_in_circulation(
        &self,
        request: Request<tari_rpc::GetBlocksRequest>,
    ) -> Result<Response<Self::GetTokensInCirculationStream>, Status>
    {
        debug!(target: LOG_TARGET, "Incoming GRPC request for GetTokensInCirculation",);
        let request = request.into_inner();
        let mut heights = request.heights;
        heights = heights
            .drain(..cmp::min(heights.len(), GET_TOKENS_IN_CIRCULATION_MAX_HEIGHTS))
            .collect();
        let network: Network = self.node_config.network.into();
        let constants = network.create_consensus_constants();
        let (mut tx, rx) = mpsc::channel(GET_TOKENS_IN_CIRCULATION_PAGE_SIZE);
        self.executor.spawn(async move {
            let mut page: Vec<u64> = heights
                .drain(..cmp::min(heights.len(), GET_TOKENS_IN_CIRCULATION_PAGE_SIZE))
                .collect();
            let (initial, decay, tail) = constants.emission_amounts();
            let schedule = EmissionSchedule::new(initial, decay, tail);
            while !page.is_empty() {
                let values: Vec<tari_rpc::ValueAtHeightResponse> = page
                    .clone()
                    .into_iter()
                    .map(|height| tari_rpc::ValueAtHeightResponse {
                        height,
                        value: schedule.supply_at_block(height).into(),
                    })
                    .collect();
                let result_size = values.len();
                for value in values {
                    match tx.send(Ok(value)).await {
                        Ok(_) => (),
                        Err(err) => {
                            warn!(target: LOG_TARGET, "Error sending value via GRPC:  {}", err);
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
                if result_size < GET_TOKENS_IN_CIRCULATION_PAGE_SIZE {
                    break;
                }
                page = heights
                    .drain(..cmp::min(heights.len(), GET_TOKENS_IN_CIRCULATION_PAGE_SIZE))
                    .collect();
            }
        });

        debug!(target: LOG_TARGET, "Sending GetTokensInCirculation response to client");
        Ok(Response::new(rx))
    }
}

enum BlockGroupType {
    BlockFees,
    BlockSize,
}
async fn get_block_group(
    mut handler: LocalNodeCommsInterface,
    request: Request<tari_rpc::BlockGroupRequest>,
    block_group_type: BlockGroupType,
) -> Result<Response<tari_rpc::BlockGroupResponse>, Status>
{
    let request = request.into_inner();
    let calc_type_response = request.calc_type;
    let calc_type: CalcType = request.calc_type();
    let height_request: tari_rpc::HeightRequest = request.into();

    debug!(
        target: LOG_TARGET,
        "Incoming GRPC request for GetBlockSize: from_tip: {:?} start_height: {:?} end_height: {:?}",
        height_request.from_tip,
        height_request.start_height,
        height_request.end_height
    );

    let heights = get_heights(&height_request, handler.clone()).await?;

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
    }
    .unwrap_or_default();
    debug!(
        target: LOG_TARGET,
        "Sending GetBlockSize response to client: {:?}", value
    );
    Ok(Response::new(tari_rpc::BlockGroupResponse {
        value,
        calc_type: calc_type_response,
    }))
}
