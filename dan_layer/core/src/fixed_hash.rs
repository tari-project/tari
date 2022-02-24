//  Copyright 2022, The Tari Project
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

use std::{convert::TryFrom, ops::Deref};

use digest::consts::U32;
use tari_utilities::hex::{Hex, HexError};

const ZERO_HASH: [u8; FixedHash::byte_size()] = [0u8; FixedHash::byte_size()];

#[derive(thiserror::Error, Debug)]
#[error("Invalid size")]
pub struct FixedHashSizeError;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default, Hash)]
pub struct FixedHash([u8; FixedHash::byte_size()]);

impl FixedHash {
    pub const fn byte_size() -> usize {
        32
    }

    pub fn zero() -> Self {
        Self(ZERO_HASH)
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }
}

impl From<[u8; FixedHash::byte_size()]> for FixedHash {
    fn from(hash: [u8; FixedHash::byte_size()]) -> Self {
        Self(hash)
    }
}

impl TryFrom<Vec<u8>> for FixedHash {
    type Error = FixedHashSizeError;

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        TryFrom::try_from(value.as_slice())
    }
}

impl TryFrom<&[u8]> for FixedHash {
    type Error = FixedHashSizeError;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        if bytes.len() != FixedHash::byte_size() {
            return Err(FixedHashSizeError);
        }

        let mut buf = [0u8; FixedHash::byte_size()];
        buf.copy_from_slice(bytes);
        Ok(Self(buf))
    }
}

impl From<digest::generic_array::GenericArray<u8, U32>> for FixedHash {
    fn from(hash: digest::generic_array::GenericArray<u8, U32>) -> Self {
        Self(hash.into())
    }
}

impl PartialEq<[u8]> for FixedHash {
    fn eq(&self, other: &[u8]) -> bool {
        self.0[..].eq(other)
    }
}

impl Hex for FixedHash {
    fn from_hex(hex: &str) -> Result<Self, HexError>
    where Self: Sized {
        let hash = <[u8; FixedHash::byte_size()] as Hex>::from_hex(hex)?;
        Ok(Self(hash))
    }

    fn to_hex(&self) -> String {
        self.0.to_hex()
    }
}

impl Deref for FixedHash {
    type Target = [u8; FixedHash::byte_size()];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
