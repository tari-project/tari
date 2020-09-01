// Copyright 2020. The Tari Project
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

use crate::tari_grpc::base_node_grpc as grpc;
use prost_types::Timestamp;
use std::convert::{TryFrom, TryInto};
use tari_core::{
    blocks::{Block, BlockHeader, NewBlockHeaderTemplate, NewBlockTemplate},
    chain_storage::HistoricalBlock,
    proof_of_work::{Difficulty, PowAlgorithm, ProofOfWork},
    transactions::types::BlindingFactor,
};
use tari_crypto::tari_utilities::{epoch_time::EpochTime, ByteArray, Hashable};

/// Utility function that converts a `chrono::DateTime` to a `prost::Timestamp`
pub fn datetime_to_timestamp(datetime: EpochTime) -> Timestamp {
    Timestamp {
        seconds: datetime.as_u64() as i64,
        nanos: 0,
    }
}

pub(crate) fn timestamp_to_datetime(timestamp: Timestamp) -> EpochTime {
    (timestamp.seconds as u64).into()
}

impl From<HistoricalBlock> for grpc::HistoricalBlock {
    fn from(hb: HistoricalBlock) -> Self {
        Self {
            confirmations: hb.confirmations,
            spent_commitments: hb.spent_commitments.iter().map(|c| Vec::from(c.as_bytes())).collect(),
            block: Some(hb.block.into()),
        }
    }
}

