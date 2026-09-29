// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use tari_common_types::types::CompressedPublicKey;
use tari_jellyfish::{
    JellyfishMerkleTree,
    LeafKey,
    SparseMerkleProofExt,
    TreeHash,
    TreeStoreWriter,
    memory_store::MemoryTreeStore,
};
use tari_sidechain::{
    CommandCommitProof,
    EvictNodeAtom,
    EvictionProof,
    SidechainBlockCommitProof,
    SidechainProofValidationError,
    ToCommand,
};
use tari_utilities::ByteArray;

mod support;

fn evict_atom(seed: u8) -> EvictNodeAtom {
    EvictNodeAtom::new(CompressedPublicKey::from_canonical_bytes(&[seed; 32]).unwrap())
}

fn command_key(atom: &EvictNodeAtom) -> LeafKey {
    LeafKey::new(TreeHash::new(atom.to_command().hash().into_array()))
}

/// Builds a command JMT (identity key mapping, key == value == command hash) containing `atoms` and returns its root
/// and an inclusion proof for each atom.
fn command_tree(atoms: &[EvictNodeAtom]) -> (TreeHash, Vec<SparseMerkleProofExt>) {
    let mut store = MemoryTreeStore::<()>::new();
    let values = atoms.iter().map(|atom| {
        let key = command_key(atom);
        (key, Some((key.bytes, ())))
    });
    let (root, diff) = JellyfishMerkleTree::new(&store)
        .batch_put_value_set(values, None, None, 1)
        .unwrap();
    for (k, v) in diff.node_batch {
        store.insert_node(k, v).unwrap();
    }
    let jmt = JellyfishMerkleTree::new(&store);
    let proofs = atoms
        .iter()
        .map(|atom| jmt.get_with_proof_ext(command_key(atom).as_ref(), 1).unwrap().1)
        .collect();
    (root, proofs)
}

fn eviction_proof(atom: EvictNodeAtom, root: TreeHash, inclusion_proof: SparseMerkleProofExt) -> EvictionProof {
    let mut commit_proof = support::load_fixture::<SidechainBlockCommitProof>("commit_proof.json");
    commit_proof.header.command_merkle_root = root.into_array().into();
    EvictionProof::new(CommandCommitProof::new(atom, commit_proof, inclusion_proof))
}

#[test]
fn it_accepts_a_valid_command_inclusion_proof() {
    let atoms = [evict_atom(1), evict_atom(2), evict_atom(3)];
    let (root, mut proofs) = command_tree(&atoms);
    let proof = eviction_proof(atoms[0].clone(), root, proofs.remove(0));
    // The fixture's QCs sign the original header, so the commit proof fails after the inclusion proof passes
    let err = proof.validate(4, &|_| Ok(true)).unwrap_err();
    assert!(
        !matches!(err, SidechainProofValidationError::JmtProofVerifyError(_)),
        "{err}"
    );
}

#[test]
fn it_rejects_an_inclusion_proof_for_another_command() {
    let atoms = [evict_atom(1), evict_atom(2), evict_atom(3)];
    let (root, mut proofs) = command_tree(&atoms);
    let proof = eviction_proof(atoms[0].clone(), root, proofs.remove(1));
    let err = proof.validate(4, &|_| Ok(true)).unwrap_err();
    assert!(
        matches!(err, SidechainProofValidationError::JmtProofVerifyError(_)),
        "{err}"
    );
}

#[test]
fn it_rejects_a_command_not_in_the_tree() {
    let atoms = [evict_atom(1), evict_atom(2), evict_atom(3)];
    let (root, mut proofs) = command_tree(&atoms);
    let proof = eviction_proof(evict_atom(4), root, proofs.remove(0));
    let err = proof.validate(4, &|_| Ok(true)).unwrap_err();
    assert!(
        matches!(err, SidechainProofValidationError::JmtProofVerifyError(_)),
        "{err}"
    );
}

#[test]
fn it_rejects_an_inclusion_proof_against_another_root() {
    let atoms = [evict_atom(1), evict_atom(2), evict_atom(3)];
    let (_, mut proofs) = command_tree(&atoms);
    let (other_root, _) = command_tree(&atoms[..2]);
    let proof = eviction_proof(atoms[0].clone(), other_root, proofs.remove(0));
    let err = proof.validate(4, &|_| Ok(true)).unwrap_err();
    assert!(
        matches!(err, SidechainProofValidationError::JmtProofVerifyError(_)),
        "{err}"
    );
}
