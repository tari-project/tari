// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use log::debug;
use tari_common_types::types::{FixedHash, UncompressedPublicKey};
use tari_utilities::ByteArray;

use crate::{
    CommitProofElement,
    DecodedValidatorBlockSignature,
    ProposalVoteMessage,
    QcSignatures,
    QuorumCertificate,
    QuorumDecision,
    SidechainBlockHeader,
    SidechainProofValidationError,
    SignerBitmap,
    SigningCommittee,
};

const LOG_TARGET: &str = "c::sidechain::validations";

/// The maximum number of signatures a quorum certificate may contain. A valid quorum certificate contains at most one
/// signature per committee member, so this is an upper bound for any committee size. It is enforced at the proto
/// deserialization boundary and again during proof validation, in both cases before any signature is verified, so that
/// the work an untrusted quorum certificate can cause is bounded.
pub const MAX_QC_SIGNATURES: usize = 1000;

pub fn check_proof_elements(
    header: &SidechainBlockHeader,
    proof_elements: &[CommitProofElement],
    committee: &dyn SigningCommittee,
    expected_decision: QuorumDecision,
    quorum_threshold: u64,
) -> Result<(), SidechainProofValidationError> {
    check_header_commits_to_its_fields(header)?;
    check_proof_elements_num_qcs(proof_elements, 3)?;

    let mut last_parent = None::<&FixedHash>;
    let mut proven_3_chain = 0usize;
    for elem in proof_elements {
        match elem {
            CommitProofElement::QuorumCertificate(qc) => {
                let justifies = qc.calculate_justified_block();
                debug!(target: LOG_TARGET, "Validating quorum certificate that justifies: {justifies}");
                validate_qc(qc, quorum_threshold, committee, expected_decision)?;
                debug!(target: LOG_TARGET, "Quorum certificate OK");

                if let Some(last_parent) = last_parent &&
                    *last_parent != justifies
                {
                    return Err(SidechainProofValidationError::InvalidProof {
                        details: format!(
                            "Parent block ID {last_parent} does not match the parent block ID {justifies} in the \
                             quorum certificate"
                        ),
                    });
                }

                if proven_3_chain < 3 {
                    proven_3_chain = proven_3_chain.saturating_add(1);
                    debug!(target: LOG_TARGET, "3-chain rule: {proven_3_chain} of 3 proven");
                }

                debug!(target: LOG_TARGET, "Setting last parent to {}", qc.parent_id);
                last_parent = Some(&qc.parent_id);
            },
            CommitProofElement::ChainLinks(chain) => {
                if proven_3_chain != 3 {
                    return Err(SidechainProofValidationError::InvalidProof {
                        details: format!(
                            "A 3-chain must be proven before a dummy chain. A chain of length {proven_3_chain} was \
                             proven"
                        ),
                    });
                }

                let Some(parent) = last_parent else {
                    return Err(SidechainProofValidationError::InvalidProof {
                        details: "Dummy chain must be preceded by a quorum certificate".to_string(),
                    });
                };
                if chain.is_empty() {
                    return Err(SidechainProofValidationError::InvalidProof {
                        details: "Dummy chain must contain at least one element".to_string(),
                    });
                }
                let mut expected = *parent;
                for (i, link) in chain.iter().enumerate() {
                    let block_id = link.calc_block_id();
                    debug!(target: LOG_TARGET, "Validating dummy chain link {i}: link {block_id} ?= last parent {expected}");
                    if block_id != expected {
                        return Err(SidechainProofValidationError::InvalidProof {
                            details: format!(
                                "Block ID in chain link at index {i} does not match the parent block ID in the quorum \
                                 certificate. Expected {expected}, but got {block_id}",
                            ),
                        });
                    }
                    expected = link.parent_id;
                    last_parent = Some(&link.parent_id);
                }
            },
        }
    }

    let Some(last_elem) = proof_elements.last() else {
        // Not reachable because we check length of proof elements above
        return Err(SidechainProofValidationError::InvalidProof {
            details: "BUG: Proof must contain at least one proof element".to_string(),
        });
    };

    let Some(last_parent) = last_parent else {
        // Not reachable because we check length of proof elements above
        return Err(SidechainProofValidationError::InvalidProof {
            details: "BUG: Proof must contain at least proof element (last_parent == None)".to_string(),
        });
    };

    let justified = match last_elem {
        CommitProofElement::QuorumCertificate(qc) => qc.calculate_justified_block(),
        CommitProofElement::ChainLinks(links) => {
            links
                .last()
                .map(|link| link.parent_id)
                // Already checked that links is not empty
                .ok_or_else(|| SidechainProofValidationError::InvalidProof {
                    details: "BUG: chain must contain at least one element".to_string(),
                })?
        },
    };

    let header_block_id = header.calculate_block_id();
    if justified != header_block_id {
        return Err(SidechainProofValidationError::InvalidProof {
            details: format!(
                "Last parent block ID does not match the block ID in the header. Expected {header_block_id}, but got \
                 {last_parent}"
            ),
        });
    }

    Ok(())
}

