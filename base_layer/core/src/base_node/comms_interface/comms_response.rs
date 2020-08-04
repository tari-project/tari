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
    blocks::{blockheader::BlockHeader, Block, NewBlockTemplate},
    chain_storage::{ChainMetadata, HistoricalBlock},
    proof_of_work::Difficulty,
    transactions::{
        transaction::{TransactionKernel, TransactionOutput},
        types::HashOutput,
    },
};
use serde::{Deserialize, Serialize};
use std::fmt::{self, Display, Formatter};

/// API Response enum
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum NodeCommsResponse {
    ChainMetadata(ChainMetadata),
    TransactionKernels(Vec<TransactionKernel>),
    BlockHeaders(Vec<BlockHeader>),
    TransactionOutputs(Vec<TransactionOutput>),
    HistoricalBlocks(Vec<HistoricalBlock>),
    NewBlockTemplate(NewBlockTemplate),
    NewBlock(Block),
    TargetDifficulty(Difficulty),
    FetchHeadersAfterResponse(Vec<BlockHeader>),
    MmrNodeCount(u32),
    MmrNodes(Vec<HashOutput>, Vec<u8>),
}

impl Display for NodeCommsResponse {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        use NodeCommsResponse::*;
        match &self {
            ChainMetadata(_) => write!(f, "ChainMetadata"),
            TransactionKernels(_) => write!(f, "TransactionKernel"),
            BlockHeaders(_) => write!(f, "BlockHeaders"),
            TransactionOutputs(_) => write!(f, "TransactionOutputs"),
            HistoricalBlocks(_) => write!(f, "HistoricalBlocks"),
            NewBlockTemplate(_) => write!(f, "NewBlockTemplate"),
            NewBlock(_) => write!(f, "NewBlock"),
            TargetDifficulty(_) => write!(f, "TargetDifficulty"),
            FetchHeadersAfterResponse(_) => write!(f, "FetchHeadersAfterResponse"),
            MmrNodeCount(_) => write!(f, "MmrNodeCount"),
            MmrNodes(_, _) => write!(f, "MmrNodes"),
        }
    }
}
