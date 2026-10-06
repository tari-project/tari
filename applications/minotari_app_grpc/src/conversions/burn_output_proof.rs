// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use tari_common_types::{
    burn_proof::{BurnOutputProof, MmrInclusionProof, OutputHashPreimage},
    types::{CompressedCommitment, CompressedPublicKey, FixedHash},
};
use tari_utilities::ByteArray;

use crate::tari_rpc as grpc;

impl From<BurnOutputProof> for grpc::BurnOutputProof {
    fn from(proof: BurnOutputProof) -> Self {
        Self {
            block_hash: proof.block_hash.to_vec(),
            block_height: proof.block_height,
            output: Some(proof.output.into()),
            normal_output_proof: Some(proof.normal_output_proof.into()),
            normal_output_mr: proof.normal_output_mr.to_vec(),
            block_output_proof: Some(proof.block_output_proof.into()),
        }
    }
}

impl TryFrom<grpc::BurnOutputProof> for BurnOutputProof {
    type Error = String;

    fn try_from(proof: grpc::BurnOutputProof) -> Result<Self, Self::Error> {
        Ok(Self {
            block_hash: to_fixed_hash(proof.block_hash)?,
            block_height: proof.block_height,
            output: proof.output.ok_or("output not provided")?.try_into()?,
            normal_output_proof: proof
                .normal_output_proof
                .ok_or("normal_output_proof not provided")?
                .try_into()?,
            normal_output_mr: to_fixed_hash(proof.normal_output_mr)?,
            block_output_proof: proof
                .block_output_proof
                .ok_or("block_output_proof not provided")?
                .try_into()?,
        })
    }
}

impl From<MmrInclusionProof> for grpc::MmrInclusionProof {
    fn from(proof: MmrInclusionProof) -> Self {
        Self {
            leaf_index: proof.leaf_index,
            mmr_size: proof.mmr_size,
            path: proof.path.iter().map(|hash| hash.to_vec()).collect(),
            peaks: proof.peaks.iter().map(|hash| hash.to_vec()).collect(),
        }
    }
}

impl TryFrom<grpc::MmrInclusionProof> for MmrInclusionProof {
    type Error = String;

    fn try_from(proof: grpc::MmrInclusionProof) -> Result<Self, Self::Error> {
        Ok(Self {
            leaf_index: proof.leaf_index,
            mmr_size: proof.mmr_size,
            path: proof.path.into_iter().map(to_fixed_hash).collect::<Result<_, _>>()?,
            peaks: proof.peaks.into_iter().map(to_fixed_hash).collect::<Result<_, _>>()?,
        })
    }
}

impl From<OutputHashPreimage> for grpc::OutputHashPreimage {
    fn from(output: OutputHashPreimage) -> Self {
        let sig = &output.metadata_signature;
        Self {
            version: u32::from(output.version),
            features: output.features,
            commitment: output.commitment.to_vec(),
            rangeproof_hash: output.rangeproof_hash.to_vec(),
            script: output.script,
            sender_offset_public_key: output.sender_offset_public_key.to_vec(),
            metadata_signature: Some(grpc::ComAndPubSignature {
                ephemeral_commitment: sig.ephemeral_commitment().to_vec(),
                ephemeral_pubkey: sig.ephemeral_pubkey().to_vec(),
                u_a: sig.u_a().to_vec(),
                u_x: sig.u_x().to_vec(),
                u_y: sig.u_y().to_vec(),
            }),
            covenant: output.covenant,
            encrypted_data: output.encrypted_data,
            minimum_value_promise: output.minimum_value_promise,
        }
    }
}

impl TryFrom<grpc::OutputHashPreimage> for OutputHashPreimage {
    type Error = String;

    fn try_from(output: grpc::OutputHashPreimage) -> Result<Self, Self::Error> {
        Ok(Self {
            version: u8::try_from(output.version).map_err(|_| "Invalid version: overflowed u8")?,
            features: output.features,
            commitment: CompressedCommitment::from_canonical_bytes(&output.commitment)
                .map_err(|e| format!("Invalid commitment: {e}"))?,
            rangeproof_hash: to_fixed_hash(output.rangeproof_hash)?,
            script: output.script,
            sender_offset_public_key: CompressedPublicKey::from_canonical_bytes(&output.sender_offset_public_key)
                .map_err(|e| format!("Invalid sender_offset_public_key: {e}"))?,
            metadata_signature: output
                .metadata_signature
                .ok_or("metadata_signature not provided")?
                .try_into()?,
            covenant: output.covenant,
            encrypted_data: output.encrypted_data,
            minimum_value_promise: output.minimum_value_promise,
        })
    }
}

fn to_fixed_hash(bytes: Vec<u8>) -> Result<FixedHash, String> {
    FixedHash::try_from(bytes).map_err(|e| format!("Invalid hash: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_round_trips() {
        let mmr_proof = MmrInclusionProof {
            leaf_index: 1,
            mmr_size: 3,
            path: vec![FixedHash::from([2u8; 32])],
            peaks: vec![FixedHash::from([8u8; 32])],
        };
        let proof = BurnOutputProof {
            block_hash: FixedHash::from([1u8; 32]),
            block_height: 10,
            output: OutputHashPreimage {
                version: 1,
                features: vec![1, 2, 3],
                commitment: Default::default(),
                rangeproof_hash: FixedHash::from([3u8; 32]),
                script: vec![4],
                sender_offset_public_key: Default::default(),
                metadata_signature: Default::default(),
                covenant: vec![0],
                encrypted_data: vec![5; 80],
                minimum_value_promise: 7,
            },
            normal_output_proof: mmr_proof.clone(),
            normal_output_mr: FixedHash::from([6u8; 32]),
            block_output_proof: mmr_proof,
        };
        let grpc_proof = grpc::BurnOutputProof::from(proof.clone());
        assert_eq!(BurnOutputProof::try_from(grpc_proof).unwrap(), proof);
    }
}
