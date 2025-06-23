// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Payment ID extraction from encrypted data
//!
//! This module provides functionality to extract the payment ID from
//! an EncryptedData instance, using a provided decryption key and commitment.
//! It supports all payment ID types: Empty, U256, Open, AddressAndData, TransactionInfo, and Raw.

use crate::{
    data_structures::{
        encrypted_data::EncryptedData,
        payment_id::{PaymentId, TxType},
        types::{CompressedCommitment, PrivateKey},
    },
    hex_utils::HexEncodable,
};
use primitive_types::U256;
use std::str::FromStr;

/// Result of payment ID extraction
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentIdExtractionResult {
    /// The extracted payment ID (if successful)
    pub payment_id: Option<PaymentId>,
    /// Error message if extraction failed
    pub error: Option<String>,
    /// Additional metadata about the extraction
    pub metadata: PaymentIdMetadata,
}

/// Metadata about the extracted payment ID
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentIdMetadata {
    /// The transaction type inferred from the payment ID
    pub transaction_type: Option<TxType>,
    /// Whether the payment ID contains valid UTF-8 data
    pub has_valid_utf8: bool,
    /// The size of the payment ID in bytes
    pub size_bytes: usize,
    /// Whether this is a standard payment ID format
    pub is_standard_format: bool,
}

impl PaymentIdMetadata {
    pub fn new(payment_id: &PaymentId) -> Self {
        let transaction_type = payment_id.get_type();
        let size_bytes = payment_id.get_size();
        let has_valid_utf8 = Self::check_utf8_validity(payment_id);
        let is_standard_format = Self::is_standard_format(payment_id);

        Self {
            transaction_type: Some(transaction_type),
            has_valid_utf8,
            size_bytes,
            is_standard_format,
        }
    }

    fn check_utf8_validity(payment_id: &PaymentId) -> bool {
        match payment_id {
            PaymentId::Empty => true,
            PaymentId::U256 { .. } => true,
            PaymentId::Open { data } => std::str::from_utf8(data).is_ok(),
            PaymentId::AddressAndData { data, .. } => std::str::from_utf8(data).is_ok(),
            PaymentId::TransactionInfo { .. } => true,
            PaymentId::Raw { data } => std::str::from_utf8(data).is_ok(),
        }
    }

    fn is_standard_format(payment_id: &PaymentId) -> bool {
        match payment_id {
            PaymentId::Empty => true,
            PaymentId::U256 { .. } => true,
            PaymentId::Open { data } => data.len() <= 256, // Standard limit
            PaymentId::AddressAndData { address, data } => {
                address.len() <= 256 && data.len() <= 256 // Standard limits
            }
            PaymentId::TransactionInfo { tx_id, .. } => tx_id.len() == 32, // Standard tx ID size
            PaymentId::Raw { data } => data.len() <= 256, // Standard limit
        }
    }
}

impl PaymentIdExtractionResult {
    pub fn success(payment_id: PaymentId) -> Self {
        let metadata = PaymentIdMetadata::new(&payment_id);
        Self {
            payment_id: Some(payment_id),
            error: None,
            metadata,
        }
    }

    pub fn failure(error: String) -> Self {
        Self {
            payment_id: None,
            error: Some(error),
            metadata: PaymentIdMetadata {
                transaction_type: None,
                has_valid_utf8: false,
                size_bytes: 0,
                is_standard_format: false,
            },
        }
    }

    pub fn is_success(&self) -> bool {
        self.payment_id.is_some()
    }

    pub fn error_message(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn get_payment_id(&self) -> Option<&PaymentId> {
        self.payment_id.as_ref()
    }

    pub fn get_metadata(&self) -> &PaymentIdMetadata {
        &self.metadata
    }
}

/// Enhanced payment ID extractor with comprehensive support for all payment ID types
pub struct PaymentIdExtractor;

impl PaymentIdExtractor {
    /// Attempt to extract the payment ID from encrypted data
    pub fn extract(
        encrypted_data: &EncryptedData,
        decryption_key: &PrivateKey,
        commitment: &CompressedCommitment,
    ) -> PaymentIdExtractionResult {
        match EncryptedData::decrypt_data(decryption_key, commitment, encrypted_data) {
            Ok((_value, _mask, payment_id)) => {
                match Self::validate_payment_id(&payment_id) {
                    Ok(()) => PaymentIdExtractionResult::success(payment_id),
                    Err(e) => PaymentIdExtractionResult::failure(format!("Payment ID validation failed: {}", e)),
                }
            }
            Err(e) => PaymentIdExtractionResult::failure(format!("Failed to decrypt data: {}", e)),
        }
    }

