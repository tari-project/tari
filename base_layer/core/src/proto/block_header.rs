// Copyright 2021. The Tari Project
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

use super::core as proto;
use crate::{
    blocks::BlockHeader,
    proof_of_work::{PowAlgorithm, ProofOfWork},
    proto::utils::{datetime_to_timestamp, timestamp_to_datetime},
    transactions::types::BlindingFactor,
};
use std::convert::TryFrom;
use tari_crypto::tari_utilities::ByteArray;

//---------------------------------- BlockHeader --------------------------------------------//
impl TryFrom<proto::BlockHeader> for BlockHeader {
    type Error = String;

    fn try_from(header: proto::BlockHeader) -> Result<Self, Self::Error> {
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
            output_mmr_size: header.output_mmr_size,
            kernel_mr: header.kernel_mr,
            kernel_mmr_size: header.kernel_mmr_size,
            total_kernel_offset,
            nonce: header.nonce,
            pow,
        })
    }
}

impl From<BlockHeader> for proto::BlockHeader {
    fn from(header: BlockHeader) -> Self {
        Self {
            version: header.version as u32,
            height: header.height,
            prev_hash: header.prev_hash,
            timestamp: Some(datetime_to_timestamp(header.timestamp)),
            output_mr: header.output_mr,
            range_proof_mr: header.range_proof_mr,
            kernel_mr: header.kernel_mr,
            total_kernel_offset: header.total_kernel_offset.to_vec(),
            nonce: header.nonce,
            pow: Some(proto::ProofOfWork::from(header.pow)),
            kernel_mmr_size: header.kernel_mmr_size,
            output_mmr_size: header.output_mmr_size,
        }
    }
}

//---------------------------------- ProofOfWork --------------------------------------------//
#[allow(deprecated)]
impl TryFrom<proto::ProofOfWork> for ProofOfWork {
    type Error = String;

    fn try_from(pow: proto::ProofOfWork) -> Result<Self, Self::Error> {
        Ok(Self {
            pow_algo: PowAlgorithm::try_from(pow.pow_algo)?,
            pow_data: pow.pow_data,
        })
    }
}

#[allow(deprecated)]
impl From<ProofOfWork> for proto::ProofOfWork {
    fn from(pow: ProofOfWork) -> Self {
        Self {
            pow_algo: pow.pow_algo as u64,
            pow_data: pow.pow_data,
        }
    }
}
