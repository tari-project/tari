// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Stealth address functionality for lightweight wallets
//! 
//! This module provides stealth address generation and key recovery capabilities
//! for private transactions in the Tari network.

use curve25519_dalek::constants::RISTRETTO_BASEPOINT_POINT;
use curve25519_dalek::ristretto::RistrettoPoint;
use curve25519_dalek::scalar::Scalar;
use blake2::Blake2b512;
use blake2::digest::Digest;
use crate::data_structures::types::{PrivateKey, CompressedPublicKey};
use crate::errors::KeyManagementError;
use crate::key_management::KeyDerivationPath;

/// Domain separator for stealth address operations
const STEALTH_ADDRESS_DOMAIN: &[u8] = b"TARI_STEALTH_ADDRESS";

/// Stealth address structure
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StealthAddress {
    /// The stealth address public key
    pub public_key: CompressedPublicKey,
    /// The ephemeral public key used to create this stealth address
    pub ephemeral_public_key: CompressedPublicKey,
    /// The derivation path used (if applicable)
    pub derivation_path: Option<KeyDerivationPath>,
}

impl StealthAddress {
    /// Create a new stealth address
    pub fn new(
        public_key: CompressedPublicKey,
        ephemeral_public_key: CompressedPublicKey,
        derivation_path: Option<KeyDerivationPath>,
    ) -> Self {
        Self {
            public_key,
            ephemeral_public_key,
            derivation_path,
        }
    }

    /// Get the stealth address as a hex string
    pub fn to_hex(&self) -> String {
        format!(
            "{}:{}",
            self.public_key.to_hex(),
            self.ephemeral_public_key.to_hex()
        )
    }

    /// Create a stealth address from hex string
    pub fn from_hex(hex: &str) -> Result<Self, KeyManagementError> {
        let parts: Vec<&str> = hex.split(':').collect();
        if parts.len() != 2 {
            return Err(KeyManagementError::InvalidPublicKey(
                "Invalid stealth address format".to_string()
            ));
        }

        let public_key = CompressedPublicKey::from_hex(parts[0])
            .map_err(|e| KeyManagementError::InvalidPublicKey(e.to_string()))?;
        let ephemeral_public_key = CompressedPublicKey::from_hex(parts[1])
            .map_err(|e| KeyManagementError::InvalidPublicKey(e.to_string()))?;

        Ok(Self::new(public_key, ephemeral_public_key, None))
    }
}

/// Stealth address generator and key recovery
pub struct StealthAddressManager;

impl StealthAddressManager {
    /// Generate a stealth address for a recipient
    /// 
    /// This function creates a stealth address that only the recipient can spend from.
    /// The sender uses the recipient's public key and an ephemeral private key to generate
    /// a one-time address.
    pub fn generate_stealth_address(
        _sender_private_key: &PrivateKey, // Not used in this implementation
        recipient_public_key: &CompressedPublicKey,
    ) -> Result<StealthAddress, KeyManagementError> {
        // 1. Generate ephemeral private key r
        let r = PrivateKey::random().0;
        // 2. Compute ephemeral public key R = r*G
        let R = (r * RISTRETTO_BASEPOINT_POINT).compress();
        // 3. Compute shared secret S = r*P
        let P = recipient_public_key.decompress().ok_or_else(|| KeyManagementError::InvalidPublicKey("Could not decompress recipient public key".to_string()))?;
        let S = r * P;
        // 4. Hash the shared secret to a scalar
        let h = hash_to_scalar(&S);
        // 5. Compute stealth public key: P_stealth = P + h*G
        let P_stealth = P + h * RISTRETTO_BASEPOINT_POINT;
        let stealth_public_key = CompressedPublicKey::from_point(&P_stealth);
        let ephemeral_public_key = CompressedPublicKey(R);
        Ok(StealthAddress::new(stealth_public_key, ephemeral_public_key, None))
    }

