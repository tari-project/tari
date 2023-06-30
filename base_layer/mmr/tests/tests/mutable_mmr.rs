//  Copyright 2022. The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use croaring::Bitmap;
use tari_mmr::{common::LeafIndex, Hash, HashSlice};
use tari_utilities::hex::Hex;

use crate::support::{create_mmr, int_to_hash, MmrTestHasherBlake256, MutableTestMmr};

fn hash_with_bitmap(hash: &HashSlice, bitmap: &mut Bitmap) -> Hash {
    bitmap.run_optimize();
    let hasher = MmrTestHasherBlake256::new();
    hasher
        .chain(hash)
        .chain(bitmap.serialize())
        .finalize()
        .as_ref()
        .to_vec()
}

/// MMRs with no elements should provide sane defaults. The merkle root must be the hash of an empty string, b"".
#[test]
fn zero_length_mmr() {
    let mmr = MutableTestMmr::new(Vec::default(), Bitmap::create()).unwrap();
    assert_eq!(mmr.len(), 0);
    assert_eq!(mmr.is_empty(), Ok(true));
    let empty_hash = MmrTestHasherBlake256::new().digest(b"").as_ref().to_vec();
    assert_eq!(
        mmr.get_merkle_root(),
        Ok(hash_with_bitmap(&empty_hash, &mut Bitmap::create()))
    );
}

#[test]
// Note the hardcoded hashes are only valid when using MutableTestMmr
fn delete() {
    let mut mmr = MutableTestMmr::new(Vec::default(), Bitmap::create()).unwrap();
    assert_eq!(mmr.is_empty(), Ok(true));
    for i in 0..5 {
        assert!(mmr.push(int_to_hash(i)).is_ok());
    }
    assert_eq!(mmr.len(), 5);
    let root = mmr.get_merkle_root().unwrap();
    assert_eq!(
        &root.to_hex(),
        "23affc8202916d88fdb991b22c6975ce4bdb69661c40257580a0c4f6147925d7"
    );
    // Can't delete past bounds
    assert!(!mmr.delete(5));
    assert_eq!(mmr.len(), 5);
    assert_eq!(mmr.is_empty(), Ok(false));
    assert_eq!(mmr.get_merkle_root(), Ok(root));
    // Delete some nodes
    assert!(mmr.push(int_to_hash(5)).is_ok());
    assert!(mmr.delete(0));
    assert!(mmr.delete(2));
    assert!(mmr.delete(4));
    let root = mmr.get_merkle_root().unwrap();
    assert_eq!(
        &root.to_hex(),
        "9418eecd5f30ae1d892024e068c18013bb4f79f584c9aa5fdba818f9bd40da1e"
    );
    assert_eq!(mmr.len(), 3);
    assert_eq!(mmr.is_empty(), Ok(false));
    // Can't delete that which has already been deleted
    assert!(!mmr.delete(0,));
    assert!(!mmr.delete(2,));
    assert!(!mmr.delete(0,));
    // .. or beyond bounds of MMR
    assert!(!mmr.delete(9));
    assert_eq!(mmr.len(), 3);
    assert_eq!(mmr.is_empty(), Ok(false));
    // Merkle root should not have changed:
    assert_eq!(mmr.get_merkle_root(), Ok(root));
    assert!(mmr.delete(1));
    assert!(mmr.delete(5));
    assert!(mmr.delete(3));
    assert_eq!(mmr.len(), 0);
    assert_eq!(mmr.is_empty(), Ok(true));
    mmr.compress();
    let root = mmr.get_merkle_root().unwrap();
    assert_eq!(
        &root.to_hex(),
        "8bdcad274c1677d94037137492185541d3ae259a863df8a4d71ac665644b78ef"
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
    let mut mmr = MutableTestMmr::new(Vec::default(), Bitmap::create()).unwrap();
    for i in 0..5 {
        assert!(mmr.push(int_to_hash(i)).is_ok());
    }
    // MutableMmr::len gives the size in terms of leaf nodes:
    assert_eq!(mmr.len(), 5);
    let mmr_root = mmr_check.get_merkle_root().unwrap();
    let root_check = hash_with_bitmap(&mmr_root, &mut bitmap);
    assert_eq!(mmr.get_merkle_root(), Ok(root_check));
    // Delete a node
    assert!(mmr.delete(3));
    bitmap.add(3);
    let root_check = hash_with_bitmap(&mmr_root, &mut bitmap);
    assert_eq!(mmr.get_merkle_root(), Ok(root_check));
}

#[test]
fn equality_check() {
    let mut ma = MutableTestMmr::new(Vec::default(), Bitmap::create()).unwrap();
    let mut mb = MutableTestMmr::new(Vec::default(), Bitmap::create()).unwrap();
    assert!(ma == mb);
    assert!(ma.push(int_to_hash(1)).is_ok());
    assert!(ma != mb);
    assert!(mb.push(int_to_hash(1)).is_ok());
    assert!(ma == mb);
    assert!(ma.push(int_to_hash(2)).is_ok());
    assert!(ma != mb);
    assert!(ma.delete(1));
    // Even though the two trees have the same apparent elements, they're still not equal, because we don't actually
    // delete anything
    assert!(ma != mb);
    // Add the same hash to mb and then delete it
    assert!(mb.push(int_to_hash(2)).is_ok());
    assert!(mb.delete(1));
    // Now they're equal!
    assert!(ma == mb);
}

#[test]
fn restore_from_leaf_nodes() {
    let mut mmr = MutableTestMmr::new(Vec::default(), Bitmap::create()).unwrap();
    for i in 0..12 {
        assert!(mmr.push(int_to_hash(i)).is_ok());
    }
    assert!(mmr.delete(2));
    assert!(mmr.delete(4));
    assert!(mmr.delete(5));

    // Request state of MMR with single call
    let leaf_count = mmr.get_leaf_count();
    let mmr_state1 = mmr.to_leaf_nodes(LeafIndex(0), leaf_count).unwrap();

    // Request state of MMR with multiple calls
    let mut mmr_state2 = mmr.to_leaf_nodes(LeafIndex(0), 3).unwrap();
    mmr_state2.combine(mmr.to_leaf_nodes(LeafIndex(3), 3).unwrap());
    mmr_state2.combine(mmr.to_leaf_nodes(LeafIndex(6), leaf_count - 6).unwrap());
    assert_eq!(mmr_state1, mmr_state2);

    // Change the state more before the restore
    let mmr_root = mmr.get_merkle_root();
    assert!(mmr.push(int_to_hash(7)).is_ok());
    assert!(mmr.push(int_to_hash(8)).is_ok());
    assert!(mmr.delete(3));

    // Restore from compact state
    assert!(mmr.assign(mmr_state1).is_ok());
    assert_eq!(mmr.get_merkle_root(), mmr_root);
    let restored_mmr_state = mmr.to_leaf_nodes(LeafIndex(0), mmr.get_leaf_count()).unwrap();
    assert_eq!(restored_mmr_state, mmr_state2);
}
