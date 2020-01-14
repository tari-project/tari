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
use support::{create_mmr, int_to_hash, Hasher};
use tari_mmr::{Hash, HashSlice, MutableMmr};
use tari_utilities::hex::Hex;

fn hash_with_bitmap(hash: &HashSlice, bitmap: &mut Bitmap) -> Hash {
    bitmap.run_optimize();
    let hasher = Hasher::new();
    hasher.chain(hash).chain(&bitmap.serialize()).result().to_vec()
}

/// MMRs with no elements should provide sane defaults. The merkle root must be the hash of an empty string, b"".
#[test]
fn zero_length_mmr() {
    let mmr = MutableMmr::<Hasher, _>::new(Vec::default(), Bitmap::create());
    assert_eq!(mmr.len(), 0);
    assert_eq!(mmr.is_empty(), Ok(true));
    let empty_hash = Hasher::digest(b"").to_vec();
    assert_eq!(
        mmr.get_merkle_root(),
        Ok(hash_with_bitmap(&empty_hash, &mut Bitmap::create()))
    );
}

#[test]
// Note the hardcoded hashes are only valid when using Blake256 as the Hasher
fn delete() {
    let mut mmr = MutableMmr::<Hasher, _>::new(Vec::default(), Bitmap::create());
    assert_eq!(mmr.is_empty(), Ok(true));
    for i in 0..5 {
        assert!(mmr.push(&int_to_hash(i)).is_ok());
    }
    assert_eq!(mmr.len(), 5);
    let root = mmr.get_merkle_root().unwrap();
    assert_eq!(
        &root.to_hex(),
        "7b7ddec2af4f3d0b9b165750cf2ff15813e965d29ecd5318e0c8fea901ceaef4"
    );
    // Can't delete past bounds
    assert_eq!(mmr.delete_and_compress(5, true), false);
    assert_eq!(mmr.len(), 5);
    assert_eq!(mmr.is_empty(), Ok(false));
    assert_eq!(mmr.get_merkle_root(), Ok(root));
    // Delete some nodes
    assert!(mmr.push(&int_to_hash(5)).is_ok());
    assert!(mmr.delete_and_compress(0, false));
    assert!(mmr.delete_and_compress(2, false));
    assert!(mmr.delete_and_compress(4, true));
    let root = mmr.get_merkle_root().unwrap();
    assert_eq!(
        &root.to_hex(),
        "69e69ba0c6222f2d9caa68282de0ba7f1259a0fa2b8d84af68f907ef4ec05054"
    );
    assert_eq!(mmr.len(), 3);
    assert_eq!(mmr.is_empty(), Ok(false));
    // Can't delete that which has already been deleted
    assert!(!mmr.delete_and_compress(0, false));
    assert!(!mmr.delete_and_compress(2, false));
    assert!(!mmr.delete_and_compress(0, true));
    // .. or beyond bounds of MMR
    assert!(!mmr.delete_and_compress(99, true));
    assert_eq!(mmr.len(), 3);
    assert_eq!(mmr.is_empty(), Ok(false));
    // Merkle root should not have changed:
    assert_eq!(mmr.get_merkle_root(), Ok(root));
    assert!(mmr.delete_and_compress(1, false));
    assert!(mmr.delete_and_compress(5, false));
    assert!(mmr.delete(3));
    assert_eq!(mmr.len(), 0);
    assert_eq!(mmr.is_empty(), Ok(true));
    let root = mmr.get_merkle_root().unwrap();
    assert_eq!(
        &root.to_hex(),
        "2a540797d919e63cff8051e54ae13197315000bcfde53efd3f711bb3d24995bc"
    );
}

/// Successively build up an MMR and check that the roots, heights and indices are all correct.
#[test]
fn build_mmr() {
    // Check the mutable MMR against a standard MMR and a roaring bitmap. Create one with 5 leaf nodes *8 MMR nodes)
    let mmr_check = create_mmr(5);
    assert_eq!(mmr_check.len(), Ok(8));
    let mut bitmap = Bitmap::create();
    // Create a small mutable MMR
    let mut mmr = MutableMmr::<Hasher, _>::new(Vec::default(), Bitmap::create());
    for i in 0..5 {
        assert!(mmr.push(&int_to_hash(i)).is_ok());
    }
    // MutableMmr::len gives the size in terms of leaf nodes:
    assert_eq!(mmr.len(), 5);
    let mmr_root = mmr_check.get_merkle_root().unwrap();
    let root_check = hash_with_bitmap(&mmr_root, &mut bitmap);
    assert_eq!(mmr.get_merkle_root(), Ok(root_check));
    // Delete a node
    assert!(mmr.delete_and_compress(3, true));
    bitmap.add(3);
    let root_check = hash_with_bitmap(&mmr_root, &mut bitmap);
    assert_eq!(mmr.get_merkle_root(), Ok(root_check));
}

#[test]
fn equality_check() {
    let mut ma = MutableMmr::<Hasher, _>::new(Vec::default(), Bitmap::create());
    let mut mb = MutableMmr::<Hasher, _>::new(Vec::default(), Bitmap::create());
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

#[test]
fn restore_from_leaf_nodes() {
    let mut mmr = MutableMmr::<Hasher, _>::new(Vec::default(), Bitmap::create());
    for i in 0..12 {
        assert!(mmr.push(&int_to_hash(i)).is_ok());
    }
    assert!(mmr.delete_and_compress(2, true));
    assert!(mmr.delete_and_compress(4, true));
    assert!(mmr.delete_and_compress(5, true));

    // Request state of MMR with single call
    let leaf_count = mmr.get_leaf_count();
    let mmr_state1 = mmr.to_leaf_nodes(0, leaf_count).unwrap();

    // Request state of MMR with multiple calls
    let mut mmr_state2 = mmr.to_leaf_nodes(0, 3).unwrap();
    mmr_state2.combine(mmr.to_leaf_nodes(3, 3).unwrap());
    mmr_state2.combine(mmr.to_leaf_nodes(6, leaf_count - 6).unwrap());
    assert_eq!(mmr_state1, mmr_state2);

    // Change the state more before the restore
    let mmr_root = mmr.get_merkle_root();
    assert!(mmr.push(&int_to_hash(7)).is_ok());
    assert!(mmr.push(&int_to_hash(8)).is_ok());
    assert!(mmr.delete_and_compress(3, true));

    // Restore from compact state
    assert!(mmr.assign(mmr_state1.clone()).is_ok());
    assert_eq!(mmr.get_merkle_root(), mmr_root);
    let restored_mmr_state = mmr.to_leaf_nodes(0, mmr.get_leaf_count()).unwrap();
    assert_eq!(restored_mmr_state, mmr_state2);
}
