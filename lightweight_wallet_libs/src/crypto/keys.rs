// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Ristretto255 key implementations
//! 
//! This provides our own implementations of Ristretto255 secret and public keys
//! to avoid dependencies on tari-crypto.

extern crate alloc;

use alloc::vec::Vec;
use core::fmt;
use core::hash::{Hash, Hasher};
use core::ops::{Add, Mul, Neg, Sub};
use curve25519_dalek::{
    constants::RISTRETTO_BASEPOINT_TABLE,
    ristretto::{CompressedRistretto, RistrettoPoint},
    scalar::Scalar,
};
use digest::{Digest, OutputSizeUser};
use rand_core::{CryptoRng, RngCore};
use subtle::{ConstantTimeEq, Choice};
use zeroize::{Zeroize, ZeroizeOnDrop};
use blake2::Blake2b;
use digest::consts::U64;

use crate::errors::KeyManagementError;

/// Error type for byte array operations
#[derive(Debug, thiserror::Error)]
pub enum ByteArrayError {
    #[error("Incorrect length")]
    IncorrectLength,
    #[error("Invalid bytes")]
    InvalidBytes,
}

/// Trait for byte array operations
pub trait ByteArray {
    fn as_bytes(&self) -> &[u8];
    fn from_bytes(bytes: &[u8]) -> Result<Self, KeyManagementError> where Self: Sized;
}

/// Ristretto255 secret key
#[derive(Debug, Clone, PartialEq, Eq, Zeroize, ZeroizeOnDrop)]
pub struct RistrettoSecretKey(pub(crate) Scalar);

impl RistrettoSecretKey {
    /// Create a random secret key
    pub fn random<R: RngCore + CryptoRng>(rng: &mut R) -> Self {
        let mut bytes = [0u8; 32];
        rng.fill_bytes(&mut bytes);
        RistrettoSecretKey(Scalar::from_bytes_mod_order(bytes))
    }

    /// Create a secret key from uniform bytes using wide reduction (64 bytes -> 32 bytes)
    /// This matches the main Tari implementation exactly
    pub fn from_uniform_bytes(bytes: &[u8]) -> Result<Self, KeyManagementError> {
        if bytes.len() != 64 {
            return Err(KeyManagementError::key_derivation_failed("Invalid byte length for secret key"));
        }
        
        let mut bytes_array = [0u8; 64];
        bytes_array.copy_from_slice(bytes);
        
        let scalar = Scalar::from_bytes_mod_order_wide(&bytes_array);
        Ok(RistrettoSecretKey(scalar))
    }

    /// Get the public key corresponding to this secret key
    pub fn public_key(&self) -> RistrettoPublicKey {
        RistrettoPublicKey {
            point: &self.0 * RISTRETTO_BASEPOINT_TABLE,
            compressed_bytes: [0u8; 32],
        }
    }
}

impl ByteArray for RistrettoSecretKey {
    fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

    fn from_bytes(bytes: &[u8]) -> Result<Self, KeyManagementError> {
        if bytes.len() != 32 {
            return Err(KeyManagementError::key_derivation_failed("Invalid byte length for secret key"));
        }
        
        let mut bytes_array = [0u8; 32];
        bytes_array.copy_from_slice(bytes);
        
        let scalar = Scalar::from_canonical_bytes(bytes_array)
            .unwrap_or_else(|| Scalar::from_bytes_mod_order(bytes_array));
        Ok(RistrettoSecretKey(scalar))
    }
}

/// Ristretto255 public key
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RistrettoPublicKey {
    pub(crate) point: RistrettoPoint,
    compressed_bytes: [u8; 32],
}

impl RistrettoPublicKey {
    /// Create a public key from a secret key
    pub fn from_secret_key(secret_key: &RistrettoSecretKey) -> Self {
        let point = &secret_key.0 * RISTRETTO_BASEPOINT_TABLE;
        let compressed = point.compress();
        let mut compressed_bytes = [0u8; 32];
        compressed_bytes.copy_from_slice(compressed.as_bytes());
        Self {
            point,
            compressed_bytes,
        }
    }

