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

use either::Either;
use futures::{channel::mpsc, SinkExt};
use log::*;
use tari_app_grpc::{
    tari_rpc,
    tari_rpc::{CalcType, Sorting},
};
use tari_app_utilities::consts;
use tari_common_types::types::{Commitment, PublicKey, Signature};
use tari_comms::{Bytes, CommsNode};
use tari_core::{
    base_node::{
        comms_interface::CommsInterfaceError,
        state_machine_service::states::StateInfo,
        LocalNodeCommsInterface,
        StateMachineHandle,
    },
    blocks::{Block, BlockHeader, NewBlockTemplate},
    chain_storage::{ChainStorageError, PrunedOutput},
    consensus::{emission::Emission, ConsensusManager, NetworkConsensus},
    iterators::NonOverlappingIntegerPairIter,
    mempool::{service::LocalMempoolService, TxStorageResponse},
    proof_of_work::PowAlgorithm,
    transactions::transaction_components::Transaction,
};
use tari_p2p::{auto_update::SoftwareUpdaterHandle, services::liveness::LivenessHandle};
use tari_utilities::{hex::Hex, message_format::MessageFormat, ByteArray, Hashable};
use tokio::task;
use tonic::{Request, Response, Status};

use crate::{
    builder::BaseNodeContext,
    grpc::{
        blocks::{block_fees, block_heights, block_size, GET_BLOCKS_MAX_HEIGHTS, GET_BLOCKS_PAGE_SIZE},
        helpers::{mean, median},
    },
};

const LOG_TARGET: &str = "tari::base_node::grpc";
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

pub struct BaseNodeGrpcServer {
    node_service: LocalNodeCommsInterface,
    mempool_service: LocalMempoolService,
    network: NetworkConsensus,
    state_machine_handle: StateMachineHandle,
    consensus_rules: ConsensusManager,
    software_updater: SoftwareUpdaterHandle,
    comms: CommsNode,
    liveness: LivenessHandle,
}

impl BaseNodeGrpcServer {
    pub fn from_base_node_context(ctx: &BaseNodeContext) -> Self {
        Self {
            node_service: ctx.local_node(),
            mempool_service: ctx.local_mempool(),
            network: ctx.network().into(),
            state_machine_handle: ctx.state_machine(),
            consensus_rules: ctx.consensus_rules().clone(),
            software_updater: ctx.software_updater(),
            comms: ctx.base_node_comms().clone(),
            liveness: ctx.liveness(),
        }
    }
}

pub async fn get_heights(
    request: &tari_rpc::HeightRequest,
    handler: LocalNodeCommsInterface,
) -> Result<(u64, u64), Status> {
    block_heights(handler, request.start_height, request.end_height, request.from_tip).await
}

#[tonic::async_trait]
impl tari_rpc::base_node_server::BaseNode for BaseNodeGrpcServer {
    type FetchMatchingUtxosStream = mpsc::Receiver<Result<tari_rpc::FetchMatchingUtxosResponse, Status>>;
    type GetBlocksStream = mpsc::Receiver<Result<tari_rpc::HistoricalBlock, Status>>;
    type GetMempoolTransactionsStream = mpsc::Receiver<Result<tari_rpc::GetMempoolTransactionsResponse, Status>>;
    type GetNetworkDifficultyStream = mpsc::Receiver<Result<tari_rpc::NetworkDifficultyResponse, Status>>;
    type GetPeersStream = mpsc::Receiver<Result<tari_rpc::GetPeersResponse, Status>>;
    type GetTokensInCirculationStream = mpsc::Receiver<Result<tari_rpc::ValueAtHeightResponse, Status>>;
    type GetTokensStream = mpsc::Receiver<Result<tari_rpc::GetTokensResponse, Status>>;
    type ListAssetRegistrationsStream = mpsc::Receiver<Result<tari_rpc::ListAssetRegistrationsResponse, Status>>;
    type ListHeadersStream = mpsc::Receiver<Result<tari_rpc::BlockHeader, Status>>;
    type SearchKernelsStream = mpsc::Receiver<Result<tari_rpc::HistoricalBlock, Status>>;
    type SearchUtxosStream = mpsc::Receiver<Result<tari_rpc::HistoricalBlock, Status>>;

