// Copyright 2021. The Tari Project
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
    cmp,
    convert::{TryFrom, TryInto},
};

use borsh::{BorshDeserialize, BorshSerialize};
use either::Either;
use futures::{channel::mpsc, SinkExt};
use log::*;
use minotari_app_grpc::{
    tari_rpc,
    tari_rpc::{CalcType, Sorting},
};
use minotari_app_utilities::consts;
use tari_common_types::{
    tari_address::TariAddress,
    types::{Commitment, FixedHash, PublicKey, Signature},
};
use tari_comms::{Bytes, CommsNode};
use tari_core::{
    base_node::{
        comms_interface::CommsInterfaceError,
        state_machine_service::states::StateInfo,
        LocalNodeCommsInterface,
        StateMachineHandle,
    },
    blocks::{Block, BlockHeader, NewBlockTemplate},
    chain_storage::ChainStorageError,
    consensus::{emission::Emission, ConsensusManager, NetworkConsensus},
    iterators::NonOverlappingIntegerPairIter,
    mempool::{service::LocalMempoolService, TxStorageResponse},
    proof_of_work::PowAlgorithm,
    transactions::{
        generate_coinbase_with_wallet_output,
        key_manager::{
            create_memory_db_key_manager,
            TariKeyId,
            TransactionKeyManagerBranch,
            TransactionKeyManagerInterface,
            TxoStage,
        },
        transaction_components::{
            KernelBuilder,
            RangeProofType,
            Transaction,
            TransactionKernel,
            TransactionKernelVersion,
        },
    },
};
use tari_key_manager::key_manager_service::KeyManagerInterface;
use tari_p2p::{auto_update::SoftwareUpdaterHandle, services::liveness::LivenessHandle};
use tari_utilities::{hex::Hex, message_format::MessageFormat, ByteArray};
use tokio::task;
use tonic::{Request, Response, Status};

use crate::{
    builder::BaseNodeContext,
    config::GrpcMethod,
    grpc::{
        blocks::{block_fees, block_heights, block_size, GET_BLOCKS_MAX_HEIGHTS, GET_BLOCKS_PAGE_SIZE},
        hash_rate::HashRateMovingAverage,
        helpers::{mean, median},
    },
    BaseNodeConfig,
};

const LOG_TARGET: &str = "minotari::base_node::grpc";
const GET_TOKENS_IN_CIRCULATION_MAX_HEIGHTS: usize = 1_000_000;
const GET_TOKENS_IN_CIRCULATION_PAGE_SIZE: usize = 1_000;
// The maximum number of difficulty ints that can be requested at a time. These will be streamed to the
// client, so memory is not really a concern here, but a malicious client could request a large
// number here to keep the node busy
const GET_DIFFICULTY_MAX_HEIGHTS: u64 = 10_000;
const GET_DIFFICULTY_PAGE_SIZE: usize = 1_000;
// The maximum number of headers a client can request at a time. If the client requests more than
// this, this is the maximum that will be returned.
const LIST_HEADERS_MAX_NUM_HEADERS: u64 = 10_000;
// The number of headers to request via the local interface at a time. These are then streamed to
// client.
const LIST_HEADERS_PAGE_SIZE: usize = 10;
// The `num_headers` value if none is provided.
const LIST_HEADERS_DEFAULT_NUM_HEADERS: u64 = 10;

const BLOCK_TIMING_MAX_BLOCKS: u64 = 10_000;

pub struct BaseNodeGrpcServer {
    node_service: LocalNodeCommsInterface,
    mempool_service: LocalMempoolService,
    network: NetworkConsensus,
    state_machine_handle: StateMachineHandle,
    consensus_rules: ConsensusManager,
    software_updater: SoftwareUpdaterHandle,
    comms: CommsNode,
    liveness: LivenessHandle,
    report_grpc_error: bool,
    config: BaseNodeConfig,
}

impl BaseNodeGrpcServer {
    pub fn from_base_node_context(ctx: &BaseNodeContext, config: BaseNodeConfig) -> Self {
        Self {
            node_service: ctx.local_node(),
            mempool_service: ctx.local_mempool(),
            network: ctx.network().into(),
            state_machine_handle: ctx.state_machine(),
            consensus_rules: ctx.consensus_rules().clone(),
            software_updater: ctx.software_updater(),
            comms: ctx.base_node_comms().clone(),
            liveness: ctx.liveness(),
            report_grpc_error: ctx.get_report_grpc_error(),
            config,
        }
    }

    pub fn report_error_flag(&self) -> bool {
        self.report_grpc_error
    }

    fn is_method_enabled(&self, grpc_method: GrpcMethod) -> bool {
        let mining_method = [
            GrpcMethod::GetVersion,
            GrpcMethod::GetNewBlockTemplate,
            GrpcMethod::GetNewBlockWithCoinbases,
            GrpcMethod::GetNewBlockTemplateWithCoinbases,
            GrpcMethod::GetNewBlock,
            GrpcMethod::GetNewBlockBlob,
            GrpcMethod::SubmitBlock,
            GrpcMethod::SubmitBlockBlob,
            GrpcMethod::GetTipInfo,
        ];

        let second_layer_methods = [
            GrpcMethod::GetVersion,
            GrpcMethod::GetConstants,
            GrpcMethod::GetMempoolTransactions,
            GrpcMethod::GetTipInfo,
            GrpcMethod::GetActiveValidatorNodes,
            GrpcMethod::GetShardKey,
            GrpcMethod::GetTemplateRegistrations,
            GrpcMethod::GetHeaderByHash,
            GrpcMethod::GetSideChainUtxos,
        ];
        if self.config.mining_enabled && mining_method.contains(&grpc_method) {
            return true;
        }
        if self.config.second_layer_grpc_enabled && second_layer_methods.contains(&grpc_method) {
            return true;
        }
        self.config.grpc_server_allow_methods.contains(&grpc_method)
    }
}

pub fn obscure_error_if_true(report: bool, status: Status) -> Status {
    if report {
        status
    } else {
        warn!(target: LOG_TARGET, "Obscured status error: {}", status);
        Status::new(status.code(), "Error has occurred. Details are obscured.")
    }
}

pub async fn get_heights(
    request: &tari_rpc::HeightRequest,
    handler: LocalNodeCommsInterface,
) -> Result<(u64, u64), Status> {
    block_heights(handler, request.start_height, request.end_height, request.from_tip).await
}
impl BaseNodeGrpcServer {}

#[tonic::async_trait]
impl tari_rpc::base_node_server::BaseNode for BaseNodeGrpcServer {
    type FetchMatchingUtxosStream = mpsc::Receiver<Result<tari_rpc::FetchMatchingUtxosResponse, Status>>;
    type GetActiveValidatorNodesStream = mpsc::Receiver<Result<tari_rpc::GetActiveValidatorNodesResponse, Status>>;
    type GetBlocksStream = mpsc::Receiver<Result<tari_rpc::HistoricalBlock, Status>>;
    type GetMempoolTransactionsStream = mpsc::Receiver<Result<tari_rpc::GetMempoolTransactionsResponse, Status>>;
    type GetNetworkDifficultyStream = mpsc::Receiver<Result<tari_rpc::NetworkDifficultyResponse, Status>>;
    type GetPeersStream = mpsc::Receiver<Result<tari_rpc::GetPeersResponse, Status>>;
    type GetSideChainUtxosStream = mpsc::Receiver<Result<tari_rpc::GetSideChainUtxosResponse, Status>>;
    type GetTemplateRegistrationsStream = mpsc::Receiver<Result<tari_rpc::GetTemplateRegistrationResponse, Status>>;
    type GetTokensInCirculationStream = mpsc::Receiver<Result<tari_rpc::ValueAtHeightResponse, Status>>;
    type ListHeadersStream = mpsc::Receiver<Result<tari_rpc::BlockHeaderResponse, Status>>;
    type SearchKernelsStream = mpsc::Receiver<Result<tari_rpc::HistoricalBlock, Status>>;
    type SearchUtxosStream = mpsc::Receiver<Result<tari_rpc::HistoricalBlock, Status>>;

    #[allow(clippy::too_many_lines)]
    async fn get_network_difficulty(
        &self,
        request: Request<tari_rpc::HeightRequest>,
    ) -> Result<Response<Self::GetNetworkDifficultyStream>, Status> {
        if !self.is_method_enabled(GrpcMethod::GetNetworkDifficulty) {
            return Err(Status::permission_denied(
                "`GetNetworkDifficulty` method not made available",
            ));
        }
        let report_error_flag = self.report_error_flag();
        let request = request.into_inner();
        debug!(
            target: LOG_TARGET,
            "Incoming GRPC request for GetNetworkDifficulty: from_tip: {:?} start_height: {:?} end_height: {:?}",
            request.from_tip,
            request.start_height,
            request.end_height
        );
        let mut handler = self.node_service.clone();
        let (start_height, end_height) = get_heights(&request, handler.clone())
            .await
            .map_err(|e| obscure_error_if_true(report_error_flag, e))?;
        let num_requested = end_height.checked_sub(start_height).ok_or(obscure_error_if_true(
            report_error_flag,
            Status::invalid_argument("Start height is more than end height"),
        ))?;
        if num_requested > GET_DIFFICULTY_MAX_HEIGHTS {
            return Err(obscure_error_if_true(
                report_error_flag,
                Status::invalid_argument(format!(
                    "Number of headers requested exceeds maximum. Expected less than {} but got {}",
                    GET_DIFFICULTY_MAX_HEIGHTS, num_requested
                )),
            ));
        }
        let (mut tx, rx) = mpsc::channel(cmp::min(
            usize::try_from(num_requested).map_err(|e| {
                obscure_error_if_true(
                    report_error_flag,
                    Status::internal(format!("Error converting u64 to usize '{}'", e)),
                )
            })?,
            GET_DIFFICULTY_PAGE_SIZE,
        ));

        let mut sha3x_hash_rate_moving_average =
            HashRateMovingAverage::new(PowAlgorithm::Sha3x, self.consensus_rules.clone());
        let mut randomx_hash_rate_moving_average =
            HashRateMovingAverage::new(PowAlgorithm::RandomX, self.consensus_rules.clone());

        let page_iter =
            NonOverlappingIntegerPairIter::new(start_height, end_height.saturating_add(1), GET_DIFFICULTY_PAGE_SIZE)
                .map_err(|e| obscure_error_if_true(report_error_flag, Status::invalid_argument(e)))?;
        task::spawn(async move {
            for (start, end) in page_iter {
                // headers are returned by height
                let headers = match handler.get_headers(start..=end).await {
                    Ok(headers) => headers,
                    Err(err) => {
                        warn!(target: LOG_TARGET, "Base node service error: {:?}", err,);
                        let _ = tx
                            .send(Err(obscure_error_if_true(
                                report_error_flag,
                                Status::internal("Internal error when fetching blocks"),
                            )))
                            .await;
                        return;
                    },
                };

                if headers.is_empty() {
                    let _network_difficulty_response = tx.send(Err(obscure_error_if_true(
                        report_error_flag,
                        Status::invalid_argument(format!("No blocks found within range {} - {}", start, end)),
                    )));
                    return;
                }

                for chain_header in &headers {
                    let current_difficulty = chain_header.accumulated_data().target_difficulty;
                    let current_timestamp = chain_header.header().timestamp;
                    let current_height = chain_header.header().height;
                    let pow_algo = chain_header.header().pow.pow_algo;

                    // update the moving average calculation with the header data
                    let current_hash_rate_moving_average = match pow_algo {
                        PowAlgorithm::RandomX => &mut randomx_hash_rate_moving_average,
                        PowAlgorithm::Sha3x => &mut sha3x_hash_rate_moving_average,
                    };
                    current_hash_rate_moving_average.add(current_height, current_difficulty);

                    let sha3x_estimated_hash_rate = sha3x_hash_rate_moving_average.average();
                    let randomx_estimated_hash_rate = randomx_hash_rate_moving_average.average();
                    let estimated_hash_rate = sha3x_estimated_hash_rate.saturating_add(randomx_estimated_hash_rate);

                    let difficulty = tari_rpc::NetworkDifficultyResponse {
                        difficulty: current_difficulty.as_u64(),
                        estimated_hash_rate,
                        sha3x_estimated_hash_rate,
                        randomx_estimated_hash_rate,
                        height: current_height,
                        timestamp: current_timestamp.as_u64(),
                        pow_algo: pow_algo.as_u64(),
                    };

                    if let Err(err) = tx.send(Ok(difficulty)).await {
                        warn!(target: LOG_TARGET, "Error sending difficulties via GRPC:  {}", err);
                        return;
                    }
                }
            }
        });

        debug!(
            target: LOG_TARGET,
            "Sending GetNetworkDifficulty response stream to client"
        );
        Ok(Response::new(rx))
    }

