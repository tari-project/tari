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

use croaring::Bitmap;
use digest::Digest;
use support::{combine_hashes, create_mmr, int_to_hash, Hasher};
use tari_mmr::{Hash, HashSlice, MerkleMountainRange, MutableMmr, VectorBackend};
use tari_utilities::hex::Hex;

fn hash_with_bitmap(hash: &HashSlice, bitmap: &mut Bitmap) -> Hash {
    bitmap.run_optimize();
    let hasher = Hasher::new();
    hasher.chain(hash).chain(&bitmap.serialize()).result().to_vec()
}

/// MMRs with no elements should provide sane defaults. The merkle root must be the hash of an empty string, b"".
#[test]
fn zero_length_mmr() {
    let mmr = MutableMmr::<Hasher, _>::new(VectorBackend::default());
    assert_eq!(mmr.len(), 0);
    assert!(mmr.is_empty());
    let empty_hash = Hasher::digest(b"").to_vec();
    assert_eq!(
        mmr.get_merkle_root(),
        hash_with_bitmap(&empty_hash, &mut Bitmap::create())
    );
}

#[test]
// Note the hardcoded hashes are only valid when using Blake256 as the Hasher
fn delete() {
    let mut mmr = MutableMmr::<Hasher, _>::new(VectorBackend::default());
    assert!(mmr.is_empty());
    for i in 0..5 {
        assert!(mmr.push(&int_to_hash(i)).is_ok());
    }
    assert_eq!(mmr.len(), 5);
    let root = mmr.get_merkle_root();
    // Can't delete past bounds
    assert_eq!(mmr.delete_and_compress(5, true), false);
    assert_eq!(mmr.len(), 5);
    assert!(!mmr.is_empty());
    assert_eq!(mmr.get_merkle_root(), root);
    // Delete some nodes
    assert!(mmr.delete_and_compress(0, false));
    assert!(mmr.delete_and_compress(2, false));
    assert!(mmr.delete_and_compress(4, true));
    let root = mmr.get_merkle_root();
    assert_eq!(
        &root.to_hex(),
        "e749ef3a776f13003426520911474f412dfeddf3f16b9783df935c9b4a9eb51c"
    );
    assert_eq!(mmr.len(), 2);
    assert!(!mmr.is_empty());
    // Can't delete that which has already been deleted
    assert!(!mmr.delete_and_compress(0, false));
    assert!(!mmr.delete_and_compress(2, false));
    assert!(!mmr.delete_and_compress(0, true));
    // .. or beyond bounds of MMR
    assert!(!mmr.delete_and_compress(99, true));
    assert_eq!(mmr.len(), 2);
    assert!(!mmr.is_empty());
    // Merkle root should not have changed:
    assert_eq!(mmr.get_merkle_root(), root);
    assert!(mmr.delete_and_compress(1, false));
    assert!(mmr.delete(3));
    assert_eq!(mmr.len(), 0);
    assert!(mmr.is_empty());
    let root = mmr.get_merkle_root();
    assert_eq!(
        &root.to_hex(),
        "f7a7378dd83853047f889fad5bab3d5fb0f9c2864a89cbb6edcb1cb6897103db"
    );
}

/// Successively build up an MMR and check that the roots, heights and indices are all correct.
#[test]
fn build_mmr() {
    // Check the mutable MMR against a standard MMR and a roaring bitmap. Create one with 5 leaf nodes *8 MMR nodes)
    let mut mmr_check = create_mmr(5);
    assert_eq!(mmr_check.len(), 8);
    let mut bitmap = Bitmap::create();
    // Create a small mutable MMR
    let mut mmr = MutableMmr::<Hasher, _>::new(VectorBackend::default());
    for i in 0..5 {
        assert!(mmr.push(&int_to_hash(i)).is_ok());
    }
    // MutableMmr::len gives the size in terms of leaf nodes:
    assert_eq!(mmr.len(), 5);
    let mmr_root = mmr_check.get_merkle_root();
    let root_check = hash_with_bitmap(&mmr_root, &mut bitmap);
    assert_eq!(mmr.get_merkle_root(), root_check);
    // Delete a node
    assert!(mmr.delete_and_compress(3, true));
    bitmap.add(3);
    let root_check = hash_with_bitmap(&mmr_root, &mut bitmap);
    assert_eq!(mmr.get_merkle_root(), root_check);
}

#[test]
fn equality_check() {
    let mut ma = MutableMmr::<Hasher, _>::new(VectorBackend::default());
    let mut mb = MutableMmr::<Hasher, _>::new(VectorBackend::default());
    assert!(ma == mb);
    assert!(ma.push(&int_to_hash(1)).is_ok());
    assert!(ma != mb);
    assert!(mb.push(&int_to_hash(1)).is_ok());
    assert!(ma == mb);
    assert!(ma.push(&int_to_hash(2)).is_ok());
    assert!(ma != mb);
    assert!(ma.delete(1));
    // Even though the two trees have the same apparent elements, they're still not equal, because we don't actually
    // delete anything
    assert!(ma != mb);
    // Add the same hash to mb and then delete it
    assert!(mb.push(&int_to_hash(2)).is_ok());
    assert!(mb.delete(1));
    // Now they're equal!
    assert!(ma == mb);
}