    async fn get_network_difficulty(
        &self,
        request: Request<tari_rpc::HeightRequest>,
    ) -> Result<Response<Self::GetNetworkDifficultyStream>, Status> {
        let request = request.into_inner();
        debug!(
            target: LOG_TARGET,
            "Incoming GRPC request for GetNetworkDifficulty: from_tip: {:?} start_height: {:?} end_height: {:?}",
            request.from_tip,
            request.start_height,
            request.end_height
        );
        let mut handler = self.node_service.clone();
        let (start_height, end_height) = get_heights(&request, handler.clone()).await?;
        // Overflow safety: checked in get_heights
        let num_requested = end_height - start_height;
        if num_requested > GET_DIFFICULTY_MAX_HEIGHTS {
            return Err(Status::invalid_argument(format!(
                "Number of headers requested exceeds maximum. Expected less than {} but got {}",
                GET_DIFFICULTY_MAX_HEIGHTS, num_requested
            )));
        }
        let (mut tx, rx) = mpsc::channel(cmp::min(num_requested as usize, GET_DIFFICULTY_PAGE_SIZE));

        task::spawn(async move {
            let page_iter = NonOverlappingIntegerPairIter::new(start_height, end_height + 1, GET_DIFFICULTY_PAGE_SIZE);
            for (start, end) in page_iter {
                // headers are returned by height
                let headers = match handler.get_headers(start..=end).await {
                    Ok(headers) => headers,
                    Err(err) => {
                        warn!(target: LOG_TARGET, "Base node service error: {:?}", err,);
                        let _ = tx
                            .send(Err(Status::internal("Internal error when fetching blocks")))
                            .await;
                        return;
                    },
                };

                if headers.is_empty() {
                    let _ = tx.send(Err(Status::invalid_argument(format!(
                        "No blocks found within range {} - {}",
                        start, end
                    ))));
                    return;
                }

                let mut headers_iter = headers.iter().peekable();

                while let Some(chain_header) = headers_iter.next() {
                    let current_difficulty = chain_header.accumulated_data().target_difficulty.as_u64();
                    let current_timestamp = chain_header.header().timestamp.as_u64();
                    let current_height = chain_header.header().height;
                    let pow_algo = chain_header.header().pow.pow_algo.as_u64();

                    let estimated_hash_rate = headers_iter
                        .peek()
                        .map(|chain_header| chain_header.header().timestamp.as_u64())
                        .and_then(|peeked_timestamp| {
                            // Sometimes blocks can have the same timestamp, lucky miner and some
                            // clock drift.
                            peeked_timestamp
                                .checked_sub(current_timestamp)
                                .filter(|td| *td > 0)
                                .map(|time_diff| current_timestamp / time_diff)
                        })
                        .unwrap_or(0);

                    let difficulty = tari_rpc::NetworkDifficultyResponse {
                        difficulty: current_difficulty,
                        estimated_hash_rate,
                        height: current_height,
                        timestamp: current_timestamp,
                        pow_algo,
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
                        warn!(
                            target: LOG_TARGET,
                            "Error sending converting transaction for GRPC:  {}", e
                        );
                        match tx.send(Err(Status::internal("Error converting transaction"))).await {
                            Ok(_) => (),
                            Err(send_err) => {
                                warn!(target: LOG_TARGET, "Error sending error to GRPC client: {}", send_err)
                            },
                        }
                        return;
                    },
                };

                match tx
                    .send(Ok(tari_rpc::GetMempoolTransactionsResponse {
                        transaction: Some(transaction),
                    }))
                    .await
                {
                    Ok(_) => (),
                    Err(err) => {
                        warn!(
                            target: LOG_TARGET,
                            "Error sending mempool transaction via GRPC:  {}", err
                        );
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
        debug!(target: LOG_TARGET, "Sending GetMempool response stream to client");
        Ok(Response::new(rx))
    }

    async fn list_headers(
        &self,
        request: Request<tari_rpc::ListHeadersRequest>,
    ) -> Result<Response<Self::ListHeadersStream>, Status> {
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
            Ok(data) => data.height_of_longest_chain(),
        };

        let sorting: Sorting = request.sorting();
        let num_headers = match request.num_headers {
            0 => LIST_HEADERS_DEFAULT_NUM_HEADERS,
            _ => request.num_headers,
        };

        let num_headers = cmp::min(num_headers, LIST_HEADERS_MAX_NUM_HEADERS);
        let (mut tx, rx) = mpsc::channel(LIST_HEADERS_PAGE_SIZE);

        let from_height = cmp::min(request.from_height, tip);

        let (header_range, is_reversed) = if from_height != 0 {
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
        } else {
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
        };

        task::spawn(async move {
            debug!(
                target: LOG_TARGET,
                "Starting base node request {}-{}",
                header_range.start(),
                header_range.end()
            );
            let page_iter = NonOverlappingIntegerPairIter::new(
                *header_range.start(),
                *header_range.end() + 1,
                LIST_HEADERS_PAGE_SIZE,
            );
            let page_iter = if is_reversed {
                Either::Left(page_iter.rev())
            } else {
                Either::Right(page_iter)
            };
            for (start, end) in page_iter {
                debug!(target: LOG_TARGET, "Page: {}-{}", start, end);
                // TODO: Better error handling
                let result_headers = match handler.get_headers(start..=end).await {
                    Err(err) => {
                        warn!(target: LOG_TARGET, "Internal base node service error: {}", err);
                        return;
                    },
                    Ok(data) => {
                        if is_reversed {
                            data.into_iter().rev().collect::<Vec<_>>()
                        } else {
                            data
                        }
                    },
                };
                let result_size = result_headers.len();
                debug!(target: LOG_TARGET, "Result headers: {}", result_size);

                for header in result_headers {
                    debug!(target: LOG_TARGET, "Sending block header: {}", header.height());
                    match tx.send(Ok(header.into_header().into())).await {
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
            }
        });

        debug!(target: LOG_TARGET, "Sending ListHeaders response stream to client");
        Ok(Response::new(rx))
    }

    async fn get_tokens(
        &self,
        request: Request<tari_rpc::GetTokensRequest>,
    ) -> Result<Response<Self::GetTokensStream>, Status> {
        let request = request.into_inner();
        debug!(
            target: LOG_TARGET,
            "Incoming GRPC request for GetTokens: asset_pub_key: {}, unique_ids: [{}]",
            request.asset_public_key.to_hex(),
            request
                .unique_ids
                .iter()
                .map(|s| s.to_hex())
                .collect::<Vec<_>>()
                .join(",")
        );

        let pub_key = PublicKey::from_bytes(&request.asset_public_key)
            .map_err(|err| Status::invalid_argument(format!("Asset public Key is not a valid public key:{}", err)))?;

        let mut handler = self.node_service.clone();
        let (mut tx, rx) = mpsc::channel(50);
        task::spawn(async move {
            let asset_pub_key_hex = request.asset_public_key.to_hex();
            debug!(
                target: LOG_TARGET,
                "Starting thread to process GetTokens: asset_pub_key: {}", asset_pub_key_hex,
            );
            let tokens = match handler.get_tokens(pub_key, request.unique_ids).await {
                Ok(tokens) => tokens,
                Err(err) => {
                    warn!(target: LOG_TARGET, "Error communicating with base node: {:?}", err,);
                    let _ = tx.send(Err(Status::internal("Internal error")));
                    return;
                },
            };

            debug!(
                target: LOG_TARGET,
                "Found {} tokens for {}",
                tokens.len(),
                asset_pub_key_hex
            );

            for token in tokens {
                let features = match token.features.clone().try_into() {
                    Ok(f) => f,
                    Err(err) => {
                        warn!(target: LOG_TARGET, "Could not convert features: {}", err,);
                        let _ = tx.send(Err(Status::internal(format!("Could not convert features:{}", err))));
                        break;
                    },
                };
                match tx
                    .send(Ok(tari_rpc::GetTokensResponse {
                        asset_public_key: token
                            .features
                            .parent_public_key
                            .map(|pk| pk.to_vec())
                            .unwrap_or_default(),
                        unique_id: token.features.unique_id.unwrap_or_default(),
                        owner_commitment: token.commitment.to_vec(),
                        mined_in_block: vec![],
                        mined_height: 0,
                        script: token.script.as_bytes(),
                        features: Some(features),
                    }))
                    .await
                {
                    Ok(_) => (),
                    Err(err) => {
                        warn!(target: LOG_TARGET, "Error sending token via GRPC:  {}", err);
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
        Ok(Response::new(rx))
    }

    async fn get_asset_metadata(
        &self,
        request: Request<tari_rpc::GetAssetMetadataRequest>,
    ) -> Result<Response<tari_rpc::GetAssetMetadataResponse>, Status> {
        let request = request.into_inner();

        let mut handler = self.node_service.clone();
        let metadata = handler
            .get_asset_metadata(
                PublicKey::from_bytes(&request.asset_public_key)
                    .map_err(|_e| Status::invalid_argument("Not a valid asset public key"))?,
            )
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        if let Some(m) = metadata {
            let mined_height = m.mined_height;
            let mined_in_block = m.header_hash.clone();
            match m.output {
                PrunedOutput::Pruned {
                    output_hash: _,
                    witness_hash: _,
                } => return Err(Status::not_found("Output has been pruned")),
                PrunedOutput::NotPruned { output } => {
                    if let Some(ref asset) = output.features.asset {
                        const ASSET_METADATA_TEMPLATE_ID: u32 = 1;
                        if asset.template_ids_implemented.contains(&ASSET_METADATA_TEMPLATE_ID) {
                            // TODO: move to a better location, or better yet, have the grpc caller split the metadata
                            let m = String::from_utf8(Vec::from(&output.features.metadata[1..])).unwrap();
                            let mut m = m
                                .as_str()
                                .split('|')
                                .map(|s| s.to_string())
                                .collect::<Vec<String>>()
                                .into_iter();
                            let name = m.next();
                            let description = m.next();
                            let image = m.next();

                            // TODO Perhaps this should just return metadata and have the client read the metadata in a
                            // pattern described by the template
                            return Ok(Response::new(tari_rpc::GetAssetMetadataResponse {
                                name: name.unwrap_or_default(),
                                description: description.unwrap_or_default(),
                                image: image.unwrap_or_default(),
                                owner_commitment: Vec::from(output.commitment.as_bytes()),
                                features: Some(output.features.clone().into()),
                                mined_height,
                                mined_in_block,
                            }));
                        }
                    }
                    return Ok(Response::new(tari_rpc::GetAssetMetadataResponse {
                        name: "".into(),
                        description: "".into(),
                        image: "".into(),
                        owner_commitment: Vec::from(output.commitment.as_bytes()),
                        features: Some(output.features.into()),
                        mined_height,
                        mined_in_block,
                    }));
                },
            };
            // Err(Status::unknown("Could not find a matching arm"))
        } else {
            Err(Status::not_found("Could not find any utxo"))
        }
    }

    async fn list_asset_registrations(
        &self,
        request: Request<tari_rpc::ListAssetRegistrationsRequest>,
    ) -> Result<Response<Self::ListAssetRegistrationsStream>, Status> {
        let request = request.into_inner();

        let mut handler = self.node_service.clone();
        let (mut tx, rx) = mpsc::channel(50);
        task::spawn(async move {
            debug!(
                target: LOG_TARGET,
                "Starting thread to process ListAssetRegistrationsStream: {:?}", request,
            );
            let start = request.offset as usize;
            let end = (request.offset + request.count) as usize;

            let outputs = match handler.get_asset_registrations(start..=end).await {
                Ok(outputs) => outputs,
                Err(err) => {
                    warn!(target: LOG_TARGET, "Error communicating with base node: {:?}", err,);
                    let _ = tx.send(Err(Status::internal("Internal error")));
                    return;
                },
            };

            debug!(target: LOG_TARGET, "Found {} tokens", outputs.len(),);

            for output in outputs {
                let mined_height = output.mined_height;
                let header_hash = output.header_hash;
                let output = match output.output.into_unpruned_output() {
                    Some(output) => output,
                    None => {
                        continue;
                    },
                };
                let features = match output.features.clone().try_into() {
                    Ok(f) => f,
                    Err(err) => {
                        warn!(target: LOG_TARGET, "Could not convert features: {}", err,);
                        let _ = tx.send(Err(Status::internal(format!("Could not convert features:{}", err))));
                        break;
                    },
                };
                let response = tari_rpc::ListAssetRegistrationsResponse {
                    asset_public_key: output
                        .features
                        .asset
                        .map(|asset| asset.public_key.to_vec())
                        .unwrap_or_default(),
                    unique_id: output.features.unique_id.unwrap_or_default(),
                    owner_commitment: output.commitment.to_vec(),
                    mined_in_block: header_hash,
                    mined_height,
                    script: output.script.as_bytes(),
                    features: Some(features),
                };
                if let Err(err) = tx.send(Ok(response)).await {
                    // This error can only happen if the Receiver has dropped, meaning the request was
                    // cancelled/disconnected
                    warn!(target: LOG_TARGET, "Error sending error to GRPC client: {}", err);
                    return;
                }
            }
        });
        Ok(Response::new(rx))
    }

    async fn get_new_block_template(
        &self,
        request: Request<tari_rpc::NewBlockTemplateRequest>,
    ) -> Result<Response<tari_rpc::NewBlockTemplateResponse>, Status> {
        let request = request.into_inner();
        debug!(target: LOG_TARGET, "Incoming GRPC request for get new block template");
        trace!(target: LOG_TARGET, "Request {:?}", request);
        let algo: PowAlgorithm = ((request.algo)
            .ok_or_else(|| Status::invalid_argument("No valid pow algo selected".to_string()))?
            .pow_algo as u64)
            .try_into()
            .map_err(|_| Status::invalid_argument("No valid pow algo selected".to_string()))?;
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
                Status::internal(e.to_string())
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
            new_block_template: Some(new_template.try_into().map_err(Status::internal)?),

            initial_sync_achieved: (*status_watch.borrow()).bootstrapped,
        };

        debug!(target: LOG_TARGET, "Sending GetNewBlockTemplate response to client");
        Ok(Response::new(response))
    }

    async fn get_new_block(
        &self,
        request: Request<tari_rpc::NewBlockTemplate>,
    ) -> Result<Response<tari_rpc::GetNewBlockResult>, Status> {
        let request = request.into_inner();
        debug!(target: LOG_TARGET, "Incoming GRPC request for get new block");
        let block_template: NewBlockTemplate = request
            .try_into()
            .map_err(|s| Status::invalid_argument(format!("Invalid block template: {}", s)))?;

        let mut handler = self.node_service.clone();

        let new_block = match handler.get_new_block(block_template).await {
            Ok(b) => b,
            Err(CommsInterfaceError::ChainStorageError(ChainStorageError::InvalidArguments { message, .. })) => {
                return Err(Status::invalid_argument(message));
            },
            Err(CommsInterfaceError::ChainStorageError(ChainStorageError::CannotCalculateNonTipMmr(msg))) => {
                let status = Status::with_details(
                    tonic::Code::FailedPrecondition,
                    msg,
                    Bytes::from_static(b"CannotCalculateNonTipMmr"),
                );
                return Err(status);
            },
            Err(e) => return Err(Status::internal(e.to_string())),
        };
        // construct response
        let block_hash = new_block.hash();
        let mining_hash = new_block.header.merged_mining_hash();
        let block: Option<tari_rpc::Block> = Some(new_block.try_into().map_err(Status::internal)?);

        let response = tari_rpc::GetNewBlockResult {
            block_hash,
            block,
            merge_mining_hash: mining_hash,
        };
        debug!(target: LOG_TARGET, "Sending GetNewBlock response to client");
        Ok(Response::new(response))
    }

    async fn submit_block(
        &self,
        request: Request<tari_rpc::Block>,
    ) -> Result<Response<tari_rpc::SubmitBlockResponse>, Status> {
        let request = request.into_inner();
        let block = Block::try_from(request)
            .map_err(|e| Status::invalid_argument(format!("Failed to convert arguments. Invalid block: {:?}", e)))?;
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
            .map_err(|e| Status::internal(e.to_string()))?;

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
        let request = request.into_inner();
        let txn: Transaction = request
            .transaction
            .ok_or_else(|| Status::invalid_argument("Transaction is empty"))?
            .try_into()
            .map_err(|e| Status::invalid_argument(format!("Failed to convert arguments. Invalid transaction.{}", e)))?;
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
            Status::internal(e.to_string())
        })?;
        let response = match res {
            TxStorageResponse::UnconfirmedPool => tari_rpc::SubmitTransactionResponse {
                result: tari_rpc::SubmitTransactionResult::Accepted.into(),
            },
            TxStorageResponse::ReorgPool | TxStorageResponse::NotStoredAlreadySpent => {
                tari_rpc::SubmitTransactionResponse {
                    result: tari_rpc::SubmitTransactionResult::AlreadyMined.into(),
                }
            },
            TxStorageResponse::NotStored |
            TxStorageResponse::NotStoredOrphan |
            TxStorageResponse::NotStoredConsensus |
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
        let request = request.into_inner();
        let excess_sig: Signature = request
            .excess_sig
            .ok_or_else(|| Status::invalid_argument("excess_sig not provided".to_string()))?
            .try_into()
            .map_err(|_| Status::invalid_argument("excess_sig could not be converted".to_string()))?;
        debug!(
            target: LOG_TARGET,
            "Received TransactionState request from client ({} excess_sig)",
            excess_sig
                .to_json()
                .unwrap_or_else(|_| "Failed to serialize signature".into()),
        );
        let mut node_handler = self.node_service.clone();
        let mut mem_handler = self.mempool_service.clone();

        let base_node_response = node_handler
            .get_kernel_by_excess_sig(excess_sig.clone())
            .await
            .map_err(|e| {
                error!(target: LOG_TARGET, "Error submitting query:{}", e);
                Status::internal(e.to_string())
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
                Status::internal(e.to_string())
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
            TxStorageResponse::NotStoredTimeLocked => tari_rpc::TransactionStateResponse {
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
        debug!(target: LOG_TARGET, "Incoming GRPC request for get all peers");

        let peers = self
            .comms
            .peer_manager()
            .all()
            .await
            .map_err(|e| Status::unknown(e.to_string()))?;
        let peers: Vec<tari_rpc::Peer> = peers.into_iter().map(|p| p.into()).collect();
        let (mut tx, rx) = mpsc::channel(peers.len());
        task::spawn(async move {
            for peer in peers {
                let response = tari_rpc::GetPeersResponse { peer: Some(peer) };
                match tx.send(Ok(response)).await {
                    Ok(_) => (),
                    Err(err) => {
                        warn!(target: LOG_TARGET, "Error sending peer via GRPC:  {}", err);
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

        debug!(target: LOG_TARGET, "Sending peers response to client");
        Ok(Response::new(rx))
    }

    async fn get_blocks(
        &self,
        request: Request<tari_rpc::GetBlocksRequest>,
    ) -> Result<Response<Self::GetBlocksStream>, Status> {
        let request = request.into_inner();
        debug!(
            target: LOG_TARGET,
            "Incoming GRPC request for GetBlocks: {:?}", request.heights
        );

        let mut heights = request.heights;
        if heights.is_empty() {
            return Err(Status::invalid_argument("heights cannot be empty"));
        }

        heights.truncate(GET_BLOCKS_MAX_HEIGHTS);
        heights.sort_unstable();
        // unreachable panic: `heights` is not empty
        let start = *heights.first().expect("unreachable");
        let end = *heights.last().expect("unreachable");

        let mut handler = self.node_service.clone();
        let (mut tx, rx) = mpsc::channel(GET_BLOCKS_PAGE_SIZE);
        task::spawn(async move {
            let page_iter = NonOverlappingIntegerPairIter::new(start, end + 1, GET_BLOCKS_PAGE_SIZE);
            for (start, end) in page_iter {
                let blocks = match handler.get_blocks(start..=end).await {
                    Err(err) => {
                        warn!(
                            target: LOG_TARGET,
                            "Error communicating with local base node: {:?}", err,
                        );
                        return;
                    },
                    Ok(data) => {
                        // TODO: Change this interface to a start-end ranged one (clients like the block explorer
                        // convert start end ranges to integer lists anyway)
                        data.into_iter().filter(|b| heights.contains(&b.header().height))
                    },
                };

                for block in blocks {
                    debug!(
                        target: LOG_TARGET,
                        "GetBlock GRPC sending block #{}",
                        block.header().height
                    );
                    match tx
                        .send(
                            block
                                .try_into()
                                .map_err(|err| Status::internal(format!("Could not provide block: {}", err))),
                        )
                        .await
                    {
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
            }
        });

        debug!(target: LOG_TARGET, "Sending GetBlocks response stream to client");
        Ok(Response::new(rx))
    }

    async fn get_tip_info(
        &self,
        _request: Request<tari_rpc::Empty>,
    ) -> Result<Response<tari_rpc::TipInfoResponse>, Status> {
        debug!(target: LOG_TARGET, "Incoming GRPC request for BN tip data");

        let mut handler = self.node_service.clone();

        let meta = handler
            .get_metadata()
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        // Determine if we are bootstrapped
        let status_watch = self.state_machine_handle.get_status_info_watch();
        let state: tari_rpc::BaseNodeState = (&(*status_watch.borrow()).state_info).into();
        let response = tari_rpc::TipInfoResponse {
            metadata: Some(meta.into()),
            initial_sync_achieved: (*status_watch.borrow()).bootstrapped,
            base_node_state: state.into(),
        };

        debug!(target: LOG_TARGET, "Sending MetaData response to client");
        Ok(Response::new(response))
    }

    async fn search_kernels(
        &self,
        request: Request<tari_rpc::SearchKernelsRequest>,
    ) -> Result<Response<Self::SearchKernelsStream>, Status> {
        debug!(target: LOG_TARGET, "Incoming GRPC request for SearchKernels");
        let request = request.into_inner();

        let converted: Result<Vec<_>, _> = request.signatures.into_iter().map(|s| s.try_into()).collect();
        let kernels = converted.map_err(|_| Status::internal("Failed to convert one or more arguments."))?;

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
                match tx
                    .send(
                        block
                            .try_into()
                            .map_err(|err| Status::internal(format!("Could not provide block:{}", err))),
                    )
                    .await
                {
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

    async fn search_utxos(
        &self,
        request: Request<tari_rpc::SearchUtxosRequest>,
    ) -> Result<Response<Self::SearchUtxosStream>, Status> {
        debug!(target: LOG_TARGET, "Incoming GRPC request for SearchUtxos");
        let request = request.into_inner();

        let converted: Result<Vec<_>, _> = request
            .commitments
            .into_iter()
            .map(|s| Commitment::from_bytes(&s))
            .collect();
        let outputs = converted.map_err(|_| Status::internal("Failed to convert one or more arguments."))?;

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
                match tx
                    .send(
                        block
                            .try_into()
                            .map_err(|err| Status::internal(format!("Could not provide block:{}", err))),
                    )
                    .await
                {
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

        debug!(target: LOG_TARGET, "Sending SearchUtxos response stream to client");
        Ok(Response::new(rx))
    }

    #[allow(clippy::useless_conversion)]
    async fn fetch_matching_utxos(
        &self,
        request: Request<tari_rpc::FetchMatchingUtxosRequest>,
    ) -> Result<Response<Self::FetchMatchingUtxosStream>, Status> {
        debug!(target: LOG_TARGET, "Incoming GRPC request for FetchMatchingUtxos");
        let request = request.into_inner();

        let converted: Result<Vec<_>, _> = request.hashes.into_iter().map(|s| s.try_into()).collect();
        let hashes = converted.map_err(|_| Status::internal("Failed to convert one or more arguments."))?;

        let mut handler = self.node_service.clone();

        let (mut tx, rx) = mpsc::channel(GET_BLOCKS_PAGE_SIZE);
        task::spawn(async move {
            let outputs = match handler.fetch_matching_utxos(hashes).await {
                Err(err) => {
                    warn!(
                        target: LOG_TARGET,
                        "Error communicating with local base node: {:?}", err,
                    );
                    return;
                },
                Ok(data) => data,
            };
            for output in outputs {
                match tx
                    .send(Ok(tari_rpc::FetchMatchingUtxosResponse {
                        output: Some(output.into()),
                    }))
                    .await
                {
                    Ok(_) => (),
                    Err(err) => {
                        warn!(target: LOG_TARGET, "Error sending output via GRPC:  {}", err);

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

        debug!(
            target: LOG_TARGET,
            "Sending FindMatchingUtxos response stream to client"
        );
        Ok(Response::new(rx))
    }

    // deprecated
    async fn get_calc_timing(
        &self,
        request: Request<tari_rpc::HeightRequest>,
    ) -> Result<Response<tari_rpc::CalcTimingResponse>, Status> {
        debug!(
            target: LOG_TARGET,
            "Incoming GRPC request for deprecated GetCalcTiming. Forwarding to GetBlockTiming.",
        );

        let tari_rpc::BlockTimingResponse { max, min, avg } = self.get_block_timing(request).await?.into_inner();
        let response = tari_rpc::CalcTimingResponse { max, min, avg };

        Ok(Response::new(response))
    }

    async fn get_block_timing(
        &self,
        request: Request<tari_rpc::HeightRequest>,
    ) -> Result<Response<tari_rpc::BlockTimingResponse>, Status> {
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

        let headers = match handler.get_headers(start..=end).await {
            Ok(headers) => headers.into_iter().map(|h| h.into_header()).collect::<Vec<_>>(),
            Err(err) => {
                warn!(target: LOG_TARGET, "Error getting headers for GRPC client: {}", err);
                Vec::new()
            },
        };
        let (max, min, avg) = BlockHeader::timing_stats(&headers);

        let response = tari_rpc::BlockTimingResponse { max, min, avg };
        debug!(target: LOG_TARGET, "Sending GetBlockTiming response to client");
        Ok(Response::new(response))
    }

    async fn get_constants(
        &self,
        _request: Request<tari_rpc::Empty>,
    ) -> Result<Response<tari_rpc::ConsensusConstants>, Status> {
        debug!(target: LOG_TARGET, "Incoming GRPC request for GetConstants",);
        debug!(target: LOG_TARGET, "Sending GetConstants response to client");
        // TODO: Switch to request height
        Ok(Response::new(
            self.network.create_consensus_constants().pop().unwrap().into(),
        ))
    }

    async fn get_block_size(
        &self,
        request: Request<tari_rpc::BlockGroupRequest>,
    ) -> Result<Response<tari_rpc::BlockGroupResponse>, Status> {
        get_block_group(self.node_service.clone(), request, BlockGroupType::BlockSize).await
    }

    async fn get_block_fees(
        &self,
        request: Request<tari_rpc::BlockGroupRequest>,
    ) -> Result<Response<tari_rpc::BlockGroupResponse>, Status> {
        get_block_group(self.node_service.clone(), request, BlockGroupType::BlockFees).await
    }

    async fn get_version(&self, _request: Request<tari_rpc::Empty>) -> Result<Response<tari_rpc::StringValue>, Status> {
        Ok(Response::new(consts::APP_VERSION.to_string().into()))
    }

    async fn check_for_updates(
        &self,
        _request: Request<tari_rpc::Empty>,
    ) -> Result<Response<tari_rpc::SoftwareUpdate>, Status> {
        let mut resp = tari_rpc::SoftwareUpdate::default();

        if let Some(ref update) = *self.software_updater.new_update_notifier().borrow() {
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
        debug!(target: LOG_TARGET, "Incoming GRPC request for GetTokensInCirculation",);
        let request = request.into_inner();
        let mut heights = request.heights;
        heights = heights
            .drain(..cmp::min(heights.len(), GET_TOKENS_IN_CIRCULATION_MAX_HEIGHTS))
            .collect();
        let consensus_manager = ConsensusManager::builder(self.network.as_network()).build();

        let (mut tx, rx) = mpsc::channel(GET_TOKENS_IN_CIRCULATION_PAGE_SIZE);
        task::spawn(async move {
            let mut page: Vec<u64> = heights
                .drain(..cmp::min(heights.len(), GET_TOKENS_IN_CIRCULATION_PAGE_SIZE))
                .collect();
            while !page.is_empty() {
                // TODO: This is not ideal. The main issue here is the interface to get_tokens_in_circulation includes
                // blocks at any height to be selected instead of a more coherent start - end range. This means we
                // cannot use the Emission iterator as intended and instead, must query the supply at a
                // given height for each block (the docs mention to use the iterator instead of supply_at_block in a
                // loop, however the Iterator was not exposed at the time this handler was written).
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

    async fn get_sync_progress(
        &self,
        _request: Request<tari_rpc::Empty>,
    ) -> Result<Response<tari_rpc::SyncProgressResponse>, Status> {
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
            StateInfo::BlockSyncStarting => tari_rpc::SyncProgressResponse {
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
                state: match state.is_synced() {
                    true => tari_rpc::SyncState::Done.into(),
                    false => tari_rpc::SyncState::Startup.into(),
                },
            },
        };
        Ok(Response::new(response))
    }

    async fn get_sync_info(
        &self,
        _request: Request<tari_rpc::Empty>,
    ) -> Result<Response<tari_rpc::SyncInfoResponse>, Status> {
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

    async fn get_header_by_hash(
        &self,
        request: Request<tari_rpc::GetHeaderByHashRequest>,
    ) -> Result<Response<tari_rpc::BlockHeaderResponse>, Status> {
        let tari_rpc::GetHeaderByHashRequest { hash } = request.into_inner();
        let mut node_service = self.node_service.clone();
        let hash_hex = hash.to_hex();
        let block = node_service
            .get_block_by_hash(hash)
            .await
            .map_err(|err| Status::internal(err.to_string()))?;

        match block {
            Some(block) => {
                let (block, acc_data, confirmations, _) = block.dissolve();
                let total_block_reward = self
                    .consensus_rules
                    .calculate_coinbase_and_fees(block.header.height, block.body.kernels());

                let resp = tari_rpc::BlockHeaderResponse {
                    difficulty: acc_data.achieved_difficulty.into(),
                    num_transactions: block.body.kernels().len() as u32,
                    confirmations,
                    header: Some(block.header.into()),
                    reward: total_block_reward.into(),
                };

                Ok(Response::new(resp))
            },
            None => Err(Status::not_found(format!("Header not found with hash `{}`", hash_hex))),
        }
    }

    async fn identify(&self, _: Request<tari_rpc::Empty>) -> Result<Response<tari_rpc::NodeIdentity>, Status> {
        let identity = self.comms.node_identity_ref();
        Ok(Response::new(tari_rpc::NodeIdentity {
            public_key: identity.public_key().to_vec(),
            public_address: identity.public_address().to_string(),
            node_id: identity.node_id().to_vec(),
        }))
    }

    async fn get_network_status(
        &self,
        _: Request<tari_rpc::Empty>,
    ) -> Result<Response<tari_rpc::NetworkStatusResponse>, Status> {
        let status = self
            .comms
            .connectivity()
            .get_connectivity_status()
            .await
            .map_err(|err| Status::internal(err.to_string()))?;

        let latency = self
            .liveness
            .clone()
            .get_network_avg_latency()
            .await
            .map_err(|err| Status::internal(err.to_string()))?;

        let resp = tari_rpc::NetworkStatusResponse {
            status: tari_rpc::ConnectivityStatus::from(status) as i32,
            avg_latency_ms: latency
                .map(|l| u32::try_from(l.as_millis()).unwrap_or(u32::MAX))
                .unwrap_or(0),
            num_node_connections: status.num_connected_nodes() as u32,
        };

        Ok(Response::new(resp))
    }

    async fn list_connected_peers(
        &self,
        _: Request<tari_rpc::Empty>,
    ) -> Result<Response<tari_rpc::ListConnectedPeersResponse>, Status> {
        let mut connectivity = self.comms.connectivity();
        let peer_manager = self.comms.peer_manager();
        let connected_peers = connectivity
            .get_active_connections()
            .await
            .map_err(|err| Status::internal(err.to_string()))?;

        let mut peers = Vec::with_capacity(connected_peers.len());
        for peer in connected_peers {
            peers.push(
                peer_manager
                    .find_by_node_id(peer.peer_node_id())
                    .await
                    .map_err(|err| Status::internal(err.to_string()))?
                    .ok_or_else(|| Status::not_found(format!("Peer {} not found", peer.peer_node_id())))?,
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
        let mut mempool_handle = self.mempool_service.clone();

        let mempool_stats = mempool_handle.get_mempool_stats().await.map_err(|e| {
            error!(target: LOG_TARGET, "Error submitting query:{}", e);
            Status::internal(e.to_string())
        })?;

        let response = tari_rpc::MempoolStatsResponse {
            total_txs: mempool_stats.total_txs as u64,
            unconfirmed_txs: mempool_stats.unconfirmed_txs as u64,
            reorg_txs: mempool_stats.reorg_txs as u64,
            total_weight: mempool_stats.total_weight,
        };

        Ok(Response::new(response))
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

    let (start, end) = get_heights(&height_request, handler.clone()).await?;

    let blocks = match handler.get_blocks(start..=end).await {
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
