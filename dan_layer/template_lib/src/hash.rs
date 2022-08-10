//  Copyright 2022. The Tari Project
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
    error::Error,
    fmt::{Display, Formatter},
    io,
    io::Write,
    ops::Deref,
};

use tari_template_abi::{Decode, Encode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
pub struct Hash([u8; 32]);

impl Hash {
    pub fn into_inner(self) -> [u8; 32] {
        self.0
    }

    pub fn from_hex(s: &str) -> Result<Self, HashParseError> {
        if s.len() != 64 {
            return Err(HashParseError);
        }

        let mut hash = [0u8; 32];
        for (i, h) in hash.iter_mut().enumerate() {
            *h = u8::from_str_radix(&s[2 * i..2 * (i + 1)], 16).map_err(|_| HashParseError)?;
        }
        Ok(Hash(hash))
    }
}

impl AsRef<[u8]> for Hash {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl From<[u8; 32]> for Hash {
    fn from(hash: [u8; 32]) -> Self {
        Self(hash)
    }
}

impl Deref for Hash {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Encode for Hash {
    fn serialize<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        self.0.serialize(writer)
    }
}

impl Decode for Hash {
    fn deserialize(buf: &mut &[u8]) -> io::Result<Self> {
        let hash = <[u8; 32] as Decode>::deserialize(buf)?;
        Ok(Hash(hash))
    }
}

impl Display for Hash {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        for x in self.0 {
            write!(f, "{:02x?}", x)?;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct HashParseError;

impl Error for HashParseError {}

impl Display for HashParseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Failed to parse hash")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_deserialize() {
        let hash = Hash::default();
        let mut buf = Vec::new();
        hash.serialize(&mut buf).unwrap();
        let hash2 = Hash::deserialize(&mut &buf[..]).unwrap();
        assert_eq!(hash, hash2);
    }
}
