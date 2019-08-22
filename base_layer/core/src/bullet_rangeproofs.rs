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

use crate::types::*;
use digest::Digest;
use serde::{
    de::{self, Visitor},
    Deserialize,
    Deserializer,
    Serialize,
    Serializer,
};
use std::fmt;
use tari_utilities::{byte_array::*, hash::*, hex::*};

#[derive(Debug, Default, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BulletRangeProof(pub Vec<u8>);
/// Implement the hashing function for RangeProof for use in the MMR
impl Hashable for BulletRangeProof {
    fn hash(&self) -> Vec<u8> {
        HashDigest::new().chain(&self.0).result().to_vec()
    }
}

impl ByteArray for BulletRangeProof {
    fn to_vec(&self) -> Vec<u8> {
        self.0.clone()
    }

    fn from_vec(v: &Vec<u8>) -> Result<Self, ByteArrayError> {
        Ok(BulletRangeProof { 0: v.clone() })
    }

    fn from_bytes(bytes: &[u8]) -> Result<Self, ByteArrayError> {
        Ok(BulletRangeProof { 0: bytes.to_vec() })
    }

    fn as_bytes(&self) -> &[u8] {
        &self.0
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
            self.to_hex().serialize(serializer)
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
                BulletRangeProof::from_bytes(v).map_err(E::custom)
            }
        }

        if deserializer.is_human_readable() {
            let s = String::deserialize(deserializer)?;
            BulletRangeProof::from_hex(&s).map_err(de::Error::custom)
        } else {
            deserializer.deserialize_bytes(RangeProofVisitor)
        }
    }
}
