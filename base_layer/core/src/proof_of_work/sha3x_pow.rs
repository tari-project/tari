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

use sha3::{Digest, Sha3_256};

use crate::{
    blocks::BlockHeader,
    proof_of_work::{error::DifficultyError, Difficulty},
};

/// The Tari Sha3X proof-of-work algorithm. This is the reference implementation of Tari's standalone mining
/// algorithm as described in [RFC-0131](https://rfc.tari.com/RFC-0131_Mining.html).
///
/// In short Sha3X is a triple Keccak Sha3-256 hash of the nonce, mining hash and PoW mode byte.
/// Mining using this CPU version of the algorithm is unlikely to be profitable, but is included for reference and
/// can be used to mine tXTR on testnets.
pub fn sha3x_difficulty(header: &BlockHeader) -> Result<Difficulty, DifficultyError> {
    Ok(sha3x_difficulty_with_hash(header)?.0)
}

/// Calculate the Tari Sha3 mining hash
pub fn sha3_hash(header: &BlockHeader) -> Vec<u8> {
    Sha3_256::new()
        .chain_update(header.nonce.to_le_bytes())
        .chain_update(header.mining_hash())
        .chain_update(header.pow.to_bytes())
        .finalize()
        .to_vec()
}

/// Calculate the Tari Sha3X mining hash and achieved difficulty
fn sha3x_difficulty_with_hash(header: &BlockHeader) -> Result<(Difficulty, Vec<u8>), DifficultyError> {
    let hash = sha3_hash(header);
    let hash = Sha3_256::digest(hash);
    let hash = Sha3_256::digest(hash);
    let difficulty = Difficulty::big_endian_difficulty(&hash)?;
    Ok((difficulty, hash.to_vec()))
}

#[cfg(test)]
pub mod test {
    use chrono::{DateTime, NaiveDate, Utc};
    use tari_utilities::epoch_time::EpochTime;

    use crate::{
        blocks::BlockHeader,
        proof_of_work::{sha3x_pow::sha3x_difficulty, Difficulty, PowAlgorithm},
    };

    /// A simple example miner. It starts at nonce = 0 and iterates until it finds a header hash that meets the desired
    /// target block
    #[allow(dead_code)]
    fn mine_sha3(target_difficulty: Difficulty, header: &mut BlockHeader) -> u64 {
        header.nonce = 0;
        // We're mining over here!
        while sha3x_difficulty(header).unwrap() < target_difficulty {
            header.nonce += 1;
        }
        header.nonce
    }

    pub fn get_header() -> BlockHeader {
        let mut header = BlockHeader::new(2);

        #[allow(clippy::cast_sign_loss)]
        let epoch_secs = DateTime::<Utc>::from_utc(
            NaiveDate::from_ymd_opt(2000, 1, 1)
                .unwrap()
                .and_hms_opt(1, 1, 1)
                .unwrap(),
            Utc,
        )
        .timestamp() as u64;
        header.timestamp = EpochTime::from_secs_since_epoch(epoch_secs);
        header.pow.pow_algo = PowAlgorithm::Sha3x;
        header
    }

    #[test]
    fn validate_max_target() {
        let mut header = get_header();
        header.nonce = 154;
        println!("{:?}", header);
        assert_eq!(sha3x_difficulty(&header).unwrap(), Difficulty::from_u64(6564).unwrap());
    }
}