    /// Create a public key from compressed bytes
    pub fn from_compressed(bytes: &[u8; 32]) -> Result<Self, KeyManagementError> {
        let compressed = CompressedRistretto::from_slice(bytes)
            .map_err(|_| KeyManagementError::key_derivation_failed("Invalid compressed point"))?;
        
        let point = compressed
            .decompress()
            .ok_or_else(|| KeyManagementError::key_derivation_failed("Failed to decompress point"))?;
        
        let mut compressed_bytes = [0u8; 32];
        compressed_bytes.copy_from_slice(bytes);
        
        Ok(RistrettoPublicKey { 
            point,
            compressed_bytes,
        })
    }

    /// Get the compressed representation
    pub fn compress(&self) -> CompressedRistretto {
        self.point.compress()
    }
}

impl ByteArray for RistrettoPublicKey {
    fn as_bytes(&self) -> &[u8] {
        &self.compressed_bytes
    }

    fn from_bytes(bytes: &[u8]) -> Result<Self, KeyManagementError> {
        if bytes.len() != 32 {
            return Err(KeyManagementError::key_derivation_failed("Invalid byte length for public key"));
        }
        
        let mut bytes_array = [0u8; 32];
        bytes_array.copy_from_slice(bytes);
        
        Self::from_compressed(&bytes_array)
    }
}

impl Add for RistrettoPublicKey {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        let point = self.point + other.point;
        let compressed = point.compress();
        let mut compressed_bytes = [0u8; 32];
        compressed_bytes.copy_from_slice(compressed.as_bytes());
        RistrettoPublicKey {
            point,
            compressed_bytes,
        }
    }
}

impl Sub for RistrettoPublicKey {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        let point = self.point - other.point;
        let compressed = point.compress();
        let mut compressed_bytes = [0u8; 32];
        compressed_bytes.copy_from_slice(compressed.as_bytes());
        RistrettoPublicKey {
            point,
            compressed_bytes,
        }
    }
}

impl Mul<Scalar> for RistrettoPublicKey {
    type Output = Self;

    fn mul(self, scalar: Scalar) -> Self {
        let point = self.point * scalar;
        let compressed = point.compress();
        let mut compressed_bytes = [0u8; 32];
        compressed_bytes.copy_from_slice(compressed.as_bytes());
        RistrettoPublicKey {
            point,
            compressed_bytes,
        }
    }
}

impl Neg for RistrettoPublicKey {
    type Output = Self;

    fn neg(self) -> Self {
        let point = -self.point;
        let compressed = point.compress();
        let mut compressed_bytes = [0u8; 32];
        compressed_bytes.copy_from_slice(compressed.as_bytes());
        RistrettoPublicKey {
            point,
            compressed_bytes,
        }
    }
}

impl fmt::Display for RistrettoPublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", hex::encode(self.as_bytes()))
    }
}

impl fmt::Display for RistrettoSecretKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", hex::encode(self.as_bytes()))
    }
}

impl Hash for RistrettoPublicKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_bytes().hash(state);
    }
}

impl Hash for RistrettoSecretKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_bytes().hash(state);
    }
}

impl ConstantTimeEq for RistrettoPublicKey {
    fn ct_eq(&self, other: &Self) -> Choice {
        self.point.ct_eq(&other.point)
    }
}

impl ConstantTimeEq for RistrettoSecretKey {
    fn ct_eq(&self, other: &Self) -> Choice {
        self.0.ct_eq(&other.0)
    }
}

impl Zeroize for RistrettoPublicKey {
    fn zeroize(&mut self) {
        self.point.zeroize();
    }
}

// Arithmetic operations for RistrettoSecretKey
impl<'b> Mul<&'b RistrettoPublicKey> for &RistrettoSecretKey {
    type Output = RistrettoPublicKey;

