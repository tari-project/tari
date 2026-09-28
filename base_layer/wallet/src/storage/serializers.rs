// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::any::type_name;

use tari_common_types::types::{CompressedPublicKey, PrivateKey};
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
    fn burn_output_proof_bincode_round_trips() {
        use tari_common_types::{
            burn_proof::{BurnOutputProof, MmrInclusionProof, OutputHashPreimage},
            types::FixedHash,
        };

        let mmr_proof = MmrInclusionProof {
            leaf_index: 1,
            mmr_size: 3,
            path: vec![FixedHash::from([2u8; 32])],
            peaks: vec![],
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
        let decoded: BurnOutputProof = bincode_decode(&bincode_encode(&proof).unwrap()).unwrap();
        assert_eq!(decoded, proof);
    }
}
