//   Copyright 2025 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use tari_common_types::types::FixedHash;
use tari_sidechain::{
    CommitProofElement,
    MAX_QC_SIGNATURES,
    ProposalVoteMessage,
    QuorumDecision,
    SidechainBlockCommitProof,
};

mod support;
use support::load_fixture;

#[test]
fn it_validates_a_correct_proof() {
    let proof = load_fixture::<SidechainBlockCommitProof>("commit_proof.json");
    proof.validate_committed(4, &|_| Ok(true)).unwrap();
}

#[test]
fn it_rejects_a_qc_with_too_many_signatures() {
    let mut proof = load_fixture::<SidechainBlockCommitProof>("commit_proof.json");
    for elem in &mut proof.proof_elements {
        if let CommitProofElement::QuorumCertificate(qc) = elem {
            let signature = qc
                .signatures
                .first()
                .expect("commit_proof.json fixture must have at least one signature to clone")
                .clone();
            qc.signatures = vec![signature; MAX_QC_SIGNATURES + 1];
        }
    }
    let err = proof.validate_committed(4, &|_| Ok(true)).unwrap_err();
    let expected = format!(
        "must contain at most {} signatures but contained {}",
        MAX_QC_SIGNATURES,
        MAX_QC_SIGNATURES + 1
    );
    assert!(
        err.to_string().contains(&expected),
        "Expected the signature-count error, got: {err}"
    );
}

#[test]
fn a_header_that_omits_the_protocol_version_is_version_0() {
    // The fixture has no `protocol_version` key, which is how every header serialised without one must read.
    let proof = load_fixture::<SidechainBlockCommitProof>("commit_proof.json");
    assert_eq!(proof.header().protocol_version, 0);
}

#[test]
fn the_protocol_version_selects_the_hash_schema() {
    let proof = load_fixture::<SidechainBlockCommitProof>("commit_proof.json");
    let v0_id = proof.header().calculate_block_id();

    let mut header = proof.header().clone();
    header.protocol_version = 1;
    let v1_id = header.calculate_block_id();
    assert_ne!(v0_id, v1_id, "a version bump must change the block ID");

    // From version 1 the version is committed to, so no two versions can share a block ID.
    header.protocol_version = 2;
    assert_ne!(v1_id, header.calculate_block_id());
}

#[test]
fn it_rejects_a_proof_whose_header_claims_another_protocol_version() {
    let mut proof = load_fixture::<SidechainBlockCommitProof>("commit_proof.json");
    proof.header.protocol_version = 1;
    let err = proof.validate_committed(4, &|_| Ok(true)).unwrap_err();
    assert!(
        err.to_string().contains("does not match the block ID in the header"),
        "Expected the block ID mismatch error, got: {err}"
    );
}

#[test]
fn it_rejects_a_qc_that_claims_another_protocol_version() {
    let mut proof = load_fixture::<SidechainBlockCommitProof>("commit_proof.json");
    for elem in &mut proof.proof_elements {
        if let CommitProofElement::QuorumCertificate(qc) = elem {
            qc.protocol_version = 1;
        }
    }
    let err = proof.validate_committed(4, &|_| Ok(true)).unwrap_err();
    assert!(
        err.to_string().contains("Invalid signature for QC"),
        "Expected the signature error, got: {err}"
    );
}

#[test]
fn the_protocol_version_selects_the_vote_message() {
    let block_id = FixedHash::from([7u8; 32]);
    let message = |protocol_version, epoch, height| {
        ProposalVoteMessage::new(protocol_version, &block_id, QuorumDecision::Accept, epoch, height).calculate_hash()
    };

    assert_ne!(message(0, 3, 9), message(1, 3, 9));
    // Version 0 does not commit to the view it was cast in, version 1 does.
    assert_eq!(message(0, 3, 9), message(0, 0, 0));
    assert_ne!(message(1, 3, 9), message(1, 0, 0));
    // Versions that share a preimage shape still produce distinct messages, so a certificate cannot be
    // relabelled to a version its members did not sign under.
    assert_ne!(message(1, 3, 9), message(2, 3, 9));
}
