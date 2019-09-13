// Copyright 2019. The Tari Project
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
    blocks::BlockHeader,
    proof_of_work::{Difficulty, PowError, ProofOfWork},
};
use bigint::uint::U256;
use blake2::Blake2b;
use digest::Digest;
use serde::{Deserialize, Serialize};
use tari_crypto::common::Blake256;
use tari_utilities::{ByteArray, ByteArrayError, Hashable};

const MAX_TARGET: U256 = U256::MAX;

/// A simple Blake2b-based proof of work. This is currently intended to be used for testing and perhaps Testnet until
/// Monero merge-mining is active.
///
/// The proof of work difficulty is given by `H256(H512(header || nonce))` where Hnnn is the Blake2b digest of length
/// `nnn` bits.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BlakePow;

impl BlakePow {
    /// A simple miner. It starts at nonce = 0 and iterates until it finds a header hash that meets the desired target
    pub fn mine(target_difficulty: Difficulty, header: &BlockHeader) -> u64 {
        let mut nonce = 0u64;
        // We're mining over here!
        while let Ok(d) = header.pow.calculate_difficulty(nonce, &header) {
            if d >= target_difficulty {
                break;
            }
            nonce += 1;
        }
        nonce
    }
}

impl ProofOfWork for BlakePow {
    fn calculate_difficulty(&self, nonce: u64, header: &BlockHeader) -> Result<Difficulty, PowError> {
        let bytes = header.hash();
        let hash = Blake2b::new()
            .chain(&bytes)
            .chain(nonce.to_le_bytes())
            .result()
            .to_vec();
        let hash = Blake256::digest(&hash).to_vec();
        let scalar = U256::from_little_endian(&hash);
        let result = MAX_TARGET / scalar;
        let difficulty = u64::from(result).into();
        Ok(difficulty)
    }
}

impl Default for BlakePow {
    fn default() -> Self {
        BlakePow
    }
}

impl ByteArray for BlakePow {
    fn from_bytes(_bytes: &[u8]) -> Result<Self, ByteArrayError> {
        Ok(BlakePow)
    }

    fn as_bytes(&self) -> &[u8] {
        &[]
    }
}

impl Hashable for BlakePow {
    fn hash(&self) -> Vec<u8> {
        vec![]
    }
}

#[cfg(test)]
mod test {
    use crate::{blocks::BlockHeader, proof_of_work::ProofOfWork};
    use chrono::{DateTime, NaiveDate, Utc};

    fn get_header() -> BlockHeader {
        let mut header = BlockHeader::new(0);
        header.timestamp = DateTime::<Utc>::from_utc(NaiveDate::from_ymd(2000, 1, 1).and_hms(1, 1, 1), Utc);
        header
    }
    #[test]
    fn validate_max_target() {
        let header = get_header();
        assert_eq!(header.pow.calculate_difficulty(2, &header), Ok(1.into()));
    }

    #[test]
    fn difficulty_1000() {
        let header = get_header();
        assert_eq!(header.pow.calculate_difficulty(108, &header), Ok(1273.into()));
    }

    #[test]
    fn difficulty_1mil() {
        let header = get_header();
        assert_eq!(header.pow.calculate_difficulty(134_390, &header), Ok(3_250_351.into()));
    }
}
