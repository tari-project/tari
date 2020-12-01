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

use crate::{
    conversions::{datetime_to_timestamp, timestamp_to_datetime},
    tari_rpc as grpc,
};
use std::convert::TryFrom;
use tari_core::{blocks::BlockHeader, proof_of_work::ProofOfWork, transactions::types::BlindingFactor};
use tari_crypto::tari_utilities::{ByteArray, Hashable};

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
                pow_algo: h.pow_algo().as_u64(),
                accumulated_monero_difficulty: h.pow.accumulated_monero_difficulty.into(),
                accumulated_blake_difficulty: h.pow.accumulated_blake_difficulty.into(),
                pow_data: h.pow.pow_data,
                target_difficulty: h.pow.target_difficulty.as_u64(),
            }),
        }
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
