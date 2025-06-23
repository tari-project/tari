// Copyright 2022 The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE

use std::fmt;
use std::ops::{Add, Sub, Mul};

use borsh::{BorshDeserialize, BorshSerialize};
use curve25519_dalek::{
    constants::RISTRETTO_BASEPOINT_POINT,
    ristretto::{CompressedRistretto, RistrettoPoint},
    scalar::Scalar,
};
use hex::{FromHex, ToHex};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use zeroize::Zeroize;

use crate::hex_utils::{HexEncodable, HexError, HexValidatable};

/// Custom serde module for Scalar
mod scalar_serde {
    use super::*;

    pub fn serialize<S>(scalar: &Scalar, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let bytes = scalar.to_bytes();
        let hex_string = hex::encode(bytes);
        serializer.serialize_str(&hex_string)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Scalar, D::Error>
    where
        D: Deserializer<'de>,
    {
        let hex_string = <String as serde::Deserialize>::deserialize(deserializer)?;
        let bytes = hex::decode(&hex_string).map_err(serde::de::Error::custom)?;
        if bytes.len() != 32 {
            return Err(serde::de::Error::custom("Expected 32 bytes"));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(Scalar::from_bytes_mod_order(arr))
    }
}

/// Custom serde module for CompressedRistretto
mod compressed_ristretto_serde {
    use super::*;

    pub fn serialize<S>(compressed: &CompressedRistretto, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let bytes = compressed.to_bytes();
        let hex_string = hex::encode(bytes);
        serializer.serialize_str(&hex_string)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<CompressedRistretto, D::Error>
    where
        D: Deserializer<'de>,
    {
        let hex_string = <String as serde::Deserialize>::deserialize(deserializer)?;
        let bytes = hex::decode(&hex_string).map_err(serde::de::Error::custom)?;
        if bytes.len() != 32 {
            return Err(serde::de::Error::custom("Expected 32 bytes"));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(CompressedRistretto(arr))
    }
}

/// Custom borsh module for Scalar
mod scalar_borsh {
    use super::*;

    pub fn serialize<W: std::io::Write>(scalar: &Scalar, writer: &mut W) -> std::io::Result<()> {
        BorshSerialize::serialize(&scalar.to_bytes(), writer)
    }

    pub fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Scalar> {
        let bytes = <[u8; 32]>::deserialize_reader(reader)?;
        Ok(Scalar::from_canonical_bytes(bytes).unwrap_or_else(|| {
            // Fallback to zero scalar if bytes are not canonical
            Scalar::from_bytes_mod_order([0u8; 32])
        }))
    }
}

/// Custom borsh module for CompressedRistretto
mod compressed_ristretto_borsh {
    use super::*;

    pub fn serialize<W: std::io::Write>(compressed: &CompressedRistretto, writer: &mut W) -> std::io::Result<()> {
        BorshSerialize::serialize(&compressed.to_bytes(), writer)
    }

    pub fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<CompressedRistretto> {
        let bytes = <[u8; 32]>::deserialize_reader(reader)?;
        Ok(CompressedRistretto(bytes))
    }
}

/// A wrapper around a private key that provides zeroization on drop
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PrivateKey(#[serde(with = "scalar_serde")] pub Scalar);

impl BorshSerialize for PrivateKey {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        BorshSerialize::serialize(&self.0.to_bytes(), writer)
    }
}

impl BorshDeserialize for PrivateKey {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let bytes = <[u8; 32]>::deserialize_reader(reader)?;
        Ok(Self(Scalar::from_canonical_bytes(bytes).unwrap_or_else(|| {
            // Fallback to zero scalar if bytes are not canonical
            Scalar::from_bytes_mod_order([0u8; 32])
        })))
    }
}

impl PrivateKey {
    /// Create a new private key from bytes
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(Scalar::from_bytes_mod_order(bytes))
    }

    /// Generate a random private key
    pub fn random() -> Self {
        let mut bytes = [0u8; 64];
        OsRng.fill_bytes(&mut bytes);
        Self(Scalar::from_bytes_mod_order_wide(&bytes))
    }

    /// Get the private key bytes
    pub fn as_bytes(&self) -> [u8; 32] {
        self.0.to_bytes()
    }

    /// Convert to hex string
    pub fn to_hex(&self) -> String {
        hex::encode(self.as_bytes())
    }

    /// Create from hex string
    pub fn from_hex(hex: &str) -> Result<Self, HexError> {
        let bytes = hex::decode(hex).map_err(|e| HexError::InvalidHex(e.to_string()))?;
        if bytes.len() != 32 {
            return Err(HexError::InvalidLength {
                expected: 32,
                actual: bytes.len(),
            });
        }
        let mut key_bytes = [0u8; 32];
        key_bytes.copy_from_slice(&bytes);
        Ok(Self::new(key_bytes))
    }

    /// Create from canonical bytes (ensuring it's a valid scalar)
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() != 32 {
            return Err("Private key must be 32 bytes".to_string());
        }
        let mut key_bytes = [0u8; 32];
        key_bytes.copy_from_slice(bytes);
        Ok(Self::new(key_bytes))
    }

    /// Get the key length
    pub const KEY_LEN: usize = 32;
}

impl Zeroize for PrivateKey {
    fn zeroize(&mut self) {
        // Overwrite the scalar's memory directly
        use zeroize::Zeroize;
        let mut bytes = self.0.to_bytes();
        bytes.zeroize();
        // Overwrite the scalar with zero scalar
        self.0 = curve25519_dalek::scalar::Scalar::from_bytes_mod_order([0u8; 32]);
    }
}

impl Drop for PrivateKey {
    fn drop(&mut self) {
        self.zeroize();
    }
}

impl HexEncodable for PrivateKey {
    fn to_hex(&self) -> String {
        self.to_hex()
    }
    fn from_hex(hex: &str) -> Result<Self, HexError> {
        Self::from_hex(hex)
    }
}

impl HexValidatable for PrivateKey {}

impl Add for PrivateKey {
    type Output = PrivateKey;
    fn add(self, rhs: PrivateKey) -> PrivateKey {
        PrivateKey(self.0 + rhs.0)
    }
}

impl<'a> Add<&'a PrivateKey> for PrivateKey {
    type Output = PrivateKey;
    fn add(self, rhs: &'a PrivateKey) -> PrivateKey {
        PrivateKey(self.0 + rhs.0)
    }
}

impl<'a> Add<PrivateKey> for &'a PrivateKey {
    type Output = PrivateKey;
    fn add(self, rhs: PrivateKey) -> PrivateKey {
        PrivateKey(self.0 + rhs.0)
    }
}

impl<'a, 'b> Add<&'a PrivateKey> for &'b PrivateKey {
    type Output = PrivateKey;
    fn add(self, rhs: &'a PrivateKey) -> PrivateKey {
        PrivateKey(self.0 + rhs.0)
    }
}

impl Sub for PrivateKey {
    type Output = PrivateKey;
    fn sub(self, rhs: PrivateKey) -> PrivateKey {
        PrivateKey(self.0 - rhs.0)
    }
}

impl<'a> Sub<&'a PrivateKey> for PrivateKey {
    type Output = PrivateKey;
    fn sub(self, rhs: &'a PrivateKey) -> PrivateKey {
        PrivateKey(self.0 - rhs.0)
    }
}

impl<'a> Sub<PrivateKey> for &'a PrivateKey {
    type Output = PrivateKey;
    fn sub(self, rhs: PrivateKey) -> PrivateKey {
        PrivateKey(self.0 - rhs.0)
    }
}

impl<'a, 'b> Sub<&'a PrivateKey> for &'b PrivateKey {
    type Output = PrivateKey;
    fn sub(self, rhs: &'a PrivateKey) -> PrivateKey {
        PrivateKey(self.0 - rhs.0)
    }
}

impl Mul for PrivateKey {
    type Output = PrivateKey;
    fn mul(self, rhs: PrivateKey) -> PrivateKey {
        PrivateKey(self.0 * rhs.0)
    }
}

impl<'a> Mul<&'a PrivateKey> for PrivateKey {
    type Output = PrivateKey;
    fn mul(self, rhs: &'a PrivateKey) -> PrivateKey {
        PrivateKey(self.0 * rhs.0)
    }
}

impl<'a> Mul<PrivateKey> for &'a PrivateKey {
    type Output = PrivateKey;
    fn mul(self, rhs: PrivateKey) -> PrivateKey {
        PrivateKey(self.0 * rhs.0)
    }
}

impl<'a, 'b> Mul<&'a PrivateKey> for &'b PrivateKey {
    type Output = PrivateKey;
    fn mul(self, rhs: &'a PrivateKey) -> PrivateKey {
        PrivateKey(self.0 * rhs.0)
    }
}

/// Micro Minotari amount (smallest unit)
#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize,
)]
pub struct MicroMinotari(u64);

