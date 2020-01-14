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
use digest::Digest;
use tari_crypto::common::Blake256;
use tari_mmr::{Hash, HashSlice, MerkleMountainRange, MutableMmr};

pub type Hasher = Blake256;

pub fn create_mmr(size: usize) -> MerkleMountainRange<Hasher, Vec<Hash>> {
    let mut mmr = MerkleMountainRange::<Hasher, _>::new(Vec::default());
    for i in 0..size {
        let hash = int_to_hash(i);
        assert!(mmr.push(&hash).is_ok());
    }
    mmr
}

pub fn create_mutable_mmr(size: usize) -> MutableMmr<Hasher, Vec<Hash>> {
    let mut mmr = MutableMmr::<Hasher, _>::new(Vec::default(), Bitmap::create());
    for i in 0..size {
        let hash = int_to_hash(i);
        assert!(mmr.push(&hash).is_ok());
    }
    mmr
}

pub fn int_to_hash(n: usize) -> Vec<u8> {
    Hasher::digest(&n.to_le_bytes()).to_vec()
}

pub fn combine_hashes(hashes: &[&HashSlice]) -> Hash {
    let hasher = Hasher::new();
    hashes
        .iter()
        .fold(hasher, |hasher, h| hasher.chain(*h))
        .result()
        .to_vec()
}