    /// Recover the private key for a stealth address
    /// 
    /// This function allows the recipient to recover the private key needed to spend
    /// from a stealth address using their private key and the ephemeral public key.
    pub fn recover_stealth_private_key(
        recipient_private_key: &PrivateKey,
        ephemeral_public_key: &CompressedPublicKey,
    ) -> Result<PrivateKey, KeyManagementError> {
        // 1. Compute shared secret S = a*R
        let a = recipient_private_key.0;
        let R = ephemeral_public_key.decompress().ok_or_else(|| KeyManagementError::InvalidPublicKey("Could not decompress ephemeral public key".to_string()))?;
        let S = a * R;
        // 2. Hash the shared secret to a scalar
        let h = hash_to_scalar(&S);
        // 3. Compute stealth private key: k_stealth = a + h
        let k_stealth = a + h;
        Ok(PrivateKey(k_stealth))
    }

    /// Validate a stealth address
    pub fn validate_stealth_address(
        stealth_address: &StealthAddress,
        recipient_private_key: &PrivateKey,
    ) -> Result<bool, KeyManagementError> {
        let recovered_private_key = Self::recover_stealth_private_key(
            recipient_private_key,
            &stealth_address.ephemeral_public_key,
        )?;
        let recovered_public_key = CompressedPublicKey::from_point(&(recovered_private_key.0 * RISTRETTO_BASEPOINT_POINT));
        Ok(recovered_public_key == stealth_address.public_key)
    }
}

fn hash_to_scalar(point: &RistrettoPoint) -> Scalar {
    let mut hasher = Blake2b512::new();
    Digest::update(&mut hasher, STEALTH_ADDRESS_DOMAIN);
    Digest::update(&mut hasher, point.compress().as_bytes());
    let result = hasher.finalize();
    let mut out = [0u8; 64];
    out.copy_from_slice(&result);
    Scalar::from_bytes_mod_order_wide(&out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_structures::types::{PrivateKey, CompressedPublicKey};
    use curve25519_dalek::constants::RISTRETTO_BASEPOINT_POINT;
    use curve25519_dalek::scalar::Scalar;

    #[test]
    fn test_stealth_address_generation_and_recovery() {
        // Generate recipient keypair
        let recipient_private = PrivateKey::random();
        let recipient_public = CompressedPublicKey::from_point(&(recipient_private.0 * RISTRETTO_BASEPOINT_POINT));
        // Generate stealth address
        let stealth_address = StealthAddressManager::generate_stealth_address(&PrivateKey::random(), &recipient_public).unwrap();
        // Recipient recovers stealth private key
        let recovered_private = StealthAddressManager::recover_stealth_private_key(&recipient_private, &stealth_address.ephemeral_public_key).unwrap();
        // Check that the public key matches the stealth address
        let recovered_public = CompressedPublicKey::from_point(&(recovered_private.0 * RISTRETTO_BASEPOINT_POINT));
        assert_eq!(recovered_public, stealth_address.public_key);
    }

    #[test]
    fn test_stealth_address_validation() {
        let recipient_private = PrivateKey::random();
        let recipient_public = CompressedPublicKey::from_point(&(recipient_private.0 * RISTRETTO_BASEPOINT_POINT));
        let stealth_address = StealthAddressManager::generate_stealth_address(&PrivateKey::random(), &recipient_public).unwrap();
        let is_valid = StealthAddressManager::validate_stealth_address(&stealth_address, &recipient_private).unwrap();
        assert!(is_valid);
    }

    #[test]
    fn test_stealth_address_hex_conversion() {
        let recipient_private = PrivateKey::random();
        let recipient_public = CompressedPublicKey::from_point(&(recipient_private.0 * RISTRETTO_BASEPOINT_POINT));
        let stealth_address = StealthAddressManager::generate_stealth_address(&PrivateKey::random(), &recipient_public).unwrap();
        let hex = stealth_address.to_hex();
        let recovered = StealthAddress::from_hex(&hex).unwrap();
        assert_eq!(stealth_address, recovered);
    }
} 