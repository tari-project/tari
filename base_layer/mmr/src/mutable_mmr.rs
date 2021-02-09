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
    backend::ArrayLike,
    common::{n_leaves, node_index},
    error::MerkleMountainRangeError,
    mutable_mmr_leaf_nodes::MutableMmrLeafNodes,
    Hash,
    MerkleMountainRange,
};
use croaring::Bitmap;
use digest::Digest;

/// Unlike a pure MMR, which is append-only, in `MutableMmr`, leaf nodes can be marked as deleted.
///
/// In `MutableMmr` a roaring bitmap tracks which data have been marked as deleted, and the merklish root is modified
/// to include the hash of the roaring bitmap.
///
/// The `MutableMmr` API maps nearly 1:1 to that of MerkleMountainRange so that you should be able to use it as a
/// drop-in replacement for the latter in most cases.
#[derive(Debug)]
pub struct MutableMmr<D, B>
where
    D: Digest,
    B: ArrayLike<Value = Hash>,
{
    pub(crate) mmr: MerkleMountainRange<D, B>,
    pub(crate) deleted: Bitmap,
    // The number of leaf nodes in the MutableMmr. Bitmap is limited to 4 billion elements, which is plenty.
    // [croaring::Treemap] is a 64bit alternative, but this would break things on 32bit systems. A good TODO would be
    // to select the bitmap backend using a feature flag
    pub(crate) size: u32,
}

