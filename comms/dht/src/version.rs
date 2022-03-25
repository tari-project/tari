//  Copyright 2021, The Tari Project
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
    convert::{TryFrom, TryInto},
    fmt,
    fmt::{Display, Formatter},
    io::Write,
};

use crate::envelope::DhtMessageError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DhtProtocolVersion {
    V1 { minor: u32 },
    V2 { minor: u32 },
}

impl DhtProtocolVersion {
    pub fn latest() -> Self {
        DhtProtocolVersion::v2()
    }

    pub fn v1() -> Self {
        DhtProtocolVersion::V1 { minor: 0 }
    }

    pub fn v2() -> Self {
        DhtProtocolVersion::V2 { minor: 0 }
    }

    pub fn to_bytes(self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(4 * 2);
        buf.write_all(&self.as_major().to_le_bytes()).unwrap();
        buf.write_all(&self.as_minor().to_le_bytes()).unwrap();
        buf
    }

    pub fn as_major(&self) -> u32 {
        use DhtProtocolVersion::{V1, V2};
        match self {
            V1 { .. } => 1,
            V2 { .. } => 2,
        }
    }

    pub fn as_minor(&self) -> u32 {
        use DhtProtocolVersion::{V1, V2};
        match self {
            V1 { minor } => *minor,
            V2 { minor } => *minor,
        }
    }
}

impl Display for DhtProtocolVersion {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "v{}.{}", self.as_major(), self.as_minor())
    }
}

impl TryFrom<u32> for DhtProtocolVersion {
    type Error = DhtMessageError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        (value, 0).try_into()
    }
}

impl TryFrom<(u32, u32)> for DhtProtocolVersion {
    type Error = DhtMessageError;

    fn try_from((major, minor): (u32, u32)) -> Result<Self, Self::Error> {
        use DhtProtocolVersion::{V1, V2};
        match major {
            0..=1 => Ok(V1 { minor }),
            2 => Ok(V2 { minor }),
            n => Err(DhtMessageError::InvalidProtocolVersion(n)),
        }
    }
}
