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

use std::{convert::TryFrom, fmt::Display};

use borsh::{
    BorshDeserialize,
    BorshSerialize,
    io::{Error, ErrorKind},
};
use serde::{Deserialize, Deserializer, Serialize};

use crate::checked_de::{read_bytes, read_checked_len};

/// A string that can only be a up to MAX length long
///
/// The bound is enforced by every constructor *and* by deserialization (see the hand written
/// `BorshDeserialize`/`Deserialize` implementations below), so `len() <= MAX` is a true invariant
/// even for values decoded from untrusted input.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, BorshSerialize)]
pub struct MaxSizeString<const MAX: usize> {
    string: String,
}

/// Mirror of [`MaxSizeString`] used only to decode the wire format before the bound is checked.
/// It must keep the exact same (serde) shape as `MaxSizeString` so that the serialized
/// representation is unchanged.
#[derive(Deserialize)]
#[serde(rename = "MaxSizeString")]
struct MaxSizeStringShadow {
    string: String,
}

impl<'de, const MAX: usize> Deserialize<'de> for MaxSizeString<MAX> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let shadow = MaxSizeStringShadow::deserialize(deserializer)?;
        Self::try_from(shadow.string).map_err(serde::de::Error::custom)
    }
}

impl<const MAX: usize> BorshDeserialize for MaxSizeString<MAX> {
    fn deserialize_reader<R: borsh::io::Read>(reader: &mut R) -> borsh::io::Result<Self> {
        // The length is validated before any data is read, so an oversized payload is rejected up
        // front instead of being decoded and silently accepted.
        let len = read_checked_len(reader, MAX, "MaxSizeString")?;
        let bytes = read_bytes(reader, len)?;
        let string = String::from_utf8(bytes).map_err(|e| Error::new(ErrorKind::InvalidData, e.to_string()))?;
        Ok(Self { string })
    }
}

impl<const MAX: usize> MaxSizeString<MAX> {
    pub fn from_str_checked(s: &str) -> Option<Self> {
        if s.len() > MAX {
            return None;
        }
        Some(Self { string: s.to_string() })
    }

    pub fn from_utf8_bytes_checked<T: AsRef<[u8]>>(bytes: T) -> Option<Self> {
        let b = bytes.as_ref();
        if b.len() > MAX {
            return None;
        }

        let s = String::from_utf8(b.to_vec()).ok()?;
        Some(Self { string: s })
    }

    pub fn len(&self) -> usize {
        self.string.len()
    }

    pub fn is_empty(&self) -> bool {
        self.string.is_empty()
    }

    pub fn as_str(&self) -> &str {
        &self.string
    }

    pub fn into_string(self) -> String {
        self.string
    }
}

impl<const MAX: usize> TryFrom<String> for MaxSizeString<MAX> {
    type Error = MaxSizeStringLengthError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.len() > MAX {
            return Err(MaxSizeStringLengthError {
                actual: value.len(),
                expected: MAX,
            });
        }
        Ok(Self { string: value })
    }
}

impl<const MAX: usize> TryFrom<&str> for MaxSizeString<MAX> {
    type Error = MaxSizeStringLengthError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        if value.len() > MAX {
            return Err(MaxSizeStringLengthError {
                actual: value.len(),
                expected: MAX,
            });
        }
        Ok(Self {
            string: value.to_string(),
        })
    }
}

impl<const MAX: usize> AsRef<[u8]> for MaxSizeString<MAX> {
    fn as_ref(&self) -> &[u8] {
        self.string.as_ref()
    }
}

impl<const MAX: usize> Display for MaxSizeString<MAX> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.string)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Invalid String length: expected {expected}, got {actual}")]
pub struct MaxSizeStringLengthError {
    expected: usize,
    actual: usize,
}

#[cfg(test)]
mod tests {
    mod from_str_checked {
        use crate::MaxSizeString;
        #[test]
        fn it_returns_none_if_size_exceeded() {
            let s = MaxSizeString::<10>::from_str_checked("12345678901234567890");
            assert_eq!(s, None);
        }

        #[test]
        fn it_returns_some_if_size_in_bounds() {
            let s = MaxSizeString::<0>::from_str_checked("").unwrap();
            assert_eq!(s.as_str(), "");
            assert_eq!(s.len(), 0);

            let s = MaxSizeString::<10>::from_str_checked("1234567890").unwrap();
            assert_eq!(s.as_str(), "1234567890");
            assert_eq!(s.len(), 10);

            let s = MaxSizeString::<10>::from_str_checked("1234").unwrap();
            assert_eq!(s.as_str(), "1234");
            assert_eq!(s.len(), 4);

            let s = MaxSizeString::<8>::from_str_checked("🚀🚀").unwrap();
            assert_eq!(s.as_str(), "🚀🚀");
            // 8 here because an emoji char take 4 bytes each
            assert_eq!(s.len(), 8);
        }
    }