impl MicroMinotari {
    /// Create a new Micro Minotari amount
    pub fn new(amount: u64) -> Self {
        Self(amount)
    }

    /// Get the amount as u64
    pub fn as_u64(&self) -> u64 {
        self.0
    }

    /// Convert to Tari (1 Tari = 1,000,000 Micro Minotari)
    pub fn as_tari(&self) -> f64 {
        self.0 as f64 / 1_000_000.0
    }

    /// Create from Tari amount
    pub fn from_tari(tari: f64) -> Self {
        Self((tari * 1_000_000.0) as u64)
    }
}

impl fmt::Display for MicroMinotari {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} μT", self.0)
    }
}

impl From<u64> for MicroMinotari {
    fn from(amount: u64) -> Self {
        Self::new(amount)
    }
}

impl From<MicroMinotari> for u64 {
    fn from(amount: MicroMinotari) -> Self {
        amount.as_u64()
    }
}

/// Compressed commitment (33 bytes)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct CompressedCommitment {
    /// The commitment bytes
    #[serde(serialize_with = "crate::hex_utils::serde_helpers::serialize_array_33", deserialize_with = "crate::hex_utils::serde_helpers::deserialize_array_33")]
    pub bytes: [u8; 33],
}

impl CompressedCommitment {
    /// Create a new compressed commitment from bytes
    pub fn new(bytes: [u8; 33]) -> Self {
        Self { bytes }
    }

