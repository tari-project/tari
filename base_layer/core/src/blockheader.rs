// Copyright 2018 The Tari Project
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
//
// Portions of this file were originally copyrighted (c) 2018 The Grin Developers, issued under the Apache License,
// Version 2.0, available at http://www.apache.org/licenses/LICENSE-2.0.

use crate::{pow::ProofOfWork, types::*};
use chrono::{DateTime, NaiveDate, Utc};
use digest::Input;
use serde::{Deserialize, Serialize};
use tari_crypto::ristretto::*;
use tari_utilities::{ByteArray, Hashable};

type BlockHash = [u8; 32];

/// The BlockHeader contains all the metadata for the block, including proof of work, a link to the previous block
/// and the transaction kernels.
#[derive(Serialize, Deserialize, Clone)]
pub struct BlockHeader {
    /// Version of the block
    pub version: u16,
    /// Height of this block since the genesis block (height 0)
    pub height: u64,
    /// Hash of the block previous to this in the chain.
    pub prev_hash: BlockHash,
    /// Timestamp at which the block was built.
    pub timestamp: DateTime<Utc>,
    /// This is the MMR root of the outputs
    pub output_mmr: BlockHash,
    /// This is the MMR root of the range proofs
    pub range_proof_mmr: BlockHash,
    /// This is the MMR root of the kernels
    pub kernel_mmr: BlockHash,
    /// Total accumulated sum of kernel offsets since genesis block. We can derive the kernel offset sum for *this*
    /// block from the total kernel offset of the previous block header.
    pub total_kernel_offset: BlindingFactor,
    /// Nonce used
    /// Proof of work summary
    pub pow: ProofOfWork,
}

impl BlockHeader {
    /// This function will validate the proof of work in the header
    pub fn validate_pow(&self) -> bool {
        unimplemented!();
    }

    pub fn create_empty() -> BlockHeader {
        BlockHeader {
            version: 0,
            height: 0,
            prev_hash: [0; 32],
            timestamp: DateTime::<Utc>::from_utc(NaiveDate::from_ymd(1900, 1, 1).and_hms(1, 1, 1), Utc),
            output_mmr: [0; 32],
            range_proof_mmr: [0; 32],
            kernel_mmr: [0; 32],
            total_kernel_offset: RistrettoSecretKey::from(0),
            pow: ProofOfWork {},
        }
    }
}

impl Hashable for BlockHeader {
    fn hash(&self) -> Vec<u8> {
        HashDigest::new()
            .chain(self.version.to_le_bytes())
            .chain(self.height.to_le_bytes())
            .chain(self.prev_hash.as_bytes())
            .chain(self.timestamp.timestamp().to_le_bytes())
            .chain(self.output_mmr.as_bytes())
            .chain(self.range_proof_mmr.as_bytes())
            .chain(self.kernel_mmr.as_bytes())
            .chain(self.total_kernel_offset.as_bytes())
            .chain(self.pow.as_bytes())
            .result()
            .to_vec()
    }
}
