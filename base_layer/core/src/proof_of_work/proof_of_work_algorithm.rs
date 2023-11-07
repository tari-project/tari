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
use thiserror::Error;

/// Indicates the algorithm used to mine a block
#[repr(u8)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Hash, Eq, BorshSerialize, BorshDeserialize)]
pub enum PowAlgorithm {
    RandomX = 0,
    Sha3x = 1,
}

impl PowAlgorithm {
    /// Returns true if the PoW algorithm is RandomX
    pub fn is_randomx(&self) -> bool {
        matches!(self, Self::RandomX)
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

/// Parse error for `PowAlgorithm`
#[derive(Debug, Error)]
pub enum PowAlgorithmParseError {
    #[error("unknown pow algorithm type {0}")]
    UnknownType(String),
}

impl TryFrom<u64> for PowAlgorithm {
    type Error = String;

    fn try_from(v: u64) -> Result<Self, Self::Error> {
        match v {
            0 => Ok(PowAlgorithm::RandomX),
            1 => Ok(PowAlgorithm::Sha3x),
            _ => Err("Invalid PoWAlgorithm".into()),
        }
    }
}

impl FromStr for PowAlgorithm {
    type Err = PowAlgorithmParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "RandomX" | "randomx" | "random_x" => Ok(Self::RandomX),
            "sha" | "sha3" | "SHA3" => Ok(Self::Sha3x),
            other => Err(PowAlgorithmParseError::UnknownType(other.into())),
        }
    }
}

impl Display for PowAlgorithm {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> std::fmt::Result {
        let algo = match self {
            PowAlgorithm::RandomX => "RandomX",
            PowAlgorithm::Sha3x => "Sha3",
        };
        fmt.write_str(algo)
    }
}
