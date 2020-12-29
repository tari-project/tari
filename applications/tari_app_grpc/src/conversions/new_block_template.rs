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

use crate::tari_rpc as grpc;
use std::convert::{TryFrom, TryInto};
use tari_core::{
    blocks::{NewBlockHeaderTemplate, NewBlockTemplate},
    proof_of_work::ProofOfWork,
    transactions::types::BlindingFactor,
};
use tari_crypto::tari_utilities::ByteArray;
impl From<NewBlockTemplate> for grpc::NewBlockTemplate {
    fn from(block: NewBlockTemplate) -> Self {
        let header = grpc::NewBlockHeaderTemplate {
            version: block.header.version as u32,
            height: block.header.height,
            prev_hash: block.header.prev_hash.clone(),
            total_kernel_offset: Vec::from(block.header.total_kernel_offset.as_bytes()),
            pow: Some(grpc::ProofOfWork {
                pow_algo: block.header.pow.pow_algo.as_u64(),
                accumulated_monero_difficulty: block.header.pow.accumulated_monero_difficulty.into(),
                accumulated_blake_difficulty: block.header.pow.accumulated_blake_difficulty.into(),
                pow_data: block.header.pow.pow_data,
            }),
            target_difficulty: block.header.target_difficulty.into(),
        };
        Self {
            body: Some(grpc::AggregateBody {
                inputs: block
                    .body
                    .inputs()
                    .iter()
                    .map(|input| grpc::TransactionInput::from(input.clone()))
                    .collect(),
                outputs: block
                    .body
                    .outputs()
                    .iter()
                    .map(|output| grpc::TransactionOutput::from(output.clone()))
                    .collect(),
                kernels: block
                    .body
                    .kernels()
                    .iter()
                    .map(|kernel| grpc::TransactionKernel::from(kernel.clone()))
                    .collect(),
            }),
            header: Some(header),
        }
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
            target_difficulty: header.target_difficulty.into(),
        };
        let body = block
            .body
            .map(TryInto::try_into)
            .ok_or_else(|| "Block body not provided".to_string())??;

        Ok(Self { header, body })
    }
}
