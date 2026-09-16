// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use tari_common_types::types::{BlockHash, FixedHash};
use tari_hashing::hashers::KernelMmrHasherBlake256;
use tari_mmr::{MerkleProof, MerkleProofError, common::LeafIndex};

#[derive(Debug, Clone)]
pub struct KernelMerkleProof {
    pub merkle_proof: MerkleProof,
    pub leaf_index: LeafIndex,
    pub kernel_hash: FixedHash,
    pub block_hash: BlockHash,
    /// The height of the block the kernel was mined in. Used by the wallet to derive the L1 epoch the burn
    /// belongs to, so a downstream (L2) claim can be deferred until that epoch has been synced.
    pub block_height: u64,
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
