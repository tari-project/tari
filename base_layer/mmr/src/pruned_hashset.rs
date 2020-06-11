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

use crate::{common::find_peaks, error::MerkleMountainRangeError, ArrayLike, Hash, MerkleMountainRange};
use digest::Digest;
use std::convert::TryFrom;

/// This is a specialised struct that represents a pruned hash set for Merkle Mountain Ranges.
///
/// The basic idea is that when adding a new hash, only the peaks to the left of the new node hierarchy are ever needed.
/// This means that if we don't care about the data earlier than a given leaf node index, n_0, (i.e. we still have the
/// hashes, but can't recalculate them from source), we _only need to store the local peaks for the MMR at that time_
/// and we can forget about the rest. There will never be a request for a hash other than those at the peaks for the
/// MMR with n_0 leaf nodes.
///
/// The awesome thing is that this struct can be dropped into [MerkleMountainRange] as a backend and it. just. works.
#[derive(Debug)]
pub struct PrunedHashSet {
    /// The size of the base MMR. Only peaks are available for indices less than this value
    base_offset: usize,
    /// The array of peak indices for an MMR of size `base_offset`
    peak_indices: Vec<usize>,
    /// The array of hashes at the MMR peaks
    peak_hashes: Vec<Hash>,
    /// New hashes added subsequent to `base_offset`.
    hashes: Vec<Hash>,
}

impl<D, B> TryFrom<&MerkleMountainRange<D, B>> for PrunedHashSet
where
    D: Digest,
    B: ArrayLike<Value = Hash>,
{
    type Error = MerkleMountainRangeError;

    fn try_from(base_mmr: &MerkleMountainRange<D, B>) -> Result<Self, Self::Error> {
        let base_offset = base_mmr.len()?;
        let peak_indices = find_peaks(base_offset);
        let peak_hashes = peak_indices
            .iter()
            .map(|i| match base_mmr.get_node_hash(*i)? {
                Some(h) => Ok(h),
                None => Err(MerkleMountainRangeError::HashNotFound(*i)),
            })
            .collect::<Result<_, _>>()?;
        Ok(PrunedHashSet {
            base_offset,
            peak_indices,
            peak_hashes,
            hashes: Vec::new(),
        })
    }
}

impl ArrayLike for PrunedHashSet {
    type Error = MerkleMountainRangeError;
    type Value = Hash;

    #[inline(always)]
    fn len(&self) -> Result<usize, Self::Error> {
        Ok(self.base_offset + self.hashes.len())
    }

    fn is_empty(&self) -> Result<bool, Self::Error> {
        Ok(self.len()? == 0)
    }

    fn push(&mut self, item: Self::Value) -> Result<usize, Self::Error> {
        self.hashes.push(item);
        Ok(self.len()? - 1)
    }

    fn get(&self, index: usize) -> Result<Option<Self::Value>, Self::Error> {
        // If the index is from before we started adding hashes, we can return the hash *if and only if* it is a peak
        if index < self.base_offset {
            return Ok(match self.peak_indices.binary_search(&index) {
                Ok(nth_peak) => Some(self.peak_hashes[nth_peak].clone()),
                Err(_) => None,
            });
        }
        Ok(self.hashes.get(index - self.base_offset)?)
    }

    fn get_or_panic(&self, index: usize) -> Self::Value {
        self.get(index)
            .unwrap()
            .expect("PrunedHashSet only tracks peaks before the offset")
    }

    fn clear(&mut self) -> Result<(), Self::Error> {
        self.base_offset = 0;
        self.peak_indices.clear();
        self.peak_hashes.clear();
        self.hashes.clear();
        Ok(())
    }

    fn position(&self, item: &Self::Value) -> Result<Option<usize>, Self::Error> {
        for index in 0..self.len()? {
            if let Some(stored_item) = self.get(index)? {
                if stored_item == *item {
                    return Ok(Some(index));
                }
            }
        }
        Ok(None)
    }
}
