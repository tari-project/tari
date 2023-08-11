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

use std::convert::{TryFrom, TryInto};

use tari_utilities::ByteArray;

use crate::{blocks::Block, chain_storage::PrunedOutput, mempool::FeePerGramStat, proto::base_node as proto};

impl TryFrom<Block> for proto::BlockBodyResponse {
    type Error = String;

    fn try_from(block: Block) -> Result<Self, Self::Error> {
        Ok(Self {
            hash: block.hash().to_vec(),
            body: Some(block.body.try_into()?),
        })
    }
}

impl TryFrom<PrunedOutput> for proto::SyncUtxo {
    type Error = String;

    fn try_from(output: PrunedOutput) -> Result<Self, Self::Error> {
        Ok(match output {
            PrunedOutput::Pruned { output_hash } => proto::SyncUtxo {
                utxo: Some(proto::sync_utxo::Utxo::PrunedOutput(proto::PrunedOutput {
                    hash: output_hash.to_vec(),
                })),
            },
            PrunedOutput::NotPruned { output } => proto::SyncUtxo {
                utxo: Some(proto::sync_utxo::Utxo::Output(output.try_into()?)),
            },
        })
    }
}

impl From<Vec<FeePerGramStat>> for proto::GetMempoolFeePerGramStatsResponse {
    fn from(stats: Vec<FeePerGramStat>) -> Self {
        Self {
            stats: stats.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<FeePerGramStat> for proto::MempoolFeePerGramStat {
    fn from(stat: FeePerGramStat) -> Self {
        Self {
            order: stat.order,
            min_fee_per_gram: stat.min_fee_per_gram.as_u64(),
            avg_fee_per_gram: stat.avg_fee_per_gram.as_u64(),
            max_fee_per_gram: stat.max_fee_per_gram.as_u64(),
        }
    }
}
