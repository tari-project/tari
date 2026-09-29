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
    convert::TryFrom,
    fmt::{self, Display},
};

use borsh::{
    BorshDeserialize,
    BorshSerialize,
    io::{Error, ErrorKind},
};
use serde::{
    Deserialize,
    Deserializer,
    Serialize,
    de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor},
};

use crate::{
    bounded_serde::{Field, FieldSeed},
    checked_de::{read_bytes, read_checked_len},
};

/// A string that can only be a up to MAX length long
///
/// The bound is enforced by every constructor *and* by deserialization (see the hand written
/// `BorshDeserialize`/`Deserialize` implementations below), so `len() <= MAX` is a true invariant
/// even for values decoded from untrusted input.
///
/// # What is (and is not) guaranteed
///
/// The *only* guarantees are that the string is valid UTF-8 and that its length **in bytes** (not characters) is
/// at most `MAX`. There is no restriction on the character set and no validation of the content: a value named or
/// used as a URL, a name or a hash is not checked to be one. Values decoded from the network or the chain can
/// contain anything UTF-8 allows, including control characters, ANSI/terminal escape sequences, newlines and
/// bidirectional-override characters.
///
/// `Display` writes the raw string unchanged (so conversions to gRPC/proto and other data formats see the real
/// value). Callers must therefore escape it before writing it to a log or a terminal, for example with
/// `{:?}` or [`str::escape_debug`] via [`MaxSizeString::as_str`], otherwise untrusted input can forge log lines or
/// drive the terminal.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, BorshSerialize)]
pub struct MaxSizeString<const MAX: usize> {
    string: String,
}

/// The serde shape of [`MaxSizeString`], which must match what `#[derive(Serialize)]` produces.
const SERDE_NAME: &str = "MaxSizeString";
const SERDE_VALUE_FIELD: &str = "string";
const SERDE_FIELDS: &[&str] = &[SERDE_VALUE_FIELD];

impl<'de, const MAX: usize> Deserialize<'de> for MaxSizeString<MAX> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // The byte length is checked before the string is copied into the value.
        deserializer.deserialize_struct(SERDE_NAME, SERDE_FIELDS, MaxSizeStringVisitor::<MAX>)
    }
}

struct MaxSizeStringVisitor<const MAX: usize>;

impl<'de, const MAX: usize> Visitor<'de> for MaxSizeStringVisitor<MAX> {
    type Value = MaxSizeString<MAX>;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("struct MaxSizeString")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let string = seq
            .next_element_seed(BoundedStringSeed::<MAX>)?
            .ok_or_else(|| de::Error::invalid_length(0, &"struct MaxSizeString with 1 element"))?;
        Ok(MaxSizeString { string })
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut string = None;
        while let Some(field) = map.next_key_seed(FieldSeed {
            value: SERDE_VALUE_FIELD,
            marker: None,
        })? {
            match field {
                Field::Value => {
                    if string.is_some() {
                        return Err(de::Error::duplicate_field(SERDE_VALUE_FIELD));
                    }
                    string = Some(map.next_value_seed(BoundedStringSeed::<MAX>)?);
                },
                Field::Marker | Field::Ignore => {
                    map.next_value::<de::IgnoredAny>()?;
                },
            }
        }
        let string = string.ok_or_else(|| de::Error::missing_field(SERDE_VALUE_FIELD))?;
        Ok(MaxSizeString { string })
    }
}

/// Decodes the `string` field as a UTF-8 string of at most `MAX` bytes.
struct BoundedStringSeed<const MAX: usize>;

impl<const MAX: usize> BoundedStringSeed<MAX> {
    fn check_len<E: de::Error>(len: usize) -> Result<(), E> {
        if len > MAX {
            return Err(E::custom(MaxSizeStringLengthError {
                expected: MAX,
                actual: len,
            }));
        }
        Ok(())
    }
}

impl<'de, const MAX: usize> DeserializeSeed<'de> for BoundedStringSeed<MAX> {
    type Value = String;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_string(self)
    }
}

impl<'de, const MAX: usize> Visitor<'de> for BoundedStringSeed<MAX> {
    type Value = String;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "a string of at most {MAX} bytes")
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
        Self::check_len(v.len())?;
        Ok(v.to_owned())
    }

    fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> {
        Self::check_len(v.len())?;
        Ok(v)
    }

    // `String`'s own `Deserialize` also accepts UTF-8 bytes; keep accepting them
    fn visit_bytes<E: de::Error>(self, v: &[u8]) -> Result<Self::Value, E> {
        Self::check_len(v.len())?;
        let s = std::str::from_utf8(v).map_err(|_| E::invalid_value(de::Unexpected::Bytes(v), &self))?;
        Ok(s.to_owned())
    }

    fn visit_byte_buf<E: de::Error>(self, v: Vec<u8>) -> Result<Self::Value, E> {
        Self::check_len(v.len())?;
        String::from_utf8(v).map_err(|e| E::invalid_value(de::Unexpected::Bytes(&e.into_bytes()), &self))
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

/// Writes the raw, unescaped string. See the type documentation: escape it before logging it or printing it to a
/// terminal.
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

        #[test]
        fn serde_json_rejects_an_oversized_string() {
            let payload = format!(r#"{{"string":"{}"}}"#, "a".repeat(10 * MAX));
            let err = serde_json::from_str::<Str>(&payload).unwrap_err();
            assert!(err.to_string().contains("Invalid String length"), "{}", err);
            // The bound is on bytes, not characters
            let payload = format!(r#"{{"string":"{}"}}"#, "🚀".repeat(MAX / 4 + 1));
            let err = serde_json::from_str::<Str>(&payload).unwrap_err();
            assert!(err.to_string().contains("Invalid String length"), "{}", err);
        }

        #[test]
        fn bincode_rejects_an_oversized_string() {
            let mut payload = u64::try_from(MAX + 1).unwrap().to_le_bytes().to_vec();
            payload.extend("a".repeat(MAX + 1).as_bytes());
            let err = bincode::deserialize::<Str>(&payload).unwrap_err();
            assert!(err.to_string().contains("Invalid String length"), "{}", err);
        }

        #[test]
        fn serde_keeps_the_derived_field_handling() {
            let s = serde_json::from_str::<Str>(r#"{"other":1,"string":"abc"}"#).unwrap();
            assert_eq!(s.as_str(), "abc");
            let err = serde_json::from_str::<Str>(r#"{"string":"a","string":"b"}"#).unwrap_err();
            assert!(err.to_string().contains("duplicate field `string`"), "{}", err);
            let err = serde_json::from_str::<Str>(r#"{}"#).unwrap_err();
            assert!(err.to_string().contains("missing field `string`"), "{}", err);
        }
    }
}