    /// Get the commitment bytes
    pub fn as_bytes(&self) -> &[u8; 33] {
        &self.bytes
    }

    /// Convert to hex string
    pub fn to_hex(&self) -> String {
        self.bytes.encode_hex()
    }

    /// Create from hex string
    pub fn from_hex(hex: &str) -> Result<Self, HexError> {
        let bytes = hex::decode(hex).map_err(|e| HexError::InvalidHex(e.to_string()))?;
        if bytes.len() != 33 {
            return Err(HexError::InvalidLength {
                expected: 33,
                actual: bytes.len(),
            });
        }
        let mut commitment_bytes = [0u8; 33];
        commitment_bytes.copy_from_slice(&bytes);
        Ok(Self::new(commitment_bytes))
    }
}

impl HexEncodable for CompressedCommitment {
    fn to_hex(&self) -> String {
        self.bytes.encode_hex()
    }
    
    fn from_hex(hex: &str) -> Result<Self, HexError> {
        let bytes = hex::decode(hex).map_err(|e| HexError::InvalidHex(e.to_string()))?;
        if bytes.len() != 33 {
            return Err(HexError::InvalidLength {
                expected: 33,
                actual: bytes.len(),
            });
        }
        let mut commitment_bytes = [0u8; 33];
        commitment_bytes.copy_from_slice(&bytes);
        Ok(Self::new(commitment_bytes))
    }
}

impl HexValidatable for CompressedCommitment {}

/// Compressed public key (Ristretto)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CompressedPublicKey(#[serde(with = "compressed_ristretto_serde")] pub CompressedRistretto);

impl BorshSerialize for CompressedPublicKey {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        BorshSerialize::serialize(&self.0.to_bytes(), writer)
    }
}

impl BorshDeserialize for CompressedPublicKey {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let bytes = <[u8; 32]>::deserialize_reader(reader)?;
        Ok(Self(CompressedRistretto(bytes)))
    }
}

impl CompressedPublicKey {
    /// Create a new compressed public key from bytes
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(CompressedRistretto(bytes))
    }

    /// Get the public key bytes
    pub fn as_bytes(&self) -> [u8; 32] {
        self.0.to_bytes()
    }

    /// Convert to hex string
    pub fn to_hex(&self) -> String {
        hex::encode(self.as_bytes())
    }

    /// Create from hex string
    pub fn from_hex(hex: &str) -> Result<Self, HexError> {
        let bytes = hex::decode(hex).map_err(|e| HexError::InvalidHex(e.to_string()))?;
        if bytes.len() != 32 {
            return Err(HexError::InvalidLength {
                expected: 32,
                actual: bytes.len(),
            });
        }
        let mut key_bytes = [0u8; 32];
        key_bytes.copy_from_slice(&bytes);
        Ok(Self::new(key_bytes))
    }

    /// Decompress to RistrettoPoint
    pub fn decompress(&self) -> Option<RistrettoPoint> {
        self.0.decompress()
    }

    /// Compress from RistrettoPoint
    pub fn from_point(point: &RistrettoPoint) -> Self {
        Self(point.compress())
    }

    /// Create from private key
    pub fn from_private_key(private_key: &PrivateKey) -> Self {
        let point = RistrettoPoint::from(private_key.0 * RISTRETTO_BASEPOINT_POINT);
        Self::from_point(&point)
    }
}

impl HexEncodable for CompressedPublicKey {
    fn to_hex(&self) -> String {
        self.to_hex()
    }
    fn from_hex(hex: &str) -> Result<Self, HexError> {
        Self::from_hex(hex)
    }
}

