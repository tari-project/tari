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
    MAX_PROOF_HASHES,
    MerkleProof,
    MerkleProofError,
    common::{LeafIndex, family_branch, is_leaf, node_index},
};
use tari_utilities::hex::{self, Hex};

use crate::support::{MmrTestHasherBlake256, create_mmr, int_to_hash};

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
    assert!(
        proof
            .verify_leaf::<MmrTestHasherBlake256>(&root, &hash, leaf_pos)
            .is_ok()
    )
}

/// A proof whose sibling path has been truncated must be rejected with an error, not panic.
#[test]
fn truncated_path_is_rejected() {
    let mmr = create_mmr(4);
    let root = mmr.get_merkle_root().unwrap();
    let hash = int_to_hash(0);

    let proof = MerkleProof::for_node(&mmr, 0).unwrap();
    assert!(proof.verify::<MmrTestHasherBlake256>(&root, &hash, 0).is_ok());

    let mut truncated = proof;
    truncated.path.pop();
    assert_eq!(
        truncated.verify::<MmrTestHasherBlake256>(&root, &hash, 0),
        Err(MerkleProofError::PathLengthMismatch {
            pos: 0,
            mmr_size: 7,
            expected: 2,
            actual: 1
        })
    );
}

/// A proof with extra siblings appended to the path must be rejected with an error, not panic.
#[test]
fn padded_path_is_rejected() {
    let mmr = create_mmr(4);
    let root = mmr.get_merkle_root().unwrap();
    let hash = int_to_hash(0);

    let mut padded = MerkleProof::for_node(&mmr, 0).unwrap();
    padded.path.push(int_to_hash(1024));
    assert_eq!(
        padded.verify::<MmrTestHasherBlake256>(&root, &hash, 0),
        Err(MerkleProofError::PathLengthMismatch {
            pos: 0,
            mmr_size: 7,
            expected: 2,
            actual: 3
        })
    );
}

/// A hand-crafted proof that claims an MMR size but provides no path and no peaks at all must be rejected with an
/// error, not panic.
#[test]
fn empty_hand_crafted_proof_is_rejected() {
    let mmr = create_mmr(2);
    let root = mmr.get_merkle_root().unwrap();
    let hash = int_to_hash(0);

    let proof = MerkleProof {
        mmr_size: 3,
        path: Vec::new(),
        peaks: Vec::new(),
    };
    assert_eq!(
        proof.verify::<MmrTestHasherBlake256>(&root, &hash, 0),
        Err(MerkleProofError::PathLengthMismatch {
            pos: 0,
            mmr_size: 3,
            expected: 1,
            actual: 0
        })
    );
}

/// Neither truncating nor padding the sibling path of an otherwise valid proof may panic, and no mangled proof may
/// verify.
#[test]
fn mangled_proofs_never_panic_or_verify() {
    for size in 1..24 {
        let mmr = create_mmr(size);
        let root = mmr.get_merkle_root().unwrap();
        let mut hash_value = 0usize;
        for pos in 0..mmr.len().unwrap() {
            if !is_leaf(pos) {
                continue;
            }
            let hash = int_to_hash(hash_value);
            hash_value += 1;
            let proof = MerkleProof::for_node(&mmr, pos).unwrap();
            // The proof we are about to mangle must itself be valid, otherwise the assertions below prove nothing
            assert!(proof.verify::<MmrTestHasherBlake256>(&root, &hash, pos).is_ok());

            // Truncate the path, one sibling at a time
            for n in 0..proof.path.len() {
                let mut mangled = proof.clone();
                mangled.path.truncate(n);
                assert!(mangled.verify::<MmrTestHasherBlake256>(&root, &hash, pos).is_err());
            }

            // Pad the path with junk siblings
            for n in 1..4 {
                let mut mangled = proof.clone();
                for i in 0..n {
                    mangled.path.push(int_to_hash(1024 + i));
                }
                assert!(mangled.verify::<MmrTestHasherBlake256>(&root, &hash, pos).is_err());
            }

            // Drop the peak hashes
            if !proof.peaks.is_empty() {
                let mut mangled = proof.clone();
                mangled.peaks.clear();
                assert!(mangled.verify::<MmrTestHasherBlake256>(&root, &hash, pos).is_err());
            }

            // Add a junk peak hash
            let mut mangled = proof.clone();
            mangled.peaks.push(int_to_hash(2048));
            assert!(mangled.verify::<MmrTestHasherBlake256>(&root, &hash, pos).is_err());
        }
    }
}

