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

use croaring::Bitmap;
use tari_crypto::hash::blake2::Blake256;
use tari_mmr::{mmr_hash_domain, Hash, HashSlice, MerkleMountainRange, MutableMmr};

pub fn create_mmr(size: usize) -> MerkleMountainRange<Blake256, Vec<Hash>> {
    let mut mmr = MerkleMountainRange::<Blake256, _>::new(Vec::default());
    for i in 0..size {
        let hash = int_to_hash(i);
        assert!(mmr.push(hash).is_ok());
    }
    mmr
}

pub fn create_mutable_mmr(size: usize) -> MutableMmr<Blake256, Vec<Hash>> {
    let mut mmr = MutableMmr::<Blake256, _>::new(Vec::default(), Bitmap::create()).unwrap();
    for i in 0..size {
        let hash = int_to_hash(i);
        assert!(mmr.push(hash).is_ok());
    }
    mmr
}

pub fn int_to_hash(n: usize) -> Vec<u8> {
    mmr_hash_domain().digest::<Blake256>(&n.to_le_bytes()).as_ref().to_vec()
}

pub fn combine_hashes(hashe_slices: &[&HashSlice]) -> Hash {
    let hasher = mmr_hash_domain().hasher::<Blake256>();
    hashe_slices
        .iter()
        .fold(hasher, |hasher, h| hasher.chain(*h))
        .finalize()
        .as_ref()
        .to_vec()
}
