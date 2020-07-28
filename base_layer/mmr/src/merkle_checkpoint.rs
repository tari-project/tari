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

use crate::{backend::ArrayLike, error::MerkleMountainRangeError, mutable_mmr::MutableMmr, Hash};
use croaring::Bitmap;
use digest::Digest;
use serde::{
    de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor},
    ser::{Serialize, SerializeStruct, Serializer},
};
use std::{fmt, hash::Hasher};

#[derive(Debug, Clone, PartialEq)]
pub struct MerkleCheckPoint {
    nodes_added: Vec<Hash>,
    nodes_deleted: Bitmap,
    prev_accumulated_nodes_added_count: u32,
}

impl MerkleCheckPoint {
    pub fn new(
        nodes_added: Vec<Hash>,
        nodes_deleted: Bitmap,
        prev_accumulated_nodes_added_count: u32,
    ) -> MerkleCheckPoint
    {
        MerkleCheckPoint {
            nodes_added,
            nodes_deleted,
            prev_accumulated_nodes_added_count,
        }
    }

    /// Apply this checkpoint to the MMR provided. Take care: The `deleted` set is not compressed after returning
    /// from here.
    pub fn apply<D, B2>(&self, mmr: &mut MutableMmr<D, B2>) -> Result<(), MerkleMountainRangeError>
    where
        D: Digest,
        B2: ArrayLike<Value = Hash>,
    {
        for node in &self.nodes_added {
            mmr.push(node)?;
        }
        mmr.deleted.or_inplace(&self.nodes_deleted);
        Ok(())
    }

    /// Resets the current MerkleCheckpoint. The accumulated_nodes_added_count is set to the current `MerkleCheckpoint`s
    /// count.
    pub fn reset(&mut self) {
        self.prev_accumulated_nodes_added_count = self.accumulated_nodes_added_count();
        self.nodes_added.clear();
        self.nodes_deleted = Bitmap::create();
    }

    /// Resets the current MerkleCheckpoint. The accumulated_nodes_added_count is set to the given `MerkleCheckpoint`s
    /// count.
    pub fn reset_to(&mut self, checkpoint: &Self) {
        self.prev_accumulated_nodes_added_count = checkpoint.accumulated_nodes_added_count();
        self.nodes_added.clear();
        self.nodes_deleted = Bitmap::create();
    }

    /// Add a hash to the set of nodes added.
    pub fn push_addition(&mut self, hash: Hash) {
        self.nodes_added.push(hash);
    }

    /// Add a a deleted index to the set of deleted nodes.
    pub fn push_deletion(&mut self, leaf_index: u32) {
        self.nodes_deleted.add(leaf_index);
    }

    /// Return a reference to the hashes of the nodes added in the checkpoint
    pub fn nodes_added(&self) -> &Vec<Hash> {
        &self.nodes_added
    }

    /// Return a reference to the roaring bitmap of nodes that were deleted in this checkpoint
    pub fn nodes_deleted(&self) -> &Bitmap {
        &self.nodes_deleted
    }

    /// Return the the total accumulated added node count including this checkpoint
    pub fn accumulated_nodes_added_count(&self) -> u32 {
        self.prev_accumulated_nodes_added_count + self.nodes_added.len() as u32
    }

    /// Merge the provided Merkle checkpoint into the current checkpoint.
    pub fn append(&mut self, mut cp: MerkleCheckPoint) {
        self.nodes_added.append(&mut cp.nodes_added);
        self.nodes_deleted.or_inplace(&cp.nodes_deleted);
    }

    /// Break a checkpoint up into its constituent parts
    pub fn into_parts(self) -> (Vec<Hash>, Bitmap) {
        (self.nodes_added, self.nodes_deleted)
    }
}

impl Default for MerkleCheckPoint {
    fn default() -> Self {
        Self {
            nodes_added: Default::default(),
            nodes_deleted: Bitmap::create(),
            prev_accumulated_nodes_added_count: Default::default(),
        }
    }
}

impl Eq for MerkleCheckPoint {}

#[allow(clippy::derive_hash_xor_eq)]
impl std::hash::Hash for MerkleCheckPoint {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.nodes_added.hash(state);
        self.nodes_deleted.to_vec().hash(state);
        self.prev_accumulated_nodes_added_count.hash(state);
    }
}

