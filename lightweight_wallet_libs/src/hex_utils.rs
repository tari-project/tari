// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::fmt;

use hex::{FromHex, ToHex};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

use crate::data_structures::{
    EncryptedData, LightweightTransactionOutput, LightweightWalletOutput,
    CompressedPublicKey, SafeArray, PaymentId, PrivateKey, CompressedCommitment,
};

/// Custom serializers for serde
pub mod serde_helpers {
    use super::*;

    /// Serialize a 33-byte array as hex
    pub fn serialize_array_33<S>(bytes: &[u8; 33], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let hex_string = hex::encode(bytes);
        hex_string.serialize(serializer)
    }

    /// Deserialize a 33-byte array from hex
    pub fn deserialize_array_33<'de, D>(deserializer: D) -> Result<[u8; 33], D::Error>
    where
        D: Deserializer<'de>,
    {
        let hex_string = String::deserialize(deserializer)?;
        let bytes = hex::decode(&hex_string).map_err(serde::de::Error::custom)?;
        
        if bytes.len() != 33 {
            return Err(serde::de::Error::custom(format!(
                "Expected 33 bytes, got {}",
                bytes.len()
            )));
        }
        
        let mut array = [0u8; 33];
        array.copy_from_slice(&bytes);
        Ok(array)
    }

    /// Serialize a 32-byte array as hex
    pub fn serialize_array_32<S>(bytes: &[u8; 32], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let hex_string = hex::encode(bytes);
        hex_string.serialize(serializer)
    }

    /// Deserialize a 32-byte array from hex
    pub fn deserialize_array_32<'de, D>(deserializer: D) -> Result<[u8; 32], D::Error>
    where
        D: Deserializer<'de>,
    {
        let hex_string = String::deserialize(deserializer)?;
        let bytes = hex::decode(&hex_string).map_err(serde::de::Error::custom)?;
        
        if bytes.len() != 32 {
            return Err(serde::de::Error::custom(format!(
                "Expected 32 bytes, got {}",
                bytes.len()
            )));
        }
        
        let mut array = [0u8; 32];
        array.copy_from_slice(&bytes);
        Ok(array)
    }
}

/// Error types for hex encoding/decoding operations
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum HexError {
    #[error("Invalid hex string: {0}")]
    InvalidHex(String),
    #[error("Invalid hex length: expected {expected}, got {actual}")]
    InvalidLength { expected: usize, actual: usize },
    #[error("Hex string is empty")]
    EmptyString,
    #[error("Hex string has odd length: {0}")]
    OddLength(usize),
}

impl From<hex::FromHexError> for HexError {
    fn from(err: hex::FromHexError) -> Self {
        HexError::InvalidHex(err.to_string())
    }
}

/// Trait for types that can be converted to and from hex strings
pub trait HexEncodable {
    /// Convert the value to a hex string
    fn to_hex(&self) -> String;
    
    /// Convert the value to a hex string with a prefix (e.g., "0x")
    fn to_hex_with_prefix(&self, prefix: &str) -> String {
        format!("{}{}", prefix, self.to_hex())
    }
    
    /// Convert from a hex string
    fn from_hex(hex: &str) -> Result<Self, HexError>
    where
        Self: Sized;
    
    /// Convert from a hex string, optionally removing a prefix
    fn from_hex_with_prefix(hex: &str, prefix: &str) -> Result<Self, HexError>
    where
        Self: Sized,
    {
        let hex = hex.strip_prefix(prefix).unwrap_or(hex);
        Self::from_hex(hex)
    }
}

/// Trait for types that can be converted to and from hex strings with validation
pub trait HexValidatable: HexEncodable {
    /// Validate that a hex string can be converted to this type
    fn is_valid_hex(hex: &str) -> bool where Self: Sized {
        Self::from_hex(hex).is_ok()
    }
    
