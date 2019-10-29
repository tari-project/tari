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

use crate::{
    backend::{ArrayLike, ArrayLikeExt},
    error::MerkleMountainRangeError,
    pruned_mmr::{prune_mutable_mmr, PrunedMutableMmr},
    Hash,
    MutableMmr,
};
use croaring::Bitmap;
use digest::Digest;
use serde::{
    de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor},
    ser::{Serialize, SerializeStruct, Serializer},
};
use std::{fmt, mem, ops::Deref};

/// A struct that wraps an MMR to keep track of changes to the MMR over time. This enables one to roll
/// back changes to a point in history. Think of `MerkleChangeTracker` as 'git' for MMRs.
///
/// [MutableMMr] implements [std::ops::Deref], so that once you've wrapped the MMR, all the immutable methods are
/// available through the auto-dereferencing.
///
/// The basic philosophy of `MerkleChangeTracker` is as follows:
/// * Start with a 'base' MMR. For efficiency, you usually want to make this a [pruned_mmr::PrunedMmr], but it
/// doesn't have to be.
/// * We then maintain a change-list for every append and delete that is made on the MMR.
/// * You can `commit` the change-set at any time, which will create a new [MerkleCheckPoint] summarising the
/// changes, and the current change-set is reset.
/// * You can `rewind` to a previously committed checkpoint, p. This entails resetting the MMR to the base state and
/// then replaying every checkpoint in sequence until checkpoint p is reached. `rewind_to_start` and `replay` perform
/// similar functions.
/// * You can `reset` the ChangeTracker, which clears the current change-set and moves you back to the most recent
/// checkpoint ('HEAD')
#[derive(Debug)]
pub struct MerkleChangeTracker<D, BaseBackend, CpBackend>
where
    D: Digest,
    BaseBackend: ArrayLike<Value = Hash>,
{
    base: MutableMmr<D, BaseBackend>,
    mmr: PrunedMutableMmr<D>,
    checkpoints: CpBackend,
    // The hashes added since the last commit
    current_additions: Vec<Hash>,
    // The deletions since the last commit
    current_deletions: Bitmap,
}