impl Serialize for MerkleCheckPoint {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
        let mut state = serializer.serialize_struct("MerkleCheckPoint", 3)?;
        state.serialize_field("nodes_added", &self.nodes_added)?;
        state.serialize_field("nodes_deleted", &self.nodes_deleted.serialize())?;
        state.serialize_field(
            "prev_accumulated_nodes_added_count",
            &self.prev_accumulated_nodes_added_count,
        )?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for MerkleCheckPoint {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: Deserializer<'de> {
        enum Field {
            NodesAdded,
            NodesDeleted,
            PrevAccumulatedNodesAddedCount,
        };

        impl<'de> Deserialize<'de> for Field {
            fn deserialize<D>(deserializer: D) -> Result<Field, D::Error>
            where D: Deserializer<'de> {
                struct FieldVisitor;

                impl<'de> Visitor<'de> for FieldVisitor {
                    type Value = Field;

                    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                        formatter.write_str("`nodes_added`, `nodes_deleted` or `prev_accumulated_nodes_added_count`")
                    }

                    fn visit_str<E>(self, value: &str) -> Result<Field, E>
                    where E: de::Error {
                        match value {
                            "nodes_added" => Ok(Field::NodesAdded),
                            "nodes_deleted" => Ok(Field::NodesDeleted),
                            "prev_accumulated_nodes_added_count" => Ok(Field::PrevAccumulatedNodesAddedCount),
                            _ => Err(de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }

                deserializer.deserialize_identifier(FieldVisitor)
            }
        }

        struct MerkleCheckPointVisitor;

        impl<'de> Visitor<'de> for MerkleCheckPointVisitor {
            type Value = MerkleCheckPoint;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("struct MerkleCheckPoint")
            }

            fn visit_seq<V>(self, mut seq: V) -> Result<MerkleCheckPoint, V::Error>
            where V: SeqAccess<'de> {
                let nodes_added = seq.next_element()?.ok_or_else(|| de::Error::invalid_length(0, &self))?;
                let nodes_deleted_buf: Vec<u8> =
                    seq.next_element()?.ok_or_else(|| de::Error::invalid_length(1, &self))?;
                let nodes_deleted = Bitmap::deserialize(&nodes_deleted_buf);
                let prev_accumulated_nodes_added_count =
                    seq.next_element()?.ok_or_else(|| de::Error::invalid_length(2, &self))?;
                Ok(MerkleCheckPoint::new(
                    nodes_added,
                    nodes_deleted,
                    prev_accumulated_nodes_added_count,
                ))
            }

            fn visit_map<V>(self, mut map: V) -> Result<MerkleCheckPoint, V::Error>
            where V: MapAccess<'de> {
                let mut nodes_added = None;
                let mut nodes_deleted = None;
                let mut prev_accumulated_nodes_added_count = None;
                while let Some(key) = map.next_key()? {
                    match key {
                        Field::NodesAdded => {
                            if nodes_added.is_some() {
                                return Err(de::Error::duplicate_field("nodes_added"));
                            }
                            nodes_added = Some(map.next_value()?);
                        },
                        Field::NodesDeleted => {
                            if nodes_deleted.is_some() {
                                return Err(de::Error::duplicate_field("nodes_deleted"));
                            }
                            let nodes_deleted_buf: Vec<u8> = map.next_value()?;
                            nodes_deleted = Some(Bitmap::deserialize(&nodes_deleted_buf));
                        },
                        Field::PrevAccumulatedNodesAddedCount => {
                            if prev_accumulated_nodes_added_count.is_some() {
                                return Err(de::Error::duplicate_field("nodes_deleted"));
                            }

                            prev_accumulated_nodes_added_count = Some(map.next_value()?);
                        },
                    }
                }

                let nodes_added = nodes_added.ok_or_else(|| de::Error::missing_field("nodes_added"))?;
                let nodes_deleted = nodes_deleted.ok_or_else(|| de::Error::missing_field("nodes_deleted"))?;
                let prev_accumulated_nodes_added_count = prev_accumulated_nodes_added_count
                    .ok_or_else(|| de::Error::missing_field("accumulated_nodes_added_count"))?;
                Ok(MerkleCheckPoint::new(
                    nodes_added,
                    nodes_deleted,
                    prev_accumulated_nodes_added_count,
                ))
            }
        }

        const FIELDS: &[&str] = &["nodes_added", "nodes_deleted", "prev_accumulated_nodes_added_count"];
        deserializer.deserialize_struct("MerkleCheckPoint", FIELDS, MerkleCheckPointVisitor)
    }
}