impl<D, B> MutableMmr<D, B>
where
    D: Digest,
    B: ArrayLike<Value = Hash>,
{
    /// Create a new mutable MMR using the backend provided
    pub fn new(mmr_backend: B, deleted: Bitmap) -> Result<MutableMmr<D, B>, MerkleMountainRangeError> {
        let mmr = MerkleMountainRange::new(mmr_backend);
        Ok(MutableMmr {
            size: mmr.get_leaf_count()? as u32,
            mmr,
            deleted,
        })
    }

    /// Clear the MutableMmr and assign the MMR state from the set of leaf_hashes and deleted nodes given in `state`.
    pub fn assign(&mut self, state: MutableMmrLeafNodes) -> Result<(), MerkleMountainRangeError> {
        self.mmr.assign(state.leaf_hashes)?;
        self.deleted = state.deleted;
        self.size = self.mmr.get_leaf_count()? as u32;
        Ok(())
    }

    /// Return the number of leaf nodes in the `MutableMmr` that have not been marked as deleted.
    ///
    /// NB: This is semantically different to `MerkleMountainRange::len()`. The latter returns the total number of
    /// nodes in the MMR, while this function returns the number of leaf nodes minus the number of nodes marked for
    /// deletion.
    #[inline(always)]
    pub fn len(&self) -> u32 {
        self.size - self.deleted.cardinality() as u32
    }

    /// Returns true if the the MMR contains no nodes, OR all nodes have been marked for deletion
    pub fn is_empty(&self) -> Result<bool, MerkleMountainRangeError> {
        Ok(self.mmr.is_empty()? || self.deleted.cardinality() == self.size as u64)
    }

    /// This function returns the hash of the leaf index provided, indexed from 0. If the hash does not exist, or if it
    /// has been marked for deletion, `None` is returned.
    pub fn get_leaf_hash(&self, leaf_index: u32) -> Result<Option<Hash>, MerkleMountainRangeError> {
        if self.deleted.contains(leaf_index) {
            return Ok(None);
        }
        self.mmr.get_node_hash(node_index(leaf_index as usize))
    }

    /// Returns the hash of the leaf index provided, as well as its deletion status. The node has been marked for
    /// deletion if the boolean value is true.
    pub fn get_leaf_status(&self, leaf_index: u32) -> Result<(Option<Hash>, bool), MerkleMountainRangeError> {
        let hash = self.mmr.get_node_hash(node_index(leaf_index as usize))?;
        let deleted = self.deleted.contains(leaf_index);
        Ok((hash, deleted))
    }

    /// Returns the number of leaf nodes in the MMR.
    pub fn get_leaf_count(&self) -> usize {
        self.size as usize
    }

    /// Returns a merkle(ish) root for this merkle set.
    ///
    /// The root is calculated by concatenating the MMR merkle root with the compressed serialisation of the bitmap
    /// and then hashing the result.
    pub fn get_merkle_root(&self) -> Result<Hash, MerkleMountainRangeError> {
        // Note that two MutableMmrs could both return true for `is_empty()`, but have different merkle roots by
        // virtue of the fact that the underlying MMRs could be different, but all elements are marked as deleted in
        // both sets.
        let mmr_root = self.mmr.get_merkle_root()?;
        let mut hasher = D::new();
        hasher.input(&mmr_root);
        Ok(self.hash_deleted(hasher).result().to_vec())
    }

    /// Returns only the MMR merkle root without the compressed serialisation of the bitmap
    pub fn get_mmr_only_root(&self) -> Result<Hash, MerkleMountainRangeError> {
        self.mmr.get_merkle_root()
    }

    /// See [MerkleMountainRange::find_node_index]
    pub fn find_node_index(&self, hash: &[u8]) -> Result<Option<usize>, MerkleMountainRangeError> {
        self.mmr.find_node_index(hash)
    }

    /// See [MerkleMountainRange::find_leaf_index]
    pub fn find_leaf_index(&self, hash: &[u8]) -> Result<Option<u32>, MerkleMountainRangeError> {
        self.mmr.find_leaf_index(hash)
    }

    /// Push a new element into the MMR. Computes new related peaks at the same time if applicable.
    /// Returns the new number of leaf nodes (regardless of deleted state) in the mutable MMR
    pub fn push(&mut self, hash: Hash) -> Result<usize, MerkleMountainRangeError> {
        if self.size == u32::MAX {
            return Err(MerkleMountainRangeError::MaximumSizeReached);
        }
        self.mmr.push(hash)?;
        self.size += 1;
        Ok(self.size as usize)
    }

    /// Mark a node for deletion and optionally compress the deletion bitmap. Don't call this function unless you're
    /// in a tight loop and want to eke out some extra performance by delaying the bitmap compression until after the
    /// batch deletion.
    ///
    /// Note that this function doesn't actually delete anything (the underlying MMR structure is immutable), but marks
    /// the leaf node as deleted. Once a leaf node has been marked for deletion:
    /// * `get_leaf_hash(n)` will return None,
    /// * `len()` will not count this node anymore
    ///
    /// **NB**: You should call compress before calling `get_merkle_root()`. If you don't, the merkle root will be
    /// incorrect.
    ///
    /// # Parameters
    /// * `leaf_node_index`: The index of the leaf node to mark for deletion, zero-based.
    ///
    /// # Return
    /// The function returns true if a node was actually marked for deletion. If the index is out of bounds, or was
    /// already deleted, the function returns false.
    pub fn delete(&mut self, leaf_index: u32) -> bool {
        if (leaf_index >= self.size) || self.deleted.contains(leaf_index) {
            return false;
        }
        self.deleted.add(leaf_index);
        true
    }

    /// Compress the roaring bitmap mapping deleted nodes. You never have to call this method unless you have been
    /// calling [delete_and_compress] with `compress` set to `false` ahead of a call to [get_merkle_root].
    pub fn compress(&mut self) -> bool {
        self.deleted.run_optimize()
    }

    /// Walks the nodes in the MMR and validates all parent hashes
    ///
    /// This just calls through to the underlying MMR's validate method. There's nothing we can do to check whether
    /// the roaring bitmap represents all the leaf nodes that we want to delete. Note: A struct that uses
    /// `MutableMmr` and links it to actual data should be able to do this though.
    pub fn validate(&self) -> Result<(), MerkleMountainRangeError> {
        self.mmr.validate()
    }

    /// Hash the roaring bitmap of nodes that are marked for deletion
    fn hash_deleted(&self, mut hasher: D) -> D {
        let bitmap_ser = self.deleted.serialize();
        hasher.input(&bitmap_ser);
        hasher
    }

    // Returns a bitmap with only the deleted nodes for the specified region in the MMR.
    fn get_sub_bitmap(&self, leaf_index: usize, count: usize) -> Result<Bitmap, MerkleMountainRangeError> {
        let mut deleted = self.deleted.clone();
        if leaf_index > 0 {
            deleted.remove_range_closed(0..(leaf_index - 1) as u32)
        }
        let leaf_count = self.mmr.get_leaf_count()?;
        if leaf_count > 1 {
            let last_index = leaf_index + count - 1;
            if last_index < leaf_count - 1 {
                deleted.remove_range_closed((last_index + 1) as u32..leaf_count as u32);
            }
        }
        Ok(deleted)
    }

    /// Returns the state of the MMR that consists of the leaf hashes and the deleted nodes.
    pub fn to_leaf_nodes(
        &self,
        leaf_index: usize,
        count: usize,
    ) -> Result<MutableMmrLeafNodes, MerkleMountainRangeError>
    {
        Ok(MutableMmrLeafNodes {
            leaf_hashes: self.mmr.get_leaf_hashes(leaf_index, count)?,
            deleted: self.get_sub_bitmap(leaf_index, count)?,
        })
    }

    /// Expose the MerkleMountainRange for verifying proofs
    pub fn mmr(&self) -> &MerkleMountainRange<D, B> {
        &self.mmr
    }

    /// Return a reference to the bitmap of deleted nodes
    pub fn deleted(&self) -> &Bitmap {
        &self.deleted
    }

    pub fn clear(&mut self) -> Result<(), MerkleMountainRangeError> {
        self.mmr.clear()?;
        self.deleted = Bitmap::create();
        self.size = 0;
        Ok(())
    }
}

impl<D, B, B2> PartialEq<MutableMmr<D, B2>> for MutableMmr<D, B>
where
    D: Digest,
    B: ArrayLike<Value = Hash>,
    B2: ArrayLike<Value = Hash>,
{
    fn eq(&self, other: &MutableMmr<D, B2>) -> bool {
        self.get_merkle_root() == other.get_merkle_root()
    }
}

impl<D, B> From<MerkleMountainRange<D, B>> for MutableMmr<D, B>
where
    D: Digest,
    B: ArrayLike<Value = Hash>,
{
    fn from(mmr: MerkleMountainRange<D, B>) -> Self {
        let size = n_leaves(mmr.len().unwrap()) as u32; // TODO: fix unwrap
        MutableMmr {
            mmr,
            deleted: Bitmap::create(),
            size,
        }
    }
}