    /// Extract and validate a specific payment ID type
    pub fn extract_with_validation(
        encrypted_data: &EncryptedData,
        decryption_key: &PrivateKey,
        commitment: &CompressedCommitment,
        expected_type: Option<PaymentIdType>,
    ) -> PaymentIdExtractionResult {
        let result = Self::extract(encrypted_data, decryption_key, commitment);
        
        if let Some(payment_id) = &result.payment_id {
            if let Some(expected) = expected_type {
                if !Self::matches_type(payment_id, &expected) {
                    return PaymentIdExtractionResult::failure(format!(
                        "Payment ID type mismatch: expected {:?}, got {:?}",
                        expected,
                        Self::get_payment_id_type(payment_id)
                    ));
                }
            }
        }
        
        result
    }

    /// Extract payment ID and convert to string representation
    pub fn extract_as_string(
        encrypted_data: &EncryptedData,
        decryption_key: &PrivateKey,
        commitment: &CompressedCommitment,
    ) -> Result<String, String> {
        let result = Self::extract(encrypted_data, decryption_key, commitment);
        
        if let Some(payment_id) = result.payment_id {
            Ok(Self::payment_id_to_string(&payment_id))
        } else {
            Err(result.error.unwrap_or_else(|| "Unknown error".to_string()))
        }
    }

    /// Extract payment ID and convert to hex representation
    pub fn extract_as_hex(
        encrypted_data: &EncryptedData,
        decryption_key: &PrivateKey,
        commitment: &CompressedCommitment,
    ) -> Result<String, String> {
        let result = Self::extract(encrypted_data, decryption_key, commitment);
        
        if let Some(payment_id) = result.payment_id {
            Ok(payment_id.to_hex())
        } else {
            Err(result.error.unwrap_or_else(|| "Unknown error".to_string()))
        }
    }

    /// Validate a payment ID
    fn validate_payment_id(payment_id: &PaymentId) -> Result<(), String> {
        match payment_id {
            PaymentId::Empty => {
                Ok(())
            }
            PaymentId::U256 { value } => {
                if *value == U256::zero() {
                    Err("U256 payment ID cannot be zero".to_string())
                } else {
                    Ok(())
                }
            }
            PaymentId::Open { data } => {
                if data.is_empty() {
                    Err("Open payment ID data cannot be empty".to_string())
                } else if data.len() > 256 {
                    Err("Open payment ID data too large (max 256 bytes)".to_string())
                } else {
                    Ok(())
                }
            }
            PaymentId::AddressAndData { address, data } => {
                if address.is_empty() {
                    Err("AddressAndData payment ID address cannot be empty".to_string())
                } else if address.len() > 256 {
                    Err("AddressAndData payment ID address too large (max 256 bytes)".to_string())
                } else if data.len() > 256 {
                    Err("AddressAndData payment ID data too large (max 256 bytes)".to_string())
                } else {
                    Ok(())
                }
            }
            PaymentId::TransactionInfo { tx_id, output_index } => {
                if tx_id.is_empty() {
                    Err("TransactionInfo payment ID tx_id cannot be empty".to_string())
                } else if tx_id.len() > 256 {
                    Err("TransactionInfo payment ID tx_id too large (max 256 bytes)".to_string())
                } else {
                    Ok(())
                }
            }
            PaymentId::Raw { data } => {
                if data.is_empty() {
                    Err("Raw payment ID data cannot be empty".to_string())
                } else if data.len() > 256 {
                    Err("Raw payment ID data too large (max 256 bytes)".to_string())
                } else {
                    Ok(())
                }
            }
        }
    }

