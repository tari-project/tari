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
    error::MerkleMountainRangeError,
    pruned_hashset::PrunedHashSet,
    ArrayLike,
    Hash,
    MerkleMountainRange,
    MutableMmr,
};
use digest::Digest;
use serde::export::PhantomData;
use std::convert::TryFrom;

pub type PrunedMmr<D> = MerkleMountainRange<D, PrunedHashSet>;
pub type PrunedMutableMmr<D> = MutableMmr<D, PrunedHashSet>;

/// Create a pruned Merkle Mountain Range from the provided MMR. Pruning entails throwing all the hashes of the
/// pruned MMR away, except for the current peaks. A new MMR instance is returned that allows you to continue
/// adding onto the MMR as before. Most functions of the pruned MMR will work as expected, but obviously, any
/// leaf hashes prior to the base point won't be available. `get_leaf_hash` will return `None` for those nodes, and
/// `validate` will throw an error.
pub fn prune_mmr<D, B>(mmr: &MerkleMountainRange<D, B>) -> Result<PrunedMmr<D>, MerkleMountainRangeError>
where
    D: Digest,
    B: ArrayLike<Value = Hash>,
{
    let backend = PrunedHashSet::try_from(mmr)?;
    Ok(MerkleMountainRange {
        hashes: backend,
        _hasher: PhantomData,
    })
}

/// A convenience function in the same vein as [prune_mmr], but applied to `MutableMmr` instances.
pub fn prune_mutable_mmr<D, B>(mmr: &MutableMmr<D, B>) -> Result<PrunedMutableMmr<D>, MerkleMountainRangeError>
where
    D: Digest,
    B: ArrayLike<Value = Hash>,
{
    let backend = PrunedHashSet::try_from(&mmr.mmr)?;
    Ok(MutableMmr {
        mmr: MerkleMountainRange::new(backend),
        deleted: mmr.deleted.clone(),
        size: mmr.size,
    })
}

/// `calculate_mmr_root`` takes an MMR instance and efficiently calculates the new MMR root by applying the given
/// additions to calculate a new MMR root without changing the original MMR.
///
/// This is done by creating a memory-backed sparse (pruned) copy of the original MMR, applying the changes and then
/// calculating a new root.
///
/// # Parameters
/// * `src`: A reference to the original MMR
/// * `additions`: A vector of leaf node hashes to append to the MMR
/// * `deletions`: A vector of leaf node _indices_ that will be marked as deleted.
///
/// # Returns
/// The new MMR root as a result of applying the given changes
pub fn calculate_pruned_mmr_root<D, B>(
    src: &MutableMmr<D, B>,
    additions: Vec<Hash>,
    deletions: Vec<u32>,
) -> Result<Hash, MerkleMountainRangeError>
where
    D: Digest,
    B: ArrayLike<Value = Hash>,
{
    let mut pruned_mmr = prune_mutable_mmr(src)?;
    for hash in additions {
        pruned_mmr.push(hash)?;
    }
    for index in deletions {
        pruned_mmr.delete(index);
    }
    pruned_mmr.compress();
    Ok(pruned_mmr.get_merkle_root()?)
}

pub fn calculate_mmr_root<D, B>(
    src: &MerkleMountainRange<D, B>,
    additions: Vec<Hash>,
) -> Result<Hash, MerkleMountainRangeError>
where
    D: Digest,
    B: ArrayLike<Value = Hash>,
{
    let mut mmr = prune_mmr(src)?;
    for hash in additions {
        mmr.push(hash)?;
    }
    Ok(mmr.get_merkle_root()?)
}
