//   Copyright 2025 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use tari_sidechain::{CommitProofElement, MAX_QC_SIGNATURES, SidechainBlockCommitProof};

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
            let signature = qc.signatures.first().unwrap().clone();
            qc.signatures = vec![signature; MAX_QC_SIGNATURES + 1];
        }
    }
    let err = proof.validate_committed(4, &|_| Ok(true)).unwrap_err();
    assert!(err.to_string().contains("at most"), "Unexpected error message: {err}");
}
