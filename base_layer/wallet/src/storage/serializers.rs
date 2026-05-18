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
    let mut bytes = Vec::with_capacity(CompressedPublicKey::key_length() + PrivateKey::key_length());
    bytes.extend_from_slice(sig.get_compressed_public_nonce().as_bytes());
    bytes.extend_from_slice(sig.get_signature().as_bytes());
    Ok(bytes)
}

pub fn decode_signature(data: &[u8]) -> Result<CompressedSignature, WalletStorageError> {
    let expected_len = CompressedPublicKey::key_length() + PrivateKey::key_length();
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
}