    /// Validate that a hex string can be converted to this type, optionally removing a prefix
    fn is_valid_hex_with_prefix(hex: &str, prefix: &str) -> bool where Self: Sized {
        Self::from_hex_with_prefix(hex, prefix).is_ok()
    }
}

/// Utility functions for hex encoding/decoding
pub struct HexUtils;

impl HexUtils {
    /// Convert bytes to hex string
    pub fn to_hex(bytes: &[u8]) -> String {
        bytes.encode_hex()
    }
    
    /// Convert bytes to hex string with prefix
    pub fn to_hex_with_prefix(bytes: &[u8], prefix: &str) -> String {
        format!("{}{}", prefix, Self::to_hex(bytes))
    }
    
    /// Convert hex string to bytes
    pub fn from_hex(hex: &str) -> Result<Vec<u8>, HexError> {
        if hex.is_empty() {
            return Err(HexError::EmptyString);
        }
        
        if hex.len() % 2 != 0 {
            return Err(HexError::OddLength(hex.len()));
        }
        
        Vec::from_hex(hex).map_err(Into::into)
    }
    
    /// Convert hex string to bytes, optionally removing a prefix
    pub fn from_hex_with_prefix(hex: &str, prefix: &str) -> Result<Vec<u8>, HexError> {
        let hex = hex.strip_prefix(prefix).unwrap_or(hex);
        Self::from_hex(hex)
    }
    
    /// Convert hex string to fixed-size byte array
    pub fn from_hex_to_array<const N: usize>(hex: &str) -> Result<[u8; N], HexError> {
        let bytes = Self::from_hex(hex)?;
        
        if bytes.len() != N {
            return Err(HexError::InvalidLength {
                expected: N,
                actual: bytes.len(),
            });
        }
        
        let mut array = [0u8; N];
        array.copy_from_slice(&bytes);
        Ok(array)
    }
    
    /// Convert hex string to fixed-size byte array, optionally removing a prefix
    pub fn from_hex_to_array_with_prefix<const N: usize>(
        hex: &str,
        prefix: &str,
    ) -> Result<[u8; N], HexError> {
        let hex = hex.strip_prefix(prefix).unwrap_or(hex);
        Self::from_hex_to_array(hex)
    }
    
    /// Validate that a string is a valid hex string
    pub fn is_valid_hex(hex: &str) -> bool {
        if hex.is_empty() {
            return false;
        }
        
        if hex.len() % 2 != 0 {
            return false;
        }
        
        hex.chars().all(|c| c.is_ascii_hexdigit())
    }
    
    /// Validate that a string is a valid hex string, optionally removing a prefix
    pub fn is_valid_hex_with_prefix(hex: &str, prefix: &str) -> bool {
        let hex = hex.strip_prefix(prefix).unwrap_or(hex);
        Self::is_valid_hex(hex)
    }
    
    /// Format a hex string with proper spacing (e.g., "12 34 56 78")
    pub fn format_hex_with_spacing(bytes: &[u8], bytes_per_line: Option<usize>) -> String {
        let hex = Self::to_hex(bytes);
        let mut formatted = String::new();
        
        for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
            if i > 0 {
                if let Some(bytes_per_line) = bytes_per_line {
                    if i % bytes_per_line == 0 {
                        formatted.push('\n');
                    } else {
                        formatted.push(' ');
                    }
                } else {
                    formatted.push(' ');
                }
            }
            formatted.push_str(std::str::from_utf8(chunk).unwrap_or("??"));
        }
        
        formatted
    }
    
    /// Convert a hex string to uppercase
    pub fn to_uppercase_hex(bytes: &[u8]) -> String {
        Self::to_hex(bytes).to_uppercase()
    }
    
    /// Convert a hex string to lowercase
    pub fn to_lowercase_hex(bytes: &[u8]) -> String {
        Self::to_hex(bytes).to_lowercase()
    }
}

/// Display wrapper for hex formatting
pub struct HexDisplay<'a>(&'a [u8]);