    mod from_utf8_bytes_checked {
        use crate::MaxSizeString;
        #[test]
        fn it_returns_none_if_size_exceeded() {
            let s = MaxSizeString::<10>::from_utf8_bytes_checked([0u8; 11]);
            assert_eq!(s, None);
        }

        #[test]
        fn it_returns_some_if_size_in_bounds() {
            let s = MaxSizeString::<12>::from_utf8_bytes_checked("💡🧭🛖".as_bytes()).unwrap();
            assert_eq!(s.as_str(), "💡🧭🛖");
            assert_eq!(s.len(), 12);
        }

        #[test]
        fn it_returns_none_if_invalid_utf8() {
            let s = MaxSizeString::<10>::from_utf8_bytes_checked([255u8; 10]);
            assert_eq!(s, None);
        }
    }

    mod deserialization {
        use borsh::BorshDeserialize;

        use crate::MaxSizeString;

        const MAX: usize = 10;
        type Str = MaxSizeString<MAX>;

        #[test]
        fn borsh_round_trips_a_valid_value() {
            let s = Str::try_from("abc").unwrap();
            let encoded = borsh::to_vec(&s).unwrap();
            assert_eq!(Str::try_from_slice(&encoded).unwrap(), s);

            // The encoding is unchanged from the derived implementation, i.e. it is the plain
            // borsh encoding of the inner `String`
            assert_eq!(encoded, borsh::to_vec(&"abc".to_string()).unwrap());
        }

        #[test]
        fn borsh_accepts_exactly_max_and_rejects_max_plus_one() {
            let at_max = borsh::to_vec(&"a".repeat(MAX)).unwrap();
            assert_eq!(Str::try_from_slice(&at_max).unwrap().len(), MAX);

            let over_max = borsh::to_vec(&"a".repeat(MAX + 1)).unwrap();
            let err = Str::try_from_slice(&over_max).unwrap_err();
            assert!(err.to_string().contains("exceeds the maximum size"), "{}", err);
        }

        #[test]
        fn borsh_rejects_an_oversized_length_prefix_without_reading_the_body() {
            // A length prefix of 4 GiB and no data at all: this must fail on the length check
            // alone
            let payload = u32::MAX.to_le_bytes();
            let err = Str::try_from_slice(&payload).unwrap_err();
            assert!(err.to_string().contains("exceeds the maximum size"), "{}", err);
        }

        #[test]
        fn borsh_rejects_invalid_utf8() {
            let payload = borsh::to_vec(&vec![255u8; MAX]).unwrap();
            assert!(Str::try_from_slice(&payload).is_err());
        }

        #[test]
        fn borsh_rejects_a_truncated_body() {
            let mut payload = borsh::to_vec(&"a".repeat(MAX)).unwrap();
            payload.pop();
            assert!(Str::try_from_slice(&payload).is_err());
        }

        #[test]
        fn serde_round_trips_a_valid_value_without_changing_the_representation() {
            let s = Str::try_from("abc").unwrap();
            let json = serde_json::to_string(&s).unwrap();
            assert_eq!(json, r#"{"string":"abc"}"#);
            assert_eq!(serde_json::from_str::<Str>(&json).unwrap(), s);
        }

        #[test]
        fn bincode_round_trips_a_valid_value_and_rejects_max_plus_one() {
            // bincode is the compact (non human readable) serde format used for the on-disk chain
            // storage, so the encoding must be unchanged
            let s = Str::try_from("abc").unwrap();
            let encoded = bincode::serialize(&s).unwrap();
            assert_eq!(encoded, bincode::serialize(&"abc".to_string()).unwrap());
            assert_eq!(bincode::deserialize::<Str>(&encoded).unwrap(), s);

            let at_max = bincode::serialize(&"a".repeat(MAX)).unwrap();
            assert_eq!(bincode::deserialize::<Str>(&at_max).unwrap().len(), MAX);

            let over_max = bincode::serialize(&"a".repeat(MAX + 1)).unwrap();
            let err = bincode::deserialize::<Str>(&over_max).unwrap_err();
            assert!(err.to_string().contains("Invalid String length"), "{}", err);
        }

        #[test]
        fn serde_accepts_exactly_max_and_rejects_max_plus_one() {
            let at_max = format!(r#"{{"string":"{}"}}"#, "a".repeat(MAX));
            assert_eq!(serde_json::from_str::<Str>(&at_max).unwrap().len(), MAX);

            let over_max = format!(r#"{{"string":"{}"}}"#, "a".repeat(MAX + 1));
            let err = serde_json::from_str::<Str>(&over_max).unwrap_err();
            assert!(err.to_string().contains("Invalid String length"), "{}", err);
        }
    }
}
