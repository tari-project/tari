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
use base_node_grpc::*;
use log::*;
use prost_types::Timestamp;
use std::cmp;
use tari_core::{
    base_node::LocalNodeCommsInterface,
    blocks::{Block, BlockHeader},
    chain_storage::HistoricalBlock,
    proof_of_work::PowAlgorithm,
};
use tari_crypto::tari_utilities::{epoch_time::EpochTime, ByteArray, Hashable};
use tokio::{runtime, sync::mpsc};
use tonic::{Request, Response, Status};

const LOG_TARGET: &str = "base_node::grpc";
// The maximum number of headers a client can request at a time. If the client requests more than
// this, this is the maximum that will be returned.
const LIST_HEADERS_MAX_NUM_HEADERS: u64 = 10_000;
// The number of headers to request via the local interface at a time. These are then streamed to
// client.
const LIST_HEADERS_PAGE_SIZE: usize = 10;
// The `num_headers` value if none is provided.
const LIST_HEADERS_DEFAULT_NUM_HEADERS: u64 = 10;

// The maximum number of blocks that can be requested at a time. These will be streamed to the
// client, so memory is not really a concern here, but a malicious client could request a large
// number here to keep the node busy
const GET_BLOCKS_MAX_HEIGHTS: usize = 1000;

// The number of blocks to request from the base node at a time. This is to reduce the number of
// requests to the base node, but if you'd like to stream directly, this can be set to 1.
const GET_BLOCKS_PAGE_SIZE: usize = 10;

pub(crate) mod base_node_grpc {
    tonic::include_proto!("tari.base_node");
}

pub struct BaseNodeGrpcServer {
    executor: runtime::Handle,
    node_service: LocalNodeCommsInterface,
}

impl BaseNodeGrpcServer {
    pub fn new(executor: runtime::Handle, local_node: LocalNodeCommsInterface) -> Self {
        Self {
            executor,
            node_service: local_node,
        }
    }
}

#[tonic::async_trait]
impl base_node_grpc::base_node_server::BaseNode for BaseNodeGrpcServer {
    type GetBlocksStream = mpsc::Receiver<Result<base_node_grpc::HistoricalBlock, Status>>;
    type ListHeadersStream = mpsc::Receiver<Result<base_node_grpc::BlockHeader, Status>>;

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

    async fn get_calc_timing(
        &self,
        request: Request<GetCalcTimingRequest>,
    ) -> Result<Response<CalcTimingResponse>, Status>
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
        let heights = if request.start_height > 0 && request.end_height > 0 {
            BlockHeader::get_height_range(request.start_height, request.end_height)
        } else if request.from_tip > 0 {
            match BlockHeader::get_heights_from_tip(handler.clone(), request.from_tip).await {
                Ok(heights) => heights,
                Err(err) => {
                    warn!(
                        target: LOG_TARGET,
                        "Error getting heights from tip for GRPC client: {}", err
                    );
                    Vec::new()
                },
            }
        } else {
            return Err(Status::invalid_argument("Invalid arguments provided"));
        };

        let headers = match handler.get_headers(heights).await {
            Ok(headers) => headers,
            Err(err) => {
                warn!(target: LOG_TARGET, "Error getting headers for GRPC client: {}", err);
                Vec::new()
            },
        };
        let (max, min, avg) = BlockHeader::timing_stats(&headers);

        let response: base_node_grpc::CalcTimingResponse = base_node_grpc::CalcTimingResponse { max, min, avg };
        debug!(target: LOG_TARGET, "Sending GetCalcTiming response stream to client");
        Ok(Response::new(response))
    }
}

/// Utility function that converts a `chrono::DateTime` to a `prost::Timestamp`
fn datetime_to_timestamp(datetime: EpochTime) -> Timestamp {
    Timestamp {
        seconds: datetime.as_u64() as i64,
        nanos: 0,
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
