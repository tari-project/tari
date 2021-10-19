// Copyright 2019, The Tari Project
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

pub use crate::proto::base_node::base_node_service_response::Response as ProtoNodeCommsResponse;
use crate::{
    base_node::comms_interface as ci,
    blocks::{BlockHeader, HistoricalBlock},
    proof_of_work::Difficulty,
    proto,
    proto::{
        base_node as base_node_proto,
        base_node::{
            BlockHeaders as ProtoBlockHeaders,
            HistoricalBlocks as ProtoHistoricalBlocks,
            MmrNodes as ProtoMmrNodes,
            NewBlockResponse as ProtoNewBlockResponse,
            TransactionKernels as ProtoTransactionKernels,
            TransactionOutputs as ProtoTransactionOutputs,
        },
        core as core_proto_types,
    },
    tari_utilities::convert::try_convert_all,
};
use std::{
    convert::TryInto,
    iter::{FromIterator, Iterator},
};

impl TryInto<ci::NodeCommsResponse> for ProtoNodeCommsResponse {
    type Error = String;

    fn try_into(self) -> Result<ci::NodeCommsResponse, Self::Error> {
        use ProtoNodeCommsResponse::*;
        let response = match self {
            ChainMetadata(chain_metadata) => ci::NodeCommsResponse::ChainMetadata(chain_metadata.try_into()?),
            TransactionKernels(kernels) => {
                let kernels = try_convert_all(kernels.kernels)?;
                ci::NodeCommsResponse::TransactionKernels(kernels)
            },
            BlockHeaders(headers) => {
                let headers = try_convert_all(headers.headers)?;
                ci::NodeCommsResponse::BlockHeaders(headers)
            },
            BlockHeader(header) => ci::NodeCommsResponse::BlockHeader(header.try_into()?),
            HistoricalBlock(block) => ci::NodeCommsResponse::HistoricalBlock(Box::new(block.try_into()?)),
            FetchHeadersAfterResponse(headers) => {
                let headers = try_convert_all(headers.headers)?;
                ci::NodeCommsResponse::FetchHeadersAfterResponse(headers)
            },
            TransactionOutputs(outputs) => {
                let outputs = try_convert_all(outputs.outputs)?;
                ci::NodeCommsResponse::TransactionOutputs(outputs)
            },
            HistoricalBlocks(blocks) => {
                let blocks = try_convert_all(blocks.blocks)?;
                ci::NodeCommsResponse::HistoricalBlocks(blocks)
            },
            NewBlockTemplate(block_template) => ci::NodeCommsResponse::NewBlockTemplate(block_template.try_into()?),
            NewBlock(block) => ci::NodeCommsResponse::NewBlock {
                success: block.success,
                error: Some(block.error),
                block: match block.block {
                    Some(b) => Some(b.try_into()?),
                    None => None,
                },
            },
            TargetDifficulty(difficulty) => ci::NodeCommsResponse::TargetDifficulty(Difficulty::from(difficulty)),
            MmrNodes(response) => ci::NodeCommsResponse::MmrNodes(response.added, response.deleted),
        };

        Ok(response)
    }
}

impl From<ci::NodeCommsResponse> for ProtoNodeCommsResponse {
    fn from(response: ci::NodeCommsResponse) -> Self {
        use ci::NodeCommsResponse::*;
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
            BlockHeader(header) => ProtoNodeCommsResponse::BlockHeader(header.into()),
            HistoricalBlock(block) => ProtoNodeCommsResponse::HistoricalBlock((*block).into()),
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
            NewBlock { success, error, block } => ProtoNodeCommsResponse::NewBlock(ProtoNewBlockResponse {
                success,
                error: error.unwrap_or_else(|| "".to_string()),
                block: block.map(|b| b.into()),
            }),
            TargetDifficulty(difficulty) => ProtoNodeCommsResponse::TargetDifficulty(difficulty.as_u64()),
            MmrNodes(added, deleted) => ProtoNodeCommsResponse::MmrNodes(ProtoMmrNodes { added, deleted }),
        }
    }
}

impl From<Option<BlockHeader>> for base_node_proto::BlockHeaderResponse {
    fn from(v: Option<BlockHeader>) -> Self {
        Self {
            header: v.map(Into::into),
        }
    }
}

impl TryInto<Option<BlockHeader>> for base_node_proto::BlockHeaderResponse {
    type Error = String;

    fn try_into(self) -> Result<Option<BlockHeader>, Self::Error> {
        match self.header {
            Some(header) => {
                let header = header.try_into()?;
                Ok(Some(header))
            },
            None => Ok(None),
        }
    }
}

impl From<Option<HistoricalBlock>> for base_node_proto::HistoricalBlockResponse {
    fn from(v: Option<HistoricalBlock>) -> Self {
        Self {
            block: v.map(Into::into),
        }
    }
}

impl TryInto<Option<HistoricalBlock>> for base_node_proto::HistoricalBlockResponse {
    type Error = String;

    fn try_into(self) -> Result<Option<HistoricalBlock>, Self::Error> {
        match self.block {
            Some(block) => {
                let block = block.try_into()?;
                Ok(Some(block))
            },
            None => Ok(None),
        }
    }
}

//---------------------------------- Collection impls --------------------------------------------//

// The following allow `Iterator::collect` to collect into these repeated types

impl FromIterator<proto::types::TransactionKernel> for ProtoTransactionKernels {
    fn from_iter<T: IntoIterator<Item = proto::types::TransactionKernel>>(iter: T) -> Self {
        Self {
            kernels: iter.into_iter().collect(),
        }
    }
}

impl FromIterator<proto::core::BlockHeader> for ProtoBlockHeaders {
    fn from_iter<T: IntoIterator<Item = core_proto_types::BlockHeader>>(iter: T) -> Self {
        Self {
            headers: iter.into_iter().collect(),
        }
    }
}

impl FromIterator<proto::types::TransactionOutput> for ProtoTransactionOutputs {
    fn from_iter<T: IntoIterator<Item = proto::types::TransactionOutput>>(iter: T) -> Self {
        Self {
            outputs: iter.into_iter().collect(),
        }
    }
}

impl FromIterator<proto::core::HistoricalBlock> for ProtoHistoricalBlocks {
    fn from_iter<T: IntoIterator<Item = core_proto_types::HistoricalBlock>>(iter: T) -> Self {
        Self {
            blocks: iter.into_iter().collect(),
        }
    }
}
