// Copyright 2022 The Tari Project
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
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE
//
// Portions of this file were originally copyrighted (c) 2018 The Grin Developers, issued under the Apache License,
// Version 2.0, available at http://www.apache.org/licenses/LICENSE-2.0.

use std::{
    cmp,
    convert::TryFrom,
    fmt::Display,
    ops::{Deref, DerefMut},
};

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use tari_utilities::{
    hex::{from_hex, to_hex, HexError},
    ByteArray,
    ByteArrayError,
};

#[derive(
    Debug,
    Clone,
    Hash,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Default,
    Deserialize,
    Serialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct MaxSizeBytes<const MAX: usize> {
    inner: Vec<u8>,
}

impl<const MAX: usize> MaxSizeBytes<MAX> {
    pub fn into_vec(self) -> Vec<u8> {
        self.inner
    }

    pub fn from_bytes_checked<T: AsRef<[u8]>>(bytes: T) -> Option<Self> {
        let b = bytes.as_ref();
        if b.len() > MAX {
            None
        } else {
            Some(Self { inner: b.to_vec() })
        }
    }

    pub fn from_bytes_truncate<T: AsRef<[u8]>>(bytes: T) -> Self {
        let b = bytes.as_ref();
        let len = cmp::min(b.len(), MAX);
        Self {
            inner: b[..len].to_vec(),
        }
    }

    pub fn max_size(&self) -> usize {
        MAX
    }
}

impl<const MAX: usize> From<MaxSizeBytes<MAX>> for Vec<u8> {
    fn from(value: MaxSizeBytes<MAX>) -> Self {
        value.inner
    }
}

impl<const MAX: usize> TryFrom<Vec<u8>> for MaxSizeBytes<MAX> {
    type Error = MaxSizeBytesError;

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        if value.len() > MAX {
            Err(MaxSizeBytesError::MaxSizeBytesLengthError {
                expected: MAX,
                actual: value.len(),
            })
        } else {
            Ok(MaxSizeBytes { inner: value })
        }
    }
}

impl<const MAX: usize> TryFrom<&str> for MaxSizeBytes<MAX> {
    type Error = MaxSizeBytesError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::try_from(from_hex(value)?)
    }
}

impl<const MAX: usize> TryFrom<String> for MaxSizeBytes<MAX> {
    type Error = MaxSizeBytesError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_from(from_hex(value.as_str())?)
    }
}

impl<const MAX: usize> AsRef<[u8]> for MaxSizeBytes<MAX> {
    fn as_ref(&self) -> &[u8] {
        &self.inner
    }
}

impl<const MAX: usize> Deref for MaxSizeBytes<MAX> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<const MAX: usize> DerefMut for MaxSizeBytes<MAX> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<const MAX: usize> ByteArray for MaxSizeBytes<MAX> {
    /// Try and convert the given byte array to a MaxSizeBytes. Any failures (incorrect array length,
    /// implementation-specific checks, etc) return a [ByteArrayError](enum.ByteArrayError.html).
    fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, ByteArrayError> {
        Self::from_bytes_checked(bytes).ok_or(ByteArrayError::ConversionError {
            reason: "Invalid byte length".to_string(),
        })
    }

    /// Return the data as a byte array
    fn as_bytes(&self) -> &[u8] {
        self.inner.as_ref()
    }
}

impl<const MAX: usize> Display for MaxSizeBytes<MAX> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", to_hex(&self.inner))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MaxSizeBytesError {
    #[error("Invalid Bytes length: expected {expected}, got {actual}")]
    MaxSizeBytesLengthError { expected: usize, actual: usize },
    #[error("Conversion error: {0}")]
    HexError(String),
}

impl From<HexError> for MaxSizeBytesError {
    fn from(err: HexError) -> Self {
        MaxSizeBytesError::HexError(err.to_string())
    }
}