    /// Convert payment ID to string representation
    fn payment_id_to_string(payment_id: &PaymentId) -> String {
        match payment_id {
            PaymentId::Empty => "Empty".to_string(),
            PaymentId::U256 { value } => {
                // Format as zero-padded 64-character hex string
                let mut bytes = [0u8; 32];
                value.to_big_endian(&mut bytes);
                format!("U256: {:064x}", U256::from_big_endian(&bytes))
            }
            PaymentId::Open { data } => {
                if let Ok(s) = std::str::from_utf8(data) {
                    format!("Open: {}", s)
                } else {
                    format!("Open: {}", hex::encode(data))
                }
            }
            PaymentId::AddressAndData { address, data } => {
                let address_str = if let Ok(s) = std::str::from_utf8(address) {
                    s.to_string()
                } else {
                    hex::encode(address)
                };
                let data_str = if let Ok(s) = std::str::from_utf8(data) {
                    s.to_string()
                } else {
                    hex::encode(data)
                };
                format!("AddressAndData: address={}, data={}", address_str, data_str)
            }
            PaymentId::TransactionInfo { tx_id, output_index } => {
                format!("TransactionInfo: tx_id={}, output_index={}", hex::encode(tx_id), output_index)
            }
            PaymentId::Raw { data } => {
                if let Ok(s) = std::str::from_utf8(data) {
                    format!("Raw: {}", s)
                } else {
                    format!("Raw: {}", hex::encode(data))
                }
            }
        }
    }

    /// Check if payment ID matches a specific type
    fn matches_type(payment_id: &PaymentId, expected_type: &PaymentIdType) -> bool {
        Self::get_payment_id_type(payment_id) == *expected_type
    }

    /// Get the type of a payment ID
    fn get_payment_id_type(payment_id: &PaymentId) -> PaymentIdType {
        match payment_id {
            PaymentId::Empty => PaymentIdType::Empty,
            PaymentId::U256 { .. } => PaymentIdType::U256,
            PaymentId::Open { .. } => PaymentIdType::Open,
            PaymentId::AddressAndData { .. } => PaymentIdType::AddressAndData,
            PaymentId::TransactionInfo { .. } => PaymentIdType::TransactionInfo,
            PaymentId::Raw { .. } => PaymentIdType::Raw,
        }
    }

    /// Create a payment ID from string representation
    pub fn from_string(s: &str) -> Result<PaymentId, String> {
        if s.is_empty() || s == "Empty" {
            return Ok(PaymentId::Empty);
        }

        if s.starts_with("U256: ") {
            let value_str = &s[6..];
            let value = U256::from_str(value_str)
                .map_err(|e| format!("Invalid U256 value: {}", e))?;
            return Ok(PaymentId::U256 { value });
        }

        if s.starts_with("Open: ") {
            let data_str = &s[6..];
            let data = data_str.as_bytes().to_vec();
            return Ok(PaymentId::Open { data });
        }

        if s.starts_with("Raw: ") {
            let data_str = &s[5..];
            let data = data_str.as_bytes().to_vec();
            return Ok(PaymentId::Raw { data });
        }

        // Try to parse as hex for other types
        if let Ok(bytes) = hex::decode(s) {
            return Ok(PaymentId::Raw { data: bytes });
        }

        Err(format!("Unable to parse payment ID from string: {}", s))
    }
}

/// Payment ID type enumeration for validation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaymentIdType {
    Empty,
    U256,
    Open,
    AddressAndData,
    TransactionInfo,
    Raw,
}

