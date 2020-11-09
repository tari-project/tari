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

use super::base_node::{
    base_node_service_request::Request as ProtoNodeCommsRequest,
    BlockHeights,
    FetchHeadersAfter as ProtoFetchHeadersAfter,
    FetchMatchingMmrNodes as ProtoFetchMmrNodes,
    FetchMmrNodeCount as ProtoFetchMmrNodeCount,
    HashOutputs,
};
use crate::{
    base_node::comms_interface as ci,
    proof_of_work::PowAlgorithm,
    transactions::types::{Commitment, HashOutput, Signature},
};
use std::convert::{From, TryFrom, TryInto};
use tari_crypto::tari_utilities::ByteArrayError;

//---------------------------------- BaseNodeRequest --------------------------------------------//
impl TryInto<ci::NodeCommsRequest> for ProtoNodeCommsRequest {
    type Error = String;

    fn try_into(self) -> Result<ci::NodeCommsRequest, Self::Error> {
        use ProtoNodeCommsRequest::*;
        let request = match self {
            // Field was not specified
            GetChainMetadata(_) => ci::NodeCommsRequest::GetChainMetadata,
            FetchKernels(hash_outputs) => ci::NodeCommsRequest::FetchKernels(hash_outputs.outputs),
            FetchHeaders(block_heights) => ci::NodeCommsRequest::FetchHeaders(block_heights.heights),
            FetchHeadersWithHashes(block_hashes) => ci::NodeCommsRequest::FetchHeadersWithHashes(block_hashes.outputs),
            FetchHeadersAfter(request) => {
                ci::NodeCommsRequest::FetchHeadersAfter(request.hashes, request.stopping_hash)
            },
            FetchMatchingUtxos(hash_outputs) => ci::NodeCommsRequest::FetchMatchingUtxos(hash_outputs.outputs),
            FetchMatchingTxos(hash_outputs) => ci::NodeCommsRequest::FetchMatchingTxos(hash_outputs.outputs),
            FetchMatchingBlocks(block_heights) => ci::NodeCommsRequest::FetchMatchingBlocks(block_heights.heights),
            FetchBlocksWithHashes(block_hashes) => ci::NodeCommsRequest::FetchBlocksWithHashes(block_hashes.outputs),
            FetchBlocksWithKernels(signatures) => {
                let mut sigs = Vec::new();
                for sig in signatures.sigs {
                    sigs.push(Signature::try_from(sig).map_err(|err: ByteArrayError| err.to_string())?)
                }
                ci::NodeCommsRequest::FetchBlocksWithKernels(sigs)
            },
            FetchBlocksWithStxos(commitments) => {
                let mut commits = Vec::new();
                for stxo in commitments.commitments {
                    commits.push(Commitment::try_from(stxo).map_err(|err: ByteArrayError| err.to_string())?)
                }
                ci::NodeCommsRequest::FetchBlocksWithStxos(commits)
            },
            FetchBlocksWithUtxos(commitments) => {
                let mut commits = Vec::new();
                for stxo in commitments.commitments {
                    commits.push(Commitment::try_from(stxo).map_err(|err: ByteArrayError| err.to_string())?)
                }
                ci::NodeCommsRequest::FetchBlocksWithUtxos(commits)
            },
            GetNewBlockTemplate(pow_algo) => {
                ci::NodeCommsRequest::GetNewBlockTemplate(PowAlgorithm::try_from(pow_algo)?)
            },
            GetNewBlock(block_template) => ci::NodeCommsRequest::GetNewBlock(block_template.try_into()?),
            FetchMmrNodeCount(request) => {
                ci::NodeCommsRequest::FetchMmrNodeCount(request.tree.try_into()?, request.height)
            },
            FetchMatchingMmrNodes(request) => ci::NodeCommsRequest::FetchMatchingMmrNodes(
                request.tree.try_into()?,
                request.pos,
                request.count,
                request.hist_height,
            ),
        };
        Ok(request)
    }
}

impl From<ci::NodeCommsRequest> for ProtoNodeCommsRequest {
    fn from(request: ci::NodeCommsRequest) -> Self {
        use ci::NodeCommsRequest::*;
        match request {
            GetChainMetadata => ProtoNodeCommsRequest::GetChainMetadata(true),
            FetchKernels(hash_outputs) => ProtoNodeCommsRequest::FetchKernels(hash_outputs.into()),
            FetchHeaders(block_heights) => ProtoNodeCommsRequest::FetchHeaders(block_heights.into()),
            FetchHeadersWithHashes(block_hashes) => ProtoNodeCommsRequest::FetchHeadersWithHashes(block_hashes.into()),
            FetchHeadersAfter(hashes, stopping_hash) => {
                ProtoNodeCommsRequest::FetchHeadersAfter(ProtoFetchHeadersAfter { hashes, stopping_hash })
            },
            FetchMatchingUtxos(hash_outputs) => ProtoNodeCommsRequest::FetchMatchingUtxos(hash_outputs.into()),
            FetchMatchingTxos(hash_outputs) => ProtoNodeCommsRequest::FetchMatchingTxos(hash_outputs.into()),
            FetchMatchingBlocks(block_heights) => ProtoNodeCommsRequest::FetchMatchingBlocks(block_heights.into()),
            FetchBlocksWithHashes(block_hashes) => ProtoNodeCommsRequest::FetchBlocksWithHashes(block_hashes.into()),
            FetchBlocksWithKernels(signatures) => {
                let sigs = signatures.into_iter().map(Into::into).collect();
                ProtoNodeCommsRequest::FetchBlocksWithKernels(super::base_node::Signatures { sigs })
            },
            FetchBlocksWithStxos(commitments) => {
                let commits = commitments.into_iter().map(Into::into).collect();
                ProtoNodeCommsRequest::FetchBlocksWithStxos(super::base_node::Commitments { commitments: commits })
            },
            FetchBlocksWithUtxos(commitments) => {
                let commits = commitments.into_iter().map(Into::into).collect();
                ProtoNodeCommsRequest::FetchBlocksWithUtxos(super::base_node::Commitments { commitments: commits })
            },
            GetNewBlockTemplate(pow_algo) => ProtoNodeCommsRequest::GetNewBlockTemplate(pow_algo as u64),
            GetNewBlock(block_template) => ProtoNodeCommsRequest::GetNewBlock(block_template.into()),
            FetchMmrNodeCount(tree, height) => ProtoNodeCommsRequest::FetchMmrNodeCount(ProtoFetchMmrNodeCount {
                tree: tree as i32,
                height,
            }),
            FetchMatchingMmrNodes(tree, pos, count, hist_height) => {
                ProtoNodeCommsRequest::FetchMatchingMmrNodes(ProtoFetchMmrNodes {
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
