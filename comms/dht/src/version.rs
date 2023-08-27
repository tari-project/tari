//  Copyright 2021, The Taiji Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{
    convert::TryFrom,
    fmt,
    fmt::{Display, Formatter},
};

use serde::{Deserialize, Serialize};

use crate::envelope::DhtMessageError;

/// Versions for the DHT protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u32", into = "u32")]
pub enum DhtProtocolVersion {
    V1 = 1,
    V2,
}

impl DhtProtocolVersion {
    /// Returns the latest version
    pub fn latest() -> Self {
        DhtProtocolVersion::v2()
    }

    /// Returns v1 version
    pub fn v1() -> Self {
        DhtProtocolVersion::V1
    }

    /// Returns v2 version
    pub fn v2() -> Self {
        DhtProtocolVersion::V2
    }

    /// Returns the byte representation for the version
    pub fn as_bytes(self) -> [u8; 4] {
        self.as_major().to_le_bytes()
    }

    /// Returns the major version number
    pub fn as_major(&self) -> u32 {
        *self as u32
    }
}

impl Display for DhtProtocolVersion {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "v{}", self.as_major())
    }
}

impl TryFrom<u32> for DhtProtocolVersion {
    type Error = DhtMessageError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        if value == DhtProtocolVersion::V1 as u32 {
            Ok(DhtProtocolVersion::V1)
        } else if value == DhtProtocolVersion::V2 as u32 {
            Ok(DhtProtocolVersion::V2)
        } else {
            Err(DhtMessageError::InvalidProtocolVersion(value))
        }
    }
}

impl From<DhtProtocolVersion> for u32 {
    fn from(source: DhtProtocolVersion) -> Self {
        source.as_major()
    }
}