impl std::fmt::Display for PaymentIdType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PaymentIdType::Empty => write!(f, "Empty"),
            PaymentIdType::U256 => write!(f, "U256"),
            PaymentIdType::Open => write!(f, "Open"),
            PaymentIdType::AddressAndData => write!(f, "AddressAndData"),
            PaymentIdType::TransactionInfo => write!(f, "TransactionInfo"),
            PaymentIdType::Raw => write!(f, "Raw"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_structures::{
        payment_id::PaymentId,
        types::{CompressedCommitment, PrivateKey, MicroMinotari},
        encrypted_data::EncryptedData,
    };
    use hex;

    fn create_test_encrypted_data(payment_id: PaymentId) -> (EncryptedData, CompressedCommitment, PrivateKey) {
        let encryption_key = PrivateKey::random();
        let commitment = CompressedCommitment::new([0x08; 33]);
        let value = MicroMinotari::new(1000);
        let mask = PrivateKey::random();
        let encrypted_data = EncryptedData::encrypt_data(
            &encryption_key,
            &commitment,
            value,
            &mask,
            payment_id,
        ).unwrap();
        (encrypted_data, commitment, encryption_key)
    }

    #[test]
    fn test_extract_payment_id_success() {
        let u256_bytes = [0u8; 31].iter().cloned().chain([1u8]).collect::<Vec<u8>>();
        let (encrypted_data, commitment, key) = create_test_encrypted_data(PaymentId::U256 { value: U256::from_big_endian(&u256_bytes) });
        let result = PaymentIdExtractor::extract(&encrypted_data, &key, &commitment);
        assert!(result.is_success());
        assert!(matches!(result.payment_id, Some(PaymentId::U256 { .. })));
    }

    #[test]
    fn test_extract_payment_id_failure_wrong_key() {
        let (encrypted_data, commitment, _key) = create_test_encrypted_data(PaymentId::Empty);
        let wrong_key = PrivateKey::random();
        let result = PaymentIdExtractor::extract(&encrypted_data, &wrong_key, &commitment);
        assert!(!result.is_success());
        assert!(result.error_message().is_some());
    }

    #[test]
    fn test_extract_all_payment_id_types() {
        let encryption_key = PrivateKey::random();
        let commitment = CompressedCommitment::new([0x09; 33]);
        let value = MicroMinotari::new(1234);
        let mask = PrivateKey::random();

        // Test Empty
        let empty_payment_id = PaymentId::Empty;
        let encrypted_empty = EncryptedData::encrypt_data(
            &encryption_key,
            &commitment,
            value,
            &mask,
            empty_payment_id,
        ).unwrap();
        let result_empty = PaymentIdExtractor::extract(&encrypted_empty, &encryption_key, &commitment);
        assert!(result_empty.is_success());
        assert!(matches!(result_empty.payment_id, Some(PaymentId::Empty)));

        // Test U256
        let u256_bytes = [0u8; 31].iter().cloned().chain([1u8]).collect::<Vec<u8>>();
        let u256_payment_id = PaymentId::U256 { value: U256::from_big_endian(&u256_bytes) };
        let encrypted_u256 = EncryptedData::encrypt_data(
            &encryption_key,
            &commitment,
            value,
            &mask,
            u256_payment_id,
        ).unwrap();
        let result_u256 = PaymentIdExtractor::extract(&encrypted_u256, &encryption_key, &commitment);
        assert!(result_u256.is_success());
        assert!(matches!(result_u256.payment_id, Some(PaymentId::U256 { .. })));

        // Test Open
        let open_payment_id = PaymentId::Open { data: b"test_data".to_vec() };
        let encrypted_open = EncryptedData::encrypt_data(
            &encryption_key,
            &commitment,
            value,
            &mask,
            open_payment_id,
        ).unwrap();
        let result_open = PaymentIdExtractor::extract(&encrypted_open, &encryption_key, &commitment);
        assert!(result_open.is_success());
        assert!(matches!(result_open.payment_id, Some(PaymentId::Open { .. })));

        // Test AddressAndData
        let address_data_payment_id = PaymentId::AddressAndData {
            address: b"test_address".to_vec(),
            data: b"test_data".to_vec(),
        };
        let encrypted_address_data = EncryptedData::encrypt_data(
            &encryption_key,
            &commitment,
            value,
            &mask,
            address_data_payment_id,
        ).unwrap();
        let result_address_data = PaymentIdExtractor::extract(&encrypted_address_data, &encryption_key, &commitment);
        assert!(result_address_data.is_success());
        assert!(matches!(result_address_data.payment_id, Some(PaymentId::AddressAndData { .. })));

        // Test TransactionInfo
        let tx_info_payment_id = PaymentId::TransactionInfo {
            tx_id: [3u8; 32].to_vec(),
            output_index: 42,
        };
        let encrypted_tx_info = EncryptedData::encrypt_data(
            &encryption_key,
            &commitment,
            value,
            &mask,
            tx_info_payment_id,
        ).unwrap();
        let result_tx_info = PaymentIdExtractor::extract(&encrypted_tx_info, &encryption_key, &commitment);
        assert!(result_tx_info.is_success());
        assert!(matches!(result_tx_info.payment_id, Some(PaymentId::TransactionInfo { .. })));

        // Test Raw
        let raw_payment_id = PaymentId::Raw { data: b"raw_data".to_vec() };
        let encrypted_raw = EncryptedData::encrypt_data(
            &encryption_key,
            &commitment,
            value,
            &mask,
            raw_payment_id,
        ).unwrap();
        let result_raw = PaymentIdExtractor::extract(&encrypted_raw, &encryption_key, &commitment);
        assert!(result_raw.is_success());
        assert!(matches!(result_raw.payment_id, Some(PaymentId::Raw { .. })));
    }

    #[test]
    fn test_payment_id_validation() {
        // Test valid payment IDs
        assert!(PaymentIdExtractor::validate_payment_id(&PaymentId::Empty).is_ok());
        let u256_bytes = [0u8; 31].iter().cloned().chain([1u8]).collect::<Vec<u8>>();
        assert!(PaymentIdExtractor::validate_payment_id(&PaymentId::U256 { value: U256::from_big_endian(&u256_bytes) }).is_ok());
        assert!(PaymentIdExtractor::validate_payment_id(&PaymentId::Open { data: b"test".to_vec() }).is_ok());

        // Test invalid payment IDs
        assert!(PaymentIdExtractor::validate_payment_id(&PaymentId::U256 { value: U256::zero() }).is_err());
        assert!(PaymentIdExtractor::validate_payment_id(&PaymentId::Open { data: vec![] }).is_err());
        assert!(PaymentIdExtractor::validate_payment_id(&PaymentId::Open { data: vec![0u8; 300] }).is_err());
    }

    #[test]
    fn test_payment_id_to_string() {
        assert_eq!(PaymentIdExtractor::payment_id_to_string(&PaymentId::Empty), "Empty");
        assert_eq!(PaymentIdExtractor::payment_id_to_string(&PaymentId::U256 { value: U256::from_big_endian(&[0u8; 31].iter().cloned().chain([1u8]).collect::<Vec<u8>>()[..]) }), "U256: 0000000000000000000000000000000000000000000000000000000000000001");
        assert_eq!(PaymentIdExtractor::payment_id_to_string(&PaymentId::Open { data: b"test".to_vec() }), "Open: test");
        assert_eq!(PaymentIdExtractor::payment_id_to_string(&PaymentId::Raw { data: b"raw".to_vec() }), "Raw: raw");
    }

    #[test]
    fn test_payment_id_from_string() {
        assert!(matches!(PaymentIdExtractor::from_string("Empty").unwrap(), PaymentId::Empty));
        assert!(matches!(PaymentIdExtractor::from_string("").unwrap(), PaymentId::Empty));
        assert!(matches!(PaymentIdExtractor::from_string("Open: test").unwrap(), PaymentId::Open { .. }));
        assert!(matches!(PaymentIdExtractor::from_string("Raw: raw_data").unwrap(), PaymentId::Raw { .. }));
    }

    #[test]
    fn test_extract_as_string() {
        let (encrypted_data, commitment, key) = create_test_encrypted_data(PaymentId::Open { data: b"test_string".to_vec() });
        let result = PaymentIdExtractor::extract_as_string(&encrypted_data, &key, &commitment);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Open: test_string");
    }

    #[test]
    fn test_extract_as_hex() {
        let u256_bytes = [0u8; 31].iter().cloned().chain([1u8]).collect::<Vec<u8>>();
        let (encrypted_data, commitment, key) = create_test_encrypted_data(PaymentId::U256 { value: U256::from_big_endian(&u256_bytes) });
        let result = PaymentIdExtractor::extract_as_hex(&encrypted_data, &key, &commitment);
        assert!(result.is_ok());
        assert!(result.unwrap().contains("0000000000000000000000000000000000000000000000000000000000000001"));
    }

    #[test]
    fn test_metadata_extraction() {
        let (encrypted_data, commitment, key) = create_test_encrypted_data(PaymentId::AddressAndData {
            address: b"address".to_vec(),
            data: b"data".to_vec(),
        });
        let result = PaymentIdExtractor::extract(&encrypted_data, &key, &commitment);
        assert!(result.is_success());
        
        let metadata = result.get_metadata();
        assert_eq!(metadata.transaction_type, Some(TxType::PaymentToOther));
        assert!(metadata.has_valid_utf8);
        assert!(metadata.is_standard_format);
        assert!(metadata.size_bytes > 0);
    }
} 