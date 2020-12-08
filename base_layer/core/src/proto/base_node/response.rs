//  Copyright 2020, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

pub use super::base_node_service_response::Response as ProtoNodeCommsResponse;
use crate::{
    proof_of_work::Difficulty,
    proto,
    tari_utilities::convert::try_convert_all,
    types::base_node::NodeCommsResponse,
};
use std::{
    convert::TryInto,
    iter::{FromIterator, Iterator},
};

impl TryInto<NodeCommsResponse> for ProtoNodeCommsResponse {
    type Error = String;

    fn try_into(self) -> Result<NodeCommsResponse, Self::Error> {
        use ProtoNodeCommsResponse::*;
        let response = match self {
            ChainMetadata(chain_metadata) => NodeCommsResponse::ChainMetadata(chain_metadata.try_into()?),
            TransactionKernels(kernels) => {
                let kernels = try_convert_all(kernels.kernels)?;
                NodeCommsResponse::TransactionKernels(kernels)
            },
            BlockHeaders(headers) => {
                let headers = try_convert_all(headers.headers)?;
                NodeCommsResponse::BlockHeaders(headers)
            },
            FetchHeadersAfterResponse(headers) => {
                let headers = try_convert_all(headers.headers)?;
                NodeCommsResponse::FetchHeadersAfterResponse(headers)
            },
            TransactionOutputs(outputs) => {
                let outputs = try_convert_all(outputs.outputs)?;
                NodeCommsResponse::TransactionOutputs(outputs)
            },
            HistoricalBlocks(blocks) => {
                let blocks = try_convert_all(blocks.blocks)?;
                NodeCommsResponse::HistoricalBlocks(blocks)
            },
            NewBlockTemplate(block_template) => NodeCommsResponse::NewBlockTemplate(block_template.try_into()?),
            NewBlock(block) => NodeCommsResponse::NewBlock {
                success: block.success,
                error: Some(block.error),
                block: match block.block {
                    Some(b) => Some(b.try_into()?),
                    None => None,
                },
            },
            TargetDifficulty(difficulty) => NodeCommsResponse::TargetDifficulty(Difficulty::from(difficulty)),
            MmrNodeCount(u64) => NodeCommsResponse::MmrNodeCount(u64),
            MmrNodes(response) => NodeCommsResponse::MmrNodes(response.added, response.deleted),
        };

        Ok(response)
    }
}

impl From<NodeCommsResponse> for ProtoNodeCommsResponse {
    fn from(response: NodeCommsResponse) -> Self {
        use NodeCommsResponse::*;
        match response {
            ChainMetadata(chain_metadata) => ProtoNodeCommsResponse::ChainMetadata(chain_metadata.into()),
            TransactionKernels(kernels) => {
                let kernels = kernels.into_iter().map(Into::into).collect();
                ProtoNodeCommsResponse::TransactionKernels(kernels)
            },
            BlockHeaders(headers) => {
                let block_headers = headers.into_iter().map(Into::into).collect();
                ProtoNodeCommsResponse::BlockHeaders(block_headers)
            },
            FetchHeadersAfterResponse(headers) => {
                let block_headers = headers.into_iter().map(Into::into).collect();
                ProtoNodeCommsResponse::FetchHeadersAfterResponse(block_headers)
            },
            TransactionOutputs(outputs) => {
                let outputs = outputs.into_iter().map(Into::into).collect();
                ProtoNodeCommsResponse::TransactionOutputs(outputs)
            },
            HistoricalBlocks(historical_blocks) => {
                let historical_blocks = historical_blocks.into_iter().map(Into::into).collect();
                ProtoNodeCommsResponse::HistoricalBlocks(historical_blocks)
            },
            NewBlockTemplate(block_template) => ProtoNodeCommsResponse::NewBlockTemplate(block_template.into()),
            NewBlock { success, error, block } => {
                ProtoNodeCommsResponse::NewBlock(proto::base_node::NewBlockResponse {
                    success,
                    error: error.unwrap_or_else(|| "".to_string()),
                    block: block.map(|b| b.into()),
                })
            },
            TargetDifficulty(difficulty) => ProtoNodeCommsResponse::TargetDifficulty(difficulty.as_u64()),
            MmrNodeCount(node_count) => ProtoNodeCommsResponse::MmrNodeCount(node_count),
            MmrNodes(added, deleted) => ProtoNodeCommsResponse::MmrNodes(proto::base_node::MmrNodes { added, deleted }),
        }
    }
}

//---------------------------------- Collection impls --------------------------------------------//

// The following allow `Iterator::collect` to collect into these repeated types

impl FromIterator<proto::types::TransactionKernel> for proto::base_node::TransactionKernels {
    fn from_iter<T: IntoIterator<Item = proto::types::TransactionKernel>>(iter: T) -> Self {
        Self {
            kernels: iter.into_iter().collect(),
        }
    }
}

impl FromIterator<proto::core::BlockHeader> for proto::base_node::BlockHeaders {
    fn from_iter<T: IntoIterator<Item = proto::core::BlockHeader>>(iter: T) -> Self {
        Self {
            headers: iter.into_iter().collect(),
        }
    }
}

impl FromIterator<proto::types::TransactionOutput> for proto::base_node::TransactionOutputs {
    fn from_iter<T: IntoIterator<Item = proto::types::TransactionOutput>>(iter: T) -> Self {
        Self {
            outputs: iter.into_iter().collect(),
        }
    }
}

impl FromIterator<proto::core::HistoricalBlock> for proto::base_node::HistoricalBlocks {
    fn from_iter<T: IntoIterator<Item = proto::core::HistoricalBlock>>(iter: T) -> Self {
        Self {
            blocks: iter.into_iter().collect(),
        }
    }
}