impl<D, BaseBackend, CpBackend> MerkleChangeTracker<D, BaseBackend, CpBackend>
where
    D: Digest,
    BaseBackend: ArrayLike<Value = Hash>,
    CpBackend: ArrayLike<Value = MerkleCheckPoint> + ArrayLikeExt<Value = MerkleCheckPoint>,
{
    /// Wrap an MMR inside a change tracker.
    ///
    /// # Parameters
    /// * `base`: The base, or anchor point of the change tracker. This represents the earliest point that you can
    ///   [MerkleChangeTracker::rewind] to.
    /// * `mmr`: An empty MMR instance that will be used to maintain the current state of the MMR.
    /// * `diffs`: The (usually empty) collection of diffs that will be used to store the MMR checkpoints.
    ///
    /// # Returns
    /// A new `MerkleChangeTracker` instance that is configured using the MMR and ChangeTracker instances provided.
    pub fn new(
        base: MutableMmr<D, BaseBackend>,
        diffs: CpBackend,
    ) -> Result<MerkleChangeTracker<D, BaseBackend, CpBackend>, MerkleMountainRangeError>
    {
        let mmr = prune_mutable_mmr::<D, _>(&base)?;
        Ok(MerkleChangeTracker {
            base,
            mmr,
            checkpoints: diffs,
            current_additions: Vec::new(),
            current_deletions: Bitmap::create(),
        })
    }

    /// Return the number of Checkpoints this change tracker has recorded
    pub fn checkpoint_count(&self) -> Result<usize, MerkleMountainRangeError> {
        self.checkpoints
            .len()
            .map_err(|e| MerkleMountainRangeError::BackendError(e.to_string()))
    }

    /// Push the given hash into the MMR and update the current change-set
    pub fn push(&mut self, hash: &Hash) -> Result<usize, MerkleMountainRangeError> {
        let result = self.mmr.push(hash)?;
        self.current_additions.push(hash.clone());
        Ok(result)
    }

    /// Discards the current change-set and resets the MMR state to that of the last checkpoint
    pub fn reset(&mut self) -> Result<(), MerkleMountainRangeError> {
        self.replay(self.checkpoint_count()?)
    }

    /// Mark a node for deletion and optionally compress the deletion bitmap. See [MutableMmr::delete_and_compress]
    /// for more details
    pub fn delete_and_compress(&mut self, leaf_node_index: u32, compress: bool) -> bool {
        let result = self.mmr.delete_and_compress(leaf_node_index, compress);
        if result {
            self.current_deletions.add(leaf_node_index)
        }
        result
    }

    /// Mark a node for completion, and compress the roaring bitmap. See [delete_and_compress] for details.
    pub fn delete(&mut self, leaf_node_index: u32) -> bool {
        self.delete_and_compress(leaf_node_index, true)
    }

    /// Compress the roaring bitmap mapping deleted nodes. You never have to call this method unless you have been
    /// calling [delete_and_compress] with `compress` set to `false` ahead of a call to [get_merkle_root].
    pub fn compress(&mut self) -> bool {
        self.mmr.compress()
    }

    /// Commit the change history since the last commit to a new [MerkleCheckPoint] and clear the current change set.
    pub fn commit(&mut self) -> Result<(), CpBackend::Error> {
        let mut hash_set = Vec::new();
        mem::swap(&mut hash_set, &mut self.current_additions);
        let mut deleted_set = Bitmap::create();
        mem::swap(&mut deleted_set, &mut self.current_deletions);
        let diff = MerkleCheckPoint::new(hash_set, deleted_set);
        self.checkpoints.push(diff)?;
        Ok(())
    }

    /// Rewind the MMR state by the given number of Checkpoints.
    ///
    /// Example:
    ///
    /// Assuming we start with an empty Mutable MMR, and apply the following:
    /// push(1), push(2), delete(1), *Checkpoint*  (1)
    /// push(3), push(4)             *Checkpoint*  (2)
    /// push(5), delete(4)           *Checkpoint*  (3)
    /// push(6)
    ///
    /// The state is now:
    /// ```text
    /// 1 2 3 4 5 6
    /// x     x
    /// ```
    ///
    /// After calling `rewind(1)`, The push of 6 wasn't check-pointed, so it will be discarded, and rewinding back one
    /// point to checkpoint 2 the state will be:
    /// ```text
    /// 1 2 3 4
    /// x
    /// ```
    ///
    /// Calling `rewind(1)` again will yield:
    /// ```text
    /// 1 2
    /// x
    /// ```
    pub fn rewind(&mut self, steps_back: usize) -> Result<(), MerkleMountainRangeError> {
        self.replay(self.checkpoint_count()? - steps_back)
    }

    /// Rewinds the MMR back to the state of the base MMR; essentially discarding all the history accumulated to date.
    pub fn rewind_to_start(&mut self) -> Result<(), MerkleMountainRangeError> {
        self.mmr = self.revert_mmr_to_base()?;
        Ok(())
    }

    // Common function for rewind_to_start and replay
    fn revert_mmr_to_base(&mut self) -> Result<PrunedMutableMmr<D>, MerkleMountainRangeError> {
        let mmr = prune_mutable_mmr::<D, _>(&self.base)?;
        self.current_deletions = Bitmap::create();
        self.current_additions = Vec::new();
        Ok(mmr)
    }

    /// Similar to [MerkleChangeTracker::rewind], `replay` moves the MMR state through checkpoints, but uses the base
    /// MMR as the starting point and steps forward through `num_checkpoints` checkpoints, rather than rewinding from
    /// the current state.
    pub fn replay(&mut self, num_checkpoints: usize) -> Result<(), MerkleMountainRangeError> {
        let mut mmr = self.revert_mmr_to_base()?;
        self.checkpoints.truncate(num_checkpoints)?;
        let mut result = Ok(());
        self.checkpoints.for_each(|v| {
            if result.is_err() {
                return;
            }
            result = match v {
                Ok(cp) => cp.apply(&mut mmr),
                Err(e) => Err(e),
            };
        })?;
        mmr.compress();
        self.mmr = mmr;
        result
    }

    /// Returns the Merkle Checkpoint specified by the provided index.
    pub fn get_checkpoint(&self, index: usize) -> Result<MerkleCheckPoint, MerkleMountainRangeError> {
        match self
            .checkpoints
            .get(index)
            .map_err(|e| MerkleMountainRangeError::BackendError(e.to_string()))?
        {
            None => Err(MerkleMountainRangeError::OutOfRange),
            Some(cp) => Ok(cp.clone()),
        }
    }

    /// Returns the MMR index of a newly added hash, this index is only valid if the change history is Committed.
    pub fn index(&self, hash: &Hash) -> Option<usize> {
        self.current_additions
            .iter()
            .position(|h| h == hash)
            .map(|i| self.mmr.len() as usize - self.current_additions.len() + i)
    }
}

