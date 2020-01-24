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

use crate::{blocks::BlockHeader, proof_of_work::Difficulty};
use bigint::uint::U256;
use blake2::Blake2b;
use digest::Digest;
use tari_crypto::common::Blake256;
use tari_utilities::Hashable;

const MAX_TARGET: U256 = U256::MAX;

/// A simple Blake2b-based proof of work. This is currently intended to be used for testing and perhaps Testnet until
/// Monero merge-mining is active.
///
/// The proof of work difficulty is given by `H256(H512(header || nonce))` where Hnnn is the Blake2b digest of length
/// `nnn` bits.
pub fn blake_difficulty(header: &BlockHeader) -> Difficulty {
    blake_difficulty_with_hash(header).0
}

pub fn blake_difficulty_with_hash(header: &BlockHeader) -> (Difficulty, Vec<u8>) {
    let bytes = header.hash();
    let hash = Blake2b::digest(&bytes).to_vec();
    let hash = Blake256::digest(&hash).to_vec();
    let scalar = U256::from_big_endian(&hash); // Big endian so the hash has leading zeroes
    let result = MAX_TARGET / scalar;
    let difficulty = u64::from(result).into();
    (difficulty, hash)
}

#[cfg(test)]
pub mod test {
    use crate::{
        blocks::BlockHeader,
        proof_of_work::{
            blake_pow::{blake_difficulty, blake_difficulty_with_hash},
            Difficulty,
        },
    };
    use chrono::{DateTime, NaiveDate, Utc};
    use tari_utilities::hex::Hex;

    /// A simple example miner. It starts at nonce = 0 and iterates until it finds a header hash that meets the desired
    /// target block
    #[allow(dead_code)]
    fn mine_blake(target_difficulty: Difficulty, header: &mut BlockHeader) -> u64 {
        header.nonce = 0;
        // We're mining over here!
        while blake_difficulty(&header) < target_difficulty {
            header.nonce += 1;
        }
        header.nonce
    }

    pub fn get_header() -> BlockHeader {
        let mut header = BlockHeader::new(0);
        header.timestamp = DateTime::<Utc>::from_utc(NaiveDate::from_ymd(2000, 1, 1).and_hms(1, 1, 1), Utc).into();
        header
    }

    #[test]
    fn validate_max_target() {
        let mut header = get_header();
        header.nonce = 1;
        assert_eq!(blake_difficulty(&header), Difficulty::from(5));
    }

    #[test]
    fn difficulty_1000() {
        let mut header = get_header();
        header.nonce = 2606;
        let (diff, hash) = blake_difficulty_with_hash(&header);
        assert_eq!(diff, Difficulty::from(1_385));
        assert_eq!(
            hash.to_hex(),
            "002f4dc46d5ac0f0207629095a479d6b0fa7d3db08a1ae1790e4d2078376948d"
        );
    }

    #[test]
    fn difficulty_1mil() {
        let mut header = get_header();
        header.nonce = 7_945_536;
        let (diff, hash) = blake_difficulty_with_hash(&header);
        assert_eq!(diff, Difficulty::from(2_459_030));
        assert_eq!(
            hash.to_hex(),
            "000006d29c3fce2f73e2a96daa9071f3c5c65f0b9334513bca6a39d279c5faaf"
        );
    }
}
