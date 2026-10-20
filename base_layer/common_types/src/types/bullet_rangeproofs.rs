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

use std::fmt;

use borsh::{
    BorshDeserialize,
    BorshSerialize,
    io::{Error, ErrorKind, Read},
};
use serde::{
    Deserialize,
    Deserializer,
    Serialize,
    Serializer,
    de::{self, Visitor},
};
use tari_crypto::hashing::AsFixedBytes;
use tari_max_size::{ValidatedDecode, impl_validated_decode};
use tari_utilities::{ByteArray, ByteArrayError, hex::*};

use super::BulletRangeProofHasherBlake256;
use crate::types::{FixedHash, MAX_RANGE_PROOF_BYTES};

/// Decoded through [`ValidatedDecode`], so every decoder rejects more than `MAX_RANGE_PROOF_BYTES` bytes.
#[derive(Debug, Default, Clone, PartialEq, Eq, PartialOrd, Ord, BorshSerialize)]
pub struct BulletRangeProof(pub Vec<u8>);
impl BulletRangeProof {
    /// Implement the hashing function for RangeProof for use in the MMR
    pub fn hash(&self) -> FixedHash {
        BulletRangeProofHasherBlake256::new()
            .chain(&self.0)
            .finalize()
            .as_fixed_bytes()
            .expect("This should be 32 bytes for a Blake 256 hash")
            .into()
    }

    /// Get the range proof as a vector reference, which is useful to satisfy the verification API without cloning
    pub fn as_vec(&self) -> &Vec<u8> {
        &self.0
    }
}

impl ByteArray for BulletRangeProof {
    fn to_vec(&self) -> Vec<u8> {
        self.0.clone()
    }

    fn from_vec(v: &Vec<u8>) -> Result<Self, ByteArrayError> {
        Self::from_canonical_bytes(v)
    }

    fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, ByteArrayError> {
        check_range_proof_len(bytes.len()).map_err(|reason| ByteArrayError::ConversionError { reason })?;
        Ok(BulletRangeProof(bytes.to_vec()))
    }

    fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl From<Vec<u8>> for BulletRangeProof {
    fn from(v: Vec<u8>) -> Self {
        BulletRangeProof(v)
    }
}

impl fmt::Display for BulletRangeProof {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

impl Serialize for BulletRangeProof {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
        if serializer.is_human_readable() {
            serializer.serialize_str(self.to_hex().as_str())
        } else {
            serializer.serialize_bytes(self.as_bytes())
        }
    }
}

fn check_range_proof_len(len: usize) -> Result<(), String> {
    if len > MAX_RANGE_PROOF_BYTES {
        return Err(format!(
            "Range proof of {len} bytes exceeds the maximum of {MAX_RANGE_PROOF_BYTES} bytes"
        ));
    }
    Ok(())
}

/// The raw form of [`BulletRangeProof`]: a hex string (human readable serde), a byte array (binary serde), or the
/// bytes behind a `u32` length prefix (borsh). The length is checked before the bytes are kept.
pub struct BulletRangeProofRaw(Vec<u8>);

impl<'de> Deserialize<'de> for BulletRangeProofRaw {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: Deserializer<'de> {
        struct RangeProofVisitor;

        impl Visitor<'_> for RangeProofVisitor {
            type Value = BulletRangeProofRaw;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a bulletproof range proof in binary format")
            }

            fn visit_bytes<E>(self, v: &[u8]) -> Result<BulletRangeProofRaw, E>
            where E: de::Error {
                check_range_proof_len(v.len()).map_err(E::custom)?;
                Ok(BulletRangeProofRaw(v.to_vec()))
            }
        }

        if deserializer.is_human_readable() {
            let s: String = Deserialize::deserialize(deserializer)?;
            let bytes = from_hex(s.as_str()).map_err(de::Error::custom)?;
            check_range_proof_len(bytes.len()).map_err(de::Error::custom)?;
            Ok(BulletRangeProofRaw(bytes))
        } else {
            deserializer.deserialize_bytes(RangeProofVisitor)
        }
    }
}

impl BorshDeserialize for BulletRangeProofRaw {
    fn deserialize_reader<R: Read>(reader: &mut R) -> borsh::io::Result<Self> {
        let len = u32::deserialize_reader(reader)? as usize;
        check_range_proof_len(len).map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
        let mut bytes = vec![0u8; len];
        reader.read_exact(&mut bytes)?;
        Ok(BulletRangeProofRaw(bytes))
    }
}

impl ValidatedDecode for BulletRangeProof {
    type Error = ByteArrayError;
    type Raw = BulletRangeProofRaw;

    fn validate(raw: Self::Raw) -> Result<Self, Self::Error> {
        Self::from_canonical_bytes(&raw.0)
    }
}

impl_validated_decode!(BulletRangeProof);

#[cfg(test)]
mod test {
    use super::*;

    fn proof_bytes(len: usize) -> Vec<u8> {
        vec![0xab; len]
    }

    #[test]
    fn range_proof_decoders_reject_more_than_the_cap() {
        for (len, accept) in [(MAX_RANGE_PROOF_BYTES, true), (MAX_RANGE_PROOF_BYTES + 1, false)] {
            let bytes = proof_bytes(len);
            let hex = to_hex(&bytes);
            assert_eq!(BulletRangeProof::from_canonical_bytes(&bytes).is_ok(), accept, "{len}");
            assert_eq!(BulletRangeProof::from_vec(&bytes).is_ok(), accept, "{len}");
            assert_eq!(BulletRangeProof::from_hex(&hex).is_ok(), accept, "{len}");

            let json = format!("\"{hex}\"");
            assert_eq!(serde_json::from_str::<BulletRangeProof>(&json).is_ok(), accept, "{len}");

            // Encode with the unchecked constructor, so the oversized proof reaches the decoders
            let proof = BulletRangeProof(bytes);
            let borsh_bytes = borsh::to_vec(&proof).unwrap();
            let decoded = borsh::from_slice::<BulletRangeProof>(&borsh_bytes);
            assert_eq!(decoded.is_ok(), accept, "{len}");
            if accept {
                assert_eq!(decoded.unwrap(), proof);
            }
        }

        // Only the length prefix: rejected before any allocation or read
        let err = borsh::from_slice::<BulletRangeProof>(&u32::MAX.to_le_bytes()).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn range_proof_encoding_is_unchanged() {
        let bytes = proof_bytes(MAX_RANGE_PROOF_BYTES);
        let proof = BulletRangeProof::from_canonical_bytes(&bytes).unwrap();

        let mut expected_borsh = vec![0x41, 0x02, 0x00, 0x00];
        expected_borsh.extend_from_slice(&bytes);
        assert_eq!(borsh::to_vec(&proof).unwrap(), expected_borsh);

        let expected_json = format!("\"{}\"", "ab".repeat(MAX_RANGE_PROOF_BYTES));
        assert_eq!(serde_json::to_string(&proof).unwrap(), expected_json);

        assert_eq!(borsh::from_slice::<BulletRangeProof>(&expected_borsh).unwrap(), proof);
        assert_eq!(serde_json::from_str::<BulletRangeProof>(&expected_json).unwrap(), proof);
    }
}