impl<D, BaseBackend, DiffBackend> Deref for MerkleChangeTracker<D, BaseBackend, DiffBackend>
where
    D: Digest,
    BaseBackend: ArrayLike<Value = Hash>,
{
    type Target = PrunedMutableMmr<D>;

    fn deref(&self) -> &Self::Target {
        &self.mmr
    }
}

#[derive(Debug, Clone)]
pub struct MerkleCheckPoint {
    nodes_added: Vec<Hash>,
    nodes_deleted: Bitmap,
}

impl MerkleCheckPoint {
    pub fn new(nodes_added: Vec<Hash>, nodes_deleted: Bitmap) -> MerkleCheckPoint {
        MerkleCheckPoint {
            nodes_added,
            nodes_deleted,
        }
    }

    /// Apply this checkpoint to the MMR provided. Take care: The `deleted` set is not compressed after returning
    /// from here.
    fn apply<D, B2>(&self, mmr: &mut MutableMmr<D, B2>) -> Result<(), MerkleMountainRangeError>
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

    /// Return a reference to the hashes of the nodes added in the checkpoint
    pub fn nodes_added(&self) -> &Vec<Hash> {
        &self.nodes_added
    }

    /// Return a reference to the roaring bitmap of nodes that were deleted in this checkpoint
    pub fn nodes_deleted(&self) -> &Bitmap {
        &self.nodes_deleted
    }

    /// Break a checkpoint up into its constituent parts
    pub fn into_parts(self) -> (Vec<Hash>, Bitmap) {
        (self.nodes_added, self.nodes_deleted)
    }
}

impl Serialize for MerkleCheckPoint {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
        let mut state = serializer.serialize_struct("MerkleCheckPoint", 2)?;
        state.serialize_field("nodes_added", &self.nodes_added)?;
        state.serialize_field("nodes_deleted", &self.nodes_deleted.serialize())?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for MerkleCheckPoint {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: Deserializer<'de> {
        enum Field {
            NodesAdded,
            NodesDeleted,
        };

        impl<'de> Deserialize<'de> for Field {
            fn deserialize<D>(deserializer: D) -> Result<Field, D::Error>
            where D: Deserializer<'de> {
                struct FieldVisitor;

                impl<'de> Visitor<'de> for FieldVisitor {
                    type Value = Field;

                    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                        formatter.write_str("`nodes_added` or `nodes_deleted`")
                    }

                    fn visit_str<E>(self, value: &str) -> Result<Field, E>
                    where E: de::Error {
                        match value {
                            "nodes_added" => Ok(Field::NodesAdded),
                            "nodes_deleted" => Ok(Field::NodesDeleted),
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
                let nodes_deleted: Bitmap = Bitmap::deserialize(&nodes_deleted_buf);
                Ok(MerkleCheckPoint::new(nodes_added, nodes_deleted))
            }

            fn visit_map<V>(self, mut map: V) -> Result<MerkleCheckPoint, V::Error>
            where V: MapAccess<'de> {
                let mut nodes_added = None;
                let mut nodes_deleted = None;
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
                    }
                }
                let nodes_added = nodes_added.ok_or_else(|| de::Error::missing_field("nodes_added"))?;
                let nodes_deleted = nodes_deleted.ok_or_else(|| de::Error::missing_field("nodes_deleted"))?;
                Ok(MerkleCheckPoint::new(nodes_added, nodes_deleted))
            }
        }

        const FIELDS: &[&str] = &["nodes_added", "nodes_deleted"];
        deserializer.deserialize_struct("MerkleCheckPoint", FIELDS, MerkleCheckPointVisitor)
    }
}
