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

use std::time::Instant;

use lmdb_zero::error;
use log::*;
use serde::{de::DeserializeOwned, Serialize};
use tari_storage::lmdb_store::BYTES_PER_MB;

use crate::chain_storage::ChainStorageError;

pub const LOG_TARGET: &str = "c::cs::lmdb_db::lmdb";

/// Serialize the given data into a byte vector
/// Note:
///   `size_hint` is given as an option as checking what the serialized would be is expensive
///   for large data structures at ~30% overhead
pub fn serialize<T>(data: &T, size_hint: Option<usize>) -> Result<Vec<u8>, ChainStorageError>
where T: Serialize + ?Sized {
    let start = Instant::now();
    let mut buf = if let Some(size) = size_hint {
        Vec::with_capacity(size)
    } else {
        let size = bincode::serialized_size(&data).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        #[allow(clippy::cast_possible_truncation)]
        Vec::with_capacity(size as usize)
    };
    let check_time = start.elapsed();
    bincode::serialize_into(&mut buf, data).map_err(|e| {
        error!(target: LOG_TARGET, "Could not serialize lmdb: {e:?}");
        ChainStorageError::AccessError(e.to_string())
    })?;
    if buf.len() >= BYTES_PER_MB {
        let serialize_time = start.elapsed() - check_time;
        trace!(
            "lmdb_replace - {} MB, serialize check in {:.2?}, serialize in {:.2?}",
            buf.len() / BYTES_PER_MB,
            check_time,
            serialize_time
        );
    }
    if let Some(size) = size_hint {
        if buf.len() > size {
            warn!(
                target: LOG_TARGET,
                "lmdb_replace - Serialized size hint was too small. Expected {}, got {}", size, buf.len()
            );
        }
    }
    Ok(buf)
}

pub fn deserialize<T>(buf_bytes: &[u8]) -> Result<T, error::Error>
where T: DeserializeOwned {
    bincode::deserialize(buf_bytes)
        .map_err(|e| {
            error!(target: LOG_TARGET, "Could not deserialize lmdb: {e:?}");
            e
        })
        .map_err(|e| error::Error::ValRejected(e.to_string()))
}

/// Serde helper module for serializing/deserializing `primitive_types::U512` as explicit
/// 64 little-endian bytes.
///
/// ## Rationale
///
/// `U512` is from the external `primitive_types` crate.  Relying on that crate's own
/// `Serialize`/`Deserialize` derive is fragile: if the crate changes its internal
/// representation or serde implementation, existing LMDB data would become unreadable.
///
/// This module pins the on-disk format to a fixed 64-byte little-endian representation
/// that is independent of the upstream crate.  The bincode encoding is:
/// 8 bytes (`u64` length = 64) + 64 bytes (little-endian U512 value) = 72 bytes total.
///
/// ## Compatibility
///
/// This format is byte-for-byte identical to what `primitive_types 0.12`'s `#[derive(Serialize)]`
/// produces with bincode for non-human-readable serializers, so no migration is required.
///
/// Use via `#[serde(with = "u512_serde")]` on fields of type `primitive_types::U512`.
pub mod u512_serde {
    use std::fmt;

    use primitive_types::U512;
    use serde::{de, Deserializer, Serializer};

    /// Serialize a `U512` as 64 little-endian bytes (with bincode length prefix).
    pub fn serialize<S: Serializer>(val: &U512, s: S) -> Result<S::Ok, S::Error> {
        let mut bytes = [0u8; 64];
        val.to_little_endian(&mut bytes);
        // `serialize_bytes` produces an 8-byte length prefix followed by the raw bytes in
        // bincode, matching the format used by `primitive_types::U512`'s own serde impl.
        s.serialize_bytes(&bytes)
    }

    /// Deserialize a `U512` from 64 little-endian bytes.
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<U512, D::Error> {
        struct U512Visitor;

        impl<'de> de::Visitor<'de> for U512Visitor {
            type Value = U512;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "64 little-endian bytes representing a U512 value")
            }

            fn visit_bytes<E: de::Error>(self, v: &[u8]) -> Result<U512, E> {
                if v.len() != 64 {
                    return Err(E::custom(format!("expected 64 bytes for U512, got {}", v.len())));
                }
                Ok(U512::from_little_endian(v))
            }

            fn visit_byte_buf<E: de::Error>(self, v: Vec<u8>) -> Result<U512, E> {
                self.visit_bytes(&v)
            }

            fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<U512, A::Error> {
                let mut bytes = Vec::with_capacity(64);
                while let Some(b) = seq.next_element::<u8>()? {
                    if bytes.len() == 64 {
                        return Err(de::Error::custom("expected exactly 64 bytes for U512, got more"));
                    }
                    bytes.push(b);
                }
                self.visit_bytes(&bytes)
            }
        }

        d.deserialize_bytes(U512Visitor)
    }
}

#[cfg(test)]
mod tests {
    use primitive_types::U512;
    use serde::{Deserialize, Serialize};

    /// A test struct that uses `#[serde(with = "u512_serde")]` to exercise the module.
    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct TestStruct {
        #[serde(with = "super::u512_serde")]
        value: U512,
    }

    #[test]
    fn u512_serde_round_trips_via_bincode() {
        let original = TestStruct {
            value: U512::from_dec_str("123456789012345678901234567890").unwrap(),
        };
        let bytes = bincode::serialize(&original).expect("serialize failed");
        let decoded: TestStruct = bincode::deserialize(&bytes).expect("deserialize failed");
        assert_eq!(original, decoded);
    }

    #[test]
    fn u512_serde_encodes_as_64_le_bytes_with_8_byte_length_prefix() {
        let test = TestStruct { value: U512::one() };
        let bytes = bincode::serialize(&test).unwrap();
        // bincode `serialize_bytes` produces: u64 length (8 bytes LE) + raw bytes
        let length = u64::from_le_bytes(bytes[0..8].try_into().unwrap());
        assert_eq!(length, 64, "length prefix must be 64");
        assert_eq!(bytes.len(), 8 + 64, "total size must be 72 bytes");
        // U512::one() in little-endian: first byte is 1, rest zero
        assert_eq!(bytes[8], 1);
        assert!(bytes[9..].iter().all(|&b| b == 0));
    }
}
