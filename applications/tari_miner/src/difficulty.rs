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

use std::convert::TryInto;

use tari_app_grpc::tari_rpc::BlockHeader as grpc_header;
use tari_core::{
    blocks::BlockHeader,
    proof_of_work::{sha3x_difficulty, DifficultyError},
};
use tari_utilities::epoch_time::EpochTime;

use crate::errors::MinerError;

pub type Difficulty = u64;

#[derive(Clone)]
pub struct BlockHeaderSha3 {
    pub header: BlockHeader,
    pub hashes: u64,
}

impl BlockHeaderSha3 {
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_sign_loss)]
    pub fn new(header: grpc_header) -> Result<Self, MinerError> {
        let header: BlockHeader = header.try_into().map_err(MinerError::BlockHeader)?;
        Ok(Self { header, hashes: 0 })
    }

    /// This function will update the timestamp of the header, but only if the new timestamp is greater than the current
    /// one.
    pub fn set_forward_timestamp(&mut self, timestamp: u64) {
        // if the timestamp has been advanced by the base_node due to the median time we should not reverse it but we
        // should only change the timestamp if we move it forward.
        if timestamp > self.header.timestamp.as_u64() {
            self.header.timestamp = EpochTime::from(timestamp);
        }
    }

    pub fn random_nonce(&mut self) {
        use rand::{rngs::OsRng, RngCore};
        self.header.nonce = OsRng.next_u64();
    }

    #[inline]
    pub fn inc_nonce(&mut self) {
        self.header.nonce = self.header.nonce.wrapping_add(1);
    }

    #[inline]
    pub fn difficulty(&mut self) -> Result<Difficulty, DifficultyError> {
        self.hashes = self.hashes.saturating_add(1);
        Ok(sha3x_difficulty(&self.header)?.as_u64())
    }

    #[allow(clippy::cast_possible_wrap)]
    pub fn create_header(&self) -> grpc_header {
        self.header.clone().into()
    }

    #[inline]
    pub fn height(&self) -> u64 {
        self.header.height
    }
}

#[cfg(test)]
pub mod test {
    use chrono::{DateTime, NaiveDate, Utc};
    use tari_core::proof_of_work::sha3x_difficulty as core_sha3x_difficulty;

    use super::*;

    #[allow(clippy::cast_sign_loss)]
    pub fn get_header() -> (grpc_header, BlockHeader) {
        let mut header = BlockHeader::new(0);
        header.timestamp = (DateTime::<Utc>::from_utc(
            NaiveDate::from_ymd_opt(2000, 1, 1)
                .unwrap()
                .and_hms_opt(1, 1, 1)
                .unwrap(),
            Utc,
        )
        .timestamp() as u64)
            .into();
        header.pow.pow_algo = tari_core::proof_of_work::PowAlgorithm::Sha3x;
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
                hasher.difficulty().unwrap(),
                core_sha3x_difficulty(&core_header).unwrap().as_u64(),
                "with nonces = {}:{}",
                hasher.header.nonce,
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
                hasher.difficulty().unwrap(),
                core_sha3x_difficulty(&core_header).unwrap().as_u64(),
                "with timestamp = {}",
                timestamp
            );
            timestamp = timestamp.increase(1);
            core_header.timestamp = timestamp;
            hasher.set_forward_timestamp(timestamp.as_u64());
        }
    }
}