/// Rejects a header that carries a field its protocol version's hash preimage does not cover, since nothing
/// authenticates such a field.
fn check_header_commits_to_its_fields(header: &SidechainBlockHeader) -> Result<(), SidechainProofValidationError> {
    if header.protocol_version == 0 && header.transaction_merkle_root.is_some() {
        return Err(SidechainProofValidationError::InvalidProof {
            details: "Header claims protocol version 0, which commits to no transaction merkle root, but carries one"
                .to_string(),
        });
    }
    Ok(())
}

pub fn check_proof_elements_num_qcs(
    proof_elems: &[CommitProofElement],
    expected_len: usize,
) -> Result<(), SidechainProofValidationError> {
    const MAX_PROOF_ELEMS: usize = 20;
    if proof_elems.len() > MAX_PROOF_ELEMS {
        return Err(SidechainProofValidationError::InvalidProof {
            details: format!(
                "Commit Proof contained too many proof elements. Expected at most {} but got {}",
                MAX_PROOF_ELEMS,
                proof_elems.len()
            ),
        });
    }
    let num_qcs = proof_elems
        .iter()
        .filter(|elem| matches!(elem, CommitProofElement::QuorumCertificate(_)))
        .count();
    if num_qcs < expected_len {
        return Err(SidechainProofValidationError::InvalidProof {
            details: format!(
                "Expected at least {} QC proof elements, but got {}",
                expected_len,
                proof_elems.len()
            ),
        });
    }
    Ok(())
}

fn validate_qc(
    qc: &QuorumCertificate,
    quorum_threshold: u64,
    committee: &dyn SigningCommittee,
    quorum_decision: QuorumDecision,
) -> Result<(), SidechainProofValidationError> {
    if qc.signatures.num_signatures() > MAX_QC_SIGNATURES {
        return Err(SidechainProofValidationError::InvalidProof {
            details: format!(
                "Quorum certificate must contain at most {} signatures but contained {}",
                MAX_QC_SIGNATURES,
                qc.signatures.num_signatures()
            ),
        });
    }

    if qc.decision != quorum_decision {
        return Err(SidechainProofValidationError::InvalidProof {
            details: format!(
                "Quorum certificate decision must be {:?} but was {:?}",
                quorum_decision, qc.decision
            ),
        });
    }

    qc.signatures.check_well_formed(committee.len())?;

    let block_id = qc.calculate_justified_block();
    match &qc.signatures {
        QcSignatures::Schnorr { signers, signatures } => {
            let members = resolve_signers(signers, committee)?;

            // Each signer is a distinct member and the batch below accepts all of their signatures or none, so this
            // is the power of the signers whose signatures verify. It is checked first so that a QC short of quorum
            // costs no verification. An empty QC has no power, so it is rejected here unless the threshold is 0.
            let signed_power = members.iter().fold(0u64, |acc, (_, power)| acc.saturating_add(*power));
            if signed_power < quorum_threshold {
                return Err(SidechainProofValidationError::InvalidProof {
                    details: format!(
                        "Quorum certificate signers hold {signed_power} voting power but at least {quorum_threshold} \
                         is required"
                    ),
                });
            }

            let mut decoded = Vec::with_capacity(signatures.len());
            for ((public_key, _), signature) in members.iter().zip(signatures) {
                let public_key = UncompressedPublicKey::from_canonical_bytes(public_key).map_err(|e| {
                    SidechainProofValidationError::internal_error(format!(
                        "Committee member public key {} is not a valid public key: {e}",
                        hex::encode(public_key)
                    ))
                })?;
                let signature = signature
                    .to_schnorr_signature()
                    .map_err(|_| invalid_qc_signature(&block_id))?;
                decoded.push((public_key, signature));
            }

            // Every member of a QC signs the same message.
            let message =
                ProposalVoteMessage::new(qc.protocol_version, &block_id, quorum_decision, qc.epoch, qc.height)
                    .calculate_hash();
            let batch = decoded
                .iter()
                .map(|(public_key, signature)| (signature, public_key, message))
                .collect::<Vec<_>>();
            if !DecodedValidatorBlockSignature::verify_batch(&batch) {
                return Err(invalid_qc_signature(&block_id));
            }
        },
    }
    Ok(())
}

/// The public key and voting power of each signer in `signers`, in committee order.
fn resolve_signers<'a>(
    signers: &SignerBitmap,
    committee: &'a dyn SigningCommittee,
) -> Result<Vec<(&'a [u8], u64)>, SidechainProofValidationError> {
    let mut members = Vec::<(&[u8], u64)>::with_capacity(signers.count());
    for index in signers.iter() {
        let member = committee.member(index).ok_or_else(|| {
            SidechainProofValidationError::internal_error(format!(
                "Committee of {} members has no member at index {index}",
                committee.len()
            ))
        })?;
        // Ascending keys mean every signer is a distinct validator whose power counts once.
        if let Some((previous, _)) = members.last() &&
            *previous >= member.0
        {
            return Err(SidechainProofValidationError::internal_error(format!(
                "Committee lists public key {} at index {index}, which is not after the key before it",
                hex::encode(member.0)
            )));
        }
        members.push(member);
    }
    Ok(members)
}

fn invalid_qc_signature(block_id: &FixedHash) -> SidechainProofValidationError {
    SidechainProofValidationError::InvalidProof {
        details: format!("Invalid signature for QC for block ID {block_id}"),
    }
}
