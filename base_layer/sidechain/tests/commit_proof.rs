//   Copyright 2025 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use tari_common_types::types::FixedHash;
use tari_jellyfish::JmtHashScheme;
use tari_sidechain::{
    CommitProofElement,
    MAX_QC_SIGNATURES,
    ProposalVoteMessage,
    QcSignatures,
    QuorumCertificate,
    QuorumDecision,
    SidechainBlockCommitProof,
    SidechainBlockHeader,
    jmt_hash_scheme,
};

mod support;
use support::{
    PROTOCOL_VERSION,
    TestCommittee,
    fixture_committee,
    fixture_proof,
    load_fixture,
    signed_proof,
    test_committee,
    test_header,
    validator_public_key,
};

fn schnorr_signatures(qc: &mut QuorumCertificate) -> &mut Vec<tari_sidechain::ValidatorBlockSignature> {
    let QcSignatures::Schnorr { signatures, .. } = &mut qc.signatures;
    signatures
}

#[test]
fn the_fixture_is_the_proof_the_fixture_committee_signs() {
    assert_eq!(
        load_fixture::<SidechainBlockCommitProof>("commit_proof.json"),
        fixture_proof(),
        "regenerate commit_proof.json with `cargo test -p tari_sidechain --test commit_proof -- --ignored \
         regenerate_fixture`"
    );
}

#[test]
#[ignore = "writes tests/fixtures/commit_proof.json"]
fn regenerate_fixture() {
    let json = serde_json::to_string_pretty(&fixture_proof()).unwrap();
    std::fs::write("tests/fixtures/commit_proof.json", json + "\n").unwrap();
}

#[test]
fn it_validates_a_correct_proof() {
    let proof = load_fixture::<SidechainBlockCommitProof>("commit_proof.json");
    proof.validate_committed(4, &fixture_committee()).unwrap();
}

#[test]
fn it_rejects_a_proof_checked_against_another_committee() {
    let proof = load_fixture::<SidechainBlockCommitProof>("commit_proof.json");
    let committee = test_committee(5..=8, |_| 1);
    let err = proof.validate_committed(4, &committee).unwrap_err();
    assert!(
        err.to_string().contains("Invalid signature for QC"),
        "Expected the signature error, got: {err}"
    );
}

#[test]
fn it_rejects_a_qc_with_too_many_signatures() {
    let mut proof = load_fixture::<SidechainBlockCommitProof>("commit_proof.json");
    for elem in &mut proof.proof_elements {
        if let CommitProofElement::QuorumCertificate(qc) = elem {
            let signatures = schnorr_signatures(qc);
            let signature = signatures
                .first()
                .expect("commit_proof.json fixture must have at least one signature to clone")
                .clone();
            *signatures = vec![signature; MAX_QC_SIGNATURES + 1];
        }
    }
    let err = proof.validate_committed(4, &fixture_committee()).unwrap_err();
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
fn the_block_id_commits_to_the_protocol_version() {
    let mut header = test_header(None);
    let id = header.calculate_block_id();
    header.protocol_version = PROTOCOL_VERSION + 1;
    assert_ne!(id, header.calculate_block_id());
}

#[test]
fn it_rejects_a_proof_whose_header_claims_another_protocol_version() {
    let mut proof = load_fixture::<SidechainBlockCommitProof>("commit_proof.json");
    proof.header.protocol_version = PROTOCOL_VERSION + 1;
    let err = proof.validate_committed(4, &fixture_committee()).unwrap_err();
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
            qc.protocol_version = PROTOCOL_VERSION + 1;
        }
    }
    let err = proof.validate_committed(4, &fixture_committee()).unwrap_err();
    assert!(
        err.to_string().contains("Invalid signature for QC"),
        "Expected the signature error, got: {err}"
    );
}

#[test]
fn a_header_that_omits_the_transaction_merkle_root_carries_none() {
    let proof = load_fixture::<SidechainBlockCommitProof>("commit_proof.json");
    assert_eq!(proof.header().transaction_merkle_root(), None);
}

