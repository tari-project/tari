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

use crate::{
    blocks::NewBlockTemplate,
    chain_storage::MmrTree,
    proof_of_work::PowAlgorithm,
    transactions::types::{Commitment, Signature},
};
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Error, Formatter};
use tari_common_types::types::HashOutput;
use tari_crypto::tari_utilities::hex::Hex;

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
    GetHeaderByHash(HashOutput),
    GetBlockByHash(HashOutput),
    GetNewBlockTemplate(GetNewBlockTemplateRequest),
    GetNewBlock(NewBlockTemplate),
    FetchKernelByExcessSig(Signature),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetNewBlockTemplateRequest {
    pub algo: PowAlgorithm,
    pub max_weight: u64,
}

impl Display for NodeCommsRequest {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        use NodeCommsRequest::*;
        match self {
            GetChainMetadata => write!(f, "GetChainMetadata"),
            FetchHeaders(v) => write!(f, "FetchHeaders (n={})", v.len()),
            FetchHeadersWithHashes(v) => write!(f, "FetchHeadersWithHashes (n={})", v.len()),
            FetchHeadersAfter(v, _hash) => write!(f, "FetchHeadersAfter (n={})", v.len()),
            FetchMatchingUtxos(v) => write!(f, "FetchMatchingUtxos (n={})", v.len()),
            FetchMatchingTxos(v) => write!(f, "FetchMatchingTxos (n={})", v.len()),
            FetchMatchingBlocks(v) => write!(f, "FetchMatchingBlocks (n={})", v.len()),
            FetchBlocksWithHashes(v) => write!(f, "FetchBlocksWithHashes (n={})", v.len()),
            FetchBlocksWithKernels(v) => write!(f, "FetchBlocksWithKernels (n={})", v.len()),
            FetchBlocksWithStxos(v) => write!(f, "FetchBlocksWithStxos (n={})", v.len()),
            FetchBlocksWithUtxos(v) => write!(f, "FetchBlocksWithUtxos (n={})", v.len()),
            GetHeaderByHash(v) => write!(f, "GetHeaderByHash({})", v.to_hex()),
            GetBlockByHash(v) => write!(f, "GetBlockByHash({})", v.to_hex()),
            GetNewBlockTemplate(v) => write!(f, "GetNewBlockTemplate ({}) with weight {}", v.algo, v.max_weight),
            GetNewBlock(b) => write!(f, "GetNewBlock (Block Height={})", b.header.height),
            FetchKernelByExcessSig(s) => write!(
                f,
                "FetchKernelByExcessSig (signature=({}, {}))",
                s.get_public_nonce().to_hex(),
                s.get_signature().to_hex()
            ),
        }
    }
}