impl HexValidatable for CompressedPublicKey {}

impl fmt::Display for CompressedPublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

/// Safe array wrapper for zeroization
#[derive(Debug, Clone, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct SafeArray<const N: usize> {
    /// The array data
    pub data: [u8; N],
}

impl<const N: usize> SafeArray<N> {
    /// Create a new safe array
    pub fn new(data: [u8; N]) -> Self {
        Self { data }
    }

    /// Create a default safe array
    pub fn default() -> Self {
        Self { data: [0u8; N] }
    }

    /// Get the array data
    pub fn as_bytes(&self) -> &[u8; N] {
        &self.data
    }

    /// Convert to hex string
    pub fn to_hex(&self) -> String {
        self.data.encode_hex()
    }

    /// Create from hex string
    pub fn from_hex(hex: &str) -> Result<Self, HexError> {
        let bytes = hex::decode(hex).map_err(|e| HexError::InvalidHex(e.to_string()))?;
        if bytes.len() != N {
            return Err(HexError::InvalidLength {
                expected: N,
                actual: bytes.len(),
            });
        }
        let mut array = [0u8; N];
        array.copy_from_slice(&bytes);
        Ok(Self::new(array))
    }
}

impl<const N: usize> Zeroize for SafeArray<N> {
    fn zeroize(&mut self) {
        self.data.zeroize();
    }
}

impl<const N: usize> Drop for SafeArray<N> {
    fn drop(&mut self) {
        self.zeroize();
    }
}

impl<const N: usize> HexEncodable for SafeArray<N> {
    fn to_hex(&self) -> String {
        self.data.encode_hex()
    }

    fn from_hex(hex: &str) -> Result<Self, HexError> {
        let bytes = hex::decode(hex).map_err(|e| HexError::InvalidHex(e.to_string()))?;
        if bytes.len() != N {
            return Err(HexError::InvalidLength {
                expected: N,
                actual: bytes.len(),
            });
        }
        let mut array = [0u8; N];
        array.copy_from_slice(&bytes);
        Ok(Self::new(array))
    }
}

impl<const N: usize> HexValidatable for SafeArray<N> {}

impl<const N: usize> fmt::Display for SafeArray<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

/// Encrypted data key wrapper
pub struct EncryptedDataKey(SafeArray<32>);

impl EncryptedDataKey {
    /// Create from a safe array
    pub fn from(safe_array: SafeArray<32>) -> Self {
        Self(safe_array)
    }

    /// Reveal the key (use with caution)
    pub fn reveal(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }

    /// Get the key as a mutable byte slice
    pub fn as_mut(&mut self) -> &mut [u8; 32] {
        &mut self.0.data
    }
}

impl From<SafeArray<32>> for EncryptedDataKey {
    fn from(safe_array: SafeArray<32>) -> Self {
        Self(safe_array)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_micro_minotari() {
        let amount = MicroMinotari::new(1_000_000);
        assert_eq!(amount.as_u64(), 1_000_000);
        assert_eq!(amount.as_tari(), 1.0);
        assert_eq!(MicroMinotari::from_tari(2.5).as_u64(), 2_500_000);
    }

    #[test]
    fn test_private_key() {
        let key_bytes = [1u8; 32];
        let key = PrivateKey::new(key_bytes);
        assert_eq!(key.as_bytes(), key_bytes);

        let hex = key.to_hex();
        let key_from_hex = PrivateKey::from_hex(&hex).unwrap();
        assert_eq!(key, key_from_hex);
    }

    #[test]
    fn test_compressed_commitment() {
        let commitment_bytes = [1u8; 33];
        let commitment = CompressedCommitment::new(commitment_bytes);
        assert_eq!(commitment.as_bytes(), &commitment_bytes);

        let hex = commitment.to_hex();
        let commitment_from_hex = CompressedCommitment::from_hex(&hex).unwrap();
        assert_eq!(commitment, commitment_from_hex);
    }

    #[test]
    fn test_compressed_public_key() {
        let key_bytes = [1u8; 32];
        let key = CompressedPublicKey::new(key_bytes);
        assert_eq!(key.as_bytes(), key_bytes);

        let hex = key.to_hex();
        let key_from_hex = CompressedPublicKey::from_hex(&hex).unwrap();
        assert_eq!(key, key_from_hex);
    }

    #[test]
    fn test_safe_array() {
        let array_data = [1u8; 32];
        let array = SafeArray::new(array_data);
        assert_eq!(array.as_bytes(), &array_data);

        let hex = array.to_hex();
        let array_from_hex = SafeArray::from_hex(&hex).unwrap();
        assert_eq!(array, array_from_hex);
    }
}
