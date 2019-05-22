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
use merklemountainrange::{merkleproof::MerkleProof, mmr::*};
use std::fs;
use tari_storage::lmdb::*;
use tari_utilities::hex::*;

fn create_mmr(leaves: u32) -> MerkleMountainRange<TestObject, Blake2b> {
    let mut mmr: MerkleMountainRange<TestObject, Blake2b> = MerkleMountainRange::new();
    mmr.init_persistance_store(&"mmr".to_string(), 5);
    for i in 1..leaves + 1 {
        let object: TestObject = TestObject::new(i.to_string());
        mmr.push(object);
    }
    mmr
}
#[test]
fn create_small_mmr() {
    let _res = fs::remove_dir_all("./tests/test_mmr_s"); // we ensure that the test dir is empty
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
    // create storage
    fs::create_dir("./tests/test_mmr_s").unwrap();
    let builder = LMDBBuilder::new();
    let mut store = builder
        .set_mapsize(5)
        .set_path("./tests/test_mmr_s/")
        .add_database(&"mmr_mmr_checkpoints".to_string())
        .add_database(&"mmr_mmr_objects".to_string())
        .add_database(&"mmr_init".to_string())
        .build()
        .unwrap();
    let result = mmr.apply_checkpoint(&mut store);
    assert_eq!(result.is_ok(), true);
    let mut mmr2: MerkleMountainRange<TestObject, Blake2b> = MerkleMountainRange::new();
    mmr2.init_persistance_store(&"mmr".to_string(), 5);
    let result = mmr2.load_from_store(&mut store);
    assert_eq!(result.is_ok(), true);
    assert_eq!(mmr.get_merkle_root(), mmr2.get_merkle_root());
    let _res = fs::remove_dir_all("./tests/test_mmr_s"); // we ensure that the test dir is empty
}

#[test]
fn create_med_mmr() {
    let _res = fs::remove_dir_all("./tests/test_mmr_m"); // we ensure that the test dir is empty
    let mut mmr = create_mmr(14);
    // create storage
    fs::create_dir("./tests/test_mmr_m").unwrap();
    let builder = LMDBBuilder::new();
    let mut store = builder
        .set_mapsize(5)
        .set_path("./tests/test_mmr_m/")
        .add_database(&"mmr_mmr_checkpoints".to_string())
        .add_database(&"mmr_mmr_objects".to_string())
        .add_database(&"mmr_init".to_string())
        .build()
        .unwrap();
    let result = mmr.apply_checkpoint(&mut store);
    assert_eq!(result.is_ok(), true);
    let mut mmr2: MerkleMountainRange<TestObject, Blake2b> = MerkleMountainRange::new();
    mmr2.init_persistance_store(&"mmr".to_string(), 5);
    let result = mmr2.load_from_store(&mut store);
    assert_eq!(result.is_ok(), true);
    assert_eq!(mmr.get_merkle_root(), mmr2.get_merkle_root());

    // add more leafs
    for i in 15..25 {
        dbg!(&i);
        let object: TestObject = TestObject::new(i.to_string());
        mmr.push(object);
        let result = mmr.apply_checkpoint(&mut store);
        assert_eq!(result.is_ok(), true);
        let mut mmr2: MerkleMountainRange<TestObject, Blake2b> = MerkleMountainRange::new();
        mmr2.init_persistance_store(&"mmr".to_string(), 5);
        let result = mmr2.load_from_store(&mut store);
        assert_eq!(result.is_ok(), true);
        assert_eq!(mmr.get_merkle_root(), mmr2.get_merkle_root());
    }
    let _res = fs::remove_dir_all("./tests/test_mmr_s"); // we ensure that the test dir is empty
}
