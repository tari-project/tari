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

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{
    de::{self, Visitor},
    Deserialize,
    Deserializer,
    Serialize,
    Serializer,
};
use tari_crypto::hashing::AsFixedBytes;
use tari_utilities::{hex::*, ByteArray, ByteArrayError};

use super::BulletRangeProofHasherBlake256;
use crate::types::FixedHash;

#[derive(Debug, Default, Clone, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
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
        Ok(BulletRangeProof(v.clone()))
    }

    fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, ByteArrayError> {
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

impl<'de> Deserialize<'de> for BulletRangeProof {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: Deserializer<'de> {
        struct RangeProofVisitor;

        impl<'de> Visitor<'de> for RangeProofVisitor {
            type Value = BulletRangeProof;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a bulletproof range proof in binary format")
            }

            fn visit_bytes<E>(self, v: &[u8]) -> Result<BulletRangeProof, E>
            where E: de::Error {
                BulletRangeProof::from_canonical_bytes(v).map_err(E::custom)
            }
        }

        if deserializer.is_human_readable() {
            let s: String = Deserialize::deserialize(deserializer)?;
            BulletRangeProof::from_hex(s.as_str()).map_err(de::Error::custom)
        } else {
            deserializer.deserialize_bytes(RangeProofVisitor)
        }
    }
}
