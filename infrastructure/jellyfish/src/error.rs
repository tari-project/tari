//   Copyright 2024 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use crate::{LeafKey, TreeHash};

#[derive(Debug, thiserror::Error)]
pub enum JmtProofVerifyError {
    #[error("Sparse Merkle Tree proof has more than 256 ({num_siblings}) siblings.")]
    TooManySiblings { num_siblings: usize },
    #[error("Keys do not match. Key in proof: {actual_key}. Expected key: {expected_key}.")]
    KeyMismatch { actual_key: LeafKey, expected_key: LeafKey },
    #[error("Value hashes do not match. Value hash in proof: {actual}. Expected value hash: {expected}.")]
    ValueMismatch { actual: TreeHash, expected: TreeHash },
    #[error("Expected inclusion proof. Found non-inclusion proof.")]
    ExpectedInclusionProof,
    #[error("Expected non-inclusion proof, but key exists in proof.")]
    ExpectedNonInclusionProof,
    #[error(
        "Key would not have ended up in the subtree where the provided key in proof is the only existing  key, if it \
         existed. So this is not a valid non-inclusion proof."
    )]
    InvalidNonInclusionProof,
    #[error(
        "Root hashes do not match. Actual root hash: {actual_root_hash}. Expected root hash: {expected_root_hash}."
    )]
    RootHashMismatch {
        actual_root_hash: TreeHash,
        expected_root_hash: TreeHash,
    },
}