#[test]
fn the_block_id_commits_to_the_transaction_merkle_root() {
    let mut header = test_header(None);
    let without_root = header.calculate_block_id();

    header.transaction_merkle_root = Some(FixedHash::zero());
    let zero_root = header.calculate_block_id();
    header.transaction_merkle_root = Some(FixedHash::from([1u8; 32]));
    let other_root = header.calculate_block_id();

    assert_ne!(without_root, zero_root, "carrying a root must change the block ID");
    assert_ne!(zero_root, other_root, "the root must be part of the block ID");
}

#[test]
fn it_validates_a_proof_that_carries_a_transaction_merkle_root() {
    let root = FixedHash::from([1u8; 32]);
    let committee = fixture_committee();
    let proof = signed_proof(&committee, 1..=4, test_header(Some(root)));
    proof.validate_committed(4, &committee).unwrap();

    let mut tampered = proof.clone();
    tampered.header.transaction_merkle_root = Some(FixedHash::from([2u8; 32]));
    let err = tampered.validate_committed(4, &committee).unwrap_err();
    assert!(
        err.to_string().contains("does not match the block ID in the header"),
        "Expected the block ID mismatch error, got: {err}"
    );

    let mut stripped = proof;
    stripped.header.transaction_merkle_root = None;
    let err = stripped.validate_committed(4, &committee).unwrap_err();
    assert!(
        err.to_string().contains("does not match the block ID in the header"),
        "Expected the block ID mismatch error, got: {err}"
    );
}

mod weighted_committee {
    use super::*;

    /// Seven validators: 1 to 4 hold power 1 each and 5 to 7 hold power 10 each, 34 in all.
    fn committee() -> TestCommittee {
        test_committee(1..=7, |i| if i <= 4 { 1 } else { 10 })
    }

    /// More than two thirds of the committee's 34 voting power.
    const QUORUM_POWER: u64 = 23;

    #[test]
    fn it_rejects_a_qc_with_a_supermajority_of_signers_but_too_little_power() {
        // Five of seven members, more than two thirds by count, but only 14 of 34 power.
        let committee = committee();
        let proof = signed_proof(&committee, 1..=5, test_header(None));
        let err = proof.validate_committed(QUORUM_POWER, &committee).unwrap_err();
        assert!(
            err.to_string()
                .contains("signers hold 14 voting power but at least 23 is required"),
            "Expected the voting power error, got: {err}"
        );
    }

    #[test]
    fn it_accepts_a_qc_with_few_signers_holding_enough_power() {
        // Three of seven members, but 30 of 34 power.
        let committee = committee();
        let proof = signed_proof(&committee, 5..=7, test_header(None));
        proof.validate_committed(QUORUM_POWER, &committee).unwrap();
    }
}

mod signer_bitmap {
    use tari_sidechain::{SidechainProofValidationError, SignerBitmap};

    use super::*;

    fn with_first_qc(mutate: impl FnOnce(&mut QuorumCertificate)) -> SidechainBlockCommitProof {
        let mut proof = load_fixture::<SidechainBlockCommitProof>("commit_proof.json");
        let Some(CommitProofElement::QuorumCertificate(qc)) = proof.proof_elements.first_mut() else {
            panic!("commit_proof.json fixture must begin with a quorum certificate");
        };
        mutate(qc);
        proof
    }

    fn set_signers(qc: &mut QuorumCertificate, bitmap: SignerBitmap) {
        let QcSignatures::Schnorr { signers, .. } = &mut qc.signatures;
        *signers = bitmap;
    }

    #[test]
    fn it_rejects_a_bitmap_sized_for_another_committee() {
        let proof = load_fixture::<SidechainBlockCommitProof>("commit_proof.json");
        // Nine members need a two byte bitmap, but the fixture's is one byte.
        let committee = test_committee(1..=9, |_| 1);
        let err = proof.validate_committed(4, &committee).unwrap_err();
        assert!(
            err.to_string()
                .contains("is 1 bytes, but a committee of 9 members requires 2"),
            "Expected the bitmap length error, got: {err}"
        );
    }

