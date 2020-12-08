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
    blocks::NewBlockTemplate,
    proof_of_work::PowAlgorithm,
    transactions::types::{Commitment, HashOutput, Signature},
    types::MmrTree,
};
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Error, Formatter};

/// A container for the parameters required for a FetchMmrState request.
#[derive(Debug, Serialize, Deserialize)]
pub struct MmrStateRequest {
    pub tree: MmrTree,
    pub index: u64,
    pub count: u64,
}

/// API Request enum
#[derive(Debug, Serialize, Deserialize)]
pub enum NodeCommsRequest {
    GetChainMetadata,
    FetchKernels(Vec<HashOutput>),
    FetchHeaders(Vec<u64>),
    FetchHeadersWithHashes(Vec<HashOutput>),
    FetchHeadersAfter(Vec<HashOutput>, HashOutput),
    FetchMatchingUtxos(Vec<HashOutput>),
    FetchMatchingTxos(Vec<HashOutput>),
    FetchMatchingBlocks(Vec<u64>),
    FetchBlocksWithHashes(Vec<HashOutput>),
    FetchBlocksWithKernels(Vec<Signature>),
    FetchBlocksWithStxos(Vec<Commitment>),
    FetchBlocksWithUtxos(Vec<Commitment>),
    GetNewBlockTemplate(PowAlgorithm),
    GetNewBlock(NewBlockTemplate),
    FetchMmrNodeCount(MmrTree, u64),
    FetchMatchingMmrNodes(MmrTree, u32, u32, u64),
}

impl Display for NodeCommsRequest {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            NodeCommsRequest::GetChainMetadata => f.write_str("GetChainMetadata"),
            NodeCommsRequest::FetchKernels(v) => f.write_str(&format!("FetchKernels (n={})", v.len())),
            NodeCommsRequest::FetchHeaders(v) => f.write_str(&format!("FetchHeaders (n={})", v.len())),
            NodeCommsRequest::FetchHeadersWithHashes(v) => {
                f.write_str(&format!("FetchHeadersWithHashes (n={})", v.len()))
            },
            NodeCommsRequest::FetchHeadersAfter(v, _hash) => f.write_str(&format!("FetchHeadersAfter (n={})", v.len())),
            NodeCommsRequest::FetchMatchingUtxos(v) => f.write_str(&format!("FetchMatchingUtxos (n={})", v.len())),
            NodeCommsRequest::FetchMatchingTxos(v) => f.write_str(&format!("FetchMatchingTxos (n={})", v.len())),
            NodeCommsRequest::FetchMatchingBlocks(v) => f.write_str(&format!("FetchMatchingBlocks (n={})", v.len())),
            NodeCommsRequest::FetchBlocksWithHashes(v) => {
                f.write_str(&format!("FetchBlocksWithHashes (n={})", v.len()))
            },
            NodeCommsRequest::FetchBlocksWithKernels(v) => {
                f.write_str(&format!("FetchBlocksWithKernels (n={})", v.len()))
            },
            NodeCommsRequest::FetchBlocksWithStxos(v) => f.write_str(&format!("FetchBlocksWithStxos (n={})", v.len())),
            NodeCommsRequest::FetchBlocksWithUtxos(v) => f.write_str(&format!("FetchBlocksWithUtxos (n={})", v.len())),
            NodeCommsRequest::GetNewBlockTemplate(algo) => f.write_str(&format!("GetNewBlockTemplate ({})", algo)),
            NodeCommsRequest::GetNewBlock(b) => f.write_str(&format!("GetNewBlock (Block Height={})", b.header.height)),
            NodeCommsRequest::FetchMmrNodeCount(tree, height) => {
                f.write_str(&format!("FetchMmrNodeCount (tree={},Block Height={})", tree, height))
            },
            NodeCommsRequest::FetchMatchingMmrNodes(tree, pos, count, hist_height) => f.write_str(&format!(
                "FetchMatchingMmrNodes (tree={},pos={},count={},hist_height={})",
                tree, pos, count, hist_height
            )),
        }
    }
}
