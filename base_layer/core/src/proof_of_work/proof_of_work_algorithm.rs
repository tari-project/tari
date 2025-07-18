// Copyright 2020. The Tari Project
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
use std::{
    convert::TryFrom,
    fmt::{Display, Formatter},
    str::FromStr,
};

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

/// Indicates the algorithm used to mine a block
#[repr(u8)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Hash, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
pub enum PowAlgorithm {
    RandomXM = 0,
    Sha3x = 1,
    RandomXT = 2,
}

impl PowAlgorithm {
    /// Returns true if the PoW algorithm is merged mined monero RandomX
    pub fn is_merged_mined_randomx(&self) -> bool {
        matches!(self, Self::RandomXM)
    }

    /// Returns true if the PoW algorithm is solo tari RandomX
    pub fn is_tari_randomx(&self) -> bool {
        matches!(self, Self::RandomXT)
    }

    /// Returns true if the PoW algorithm is Sha3
    pub fn is_sha3(&self) -> bool {
        matches!(self, Self::Sha3x)
    }

    /// A convenience functions that returns the PoW algorithm as a u64
    pub fn as_u64(&self) -> u64 {
        *self as u64
    }
}

impl TryFrom<u64> for PowAlgorithm {
    type Error = String;

    fn try_from(v: u64) -> Result<Self, Self::Error> {
        match v {
            0 => Ok(PowAlgorithm::RandomXM),
            1 => Ok(PowAlgorithm::Sha3x),
            2 => Ok(PowAlgorithm::RandomXT),
            _ => Err("Invalid PoWAlgorithm".into()),
        }
    }
}

impl FromStr for PowAlgorithm {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s_trimmed = s.replace("\"", "").replace("\'", "").replace(" ", "").to_uppercase();
        match s_trimmed.as_str() {
            "RANDOMXM" | "RANDOM_XM" | "MONERO_RANDOM_X" | "RANDOMX" | "RANDOM_X" | "RANDOMXMONERO" => {
                Ok(Self::RandomXM)
            },
            "SHA" | "SHA3" | "SHA3X" => Ok(Self::Sha3x),
            "RANDOMXT" | "RANDOM_XT" | "TARI_RANDOM_X" | "RANDOMXTARI" => Ok(Self::RandomXT),
            _ => Err(anyhow::Error::msg(format!("Unknown pow algorithm type: {}", s))),
        }
    }
}

impl Display for PowAlgorithm {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> std::fmt::Result {
        let algo = match self {
            PowAlgorithm::RandomXM => "RandomXMonero",
            PowAlgorithm::Sha3x => "Sha3",
            PowAlgorithm::RandomXT => "RandomXTari",
        };
        fmt.write_str(algo)
    }
}

#[cfg(test)]
mod tests {
    use serde_json;

    use super::*;

    #[test]
    fn test_pow_algorithm_from_str_variants() {
        // Test valid variants for RandomXM
        let randomxm_variants = vec![
            "RandomXM",
            "RandomX",
            "randomx",
            "random_x",
            "randomxm",
            "RANDOM_XM",
            "monero_random_x",
        ];
        for variant in randomxm_variants {
            let algo = PowAlgorithm::from_str(variant).expect("Failed to parse RandomXM variant");
            assert_eq!(algo, PowAlgorithm::RandomXM);
        }

        // Test valid variants for Sha3x
        let sha3x_variants = vec![
            "Sha3x",
            "\"Sha3x\"",
            "\'Sha3x\'",
            "Sha 3 x",
            "sha",
            "sha3",
            "SHA3",
            "sha3X",
            "Sha3X",
            "SHA3X",
        ];
        for variant in sha3x_variants {
            let algo = PowAlgorithm::from_str(variant).expect("Failed to parse Sha3x variant");
            assert_eq!(algo, PowAlgorithm::Sha3x);
        }

        // Test valid variants for RandomXT
        let randomxt_variants = vec!["RandomXT", "randomxt", "tari_random_x", "RANDOM_XT"];
        for variant in randomxt_variants {
            let algo = PowAlgorithm::from_str(variant).expect("Failed to parse RandomXT variant");
            assert_eq!(algo, PowAlgorithm::RandomXT);
        }
    }

    #[test]
    fn test_pow_algorithm_serialization() {
        for algo in [PowAlgorithm::Sha3x, PowAlgorithm::RandomXM, PowAlgorithm::RandomXT] {
            let serialized = serde_json::to_string(&algo).expect("Failed to serialize PowAlgorithm");
            let deserialized: PowAlgorithm =
                serde_json::from_str(&serialized).expect("Failed to deserialize PowAlgorithm");
            assert_eq!(deserialized, algo);
        }
    }
}
