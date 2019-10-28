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

use bigint::uint::U256;
use blake2::Blake2b;
use chrono::Duration;
use digest::Digest;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tari_core::{
    blocks::BlockHeader,
    proof_of_work::{Difficulty, PowError, ProofOfWork},
};
use tari_crypto::common::Blake256;
use tari_utilities::{ByteArray, ByteArrayError, Hashable};

const MAX_TARGET: U256 = U256::MAX;

/// A simple Blake2b-based proof of work. This is currently intended to be used for testing and perhaps Testnet until
/// Monero merge-mining is active.
///
/// The proof of work difficulty is given by `H256(H512(header || nonce))` where Hnnn is the Blake2b digest of length
/// `nnn` bits.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TestBlakePow;

impl TestBlakePow {
    /// A simple miner. It starts at nonce = 0 and iterates until it finds a header hash that meets the desired target
    // ToDo convert to future, with ability to break function. We need to be able to stop this if we receive a mined
    // block
    pub fn mine(target_difficulty: Difficulty, mut header: BlockHeader, stop_flag: Arc<AtomicBool>) -> BlockHeader {
        let mut rng = rand::OsRng::new().unwrap();
        let mut nonce: u64 = rng.next_u64();
        let start_nonce = nonce;
        // We're mining over here!
        while let Ok(d) = header.pow.achieved_difficulty(nonce, &header) {
            if d >= target_difficulty || stop_flag.load(Ordering::Relaxed) {
                header.nonce = nonce;
                break;
            }
            if nonce == std::u64::MAX {
                nonce = 0;
            } else {
                nonce += 1;
            }
            if nonce == start_nonce {
                header.timestamp = header.timestamp.checked_add_signed(Duration::milliseconds(1)).unwrap();
            }
        }
        header
    }
}

impl ProofOfWork for TestBlakePow {
    fn achieved_difficulty(&self, nonce: u64, header: &BlockHeader) -> Result<Difficulty, PowError> {
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

impl Default for TestBlakePow {
    fn default() -> Self {
        TestBlakePow
    }
}

impl ByteArray for TestBlakePow {
    fn from_bytes(_bytes: &[u8]) -> Result<Self, ByteArrayError> {
        Ok(TestBlakePow)
    }

    fn as_bytes(&self) -> &[u8] {
        &[]
    }
}

impl Hashable for TestBlakePow {
    fn hash(&self) -> Vec<u8> {
        vec![]
    }
}
