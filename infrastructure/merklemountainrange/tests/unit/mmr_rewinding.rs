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

use crate::support::testobject::TestObject;
use blake2::Blake2b;
use merklemountainrange::mmr::*;
use std::fs;
use tari_storage::lmdb::*;

fn create_mmr(leaves: u32) -> MerkleMountainRange<TestObject, Blake2b> {
    let mut mmr: MerkleMountainRange<TestObject, Blake2b> = MerkleMountainRange::new();
    mmr.init_persistance_store(&"mmr".to_string(), 20);
    for i in 1..leaves + 1 {
        let object: TestObject = TestObject::new(i.to_string());
        assert!(mmr.push(object).is_ok());
    }
    mmr
}
#[test]
fn rewind_simple() {
    let _ = fs::remove_dir_all("./tests/test_mmr_r"); // we ensure that the test dir is empty
    let mut mmr = create_mmr(14);
    // create storage
    fs::create_dir("./tests/test_mmr_r").unwrap();
    let builder = LMDBBuilder::new();
    let mut store = builder
        .set_mapsize(5)
        .set_path("./tests/test_mmr_r/")
        .add_database(&"mmr_mmr_checkpoints".to_string())
        .add_database(&"mmr_mmr_objects".to_string())
        .add_database(&"mmr_init".to_string())
        .build()
        .unwrap();
    assert!(mmr.checkpoint().is_ok());
    assert!(mmr.apply_state(&mut store).is_ok());;

    let mut mmr2: MerkleMountainRange<TestObject, Blake2b> = MerkleMountainRange::new();
    mmr2.init_persistance_store(&"mmr".to_string(), 20);
    assert!(mmr2.load_from_store(&mut store).is_ok());
    assert_eq!(mmr.get_merkle_root(), mmr2.get_merkle_root());
    assert_eq!(mmr.get_unpruned_hash(), mmr2.get_unpruned_hash());

    let mut mmr3: MerkleMountainRange<TestObject, Blake2b> = MerkleMountainRange::new();
    mmr3.init_persistance_store(&"mmr".to_string(), 20);
    assert!(mmr3.load_from_store(&mut store).is_ok());
    assert_eq!(mmr.get_merkle_root(), mmr3.get_merkle_root());
    assert_eq!(mmr.get_unpruned_hash(), mmr3.get_unpruned_hash());
    // add much more leaves
    for j in 0..5 {
        for i in 1..11 {
            let object: TestObject = TestObject::new((14 * j + i + 14).to_string());
            assert!(mmr.push(object).is_ok());
        }
        for i in 1..11 {
            let object: TestObject = TestObject::new((14 * j + i + 14).to_string());
            assert!(mmr3.push(object).is_ok());
        }
        assert!(mmr.checkpoint().is_ok());
        assert!(mmr.apply_state(&mut store).is_ok());

        assert!(mmr.rewind(&mut store, j + 1).is_ok());
        assert_eq!(mmr.get_merkle_root(), mmr2.get_merkle_root());
        assert_eq!(mmr.get_unpruned_hash(), mmr2.get_unpruned_hash());

        assert!(mmr.ff_to_head(&mut store).is_ok());
        assert_eq!(mmr.get_merkle_root(), mmr3.get_merkle_root()); // are we where we are suppose to be
        assert_eq!(mmr.get_unpruned_hash(), mmr3.get_unpruned_hash());
    }
    assert!(fs::remove_dir_all("./tests/test_mmr_r").is_ok()); // we ensure that the test dir is empty
}

#[test]
fn batch_save() {
    let _ = fs::remove_dir_all("./tests/test_mmr_bs"); // we ensure that the test dir is empty
    let mut mmr = create_mmr(14);
    // create storage
    fs::create_dir("./tests/test_mmr_bs").unwrap();
    let builder = LMDBBuilder::new();
    let mut store = builder
        .set_mapsize(5)
        .set_path("./tests/test_mmr_bs/")
        .add_database(&"mmr_mmr_checkpoints".to_string())
        .add_database(&"mmr_mmr_objects".to_string())
        .add_database(&"mmr_init".to_string())
        .build()
        .unwrap();
    assert!(mmr.checkpoint().is_ok());
    assert!(mmr.apply_state(&mut store).is_ok());;

    let mut mmr2: MerkleMountainRange<TestObject, Blake2b> = MerkleMountainRange::new();
    mmr2.init_persistance_store(&"mmr".to_string(), 20);
    assert!(mmr2.load_from_store(&mut store).is_ok());
    assert_eq!(mmr.get_merkle_root(), mmr2.get_merkle_root());
    assert_eq!(mmr.get_unpruned_hash(), mmr2.get_unpruned_hash());

    let mut mmr3: MerkleMountainRange<TestObject, Blake2b> = MerkleMountainRange::new();
    mmr3.init_persistance_store(&"mmr".to_string(), 20);
    assert!(mmr3.load_from_store(&mut store).is_ok());
    assert_eq!(mmr.get_merkle_root(), mmr3.get_merkle_root());
    assert_eq!(mmr.get_unpruned_hash(), mmr3.get_unpruned_hash());
    // add much more leaves
    for j in 0..5 {
        for i in 1..11 {
            let object: TestObject = TestObject::new((14 * j + i + 14).to_string());
            assert!(mmr.push(object).is_ok());
        }
        for i in 1..11 {
            let object: TestObject = TestObject::new((14 * j + i + 14).to_string());
            assert!(mmr3.push(object).is_ok());
        }
        assert!(mmr.checkpoint().is_ok());

        // are we where we are suppose to be
    }

    assert!(mmr.apply_state(&mut store).is_ok());
    assert!(mmr.rewind(&mut store, 5).is_ok());
    assert_eq!(mmr.get_merkle_root(), mmr2.get_merkle_root());
    assert_eq!(mmr.get_unpruned_hash(), mmr2.get_unpruned_hash());

    assert!(mmr.ff_to_head(&mut store).is_ok());
    assert_eq!(mmr.get_merkle_root(), mmr3.get_merkle_root());
    assert_eq!(mmr.get_unpruned_hash(), mmr3.get_unpruned_hash());

    // have we  saved correctly and can we load again
    let mut mmr4: MerkleMountainRange<TestObject, Blake2b> = MerkleMountainRange::new();
    mmr4.init_persistance_store(&"mmr".to_string(), 20);
    assert!(mmr4.load_from_store(&mut store).is_ok());
    assert_eq!(mmr4.get_merkle_root(), mmr3.get_merkle_root());
    assert_eq!(mmr4.get_unpruned_hash(), mmr3.get_unpruned_hash());

    assert!(fs::remove_dir_all("./tests/test_mmr_bs").is_ok()); // we ensure that the test dir is empty
}
