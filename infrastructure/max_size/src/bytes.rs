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
use serde::{Deserialize, Deserializer, Serialize};
use tari_utilities::{
    ByteArray,
    ByteArrayError,
    hex::{HexError, from_hex, to_hex},
};

use crate::checked_de::{read_bytes, read_checked_len};

/// A byte vector that can be at most `MAX` bytes long.
///
/// The bound is enforced by every constructor *and* by deserialization (see the hand written
/// `BorshDeserialize`/`Deserialize` implementations below), so `len() <= MAX` is a true invariant
/// even for values decoded from untrusted input.
#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, BorshSerialize)]
pub struct MaxSizeBytes<const MAX: usize> {
    inner: Vec<u8>,
}

/// Mirror of [`MaxSizeBytes`] used only to decode the wire format before the bound is checked.
/// It must keep the exact same (serde) shape as `MaxSizeBytes` so that the serialized
/// representation is unchanged.
#[derive(Deserialize)]
#[serde(rename = "MaxSizeBytes")]
struct MaxSizeBytesShadow {
    inner: Vec<u8>,
}

impl<'de, const MAX: usize> Deserialize<'de> for MaxSizeBytes<MAX> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let shadow = MaxSizeBytesShadow::deserialize(deserializer)?;
        Self::try_from(shadow.inner).map_err(serde::de::Error::custom)
    }
}

impl<const MAX: usize> BorshDeserialize for MaxSizeBytes<MAX> {
    fn deserialize_reader<R: borsh::io::Read>(reader: &mut R) -> borsh::io::Result<Self> {
        // The length is validated before any data is read, so an oversized payload is rejected up
        // front instead of being decoded and silently accepted.
        let len = read_checked_len(reader, MAX, "MaxSizeBytes")?;
        Ok(Self {
            inner: read_bytes(reader, len)?,
        })
    }
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

    pub fn empty() -> Self {
        Self { inner: Vec::new() }
    }

    pub fn from_bytes_truncate<T: AsRef<[u8]>>(bytes: T) -> Self {
        let mut b = bytes.as_ref().to_vec();
        b.truncate(cmp::min(b.len(), MAX));
        Self { inner: b }
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

#[cfg(test)]
mod tests {
    use super::*;

    const MAX: usize = 10;
    type Bytes = MaxSizeBytes<MAX>;

    #[test]
    fn borsh_round_trips_a_valid_value() {
        let bytes = Bytes::try_from(vec![1u8, 2, 3]).unwrap();
        let encoded = borsh::to_vec(&bytes).unwrap();
        assert_eq!(Bytes::try_from_slice(&encoded).unwrap(), bytes);

        // The encoding is unchanged from the derived implementation, i.e. it is the plain borsh
        // encoding of the inner `Vec<u8>`
        assert_eq!(encoded, borsh::to_vec(&vec![1u8, 2, 3]).unwrap());
    }

    #[test]
    fn borsh_accepts_exactly_max_and_rejects_max_plus_one() {
        let at_max = borsh::to_vec(&vec![0u8; MAX]).unwrap();
        assert_eq!(Bytes::try_from_slice(&at_max).unwrap().len(), MAX);

        let over_max = borsh::to_vec(&vec![0u8; MAX + 1]).unwrap();
        let err = Bytes::try_from_slice(&over_max).unwrap_err();
        assert!(err.to_string().contains("exceeds the maximum size"), "{}", err);
    }

    #[test]
    fn borsh_rejects_an_oversized_length_prefix_without_reading_the_body() {
        // A length prefix of 4 GiB and no data at all: this must fail on the length check alone
        let payload = u32::MAX.to_le_bytes();
        let err = Bytes::try_from_slice(&payload).unwrap_err();
        assert!(err.to_string().contains("exceeds the maximum size"), "{}", err);
    }

    #[test]
    fn borsh_rejects_a_truncated_body() {
        let mut payload = borsh::to_vec(&vec![0u8; MAX]).unwrap();
        payload.pop();
        assert!(Bytes::try_from_slice(&payload).is_err());
    }

    #[test]
    fn serde_round_trips_a_valid_value_without_changing_the_representation() {
        let bytes = Bytes::try_from(vec![1u8, 2, 3]).unwrap();
        let json = serde_json::to_string(&bytes).unwrap();
        assert_eq!(json, r#"{"inner":[1,2,3]}"#);
        assert_eq!(serde_json::from_str::<Bytes>(&json).unwrap(), bytes);
    }

    #[test]
    fn bincode_round_trips_a_valid_value_and_rejects_max_plus_one() {
        // bincode is the compact (non human readable) serde format used for the on-disk chain
        // storage, so the encoding must be unchanged
        let bytes = Bytes::try_from(vec![1u8, 2, 3]).unwrap();
        let encoded = bincode::serialize(&bytes).unwrap();
        assert_eq!(encoded, bincode::serialize(&vec![1u8, 2, 3]).unwrap());
        assert_eq!(bincode::deserialize::<Bytes>(&encoded).unwrap(), bytes);

        let at_max = bincode::serialize(&vec![0u8; MAX]).unwrap();
        assert_eq!(bincode::deserialize::<Bytes>(&at_max).unwrap().len(), MAX);

        let over_max = bincode::serialize(&vec![0u8; MAX + 1]).unwrap();
        let err = bincode::deserialize::<Bytes>(&over_max).unwrap_err();
        assert!(err.to_string().contains("Invalid Bytes length"), "{}", err);
    }

    #[test]
    fn serde_accepts_exactly_max_and_rejects_max_plus_one() {
        let at_max = serde_json::to_string(&vec![0u8; MAX]).unwrap();
        let at_max = format!(r#"{{"inner":{at_max}}}"#);
        assert_eq!(serde_json::from_str::<Bytes>(&at_max).unwrap().len(), MAX);

        let over_max = serde_json::to_string(&vec![0u8; MAX + 1]).unwrap();
        let over_max = format!(r#"{{"inner":{over_max}}}"#);
        let err = serde_json::from_str::<Bytes>(&over_max).unwrap_err();
        assert!(err.to_string().contains("Invalid Bytes length"), "{}", err);
    }
}
