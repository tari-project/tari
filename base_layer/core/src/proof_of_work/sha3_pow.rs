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

use crate::{blocks::BlockHeader, proof_of_work::Difficulty, U256};
use sha3::{Digest, Sha3_256};
use tari_crypto::tari_utilities::ByteArray;

const MAX_TARGET: U256 = U256::MAX;

/// A simple sha3 proof of work. This is currently intended to be used for testing and perhaps Testnet until
/// Monero merge-mining is active.
///
/// The proof of work difficulty is given by `H256(header )` where Hnnn is the sha3 digest of length
/// `nnn` bits.
pub fn sha3_difficulty(header: &BlockHeader) -> Difficulty {
    sha3_difficulty_with_hash(header).0
}

pub fn sha3_hash(header: &BlockHeader) -> Vec<u8> {
    Sha3_256::new()
        .chain(header.version.to_le_bytes())
        .chain(header.height.to_le_bytes())
        .chain(header.prev_hash.as_bytes())
        .chain(header.timestamp.as_u64().to_le_bytes())
        .chain(header.output_mr.as_bytes())
        .chain(header.range_proof_mr.as_bytes())
        .chain(header.kernel_mr.as_bytes())
        .chain(header.total_kernel_offset.as_bytes())
        .chain(header.nonce.to_le_bytes())
        .chain(header.pow.to_bytes())
        .finalize()
        .to_vec()
}

pub fn sha3_difficulty_with_hash(header: &BlockHeader) -> (Difficulty, Vec<u8>) {
    let hash = sha3_hash(header);
    let hash = Sha3_256::digest(&hash);
    let scalar = U256::from_big_endian(&hash); // Big endian so the hash has leading zeroes
    let result = MAX_TARGET / scalar;
    let difficulty = result.low_u64().into();
    (difficulty, hash.to_vec())
}

#[cfg(test)]
pub mod test {
    use crate::{
        blocks::BlockHeader,
        proof_of_work::{
            sha3_pow::{sha3_difficulty, sha3_difficulty_with_hash},
            Difficulty,
            PowAlgorithm,
        },
    };
    use chrono::{DateTime, NaiveDate, Utc};
    use tari_crypto::tari_utilities::hex::Hex;

    /// A simple example miner. It starts at nonce = 0 and iterates until it finds a header hash that meets the desired
    /// target block
    #[allow(dead_code)]
    fn mine_sha3(target_difficulty: Difficulty, header: &mut BlockHeader) -> u64 {
        header.nonce = 0;
        // We're mining over here!
        while sha3_difficulty(&header) < target_difficulty {
            header.nonce += 1;
        }
        header.nonce
    }

    pub fn get_header() -> BlockHeader {
        let mut header = BlockHeader::new(0);
        header.timestamp = DateTime::<Utc>::from_utc(NaiveDate::from_ymd(2000, 1, 1).and_hms(1, 1, 1), Utc).into();
        header.pow.pow_algo = PowAlgorithm::Sha3;
        header
    }

    #[test]
    fn validate_max_target() {
        let mut header = get_header();
        header.nonce = 1;
        assert_eq!(sha3_difficulty(&header), Difficulty::from(10));
    }

    #[test]
    fn difficulty_1000() {
        let mut header = get_header();
        header.nonce = 1_332;
        let (diff, hash) = sha3_difficulty_with_hash(&header);
        assert_eq!(diff, Difficulty::from(3_832));
        assert_eq!(
            hash.to_hex(),
            "00111a1b0aa98f1f431a582ae8c912054c53f3f36a967b3de51d152be20fc96c"
        );
    }

    #[test]
    fn difficulty_1mil() {
        let mut header = get_header();
        header.nonce = 2_602_226;
        let (diff, hash) = sha3_difficulty_with_hash(&header);
        assert_eq!(diff, Difficulty::from(1_307_012));
        assert_eq!(
            hash.to_hex(),
            "00000cd61843b495dc92adbd669dc3878c79add579a422ea2dd5b58100babb95"
        );
    }
}
