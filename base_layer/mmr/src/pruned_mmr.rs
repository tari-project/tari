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

use crate::{error::MerkleMountainRangeError, pruned_hashset::PrunedHashSet, ArrayLike, Hash, MerkleMountainRange};
use digest::Digest;
use serde::export::PhantomData;
use std::convert::TryFrom;

type PrunedMmr<D> = MerkleMountainRange<D, PrunedHashSet>;

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
