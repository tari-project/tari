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

use std::{io, io::Write};

use borsh::{BorshDeserialize, BorshSerialize};
use tari_common_types::types::FixedHash;

// This is to avoid adding borsh as a dependency in common types (and therefore every application).
// Either this becomes the standard Hash type for the dan layer, or Borsh becomes the standard serialization format for
// Tari.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Hash(FixedHash);

impl Hash {
    pub fn into_inner(self) -> FixedHash {
        self.0
    }
}

impl From<FixedHash> for Hash {
    fn from(hash: FixedHash) -> Self {
        Self(hash)
    }
}

impl BorshSerialize for Hash {
    fn serialize<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        (*self.0).serialize(writer)
    }
}

impl BorshDeserialize for Hash {
    fn deserialize(buf: &mut &[u8]) -> io::Result<Self> {
        let hash = <[u8; 32] as BorshDeserialize>::deserialize(buf)?;
        Ok(Hash(hash.into()))
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
