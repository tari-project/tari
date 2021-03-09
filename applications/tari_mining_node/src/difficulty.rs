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

use crate::errors::{err_empty, MinerError};
use sha3::{Digest, Sha3_256};
use tari_app_grpc::tari_rpc::BlockHeader;
use tari_core::large_ints::U256;

pub type Difficulty = u64;

pub struct BlockHeaderSha3 {
    header: BlockHeader,
    pow_bytes: Vec<u8>,
    hash_before_timestamp: Sha3_256,
    hash_before_nonce: Sha3_256,
    pub timestamp: u64,
    pub nonce: u64,
    pub hashes: u64,
}

impl BlockHeaderSha3 {
    pub fn new(header: BlockHeader) -> Result<Self, MinerError> {
        use std::convert::TryFrom;
        use tari_core::proof_of_work::ProofOfWork; // this is only dep left on tari_code

        // Not stressing about efficiency here as it will change soon
        let pow = ProofOfWork::try_from(header.pow.clone().ok_or_else(|| err_empty("header.pow"))?)
            .map_err(MinerError::BlockHeader)?;
        let timestamp = header.timestamp.as_ref().ok_or_else(|| err_empty("header.timestamp"))?;
        let hash_before_timestamp = Sha3_256::new()
            .chain((header.version as u16).to_le_bytes())
            .chain(header.height.to_le_bytes())
            .chain(&header.prev_hash);
        let hash_before_nonce = hash_before_timestamp
            .clone()
            .chain((timestamp.seconds as u64).to_le_bytes())
            .chain(&header.output_mr)
            .chain(&header.range_proof_mr)
            .chain(&header.kernel_mr)
            .chain(&header.total_kernel_offset);

        Ok(Self {
            pow_bytes: pow.to_bytes(),
            hash_before_timestamp,
            hash_before_nonce,
            timestamp: timestamp.seconds as u64,
            nonce: header.nonce,
            header,
            hashes: 0,
        })
    }

    pub fn set_timestamp(&mut self, timestamp: u64) {
        self.hash_before_nonce = self
            .hash_before_timestamp
            .clone()
            .chain(timestamp.to_le_bytes())
            .chain(&self.header.output_mr)
            .chain(&self.header.range_proof_mr)
            .chain(&self.header.kernel_mr)
            .chain(&self.header.total_kernel_offset);
        self.timestamp = timestamp;
    }

    pub fn random_nonce(&mut self) {
        use rand::{rngs::OsRng, RngCore};
        self.nonce = OsRng.next_u64();
    }

    #[inline]
    pub fn inc_nonce(&mut self) {
        self.nonce = self.nonce.wrapping_add(1);
    }

    #[inline]
    pub fn difficulty(&mut self) -> Difficulty {
        self.hashes = self.hashes.saturating_add(1);
        let hash = self
            .hash_before_nonce
            .clone()
            .chain(self.nonce.to_le_bytes())
            .chain(&self.pow_bytes)
            .finalize();
        let hash = Sha3_256::digest(&hash);
        big_endian_difficulty(&hash)
    }

    pub fn into_header(mut self) -> BlockHeader {
        self.header.timestamp = Some(prost_types::Timestamp {
            seconds: self.timestamp as i64,
            nanos: 0,
        });
        self.header.nonce = self.nonce;
        self.header
    }

    #[inline]
    pub fn height(&self) -> u64 {
        self.header.height
    }
}

/// This will provide the difficulty of the hash assuming the hash is big_endian
fn big_endian_difficulty(hash: &[u8]) -> Difficulty {
    let scalar = U256::from_big_endian(hash); // Big endian so the hash has leading zeroes
    let result = U256::MAX / scalar;
    result.low_u64()
}

#[cfg(test)]
pub mod test {
    use super::*;
    use chrono::{DateTime, NaiveDate, Utc};
    use tari_core::{blocks::BlockHeader as CoreBlockHeader, proof_of_work::sha3_difficulty as core_sha3_difficulty};

    pub fn get_header() -> (BlockHeader, CoreBlockHeader) {
        let mut header = CoreBlockHeader::new(0);
        header.timestamp = DateTime::<Utc>::from_utc(NaiveDate::from_ymd(2000, 1, 1).and_hms(1, 1, 1), Utc).into();
        header.pow.pow_algo = tari_core::proof_of_work::PowAlgorithm::Sha3;
        (header.clone().into(), header)
    }

    #[test]
    fn validate_nonce_difficulty() {
        let (mut header, mut core_header) = get_header();
        header.nonce = 1;
        core_header.nonce = 1;
        let mut hasher = BlockHeaderSha3::new(header).unwrap();
        for _ in 0..1000 {
            assert_eq!(
                hasher.difficulty(),
                core_sha3_difficulty(&core_header).as_u64(),
                "with nonces = {}:{}",
                hasher.nonce,
                core_header.nonce
            );
            core_header.nonce += 1;
            hasher.inc_nonce();
        }
    }

    #[test]
    fn validate_timestamp_difficulty() {
        let (mut header, mut core_header) = get_header();
        header.nonce = 1;
        core_header.nonce = 1;
        let mut hasher = BlockHeaderSha3::new(header).unwrap();
        let mut timestamp = core_header.timestamp;
        for _ in 0..1000 {
            assert_eq!(
                hasher.difficulty(),
                core_sha3_difficulty(&core_header).as_u64(),
                "with timestamp = {}",
                timestamp
            );
            timestamp = timestamp.increase(1);
            core_header.timestamp = timestamp;
            hasher.set_timestamp(timestamp.as_u64());
        }
    }
}