    async fn get_mempool_transactions(
        &self,
        request: Request<tari_rpc::GetMempoolTransactionsRequest>,
    ) -> Result<Response<Self::GetMempoolTransactionsStream>, Status> {
        if !self.is_method_enabled(GrpcMethod::GetMempoolTransactions) {
            return Err(Status::permission_denied(
                "`GetMempoolTransactions` method not made available",
            ));
        }
        let report_error_flag = self.report_error_flag();
        let _request = request.into_inner();
        debug!(target: LOG_TARGET, "Incoming GRPC request for GetMempoolTransactions",);

        let mut mempool = self.mempool_service.clone();
        let (mut tx, rx) = mpsc::channel(1000);

        task::spawn(async move {
            let transactions = match mempool.get_mempool_state().await {
                Err(err) => {
                    warn!(target: LOG_TARGET, "Error communicating with base node: {}", err,);
                    return;
                },
                Ok(data) => data,
            };
            for transaction in transactions.unconfirmed_pool {
                let transaction = match tari_rpc::Transaction::try_from(transaction) {
                    Ok(t) => t,
                    Err(e) => {
                        if tx
                            .send(Err(obscure_error_if_true(
                                report_error_flag,
                                Status::internal(format!("Error converting transaction: {}", e)),
                            )))
                            .await
                            .is_err()
                        {
                            // Sender has closed i.e the connection has dropped/request was abandoned
                            warn!(
                                target: LOG_TARGET,
                                "[get_mempool_transactions] GRPC request cancelled while sending response"
                            );
                        }
                        return;
                    },
                };

                if tx
                    .send(Ok(tari_rpc::GetMempoolTransactionsResponse {
                        transaction: Some(transaction),
                    }))
                    .await
                    .is_err()
                {
                    // Sender has closed i.e the connection has dropped/request was abandoned
                    warn!(target: LOG_TARGET, "GRPC request cancelled while sending response");
                }
            }
        });
        debug!(target: LOG_TARGET, "Sending GetMempool response stream to client");
        Ok(Response::new(rx))
    }