    #[test]
    fn it_rejects_a_bitmap_that_marks_a_padding_bit() {
        let proof = with_first_qc(|qc| set_signers(qc, SignerBitmap::from_bytes(vec![0b0001_0111])));
        let err = proof.validate_committed(3, &fixture_committee()).unwrap_err();
        assert!(
            err.to_string()
                .contains("marks member 4, but the committee has 4 members"),
            "Expected the padding bit error, got: {err}"
        );
    }

    #[test]
    fn it_rejects_a_signature_count_that_differs_from_the_signer_count() {
        let proof = with_first_qc(|qc| set_signers(qc, SignerBitmap::from_indices(4, [0, 1, 2]).unwrap()));
        let err = proof.validate_committed(3, &fixture_committee()).unwrap_err();
        assert!(
            err.to_string()
                .contains("marks 3 signers, but the certificate carries 4 signatures"),
            "Expected the signature count error, got: {err}"
        );
    }

    #[test]
    fn it_rejects_signatures_out_of_committee_order() {
        let proof = with_first_qc(|qc| schnorr_signatures(qc).swap(0, 1));
        let err = proof.validate_committed(4, &fixture_committee()).unwrap_err();
        assert!(
            err.to_string().contains("Invalid signature for QC"),
            "Expected the signature error, got: {err}"
        );
    }

    #[test]
    fn it_attributes_signatures_to_the_members_the_bitmap_marks() {
        let committee = test_committee(1..=5, |_| 1);
        let mut proof = signed_proof(&committee, 1..=4, test_header(None));
        proof.validate_committed(4, &committee).unwrap();

        // Validator 5 did not sign. Moving any signer's bit to it misattributes that signature.
        let five = committee.index_of(&validator_public_key(5)).unwrap();
        for elem in &mut proof.proof_elements {
            if let CommitProofElement::QuorumCertificate(qc) = elem {
                let indices = (0..5).filter(|&i| i != five).skip(1).chain([five]);
                set_signers(qc, SignerBitmap::from_indices(5, indices).unwrap());
            }
        }
        let err = proof.validate_committed(4, &committee).unwrap_err();
        assert!(
            err.to_string().contains("Invalid signature for QC"),
            "Expected the signature error, got: {err}"
        );
    }

    #[test]
    fn it_reports_a_committee_out_of_public_key_order_as_an_internal_error() {
        let proof = load_fixture::<SidechainBlockCommitProof>("commit_proof.json");
        let mut members = fixture_committee()
            .public_keys()
            .cloned()
            .map(|pk| (pk, 1))
            .collect::<Vec<_>>();
        members.swap(0, 1);
        let err = proof
            .validate_committed(4, &TestCommittee::in_order(members))
            .unwrap_err();
        assert!(
            matches!(err, SidechainProofValidationError::InternalError { .. }),
            "Expected an internal error, got: {err}"
        );
    }
}

#[test]
fn the_transaction_merkle_root_round_trips() {
    let mut header = test_header(Some(FixedHash::from([1u8; 32])));

    let json = serde_json::to_value(&header).unwrap();
    assert_eq!(
        json.get("transaction_merkle_root").and_then(|v| v.as_str()),
        Some("01".repeat(32).as_str())
    );
    assert_eq!(serde_json::from_value::<SidechainBlockHeader>(json).unwrap(), header);

    let bytes = borsh::to_vec(&header).unwrap();
    assert_eq!(borsh::from_slice::<SidechainBlockHeader>(&bytes).unwrap(), header);

    header.transaction_merkle_root = None;
    let json = serde_json::to_value(&header).unwrap();
    assert!(json.get("transaction_merkle_root").is_none());
    assert_eq!(serde_json::from_value::<SidechainBlockHeader>(json).unwrap(), header);
}

#[test]
fn the_vote_message_commits_to_the_version_and_view() {
    let block_id = FixedHash::from([7u8; 32]);
    let message = |protocol_version, epoch, height| {
        ProposalVoteMessage::new(protocol_version, &block_id, QuorumDecision::Accept, epoch, height).calculate_hash()
    };

    assert_ne!(message(0, 3, 9), message(1, 3, 9));
    assert_ne!(message(1, 3, 9), message(2, 3, 9));
    assert_ne!(message(0, 3, 9), message(0, 4, 9));
    assert_ne!(message(0, 3, 9), message(0, 3, 10));
}