impl From<tari_core::blocks::Block> for grpc::Block {
    fn from(block: Block) -> Self {
        Self {
            body: Some(grpc::AggregateBody {
                inputs: block
                    .body
                    .inputs()
                    .iter()
                    .map(|input| grpc::TransactionInput {
                        features: Some(grpc::OutputFeatures {
                            flags: input.features().flags.bits() as u32,
                            maturity: input.features().maturity,
                        }),
                        commitment: Vec::from(input.commitment().as_bytes()),
                        script_hash: input.script_hash().to_vec(),
                    })
                    .collect(),
                outputs: block
                    .body
                    .outputs()
                    .iter()
                    .map(|output| grpc::TransactionOutput {
                        features: Some(grpc::OutputFeatures {
                            flags: output.features().flags.bits() as u32,
                            maturity: output.features().maturity,
                        }),
                        commitment: Vec::from(output.commitment().as_bytes()),
                        range_proof: Vec::from(output.proof().as_bytes()),
                        script_hash: output.script_hash().to_vec(),
                    })
                    .collect(),
                kernels: block
                    .body
                    .kernels()
                    .iter()
                    .map(|kernel| grpc::TransactionKernel {
                        features: kernel.features.bits() as u32,
                        fee: kernel.fee.0,
                        lock_height: kernel.lock_height,
                        meta_info: kernel.meta_info.as_ref().cloned().unwrap_or_default(),
                        linked_kernel: kernel.linked_kernel.as_ref().cloned().unwrap_or_default(),
                        excess: Vec::from(kernel.excess.as_bytes()),
                        excess_sig: Some(grpc::Signature {
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

impl From<BlockHeader> for grpc::BlockHeader {
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
            pow: Some(grpc::ProofOfWork {
                pow_algo: match h.pow.pow_algo {
                    PowAlgorithm::Monero => 0,
                    PowAlgorithm::Blake => 1,
                },
                accumulated_monero_difficulty: h.pow.accumulated_monero_difficulty.into(),
                accumulated_blake_difficulty: h.pow.accumulated_blake_difficulty.into(),
                pow_data: h.pow.pow_data,
                target_difficulty: h.pow.target_difficulty.as_u64(),
            }),
        }
    }
}

impl From<NewBlockTemplate> for grpc::NewBlockTemplate {
    fn from(block: NewBlockTemplate) -> Self {
        let header = grpc::NewBlockHeaderTemplate {
            version: block.header.version as u32,
            height: block.header.height,
            prev_hash: block.header.prev_hash.clone(),
            total_kernel_offset: Vec::from(block.header.total_kernel_offset.as_bytes()),
            pow: Some(grpc::ProofOfWork {
                pow_algo: match block.header.pow.pow_algo {
                    PowAlgorithm::Monero => 0,
                    PowAlgorithm::Blake => 1,
                },
                accumulated_monero_difficulty: block.header.pow.accumulated_monero_difficulty.into(),
                accumulated_blake_difficulty: block.header.pow.accumulated_blake_difficulty.into(),
                pow_data: block.header.pow.pow_data,
                target_difficulty: block.header.pow.target_difficulty.as_u64(),
            }),
        };
        Self {
            body: Some(grpc::AggregateBody {
                inputs: block
                    .body
                    .inputs()
                    .iter()
                    .map(|input| grpc::TransactionInput {
                        features: Some(grpc::OutputFeatures {
                            flags: input.features().flags.bits() as u32,
                            maturity: input.features().maturity,
                        }),
                        commitment: Vec::from(input.commitment().as_bytes()),
                        script_hash: input.script_hash().to_vec(),
                    })
                    .collect(),
                outputs: block
                    .body
                    .outputs()
                    .iter()
                    .map(|output| grpc::TransactionOutput {
                        features: Some(grpc::OutputFeatures {
                            flags: output.features().flags.bits() as u32,
                            maturity: output.features().maturity,
                        }),
                        commitment: Vec::from(output.commitment().as_bytes()),
                        range_proof: Vec::from(output.proof().as_bytes()),
                        script_hash: output.script_hash().to_vec(),
                    })
                    .collect(),
                kernels: block
                    .body
                    .kernels()
                    .iter()
                    .map(|kernel| grpc::TransactionKernel {
                        features: kernel.features.bits() as u32,
                        fee: kernel.fee.0,
                        lock_height: kernel.lock_height,
                        meta_info: kernel.meta_info.as_ref().cloned().unwrap_or_default(),
                        linked_kernel: kernel.linked_kernel.as_ref().cloned().unwrap_or_default(),
                        excess: Vec::from(kernel.excess.as_bytes()),
                        excess_sig: Some(grpc::Signature {
                            public_nonce: Vec::from(kernel.excess_sig.get_public_nonce().as_bytes()),
                            signature: Vec::from(kernel.excess_sig.get_signature().as_bytes()),
                        }),
                    })
                    .collect(),
            }),
            header: Some(header),
        }
    }
}

impl TryFrom<grpc::Block> for Block {
    type Error = String;

    fn try_from(block: grpc::Block) -> Result<Self, Self::Error> {
        let header = block
            .header
            .map(TryInto::try_into)
            .ok_or_else(|| "Block header not provided".to_string())??;

        let body = block
            .body
            .map(TryInto::try_into)
            .ok_or_else(|| "Block body not provided".to_string())??;

        Ok(Self { header, body })
    }
}

impl TryFrom<grpc::BlockHeader> for BlockHeader {
    type Error = String;

    fn try_from(header: grpc::BlockHeader) -> Result<Self, Self::Error> {
        let total_kernel_offset =
            BlindingFactor::from_bytes(&header.total_kernel_offset).map_err(|err| err.to_string())?;

        let timestamp = header
            .timestamp
            .map(timestamp_to_datetime)
            .ok_or_else(|| "timestamp not provided".to_string())?;

        let pow = match header.pow {
            Some(p) => ProofOfWork::try_from(p)?,
            None => return Err("No proof of work provided".into()),
        };
        Ok(Self {
            version: header.version as u16,
            height: header.height,
            prev_hash: header.prev_hash,
            timestamp,
            output_mr: header.output_mr,
            range_proof_mr: header.range_proof_mr,
            kernel_mr: header.kernel_mr,
            total_kernel_offset,
            nonce: header.nonce,
            pow,
        })
    }
}

impl TryFrom<grpc::NewBlockTemplate> for NewBlockTemplate {
    type Error = String;

    fn try_from(block: grpc::NewBlockTemplate) -> Result<Self, Self::Error> {
        let header = block.header.clone().ok_or_else(|| "No header provided".to_string())?;
        let total_kernel_offset =
            BlindingFactor::from_bytes(&header.total_kernel_offset).map_err(|err| err.to_string())?;
        let pow = match header.pow {
            Some(p) => ProofOfWork::try_from(p)?,
            None => return Err("No proof of work provided".into()),
        };
        let header = NewBlockHeaderTemplate {
            version: header.version as u16,
            height: header.height,
            prev_hash: header.prev_hash,
            total_kernel_offset,
            pow,
        };
        let body = block
            .body
            .map(TryInto::try_into)
            .ok_or_else(|| "Block body not provided".to_string())??;

        Ok(Self { header, body })
    }
}

impl TryFrom<grpc::ProofOfWork> for ProofOfWork {
    type Error = String;

    fn try_from(pow: grpc::ProofOfWork) -> Result<Self, Self::Error> {
        Ok(Self {
            pow_algo: PowAlgorithm::try_from(pow.pow_algo)?,
            accumulated_monero_difficulty: Difficulty::from(pow.accumulated_monero_difficulty),
            accumulated_blake_difficulty: Difficulty::from(pow.accumulated_blake_difficulty),
            target_difficulty: Difficulty::from(pow.target_difficulty),
            pow_data: pow.pow_data,
        })
    }
}