    fn mul(self, rhs: &'b RistrettoPublicKey) -> RistrettoPublicKey {
        let p = self.0 * rhs.point;
        let compressed = p.compress();
        let mut compressed_bytes = [0u8; 32];
        compressed_bytes.copy_from_slice(compressed.as_bytes());
        RistrettoPublicKey { point: p, compressed_bytes }
    }
}

impl<'b> Add<&'b RistrettoSecretKey> for &RistrettoSecretKey {
    type Output = RistrettoSecretKey;

    fn add(self, rhs: &'b RistrettoSecretKey) -> RistrettoSecretKey {
        let k = self.0 + rhs.0;
        RistrettoSecretKey(k)
    }
}

impl<'b> Sub<&'b RistrettoSecretKey> for &RistrettoSecretKey {
    type Output = RistrettoSecretKey;

    fn sub(self, rhs: &'b RistrettoSecretKey) -> RistrettoSecretKey {
        RistrettoSecretKey(self.0 - rhs.0)
    }
}

// Arithmetic operations for RistrettoPublicKey
impl<'a> Mul<&'a RistrettoSecretKey> for &RistrettoPublicKey {
    type Output = RistrettoPublicKey;

    fn mul(self, rhs: &'a RistrettoSecretKey) -> RistrettoPublicKey {
        let p = rhs.0 * self.point;
        let compressed = p.compress();
        let mut compressed_bytes = [0u8; 32];
        compressed_bytes.copy_from_slice(compressed.as_bytes());
        RistrettoPublicKey { point: p, compressed_bytes }
    }
}

// Conversions
impl From<u64> for RistrettoSecretKey {
    fn from(v: u64) -> Self {
        let s = Scalar::from(v);
        RistrettoSecretKey(s)
    }
}

impl From<RistrettoSecretKey> for Scalar {
    fn from(k: RistrettoSecretKey) -> Self {
        k.0
    }
}

impl From<RistrettoPublicKey> for RistrettoPoint {
    fn from(pk: RistrettoPublicKey) -> Self {
        pk.point
    }
}

impl From<&RistrettoPublicKey> for RistrettoPoint {
    fn from(pk: &RistrettoPublicKey) -> Self {
        pk.point
    }
}

// Hashable trait implementation
pub trait Hashable {
    fn hash(&self) -> Vec<u8>;
}

impl Hashable for RistrettoPublicKey {
    fn hash(&self) -> Vec<u8> {
        Blake2b::<U64>::digest(self.as_bytes()).to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_core::OsRng;

    #[test]
    fn test_secret_key_creation() {
        let key = RistrettoSecretKey::random(&mut OsRng);
        assert_eq!(key.as_bytes().len(), 32);
    }

    #[test]
    fn test_public_key_from_secret() {
        let secret = RistrettoSecretKey::random(&mut OsRng);
        let public = RistrettoPublicKey::from_secret_key(&secret);
        assert_eq!(public.as_bytes().len(), 32);
    }

    #[test]
    fn test_key_arithmetic() {
        let key1 = RistrettoSecretKey::random(&mut OsRng);
        let key2 = RistrettoSecretKey::random(&mut OsRng);
        
        let sum = &key1 + &key2;
        let diff = &key1 - &key2;
        
        assert_ne!(sum, key1);
        assert_ne!(sum, key2);
        assert_ne!(diff, key1);
        assert_ne!(diff, key2);
    }

    #[test]
    fn test_public_key_arithmetic() {
        let secret1 = RistrettoSecretKey::random(&mut OsRng);
        let secret2 = RistrettoSecretKey::random(&mut OsRng);
        let public1 = RistrettoPublicKey::from_secret_key(&secret1);
        let public2 = RistrettoPublicKey::from_secret_key(&secret2);
        
        let sum = public1.clone() + public2.clone();
        let diff = public1.clone() - public2.clone();
        
        assert_ne!(sum, public1);
        assert_ne!(sum, public2);
    }
} 