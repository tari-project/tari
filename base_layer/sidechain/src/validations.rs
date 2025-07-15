// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{collections::HashSet, hash::Hash};

use log::debug;
use tari_common_types::types::FixedHash;

use crate::{
    CheckVnFunc,
    CommitProofElement,
    QuorumCertificate,
    QuorumDecision,
    SidechainBlockHeader,
    SidechainProofValidationError,
};

const LOG_TARGET: &str = "c::sidechain::validations";

pub fn check_proof_elements(
    header: &SidechainBlockHeader,
    proof_elements: &[CommitProofElement],
    check_vn: &CheckVnFunc<'_>,
    expected_decision: QuorumDecision,
    quorum_threshold: usize,
) -> Result<(), SidechainProofValidationError> {
    check_proof_elements_num_qcs(proof_elements, 3)?;

    let mut last_parent = None::<&FixedHash>;
    let mut proven_3_chain = 0usize;
    for elem in proof_elements {
        match elem {
            CommitProofElement::QuorumCertificate(qc) => {
                let justifies = qc.calculate_justified_block();
                debug!(target: LOG_TARGET, "Validating quorum certificate that justifies: {justifies}");
                validate_qc(qc, quorum_threshold, check_vn, expected_decision)?;
                debug!(target: LOG_TARGET, "Quorum certificate OK");

                if let Some(last_parent) = last_parent {
                    if *last_parent != justifies {
                        return Err(SidechainProofValidationError::InvalidProof {
                            details: format!(
                                "Parent block ID {last_parent} does not match the parent block ID {justifies} in the \
                                 quorum certificate"
                            ),
                        });
                    }
                }

                if proven_3_chain < 3 {
                    proven_3_chain += 1;
                    debug!(target: LOG_TARGET, "3-chain rule: {} of 3 proven", proven_3_chain);
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
                "Last parent block ID does not match the block ID in the header. Expected {}, but got {}",
                header_block_id, last_parent,
            ),
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
    quorum_threshold: usize,
    check_vn: &CheckVnFunc<'_>,
    quorum_decision: QuorumDecision,
) -> Result<(), SidechainProofValidationError> {
    if qc.signatures.len() < quorum_threshold {
        return Err(SidechainProofValidationError::InvalidProof {
            details: format!(
                "Quorum certificate must contain at least {} signatures but contained {}",
                quorum_threshold,
                qc.signatures.len()
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

    if has_duplicates(qc.signatures.iter().map(|s| &s.public_key)) {
        return Err(SidechainProofValidationError::InvalidProof {
            details: "Quorum certificate contains more than one signature from a single validator".to_string(),
        });
    }

    let block_id = qc.calculate_justified_block();
    for sig in &qc.signatures {
        if !check_vn(&sig.public_key)? {
            return Err(SidechainProofValidationError::InvalidProof {
                details: format!(
                    "QC was signed by public key {} that is not in the active validator set",
                    sig.public_key
                ),
            });
        }

        if !sig.verify(&block_id, quorum_decision) {
            return Err(SidechainProofValidationError::InvalidProof {
                details: format!("Invalid signature for QC for block ID {block_id}",),
            });
        }
    }
    Ok(())
}

fn has_duplicates<I, T>(iter: I) -> bool
where
    I: IntoIterator<Item = T> + ExactSizeIterator,
    T: Eq + Hash,
{
    let mut set = HashSet::with_capacity(iter.len());
    for item in iter {
        if !set.insert(item) {
            return true;
        }
    }
    false
}
