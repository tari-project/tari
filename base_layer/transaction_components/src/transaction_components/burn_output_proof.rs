// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Hashing and verification of a [`BurnOutputProof`], the proof that a burn output was mined in a block.

use borsh::BorshDeserialize;
use tari_common_types::{
    burn_proof::{BurnOutputProof, MmrInclusionProof, OutputHashPreimage},
    types::FixedHash,
};
use tari_hashing::hashers::InputMmrHasherBlake256;
use tari_mmr::{
    MerkleProof,
    MerkleProofError,
    common::{LeafIndex, checked_n_leaves},
};
use tari_script::TariScript;
use thiserror::Error;

use crate::{
    MicroMinotari,
    transaction_components::{
        EncryptedData,
        OutputFeatures,
        TransactionOutput,
        TransactionOutputVersion,
        covenants::Covenant,
        hash_output,
    },
};

#[derive(Debug, Error)]
pub enum BurnOutputProofError {
    #[error("Invalid output field `{field}`: {details}")]
    InvalidOutputField { field: &'static str, details: String },
    #[error("The {mmr} MMR proof is malformed: {details}")]
    MalformedMmrProof { mmr: &'static str, details: String },
    #[error("The normal output MMR root is not the last leaf of the block output MMR")]
    NormalOutputRootNotLastLeaf,
    #[error("The {mmr} MMR proof does not verify: {source}")]
    InvalidMmrProof {
        mmr: &'static str,
        source: MerkleProofError,
    },
}

const NORMAL_OUTPUT_MMR: &str = "normal output";
const BLOCK_OUTPUT_MMR: &str = "block output";

impl From<&TransactionOutput> for OutputHashPreimage {
    fn from(output: &TransactionOutput) -> Self {
        Self {
            version: output.version.as_u8(),
            features: consensus_encode(&output.features),
            commitment: output.commitment.clone(),
            rangeproof_hash: output
                .proof
                .as_ref()
                .map(|rp| rp.hash())
                .unwrap_or_else(FixedHash::zero),
            script: consensus_encode(&output.script),
            sender_offset_public_key: output.sender_offset_public_key.clone(),
            metadata_signature: output.metadata_signature.clone(),
            covenant: consensus_encode(&output.covenant),
            encrypted_data: consensus_encode(&output.encrypted_data),
            minimum_value_promise: output.minimum_value_promise.as_u64(),
        }
    }
}

/// Decoding and hashing of an [`OutputHashPreimage`]
pub trait OutputHashPreimageExt {
    /// Decodes the output features.
    fn decode_features(&self) -> Result<OutputFeatures, BurnOutputProofError>;

    /// Computes the output hash, as `TransactionOutput::hash` does. Every encoded field must decode canonically.
    fn hash(&self) -> Result<FixedHash, BurnOutputProofError>;
}

impl OutputHashPreimageExt for OutputHashPreimage {
    fn decode_features(&self) -> Result<OutputFeatures, BurnOutputProofError> {
        consensus_decode("features", &self.features)
    }

    fn hash(&self) -> Result<FixedHash, BurnOutputProofError> {
        let version = TransactionOutputVersion::try_from(self.version).map_err(|details| {
            BurnOutputProofError::InvalidOutputField {
                field: "version",
                details,
            }
        })?;
        let script: TariScript = consensus_decode("script", &self.script)?;
        let covenant: Covenant = consensus_decode("covenant", &self.covenant)?;
        let encrypted_data: EncryptedData = consensus_decode("encrypted_data", &self.encrypted_data)?;
        Ok(hash_output(
            version,
            &self.decode_features()?,
            &self.commitment,
            &self.rangeproof_hash,
            &script,
            &self.sender_offset_public_key,
            &self.metadata_signature,
            &covenant,
            &encrypted_data,
            MicroMinotari::from(self.minimum_value_promise),
        ))
    }
}

/// Verification of a [`BurnOutputProof`]
pub trait BurnOutputProofExt {
    /// Verifies that the output is included in the block whose header has `trusted_block_output_mr` as its
    /// `block_output_mr`. This only proves inclusion: callers must check the output itself (type, features,
    /// commitment) and that `trusted_block_output_mr` belongs to the header with `block_hash`/`block_height`.
    ///
    /// Soundness rests on the output hash being domain separated from MMR nodes, so only the hash of an output mined
    /// in that block can be proven. The MMR sizes and intermediate roots are chosen by the prover and not committed
    /// to by the root, so proofs are not unique: the same output can have several valid proofs, and
    /// `normal_output_mr` is not guaranteed to be the block's real normal output MMR root. Key anything that must be
    /// unique per burn (e.g. claim deduplication) on the output commitment or output hash, never on the proof.
    fn verify(&self, trusted_block_output_mr: &FixedHash) -> Result<(), BurnOutputProofError>;
}

impl BurnOutputProofExt for BurnOutputProof {
    fn verify(&self, trusted_block_output_mr: &FixedHash) -> Result<(), BurnOutputProofError> {
        let output_hash = self.output.hash()?;
        verify_mmr_proof(
            NORMAL_OUTPUT_MMR,
            &self.normal_output_proof,
            &self.normal_output_mr,
            &output_hash,
        )?;

        // Honest proofs have the normal output MMR root as the last leaf of the block output MMR, after the coinbase
        // outputs. This rejects malformed proofs but does not bind `normal_output_mr` to the real root, because the
        // prover chooses the MMR size (see `verify`).
        let n_leaves = usize::try_from(self.block_output_proof.mmr_size)
            .ok()
            .and_then(checked_n_leaves)
            .ok_or_else(|| BurnOutputProofError::MalformedMmrProof {
                mmr: BLOCK_OUTPUT_MMR,
                details: format!("invalid MMR size {}", self.block_output_proof.mmr_size),
            })?;
        if n_leaves.checked_sub(1).and_then(|i| u64::try_from(i).ok()) != Some(self.block_output_proof.leaf_index) {
            return Err(BurnOutputProofError::NormalOutputRootNotLastLeaf);
        }
        verify_mmr_proof(
            BLOCK_OUTPUT_MMR,
            &self.block_output_proof,
            trusted_block_output_mr,
            &self.normal_output_mr,
        )
    }
}

fn verify_mmr_proof(
    mmr: &'static str,
    proof: &MmrInclusionProof,
    root: &FixedHash,
    leaf: &FixedHash,
) -> Result<(), BurnOutputProofError> {
    let to_usize = |value: u64, name: &str| {
        usize::try_from(value).map_err(|_| BurnOutputProofError::MalformedMmrProof {
            mmr,
            details: format!("{name} {value} is out of range"),
        })
    };
    let merkle_proof = MerkleProof {
        mmr_size: to_usize(proof.mmr_size, "MMR size")?,
        path: proof.path.iter().map(|hash| hash.to_vec()).collect(),
        peaks: proof.peaks.iter().map(|hash| hash.to_vec()).collect(),
    };
    merkle_proof
        .verify_leaf::<InputMmrHasherBlake256>(
            root.as_slice(),
            leaf.as_slice(),
            LeafIndex(to_usize(proof.leaf_index, "leaf index")?),
        )
        .map_err(|source| BurnOutputProofError::InvalidMmrProof { mmr, source })
}

fn consensus_encode<T: borsh::BorshSerialize>(value: &T) -> Vec<u8> {
    let mut buf = Vec::new();
    // Writing to a Vec cannot fail
    value
        .serialize(&mut buf)
        .expect("borsh serialization into a Vec is infallible");
    buf
}

fn consensus_decode<T: BorshDeserialize>(field: &'static str, bytes: &[u8]) -> Result<T, BurnOutputProofError> {
    borsh::from_slice(bytes).map_err(|e| BurnOutputProofError::InvalidOutputField {
        field,
        details: e.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use tari_mmr::{Hash, MerkleMountainRange};

    use super::*;
    use crate::{
        key_manager::KeyManager,
        test_helpers::{TestParams, UtxoTestParams},
        transaction_components::OutputType,
    };

    type OutputMmr = MerkleMountainRange<InputMmrHasherBlake256, Vec<Hash>>;

    fn create_output(features: OutputFeatures) -> TransactionOutput {
        let key_manager = KeyManager::new_random().unwrap();
        let params = TestParams::new(&key_manager);
        params
            .create_output(
                UtxoTestParams {
                    value: 100.into(),
                    features,
                    ..Default::default()
                },
                &key_manager,
            )
            .unwrap()
            .to_transaction_output()
            .unwrap()
    }

    fn mmr_proof(mmr: &OutputMmr, leaf_index: usize) -> MmrInclusionProof {
        let proof = MerkleProof::for_leaf_node(mmr, LeafIndex(leaf_index)).unwrap();
        MmrInclusionProof {
            leaf_index: leaf_index as u64,
            mmr_size: proof.mmr_size as u64,
            path: proof
                .path
                .into_iter()
                .map(|h| FixedHash::try_from(h).unwrap())
                .collect(),
            peaks: proof
                .peaks
                .into_iter()
                .map(|h| FixedHash::try_from(h).unwrap())
                .collect(),
        }
    }

    /// Builds block output MMRs the way the base node does, with `num_coinbases` coinbases and the burn at `burn_index`
    /// among `num_normal` normal outputs. Returns the proof and the block output MR.
    fn build_proof(num_coinbases: usize, num_normal: usize, burn_index: usize) -> (BurnOutputProof, FixedHash) {
        let burn = create_output(OutputFeatures {
            output_type: OutputType::Burn,
            ..Default::default()
        });
        let mut block_output_mmr = OutputMmr::new(Vec::new());
        let mut normal_output_mmr = OutputMmr::new(Vec::new());
        for i in 0..num_coinbases {
            block_output_mmr
                .push(FixedHash::from([u8::try_from(i).unwrap(); 32]).to_vec())
                .unwrap();
        }
        for i in 0..num_normal {
            if i == burn_index {
                normal_output_mmr.push(burn.hash().to_vec()).unwrap();
            } else {
                normal_output_mmr
                    .push(FixedHash::from([0x80 | u8::try_from(i).unwrap(); 32]).to_vec())
                    .unwrap();
            }
        }
        let normal_output_mr = FixedHash::try_from(normal_output_mmr.get_merkle_root().unwrap()).unwrap();
        block_output_mmr.push(normal_output_mr.to_vec()).unwrap();
        let block_output_mr = FixedHash::try_from(block_output_mmr.get_merkle_root().unwrap()).unwrap();

        let proof = BurnOutputProof {
            block_hash: FixedHash::zero(),
            block_height: 1,
            output: OutputHashPreimage::from(&burn),
            normal_output_proof: mmr_proof(&normal_output_mmr, burn_index),
            normal_output_mr,
            block_output_proof: mmr_proof(&block_output_mmr, num_coinbases),
        };
        (proof, block_output_mr)
    }

    #[test]
    fn preimage_hashes_to_the_output_hash() {
        let output = create_output(OutputFeatures {
            output_type: OutputType::Burn,
            ..Default::default()
        });
        let preimage = OutputHashPreimage::from(&output);
        assert_eq!(preimage.hash().unwrap(), output.hash());
        assert_eq!(preimage.decode_features().unwrap(), output.features);

        let json = serde_json::to_string(&preimage).unwrap();
        let preimage = serde_json::from_str::<OutputHashPreimage>(&json).unwrap();
        assert_eq!(preimage.hash().unwrap(), output.hash());
    }

    #[test]
    fn it_rejects_trailing_bytes_in_an_encoded_field() {
        let output = create_output(OutputFeatures::default());
        let mut preimage = OutputHashPreimage::from(&output);
        preimage.features.push(0);
        let err = preimage.hash().unwrap_err();
        assert!(matches!(err, BurnOutputProofError::InvalidOutputField {
            field: "features",
            ..
        }));
    }

    #[test]
    fn it_verifies_proofs_for_varied_block_shapes() {
        // (coinbases, normal outputs, burn index)
        for (num_coinbases, num_normal, burn_index) in [(0, 1, 0), (1, 1, 0), (2, 3, 2), (5, 7, 3), (8, 16, 15)] {
            let (proof, block_output_mr) = build_proof(num_coinbases, num_normal, burn_index);
            proof
                .verify(&block_output_mr)
                .unwrap_or_else(|e| panic!("({num_coinbases}, {num_normal}, {burn_index}): {e}"));
        }
    }

    #[test]
    fn it_rejects_a_wrong_root() {
        let (proof, _) = build_proof(1, 3, 1);
        let err = proof.verify(&FixedHash::from([1u8; 32])).unwrap_err();
        assert!(matches!(err, BurnOutputProofError::InvalidMmrProof {
            mmr: BLOCK_OUTPUT_MMR,
            ..
        }));
    }

    #[test]
    fn it_rejects_a_tampered_output() {
        let (mut proof, block_output_mr) = build_proof(1, 3, 1);
        proof.output.minimum_value_promise = 1;
        let err = proof.verify(&block_output_mr).unwrap_err();
        assert!(matches!(err, BurnOutputProofError::InvalidMmrProof {
            mmr: NORMAL_OUTPUT_MMR,
            ..
        }));
    }

    #[test]
    fn it_rejects_a_normal_output_root_that_is_not_the_last_leaf() {
        let (mut proof, block_output_mr) = build_proof(2, 3, 1);
        proof.block_output_proof.leaf_index = 1;
        let err = proof.verify(&block_output_mr).unwrap_err();
        assert!(matches!(err, BurnOutputProofError::NormalOutputRootNotLastLeaf));
    }

    #[test]
    fn it_rejects_an_invalid_mmr_size() {
        let (mut proof, block_output_mr) = build_proof(2, 3, 1);
        proof.block_output_proof.mmr_size = u64::MAX;
        let err = proof.verify(&block_output_mr).unwrap_err();
        assert!(matches!(err, BurnOutputProofError::MalformedMmrProof { .. }));
    }
}
