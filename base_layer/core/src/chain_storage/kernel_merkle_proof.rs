// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use tari_common_types::types::{BlockHash, FixedHash};
use tari_hashing::hashers::KernelMmrHasherBlake256;
use tari_mmr::{common::LeafIndex, MerkleProof, MerkleProofError};

#[derive(Debug, Clone)]
pub struct KernelMerkleProof {
    pub merkle_proof: MerkleProof,
    pub leaf_index: LeafIndex,
    pub kernel_hash: FixedHash,
    pub block_hash: BlockHash,
}

impl KernelMerkleProof {
    pub fn verify(&self, trusted_root: &FixedHash) -> Result<(), MerkleProofError> {
        self.merkle_proof.verify_leaf::<KernelMmrHasherBlake256>(
            trusted_root.as_slice(),
            self.kernel_hash.as_slice(),
            self.leaf_index,
        )
    }
}