    // casting here is okay as a block cannot have more than u32 kernels
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::too_many_lines)]
    async fn list_headers(
        &self,
        request: Request<tari_rpc::ListHeadersRequest>,
    ) -> Result<Response<Self::ListHeadersStream>, Status> {
        if !self.is_method_enabled(GrpcMethod::ListHeaders) {
            return Err(Status::permission_denied("`ListHeaders` method not made available"));
        }
        let report_error_flag = self.report_error_flag();
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
                return Err(obscure_error_if_true(
                    report_error_flag,
                    Status::internal(err.to_string()),
                ));
            },
            Ok(data) => data.best_block_height(),
        };

        let sorting: Sorting = request.sorting();
        let num_headers = match request.num_headers {
            0 => LIST_HEADERS_DEFAULT_NUM_HEADERS,
            _ => request.num_headers,
        };

        let num_headers = cmp::min(num_headers, LIST_HEADERS_MAX_NUM_HEADERS);
        let (mut tx, rx) = mpsc::channel(LIST_HEADERS_PAGE_SIZE);

        let from_height = cmp::min(request.from_height, tip);

        let (header_range, is_reversed) = if from_height == 0 {
            match sorting {
                Sorting::Desc => {
                    let from = match tip.overflowing_sub(num_headers) {
                        (_, true) => 0,
                        (res, false) => res + 1,
                    };
                    (from..=tip, true)
                },
                Sorting::Asc => (0..=num_headers.saturating_sub(1), false),
            }
        } else {
            match sorting {
                Sorting::Desc => {
                    let from = match from_height.overflowing_sub(num_headers) {
                        (_, true) => 0,
                        (res, false) => res + 1,
                    };
                    (from..=from_height, true)
                },
                Sorting::Asc => {
                    let to = from_height.saturating_add(num_headers).saturating_sub(1);
                    (from_height..=to, false)
                },
            }
        };
        let consensus_rules = self.consensus_rules.clone();
        let page_iter = NonOverlappingIntegerPairIter::new(
            *header_range.start(),
            header_range.end().saturating_add(1),
            LIST_HEADERS_PAGE_SIZE,
        )
        .map_err(|e| obscure_error_if_true(report_error_flag, Status::invalid_argument(e)))?;
        task::spawn(async move {
            debug!(
                target: LOG_TARGET,
                "Starting base node request {}-{}",
                header_range.start(),
                header_range.end()
            );
            let page_iter = if is_reversed {
                Either::Left(page_iter.rev())
            } else {
                Either::Right(page_iter)
            };
            for (start, end) in page_iter {
                debug!(target: LOG_TARGET, "Page: {}-{}", start, end);
                let result_data = match handler.get_blocks(start..=end, true).await {
                    Err(err) => {
                        warn!(target: LOG_TARGET, "Internal base node service error: {}", err);
                        return;
                    },
                    Ok(data) => {
                        if is_reversed {
                            data.into_iter()
                                .map(|chain_block| {
                                    let (block, acc_data, confirmations) = chain_block.dissolve();
                                    match consensus_rules
                                        .calculate_coinbase_and_fees(block.header.height, block.body.kernels())
                                    {
                                        Ok(total_block_reward) => Ok(tari_rpc::BlockHeaderResponse {
                                            difficulty: acc_data.achieved_difficulty.into(),
                                            num_transactions: block.body.kernels().len() as u32,
                                            confirmations,
                                            header: Some(block.header.into()),
                                            reward: total_block_reward.into(),
                                        }),
                                        Err(e) => {
                                            Err(obscure_error_if_true(report_error_flag, Status::internal(e))
                                                .to_string())
                                        },
                                    }
                                })
                                .rev()
                                .collect::<Result<Vec<_>, String>>()
                        } else {
                            data.into_iter()
                                .map(|chain_block| {
                                    let (block, acc_data, confirmations) = chain_block.dissolve();
                                    match consensus_rules
                                        .calculate_coinbase_and_fees(block.header.height, block.body.kernels())
                                    {
                                        Ok(total_block_reward) => Ok(tari_rpc::BlockHeaderResponse {
                                            difficulty: acc_data.achieved_difficulty.into(),
                                            num_transactions: block.body.kernels().len() as u32,
                                            confirmations,
                                            header: Some(block.header.into()),
                                            reward: total_block_reward.into(),
                                        }),
                                        Err(e) => {
                                            Err(obscure_error_if_true(report_error_flag, Status::internal(e))
                                                .to_string())
                                        },
                                    }
                                })
                                .collect::<Result<Vec<_>, String>>()
                        }
                    },
                };

                match result_data {
                    Err(e) => {
                        error!(target: LOG_TARGET, "No result headers transmitted due to error: {}", e)
                    },
                    Ok(result_data) => {
                        let result_size = result_data.len();
                        debug!(target: LOG_TARGET, "Result headers: {}", result_size);

                        for response in result_data {
                            // header wont be none here as we just filled it in above
                            debug!(
                                target: LOG_TARGET,
                                "Sending block header: {}",
                                response.header.as_ref().map( | h| h.height).unwrap_or(0)
                            );
                            if tx.send(Ok(response)).await.is_err() {
                                // Sender has closed i.e the connection has dropped/request was abandoned
                                warn!(
                                    target: LOG_TARGET,
                                    "[list_headers] GRPC request cancelled while sending response"
                                );
                                return;
                            }
                        }
                    },
                }
            }
        });

        debug!(target: LOG_TARGET, "Sending ListHeaders response stream to client");
        Ok(Response::new(rx))
    }

    async fn get_new_block_template(
        &self,
        request: Request<tari_rpc::NewBlockTemplateRequest>,
    ) -> Result<Response<tari_rpc::NewBlockTemplateResponse>, Status> {
        if !self.is_method_enabled(GrpcMethod::GetNewBlockTemplate) {
            return Err(Status::permission_denied(
                "`GetNewBlockTemplate` method not made available",
            ));
        }
        let report_error_flag = self.report_error_flag();
        let request = request.into_inner();
        debug!(target: LOG_TARGET, "Incoming GRPC request for get new block template");
        trace!(target: LOG_TARGET, "Request {:?}", request);
        let algo = request
            .algo
            .map(|algo| u64::try_from(algo.pow_algo))
            .ok_or_else(|| obscure_error_if_true(report_error_flag, Status::invalid_argument("PoW algo not provided")))?
            .map_err(|e| {
                obscure_error_if_true(
                    report_error_flag,
                    Status::invalid_argument(format!("Invalid PoW algo '{}'", e)),
                )
            })?;

        let algo = PowAlgorithm::try_from(algo).map_err(|e| {
            obscure_error_if_true(
                report_error_flag,
                Status::invalid_argument(format!("Invalid PoW algo '{}'", e)),
            )
        })?;

        let mut handler = self.node_service.clone();

        let new_template = handler
            .get_new_block_template(algo, request.max_weight)
            .await
            .map_err(|e| {
                warn!(
                    target: LOG_TARGET,
                    "Could not get new block template: {}",
                    e.to_string()
                );
                obscure_error_if_true(report_error_flag, Status::internal(e.to_string()))
            })?;

        let status_watch = self.state_machine_handle.get_status_info_watch();
        let pow = algo as i32;
        let response = tari_rpc::NewBlockTemplateResponse {
            miner_data: Some(tari_rpc::MinerData {
                reward: new_template.reward.into(),
                target_difficulty: new_template.target_difficulty.as_u64(),
                total_fees: new_template.total_fees.into(),
                algo: Some(tari_rpc::PowAlgo { pow_algo: pow }),
            }),
            new_block_template: Some(
                new_template
                    .try_into()
                    .map_err(|e| obscure_error_if_true(report_error_flag, Status::internal(e)))?,
            ),
            initial_sync_achieved: status_watch.borrow().bootstrapped,
        };

        debug!(target: LOG_TARGET, "Sending GetNewBlockTemplate response to client");
        Ok(Response::new(response))
    }

    async fn get_new_block(
        &self,
        request: Request<tari_rpc::NewBlockTemplate>,
    ) -> Result<Response<tari_rpc::GetNewBlockResult>, Status> {
        if !self.is_method_enabled(GrpcMethod::GetNewBlock) {
            return Err(Status::permission_denied("`GetNewBlock` method not made available"));
        }
        let report_error_flag = self.report_error_flag();
        let request = request.into_inner();
        debug!(target: LOG_TARGET, "Incoming GRPC request for get new block");
        let block_template: NewBlockTemplate = request.try_into().map_err(|s| {
            obscure_error_if_true(
                report_error_flag,
                Status::invalid_argument(format!("Malformed block template provided: {}", s)),
            )
        })?;
        let algo = block_template.header.pow.pow_algo;

        let mut handler = self.node_service.clone();

        let new_block = match handler.get_new_block(block_template).await {
            Ok(b) => b,
            Err(CommsInterfaceError::ChainStorageError(ChainStorageError::InvalidArguments { message, .. })) => {
                return Err(obscure_error_if_true(
                    report_error_flag,
                    Status::invalid_argument(message),
                ));
            },
            Err(CommsInterfaceError::ChainStorageError(ChainStorageError::CannotCalculateNonTipMmr(msg))) => {
                let status = Status::with_details(
                    tonic::Code::FailedPrecondition,
                    msg,
                    Bytes::from_static(b"CannotCalculateNonTipMmr"),
                );
                return Err(obscure_error_if_true(report_error_flag, status));
            },
            Err(e) => {
                return Err(obscure_error_if_true(
                    report_error_flag,
                    Status::internal(e.to_string()),
                ))
            },
        };
        let fees = new_block.body.get_total_fee().map_err(|_| {
            obscure_error_if_true(
                report_error_flag,
                Status::invalid_argument("Invalid fees in block".to_string()),
            )
        })?;
        let gen_hash = handler
            .get_header(0)
            .await
            .map_err(|_| {
                obscure_error_if_true(
                    report_error_flag,
                    Status::invalid_argument("Tari genesis block not found".to_string()),
                )
            })?
            .ok_or_else(|| {
                obscure_error_if_true(
                    report_error_flag,
                    Status::not_found("Tari genesis block not found".to_string()),
                )
            })?
            .hash()
            .to_vec();
        // construct response
        let block_hash = new_block.hash().to_vec();
        let mining_hash = match new_block.header.pow.pow_algo {
            PowAlgorithm::Sha3x => new_block.header.mining_hash().to_vec(),
            PowAlgorithm::RandomX => new_block.header.merge_mining_hash().to_vec(),
        };
        let block: Option<tari_rpc::Block> = Some(
            new_block
                .try_into()
                .map_err(|e| obscure_error_if_true(report_error_flag, Status::internal(e)))?,
        );
        let new_template = handler.get_new_block_template(algo, 0).await.map_err(|e| {
            warn!(
                target: LOG_TARGET,
                "Could not get new block template: {}",
                e.to_string()
            );
            obscure_error_if_true(report_error_flag, Status::internal(e.to_string()))
        })?;

        let pow = algo as i32;

        let miner_data = tari_rpc::MinerData {
            reward: new_template.reward.into(),
            target_difficulty: new_template.target_difficulty.as_u64(),
            total_fees: fees.as_u64(),
            algo: Some(tari_rpc::PowAlgo { pow_algo: pow }),
        };

        let response = tari_rpc::GetNewBlockResult {
            block_hash,
            block,
            merge_mining_hash: mining_hash,
            tari_unique_id: gen_hash,
            miner_data: Some(miner_data),
        };
        debug!(target: LOG_TARGET, "Sending GetNewBlock response to client");
        Ok(Response::new(response))
    }

    #[allow(clippy::too_many_lines)]
    async fn get_new_block_template_with_coinbases(
        &self,
        request: Request<tari_rpc::GetNewBlockTemplateWithCoinbasesRequest>,
    ) -> Result<Response<tari_rpc::GetNewBlockResult>, Status> {
        if !self.is_method_enabled(GrpcMethod::GetNewBlockTemplateWithCoinbases) {
            return Err(Status::permission_denied(
                "`GetNewBlockTemplateWithCoinbases` method not made available",
            ));
        }
        debug!(target: LOG_TARGET, "Incoming GRPC request for get new block template with coinbases");
        let report_error_flag = self.report_error_flag();
        let request = request.into_inner();
        let algo = request
            .algo
            .map(|algo| u64::try_from(algo.pow_algo))
            .ok_or_else(|| obscure_error_if_true(report_error_flag, Status::invalid_argument("PoW algo not provided")))?
            .map_err(|e| {
                obscure_error_if_true(
                    report_error_flag,
                    Status::invalid_argument(format!("Invalid PoW algo '{}'", e)),
                )
            })?;

        let algo = PowAlgorithm::try_from(algo).map_err(|e| {
            obscure_error_if_true(
                report_error_flag,
                Status::invalid_argument(format!("Invalid PoW algo '{}'", e)),
            )
        })?;

        let mut handler = self.node_service.clone();

        let mut new_template = handler
            .get_new_block_template(algo, request.max_weight)
            .await
            .map_err(|e| {
                warn!(
                    target: LOG_TARGET,
                    "Could not get new block template: {}",
                    e.to_string()
                );
                obscure_error_if_true(report_error_flag, Status::internal(e.to_string()))
            })?;

        let pow = algo as i32;

        let miner_data = tari_rpc::MinerData {
            reward: new_template.reward.into(),
            target_difficulty: new_template.target_difficulty.as_u64(),
            total_fees: new_template.total_fees.into(),
            algo: Some(tari_rpc::PowAlgo { pow_algo: pow }),
        };

        let mut coinbases: Vec<tari_rpc::NewBlockCoinbase> = request.coinbases;

        // let validate the coinbase amounts;
        let reward = self
            .consensus_rules
            .calculate_coinbase_and_fees(new_template.header.height, new_template.body.kernels())
            .map_err(|_| {
                obscure_error_if_true(
                    report_error_flag,
                    Status::internal("Could not calculate the amount of fees in the block".to_string()),
                )
            })?
            .as_u64();
        let mut total_shares = 0u64;
        for coinbase in &coinbases {
            total_shares += coinbase.value;
        }
        let mut remainder = reward - ((reward / total_shares) * total_shares);
        for coinbase in &mut coinbases {
            coinbase.value *= reward / total_shares;
            if remainder > 0 {
                coinbase.value += 1;
                remainder -= 1;
            }
        }

        let key_manager = create_memory_db_key_manager();
        let height = new_template.header.height;
        // The script key is not used in the Diffie-Hellmann protocol, so we assign default.
        let script_key_id = TariKeyId::default();

        let mut total_excess = Commitment::default();
        let mut total_nonce = PublicKey::default();
        let mut private_keys = Vec::new();
        let mut kernel_message = [0; 32];
        let mut last_kernel = Default::default();
        for coinbase in coinbases {
            let address = TariAddress::from_hex(&coinbase.address)
                .map_err(|e| obscure_error_if_true(report_error_flag, Status::internal(e.to_string())))?;
            let range_proof_type = if coinbase.revealed_value_proof {
                RangeProofType::RevealedValue
            } else {
                RangeProofType::BulletProofPlus
            };
            let (_, coinbase_output, coinbase_kernel, wallet_output) = generate_coinbase_with_wallet_output(
                0.into(),
                coinbase.value.into(),
                height,
                &coinbase.coinbase_extra,
                &key_manager,
                &script_key_id,
                &address,
                coinbase.stealth_payment,
                self.consensus_rules.consensus_constants(height),
                range_proof_type,
            )
            .await
            .map_err(|e| obscure_error_if_true(report_error_flag, Status::internal(e.to_string())))?;
            new_template.body.add_output(coinbase_output);
            let (new_private_nonce, pub_nonce) = key_manager
                .get_next_key(TransactionKeyManagerBranch::KernelNonce.get_branch_key())
                .await
                .map_err(|e| obscure_error_if_true(report_error_flag, Status::internal(e.to_string())))?;
            total_nonce = &total_nonce + &pub_nonce;
            total_excess = &total_excess + &coinbase_kernel.excess;
            private_keys.push((wallet_output.spending_key_id, new_private_nonce));
            kernel_message = TransactionKernel::build_kernel_signature_message(
                &TransactionKernelVersion::get_current_version(),
                coinbase_kernel.fee,
                coinbase_kernel.lock_height,
                &coinbase_kernel.features,
                &None,
            );
            last_kernel = coinbase_kernel;
        }
        let mut kernel_signature = Signature::default();
        for (spending_key_id, nonce) in private_keys {
            kernel_signature = &kernel_signature +
                &key_manager
                    .get_partial_txo_kernel_signature(
                        &spending_key_id,
                        &nonce,
                        &total_nonce,
                        total_excess.as_public_key(),
                        &TransactionKernelVersion::get_current_version(),
                        &kernel_message,
                        &last_kernel.features,
                        TxoStage::Output,
                    )
                    .await
                    .map_err(|e| obscure_error_if_true(report_error_flag, Status::internal(e.to_string())))?;
        }
        let kernel_new = KernelBuilder::new()
            .with_fee(0.into())
            .with_features(last_kernel.features)
            .with_lock_height(last_kernel.lock_height)
            .with_excess(&total_excess)
            .with_signature(kernel_signature)
            .build()
            .unwrap();

        new_template.body.add_kernel(kernel_new);
        new_template.body.sort();

        let new_block = match handler.get_new_block(new_template).await {
            Ok(b) => b,
            Err(CommsInterfaceError::ChainStorageError(ChainStorageError::InvalidArguments { message, .. })) => {
                return Err(obscure_error_if_true(
                    report_error_flag,
                    Status::invalid_argument(message),
                ));
            },
            Err(CommsInterfaceError::ChainStorageError(ChainStorageError::CannotCalculateNonTipMmr(msg))) => {
                let status = Status::with_details(
                    tonic::Code::FailedPrecondition,
                    msg,
                    Bytes::from_static(b"CannotCalculateNonTipMmr"),
                );
                return Err(obscure_error_if_true(report_error_flag, status));
            },
            Err(e) => {
                return Err(obscure_error_if_true(
                    report_error_flag,
                    Status::internal(e.to_string()),
                ))
            },
        };
        let gen_hash = handler
            .get_header(0)
            .await
            .map_err(|_| {
                obscure_error_if_true(
                    report_error_flag,
                    Status::invalid_argument("Tari genesis block not found".to_string()),
                )
            })?
            .ok_or_else(|| {
                obscure_error_if_true(
                    report_error_flag,
                    Status::not_found("Tari genesis block not found".to_string()),
                )
            })?
            .hash()
            .to_vec();
        // construct response
        let block_hash = new_block.hash().to_vec();
        let mining_hash = match new_block.header.pow.pow_algo {
            PowAlgorithm::Sha3x => new_block.header.mining_hash().to_vec(),
            PowAlgorithm::RandomX => new_block.header.merge_mining_hash().to_vec(),
        };
        let block: Option<tari_rpc::Block> = Some(
            new_block
                .try_into()
                .map_err(|e| obscure_error_if_true(report_error_flag, Status::internal(e)))?,
        );

        let response = tari_rpc::GetNewBlockResult {
            block_hash,
            block,
            merge_mining_hash: mining_hash,
            tari_unique_id: gen_hash,
            miner_data: Some(miner_data),
        };
        debug!(target: LOG_TARGET, "Sending GetNewBlock response to client");
        Ok(Response::new(response))
    }

    #[allow(clippy::too_many_lines)]
    async fn get_new_block_with_coinbases(
        &self,
        request: Request<tari_rpc::GetNewBlockWithCoinbasesRequest>,
    ) -> Result<Response<tari_rpc::GetNewBlockResult>, Status> {
        if !self.is_method_enabled(GrpcMethod::GetNewBlockWithCoinbases) {
            return Err(Status::permission_denied(
                "`GetNewBlockWithCoinbasesRequest` method not made available",
            ));
        }
        let report_error_flag = self.report_error_flag();
        let request = request.into_inner();
        debug!(target: LOG_TARGET, "Incoming GRPC request for get new block with coinbases");
        let mut block_template: NewBlockTemplate = request
            .new_template
            .ok_or(obscure_error_if_true(
                report_error_flag,
                Status::invalid_argument("Malformed block template provided".to_string()),
            ))?
            .try_into()
            .map_err(|s| {
                obscure_error_if_true(
                    report_error_flag,
                    Status::invalid_argument(format!("Malformed block template provided: {}", s)),
                )
            })?;
        let coinbases: Vec<tari_rpc::NewBlockCoinbase> = request.coinbases;

        let mut handler = self.node_service.clone();

        // let validate the coinbase amounts;
        let reward = self
            .consensus_rules
            .calculate_coinbase_and_fees(block_template.header.height, block_template.body.kernels())
            .map_err(|_| {
                obscure_error_if_true(
                    report_error_flag,
                    Status::internal("Could not calculate the amount of fees in the block".to_string()),
                )
            })?;
        let mut amount = 0u64;
        for coinbase in &coinbases {
            amount += coinbase.value;
        }

        if amount != reward.as_u64() {
            return Err(obscure_error_if_true(
                report_error_flag,
                Status::invalid_argument("Malformed coinbase amounts".to_string()),
            ));
        }
        let key_manager = create_memory_db_key_manager();
        let height = block_template.header.height;
        // The script key is not used in the Diffie-Hellmann protocol, so we assign default.
        let script_key_id = TariKeyId::default();

        let mut total_excess = Commitment::default();
        let mut total_nonce = PublicKey::default();
        let mut private_keys = Vec::new();
        let mut kernel_message = [0; 32];
        let mut last_kernel = Default::default();
        for coinbase in coinbases {
            let address = TariAddress::from_hex(&coinbase.address)
                .map_err(|e| obscure_error_if_true(report_error_flag, Status::internal(e.to_string())))?;
            let range_proof_type = if coinbase.revealed_value_proof {
                RangeProofType::RevealedValue
            } else {
                RangeProofType::BulletProofPlus
            };
            let (_, coinbase_output, coinbase_kernel, wallet_output) = generate_coinbase_with_wallet_output(
                0.into(),
                coinbase.value.into(),
                height,
                &coinbase.coinbase_extra,
                &key_manager,
                &script_key_id,
                &address,
                coinbase.stealth_payment,
                self.consensus_rules.consensus_constants(height),
                range_proof_type,
            )
            .await
            .map_err(|e| obscure_error_if_true(report_error_flag, Status::internal(e.to_string())))?;
            block_template.body.add_output(coinbase_output);
            let (new_private_nonce, pub_nonce) = key_manager
                .get_next_key(TransactionKeyManagerBranch::KernelNonce.get_branch_key())
                .await
                .map_err(|e| obscure_error_if_true(report_error_flag, Status::internal(e.to_string())))?;
            total_nonce = &total_nonce + &pub_nonce;
            total_excess = &total_excess + &coinbase_kernel.excess;
            private_keys.push((wallet_output.spending_key_id, new_private_nonce));
            kernel_message = TransactionKernel::build_kernel_signature_message(
                &TransactionKernelVersion::get_current_version(),
                coinbase_kernel.fee,
                coinbase_kernel.lock_height,
                &coinbase_kernel.features,
                &None,
            );
            last_kernel = coinbase_kernel;
        }
        let mut kernel_signature = Signature::default();
        for (spending_key_id, nonce) in private_keys {
            kernel_signature = &kernel_signature +
                &key_manager
                    .get_partial_txo_kernel_signature(
                        &spending_key_id,
                        &nonce,
                        &total_nonce,
                        total_excess.as_public_key(),
                        &TransactionKernelVersion::get_current_version(),
                        &kernel_message,
                        &last_kernel.features,
                        TxoStage::Output,
                    )
                    .await
                    .map_err(|e| obscure_error_if_true(report_error_flag, Status::internal(e.to_string())))?;
        }
        let kernel_new = KernelBuilder::new()
            .with_fee(0.into())
            .with_features(last_kernel.features)
            .with_lock_height(last_kernel.lock_height)
            .with_excess(&total_excess)
            .with_signature(kernel_signature)
            .build()
            .unwrap();

        block_template.body.add_kernel(kernel_new);
        block_template.body.sort();

        let new_block = match handler.get_new_block(block_template).await {
            Ok(b) => b,
            Err(CommsInterfaceError::ChainStorageError(ChainStorageError::InvalidArguments { message, .. })) => {
                return Err(obscure_error_if_true(
                    report_error_flag,
                    Status::invalid_argument(message),
                ));
            },
            Err(CommsInterfaceError::ChainStorageError(ChainStorageError::CannotCalculateNonTipMmr(msg))) => {
                let status = Status::with_details(
                    tonic::Code::FailedPrecondition,
                    msg,
                    Bytes::from_static(b"CannotCalculateNonTipMmr"),
                );
                return Err(obscure_error_if_true(report_error_flag, status));
            },
            Err(e) => {
                return Err(obscure_error_if_true(
                    report_error_flag,
                    Status::internal(e.to_string()),
                ))
            },
        };
        let fees = new_block.body.get_total_fee().map_err(|_| {
            obscure_error_if_true(
                report_error_flag,
                Status::invalid_argument("Invalid fees in block".to_string()),
            )
        })?;
        let algo = new_block.header.pow.pow_algo;
        let gen_hash = handler
            .get_header(0)
            .await
            .map_err(|_| {
                obscure_error_if_true(
                    report_error_flag,
                    Status::invalid_argument("Tari genesis block not found".to_string()),
                )
            })?
            .ok_or_else(|| {
                obscure_error_if_true(
                    report_error_flag,
                    Status::not_found("Tari genesis block not found".to_string()),
                )
            })?
            .hash()
            .to_vec();
        // construct response
        let block_hash = new_block.hash().to_vec();
        let mining_hash = match new_block.header.pow.pow_algo {
            PowAlgorithm::Sha3x => new_block.header.mining_hash().to_vec(),
            PowAlgorithm::RandomX => new_block.header.merge_mining_hash().to_vec(),
        };
        let block: Option<tari_rpc::Block> = Some(
            new_block
                .try_into()
                .map_err(|e| obscure_error_if_true(report_error_flag, Status::internal(e)))?,
        );

        let new_template = handler.get_new_block_template(algo, 0).await.map_err(|e| {
            warn!(
                target: LOG_TARGET,
                "Could not get new block template: {}",
                e.to_string()
            );
            obscure_error_if_true(report_error_flag, Status::internal(e.to_string()))
        })?;

        let pow = algo as i32;

        let miner_data = tari_rpc::MinerData {
            reward: new_template.reward.into(),
            target_difficulty: new_template.target_difficulty.as_u64(),
            total_fees: fees.as_u64(),
            algo: Some(tari_rpc::PowAlgo { pow_algo: pow }),
        };

        let response = tari_rpc::GetNewBlockResult {
            block_hash,
            block,
            merge_mining_hash: mining_hash,
            tari_unique_id: gen_hash,
            miner_data: Some(miner_data),
        };
        debug!(target: LOG_TARGET, "Sending GetNewBlock response to client");
        Ok(Response::new(response))
    }

    async fn get_new_block_blob(
        &self,
        request: Request<tari_rpc::NewBlockTemplate>,
    ) -> Result<Response<tari_rpc::GetNewBlockBlobResult>, Status> {
        if !self.is_method_enabled(GrpcMethod::GetNewBlockBlob) {
            return Err(Status::permission_denied("`GetNewBlockBlob` method not made available"));
        }
        let report_error_flag = self.report_error_flag();
        let request = request.into_inner();
        debug!(target: LOG_TARGET, "Incoming GRPC request for get new block blob");
        let block_template: NewBlockTemplate = request.try_into().map_err(|s| {
            obscure_error_if_true(
                report_error_flag,
                Status::invalid_argument(format!("Invalid block template: {}", s)),
            )
        })?;

        let mut handler = self.node_service.clone();

        let new_block = match handler.get_new_block(block_template).await {
            Ok(b) => b,
            Err(CommsInterfaceError::ChainStorageError(ChainStorageError::InvalidArguments { message, .. })) => {
                return Err(obscure_error_if_true(
                    report_error_flag,
                    Status::invalid_argument(message),
                ));
            },
            Err(CommsInterfaceError::ChainStorageError(ChainStorageError::CannotCalculateNonTipMmr(msg))) => {
                let status = Status::with_details(
                    tonic::Code::FailedPrecondition,
                    msg,
                    Bytes::from_static(b"CannotCalculateNonTipMmr"),
                );
                return Err(obscure_error_if_true(report_error_flag, status));
            },
            Err(e) => {
                return Err(obscure_error_if_true(
                    report_error_flag,
                    Status::internal(e.to_string()),
                ))
            },
        };
        // construct response
        let block_hash = new_block.hash().to_vec();
        let mining_hash = match new_block.header.pow.pow_algo {
            PowAlgorithm::Sha3x => new_block.header.mining_hash().to_vec(),
            PowAlgorithm::RandomX => new_block.header.merge_mining_hash().to_vec(),
        };
        let gen_hash = handler
            .get_header(0)
            .await
            .map_err(|_| {
                obscure_error_if_true(
                    report_error_flag,
                    Status::invalid_argument("Tari genesis block not found".to_string()),
                )
            })?
            .ok_or_else(|| {
                obscure_error_if_true(
                    report_error_flag,
                    Status::not_found("Tari genesis block not found".to_string()),
                )
            })?
            .hash()
            .to_vec();

        let (header, block_body) = new_block.into_header_body();
        let mut header_bytes = Vec::new();
        BorshSerialize::serialize(&header, &mut header_bytes)
            .map_err(|err| obscure_error_if_true(report_error_flag, Status::internal(err.to_string())))?;
        let mut block_body_bytes = Vec::new();
        BorshSerialize::serialize(&block_body, &mut block_body_bytes)
            .map_err(|err| obscure_error_if_true(report_error_flag, Status::internal(err.to_string())))?;
        let response = tari_rpc::GetNewBlockBlobResult {
            block_hash,
            header: header_bytes,
            block_body: block_body_bytes,
            merge_mining_hash: mining_hash,
            utxo_mr: header.output_mr.to_vec(),
            tari_unique_id: gen_hash,
        };
        debug!(target: LOG_TARGET, "Sending GetNewBlockBlob response to client");
        Ok(Response::new(response))
    }

    async fn submit_block(
        &self,
        request: Request<tari_rpc::Block>,
    ) -> Result<Response<tari_rpc::SubmitBlockResponse>, Status> {
        if !self.is_method_enabled(GrpcMethod::SubmitBlock) {
            return Err(Status::permission_denied("`SubmitBlock` method not made available"));
        }
        let report_error_flag = self.report_error_flag();
        let request = request.into_inner();
        let block = Block::try_from(request).map_err(|e| {
            obscure_error_if_true(
                report_error_flag,
                Status::invalid_argument(format!("Invalid block provided: {}", e)),
            )
        })?;
        let block_height = block.header.height;
        debug!(target: LOG_TARGET, "Miner submitted block: {}", block);
        info!(
            target: LOG_TARGET,
            "Received SubmitBlock #{} request from client", block_height
        );

        let mut handler = self.node_service.clone();
        let block_hash = handler
            .submit_block(block)
            .await
            .map_err(|e| obscure_error_if_true(report_error_flag, Status::internal(e.to_string())))?
            .to_vec();

        debug!(
            target: LOG_TARGET,
            "Sending SubmitBlock #{} response to client", block_height
        );
        Ok(Response::new(tari_rpc::SubmitBlockResponse { block_hash }))
    }

    async fn submit_block_blob(
        &self,
        request: Request<tari_rpc::BlockBlobRequest>,
    ) -> Result<Response<tari_rpc::SubmitBlockResponse>, Status> {
        if !self.is_method_enabled(GrpcMethod::SubmitBlockBlob) {
            return Err(Status::permission_denied("`SubmitBlockBlob` method not made available"));
        }
        let report_error_flag = self.report_error_flag();
        debug!(target: LOG_TARGET, "Received block blob from miner: {:?}", request);
        let request = request.into_inner();
        debug!(target: LOG_TARGET, "request: {:?}", request);
        let mut header_bytes = request.header_blob.as_slice();
        let mut body_bytes = request.body_blob.as_slice();
        debug!(target: LOG_TARGET, "doing header");

        let header = BorshDeserialize::deserialize(&mut header_bytes)
            .map_err(|e| obscure_error_if_true(report_error_flag, Status::internal(e.to_string())))?;
        debug!(target: LOG_TARGET, "doing body");
        let body = BorshDeserialize::deserialize(&mut body_bytes)
            .map_err(|e| obscure_error_if_true(report_error_flag, Status::internal(e.to_string())))?;

        let block = Block::new(header, body);
        let block_height = block.header.height;
        debug!(target: LOG_TARGET, "Miner submitted block: {}", block);
        info!(
            target: LOG_TARGET,
            "Received SubmitBlock #{} request from client", block_height
        );

        let mut handler = self.node_service.clone();
        let block_hash = handler
            .submit_block(block)
            .await
            .map_err(|e| obscure_error_if_true(report_error_flag, Status::internal(e.to_string())))?
            .to_vec();

        debug!(
            target: LOG_TARGET,
            "Sending SubmitBlock #{} response to client", block_height
        );
        Ok(Response::new(tari_rpc::SubmitBlockResponse { block_hash }))
    }

    async fn submit_transaction(
        &self,
        request: Request<tari_rpc::SubmitTransactionRequest>,
    ) -> Result<Response<tari_rpc::SubmitTransactionResponse>, Status> {
        if !self.is_method_enabled(GrpcMethod::SubmitTransaction) {
            return Err(Status::permission_denied(
                "`SubmitTransaction` method not made available",
            ));
        }
        let report_error_flag = self.report_error_flag();
        let request = request.into_inner();
        let txn: Transaction = request
            .transaction
            .ok_or_else(|| obscure_error_if_true(report_error_flag, Status::invalid_argument("Transaction is empty")))?
            .try_into()
            .map_err(|e| {
                obscure_error_if_true(
                    report_error_flag,
                    Status::invalid_argument(format!("Invalid transaction provided: {}", e)),
                )
            })?;
        debug!(
            target: LOG_TARGET,
            "Received SubmitTransaction request from client ({} kernels, {} outputs, {} inputs)",
            txn.body.kernels().len(),
            txn.body.outputs().len(),
            txn.body.inputs().len()
        );

        let mut handler = self.mempool_service.clone();
        let res = handler.submit_transaction(txn).await.map_err(|e| {
            error!(target: LOG_TARGET, "Error submitting:{}", e);
            obscure_error_if_true(report_error_flag, Status::internal(e.to_string()))
        })?;
        let response = match res {
            TxStorageResponse::UnconfirmedPool => tari_rpc::SubmitTransactionResponse {
                result: tari_rpc::SubmitTransactionResult::Accepted.into(),
            },
            TxStorageResponse::ReorgPool |
            TxStorageResponse::NotStoredAlreadySpent |
            TxStorageResponse::NotStoredAlreadyMined => tari_rpc::SubmitTransactionResponse {
                result: tari_rpc::SubmitTransactionResult::AlreadyMined.into(),
            },
            TxStorageResponse::NotStored |
            TxStorageResponse::NotStoredOrphan |
            TxStorageResponse::NotStoredConsensus |
            TxStorageResponse::NotStoredFeeTooLow |
            TxStorageResponse::NotStoredTimeLocked => tari_rpc::SubmitTransactionResponse {
                result: tari_rpc::SubmitTransactionResult::Rejected.into(),
            },
        };

        debug!(target: LOG_TARGET, "Sending SubmitTransaction response to client");
        Ok(Response::new(response))
    }

    async fn transaction_state(
        &self,
        request: Request<tari_rpc::TransactionStateRequest>,
    ) -> Result<Response<tari_rpc::TransactionStateResponse>, Status> {
        if !self.is_method_enabled(GrpcMethod::TransactionState) {
            return Err(Status::permission_denied(
                "`TransactionState` method not made available",
            ));
        }
        let report_error_flag = self.report_error_flag();
        let request = request.into_inner();
        let excess_sig: Signature = request
            .excess_sig
            .ok_or_else(|| {
                obscure_error_if_true(
                    report_error_flag,
                    Status::invalid_argument("excess_sig not provided".to_string()),
                )
            })?
            .try_into()
            .map_err(|e| {
                obscure_error_if_true(
                    report_error_flag,
                    Status::invalid_argument(format!("excess_sig could not be converted '{}'", e)),
                )
            })?;
        debug!(
            target: LOG_TARGET,
            "Received TransactionState request from client ({} excess_sig)",
            excess_sig
                .to_json()
                .unwrap_or_else(|e| format!("Failed to serialize signature '{}'", e)),
        );
        let mut node_handler = self.node_service.clone();
        let mut mem_handler = self.mempool_service.clone();

        let base_node_response = node_handler
            .get_kernel_by_excess_sig(excess_sig.clone())
            .await
            .map_err(|e| {
                error!(target: LOG_TARGET, "Error submitting query:{}", e);
                obscure_error_if_true(report_error_flag, Status::internal(e.to_string()))
            })?;

        if !base_node_response.is_empty() {
            let response = tari_rpc::TransactionStateResponse {
                result: tari_rpc::TransactionLocation::Mined.into(),
            };
            debug!(
                target: LOG_TARGET,
                "Sending Transaction state response to client {:?}", response
            );
            return Ok(Response::new(response));
        }

        // Base node does not yet know of kernel excess sig, lets ask the mempool
        let res = mem_handler
            .get_transaction_state_by_excess_sig(excess_sig.clone())
            .await
            .map_err(|e| {
                error!(target: LOG_TARGET, "Error submitting query:{}", e);
                obscure_error_if_true(report_error_flag, Status::internal(e.to_string()))
            })?;
        let response = match res {
            TxStorageResponse::UnconfirmedPool => tari_rpc::TransactionStateResponse {
                result: tari_rpc::TransactionLocation::Mempool.into(),
            },
            TxStorageResponse::ReorgPool | TxStorageResponse::NotStoredAlreadySpent => {
                tari_rpc::TransactionStateResponse {
                    result: tari_rpc::TransactionLocation::Unknown.into(), /* We return Unknown here as the mempool
                                                                            * should not think its mined, but the
                                                                            * node does not think it is. */
                }
            },
            TxStorageResponse::NotStored |
            TxStorageResponse::NotStoredConsensus |
            TxStorageResponse::NotStoredOrphan |
            TxStorageResponse::NotStoredFeeTooLow |
            TxStorageResponse::NotStoredTimeLocked |
            TxStorageResponse::NotStoredAlreadyMined => tari_rpc::TransactionStateResponse {
                result: tari_rpc::TransactionLocation::NotStored.into(),
            },
        };

        debug!(
            target: LOG_TARGET,
            "Sending Transaction state response to client {:?}", response
        );
        Ok(Response::new(response))
    }

    async fn get_peers(
        &self,
        _request: Request<tari_rpc::GetPeersRequest>,
    ) -> Result<Response<Self::GetPeersStream>, Status> {
        if !self.is_method_enabled(GrpcMethod::GetPeers) {
            return Err(Status::permission_denied("`GetPeers` method not made available"));
        }
        let report_error_flag = self.report_error_flag();
        debug!(target: LOG_TARGET, "Incoming GRPC request for get all peers");

        let peers = self
            .comms
            .peer_manager()
            .all()
            .await
            .map_err(|e| obscure_error_if_true(report_error_flag, Status::internal(e.to_string())))?;
        let peers: Vec<tari_rpc::Peer> = peers.into_iter().map(|p| p.into()).collect();
        let (mut tx, rx) = mpsc::channel(peers.len());
        task::spawn(async move {
            for peer in peers {
                let response = tari_rpc::GetPeersResponse { peer: Some(peer) };
                if tx.send(Ok(response)).await.is_err() {
                    warn!(
                        target: LOG_TARGET,
                        "[get_peers] Request was cancelled while sending a response"
                    );
                    return;
                }
            }
        });

        debug!(target: LOG_TARGET, "Sending peers response to client");
        Ok(Response::new(rx))
    }

    async fn get_blocks(
        &self,
        request: Request<tari_rpc::GetBlocksRequest>,
    ) -> Result<Response<Self::GetBlocksStream>, Status> {
        if !self.is_method_enabled(GrpcMethod::GetBlocks) {
            return Err(Status::permission_denied("`GetBlocks` method not made available"));
        }
        let report_error_flag = self.report_error_flag();
        let request = request.into_inner();
        debug!(
            target: LOG_TARGET,
            "Incoming GRPC request for GetBlocks: {:?}", request.heights
        );

        let mut heights = request.heights;
        if heights.is_empty() {
            return Err(obscure_error_if_true(
                report_error_flag,
                Status::invalid_argument("heights cannot be empty"),
            ));
        }

        heights.truncate(GET_BLOCKS_MAX_HEIGHTS);
        heights.sort_unstable();
        // unreachable panic: `heights` is not empty
        let start = *heights.first().expect("unreachable");
        let end = *heights.last().expect("unreachable");

        let mut handler = self.node_service.clone();
        let (mut tx, rx) = mpsc::channel(GET_BLOCKS_PAGE_SIZE);
        let page_iter = NonOverlappingIntegerPairIter::new(start, end.saturating_add(1), GET_BLOCKS_PAGE_SIZE)
            .map_err(|e| obscure_error_if_true(report_error_flag, Status::invalid_argument(e)))?;
        task::spawn(async move {
            for (start, end) in page_iter {
                let blocks = match handler.get_blocks(start..=end, false).await {
                    Err(err) => {
                        warn!(
                            target: LOG_TARGET,
                            "Error communicating with local base node: {:?}", err,
                        );
                        return;
                    },
                    Ok(data) => data.into_iter().filter(|b| heights.contains(&b.header().height)),
                };

                for block in blocks {
                    debug!(
                        target: LOG_TARGET,
                        "GetBlock GRPC sending block #{}",
                        block.header().height
                    );
                    let result = block.try_into().map_err(|err| {
                        obscure_error_if_true(
                            report_error_flag,
                            Status::internal(format!("Could not provide block: {}", err)),
                        )
                    });
                    if tx.send(result).await.is_err() {
                        warn!(
                            target: LOG_TARGET,
                            "[get_blocks] Request was cancelled while sending a response"
                        );
                    }
                }
            }
        });

        debug!(target: LOG_TARGET, "Sending GetBlocks response stream to client");
        Ok(Response::new(rx))
    }

    async fn get_tip_info(
        &self,
        _request: Request<tari_rpc::Empty>,
    ) -> Result<Response<tari_rpc::TipInfoResponse>, Status> {
        if !self.is_method_enabled(GrpcMethod::GetTipInfo) {
            return Err(Status::permission_denied("`GetTipInfo` method not made available"));
        }
        let report_error_flag = self.report_error_flag();
        debug!(target: LOG_TARGET, "Incoming GRPC request for BN tip data");

        let mut handler = self.node_service.clone();

        let meta = handler
            .get_metadata()
            .await
            .map_err(|e| obscure_error_if_true(report_error_flag, Status::internal(e.to_string())))?;

        // Determine if we are bootstrapped
        let status_watch = self.state_machine_handle.get_status_info_watch();
        let state: tari_rpc::BaseNodeState = (&status_watch.borrow().state_info).into();
        let response = tari_rpc::TipInfoResponse {
            metadata: Some(meta.into()),
            initial_sync_achieved: status_watch.borrow().bootstrapped,
            base_node_state: state.into(),
        };

        debug!(target: LOG_TARGET, "Sending MetaData response to client");
        Ok(Response::new(response))
    }

    async fn search_kernels(
        &self,
        request: Request<tari_rpc::SearchKernelsRequest>,
    ) -> Result<Response<Self::SearchKernelsStream>, Status> {
        if !self.is_method_enabled(GrpcMethod::SearchKernels) {
            return Err(Status::permission_denied("`SearchKernels` method not made available"));
        }
        let report_error_flag = self.report_error_flag();
        debug!(target: LOG_TARGET, "Incoming GRPC request for SearchKernels");
        let request = request.into_inner();

        let kernels = request
            .signatures
            .into_iter()
            .map(|s| s.try_into())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| {
                obscure_error_if_true(
                    report_error_flag,
                    Status::invalid_argument(format!("Invalid signatures provided: {}", e)),
                )
            })?;

        let mut handler = self.node_service.clone();

        let (mut tx, rx) = mpsc::channel(GET_BLOCKS_PAGE_SIZE);
        task::spawn(async move {
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
                let result = block.try_into().map_err(|err| {
                    obscure_error_if_true(
                        report_error_flag,
                        Status::internal(format!("Could not provide block:{}", err)),
                    )
                });
                if tx.send(result).await.is_err() {
                    warn!(
                        target: LOG_TARGET,
                        "[search_kernels] Request was cancelled while sending a response"
                    );
                    return;
                }
            }
        });

        debug!(target: LOG_TARGET, "Sending SearchKernels response stream to client");
        Ok(Response::new(rx))
    }

    async fn search_utxos(
        &self,
        request: Request<tari_rpc::SearchUtxosRequest>,
    ) -> Result<Response<Self::SearchUtxosStream>, Status> {
        if !self.is_method_enabled(GrpcMethod::SearchUtxos) {
            return Err(Status::permission_denied("`SearchUtxos` method not made available"));
        }
        let report_error_flag = self.report_error_flag();
        debug!(target: LOG_TARGET, "Incoming GRPC request for SearchUtxos");
        let request = request.into_inner();

        let outputs = request
            .commitments
            .into_iter()
            .map(|s| Commitment::from_canonical_bytes(&s))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| {
                obscure_error_if_true(
                    report_error_flag,
                    Status::invalid_argument(format!("Invalid commitments provided '{}'", e)),
                )
            })?;

        let mut handler = self.node_service.clone();

        let (mut tx, rx) = mpsc::channel(GET_BLOCKS_PAGE_SIZE);
        task::spawn(async move {
            let blocks = match handler.fetch_blocks_with_utxos(outputs).await {
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
                let result = block.try_into().map_err(|err| {
                    obscure_error_if_true(
                        report_error_flag,
                        Status::internal(format!("Could not provide block:{}", err)),
                    )
                });
                if tx.send(result).await.is_err() {
                    warn!(
                        target: LOG_TARGET,
                        "[search_utxos] Request was cancelled while sending a response"
                    );
                }
            }
        });

        debug!(target: LOG_TARGET, "Sending SearchUtxos response stream to client");
        Ok(Response::new(rx))
    }

    #[allow(clippy::useless_conversion)]
    async fn fetch_matching_utxos(
        &self,
        request: Request<tari_rpc::FetchMatchingUtxosRequest>,
    ) -> Result<Response<Self::FetchMatchingUtxosStream>, Status> {
        if !self.is_method_enabled(GrpcMethod::FetchMatchingUtxos) {
            return Err(Status::permission_denied(
                "`FetchMatchingUtxos` method not made available",
            ));
        }
        let report_error_flag = self.report_error_flag();
        debug!(target: LOG_TARGET, "Incoming GRPC request for FetchMatchingUtxos");
        let request = request.into_inner();

        let hashes = request
            .hashes
            .into_iter()
            .map(|s| s.try_into())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| {
                obscure_error_if_true(
                    report_error_flag,
                    Status::invalid_argument(format!("Invalid hashes provided '{}'", e)),
                )
            })?;

        let mut handler = self.node_service.clone();

        let (mut tx, rx) = mpsc::channel(GET_BLOCKS_PAGE_SIZE);
        task::spawn(async move {
            let outputs = match handler.fetch_matching_utxos(hashes).await {
                Err(err) => {
                    warn!(
                        target: LOG_TARGET,
                        "Error communicating with local base node: {:?}", err,
                    );
                    let _ignore = tx.send(Err(obscure_error_if_true(
                        report_error_flag,
                        Status::internal(format!("Error communicating with local base node: {}", err)),
                    )));
                    return;
                },
                Ok(data) => data,
            };
            for output in outputs {
                match output.try_into() {
                    Ok(output) => {
                        let resp = tari_rpc::FetchMatchingUtxosResponse { output: Some(output) };
                        if tx.send(Ok(resp)).await.is_err() {
                            warn!(
                                target: LOG_TARGET,
                                "[fetch_matching_utxos] Request was cancelled while sending a response"
                            );
                            return;
                        }
                    },
                    Err(err) => {
                        let _ignore = tx.send(Err(obscure_error_if_true(
                            report_error_flag,
                            Status::internal(format!("Error communicating with local base node: {}", err)),
                        )));
                        return;
                    },
                }
            }
        });

        debug!(
            target: LOG_TARGET,
            "Sending FindMatchingUtxos response stream to client"
        );
        Ok(Response::new(rx))
    }

    async fn get_block_timing(
        &self,
        request: Request<tari_rpc::HeightRequest>,
    ) -> Result<Response<tari_rpc::BlockTimingResponse>, Status> {
        if !self.is_method_enabled(GrpcMethod::GetBlockTiming) {
            return Err(Status::permission_denied("`GetBlockTiming` method not made available"));
        }
        let report_error_flag = self.report_error_flag();
        let request = request.into_inner();
        debug!(
            target: LOG_TARGET,
            "Incoming GRPC request for GetBlockTiming: from_tip: {:?} start_height: {:?} end_height: {:?}",
            request.from_tip,
            request.start_height,
            request.end_height
        );

        let mut handler = self.node_service.clone();
        let (start, end) = get_heights(&request, handler.clone()).await?;

        let num_requested = end.saturating_sub(start);
        if num_requested > BLOCK_TIMING_MAX_BLOCKS {
            warn!(
                target: LOG_TARGET,
                "GetBlockTiming request for too many blocks. Requested: {}. Max: {}.",
                num_requested,
                BLOCK_TIMING_MAX_BLOCKS
            );
            return Err(obscure_error_if_true(
                report_error_flag,
                Status::invalid_argument(format!(
                    "Exceeded max blocks request limit of {}",
                    BLOCK_TIMING_MAX_BLOCKS
                )),
            ));
        }

        let headers = handler.get_headers(start..=end).await.map_err(|err| {
            obscure_error_if_true(
                report_error_flag,
                Status::internal(format!("Could not provide headers:{}", err)),
            )
        })?;
        let headers = headers.into_iter().map(|h| h.into_header()).rev().collect::<Vec<_>>();

        let (max, min, avg) = BlockHeader::timing_stats(&headers);

        let response = tari_rpc::BlockTimingResponse { max, min, avg };
        debug!(target: LOG_TARGET, "Sending GetBlockTiming response to client");
        Ok(Response::new(response))
    }

    async fn get_constants(
        &self,
        request: Request<tari_rpc::BlockHeight>,
    ) -> Result<Response<tari_rpc::ConsensusConstants>, Status> {
        if !self.is_method_enabled(GrpcMethod::GetConstants) {
            return Err(Status::permission_denied("`GetConstants` method not made available"));
        }
        let report_error_flag = self.report_error_flag();
        debug!(target: LOG_TARGET, "Incoming GRPC request for GetConstants",);
        debug!(target: LOG_TARGET, "Sending GetConstants response to client");

        let block_height = request.into_inner().block_height;

        let consensus_manager = ConsensusManager::builder(self.network.as_network())
            .build()
            .map_err(|e| {
                obscure_error_if_true(
                    report_error_flag,
                    Status::unknown(format!("Could not retrieve consensus manager '{}'", e)),
                )
            })?;
        let consensus_constants = consensus_manager.consensus_constants(block_height);

        Ok(Response::new(tari_rpc::ConsensusConstants::from(
            consensus_constants.clone(),
        )))
    }

    async fn get_block_size(
        &self,
        request: Request<tari_rpc::BlockGroupRequest>,
    ) -> Result<Response<tari_rpc::BlockGroupResponse>, Status> {
        if !self.is_method_enabled(GrpcMethod::GetBlockSize) {
            return Err(Status::permission_denied("`GetBlockSize` method not made available"));
        }
        let report_error_flag = self.report_error_flag();
        get_block_group(
            self.node_service.clone(),
            request,
            BlockGroupType::BlockSize,
            report_error_flag,
        )
        .await
    }

    async fn get_block_fees(
        &self,
        request: Request<tari_rpc::BlockGroupRequest>,
    ) -> Result<Response<tari_rpc::BlockGroupResponse>, Status> {
        if !self.is_method_enabled(GrpcMethod::GetBlockFees) {
            return Err(Status::permission_denied("`GetBlockFees` method not made available"));
        }
        let report_error_flag = self.report_error_flag();
        get_block_group(
            self.node_service.clone(),
            request,
            BlockGroupType::BlockFees,
            report_error_flag,
        )
        .await
    }

    async fn get_version(&self, _request: Request<tari_rpc::Empty>) -> Result<Response<tari_rpc::StringValue>, Status> {
        if !self.is_method_enabled(GrpcMethod::GetVersion) {
            return Err(Status::permission_denied("`GetVersion` method not made available"));
        }
        Ok(Response::new(consts::APP_VERSION.to_string().into()))
    }

    async fn check_for_updates(
        &self,
        _request: Request<tari_rpc::Empty>,
    ) -> Result<Response<tari_rpc::SoftwareUpdate>, Status> {
        if !self.is_method_enabled(GrpcMethod::CheckForUpdates) {
            return Err(Status::permission_denied("`CheckForUpdates` method not made available"));
        }
        let mut resp = tari_rpc::SoftwareUpdate::default();

        if let Some(ref update) = *self.software_updater.update_notifier().borrow() {
            resp.has_update = true;
            resp.version = update.version().to_string();
            resp.sha = update.to_hash_hex();
            resp.download_url = update.download_url().to_string();
        }

        Ok(Response::new(resp))
    }

    async fn get_tokens_in_circulation(
        &self,
        request: Request<tari_rpc::GetBlocksRequest>,
    ) -> Result<Response<Self::GetTokensInCirculationStream>, Status> {
        if !self.is_method_enabled(GrpcMethod::GetTokensInCirculation) {
            return Err(Status::permission_denied(
                "`GetTokensInCirculation` method not made available",
            ));
        }
        let report_error_flag = self.report_error_flag();
        debug!(target: LOG_TARGET, "Incoming GRPC request for GetTokensInCirculation",);
        let request = request.into_inner();
        let mut heights = request.heights;
        heights = heights
            .drain(..cmp::min(heights.len(), GET_TOKENS_IN_CIRCULATION_MAX_HEIGHTS))
            .collect();
        let consensus_manager = ConsensusManager::builder(self.network.as_network())
            .build()
            .map_err(|e| {
                obscure_error_if_true(
                    report_error_flag,
                    Status::unknown(format!("Could not retrieve consensus manager '{}'", e)),
                )
            })?;

        let (mut tx, rx) = mpsc::channel(GET_TOKENS_IN_CIRCULATION_PAGE_SIZE);
        task::spawn(async move {
            let mut page: Vec<u64> = heights
                .drain(..cmp::min(heights.len(), GET_TOKENS_IN_CIRCULATION_PAGE_SIZE))
                .collect();
            while !page.is_empty() {
                let values: Vec<tari_rpc::ValueAtHeightResponse> = page
                    .clone()
                    .into_iter()
                    .map(|height| tari_rpc::ValueAtHeightResponse {
                        height,
                        value: consensus_manager.emission_schedule().supply_at_block(height).into(),
                    })
                    .collect();
                let result_size = values.len();
                for value in values {
                    if tx.send(Ok(value)).await.is_err() {
                        warn!(
                            target: LOG_TARGET,
                            "[get_tokens_in_circulation] Request was cancelled while sending a response"
                        );
                        return;
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

    async fn get_sync_progress(
        &self,
        _request: Request<tari_rpc::Empty>,
    ) -> Result<Response<tari_rpc::SyncProgressResponse>, Status> {
        if !self.is_method_enabled(GrpcMethod::GetSyncProgress) {
            return Err(Status::permission_denied("`GetSyncProgress` method not made available"));
        }
        let state = self
            .state_machine_handle
            .get_status_info_watch()
            .borrow()
            .state_info
            .clone();
        let response = match state {
            StateInfo::HeaderSync(None) => tari_rpc::SyncProgressResponse {
                tip_height: 0,
                local_height: 0,
                state: tari_rpc::SyncState::HeaderStarting.into(),
            },
            StateInfo::HeaderSync(Some(info)) => tari_rpc::SyncProgressResponse {
                tip_height: info.tip_height,
                local_height: info.local_height,
                state: tari_rpc::SyncState::Header.into(),
            },
            StateInfo::Connecting(_) => tari_rpc::SyncProgressResponse {
                tip_height: 0,
                local_height: 0,
                state: tari_rpc::SyncState::BlockStarting.into(),
            },
            StateInfo::BlockSync(info) => tari_rpc::SyncProgressResponse {
                tip_height: info.tip_height,
                local_height: info.local_height,
                state: tari_rpc::SyncState::Block.into(),
            },
            _ => tari_rpc::SyncProgressResponse {
                tip_height: 0,
                local_height: 0,
                state: if state.is_synced() {
                    tari_rpc::SyncState::Done.into()
                } else {
                    tari_rpc::SyncState::Startup.into()
                },
            },
        };
        Ok(Response::new(response))
    }

    async fn get_sync_info(
        &self,
        _request: Request<tari_rpc::Empty>,
    ) -> Result<Response<tari_rpc::SyncInfoResponse>, Status> {
        if !self.is_method_enabled(GrpcMethod::GetSyncInfo) {
            return Err(Status::permission_denied("`GetSyncInfo` method not made available"));
        }
        debug!(target: LOG_TARGET, "Incoming GRPC request for BN sync data");
        let response = self
            .state_machine_handle
            .get_status_info_watch()
            .borrow()
            .state_info
            .get_block_sync_info()
            .map(|info| {
                let node_ids = info.sync_peer.node_id().to_string().into_bytes();
                tari_rpc::SyncInfoResponse {
                    tip_height: info.tip_height,
                    local_height: info.local_height,
                    peer_node_id: vec![node_ids],
                }
            })
            .unwrap_or_default();

        debug!(target: LOG_TARGET, "Sending SyncData response to client");
        Ok(Response::new(response))
    }

    // casting here is okay as we cannot have more than u32 kernels in a block
    #[allow(clippy::cast_possible_truncation)]
    async fn get_header_by_hash(
        &self,
        request: Request<tari_rpc::GetHeaderByHashRequest>,
    ) -> Result<Response<tari_rpc::BlockHeaderResponse>, Status> {
        if !self.is_method_enabled(GrpcMethod::GetHeaderByHash) {
            return Err(Status::permission_denied("`GetHeaderByHash` method not made available"));
        }
        let report_error_flag = self.report_error_flag();
        let tari_rpc::GetHeaderByHashRequest { hash } = request.into_inner();
        let mut node_service = self.node_service.clone();
        let hash_hex = hash.to_hex();
        let block_hash = hash.try_into().map_err(|e| {
            obscure_error_if_true(
                report_error_flag,
                Status::invalid_argument(format!("Malformed block hash '{}'", e)),
            )
        })?;
        let block = node_service
            .get_block_by_hash(block_hash)
            .await
            .map_err(|err| obscure_error_if_true(report_error_flag, Status::internal(err.to_string())))?
            .ok_or_else(|| {
                obscure_error_if_true(
                    report_error_flag,
                    Status::not_found(format!("Header not found with hash `{}`", hash_hex)),
                )
            })?;

        let (block, acc_data, confirmations) = block.dissolve();
        let total_block_reward = self
            .consensus_rules
            .calculate_coinbase_and_fees(block.header.height, block.body.kernels())
            .map_err(|e| obscure_error_if_true(report_error_flag, Status::out_of_range(e)))?;

        let resp = tari_rpc::BlockHeaderResponse {
            difficulty: acc_data.achieved_difficulty.into(),
            num_transactions: block.body.kernels().len() as u32,
            confirmations,
            header: Some(block.header.into()),
            reward: total_block_reward.into(),
        };

        Ok(Response::new(resp))
    }

    async fn identify(&self, _: Request<tari_rpc::Empty>) -> Result<Response<tari_rpc::NodeIdentity>, Status> {
        if !self.is_method_enabled(GrpcMethod::Identify) {
            return Err(Status::permission_denied("`Identify` method not made available"));
        }
        let identity = self.comms.node_identity_ref();
        Ok(Response::new(tari_rpc::NodeIdentity {
            public_key: identity.public_key().to_vec(),
            public_addresses: identity.public_addresses().iter().map(|a| a.to_string()).collect(),
            node_id: identity.node_id().to_vec(),
        }))
    }

    async fn get_network_status(
        &self,
        _: Request<tari_rpc::Empty>,
    ) -> Result<Response<tari_rpc::NetworkStatusResponse>, Status> {
        if !self.is_method_enabled(GrpcMethod::GetNetworkStatus) {
            return Err(Status::permission_denied(
                "`GetNetworkStatus` method not made available",
            ));
        }
        let report_error_flag = self.report_error_flag();
        let status = self
            .comms
            .connectivity()
            .get_connectivity_status()
            .await
            .map_err(|err| obscure_error_if_true(report_error_flag, Status::internal(err.to_string())))?;

        let latency = self
            .liveness
            .clone()
            .get_network_avg_latency()
            .await
            .map_err(|err| obscure_error_if_true(report_error_flag, Status::internal(err.to_string())))?;

        let resp = tari_rpc::NetworkStatusResponse {
            status: tari_rpc::ConnectivityStatus::from(status) as i32,
            avg_latency_ms: latency
                .map(|l| u32::try_from(l.as_millis()).unwrap_or(u32::MAX))
                .unwrap_or(0),
            num_node_connections: u32::try_from(status.num_connected_nodes()).map_err(|e| {
                obscure_error_if_true(
                    report_error_flag,
                    Status::internal(format!("Error converting usize to u32 '{}'", e)),
                )
            })?,
        };

        Ok(Response::new(resp))
    }

    async fn list_connected_peers(
        &self,
        _: Request<tari_rpc::Empty>,
    ) -> Result<Response<tari_rpc::ListConnectedPeersResponse>, Status> {
        if !self.is_method_enabled(GrpcMethod::ListConnectedPeers) {
            return Err(Status::permission_denied(
                "`ListConnectedPeers` method not made available",
            ));
        }
        let report_error_flag = self.report_error_flag();
        let mut connectivity = self.comms.connectivity();
        let peer_manager = self.comms.peer_manager();
        let connected_peers = connectivity
            .get_active_connections()
            .await
            .map_err(|err| obscure_error_if_true(report_error_flag, Status::internal(err.to_string())))?;

        let mut peers = Vec::with_capacity(connected_peers.len());
        for peer in connected_peers {
            peers.push(
                peer_manager
                    .find_by_node_id(peer.peer_node_id())
                    .await
                    .map_err(|err| obscure_error_if_true(report_error_flag, Status::internal(err.to_string())))?
                    .ok_or_else(|| {
                        obscure_error_if_true(
                            report_error_flag,
                            Status::not_found(format!("Peer {} not found", peer.peer_node_id())),
                        )
                    })?,
            );
        }

        let resp = tari_rpc::ListConnectedPeersResponse {
            connected_peers: peers.into_iter().map(Into::into).collect(),
        };

        Ok(Response::new(resp))
    }

    async fn get_mempool_stats(
        &self,
        _: Request<tari_rpc::Empty>,
    ) -> Result<Response<tari_rpc::MempoolStatsResponse>, Status> {
        if !self.is_method_enabled(GrpcMethod::GetMempoolStats) {
            return Err(Status::permission_denied("`GetMempoolStats` method not made available"));
        }
        let report_error_flag = self.report_error_flag();
        let mut mempool_handle = self.mempool_service.clone();

        let mempool_stats = mempool_handle.get_mempool_stats().await.map_err(|e| {
            error!(target: LOG_TARGET, "Error submitting query:{}", e);
            obscure_error_if_true(report_error_flag, Status::internal(e.to_string()))
        })?;

        let response = tari_rpc::MempoolStatsResponse {
            unconfirmed_txs: mempool_stats.unconfirmed_txs,
            reorg_txs: mempool_stats.reorg_txs,
            unconfirmed_weight: mempool_stats.unconfirmed_weight,
        };

        Ok(Response::new(response))
    }

    async fn get_shard_key(
        &self,
        request: Request<tari_rpc::GetShardKeyRequest>,
    ) -> Result<Response<tari_rpc::GetShardKeyResponse>, Status> {
        if !self.is_method_enabled(GrpcMethod::GetShardKey) {
            return Err(Status::permission_denied("`GetShardKey` method not made available"));
        }
        let request = request.into_inner();
        let report_error_flag = self.report_error_flag();
        let mut handler = self.node_service.clone();
        let public_key = PublicKey::from_canonical_bytes(&request.public_key)
            .map_err(|e| obscure_error_if_true(report_error_flag, Status::invalid_argument(e.to_string())))?;

        let shard_key = handler.get_shard_key(request.height, public_key).await.map_err(|e| {
            error!(target: LOG_TARGET, "Error {}", e);
            obscure_error_if_true(report_error_flag, Status::internal(e.to_string()))
        })?;
        if let Some(shard_key) = shard_key {
            Ok(Response::new(tari_rpc::GetShardKeyResponse {
                shard_key: shard_key.to_vec(),
                found: true,
            }))
        } else {
            Ok(Response::new(tari_rpc::GetShardKeyResponse {
                shard_key: vec![],
                found: false,
            }))
        }
    }

    async fn get_active_validator_nodes(
        &self,
        request: Request<tari_rpc::GetActiveValidatorNodesRequest>,
    ) -> Result<Response<Self::GetActiveValidatorNodesStream>, Status> {
        if !self.is_method_enabled(GrpcMethod::GetActiveValidatorNodes) {
            return Err(Status::permission_denied(
                "`GetActiveValidatorNodes` method not made available",
            ));
        }
        let request = request.into_inner();
        debug!(target: LOG_TARGET, "Incoming GRPC request for GetActiveValidatorNodes");

        let mut handler = self.node_service.clone();
        let (mut tx, rx) = mpsc::channel(1000);

        task::spawn(async move {
            let active_validator_nodes = match handler.get_active_validator_nodes(request.height).await {
                Err(err) => {
                    warn!(target: LOG_TARGET, "Base node service error: {}", err,);
                    return;
                },
                Ok(data) => data,
            };

            for (public_key, shard_key) in active_validator_nodes {
                let active_validator_node = tari_rpc::GetActiveValidatorNodesResponse {
                    public_key: public_key.to_vec(),
                    shard_key: shard_key.to_vec(),
                };

                if tx.send(Ok(active_validator_node)).await.is_err() {
                    debug!(
                        target: LOG_TARGET,
                        "[get_active_validator_nodes] Client has disconnected before stream completed"
                    );
                    return;
                }
            }
        });
        debug!(
            target: LOG_TARGET,
            "Sending GetActiveValidatorNodes response stream to client"
        );
        Ok(Response::new(rx))
    }

    async fn get_template_registrations(
        &self,
        request: Request<tari_rpc::GetTemplateRegistrationsRequest>,
    ) -> Result<Response<Self::GetTemplateRegistrationsStream>, Status> {
        if !self.is_method_enabled(GrpcMethod::GetTemplateRegistrations) {
            return Err(Status::permission_denied(
                "`GetTemplateRegistrations` method not made available",
            ));
        }
        let request = request.into_inner();
        let report_error_flag = self.report_error_flag();
        debug!(target: LOG_TARGET, "Incoming GRPC request for GetTemplateRegistrations");

        let (mut tx, rx) = mpsc::channel(10);

        let start_hash = Some(request.start_hash)
            .filter(|x| !x.is_empty())
            .map(FixedHash::try_from)
            .transpose()
            .map_err(|e| {
                obscure_error_if_true(
                    report_error_flag,
                    Status::invalid_argument(format!("Invalid start_hash '{}'", e)),
                )
            })?;

        let mut node_service = self.node_service.clone();

        let start_height = match start_hash {
            Some(hash) => {
                let header = node_service
                    .get_header_by_hash(hash)
                    .await
                    .map_err(|err| obscure_error_if_true(self.report_grpc_error, Status::internal(err.to_string())))?;
                header.map(|h| h.height()).ok_or_else(|| {
                    obscure_error_if_true(report_error_flag, Status::not_found("Start hash not found"))
                })?
            },
            None => 0,
        };

        if request.count == 0 {
            return Ok(Response::new(rx));
        }

        let end_height = start_height.checked_add(request.count).ok_or_else(|| {
            obscure_error_if_true(
                report_error_flag,
                Status::invalid_argument("Request start height + count overflows u64"),
            )
        })?;

        task::spawn(async move {
            let template_registrations = match node_service.get_template_registrations(start_height, end_height).await {
                Err(err) => {
                    warn!(target: LOG_TARGET, "Base node service error: {}", err);
                    return;
                },
                Ok(data) => data,
            };

            for template_registration in template_registrations {
                let registration = template_registration.registration_data.into();

                let resp = tari_rpc::GetTemplateRegistrationResponse {
                    utxo_hash: template_registration.output_hash.to_vec(),
                    registration: Some(registration),
                };

                if tx.send(Ok(resp)).await.is_err() {
                    debug!(
                        target: LOG_TARGET,
                        "[get_template_registrations] Client has disconnected before stream completed"
                    );
                    return;
                }
            }
        });
        debug!(
            target: LOG_TARGET,
            "Sending GetTemplateRegistrations response stream to client"
        );
        Ok(Response::new(rx))
    }

    #[allow(clippy::too_many_lines)]
    async fn get_side_chain_utxos(
        &self,
        request: Request<tari_rpc::GetSideChainUtxosRequest>,
    ) -> Result<Response<Self::GetSideChainUtxosStream>, Status> {
        if !self.is_method_enabled(GrpcMethod::GetSideChainUtxos) {
            return Err(Status::permission_denied(
                "`GetSideChainUtxos` method not made available",
            ));
        }
        let request = request.into_inner();
        let report_error_flag = self.report_error_flag();
        debug!(target: LOG_TARGET, "Incoming GRPC request for GetTemplateRegistrations");

        let (mut tx, rx) = mpsc::channel(10);

        let start_hash = Some(request.start_hash)
            .filter(|x| !x.is_empty())
            .map(FixedHash::try_from)
            .transpose()
            .map_err(|e| {
                obscure_error_if_true(
                    report_error_flag,
                    Status::invalid_argument(format!("Invalid start_hash '{}'", e)),
                )
            })?;

        let mut node_service = self.node_service.clone();

        let start_header = match start_hash {
            Some(hash) => node_service
                .get_header_by_hash(hash)
                .await
                .map_err(|err| obscure_error_if_true(self.report_grpc_error, Status::internal(err.to_string())))?
                .ok_or_else(|| obscure_error_if_true(report_error_flag, Status::not_found("Start hash not found")))?,
            None => node_service
                .get_header(0)
                .await
                .map_err(|err| obscure_error_if_true(self.report_grpc_error, Status::internal(err.to_string())))?
                .ok_or_else(|| {
                    obscure_error_if_true(report_error_flag, Status::unavailable("Genesis block not available"))
                })?,
        };

        if request.count == 0 {
            return Ok(Response::new(rx));
        }

        let start_height = start_header.height();
        let end_height = start_height.checked_add(request.count - 1).ok_or_else(|| {
            obscure_error_if_true(
                report_error_flag,
                Status::invalid_argument("Request start height + count overflows u64"),
            )
        })?;

        task::spawn(async move {
            let mut current_header = start_header;

            for height in start_height..=end_height {
                let header_hash = *current_header.hash();
                let utxos = match node_service.fetch_unspent_utxos_in_block(header_hash).await {
                    Ok(utxos) => utxos,
                    Err(e) => {
                        warn!(target: LOG_TARGET, "Base node service error: {}", e);
                        return;
                    },
                };

                let next_header = match node_service.get_header(height.saturating_add(1)).await {
                    Ok(h) => h,
                    Err(e) => {
                        let _ignore = tx.send(Err(obscure_error_if_true(
                            report_error_flag,
                            Status::internal(e.to_string()),
                        )));
                        return;
                    },
                };

                let sidechain_outputs = utxos
                    .into_iter()
                    .filter(|u| u.features.output_type.is_sidechain_type())
                    .map(TryInto::try_into)
                    .collect::<Result<Vec<_>, _>>();

                match sidechain_outputs {
                    Ok(outputs) => {
                        let resp = tari_rpc::GetSideChainUtxosResponse {
                            block_info: Some(tari_rpc::BlockInfo {
                                height: current_header.height(),
                                hash: header_hash.to_vec(),
                                next_block_hash: next_header.as_ref().map(|h| h.hash().to_vec()).unwrap_or_default(),
                            }),
                            outputs,
                        };

                        if tx.send(Ok(resp)).await.is_err() {
                            debug!(
                                target: LOG_TARGET,
                                "[get_template_registrations] Client has disconnected before stream completed"
                            );
                            return;
                        }
                    },
                    Err(e) => {
                        warn!(
                            target: LOG_TARGET,
                            "Error sending converting sidechain output for GRPC: {}", e
                        );
                        let _ignore = tx
                            .send(Err(obscure_error_if_true(
                                report_error_flag,
                                Status::internal(format!("Error converting sidechain output: {}", e)),
                            )))
                            .await;
                        return;
                    },
                };

                match next_header {
                    Some(header) => {
                        current_header = header;
                    },
                    None => break,
                }
            }
        });
        debug!(
            target: LOG_TARGET,
            "Sending GetTemplateRegistrations response stream to client"
        );
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
    report_error_flag: bool,
) -> Result<Response<tari_rpc::BlockGroupResponse>, Status> {
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

    let (start, end) = get_heights(&height_request, handler.clone())
        .await
        .map_err(|e| obscure_error_if_true(report_error_flag, e))?;

    let blocks = match handler.get_blocks(start..=end, false).await {
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
        CalcType::Quantile => {
            return Err(obscure_error_if_true(
                report_error_flag,
                Status::unimplemented("Quantile has not been implemented"),
            ))
        },
        CalcType::Quartile => {
            return Err(obscure_error_if_true(
                report_error_flag,
                Status::unimplemented("Quartile has not been implemented"),
            ))
        },
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
