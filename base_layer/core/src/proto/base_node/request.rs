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

use crate::{
    proof_of_work::PowAlgorithm,
    proto,
    proto::base_node::{base_node_service_request::Request as BaseNodeRequest, BlockHeights, HashOutputs},
    transactions::types::{Commitment, HashOutput, Signature},
    types::base_node::NodeCommsRequest,
};
use std::convert::{From, TryFrom, TryInto};
use tari_crypto::tari_utilities::ByteArrayError;

//---------------------------------- BaseNodeRequest --------------------------------------------//
impl TryInto<NodeCommsRequest> for BaseNodeRequest {
    type Error = String;

    fn try_into(self) -> Result<NodeCommsRequest, Self::Error> {
        use BaseNodeRequest::*;
        let request = match self {
            // Field was not specified
            GetChainMetadata(_) => NodeCommsRequest::GetChainMetadata,
            FetchKernels(hash_outputs) => NodeCommsRequest::FetchKernels(hash_outputs.outputs),
            FetchHeaders(block_heights) => NodeCommsRequest::FetchHeaders(block_heights.heights),
            FetchHeadersWithHashes(block_hashes) => NodeCommsRequest::FetchHeadersWithHashes(block_hashes.outputs),
            FetchHeadersAfter(request) => NodeCommsRequest::FetchHeadersAfter(request.hashes, request.stopping_hash),
            FetchMatchingUtxos(hash_outputs) => NodeCommsRequest::FetchMatchingUtxos(hash_outputs.outputs),
            FetchMatchingTxos(hash_outputs) => NodeCommsRequest::FetchMatchingTxos(hash_outputs.outputs),
            FetchMatchingBlocks(block_heights) => NodeCommsRequest::FetchMatchingBlocks(block_heights.heights),
            FetchBlocksWithHashes(block_hashes) => NodeCommsRequest::FetchBlocksWithHashes(block_hashes.outputs),
            FetchBlocksWithKernels(signatures) => {
                let mut sigs = Vec::new();
                for sig in signatures.sigs {
                    sigs.push(Signature::try_from(sig).map_err(|err: ByteArrayError| err.to_string())?)
                }
                NodeCommsRequest::FetchBlocksWithKernels(sigs)
            },
            FetchBlocksWithStxos(commitments) => {
                let mut commits = Vec::new();
                for stxo in commitments.commitments {
                    commits.push(Commitment::try_from(stxo).map_err(|err: ByteArrayError| err.to_string())?)
                }
                NodeCommsRequest::FetchBlocksWithStxos(commits)
            },
            FetchBlocksWithUtxos(commitments) => {
                let mut commits = Vec::new();
                for stxo in commitments.commitments {
                    commits.push(Commitment::try_from(stxo).map_err(|err: ByteArrayError| err.to_string())?)
                }
                NodeCommsRequest::FetchBlocksWithUtxos(commits)
            },
            GetNewBlockTemplate(pow_algo) => NodeCommsRequest::GetNewBlockTemplate(PowAlgorithm::try_from(pow_algo)?),
            GetNewBlock(block_template) => NodeCommsRequest::GetNewBlock(block_template.try_into()?),
            FetchMmrNodeCount(request) => NodeCommsRequest::FetchMmrNodeCount(request.tree.try_into()?, request.height),
            FetchMatchingMmrNodes(request) => NodeCommsRequest::FetchMatchingMmrNodes(
                request.tree.try_into()?,
                request.pos,
                request.count,
                request.hist_height,
            ),
        };
        Ok(request)
    }
}

impl From<NodeCommsRequest> for BaseNodeRequest {
    fn from(request: NodeCommsRequest) -> Self {
        use NodeCommsRequest::*;
        match request {
            GetChainMetadata => BaseNodeRequest::GetChainMetadata(true),
            FetchKernels(hash_outputs) => BaseNodeRequest::FetchKernels(hash_outputs.into()),
            FetchHeaders(block_heights) => BaseNodeRequest::FetchHeaders(block_heights.into()),
            FetchHeadersWithHashes(block_hashes) => BaseNodeRequest::FetchHeadersWithHashes(block_hashes.into()),
            FetchHeadersAfter(hashes, stopping_hash) => {
                BaseNodeRequest::FetchHeadersAfter(proto::base_node::FetchHeadersAfter { hashes, stopping_hash })
            },
            FetchMatchingUtxos(hash_outputs) => BaseNodeRequest::FetchMatchingUtxos(hash_outputs.into()),
            FetchMatchingTxos(hash_outputs) => BaseNodeRequest::FetchMatchingTxos(hash_outputs.into()),
            FetchMatchingBlocks(block_heights) => BaseNodeRequest::FetchMatchingBlocks(block_heights.into()),
            FetchBlocksWithHashes(block_hashes) => BaseNodeRequest::FetchBlocksWithHashes(block_hashes.into()),
            FetchBlocksWithKernels(signatures) => {
                let sigs = signatures.into_iter().map(Into::into).collect();
                BaseNodeRequest::FetchBlocksWithKernels(super::Signatures { sigs })
            },
            FetchBlocksWithStxos(commitments) => {
                let commits = commitments.into_iter().map(Into::into).collect();
                BaseNodeRequest::FetchBlocksWithStxos(super::Commitments { commitments: commits })
            },
            FetchBlocksWithUtxos(commitments) => {
                let commits = commitments.into_iter().map(Into::into).collect();
                BaseNodeRequest::FetchBlocksWithUtxos(super::Commitments { commitments: commits })
            },
            GetNewBlockTemplate(pow_algo) => BaseNodeRequest::GetNewBlockTemplate(pow_algo as u64),
            GetNewBlock(block_template) => BaseNodeRequest::GetNewBlock(block_template.into()),
            FetchMmrNodeCount(tree, height) => {
                BaseNodeRequest::FetchMmrNodeCount(proto::base_node::FetchMmrNodeCount {
                    tree: tree as i32,
                    height,
                })
            },
            FetchMatchingMmrNodes(tree, pos, count, hist_height) => {
                BaseNodeRequest::FetchMatchingMmrNodes(proto::base_node::FetchMatchingMmrNodes {
                    tree: tree as i32,
                    pos,
                    count,
                    hist_height,
                })
            },
        }
    }
}

//---------------------------------- Wrappers --------------------------------------------//

impl From<Vec<HashOutput>> for HashOutputs {
    fn from(outputs: Vec<HashOutput>) -> Self {
        Self { outputs }
    }
}

impl From<Vec<u64>> for BlockHeights {
    fn from(heights: Vec<u64>) -> Self {
        Self { heights }
    }
}