/// Walking a proof up past height 31 must return an error rather than overflowing the shift used to calculate the
/// peak of a node. Both the `mmr_size` and the path are provided by whoever sent us the proof, and the leaf index is
/// provided by the caller, so none of them can be trusted.
#[test]
fn proof_that_walks_past_height_31_is_rejected() {
    let mmr = create_mmr(2);
    let root = mmr.get_merkle_root().unwrap();
    let hash = int_to_hash(0);

    let proof = MerkleProof {
        mmr_size: usize::MAX,
        path: vec![int_to_hash(0); 33],
        peaks: Vec::new(),
    };
    assert!(
        proof
            .verify_leaf::<MmrTestHasherBlake256>(&root, &hash, LeafIndex(1usize << 31))
            .is_err()
    );
}

#[test]
fn honest_proofs_still_verify() {
    for n in 1..=20 {
        let mmr = create_mmr(n);
        let root = mmr.get_merkle_root().unwrap();
        let mmr_size = mmr.len().unwrap();
        for i in 0..n {
            let proof = MerkleProof::for_leaf_node(&mmr, LeafIndex(i)).unwrap();
            assert_eq!(
                proof.path.len(),
                family_branch(node_index(LeafIndex(i)), mmr_size).len()
            );
            assert!(
                proof
                    .verify_leaf::<MmrTestHasherBlake256>(&root, &int_to_hash(i), LeafIndex(i))
                    .is_ok()
            );
        }
    }
}

/// `node_index` saturates, so without a checked conversion these huge leaf indices alias onto one position near
/// `usize::MAX` and verify a real leaf.
#[test]
fn rejects_saturating_leaf_index() {
    let mmr = create_mmr(2);
    let root = mmr.get_merkle_root().unwrap();
    let proof = MerkleProof {
        mmr_size: usize::MAX,
        path: vec![int_to_hash(0)],
        peaks: vec![],
    };
    for leaf_index in [0x8000_0000_0000_0001usize, 0x8000_0000_0000_0002, 0xc000_0000_0000_0000] {
        assert_eq!(
            proof.verify_leaf::<MmrTestHasherBlake256>(&root, &int_to_hash(1), LeafIndex(leaf_index)),
            Err(MerkleProofError::LeafIndexOutOfRange(leaf_index))
        );
    }
}

#[test]
fn rejects_position_outside_mmr() {
    let mmr = create_mmr(3);
    let root = mmr.get_merkle_root().unwrap();
    let proof = MerkleProof::for_leaf_node(&mmr, LeafIndex(2)).unwrap();
    assert_eq!(
        proof.verify::<MmrTestHasherBlake256>(&root, &int_to_hash(2), proof.mmr_size),
        Err(MerkleProofError::PositionOutOfRange { pos: 4, mmr_size: 4 })
    );
}

#[test]
fn rejects_non_leaf_position() {
    let mmr = create_mmr(3);
    let root = mmr.get_merkle_root().unwrap();
    let proof = MerkleProof::for_leaf_node(&mmr, LeafIndex(0)).unwrap();
    let n2 = mmr.get_node_hash(2).unwrap().unwrap();
    assert_eq!(
        proof.verify::<MmrTestHasherBlake256>(&root, &n2, 2),
        Err(MerkleProofError::NonLeafNode)
    );
}

#[test]
fn rejects_wrong_path_length() {
    let mmr = create_mmr(5);
    let root = mmr.get_merkle_root().unwrap();
    let hash = int_to_hash(1);
    let proof = MerkleProof::for_leaf_node(&mmr, LeafIndex(1)).unwrap();

    let mut longer = proof.clone();
    longer.path.push(int_to_hash(1024));
    assert!(matches!(
        longer.verify_leaf::<MmrTestHasherBlake256>(&root, &hash, LeafIndex(1)),
        Err(MerkleProofError::PathLengthMismatch { .. })
    ));

    let mut shorter = proof;
    shorter.path.pop();
    assert!(matches!(
        shorter.verify_leaf::<MmrTestHasherBlake256>(&root, &hash, LeafIndex(1)),
        Err(MerkleProofError::PathLengthMismatch { .. })
    ));
}

