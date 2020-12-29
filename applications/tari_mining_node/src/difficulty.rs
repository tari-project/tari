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

use sha3::{Digest, Sha3_256};
use tari_app_grpc::tari_rpc::BlockHeader;
use tari_core::U256;

pub type Difficulty = u64;

/// A simple sha3 proof of work. This is currently intended to be used for testing and perhaps Testnet until
/// Monero merge-mining is active.
///
/// The proof of work difficulty is given by `H256(header )` where Hnnn is the sha3 digest of length
/// `nnn` bits.
pub fn sha3_difficulty(header: &BlockHeader) -> Result<Difficulty, String> {
    Ok(sha3_difficulty_with_hash(header)?.0)
}

pub fn sha3_hash(header: &BlockHeader) -> Result<Vec<u8>, String> {
    use std::convert::TryFrom;
    use tari_core::proof_of_work::ProofOfWork;

    // Not stressing about efficiency here as it will change soon
    let pow = ProofOfWork::try_from(header.pow.clone().ok_or("Empty header.pow")?)?;
    let timestamp = header.timestamp.as_ref().ok_or("Empty header.timestamp")?;
    Ok(Sha3_256::new()
        .chain(header.version.to_le_bytes())
        .chain(header.height.to_le_bytes())
        .chain(&header.prev_hash)
        .chain((timestamp.seconds as u64).to_le_bytes())
        .chain(&header.output_mr)
        .chain(&header.range_proof_mr)
        .chain(&header.kernel_mr)
        .chain(&header.total_kernel_offset)
        .chain(header.nonce.to_le_bytes())
        .chain(pow.to_bytes())
        .finalize()
        .to_vec())
}

fn sha3_difficulty_with_hash(header: &BlockHeader) -> Result<(Difficulty, Vec<u8>), String> {
    let hash = sha3_hash(header)?;
    let hash = Sha3_256::digest(&hash);
    let difficulty = big_endian_difficulty(&hash);
    Ok((difficulty, hash.to_vec()))
}

/// This will provide the difficulty of the hash assuming the hash is big_endian
fn big_endian_difficulty(hash: &[u8]) -> Difficulty {
    let scalar = U256::from_big_endian(hash); // Big endian so the hash has leading zeroes
    let result = U256::MAX / scalar;
    result.low_u64().into()
}

#[cfg(test)]
pub mod test {
    use super::*;
    use chrono::{DateTime, NaiveDate, Utc};
    use tari_core::blocks::BlockHeader as CoreBlockHeader;
    use tari_crypto::tari_utilities::hex::Hex;

    pub fn get_header() -> BlockHeader {
        let mut header = CoreBlockHeader::new(0);
        header.timestamp = DateTime::<Utc>::from_utc(NaiveDate::from_ymd(2000, 1, 1).and_hms(1, 1, 1), Utc).into();
        header.pow.pow_algo = tari_core::proof_of_work::PowAlgorithm::Sha3;
        header.into()
    }

    #[test]
    fn validate_max_target() {
        let mut header = get_header();
        header.nonce = 1;
        assert_eq!(sha3_difficulty(&header).unwrap(), 10);
    }

    #[test]
    fn difficulty_1000() {
        let mut header = get_header();
        header.nonce = 1_332;
        let (diff, hash) = sha3_difficulty_with_hash(&header).unwrap();
        assert_eq!(diff, 3_832);
        assert_eq!(
            hash.to_hex(),
            "00111a1b0aa98f1f431a582ae8c912054c53f3f36a967b3de51d152be20fc96c"
        );
    }
}
