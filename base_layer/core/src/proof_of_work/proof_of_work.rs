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

use crate::proof_of_work::PowAlgorithm;
use bytes::BufMut;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Error, Formatter};
use tari_crypto::tari_utilities::hex::Hex;

pub trait AchievedDifficulty {}

/// The proof of work data structure that is included in the block header. There's some non-Rustlike redundancy here
/// to make serialization more straightforward
#[allow(deprecated)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProofOfWork {
    /// The algorithm used to mine this block
    pub pow_algo: PowAlgorithm,
    /// Supplemental proof of work data. For example for Sha3, this would be empty (only the block header is
    /// required), but for Monero merge mining we need the Monero block header and RandomX seed hash.
    pub pow_data: Vec<u8>,
}

impl Default for ProofOfWork {
    #[allow(deprecated)]
    fn default() -> Self {
        Self {
            pow_algo: PowAlgorithm::Sha3,
            pow_data: vec![],
        }
    }
}

impl ProofOfWork {
    /// Create a new `ProofOfWork` instance. Except for the algorithm used, the fields are uninitialized.
    pub fn new(pow_algo: PowAlgorithm) -> Self {
        Self {
            pow_algo,
            ..Default::default()
        }
    }

    /// Serialises the ProofOfWork instance into a byte string. Useful for feeding the PoW into a hash function.
    #[allow(deprecated)]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(256);
        buf.put_u8(self.pow_algo as u8);
        buf.put_slice(&self.pow_data);
        buf
    }
}

impl Display for PowAlgorithm {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), Error> {
        let algo = match self {
            PowAlgorithm::Monero => "Monero",
            PowAlgorithm::Blake => "Blake",
            PowAlgorithm::Sha3 => "Sha3",
        };
        fmt.write_str(&algo.to_string())
    }
}

impl Display for ProofOfWork {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), Error> {
        writeln!(fmt, "Mining algorithm: {}", self.pow_algo)?;
        writeln!(fmt, "Pow data: {}", self.pow_data.to_hex())?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use crate::proof_of_work::proof_of_work::{PowAlgorithm, ProofOfWork};

    #[test]
    fn display() {
        let pow = ProofOfWork::default();
        assert_eq!(&format!("{}", pow), "Mining algorithm: Sha3\nPow data: \n");
    }

    #[test]
    fn to_bytes() {
        let mut pow = ProofOfWork::default();
        pow.pow_algo = PowAlgorithm::Sha3;
        assert_eq!(pow.to_bytes(), vec![2]);
    }
}
