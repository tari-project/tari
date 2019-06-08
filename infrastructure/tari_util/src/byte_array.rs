// Copyright 2019 The Tari Project
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

use crate::hex::{from_hex, to_hex, Hex, HexError};
use derive_error::Error;

#[derive(Debug, Error, PartialEq)]
pub enum ByteArrayError {
    // Could not create a ByteArray when converting from a different format
    #[error(msg_embedded, non_std, no_from)]
    ConversionError(String),
    // The input data was the incorrect length to perform the desired conversion
    IncorrectLength,
}

/// Many of the types in this crate are just large numbers (256 bit usually). This trait provides the common
/// functionality for  types  like secret keys, signatures, commitments etc. to be converted to and from byte arrays
/// and hexadecimal formats.
#[allow(clippy::ptr_arg)]
pub trait ByteArray: Sized {
    /// Return the type as a byte vector
    fn to_vec(&self) -> Vec<u8> {
        self.as_bytes().to_vec()
    }

    /// Try and convert the given byte vector to the implemented type. Any failures (incorrect string length etc)
    /// return a [KeyError](enum.KeyError.html) with an explanatory note.
    fn from_vec(v: &Vec<u8>) -> Result<Self, ByteArrayError> {
        Self::from_bytes(v.as_slice())
    }

    /// Try and convert the given byte array to the implemented type. Any failures (incorrect array length,
    /// implementation-specific checks, etc) return a [ByteArrayError](enum.ByteArrayError.html).
    fn from_bytes(bytes: &[u8]) -> Result<Self, ByteArrayError>;

    /// Return the type as a byte array
    fn as_bytes(&self) -> &[u8];
}

impl ByteArray for Vec<u8> {
    fn to_vec(&self) -> Vec<u8> {
        self.clone()
    }

    fn from_vec(v: &Vec<u8>) -> Result<Self, ByteArrayError> {
        Ok(v.clone())
    }

    fn from_bytes(bytes: &[u8]) -> Result<Self, ByteArrayError> {
        Ok(bytes.to_vec())
    }

    fn as_bytes(&self) -> &[u8] {
        Vec::as_slice(self)
    }
}

impl ByteArray for [u8; 32] {
    fn from_bytes(bytes: &[u8]) -> Result<Self, ByteArrayError> {
        if bytes.len() != 32 {
            return Err(ByteArrayError::IncorrectLength);
        }
        let mut a = [0u8; 32];
        a.copy_from_slice(bytes);
        Ok(a)
    }

    fn as_bytes(&self) -> &[u8] {
        self
    }
}

impl<T: ByteArray> Hex for T {
    type T = T;

    fn from_hex(hex: &str) -> Result<Self::T, HexError> {
        let v = from_hex(hex)?;
        Self::from_vec(&v).map_err(|_| HexError::HexConversionError)
    }

    fn to_hex(&self) -> String {
        to_hex(&self.to_vec())
    }
}
