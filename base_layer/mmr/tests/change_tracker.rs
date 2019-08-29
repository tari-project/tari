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
use support::{create_mmr, int_to_hash, Hasher};
use tari_mmr::{MerkleChangeTracker, MutableMmr};
use tari_utilities::hex::Hex;

#[test]
fn change_tracker() {
    let mmr = MutableMmr::<Hasher, _>::new(Vec::default());
    let mmr = MerkleChangeTracker::new(mmr, Vec::new()).unwrap();
    assert_eq!(mmr.checkpoint_count(), 0);
    assert!(mmr.is_empty());
}

#[test]
/// Test the same MMR structure as the test in mutable_mmr, but add in rewinding and restoring of state
fn checkpoints() {
    //----------- Construct and populate the initial MMR --------------------------
    let base = MutableMmr::<Hasher, _>::new(Vec::default());
    let mut mmr = MerkleChangeTracker::new(base, Vec::new()).unwrap();
    for i in 0..5 {
        assert!(mmr.push(&int_to_hash(i)).is_ok());
    }
    assert_eq!(mmr.len(), 5);
    assert_eq!(mmr.checkpoint_count(), 0);
    //----------- Commit the history thus far  -----------------------------------
    assert!(mmr.commit().is_ok());
    assert_eq!(mmr.checkpoint_count(), 1);
    let root_at_1 = mmr.get_merkle_root();
    assert_eq!(
        &root_at_1.to_hex(),
        "7b7ddec2af4f3d0b9b165750cf2ff15813e965d29ecd5318e0c8fea901ceaef4"
    );
    //----------- Add a node and delete a few nodes  -----------------------------
    assert!(mmr.push(&int_to_hash(5)).is_ok());
    assert!(mmr.delete_and_compress(0, false));
    assert!(mmr.delete_and_compress(2, false));
    assert!(mmr.delete_and_compress(4, true));
    //----------- Commit the history again, and check the expected sizes  --------
    let root_at_2 = mmr.get_merkle_root();
    assert_eq!(
        &root_at_2.to_hex(),
        "69e69ba0c6222f2d9caa68282de0ba7f1259a0fa2b8d84af68f907ef4ec05054"
    );
    assert!(mmr.commit().is_ok());
    assert_eq!(mmr.len(), 3);
    assert_eq!(mmr.checkpoint_count(), 2);
    //----------- Generate another checkpoint, the MMR is now empty  --------
    assert!(mmr.delete_and_compress(1, false));
    assert!(mmr.delete_and_compress(5, false));
    assert!(mmr.delete(3));
    assert!(mmr.commit().is_ok());
    assert!(mmr.is_empty());
    assert_eq!(mmr.checkpoint_count(), 3);
    let root = mmr.get_merkle_root();
    assert_eq!(
        &root.to_hex(),
        "2a540797d919e63cff8051e54ae13197315000bcfde53efd3f711bb3d24995bc"
    );
    //----------- Create an empty checkpoint -------------------------------
    assert!(mmr.commit().is_ok());
    assert_eq!(mmr.checkpoint_count(), 4);
    assert_eq!(
        &mmr.get_merkle_root().to_hex(),
        "2a540797d919e63cff8051e54ae13197315000bcfde53efd3f711bb3d24995bc"
    );
    //----------- Rewind the MMR two commits----------------------------------
    assert!(mmr.rewind(2).is_ok());
    assert_eq!(mmr.get_merkle_root().to_hex(), root_at_2.to_hex());
    //----------- Perform an empty commit ------------------------------------
    assert!(mmr.commit().is_ok());
    assert_eq!(mmr.len(), 3);
    assert_eq!(mmr.checkpoint_count(), 3);
}

#[test]
fn reset_and_replay() {
    // You don't have to use a Pruned MMR... any MMR implementation is fine
    let base = MutableMmr::from(create_mmr(5));
    let mut mmr = MerkleChangeTracker::new(base, Vec::new()).unwrap();
    let root = mmr.get_merkle_root();
    // Add some new nodes etc
    assert!(mmr.push(&int_to_hash(10)).is_ok());
    assert!(mmr.push(&int_to_hash(11)).is_ok());
    assert!(mmr.push(&int_to_hash(12)).is_ok());
    assert!(mmr.delete(7));
    // Reset - should be back to base state
    assert!(mmr.reset().is_ok());
    assert_eq!(mmr.get_merkle_root(), root);

    // Change some more state
    assert!(mmr.delete(1));
    assert!(mmr.delete(3));
    assert!(mmr.commit().is_ok()); //--- Checkpoint 0 ---
    let root = mmr.get_merkle_root();

    // Change a bunch more things
    let hash_5 = int_to_hash(5);
    assert!(mmr.push(&hash_5).is_ok());
    assert!(mmr.commit().is_ok()); //--- Checkpoint 1 ---
    assert!(mmr.push(&int_to_hash(6)).is_ok());
    assert!(mmr.commit().is_ok()); //--- Checkpoint 2 ---

    assert!(mmr.push(&int_to_hash(7)).is_ok());
    assert!(mmr.commit().is_ok()); //--- Checkpoint 3 ---
    assert!(mmr.delete(0));
    assert!(mmr.delete(6));
    assert!(mmr.commit().is_ok()); //--- Checkpoint 4 ---

    // Get checkpoint 1
    let cp = mmr.get_checkpoint(1).unwrap();
    assert_eq!(cp.nodes_added(), &[hash_5]);
    assert_eq!(*cp.nodes_deleted(), Bitmap::create());

    // Get checkpoint 0
    let cp = mmr.get_checkpoint(0).unwrap();
    assert!(cp.nodes_added().is_empty());
    let mut deleted = Bitmap::create();
    deleted.add(1);
    deleted.add(3);
    assert_eq!(*cp.nodes_deleted(), deleted);

    // Roll back to last time we save the root
    assert!(mmr.replay(1).is_ok());
    assert_eq!(mmr.len(), 3);

    assert_eq!(mmr.get_merkle_root(), root);
}
