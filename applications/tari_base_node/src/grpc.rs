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
use tonic::{transport::Server, Request, Response, Status, Code};
use crate::builder::NodeContainer;
use base_node_grpc::*;
use log::*;
use prost_types::Timestamp;
use tari_core::{
    base_node::LocalNodeCommsInterface,
    blocks::BlockHeader,
    mempool::service::LocalMempoolService,
    proof_of_work::PowAlgorithm,
};
use tari_crypto::tari_utilities::{epoch_time::EpochTime, ByteArray, Hashable};
use tokio::runtime;
use tari_core::base_node::comms_interface::CommsInterfaceError;

const LOG_TARGET: &str = "base_node::grpc";

pub mod base_node_grpc {
    tonic::include_proto!("tari.base_node");
}

pub struct BaseNodeGrpcServer {
    executor: runtime::Handle,
    node_service: LocalNodeCommsInterface,
    mempool_service: LocalMempoolService,
}

impl BaseNodeGrpcServer {
    pub fn new(executor: runtime::Handle, ctx: &NodeContainer) -> Self {
        Self {
            executor,
            node_service: ctx.local_node(),
            mempool_service: ctx.local_mempool(),
        }
    }
}

#[tonic::async_trait]
impl base_node_grpc::base_node_server::BaseNode for BaseNodeGrpcServer {
    async fn list_headers(
        &self,
        request: Request<ListHeadersRequest>,
    ) -> Result<Response<ListHeadersResponse>, Status>
    {
        trace!(target: LOG_TARGET, "List headers called:{:?}", request);
        let request = request.into_inner();
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
            0 => 10,
            _ => request.num_headers,
        };

        let headers: Vec<u64> = if request.from_height != 0 {
            match sorting {
                Sorting::Desc => ((request.from_height - num_headers + 1)..=request.from_height).collect(),
                Sorting::Asc => (request.from_height..(request.from_height + num_headers)).collect(),
            }
        } else {
            match sorting {
                Sorting::Desc => ((tip - num_headers)..tip).collect(),
                Sorting::Asc => (0..num_headers).collect(),
            }
        };

        let mut headers = match handler.get_headers(headers).await {
            Err(err) => {
                warn!(target: LOG_TARGET, "Error communicating with base node: {}", err,);
                return Err(Status::internal(err.to_string()));
            },
            Ok(data) => data,
        };
        headers.sort_by(|a, b| match sorting {
            Sorting::Desc => b.height.cmp(&a.height),
            Sorting::Asc => a.height.cmp(&b.height)
        });
        let reply = ListHeadersResponse {
            headers: headers.iter().map(|h| convert_block_header(h)).collect(),
        };

        debug!(target: LOG_TARGET, "Responding:{:?}", reply);

        Ok(Response::new(reply))
    }

    async fn get_blocks(&self, request: Request<GetBlocksRequest>) -> Result<Response<GetBlocksResponse>, Status> {
        let request = request.into_inner();
        let mut handler = self.node_service.clone();
        let blocks = match handler.get_blocks(request.heights).await {
            Err(err) => {
                warn!(
                    target: LOG_TARGET,
                    "Error communicating with local base node: {:?}", err,
                );
                return Err(Status::internal(err.to_string()));
            },
            Ok(data) => data,
        };
        let reply = GetBlocksResponse {
            blocks: blocks
                .into_iter()
                .map(|b| base_node_grpc::HistoricalBlock {
                    confirmations: b.confirmations,
                    spent_commitments: b.spent_commitments.iter().map(|c| Vec::from(c.as_bytes())).collect(),
                    block: Some(convert_block(&b.block)),
                })
                .collect(),
        };
        Ok(Response::new(reply))
    }
}

/// Utility function that converts a `chrono::DateTime` to a `prost::Timestamp`
fn datetime_to_timestamp(datetime: EpochTime) -> Timestamp {
    Timestamp {
        seconds: datetime.as_u64() as i64,
        nanos: 0,
    }
}

fn convert_block(block: &tari_core::blocks::Block) -> base_node_grpc::Block {
    base_node_grpc::Block {
        header: Some(convert_block_header(&block.header)),
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
    }
}

fn convert_block_header(h: &BlockHeader) -> base_node_grpc::BlockHeader {
    base_node_grpc::BlockHeader {
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
            pow_data: h.pow.pow_data.clone(),
        }),
    }
}