mod batch_verification {
    use tari_common_types::types::{CompressedPublicKey, PrivateKey, UncompressedPublicKey};
    use tari_crypto::keys::SecretKey;
    use tari_sidechain::ValidatorBlockSignature;
    use tari_utilities::ByteArray;

    use super::*;

    fn with_first_qc(mutate: impl FnOnce(&mut QuorumCertificate)) -> SidechainBlockCommitProof {
        let mut proof = load_fixture::<SidechainBlockCommitProof>("commit_proof.json");
        let Some(CommitProofElement::QuorumCertificate(qc)) = proof.proof_elements.first_mut() else {
            panic!("commit_proof.json fixture must begin with a quorum certificate");
        };
        mutate(qc);
        proof
    }

    fn assert_invalid_signature(proof: &SidechainBlockCommitProof, committee: &TestCommittee) {
        let err = proof.validate_committed(4, committee).unwrap_err();
        assert!(
            err.to_string().contains("Invalid signature for QC"),
            "Expected the signature error, got: {err}"
        );
    }

    // Scalar arithmetic is modular.
    #[allow(clippy::arithmetic_side_effects)]
    fn shift_s(signature: &ValidatorBlockSignature, delta: &PrivateKey, add: bool) -> ValidatorBlockSignature {
        let s = signature.get_signature();
        let s = if add { s + delta } else { s - delta };
        ValidatorBlockSignature::new(signature.get_compressed_public_nonce().clone(), s)
    }

    #[test]
    fn it_rejects_two_invalid_signatures_whose_errors_cancel() {
        let delta = PrivateKey::from_uniform_bytes(&[9u8; 64]).unwrap();
        let proof = with_first_qc(|qc| {
            let [first, second, ..] = schnorr_signatures(qc).as_mut_slice() else {
                panic!("commit_proof.json fixture QC must have at least two signatures");
            };
            // Each signature is individually invalid, and the two errors sum to zero.
            *first = shift_s(first, &delta, true);
            *second = shift_s(second, &delta, false);
        });

        let Some(CommitProofElement::QuorumCertificate(qc)) = proof.proof_elements.first() else {
            panic!("commit_proof.json fixture must begin with a quorum certificate");
        };
        let block_id = qc.calculate_justified_block();
        let message =
            ProposalVoteMessage::new(qc.protocol_version, &block_id, qc.decision, qc.epoch, qc.height).calculate_hash();
        let QcSignatures::Schnorr { signatures, .. } = &qc.signatures;
        let committee = fixture_committee();
        for (public_key, signature) in committee.public_keys().zip(signatures).take(2) {
            let public_key = public_key.to_public_key().unwrap();
            assert!(!signature.to_schnorr_signature().unwrap().verify(&public_key, message));
        }
        assert_invalid_signature(&proof, &committee);
    }

    #[test]
    fn it_rejects_a_signature_under_the_identity_public_key() {
        let s = PrivateKey::from_uniform_bytes(&[11u8; 64]).unwrap();
        // The identity encodes as zero bytes, so it is the committee's first member and the first signer.
        let identity = CompressedPublicKey::default();
        assert!(UncompressedPublicKey::from_canonical_bytes(identity.as_bytes()).is_ok());
        let committee = TestCommittee::new(
            fixture_committee()
                .public_keys()
                .skip(1)
                .cloned()
                .chain([identity])
                .map(|pk| (pk, 1)),
        );
        let proof = with_first_qc(|qc| {
            let signatures = schnorr_signatures(qc);
            signatures.remove(0);
            // With `P` the identity, `s·G == R` satisfies the verification equation for any message.
            signatures.insert(
                0,
                ValidatorBlockSignature::new(CompressedPublicKey::from_secret_key(&s), s),
            );
        });
        assert_invalid_signature(&proof, &committee);
    }
}

#[test]
fn every_current_protocol_version_uses_jmt_hash_scheme_v1() {
    for protocol_version in [0, 1, 2, 3, u32::MAX] {
        assert_eq!(jmt_hash_scheme(protocol_version), JmtHashScheme::V1);
    }
}
