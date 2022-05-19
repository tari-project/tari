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

use std::convert::TryFrom;

use tari_common_types::types::BlindingFactor;
use tari_core::{blocks::BlockHeader, proof_of_work::ProofOfWork};
use tari_utilities::{ByteArray, Hashable};

use crate::{
    conversions::{datetime_to_timestamp, timestamp_to_datetime},
    tari_rpc as grpc,
};

impl From<BlockHeader> for grpc::BlockHeader {
    fn from(h: BlockHeader) -> Self {
        let pow_algo = h.pow_algo();
        Self {
            hash: h.hash(),
            version: u32::from(h.version),
            height: h.height,
            prev_hash: h.prev_hash,
            timestamp: datetime_to_timestamp(h.timestamp),
            input_mr: h.input_mr,
            output_mr: h.output_mr,
            output_mmr_size: h.output_mmr_size,
            kernel_mr: h.kernel_mr,
            kernel_mmr_size: h.kernel_mmr_size,
            witness_mr: h.witness_mr,
            total_kernel_offset: h.total_kernel_offset.to_vec(),
            total_script_offset: h.total_script_offset.to_vec(),
            nonce: h.nonce,
            pow: Some(grpc::ProofOfWork {
                pow_algo: pow_algo.as_u64(),
                pow_data: h.pow.pow_data,
            }),
        }
    }
}

impl TryFrom<grpc::BlockHeader> for BlockHeader {
    type Error = String;

    fn try_from(header: grpc::BlockHeader) -> Result<Self, Self::Error> {
        let total_kernel_offset =
            BlindingFactor::from_bytes(&header.total_kernel_offset).map_err(|err| err.to_string())?;

        let total_script_offset =
            BlindingFactor::from_bytes(&header.total_script_offset).map_err(|err| err.to_string())?;

        let timestamp = header
            .timestamp
            .and_then(timestamp_to_datetime)
            .ok_or_else(|| "timestamp not provided or was negative".to_string())?;

        let pow = match header.pow {
            Some(p) => ProofOfWork::try_from(p)?,
            None => return Err("No proof of work provided".into()),
        };
        Ok(Self {
            version: u16::try_from(header.version).map_err(|_| "header version too large")?,
            height: header.height,
            prev_hash: header.prev_hash,
            timestamp,
            input_mr: header.input_mr,
            output_mr: header.output_mr,
            witness_mr: header.witness_mr,
            output_mmr_size: header.output_mmr_size,
            kernel_mr: header.kernel_mr,
            kernel_mmr_size: header.kernel_mmr_size,
            total_kernel_offset,
            total_script_offset,
            nonce: header.nonce,
            pow,
        })
    }
}
