// Copyright 2019 The Tari Project
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

use crate::support::{hashvalues::HashValues, testobject::TestObject};
use blake2::Blake2b;
use merklemountainrange::{
    merkleproof::MerkleProof,
    mmr::{self, *},
};
use tari_utilities::hex::*;

fn create_mmr(leaves: u32) -> MerkleMountainRange<TestObject, Blake2b> {
    let mut mmr: MerkleMountainRange<TestObject, Blake2b> = MerkleMountainRange::new();
    for i in 1..leaves + 1 {
        let object: TestObject = TestObject::new(i.to_string());
        mmr.push(object);
    }
    mmr
}
#[test]
fn create_small_mmr() {
    let mut mmr = create_mmr(2);
    assert_eq!(1, mmr.get_peak_height());
    let hash_values = HashValues::new();
    let hash0 = mmr.get_node_hash(0).unwrap();
    let proof = mmr.get_hash_proof(&hash0);
    let mut our_proof = MerkleProof::new();
    for i in 0..3 {
        our_proof.push(mmr.get_node_hash(i));
    }
    assert_eq!(hash_values.create_merkleproof(vec![0, 1, 2]), proof);
    assert_eq!(mmr.verify_proof(&our_proof), true);
    assert_eq!(mmr.get_merkle_root(), mmr.get_node_hash(2).unwrap());
    // test pruning
    assert_eq!(mmr.get_data_object(hash0.clone()).unwrap().pruned, false);
    assert_eq!(mmr.prune_object_hash(hash0.clone()).is_ok(), true);
    assert_eq!(mmr.get_data_object(hash0.clone()).unwrap().pruned, true);

    let hash1 = mmr.get_node_hash(1).unwrap();
    assert_eq!(mmr.get_data_object(hash1.clone()).unwrap().pruned, false);
    assert_eq!(mmr.prune_object_hash(hash1.clone()).is_ok(), true);
    // both are now pruned, thus deleted
    assert_eq!(mmr.get_data_object(hash1).is_none(), true);
    assert_eq!(mmr.get_data_object(hash0).is_none(), true);
}
#[test]
fn create_mmr_with_2_peaks() {
    let mmr = create_mmr(20);
    assert_eq!(4, mmr.get_peak_height());
    let hash_values = HashValues::new();
    let hash0 = mmr.get_node_hash(0).unwrap();
    let proof = mmr.get_hash_proof(&hash0);
    let our_proof = hash_values.create_merkleproof(vec![0, 1, -1, 5, -1, 13, -1, 29, -1, 37, 42]);
    assert_eq!(proof, our_proof);
    assert_eq!(mmr.verify_proof(&proof), true);
    let proof = mmr.get_hash_proof(&mmr.get_node_hash(31).unwrap());
    let our_proof = hash_values.create_merkleproof(vec![31, 32, -1, 36, 30, -1, 42]);
    assert_eq!(proof, our_proof);
    assert_eq!(mmr.verify_proof(&proof), true);

    let proof = mmr.get_hash_proof(&mmr.get_node_hash(1).unwrap());
    let our_proof = hash_values.create_merkleproof(vec![0, 1, -1, 5, -1, 13, -1, 29, -1, 37, 42]);
    assert_eq!(proof, our_proof);
    assert_eq!(mmr.verify_proof(&proof), true);

    // test some more proofs
    let proof = mmr.get_hash_proof(&mmr.get_node_hash(6).unwrap());
    let our_proof = hash_values.create_merkleproof(vec![6, 13, -1, 29, -1, 37, 42]);
    assert_eq!(proof, our_proof);
    assert_eq!(mmr.verify_proof(&proof), true);

    let proof = mmr.get_hash_proof(&mmr.get_node_hash(22).unwrap());
    let our_proof = hash_values.create_merkleproof(vec![22, 23, -1, 27, 21, -1, 14, -1, -1, 37, 42]);
    assert_eq!(proof, our_proof);
    assert_eq!(mmr.verify_proof(&proof), true);

    let proof = mmr.get_hash_proof(&mmr.get_node_hash(26).unwrap());
    let our_proof = hash_values.create_merkleproof(vec![25, 26, 24, -1, 21, -1, 14, -1, -1, 37, 42]);
    assert_eq!(proof, our_proof);
    assert_eq!(mmr.verify_proof(&proof), true);

    let proof = mmr.get_hash_proof(&mmr.get_node_hash(14).unwrap());
    let our_proof = hash_values.create_merkleproof(vec![14, 29, -1, 37, 42]);
    assert_eq!(proof, our_proof);
    assert_eq!(mmr.verify_proof(&proof), true);

    let proof = mmr.get_hash_proof(&mmr.get_node_hash(11).unwrap());
    let our_proof = hash_values.create_merkleproof(vec![10, 11, 9, -1, 6, -1, -1, 29, -1, 37, 42]);
    assert_eq!(proof, our_proof);
    assert_eq!(mmr.verify_proof(&proof), true);

    assert_eq!(to_hex(&mmr.get_merkle_root()), hash_values.get_value(42));
}

