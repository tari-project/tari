// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::any::type_name;

use tari_common_types::{
    burn_proof::{EncodedMerkleProof, KernelMerkleProof},
    types::{CompressedPublicKey, FixedHash, PrivateKey},
};
use tari_comms::types::CompressedSignature;
use tari_crypto::keys::SecretKey;
use tari_utilities::ByteArray;

use crate::error::WalletStorageError;

pub fn bincode_encode<T: serde::Serialize + ?Sized>(obj: &T) -> Result<Vec<u8>, WalletStorageError> {
    bincode::serialize(obj).map_err(|e| {
        WalletStorageError::ConversionError(format!("Failed to serialize type {}: {}", type_name::<T>(), e))
    })
}

pub fn bincode_decode<T: serde::de::DeserializeOwned>(data: &[u8]) -> Result<T, WalletStorageError> {
    bincode::deserialize(data).map_err(|e| {
        WalletStorageError::ConversionError(format!("Failed to deserialize type {}: {}", type_name::<T>(), e))
    })
}

/// Decodes the bincode-encoded `tari_mmr::MerkleProof` returned by the base node into a [`KernelMerkleProof`].
pub fn decode_kernel_merkle_proof(proof: &EncodedMerkleProof) -> Result<KernelMerkleProof, WalletStorageError> {
    let merkle_proof = bincode_decode::<tari_mmr::MerkleProof>(&proof.encoded_merkle_proof)?;
    let to_fixed_hashes = |hashes: Vec<tari_mmr::Hash>| {
        hashes
            .into_iter()
            .map(|hash| {
                FixedHash::try_from(hash).map_err(|_| {
                    WalletStorageError::ConversionError("Invalid hash length in kernel merkle proof".to_string())
                })
            })
            .collect::<Result<Vec<_>, _>>()
    };
    Ok(KernelMerkleProof {
        block_hash: proof.block_hash,
        leaf_index: proof.leaf_index,
        mmr_size: merkle_proof.mmr_size as u64,
        path: to_fixed_hashes(merkle_proof.path)?,
        peaks: to_fixed_hashes(merkle_proof.peaks)?,
    })
}

pub fn encode_signature(sig: &CompressedSignature) -> Result<Vec<u8>, WalletStorageError> {
    let mut bytes = Vec::with_capacity(CompressedPublicKey::key_length().saturating_add(PrivateKey::key_length()));
    bytes.extend_from_slice(sig.get_compressed_public_nonce().as_bytes());
    bytes.extend_from_slice(sig.get_signature().as_bytes());
    Ok(bytes)
}

pub fn decode_signature(data: &[u8]) -> Result<CompressedSignature, WalletStorageError> {
    let expected_len = CompressedPublicKey::key_length().saturating_add(PrivateKey::key_length());
    if data.len() != expected_len {
        return Err(WalletStorageError::ConversionError(format!(
            "Invalid signature length: expected {}, got {}",
            expected_len,
            data.len()
        )));
    }

    let nonce_bytes = data
        .get(..CompressedPublicKey::key_length())
        .expect("public nonce length checked above");
    let pub_nonce = CompressedPublicKey::from_canonical_bytes(nonce_bytes)
        .map_err(|e| WalletStorageError::ConversionError(format!("Failed to decode public nonce: {}", e)))?;
    let signature_bytes = data
        .get(CompressedPublicKey::key_length()..)
        .expect("signature length checked above");
    let signature = PrivateKey::from_canonical_bytes(signature_bytes)
        .map_err(|e| WalletStorageError::ConversionError(format!("Failed to decode signature: {}", e)))?;
    Ok(CompressedSignature::new(pub_nonce, signature))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_signature() {
        let secret_key = PrivateKey::random(&mut rand::rng());
        let public_key = CompressedPublicKey::from_secret_key(&secret_key);
        let signature = CompressedSignature::new(public_key, secret_key);

        let encoded = encode_signature(&signature).expect("Encoding failed");
        let decoded = decode_signature(&encoded).expect("Decoding failed");

        assert_eq!(signature, decoded);
    }

    #[test]
    fn it_decodes_a_kernel_merkle_proof() {
        let merkle_proof = tari_mmr::MerkleProof {
            mmr_size: 11,
            path: vec![vec![2u8; 32], vec![3u8; 32]],
            peaks: vec![vec![4u8; 32]],
        };
        let encoded = EncodedMerkleProof {
            block_hash: FixedHash::from([1u8; 32]),
            encoded_merkle_proof: bincode_encode(&merkle_proof).unwrap(),
            leaf_index: 7,
        };

        let proof = decode_kernel_merkle_proof(&encoded).unwrap();
        assert_eq!(proof, KernelMerkleProof {
            block_hash: FixedHash::from([1u8; 32]),
            leaf_index: 7,
            mmr_size: 11,
            path: vec![FixedHash::from([2u8; 32]), FixedHash::from([3u8; 32])],
            peaks: vec![FixedHash::from([4u8; 32])],
        });
    }

    #[test]
    fn it_rejects_a_kernel_merkle_proof_with_a_short_hash() {
        let merkle_proof = tari_mmr::MerkleProof {
            mmr_size: 1,
            path: vec![vec![2u8; 31]],
            peaks: vec![],
        };
        let encoded = EncodedMerkleProof {
            block_hash: FixedHash::default(),
            encoded_merkle_proof: bincode_encode(&merkle_proof).unwrap(),
            leaf_index: 0,
        };
        decode_kernel_merkle_proof(&encoded).unwrap_err();
    }
}