#[test]
fn rejects_more_than_max_hashes() {
    let root = create_mmr(2).get_merkle_root().unwrap();
    let hash = int_to_hash(0);
    let too_many = vec![int_to_hash(0); MAX_PROOF_HASHES + 1];

    let long_path = MerkleProof {
        mmr_size: usize::MAX,
        path: too_many.clone(),
        peaks: vec![],
    };
    assert_eq!(
        long_path.verify::<MmrTestHasherBlake256>(&root, &hash, 0),
        Err(MerkleProofError::TooManyHashes {
            field: "path",
            actual: MAX_PROOF_HASHES + 1,
            max: MAX_PROOF_HASHES
        })
    );

    let many_peaks = MerkleProof {
        mmr_size: usize::MAX,
        path: vec![],
        peaks: too_many,
    };
    assert_eq!(
        many_peaks.verify::<MmrTestHasherBlake256>(&root, &hash, 0),
        Err(MerkleProofError::TooManyHashes {
            field: "peaks",
            actual: MAX_PROOF_HASHES + 1,
            max: MAX_PROOF_HASHES
        })
    );
}

/// Peak reinterpretation is an accepted residual (see the `MerkleProof` rustdoc): the bagged root does not commit to
/// `mmr_size` or position, so each of these well-formed proofs verifies. Flip these if a consensus change closes it.
#[test]
fn peak_reinterpretation_is_accepted_residual() {
    // Leaves [a,b,c] (size 4, peaks [2,3]): c also verifies as leaf 4 of a size-8 MMR whose other peak is H(a‖b)
    let mmr = create_mmr(3);
    let root = mmr.get_merkle_root().unwrap();
    let n2 = mmr.get_node_hash(2).unwrap().unwrap();
    let c = int_to_hash(2);
    let proof = MerkleProof {
        mmr_size: 8,
        path: vec![],
        peaks: vec![n2],
    };
    assert!(
        proof
            .verify_leaf::<MmrTestHasherBlake256>(&root, &c, LeafIndex(4))
            .is_ok()
    );

    // Leaves [a,b,c,d] (size 7): the internal peak verifies as leaf 0 of a size-1 MMR
    let mmr = create_mmr(4);
    let root = mmr.get_merkle_root().unwrap();
    let n6 = mmr.get_node_hash(6).unwrap().unwrap();
    let proof = MerkleProof {
        mmr_size: 1,
        path: vec![],
        peaks: vec![],
    };
    assert!(
        proof
            .verify_leaf::<MmrTestHasherBlake256>(&root, &n6, LeafIndex(0))
            .is_ok()
    );

    // 6 leaves (size 10, peaks [6,9]): internal node 9 verifies as leaf 2 of a size-4 MMR
    let mmr = create_mmr(6);
    let root = mmr.get_merkle_root().unwrap();
    let n6 = mmr.get_node_hash(6).unwrap().unwrap();
    let n9 = mmr.get_node_hash(9).unwrap().unwrap();
    let proof = MerkleProof {
        mmr_size: 4,
        path: vec![],
        peaks: vec![n6],
    };
    assert!(
        proof
            .verify_leaf::<MmrTestHasherBlake256>(&root, &n9, LeafIndex(2))
            .is_ok()
    );
}

const JSON_PROOF: &str = r#"{"mmr_size":8,"path":["2e53af27cab59e217386f5138cbac4f0ee53087e8fd1500b8ef836d7e80fd9a8","aa72bf6d136aac5df8faec94246439f7045487a1bd9984101f46fa926f527e8d"],"peaks":["fd11974cff85dcac247817c33efaf3f7b8c9bc43e980dd80553af84231389088"]}"#;
const BINCODE_PROOF: &str = "0800000000000000020000000000000020000000000000002e53af27cab59e217386f5138cbac4f0ee53087e8fd1500b8ef836d7e80fd9a82000000000000000aa72bf6d136aac5df8faec94246439f7045487a1bd9984101f46fa926f527e8d01000000000000002000000000000000fd11974cff85dcac247817c33efaf3f7b8c9bc43e980dd80553af84231389088";

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

    let root = hex::from_hex("95644732dfe67fb86beedead8b9f8676b1cd5399429fc4b09daa1138708abc92").unwrap();
    // Verify JSON-derived proof
    let proof: MerkleProof = serde_json::from_str(JSON_PROOF).unwrap();
    println!("{proof}");
    assert!(
        proof
            .verify_leaf::<MmrTestHasherBlake256>(&root, &int_to_hash(3), LeafIndex(3))
            .is_ok()
    );

    // Verify bincode-derived proof
    let bin_proof = hex::from_hex(BINCODE_PROOF).unwrap();
    let proof: MerkleProof = bincode::deserialize(&bin_proof).unwrap();
    println!("{proof}");
    assert!(
        proof
            .verify_leaf::<MmrTestHasherBlake256>(&root, &int_to_hash(3), LeafIndex(3))
            .is_ok()
    );
}
