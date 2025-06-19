// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Payment ID extraction from encrypted data
//!
//! This module provides functionality to extract the payment ID from
//! an EncryptedData instance, using a provided decryption key and commitment.

use crate::{
    data_structures::{
        encrypted_data::EncryptedData,
        payment_id::PaymentId,
        types::{CompressedCommitment, PrivateKey},
    },
    errors::LightweightWalletError,
};

/// Result of payment ID extraction
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentIdExtractionResult {
    /// The extracted payment ID (if successful)
    pub payment_id: Option<PaymentId>,
    /// Error message if extraction failed
    pub error: Option<String>,
}

impl PaymentIdExtractionResult {
    pub fn success(payment_id: PaymentId) -> Self {
        Self {
            payment_id: Some(payment_id),
            error: None,
        }
    }
    pub fn failure(error: String) -> Self {
        Self {
            payment_id: None,
            error: Some(error),
        }
    }
    pub fn is_success(&self) -> bool {
        self.payment_id.is_some()
    }
    pub fn error_message(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

/// Extracts the payment ID from encrypted data using the provided key and commitment
pub struct PaymentIdExtractor;

impl PaymentIdExtractor {
    /// Attempt to extract the payment ID from encrypted data
    pub fn extract(
        encrypted_data: &EncryptedData,
        decryption_key: &PrivateKey,
        commitment: &CompressedCommitment,
    ) -> PaymentIdExtractionResult {
        match EncryptedData::decrypt_data(decryption_key, commitment, encrypted_data) {
            Ok((_value, _mask, payment_id)) => PaymentIdExtractionResult::success(payment_id),
            Err(e) => PaymentIdExtractionResult::failure(format!("Failed to extract payment ID: {e}")),
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
        let (encrypted_data, commitment, key) = create_test_encrypted_data(PaymentId::U256 { value: [1u8; 32].into() });
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
    fn test_extract_utf8_string_from_address_and_data() {
        use crate::data_structures::types::MicroMinotari;
        let encryption_key = PrivateKey::random();
        let commitment = CompressedCommitment::new([0x09; 33]);
        let value = MicroMinotari::new(1234);
        let mask = PrivateKey::random();
        let address = vec![0x01, 0x02, 0x03, 0x04, 0x05]; // Simulate address bytes
        let utf8_data = "test".as_bytes().to_vec();
        let payment_id = PaymentId::AddressAndData {
            address: address.clone(),
            data: utf8_data.clone(),
        };
        let encrypted_data = EncryptedData::encrypt_data(
            &encryption_key,
            &commitment,
            value,
            &mask,
            payment_id.clone(),
        ).unwrap();
        let result = PaymentIdExtractor::extract(&encrypted_data, &encryption_key, &commitment);
        assert!(result.is_success());
        match result.payment_id {
            Some(PaymentId::AddressAndData { address: ref a, data: ref d }) => {
                assert_eq!(a, &address);
                assert_eq!(d, &utf8_data);
                assert_eq!(std::str::from_utf8(d).unwrap(), "test");
            }
            _ => panic!("Expected AddressAndData payment ID with UTF-8 data"),
        }
    }

    #[test]
    fn test_extract_real_address_and_payment_id() {
        use crate::data_structures::types::MicroMinotari;
        
        // Real Tari address: 169MdJoZvqcy2iZForAvTh286VT6V69htSHS2swJB2uwyaeyDNUt2S1ZZ6wnrk1eB7aZHBrzBtdnNPpoREKRYwrjiBuTr2xd
        // This is base58 encoded. In a real implementation, you would use a base58 decoder:
        // let address_bytes = bs58::decode("169MdJoZvqcy2iZForAvTh286VT6V69htSHS2swJB2uwyaeyDNUt2S1ZZ6wnrk1eB7aZHBrzBtdnNPpoREKRYwrjiBuTr2xd").into_vec().unwrap();
        
        // Payment ID: 74657374 (hex) = "test" (ASCII)
        let payment_id_hex = "74657374";
        let expected_utf8 = "test";
        
        // For testing purposes, we'll create realistic address bytes
        // In reality, this would be the decoded base58 address (typically 69 bytes for Tari addresses)
        let address_bytes = vec![0x16, 0x9d, 0x4a, 0x6f, 0x76, 0x63, 0x79, 0x32, 0x69, 0x5a, 0x46, 0x6f, 0x72, 0x41, 0x76, 0x54, 0x68, 0x32, 0x38, 0x36, 0x56, 0x54, 0x36, 0x56, 0x36, 0x39, 0x68, 0x74, 0x53, 0x48, 0x53, 0x32, 0x73, 0x77, 0x4a, 0x42, 0x32, 0x75, 0x77, 0x79, 0x61, 0x65, 0x79, 0x44, 0x4e, 0x55, 0x74, 0x32, 0x53, 0x31, 0x5a, 0x5a, 0x36, 0x77, 0x6e, 0x72, 0x6b, 0x31, 0x65, 0x42, 0x37, 0x61, 0x5a, 0x48, 0x42, 0x72, 0x7a, 0x42, 0x74, 0x64, 0x6e, 0x4e, 0x50, 0x70, 0x6f, 0x52, 0x45, 0x4b, 0x52, 0x59, 0x77, 0x72, 0x6a, 0x69, 0x42, 0x75, 0x54, 0x72, 0x32, 0x78, 0x64];
        
        // Decode the hex payment ID
        let payment_id_bytes = hex::decode(payment_id_hex).expect("Invalid hex payment ID");
        assert_eq!(std::str::from_utf8(&payment_id_bytes).unwrap(), expected_utf8);
        
        let encryption_key = PrivateKey::random();
        let commitment = CompressedCommitment::new([0x0A; 33]);
        let value = MicroMinotari::new(5000);
        let mask = PrivateKey::random();
        
        let payment_id = PaymentId::AddressAndData {
            address: address_bytes.clone(),
            data: payment_id_bytes.clone(),
        };
        
        let encrypted_data = EncryptedData::encrypt_data(
            &encryption_key,
            &commitment,
            value,
            &mask,
            payment_id.clone(),
        ).unwrap();
        
        let result = PaymentIdExtractor::extract(&encrypted_data, &encryption_key, &commitment);
        assert!(result.is_success());
        
        match result.payment_id {
            Some(PaymentId::AddressAndData { address: ref a, data: ref d }) => {
                assert_eq!(a, &address_bytes);
                assert_eq!(d, &payment_id_bytes);
                assert_eq!(std::str::from_utf8(d).unwrap(), expected_utf8);
                println!("Successfully extracted payment ID: '{}' from address", expected_utf8);
            }
            _ => panic!("Expected AddressAndData payment ID with real address data"),
        }
    }
} 