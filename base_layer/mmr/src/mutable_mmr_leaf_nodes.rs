// Copyright 2019. The Tari Project
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

use crate::Hash;
use croaring::Bitmap;
use serde::{
    de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor},
    ser::{Serialize, SerializeStruct, Serializer},
};
use std::fmt;

/// The MutableMmrLeafNodes is used to create and share a restorable state for an MMR.
#[derive(Debug, Clone, PartialEq)]
pub struct MutableMmrLeafNodes {
    pub leaf_hashes: Vec<Hash>,
    pub deleted: Bitmap,
}

impl MutableMmrLeafNodes {
    /// Create a new MutableMmrLeafNodes using the set of leaf hashes and deleted nodes.
    pub fn new(leaf_hashes: Vec<Hash>, deleted: Bitmap) -> Self {
        Self { leaf_hashes, deleted }
    }

    /// Merge the current state with the next state bundle
    pub fn combine(&mut self, next_state: MutableMmrLeafNodes) {
        let MutableMmrLeafNodes {
            mut leaf_hashes,
            deleted,
        } = next_state;
        self.leaf_hashes.append(&mut leaf_hashes);
        self.deleted.or_inplace(&deleted);
    }
}

impl From<Vec<Hash>> for MutableMmrLeafNodes {
    fn from(leaf_hashes: Vec<Hash>) -> Self {
        Self {
            leaf_hashes,
            deleted: Bitmap::create(),
        }
    }
}

impl Serialize for MutableMmrLeafNodes {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
        let mut state = serializer.serialize_struct("MutableMmrLeafNodes", 2)?;
        state.serialize_field("leaf_hashes", &self.leaf_hashes)?;
        state.serialize_field("deleted", &self.deleted.serialize())?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for MutableMmrLeafNodes {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: Deserializer<'de> {
        enum Field {
            LeafHashes,
            Deleted,
        };

        impl<'de> Deserialize<'de> for Field {
            fn deserialize<D>(deserializer: D) -> Result<Field, D::Error>
            where D: Deserializer<'de> {
                struct FieldVisitor;

                impl<'de> Visitor<'de> for FieldVisitor {
                    type Value = Field;

                    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                        formatter.write_str("`leaf_hashes` or `deleted`")
                    }

                    fn visit_str<E>(self, value: &str) -> Result<Field, E>
                    where E: de::Error {
                        match value {
                            "leaf_hashes" => Ok(Field::LeafHashes),
                            "deleted" => Ok(Field::Deleted),
                            _ => Err(de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }

                deserializer.deserialize_identifier(FieldVisitor)
            }
        }

        struct MutableMmrLeafNodesVisitor;

        impl<'de> Visitor<'de> for MutableMmrLeafNodesVisitor {
            type Value = MutableMmrLeafNodes;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("struct MutableMmrLeafNodes")
            }

            fn visit_seq<V>(self, mut seq: V) -> Result<MutableMmrLeafNodes, V::Error>
            where V: SeqAccess<'de> {
                let leaf_hashes = seq.next_element()?.ok_or_else(|| de::Error::invalid_length(0, &self))?;
                let deleted_buf: Vec<u8> = seq.next_element()?.ok_or_else(|| de::Error::invalid_length(1, &self))?;
                let deleted: Bitmap = Bitmap::deserialize(&deleted_buf);
                Ok(MutableMmrLeafNodes::new(leaf_hashes, deleted))
            }

            fn visit_map<V>(self, mut map: V) -> Result<MutableMmrLeafNodes, V::Error>
            where V: MapAccess<'de> {
                let mut leaf_hashes = None;
                let mut deleted = None;
                while let Some(key) = map.next_key()? {
                    match key {
                        Field::LeafHashes => {
                            if leaf_hashes.is_some() {
                                return Err(de::Error::duplicate_field("nodes_added"));
                            }
                            leaf_hashes = Some(map.next_value()?);
                        },
                        Field::Deleted => {
                            if deleted.is_some() {
                                return Err(de::Error::duplicate_field("nodes_deleted"));
                            }
                            let deleted_buf: Vec<u8> = map.next_value()?;
                            deleted = Some(Bitmap::deserialize(&deleted_buf));
                        },
                    }
                }
                let leaf_hashes = leaf_hashes.ok_or_else(|| de::Error::missing_field("leaf_hashes"))?;
                let deleted = deleted.ok_or_else(|| de::Error::missing_field("deleted"))?;
                Ok(MutableMmrLeafNodes::new(leaf_hashes, deleted))
            }
        }

        const FIELDS: &[&str] = &["leaf_hashes", "deleted"];
        deserializer.deserialize_struct("MutableMmrLeafNodes", FIELDS, MutableMmrLeafNodesVisitor)
    }
}
