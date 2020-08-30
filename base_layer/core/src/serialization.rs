// Copyright 2020. The Tari Project
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
//

pub(crate) mod hash_serializer {
    use crate::{tari_utilities::ByteArray, transactions::types::BlockHash};
    use serde::{
        de::{self, Visitor},
        Deserialize,
        Deserializer,
        Serialize,
        Serializer,
    };
    use std::fmt;
    use tari_crypto::tari_utilities::hex::Hex;

    pub fn serialize<S>(bytes: &BlockHash, serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
        if serializer.is_human_readable() {
            bytes.to_hex().serialize(serializer)
        } else {
            serializer.serialize_bytes(bytes.as_bytes())
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<BlockHash, D::Error>
    where D: Deserializer<'de> {
        struct BlockHashVisitor;

        impl<'de> Visitor<'de> for BlockHashVisitor {
            type Value = BlockHash;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("A block header hash in binary format")
            }

            fn visit_bytes<E>(self, v: &[u8]) -> Result<BlockHash, E>
            where E: de::Error {
                BlockHash::from_bytes(v).map_err(E::custom)
            }
        }

        if deserializer.is_human_readable() {
            let s = String::deserialize(deserializer)?;
            BlockHash::from_hex(&s).map_err(de::Error::custom)
        } else {
            deserializer.deserialize_bytes(BlockHashVisitor)
        }
    }
}
