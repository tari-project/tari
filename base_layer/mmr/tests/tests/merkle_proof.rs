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

use tari_mmr::{
    common::{is_leaf, node_index, LeafIndex},
    MerkleProof,
    MerkleProofError,
};
use tari_utilities::hex::{self, Hex};

use crate::support::{create_mmr, int_to_hash, MmrTestHasherBlake256};

#[test]
fn zero_size_mmr() {
    let mmr = create_mmr(0);
    match MerkleProof::for_node(&mmr, 0) {
        Err(MerkleProofError::HashNotFound(i)) => assert_eq!(i, 0),
        _ => panic!("Incorrect zero-length merkle proof"),
    }
}

/// Thorough check of MerkleProof process for each position in various MMR sizes
#[test]
fn merkle_proof_small_mmrs() {
    for size in 1..32 {
        let mmr = create_mmr(size);
        let root = mmr.get_merkle_root().unwrap();
        let mut hash_value = 0usize;
        for pos in 0..mmr.len().unwrap() {
            if is_leaf(pos) {
                let hash = int_to_hash(hash_value);
                hash_value += 1;
                let proof = MerkleProof::for_node(&mmr, pos).unwrap();
                assert!(proof.verify::<MmrTestHasherBlake256>(&root, &hash, pos).is_ok());
            } else {
                assert_eq!(MerkleProof::for_node(&mmr, pos), Err(MerkleProofError::NonLeafNode));
            }
        }
    }
}

#[test]
fn med_mmr() {
    let size = 500;
    let mmr = create_mmr(size);
    let root = mmr.get_merkle_root().unwrap();
    let i = 499;
    let pos = node_index(LeafIndex(i));
    let hash = int_to_hash(i);
    let proof = MerkleProof::for_node(&mmr, pos).unwrap();
    assert!(proof.verify::<MmrTestHasherBlake256>(&root, &hash, pos).is_ok());
}

#[test]
fn a_big_proof() {
    let mmr = create_mmr(100_000);
    let leaf_pos = LeafIndex(28_543);
    let mmr_index = node_index(leaf_pos);
    let root = mmr.get_merkle_root().unwrap();
    let hash = int_to_hash(leaf_pos.0);
    let proof = MerkleProof::for_node(&mmr, mmr_index).unwrap();
    assert!(proof.verify::<MmrTestHasherBlake256>(&root, &hash, mmr_index).is_ok())
}

#[test]
fn for_leaf_node() {
    let mmr = create_mmr(100);
    let root = mmr.get_merkle_root().unwrap();
    let leaf_pos = LeafIndex(28);
    let hash = int_to_hash(leaf_pos.0);
    let proof = MerkleProof::for_leaf_node(&mmr, leaf_pos).unwrap();
    assert!(proof
        .verify_leaf::<MmrTestHasherBlake256>(&root, &hash, leaf_pos)
        .is_ok())
}

const JSON_PROOF: &str = r#"{"mmr_size":8,"path":["8343a0e3122b3a82cbfb0ff8c7aabd6ea48580d0aa14b9e9a0f4b78a4852efa9","f7010ddf7881e9fcd2c51a1f2bc66448dba431c1661fa4bb0de5f842bd30ef79"],"peaks":["f5e19d13dbd76ecb10544dd434620169a7d2cddf516aa8e1a15a2b66f0b5022e"]}"#;
const BINCODE_PROOF: &str = "0800000000000000020000000000000020000000000000008343a0e3122b3a82cbfb0ff8c7aabd6ea48580d0aa14b9e9a0f4b78a4852efa92000000000000000f7010ddf7881e9fcd2c51a1f2bc66448dba431c1661fa4bb0de5f842bd30ef7901000000000000002000000000000000f5e19d13dbd76ecb10544dd434620169a7d2cddf516aa8e1a15a2b66f0b5022e";

#[test]
fn serialisation() {
    let mmr = create_mmr(5);
    let proof = MerkleProof::for_leaf_node(&mmr, LeafIndex(3)).unwrap();
    let json_proof = serde_json::to_string(&proof).unwrap();
    assert_eq!(&json_proof, JSON_PROOF);

    let bincode_proof = bincode::serialize(&proof).unwrap();
    assert_eq!(bincode_proof.to_hex(), BINCODE_PROOF);
}

#[test]
fn deserialization() {
    // Note: To create a new root, uncomment these two lines
    let mmr = create_mmr(5);
    println!("\nNew root: {}\n", mmr.get_merkle_root().unwrap().to_hex());

    let root = hex::from_hex("3835a5f6c170d671c9306cca3bb035d701970d541c0365c15d51550e051bf57d").unwrap();
    // Verify JSON-derived proof
    let proof: MerkleProof = serde_json::from_str(JSON_PROOF).unwrap();
    println!("{}", proof);
    assert!(proof
        .verify_leaf::<MmrTestHasherBlake256>(&root, &int_to_hash(3), LeafIndex(3))
        .is_ok());

    // Verify bincode-derived proof
    let bin_proof = hex::from_hex(BINCODE_PROOF).unwrap();
    let proof: MerkleProof = bincode::deserialize(&bin_proof).unwrap();
    println!("{}", proof);
    assert!(proof
        .verify_leaf::<MmrTestHasherBlake256>(&root, &int_to_hash(3), LeafIndex(3))
        .is_ok());
}
