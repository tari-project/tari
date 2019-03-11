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
use merklemountainrange::merklemountainrange::*;

fn create_mmr(leaves: u32) -> MerkleMountainRange<TestObject<Blake2b>, Blake2b> {
    let mut mmr: MerkleMountainRange<TestObject<Blake2b>, Blake2b> = MerkleMountainRange::new();
    for i in 1..leaves + 1 {
        let object: TestObject<Blake2b> = TestObject::new(i.to_string());
        mmr.add_single(object);
    }
    mmr
}

#[test]
fn create_small_mmr() {
    let mmr = create_mmr(2);
    assert_eq!(1, mmr.get_peak_height());
    let hash_values = HashValues::new();
    let hash0 = mmr.get_hash(0).unwrap();
    let proof = mmr.get_hash_proof(&hash0);
    let mut our_proof = Vec::new();
    for i in 0..3 {
        our_proof.push(mmr.get_hash(i).unwrap());
    }
    assert_eq!(hash_values.get_slice(0, 2), HashValues::to_hex_multiple(&proof));
    assert_eq!(mmr.verify_proof(&our_proof), true);
    assert_eq!(mmr.get_merkle_root(), mmr.get_hash(2).unwrap())
}

#[test]
fn create_mmr_with_2_peaks() {
    let mmr = create_mmr(20);
    assert_eq!(4, mmr.get_peak_height());
    let hash_values = HashValues::new();

    let hash0 = mmr.get_hash(0).unwrap();
    let proof = mmr.get_hash_proof(&hash0);
    let our_proof = hash_values.get_indexes(vec![0, 1, 2, 5, 6, 13, 14, 29, 30, 37, 42]);
    assert_eq!(HashValues::to_hex_multiple(&proof), our_proof);
    let proof = mmr.get_hash_proof(&mmr.get_hash(1).unwrap());
    let our_proof = hash_values.get_indexes(vec![0, 1, 2, 5, 6, 13, 14, 29, 30, 37, 42]);
    assert_eq!(HashValues::to_hex_multiple(&proof), our_proof);

    // test some more proofs
    let proof = mmr.get_hash_proof(&mmr.get_hash(6).unwrap());
    let our_proof = hash_values.get_indexes(vec![6, 13, 14, 29, 30, 37, 42]);
    assert_eq!(HashValues::to_hex_multiple(&proof), our_proof);

    let proof = mmr.get_hash_proof(&mmr.get_hash(22).unwrap());
    let our_proof = hash_values.get_indexes(vec![22, 23, 24, 27, 21, 28, 14, 29, 30, 37, 42]);
    assert_eq!(HashValues::to_hex_multiple(&proof), our_proof);

    let proof = mmr.get_hash_proof(&mmr.get_hash(26).unwrap());
    let our_proof = hash_values.get_indexes(vec![25, 26, 24, 27, 21, 28, 14, 29, 30, 37, 42]);
    assert_eq!(HashValues::to_hex_multiple(&proof), our_proof);

    let proof = mmr.get_hash_proof(&mmr.get_hash(14).unwrap());
    let our_proof = hash_values.get_indexes(vec![14, 29, 30, 37, 42]);
    assert_eq!(HashValues::to_hex_multiple(&proof), our_proof);

    let proof = mmr.get_hash_proof(&mmr.get_hash(11).unwrap());
    let our_proof = hash_values.get_indexes(vec![10, 11, 9, 12, 6, 13, 14, 29, 30, 37, 42]);
    assert_eq!(HashValues::to_hex_multiple(&proof), our_proof);

    assert_eq!(HashValues::to_hex(&mmr.get_merkle_root()), hash_values.get_value(42));
}

#[test]
fn mmr_with_3_peaks() {
    let mmr = create_mmr(21);
    assert_eq!(4, mmr.get_peak_height());
    let mut raw = Vec::new();
    for i in 0..39 {
        raw.push(mmr.get_hash(i).unwrap());
    }
    let hash_values = HashValues::new();
    let proof = mmr.get_hash_proof(&mmr.get_hash(35).unwrap());
    let our_proof = hash_values.get_indexes(vec![34, 35, 33, 36, 37, 38, 30, 43, 44]);
    assert_eq!(HashValues::to_hex_multiple(&proof), our_proof);

    assert_eq!(HashValues::to_hex(&mmr.get_merkle_root()), hash_values.get_value(44));
}

