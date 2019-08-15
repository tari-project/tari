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
//
// Portions of this file were originally copyrighted (c) 2018 The Grin Developers, issued under the Apache License,
// Version 2.0, available at http://www.apache.org/licenses/LICENSE-2.0.

use crate::{
    backend::ArrayLike,
    common::{bintree_height, find_peaks, hash_together, leaf_index, peak_map_height},
    error::MerkleMountainRangeError,
    Hash,
};
use digest::Digest;
use log::*;
use std::marker::PhantomData;

const LOG_TARGET: &str = "mmr::merkle_mountain_range";

/// An implementation of a Merkle Mountain Range (MMR). The MMR is append-only and immutable. Only the hashes are
/// stored in this data structure. The data itself can be stored anywhere as long as you can maintain a 1:1 mapping
/// of the hash of that data to the leaf nodes in the MMR.
pub struct MerkleMountainRange<D, B>
where B: ArrayLike
{
    pub(crate) hashes: B,
    pub(crate) _hasher: PhantomData<D>,
}

impl<D, B> MerkleMountainRange<D, B>
where
    D: Digest,
    B: ArrayLike<Value = Hash>,
{
    /// Create a new Merkle mountain range using the given backend for storage
    pub fn new(backend: B) -> MerkleMountainRange<D, B> {
        MerkleMountainRange {
            hashes: backend,
            _hasher: PhantomData,
        }
    }

    /// Return the number of nodes in the full Merkle Mountain range, excluding bagged hashes
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.hashes.len()
    }

    /// Returns true if the MMR contains no hashes
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// This function returns the hash of the node index provided indexed from 0
    pub fn get_node_hash(&self, node_index: usize) -> Option<&Hash> {
        self.hashes.get(node_index)
    }

    /// This function returns the hash of the leaf index provided, indexed from 0
    pub fn get_leaf_hash(&self, leaf_node_index: usize) -> Option<&Hash> {
        self.get_node_hash(leaf_index(leaf_node_index))
    }

    /// This function will return the single merkle root of the MMR by simply hashing the peaks together.
    ///
    /// Note that this differs from the bagging strategy used in other MMR implementations, and saves you a few hashes
    pub fn get_merkle_root(&self) -> Hash {
        if self.is_empty() {
            return MerkleMountainRange::<D, B>::null_hash();
        }
        let hasher = D::new();
        self.hash_to_root(hasher).result().to_vec()
    }

    pub(crate) fn hash_to_root(&self, hasher: D) -> D {
        let peaks = find_peaks(self.hashes.len());
        peaks
            .into_iter()
            .map(|i| self.hashes.get_or_panic(i))
            .fold(hasher, |hasher, h| hasher.chain(h))
    }

    /// Push a new element into the MMR. Computes new related peaks at
    /// the same time if applicable.
    pub fn push(&mut self, hash: &Hash) -> Result<usize, MerkleMountainRangeError> {
        if self.is_empty() {
            return self.push_hash(hash.clone());
        }
        let mut pos = self.len();
        let (peak_map, height) = peak_map_height(pos);
        if height != 0 {
            return Err(MerkleMountainRangeError::CorruptDataStructure);
        }
        self.push_hash(hash.clone())?;
        // hash with all immediately preceding peaks, as indicated by peak map
        let mut peak = 1;
        while (peak_map & peak) != 0 {
            let left_sibling = pos + 1 - 2 * peak;
            let left_hash = &self.hashes.get_or_panic(left_sibling);
            peak *= 2;
            pos += 1;
            let last_hash = &self.hashes.get_or_panic(self.hashes.len() - 1);
            let new_hash = hash_together::<D>(left_hash, last_hash);
            self.push_hash(new_hash)?;
        }
        Ok(pos)
    }

    /// Walks the nodes in the MMR and revalidates all parent hashes
    pub fn validate(&self) -> Result<(), MerkleMountainRangeError> {
        // iterate on all parent nodes
        for n in 0..self.len() {
            let height = bintree_height(n);
            if height > 0 {
                let hash = self
                    .get_node_hash(n)
                    .ok_or(MerkleMountainRangeError::CorruptDataStructure)?;
                let left_pos = n - (1 << height);
                let right_pos = n - 1;
                let left_child_hash = self
                    .get_node_hash(left_pos)
                    .ok_or(MerkleMountainRangeError::CorruptDataStructure)?;
                let right_child_hash = self
                    .get_node_hash(right_pos)
                    .ok_or(MerkleMountainRangeError::CorruptDataStructure)?;
                // hash the two child nodes together with parent_pos and compare
                let hash_check = hash_together::<D>(left_child_hash, right_child_hash);
                if &hash_check != hash {
                    return Err(MerkleMountainRangeError::InvalidMerkleTree);
                }
            }
        }
        Ok(())
    }

    pub(crate) fn null_hash() -> Hash {
        D::digest(b"").to_vec()
    }

    fn push_hash(&mut self, hash: Hash) -> Result<usize, MerkleMountainRangeError> {
        self.hashes.push(hash).map_err(|e| {
            error!(target: LOG_TARGET, "{:?}", e);
            MerkleMountainRangeError::BackendPushError
        })
    }
}

impl<D, B, B2> PartialEq<MerkleMountainRange<D, B2>> for MerkleMountainRange<D, B>
where
    D: Digest,
    B: ArrayLike<Value = Hash>,
    B2: ArrayLike<Value = Hash>,
{
    fn eq(&self, other: &MerkleMountainRange<D, B2>) -> bool {
        (self.get_merkle_root() == other.get_merkle_root())
    }
}
