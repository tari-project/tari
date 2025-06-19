// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Stealth address functionality for lightweight wallets
//! 
//! This module provides stealth address generation and key recovery capabilities
//! for private transactions in the Tari network.

use blake2::{Blake2b, Blake2b512, Digest};
use curve25519_dalek::{
    constants::RISTRETTO_BASEPOINT_POINT,
    ristretto::{CompressedRistretto, RistrettoPoint},
    scalar::Scalar,
    traits::Identity,
};
use digest::consts::U32;
use crate::{
    data_structures::types::{CompressedPublicKey, PrivateKey},
    errors::KeyManagementError,
    key_management::KeyDerivationPath,
};

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

    /// Generate a stealth address from recipient public key and sender's ephemeral private key
    pub fn generate(
        recipient_public_key: &CompressedPublicKey,
        sender_ephemeral_private_key: &PrivateKey,
    ) -> Self {
        // Compute ephemeral public key
        let ephemeral_public_key = CompressedPublicKey::from_private_key(sender_ephemeral_private_key);
        // Perform ECDH to get shared secret: r * P
        let recipient_point = recipient_public_key.decompress().unwrap_or_else(|| RistrettoPoint::identity());
        let shared_point = sender_ephemeral_private_key.0 * recipient_point;
        let shared_secret = shared_point.compress().to_bytes();
        // Derive the stealth public key: P_stealth = P + H(shared_secret) * G
        let mut hasher = Blake2b::<U32>::new();
        hasher.update(b"TARI_STEALTH_PUBKEY");
        hasher.update(recipient_public_key.as_bytes());
        hasher.update(&shared_secret);
        let result = hasher.finalize();
        let mut tweak_bytes = [0u8; 32];
        tweak_bytes.copy_from_slice(result.as_slice());
        let tweak = Scalar::from_bytes_mod_order(tweak_bytes);
        let p_stealth = recipient_point + tweak * RISTRETTO_BASEPOINT_POINT;
        let stealth_public_key = CompressedPublicKey::from_point(&p_stealth);
        Self::new(stealth_public_key, ephemeral_public_key, None)
    }

    /// Recover private key from recipient private key, recipient public key, and sender's ephemeral public key
    pub fn recover_private_key(
        recipient_private_key: &PrivateKey,
        recipient_public_key: &CompressedPublicKey,
        sender_ephemeral_public_key: &CompressedPublicKey,
    ) -> PrivateKey {
        // Compute shared secret: S = a*R
        let ephemeral_point = sender_ephemeral_public_key.decompress().unwrap_or_else(|| RistrettoPoint::identity());
        let shared_point = recipient_private_key.0 * ephemeral_point;
        let shared_secret = shared_point.compress().to_bytes();
        // Derive tweak: tweak = H(P || S)
        let mut hasher = Blake2b::<U32>::new();
        hasher.update(b"TARI_STEALTH_PUBKEY");
        hasher.update(recipient_public_key.as_bytes());
        hasher.update(&shared_secret);
        let result = hasher.finalize();
        let mut tweak_bytes = [0u8; 32];
        tweak_bytes.copy_from_slice(result.as_slice());
        let tweak = Scalar::from_bytes_mod_order(tweak_bytes);
        // Compute stealth private key: a_stealth = a + tweak
        let a_stealth = recipient_private_key.0 + tweak;
        PrivateKey(a_stealth)
    }

    /// Convert to hex string
    pub fn to_hex(&self) -> String {
        let mut hex = String::new();
        hex.push_str(&self.public_key.to_hex());
        hex.push_str(&self.ephemeral_public_key.to_hex());
        if let Some(path) = &self.derivation_path {
            hex.push_str(&path.to_string());
        }
        hex
    }

    /// Create from hex string
    pub fn from_hex(hex: &str) -> Result<Self, KeyManagementError> {
        if hex.len() < 128 { // 32 bytes each for public key and ephemeral public key
            return Err(KeyManagementError::stealth_address_recovery_failed(&format!("Invalid hex length: {}", hex.len())));
        }
        
        let public_key_hex = &hex[..64];
        let ephemeral_public_key_hex = &hex[64..128];
        
        let public_key = CompressedPublicKey::from_hex(public_key_hex)
            .map_err(|e| KeyManagementError::stealth_address_recovery_failed(&e.to_string()))?;
        let ephemeral_public_key = CompressedPublicKey::from_hex(ephemeral_public_key_hex)
            .map_err(|e| KeyManagementError::stealth_address_recovery_failed(&e.to_string()))?;
        
        Ok(Self::new(public_key, ephemeral_public_key, None))
    }

    /// Derive shared secret from public keys (for sender)
    fn derive_shared_secret(
        recipient_public_key: &CompressedPublicKey,
        ephemeral_public_key: &CompressedPublicKey,
    ) -> [u8; 32] {
        // This is a simplified version - in practice, you'd need the ephemeral private key
        // For now, we'll use a hash of both public keys
        let mut hasher = Blake2b::<U32>::new();
        hasher.update(b"TARI_STEALTH_ECDH");
        hasher.update(recipient_public_key.as_bytes());
        hasher.update(ephemeral_public_key.as_bytes());
        let result = hasher.finalize();
        let mut shared_secret = [0u8; 32];
        shared_secret.copy_from_slice(result.as_slice());
        shared_secret
    }

    /// Derive shared secret from private key and public key (for recipient)
    fn derive_shared_secret_from_private_key(
        recipient_private_key: &PrivateKey,
        ephemeral_public_key: &CompressedPublicKey,
    ) -> [u8; 32] {
        // Perform ECDH: recipient_private_key * ephemeral_public_key
        let ephemeral_point = ephemeral_public_key.decompress().unwrap_or_else(|| {
            // Fallback to identity point if decompression fails
            RistrettoPoint::identity()
        });
        let shared_point = recipient_private_key.0 * ephemeral_point;
        let mut shared_secret = [0u8; 32];
        shared_secret.copy_from_slice(shared_point.compress().to_bytes().as_slice());
        shared_secret
    }

    /// Derive stealth private key
    fn derive_stealth_private_key(
        recipient_private_key: &PrivateKey,
        shared_secret: &[u8; 32],
    ) -> PrivateKey {
        // Use Blake2b to derive the stealth private key
        let mut hasher = Blake2b::<U32>::new();
        hasher.update(b"TARI_STEALTH_PRIVKEY");
        hasher.update(recipient_private_key.as_bytes());
        hasher.update(shared_secret);
        let result = hasher.finalize();
        let mut stealth_key_bytes = [0u8; 32];
        stealth_key_bytes.copy_from_slice(result.as_slice());
        PrivateKey::new(stealth_key_bytes)
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
        Ok(StealthAddress::new(stealth_public_key, ephemeral_public_key.clone(), None))
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
        // Generate a random private key for the recipient
        let recipient_private_key = PrivateKey::random();
        let recipient_public_key = CompressedPublicKey::from_private_key(&recipient_private_key);
        // Generate a random ephemeral private key for the sender
        let sender_ephemeral_private_key = PrivateKey::random();
        let sender_ephemeral_public_key = CompressedPublicKey::from_private_key(&sender_ephemeral_private_key);
        // Sender generates the stealth address using their ephemeral private key and the recipient's public key
        let stealth_address = StealthAddress::generate(
            &recipient_public_key,
            &sender_ephemeral_private_key,
        );
        // Receiver recovers the private key using their private key, their public key, and the sender's ephemeral public key
        let recovered_private_key = StealthAddress::recover_private_key(
            &recipient_private_key,
            &recipient_public_key,
            &sender_ephemeral_public_key,
        );
        // The recovered private key should be different from the original recipient private key
        assert_ne!(recovered_private_key.as_bytes(), recipient_private_key.as_bytes());
        // But the public key derived from the recovered private key should match the stealth address
        let recovered_public_key = CompressedPublicKey::from_private_key(&recovered_private_key);
        assert_eq!(recovered_public_key.as_bytes(), stealth_address.public_key.as_bytes());
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