#[test]
fn mmr_with_3_peaks() {
    let mmr = create_mmr(21);
    assert_eq!(4, mmr.get_peak_height());

    let hash_values = HashValues::new();
    let proof = mmr.get_hash_proof(&mmr.get_node_hash(35).unwrap());
    let our_proof = hash_values.create_merkleproof(vec![34, 35, 33, -1, -1, 38, 30, -1, 44]);
    assert_eq!(proof, our_proof);
    assert_eq!(mmr.verify_proof(&proof), true);

    let proof = mmr.get_hash_proof(&mmr.get_node_hash(38).unwrap());
    let our_proof = hash_values.create_merkleproof(vec![37, 38, 30, -1, 44]);
    assert_eq!(proof, our_proof);
    assert_eq!(mmr.verify_proof(&proof), true);

    let proof = mmr.get_hash_proof(&mmr.get_node_hash(0).unwrap());
    let our_proof = hash_values.create_merkleproof(vec![0, 1, -1, 5, -1, 13, -1, 29, -1, 43, 44]);
    assert_eq!(proof, our_proof);
    assert_eq!(mmr.verify_proof(&proof), true);

    assert_eq!(to_hex(&mmr.get_merkle_root()), hash_values.get_value(44));
}

#[test]
fn mmr_with_4_peaks() {
    let mmr = create_mmr(23);
    assert_eq!(4, mmr.get_peak_height());
    let mut raw = Vec::new();
    for i in 0..42 {
        raw.push(mmr.get_node_hash(i).unwrap());
    }
    let hash_values = HashValues::new();
    assert_eq!(to_hex(&mmr.get_merkle_root()), hash_values.get_value(47));
    let proof = mmr.get_hash_proof(&mmr.get_node_hash(35).unwrap());
    let our_proof = hash_values.create_merkleproof(vec![34, 35, 33, -1, -1, 45, 30, -1, 47]);
    assert_eq!(proof, our_proof);
    assert_eq!(mmr.verify_proof(&proof), true);
    let proof = mmr.get_hash_proof(&mmr.get_node_hash(34).unwrap());
    let our_proof = hash_values.create_merkleproof(vec![34, 35, 33, -1, -1, 45, 30, -1, 47]);
    assert_eq!(proof, our_proof);
    assert_eq!(mmr.verify_proof(&proof), true);
    let proof = mmr.get_hash_proof(&mmr.get_node_hash(21).unwrap());
    let our_proof = hash_values.create_merkleproof(vec![21, 28, 14, -1, -1, 46, 47]);
    assert_eq!(proof, our_proof);
    assert_eq!(mmr.verify_proof(&proof), true);
    let proof = mmr.get_hash_proof(&mmr.get_node_hash(41).unwrap());
    let our_proof = hash_values.create_merkleproof(vec![40, 41, 37, -1, 30, -1, 47]);
    assert_eq!(proof, our_proof);
    assert_eq!(mmr.verify_proof(&proof), true);

    let proof = mmr.get_hash_proof(&mmr.get_node_hash(0).unwrap());
    let our_proof = hash_values.create_merkleproof(vec![0, 1, -1, 5, -1, 13, -1, 29, -1, 46, 47]);
    assert_eq!(proof, our_proof);
    assert_eq!(mmr.verify_proof(&proof), true);

    let proof = mmr.get_hash_proof(&mmr.get_node_hash(1).unwrap());
    let our_proof = hash_values.create_merkleproof(vec![0, 1, -1, 5, -1, 13, -1, 29, -1, 46, 47]);
    assert_eq!(proof, our_proof);
    assert_eq!(mmr.verify_proof(&proof), true);

    let proof = mmr.get_hash_proof(&mmr.get_node_hash(21).unwrap());
    let our_proof = hash_values.create_merkleproof(vec![21, 28, 14, -1, -1, 46, 47]);
    assert_eq!(proof, our_proof);
    assert_eq!(mmr.verify_proof(&proof), true);

    let proof = mmr.get_hash_proof(&mmr.get_node_hash(28).unwrap());
    let our_proof = hash_values.create_merkleproof(vec![21, 28, 14, -1, -1, 46, 47]);
    assert_eq!(proof, our_proof);
    assert_eq!(mmr.verify_proof(&proof), true);
}

