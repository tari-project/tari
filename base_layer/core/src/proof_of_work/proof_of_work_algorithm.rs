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
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;

#[repr(u8)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Hash, Eq)]
pub enum PowAlgorithm {
    Monero = 0,
    Blake = 1,
    Sha3 = 2,
}

impl PowAlgorithm {
    pub fn is_monero(&self) -> bool {
        match self {
            Self::Monero => true,
            _ => false,
        }
    }

    pub fn is_sha3(&self) -> bool {
        match self {
            Self::Sha3 => true,
            _ => false,
        }
    }

    pub fn as_u64(&self) -> u64 {
        *self as u64
    }
}

impl TryFrom<u64> for PowAlgorithm {
    type Error = String;

    fn try_from(v: u64) -> Result<Self, Self::Error> {
        match v {
            0 => Ok(PowAlgorithm::Monero),
            2 => Ok(PowAlgorithm::Sha3),
            _ => Err("Invalid PoWAlgorithm".into()),
        }
    }
}
