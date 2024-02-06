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

use std::convert::TryFrom;

use tari_common_types::types::{FixedHash, PrivateKey};
use tari_utilities::{epoch_time::EpochTime, ByteArray};

use super::core as proto;
use crate::{
    blocks::BlockHeader,
    proof_of_work::{PowAlgorithm, ProofOfWork},
};

//---------------------------------- BlockHeader --------------------------------------------//

impl TryFrom<proto::BlockHeader> for BlockHeader {
    type Error = String;

    fn try_from(header: proto::BlockHeader) -> Result<Self, Self::Error> {
        let total_kernel_offset =
            PrivateKey::from_canonical_bytes(&header.total_kernel_offset).map_err(|err| err.to_string())?;

        let total_script_offset =
            PrivateKey::from_canonical_bytes(&header.total_script_offset).map_err(|err| err.to_string())?;

        let pow = match header.pow {
            Some(p) => ProofOfWork::try_from(p)?,
            None => return Err("No proof of work provided".into()),
        };
        Ok(Self {
            version: u16::try_from(header.version).map_err(|err| err.to_string())?,
            height: header.height,
            prev_hash: FixedHash::try_from(header.prev_hash).map_err(|err| err.to_string())?,
            timestamp: EpochTime::from(header.timestamp),
            output_mr: FixedHash::try_from(header.output_mr).map_err(|err| err.to_string())?,
            output_smt_size: header.output_mmr_size,
            kernel_mr: FixedHash::try_from(header.kernel_mr).map_err(|err| err.to_string())?,
            kernel_mmr_size: header.kernel_mmr_size,
            input_mr: FixedHash::try_from(header.input_mr).map_err(|err| err.to_string())?,
            total_kernel_offset,
            total_script_offset,
            nonce: header.nonce,
            pow,
            validator_node_mr: FixedHash::try_from(header.validator_node_merkle_root).map_err(|err| err.to_string())?,
            validator_node_size: header.validator_node_size,
        })
    }
}

impl From<BlockHeader> for proto::BlockHeader {
    fn from(header: BlockHeader) -> Self {
        Self {
            version: u32::from(header.version),
            height: header.height,
            prev_hash: header.prev_hash.to_vec(),
            timestamp: header.timestamp.as_u64(),
            output_mr: header.output_mr.to_vec(),
            kernel_mr: header.kernel_mr.to_vec(),
            input_mr: header.input_mr.to_vec(),
            total_kernel_offset: header.total_kernel_offset.to_vec(),
            total_script_offset: header.total_script_offset.to_vec(),
            nonce: header.nonce,
            pow: Some(proto::ProofOfWork::from(header.pow)),
            kernel_mmr_size: header.kernel_mmr_size,
            output_mmr_size: header.output_smt_size,
            validator_node_merkle_root: header.validator_node_mr.to_vec(),
            validator_node_size: header.validator_node_size,
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
