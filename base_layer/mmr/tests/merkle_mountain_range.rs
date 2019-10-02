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

mod support;

use digest::Digest;
use support::{combine_hashes, create_mmr, int_to_hash, Hasher};
use tari_mmr::MerkleMountainRange;

/// MMRs with no elements should provide sane defaults. The merkle root must be the hash of an empty string, b"".
#[test]
fn zero_length_mmr() {
    let mmr = MerkleMountainRange::<Hasher, _>::new(Vec::default());
    assert_eq!(mmr.len(), Ok(0));
    assert_eq!(mmr.is_empty(), Ok(true));
    let empty_hash = Hasher::digest(b"").to_vec();
    assert_eq!(mmr.get_merkle_root(), Ok(empty_hash));
}

/// Successively build up an MMR and check that the roots, heights and indices are all correct.
#[test]
fn build_mmr() {
    let mut mmr = MerkleMountainRange::<Hasher, _>::new(Vec::default());
    // Add a single item
    let h0 = int_to_hash(0);

    assert!(mmr.push(&h0).is_ok());
    // The root of a single hash is the hash of that hash
    assert_eq!(mmr.len(), Ok(1));
    assert_eq!(mmr.get_merkle_root(), Ok(combine_hashes(&[&h0])));
    // Two leaf item items:
    //    2
    //  0   1
    let h1 = int_to_hash(1);
    assert!(mmr.push(&h1).is_ok());
    let h_2 = combine_hashes(&[&h0, &h1]);
    assert_eq!(mmr.get_merkle_root(), Ok(combine_hashes(&[&h_2])));
    assert_eq!(mmr.len(), Ok(3));
    // Three leaf item items:
    //    2
    //  0   1  3
    let h3 = int_to_hash(3);
    assert!(mmr.push(&h3).is_ok());
    // The root is a bagged root
    let root = combine_hashes(&[&h_2, &h3]);
    assert_eq!(mmr.get_merkle_root(), Ok(root));
    assert_eq!(mmr.len(), Ok(4));
    // Four leaf items:
    //        6
    //    2      5
    //  0   1  3   4
    let h4 = int_to_hash(4);
    assert!(mmr.push(&h4).is_ok());
    let h_5 = combine_hashes(&[&h3, &h4]);
    let h_6 = combine_hashes(&[&h_2, &h_5]);
    assert_eq!(mmr.get_merkle_root(), Ok(combine_hashes(&[&h_6])));
    assert_eq!(mmr.len(), Ok(7));
    // Five leaf items:
    //        6
    //    2      5
    //  0   1  3   4  7
    let h7 = int_to_hash(7);
    assert!(mmr.push(&h7).is_ok());
    let root = combine_hashes(&[&h_6, &h7]);
    assert_eq!(mmr.get_merkle_root(), Ok(root));
    assert_eq!(mmr.len(), Ok(8));
    // Six leaf item items:
    //        6
    //    2      5      9
    //  0   1  3   4  7  8
    let h8 = int_to_hash(8);
    let h_9 = combine_hashes(&[&h7, &h8]);
    assert!(mmr.push(&h8).is_ok());
    let root = combine_hashes(&[&h_6, &h_9]);
    assert_eq!(mmr.get_merkle_root(), Ok(root));
    assert_eq!(mmr.len(), Ok(10));
}

#[test]
fn equality_check() {
    let mut ma = MerkleMountainRange::<Hasher, _>::new(Vec::default());
    let mut mb = MerkleMountainRange::<Hasher, _>::new(Vec::default());
    assert!(ma == mb);
    assert!(ma.push(&int_to_hash(1)).is_ok());
    assert!(ma != mb);
    assert!(mb.push(&int_to_hash(1)).is_ok());
    assert!(ma == mb);
    assert!(ma.push(&int_to_hash(2)).is_ok());
    assert!(mb.push(&int_to_hash(3)).is_ok());
    assert!(ma != mb);
}

#[test]
fn validate() {
    let mmr = create_mmr(65);
    assert!(mmr.validate().is_ok());
}
