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
//

#[allow(dead_code)]
mod support;

use support::{create_mmr, int_to_hash};
use tari_crypto::hash::blake2::Blake256;
use tari_mmr::{
    common::{is_leaf, node_index},
    MerkleProof,
    MerkleProofError,
};
use tari_utilities::hex::{self, Hex};

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
                assert!(proof.verify::<Blake256>(&root, &hash, pos).is_ok());
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
    let pos = node_index(i);
    let hash = int_to_hash(i);
    let proof = MerkleProof::for_node(&mmr, pos).unwrap();
    assert!(proof.verify::<Blake256>(&root, &hash, pos).is_ok());
}

#[test]
fn a_big_proof() {
    let mmr = create_mmr(100_000);
    let leaf_pos = 28_543;
    let mmr_index = node_index(leaf_pos);
    let root = mmr.get_merkle_root().unwrap();
    let hash = int_to_hash(leaf_pos);
    let proof = MerkleProof::for_node(&mmr, mmr_index).unwrap();
    assert!(proof.verify::<Blake256>(&root, &hash, mmr_index).is_ok())
}

#[test]
fn for_leaf_node() {
    let mmr = create_mmr(100);
    let root = mmr.get_merkle_root().unwrap();
    let leaf_pos = 28;
    let hash = int_to_hash(leaf_pos);
    let proof = MerkleProof::for_leaf_node(&mmr, leaf_pos).unwrap();
    assert!(proof.verify_leaf::<Blake256>(&root, &hash, leaf_pos).is_ok())
}

const JSON_PROOF: &str = r#"{"mmr_size":8,"path":["2c20e9c611aa4b9498040de76e6922a46b7994bb7c02fa7dd56fc7cb4d689f97","ca28b1e65c03cb49bdd57703633d2fdf155b429e7d6da4f5805620b45e8c0d79"],"peaks":["443d43ca1936b06b9267e93821fc90eaa59bf386af599bcec5dfb62f62748362"]}"#;
const BINCODE_PROOF: &str = "0800000000000000020000000000000020000000000000002c20e9c611aa4b9498040de76e6922a46b7994bb7c02fa7dd56fc7cb4d689f972000000000000000ca28b1e65c03cb49bdd57703633d2fdf155b429e7d6da4f5805620b45e8c0d7901000000000000002000000000000000443d43ca1936b06b9267e93821fc90eaa59bf386af599bcec5dfb62f62748362";

#[test]
fn serialisation() {
    let mmr = create_mmr(5);
    let proof = MerkleProof::for_leaf_node(&mmr, 3).unwrap();
    let json_proof = serde_json::to_string(&proof).unwrap();
    assert_eq!(&json_proof, JSON_PROOF);

    let bincode_proof = bincode::serialize(&proof).unwrap();
    assert_eq!(bincode_proof.to_hex(), BINCODE_PROOF);
}

#[test]
fn deserialization() {
    // Note: To create a new root, uncomment these two lines
    // let mmr = create_mmr(5);
    // println!("\nNew root: {}\n", mmr.get_merkle_root().unwrap().to_hex());

    let root = hex::from_hex("fc25ea9a702604f2f0c91f6893cdc055cb223d4890c4f07c3d5258e50d400f59").unwrap();
    // Verify JSON-derived proof
    let proof: MerkleProof = serde_json::from_str(JSON_PROOF).unwrap();
    println!("{}", proof);
    assert!(proof.verify_leaf::<Blake256>(&root, &int_to_hash(3), 3).is_ok());

    // Verify bincode-derived proof
    let bin_proof = hex::from_hex(BINCODE_PROOF).unwrap();
    let proof: MerkleProof = bincode::deserialize(&bin_proof).unwrap();
    println!("{}", proof);
    assert!(proof.verify_leaf::<Blake256>(&root, &int_to_hash(3), 3).is_ok());
}