impl<'a> HexDisplay<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self(bytes)
    }
    
    pub fn with_prefix(bytes: &'a [u8], prefix: &'a str) -> HexDisplayWithPrefix<'a> {
        HexDisplayWithPrefix { bytes, prefix }
    }
}

impl<'a> fmt::Display for HexDisplay<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", HexUtils::to_hex(self.0))
    }
}

impl<'a> fmt::Debug for HexDisplay<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "HexDisplay(\"{}\")", self)
    }
}

/// Display wrapper for hex formatting with prefix
pub struct HexDisplayWithPrefix<'a> {
    bytes: &'a [u8],
    prefix: &'a str,
}

impl<'a> fmt::Display for HexDisplayWithPrefix<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.prefix, HexUtils::to_hex(self.bytes))
    }
}

impl<'a> fmt::Debug for HexDisplayWithPrefix<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "HexDisplayWithPrefix(\"{}\")", self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use primitive_types::U256;

    #[test]
    fn test_hex_utils_basic() {
        let data = [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0];
        
        // Test basic hex conversion
        let hex = HexUtils::to_hex(&data);
        assert_eq!(hex, "123456789abcdef0");
        
        // Test hex conversion with prefix
        let hex_with_prefix = HexUtils::to_hex_with_prefix(&data, "0x");
        assert_eq!(hex_with_prefix, "0x123456789abcdef0");
        
        // Test hex parsing
        let parsed = HexUtils::from_hex(&hex).unwrap();
        assert_eq!(parsed, data);
        
        // Test hex parsing with prefix
        let parsed_with_prefix = HexUtils::from_hex_with_prefix(&hex_with_prefix, "0x").unwrap();
        assert_eq!(parsed_with_prefix, data);
    }
    
    #[test]
    fn test_hex_utils_array() {
        let data = [0x12, 0x34, 0x56, 0x78];
        let hex = "12345678";
        
        // Test array conversion
        let parsed = HexUtils::from_hex_to_array::<4>(hex).unwrap();
        assert_eq!(parsed, data);
        
        // Test array conversion with prefix
        let parsed_with_prefix = HexUtils::from_hex_to_array_with_prefix::<4>("0x12345678", "0x").unwrap();
        assert_eq!(parsed_with_prefix, data);
    }
    
    #[test]
    fn test_hex_utils_validation() {
        // Valid hex strings
        assert!(HexUtils::is_valid_hex("123456789abcdef0"));
        assert!(HexUtils::is_valid_hex("ABCDEF"));
        assert!(!HexUtils::is_valid_hex("")); // Empty string is invalid
        
        // Invalid hex strings
        assert!(!HexUtils::is_valid_hex("123456789abcdef")); // Odd length
        assert!(!HexUtils::is_valid_hex("123456789abcdefg")); // Invalid characters
        
        // Test with prefix
        assert!(HexUtils::is_valid_hex_with_prefix("0x123456789abcdef0", "0x"));
        assert!(!HexUtils::is_valid_hex_with_prefix("0x123456789abcdef", "0x"));
    }
    
    #[test]
    fn test_hex_utils_formatting() {
        let data = [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0];
        
        // Test spacing
        let formatted = HexUtils::format_hex_with_spacing(&data, None);
        assert_eq!(formatted, "12 34 56 78 9a bc de f0");
        
        // Test with line breaks
        let formatted_with_lines = HexUtils::format_hex_with_spacing(&data, Some(4));
        assert_eq!(formatted_with_lines, "12 34 56 78\n9a bc de f0");
        
        // Test case conversion
        let uppercase = HexUtils::to_uppercase_hex(&data);
        assert_eq!(uppercase, "123456789ABCDEF0");
        
        let lowercase = HexUtils::to_lowercase_hex(&data);
        assert_eq!(lowercase, "123456789abcdef0");
    }
    
    #[test]
    fn test_hex_display() {
        let data = [0x12, 0x34, 0x56, 0x78];
        
        // Test basic display
        let display = HexDisplay::new(&data);
        assert_eq!(display.to_string(), "12345678");
        
        // Test display with prefix
        let display_with_prefix = HexDisplay::with_prefix(&data, "0x");
        assert_eq!(display_with_prefix.to_string(), "0x12345678");
    }
    
    #[test]
    fn test_hex_errors() {
        // Test empty string
        assert!(matches!(
            HexUtils::from_hex(""),
            Err(HexError::EmptyString)
        ));
        
        // Test odd length
        assert!(matches!(
            HexUtils::from_hex("123"),
            Err(HexError::OddLength(3))
        ));
        
        // Test invalid hex
        assert!(matches!(
            HexUtils::from_hex("123456789abcdefg"),
            Err(HexError::InvalidHex(_))
        ));
        
        // Test wrong array size
        assert!(matches!(
            HexUtils::from_hex_to_array::<4>("1234567890"),
            Err(HexError::InvalidLength { expected: 4, actual: 5 })
        ));
    }
    
    #[test]
    fn test_private_key_hex() {
        let key = PrivateKey::random();
        let hex = key.to_hex();
        let key_from_hex = PrivateKey::from_hex(&hex).unwrap();
        assert_eq!(key, key_from_hex);
    }
    
    #[test]
    fn test_compressed_commitment_hex() {
        let commitment_bytes = [0x34; 33];
        let commitment = CompressedCommitment::new(commitment_bytes);
        
        // Test to_hex
        let hex = commitment.to_hex();
        assert_eq!(hex.len(), 66); // 33 bytes * 2 hex chars per byte
        
        // Test from_hex
        let parsed = CompressedCommitment::from_hex(&hex).unwrap();
        assert_eq!(parsed, commitment);
        
        // Test validation
        assert!(CompressedCommitment::is_valid_hex(&hex));
        assert!(!CompressedCommitment::is_valid_hex("123")); // Wrong length
    }
    
    #[test]
    fn test_compressed_public_key_hex() {
        let key_bytes = [0x56; 32];
        let public_key = CompressedPublicKey::new(key_bytes);
        
        // Test to_hex
        let hex = public_key.to_hex();
        assert_eq!(hex.len(), 64); // 32 bytes * 2 hex chars per byte
        
        // Test from_hex
        let parsed = CompressedPublicKey::from_hex(&hex).unwrap();
        assert_eq!(parsed, public_key);
        
        // Test validation
        assert!(CompressedPublicKey::is_valid_hex(&hex));
        assert!(!CompressedPublicKey::is_valid_hex("123")); // Wrong length
    }
    
    #[test]
    fn test_safe_array_hex() {
        let array_data = [0x78; 16];
        let safe_array = SafeArray::new(array_data);
        
        // Test to_hex
        let hex = safe_array.to_hex();
        assert_eq!(hex.len(), 32); // 16 bytes * 2 hex chars per byte
        
        // Test from_hex
        let parsed = SafeArray::<16>::from_hex(&hex).unwrap();
        assert_eq!(parsed, safe_array);
        
        // Test validation
        assert!(SafeArray::<16>::is_valid_hex(&hex));
        assert!(!SafeArray::<16>::is_valid_hex("123")); // Wrong length
    }
    
    #[test]
    fn test_encrypted_data_hex() {
        let data = vec![0x9a; 80]; // Use minimum required size
        let encrypted_data = EncryptedData::from_bytes(&data).unwrap();
        
        // Test to_hex
        let hex = encrypted_data.to_hex();
        assert_eq!(hex, hex::encode(&data));
        
        // Test from_hex
        let parsed = EncryptedData::from_hex(&hex).unwrap();
        assert_eq!(parsed.as_bytes(), data.as_slice());
        
        // Test validation
        assert!(EncryptedData::is_valid_hex(&hex));
    }
    
    #[test]
    fn test_payment_id_hex() {
        // Test Empty payment ID
        let empty_payment_id = PaymentId::Empty;
        let hex = empty_payment_id.to_hex();
        assert_eq!(hex, "");
        let parsed = PaymentId::from_hex(&hex).unwrap();
        assert_eq!(parsed, empty_payment_id);
        
        // Test U256 payment ID (64 hex chars)
        let u256_value = U256::from(0x123456789abcdef0u64);
        let u256_hex = format!("{:064x}", u256_value); // pad to 64 chars
        let u256_payment_id = PaymentId::U256 { value: U256::from_str_radix(&u256_hex, 16).unwrap() };
        let parsed = PaymentId::from_hex(&u256_hex).unwrap();
        assert_eq!(parsed, u256_payment_id);
        
        // Test Raw payment ID (less than 64 chars)
        let raw_data = vec![0xaa, 0xbb, 0xcc, 0xdd];
        let hex = hex::encode(&raw_data);
        let raw_payment_id = PaymentId::Raw { data: raw_data.clone() };
        let parsed = PaymentId::from_hex(&hex).unwrap();
        assert_eq!(parsed, raw_payment_id);
        
        // Test validation
        assert!(PaymentId::is_valid_hex(""));
        assert!(PaymentId::is_valid_hex(&u256_hex));
        assert!(PaymentId::is_valid_hex(&hex));
    }
    
    #[test]
    fn test_wallet_output_hex() {
        // Create a simple wallet output using default values
        let wallet_output = LightweightWalletOutput::default();
        
        // Test to_hex
        let hex = wallet_output.to_hex();
        assert!(!hex.is_empty());
        
        // Test from_hex
        let parsed = LightweightWalletOutput::from_hex(&hex).unwrap();
        assert_eq!(parsed, wallet_output);
        
        // Test validation
        assert!(LightweightWalletOutput::is_valid_hex(&hex));
    }
    
    #[test]
    fn test_transaction_output_hex() {
        // Create a simple transaction output using default values
        let tx_output = LightweightTransactionOutput::default();
        
        // Test to_hex
        let hex = tx_output.to_hex();
        assert!(!hex.is_empty());
        
        // Test from_hex
        let parsed = LightweightTransactionOutput::from_hex(&hex).unwrap();
        assert_eq!(parsed, tx_output);
        
        // Test validation
        assert!(LightweightTransactionOutput::is_valid_hex(&hex));
    }
    
    #[test]
    fn test_hex_encodable_traits() {
        // Test that all types implement HexEncodable and HexValidatable
        let private_key = PrivateKey::new([0x12; 32]);
        let commitment = CompressedCommitment::new([0x34; 33]);
        let public_key = CompressedPublicKey::new([0x56; 32]);
        let safe_array = SafeArray::new([0x78; 16]);
        let encrypted_data = EncryptedData::from_bytes(&vec![0x9a; 80]).unwrap();
        let payment_id = PaymentId::U256 { value: U256::from(0x123456789abcdef0u64) };
        
        // Test that they all have hex methods
        assert!(!private_key.to_hex().is_empty());
        assert!(!commitment.to_hex().is_empty());
        assert!(!public_key.to_hex().is_empty());
        assert!(!safe_array.to_hex().is_empty());
        assert!(!encrypted_data.to_hex().is_empty());
        assert!(!payment_id.to_hex().is_empty());
        
        // Test validation
        assert!(PrivateKey::is_valid_hex(&private_key.to_hex()));
        assert!(CompressedCommitment::is_valid_hex(&commitment.to_hex()));
        assert!(CompressedPublicKey::is_valid_hex(&public_key.to_hex()));
        assert!(SafeArray::<16>::is_valid_hex(&safe_array.to_hex()));
        assert!(EncryptedData::is_valid_hex(&encrypted_data.to_hex()));
        assert!(PaymentId::is_valid_hex(&payment_id.to_hex()));
    }
} 