#[test]
fn mmr_with_4_peaks() {
    let mmr = create_mmr(23);
    assert_eq!(4, mmr.get_peak_height());
    let mut raw = Vec::new();
    for i in 0..42 {
        raw.push(mmr.get_hash(i).unwrap());
    }
    let hash_values = HashValues::new();
    assert_eq!(HashValues::to_hex(&mmr.get_merkle_root()), hash_values.get_value(47));

    let proof = mmr.get_hash_proof(&mmr.get_hash(35).unwrap());
    let our_proof = hash_values.get_indexes(vec![34, 35, 33, 36, 37, 45, 30, 46, 47]);
    assert_eq!(HashValues::to_hex_multiple(&proof), our_proof);

    let proof = mmr.get_hash_proof(&mmr.get_hash(34).unwrap());
    let our_proof = hash_values.get_indexes(vec![34, 35, 33, 36, 37, 45, 30, 46, 47]);
    assert_eq!(HashValues::to_hex_multiple(&proof), our_proof);

    let proof = mmr.get_hash_proof(&mmr.get_hash(21).unwrap());
    let our_proof = hash_values.get_indexes(vec![21, 28, 14, 29,30, 46, 47]);
    assert_eq!(HashValues::to_hex_multiple(&proof), our_proof);

    let proof = mmr.get_hash_proof(&mmr.get_hash(41).unwrap());
    let our_proof = hash_values.get_indexes(vec![40, 41, 37, 45, 30, 46, 47]);
    assert_eq!(HashValues::to_hex_multiple(&proof), our_proof);

    let proof = mmr.get_hash_proof(&mmr.get_hash(0).unwrap());
    let our_proof = hash_values.get_indexes(vec![0, 1, 2, 5, 6, 13, 14, 29, 30, 46, 47]);
    assert_eq!(HashValues::to_hex_multiple(&proof), our_proof);

    let proof = mmr.get_hash_proof(&mmr.get_hash(1).unwrap());
    let our_proof = hash_values.get_indexes(vec![0, 1, 2, 5, 6, 13, 14, 29, 30, 46, 47]);
    assert_eq!(HashValues::to_hex_multiple(&proof), our_proof);

    let proof = mmr.get_hash_proof(&mmr.get_hash(21).unwrap());
    let our_proof = hash_values.get_indexes(vec![21, 28, 14, 29, 30, 46, 47]);
    assert_eq!(HashValues::to_hex_multiple(&proof), our_proof);

    let proof = mmr.get_hash_proof(&mmr.get_hash(28).unwrap());
    let our_proof = hash_values.get_indexes(vec![21, 28, 14, 29, 30, 46, 47]);
    assert_eq!(HashValues::to_hex_multiple(&proof), our_proof);
}

#[test]
fn test_node_sides() {
    // test some true
    assert_eq!(merklemountainrange::merklemountainrange::is_node_right(11), true);
    assert_eq!(merklemountainrange::merklemountainrange::is_node_right(20), true);
    assert_eq!(merklemountainrange::merklemountainrange::is_node_right(35), true);
    assert_eq!(merklemountainrange::merklemountainrange::is_node_right(36), true);
    assert_eq!(merklemountainrange::merklemountainrange::is_node_right(29), true);
    assert_eq!(merklemountainrange::merklemountainrange::is_node_right(13), true);
    assert_eq!(merklemountainrange::merklemountainrange::is_node_right(1), true);
    assert_eq!(merklemountainrange::merklemountainrange::is_node_right(28), true);
    assert_eq!(merklemountainrange::merklemountainrange::is_node_right(5), true);
    assert_eq!(merklemountainrange::merklemountainrange::is_node_right(23), true);
    // test some false
    assert_eq!(merklemountainrange::merklemountainrange::is_node_right(0), false);
    assert_eq!(merklemountainrange::merklemountainrange::is_node_right(34), false);
    assert_eq!(merklemountainrange::merklemountainrange::is_node_right(21), false);
    assert_eq!(merklemountainrange::merklemountainrange::is_node_right(7), false);
    assert_eq!(merklemountainrange::merklemountainrange::is_node_right(34), false);
    assert_eq!(merklemountainrange::merklemountainrange::is_node_right(14), false);
    assert_eq!(merklemountainrange::merklemountainrange::is_node_right(10), false);
    assert_eq!(merklemountainrange::merklemountainrange::is_node_right(30), false);
    assert_eq!(merklemountainrange::merklemountainrange::is_node_right(15), false);
    assert_eq!(merklemountainrange::merklemountainrange::is_node_right(37), false);
}

#[test]
fn test_node_heights() {
    // test some 0
    assert_eq!(merklemountainrange::merklemountainrange::get_node_height(11), 0);
    assert_eq!(merklemountainrange::merklemountainrange::get_node_height(10), 0);
    assert_eq!(merklemountainrange::merklemountainrange::get_node_height(0), 0);
    assert_eq!(merklemountainrange::merklemountainrange::get_node_height(11), 0);
    assert_eq!(merklemountainrange::merklemountainrange::get_node_height(1), 0);
    assert_eq!(merklemountainrange::merklemountainrange::get_node_height(16), 0);
    assert_eq!(merklemountainrange::merklemountainrange::get_node_height(23), 0);
    assert_eq!(merklemountainrange::merklemountainrange::get_node_height(35), 0);
    assert_eq!(merklemountainrange::merklemountainrange::get_node_height(32), 0);
    assert_eq!(merklemountainrange::merklemountainrange::get_node_height(34), 0);
    assert_eq!(merklemountainrange::merklemountainrange::get_node_height(19), 0);
    assert_eq!(merklemountainrange::merklemountainrange::get_node_height(8), 0);

    // test some 1
    assert_eq!(merklemountainrange::merklemountainrange::get_node_height(2), 1);
    assert_eq!(merklemountainrange::merklemountainrange::get_node_height(5), 1);
    assert_eq!(merklemountainrange::merklemountainrange::get_node_height(20), 1);
    assert_eq!(merklemountainrange::merklemountainrange::get_node_height(27), 1);
    assert_eq!(merklemountainrange::merklemountainrange::get_node_height(36), 1);

    // some larger
    assert_eq!(merklemountainrange::merklemountainrange::get_node_height(6), 2);
    assert_eq!(merklemountainrange::merklemountainrange::get_node_height(13), 2);
    assert_eq!(merklemountainrange::merklemountainrange::get_node_height(21), 2);
    assert_eq!(merklemountainrange::merklemountainrange::get_node_height(37), 2);
    assert_eq!(merklemountainrange::merklemountainrange::get_node_height(14), 3);
    assert_eq!(merklemountainrange::merklemountainrange::get_node_height(29), 3);
    assert_eq!(merklemountainrange::merklemountainrange::get_node_height(30), 4);
    assert_eq!(merklemountainrange::merklemountainrange::get_node_height(62), 5);
}
