// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use tari_common_types::{burn_proof::KernelMerkleProof, types::FixedHash};

use crate::tari_rpc as grpc;

impl From<KernelMerkleProof> for grpc::KernelMerkleProof {
    fn from(proof: KernelMerkleProof) -> Self {
        Self {
            block_hash: proof.block_hash.to_vec(),
            leaf_index: proof.leaf_index,
            mmr_size: proof.mmr_size,
            path: proof.path.iter().map(|hash| hash.to_vec()).collect(),
            peaks: proof.peaks.iter().map(|hash| hash.to_vec()).collect(),
        }
    }
}

impl TryFrom<grpc::KernelMerkleProof> for KernelMerkleProof {
    type Error = String;

    fn try_from(proof: grpc::KernelMerkleProof) -> Result<Self, Self::Error> {
        let to_fixed_hash = |bytes: Vec<u8>| FixedHash::try_from(bytes).map_err(|e| format!("Invalid hash: {e}"));
        Ok(Self {
            block_hash: to_fixed_hash(proof.block_hash)?,
            leaf_index: proof.leaf_index,
            mmr_size: proof.mmr_size,
            path: proof.path.into_iter().map(to_fixed_hash).collect::<Result<_, _>>()?,
            peaks: proof.peaks.into_iter().map(to_fixed_hash).collect::<Result<_, _>>()?,
        })
    }
}