#[test]
fn very_large_mmr() {
    // test test only tests that it doesn't crash currently, we need to create fuzz testing to test this properly
    let mmr = create_mmr(23000);
    let _merkle_root = mmr.get_merkle_root();
    let proof = mmr.get_hash_proof(&mmr.get_node_hash(1).unwrap());
    assert_eq!(mmr.verify_proof(&proof), true);
}

#[test]
fn test_node_sides() {
    // test some true
    assert_eq!(mmr::is_node_right(11), true);
    assert_eq!(mmr::is_node_right(20), true);
    assert_eq!(mmr::is_node_right(35), true);
    assert_eq!(mmr::is_node_right(36), true);
    assert_eq!(mmr::is_node_right(29), true);
    assert_eq!(mmr::is_node_right(13), true);
    assert_eq!(mmr::is_node_right(1), true);
    assert_eq!(mmr::is_node_right(28), true);
    assert_eq!(mmr::is_node_right(5), true);
    assert_eq!(mmr::is_node_right(23), true);
    // test some false
    assert_eq!(mmr::is_node_right(0), false);
    assert_eq!(mmr::is_node_right(34), false);
    assert_eq!(mmr::is_node_right(21), false);
    assert_eq!(mmr::is_node_right(7), false);
    assert_eq!(mmr::is_node_right(34), false);
    assert_eq!(mmr::is_node_right(14), false);
    assert_eq!(mmr::is_node_right(10), false);
    assert_eq!(mmr::is_node_right(30), false);
    assert_eq!(mmr::is_node_right(15), false);
    assert_eq!(mmr::is_node_right(37), false);
}

#[test]
fn test_node_heights() {
    // test some 0
    assert_eq!(mmr::get_node_height(11), 0);
    assert_eq!(mmr::get_node_height(10), 0);
    assert_eq!(mmr::get_node_height(0), 0);
    assert_eq!(mmr::get_node_height(11), 0);
    assert_eq!(mmr::get_node_height(1), 0);
    assert_eq!(mmr::get_node_height(16), 0);
    assert_eq!(mmr::get_node_height(23), 0);
    assert_eq!(mmr::get_node_height(35), 0);
    assert_eq!(mmr::get_node_height(32), 0);
    assert_eq!(mmr::get_node_height(34), 0);
    assert_eq!(mmr::get_node_height(19), 0);
    assert_eq!(mmr::get_node_height(8), 0);

    // test some 1
    assert_eq!(mmr::get_node_height(2), 1);
    assert_eq!(mmr::get_node_height(5), 1);
    assert_eq!(mmr::get_node_height(20), 1);
    assert_eq!(mmr::get_node_height(27), 1);
    assert_eq!(mmr::get_node_height(36), 1);

    // some larger
    assert_eq!(mmr::get_node_height(6), 2);
    assert_eq!(mmr::get_node_height(13), 2);
    assert_eq!(mmr::get_node_height(21), 2);
    assert_eq!(mmr::get_node_height(37), 2);
    assert_eq!(mmr::get_node_height(14), 3);
    assert_eq!(mmr::get_node_height(29), 3);
    assert_eq!(mmr::get_node_height(30), 4);
    assert_eq!(mmr::get_node_height(62), 5);
}

#[test]
fn get_object_index() {
    assert_eq!(mmr::get_object_index(10), 18);
    assert_eq!(mmr::get_object_index(0), 0);
    assert_eq!(mmr::get_object_index(3), 4);
    assert_eq!(mmr::get_object_index(11), 19);
    assert_eq!(mmr::get_object_index(16), 31);
    assert_eq!(mmr::get_object_index(17), 32);
    assert_eq!(mmr::get_object_index(6), 10);
    assert_eq!(mmr::get_object_index(1), 1);
    assert_eq!(mmr::get_object_index(12), 22);
}
