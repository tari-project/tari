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
    #[error(
        "Expected root hash is the empty-tree root, which proves the absence of any key. Use \
         `verify_exclusion_or_empty_tree` if the root is authenticated for this specific tree."
    )]
    EmptyTreeRoot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("Nibble out of range: {0}")]
pub struct NibbleOutOfRange(pub u8);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NibblePathError {
    #[error("An odd-length NibblePath must have at least one byte")]
    EmptyOddPath,
    #[error("NibblePath with odd number of nibbles must have a zero last nibble")]
    NonZeroTrailingNibble,
    #[error("Cannot truncate NibblePath of {num_nibbles} nibbles to {len} nibbles")]
    TruncateBeyondLength { len: usize, num_nibbles: usize },
    #[error("NibblePath has {num_nibbles} nibbles, max is {max}")]
    TooLong { num_nibbles: usize, max: usize },
    #[error("NibblePath has {num_nibbles} nibbles but {num_bytes} bytes")]
    LengthMismatch { num_nibbles: usize, num_bytes: usize },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InternalNodeError {
    #[error("InternalNode child is Null")]
    NullChild,
    #[error("InternalNode has no children")]
    NoChildren,
    #[error("InternalNode has a single leaf child, which must be collapsed into the leaf")]
    SingleLeafChild,
    #[error("InternalNode leaf count overflow")]
    LeafCountOverflow,
}
