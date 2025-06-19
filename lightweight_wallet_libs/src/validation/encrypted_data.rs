// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Encrypted data integrity validation for lightweight wallets
//! 
//! This module provides validation for encrypted data structure and integrity
//! without requiring decryption.

use crate::{
    data_structures::encrypted_data::EncryptedData,
    errors::ValidationError,
};

/// Lightweight encrypted data integrity validator
/// 
/// This provides validation for encrypted data structure and integrity
/// without requiring the encryption key or decryption.
#[derive(Debug, Clone)]
pub struct LightweightEncryptedDataValidator {
    /// Maximum allowed encrypted data size in bytes
    max_size: usize,
    /// Minimum required encrypted data size in bytes
    min_size: usize,
}

impl Default for LightweightEncryptedDataValidator {
    fn default() -> Self {
        Self {
            max_size: 1024, // 1KB as reasonable default
            min_size: 64,   // Minimum size for valid encrypted data
        }
    }
}

impl LightweightEncryptedDataValidator {
    /// Create a new validator with custom size limits
    pub fn new(min_size: usize, max_size: usize) -> Self {
        Self {
            min_size,
            max_size,
        }
    }

    /// Get the maximum allowed size
    pub fn max_size(&self) -> usize {
        self.max_size
    }

    /// Get the minimum required size
    pub fn min_size(&self) -> usize {
        self.min_size
    }

    /// Validate encrypted data integrity
    /// 
    /// # Arguments
    /// * `encrypted_data` - The encrypted data to validate
    /// 
    /// # Returns
    /// * `Ok(())` if the data is valid
    /// * `Err(ValidationError)` if the data is invalid
    pub fn validate_integrity(&self, encrypted_data: &EncryptedData) -> Result<(), ValidationError> {
        let data_bytes = encrypted_data.as_bytes();

        // Check size constraints
        if data_bytes.len() < self.min_size {
            return Err(ValidationError::IntegrityCheckFailed(
                format!("Encrypted data too small: {} bytes (minimum: {} bytes)", data_bytes.len(), self.min_size).into(),
            ));
        }

        if data_bytes.len() > self.max_size {
            return Err(ValidationError::IntegrityCheckFailed(
                format!("Encrypted data too large: {} bytes (maximum: {} bytes)", data_bytes.len(), self.max_size).into(),
            ));
        }

        // Check for suspicious patterns that might indicate corruption
        if data_bytes.iter().all(|&b| b == 0) {
            return Err(ValidationError::IntegrityCheckFailed(
                "Encrypted data contains only zeros".into(),
            ));
        }

        if data_bytes.iter().all(|&b| b == 0xFF) {
            return Err(ValidationError::IntegrityCheckFailed(
                "Encrypted data contains only ones".into(),
            ));
        }

        // Check for low entropy patterns that might indicate corruption
        let mut consecutive_zeros = 0;
        let mut consecutive_ones = 0;
        let mut max_consecutive_zeros = 0;
        let mut max_consecutive_ones = 0;

        for &byte in data_bytes {
            if byte == 0 {
                consecutive_zeros += 1;
                consecutive_ones = 0;
                max_consecutive_zeros = max_consecutive_zeros.max(consecutive_zeros);
            } else if byte == 0xFF {
                consecutive_ones += 1;
                consecutive_zeros = 0;
                max_consecutive_ones = max_consecutive_ones.max(consecutive_ones);
            } else {
                consecutive_zeros = 0;
                consecutive_ones = 0;
            }
        }

        // Check for suspiciously long runs of zeros or ones
        if max_consecutive_zeros > data_bytes.len() / 2 {
            return Err(ValidationError::IntegrityCheckFailed(
                "Encrypted data contains suspiciously long runs of zeros".into(),
            ));
        }

        if max_consecutive_ones > data_bytes.len() / 2 {
            return Err(ValidationError::IntegrityCheckFailed(
                "Encrypted data contains suspiciously long runs of ones".into(),
            ));
        }

        // Check for repeating patterns that might indicate corruption
        if data_bytes.len() >= 8 {
            let pattern_size = data_bytes.len() / 8;
            let mut has_repeating_pattern = true;
            
            for i in 0..pattern_size {
                if data_bytes[i] != data_bytes[i + pattern_size] {
                    has_repeating_pattern = false;
                    break;
                }
            }
            
            if has_repeating_pattern {
                return Err(ValidationError::IntegrityCheckFailed(
                    "Encrypted data contains suspicious repeating patterns".into(),
                ));
            }
        }

        // Check entropy (encrypted data should have high entropy)
        if !self.has_sufficient_entropy(data_bytes) {
            return Err(ValidationError::IntegrityCheckFailed(
                "Encrypted data has insufficient entropy (may be corrupted)".into(),
            ));
        }

        Ok(())
    }

    /// Validate multiple encrypted data items in batch
    /// 
    /// # Arguments
    /// * `encrypted_data_items` - Vector of encrypted data to validate
    /// 
    /// # Returns
    /// * `Ok(())` if all data is valid
    /// * `Err(ValidationError)` if any data is invalid
    pub fn validate_batch(&self, encrypted_data_items: &[EncryptedData]) -> Result<(), ValidationError> {
        let mut failed_count = 0;
        for (index, encrypted_data) in encrypted_data_items.iter().enumerate() {
            if let Err(e) = self.validate_integrity(encrypted_data) {
                failed_count += 1;
                println!("Item {}: {}", index, e.to_string());
            }
        }
        if failed_count > 0 {
            return Err(ValidationError::IntegrityCheckFailed(
                format!("Batch validation failed: {} items failed", failed_count).into(),
            ));
        }
        Ok(())
    }

    /// Check if data has sufficient entropy (basic check)
    fn has_sufficient_entropy(&self, data: &[u8]) -> bool {
        if data.len() < 16 {
            return true; // Too short for meaningful entropy check
        }

        // Count unique bytes
        let mut byte_counts = [0u8; 256];
        for &byte in data {
            byte_counts[byte as usize] += 1;
        }

        // Calculate basic entropy measure
        let unique_bytes = byte_counts.iter().filter(|&&count| count > 0).count();
        let min_unique_ratio = 0.1; // At least 10% of possible byte values should be present
        let min_unique_bytes = (256.0 * min_unique_ratio) as usize;

        unique_bytes >= min_unique_bytes
    }

    /// Check if encrypted data appears to be properly encrypted
    /// 
    /// This is a heuristic check that looks for characteristics of properly encrypted data
    pub fn appears_properly_encrypted(&self, encrypted_data: &EncryptedData) -> bool {
        let data_bytes = encrypted_data.as_bytes();

        // Must have reasonable size
        if data_bytes.len() < self.min_size || data_bytes.len() > self.max_size {
            return false;
        }

        // Must not be all zeros or all ones
        if data_bytes.iter().all(|&b| b == 0) || data_bytes.iter().all(|&b| b == 0xFF) {
            return false;
        }

        // Must have sufficient entropy
        if !self.has_sufficient_entropy(data_bytes) {
            return false;
        }

        // Must not have obvious patterns
        if self.has_repeated_pattern(data_bytes) {
            return false;
        }

        true
    }

    /// Check if data has suspicious repeated patterns
    fn has_repeated_pattern(&self, data: &[u8]) -> bool {
        if data.len() < 8 {
            return false; // Too short to have meaningful patterns
        }

        // Check for simple repeated bytes
        let first_byte = data[0];
        if data.iter().take(8).all(|&b| b == first_byte) {
            return true;
        }

        // Check for alternating patterns
        if data.len() >= 4 {
            let pattern = [data[0], data[1]];
            if data.chunks(2).take(4).all(|chunk| {
                chunk.len() == 2 && chunk[0] == pattern[0] && chunk[1] == pattern[1]
            }) {
                return true;
            }
        }

        false
    }
}

/// Validation result for encrypted data integrity checks
#[derive(Debug, Clone)]
pub struct EncryptedDataValidationResult {
    /// Whether the encrypted data is valid
    pub is_valid: bool,
    /// Specific validation errors
    pub errors: Vec<String>,
    /// Whether the data appears to be properly encrypted
    pub appears_properly_encrypted: bool,
    /// Data size in bytes
    pub size: usize,
}

impl EncryptedDataValidationResult {
    /// Create a new validation result
    pub fn new(
        is_valid: bool,
        errors: Vec<String>,
        appears_properly_encrypted: bool,
        size: usize,
    ) -> Self {
        Self {
            is_valid,
            errors,
            appears_properly_encrypted,
            size,
        }
    }

    /// Create a successful validation result
    pub fn success(size: usize) -> Self {
        Self {
            is_valid: true,
            errors: Vec::new(),
            appears_properly_encrypted: true,
            size,
        }
    }

    /// Create a failed validation result
    pub fn failure(errors: Vec<String>, size: usize) -> Self {
        Self {
            is_valid: false,
            errors,
            appears_properly_encrypted: false,
            size,
        }
    }
}

/// Comprehensive encrypted data validation with detailed results
pub fn validate_encrypted_data_comprehensive(
    encrypted_data: &EncryptedData,
) -> EncryptedDataValidationResult {
    let validator = LightweightEncryptedDataValidator::default();
    let data_bytes = encrypted_data.as_bytes();
    let mut errors = Vec::new();

    // Perform all validation checks
    if let Err(e) = validator.validate_integrity(encrypted_data) {
        errors.push(e.to_string());
    }

    let appears_properly_encrypted = validator.appears_properly_encrypted(encrypted_data);
    let is_valid = errors.is_empty();

    EncryptedDataValidationResult::new(
        is_valid,
        errors,
        appears_properly_encrypted,
        data_bytes.len(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_structures::encrypted_data::EncryptedData;

    #[test]
    fn test_validator_creation() {
        let validator = LightweightEncryptedDataValidator::new(32, 1024);
        assert_eq!(validator.min_size(), 32);
        assert_eq!(validator.max_size(), 1024);
    }

    #[test]
    fn test_validator_default() {
        let validator = LightweightEncryptedDataValidator::default();
        assert_eq!(validator.min_size(), 64);
        assert_eq!(validator.max_size(), 1024);
    }

    #[test]
    fn test_validate_integrity_valid_data() {
        let validator = LightweightEncryptedDataValidator::default();
        let valid_data = EncryptedData::from_hex(
            "a1b2c3d4e5f60718293a4b5c6d7e8f90123456789abcdef0fedcba9876543210aabbccddeeff00112233445566778899ffeeddccbbaa99887766554433221100cafebabedeadbeef0011223344556677"
        ).unwrap();
        assert!(validator.validate_integrity(&valid_data).is_ok());
    }

    #[test]
    fn test_validate_integrity_too_small() {
        let validator = LightweightEncryptedDataValidator::new(64, 1024);
        let small_data = EncryptedData::from_hex("0123456789abcdef").unwrap();

        let result = validator.validate_integrity(&small_data);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too small"));
    }

    #[test]
    fn test_validate_integrity_too_large() {
        let validator = LightweightEncryptedDataValidator::new(32, 64);
        let large_data = EncryptedData::from_hex(
            "a1b2c3d4e5f60718293a4b5c6d7e8f90123456789abcdef0fedcba9876543210aabbccddeeff00112233445566778899ffeeddccbbaa99887766554433221100cafebabedeadbeef0011223344556677aabbccddeeff00112233445566778899"
        ).unwrap();
        let result = validator.validate_integrity(&large_data);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too large"));
    }

    #[test]
    fn test_validate_integrity_all_zeros() {
        let validator = LightweightEncryptedDataValidator::default();
        let zero_data = EncryptedData::from_hex("00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").unwrap();

        let result = validator.validate_integrity(&zero_data);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("only zeros"));
    }

    #[test]
    fn test_validate_integrity_all_ones() {
        let validator = LightweightEncryptedDataValidator::default();
        let ones_data = EncryptedData::from_hex("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff").unwrap();

        let result = validator.validate_integrity(&ones_data);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("only ones"));
    }

    #[test]
    fn test_validate_integrity_repeated_pattern() {
        let validator = LightweightEncryptedDataValidator::default();
        let pattern_data = EncryptedData::from_hex("01010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101").unwrap();

        let result = validator.validate_integrity(&pattern_data);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("repeating patterns"));
    }

    #[test]
    fn test_validate_batch_success() {
        let validator = LightweightEncryptedDataValidator::default();
        let valid_data1 = EncryptedData::from_hex(
            "a1b2c3d4e5f60718293a4b5c6d7e8f90123456789abcdef0fedcba9876543210aabbccddeeff00112233445566778899ffeeddccbbaa99887766554433221100cafebabedeadbeef0011223344556677"
        ).unwrap();
        let valid_data2 = EncryptedData::from_hex(
            "b2c3d4e5f60718293a4b5c6d7e8f90123456789abcdef0fedcba9876543210aabbccddeeff00112233445566778899ffeeddccbbaa99887766554433221100cafebabedeadbeef0011223344556677"
        ).unwrap();
        let batch = vec![valid_data1, valid_data2];
        assert!(validator.validate_batch(&batch).is_ok());
    }

    #[test]
    fn test_validate_batch_failure() {
        let validator = LightweightEncryptedDataValidator::default();
        let valid_data = EncryptedData::from_hex(
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        ).unwrap();
        let invalid_data = EncryptedData::from_hex("00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").unwrap();

        let batch = vec![valid_data, invalid_data];
        let result = validator.validate_batch(&batch);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Batch validation failed"));
    }

    #[test]
    fn test_appears_properly_encrypted() {
        let validator = LightweightEncryptedDataValidator::default();
        let valid_data = EncryptedData::from_hex(
            "a1b2c3d4e5f60718293a4b5c6d7e8f90123456789abcdef0fedcba9876543210aabbccddeeff00112233445566778899ffeeddccbbaa99887766554433221100cafebabedeadbeef0011223344556677"
        ).unwrap();
        assert!(validator.appears_properly_encrypted(&valid_data));
        let invalid_data = EncryptedData::from_hex("00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").unwrap();
        assert!(!validator.appears_properly_encrypted(&invalid_data));
    }

    #[test]
    fn test_comprehensive_validation() {
        let valid_data = EncryptedData::from_hex(
            "a1b2c3d4e5f60718293a4b5c6d7e8f90123456789abcdef0fedcba9876543210aabbccddeeff00112233445566778899ffeeddccbbaa99887766554433221100cafebabedeadbeef0011223344556677"
        ).unwrap();
        let result = validate_encrypted_data_comprehensive(&valid_data);
        assert!(result.is_valid);
        assert!(result.appears_properly_encrypted);
        assert_eq!(result.size, 80);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_comprehensive_validation_failure() {
        let invalid_data = EncryptedData::from_hex("00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").unwrap();

        let result = validate_encrypted_data_comprehensive(&invalid_data);
        assert!(!result.is_valid);
        assert!(!result.appears_properly_encrypted);
        assert_eq!(result.size, 64);
        assert!(!result.errors.is_empty());
    }
} 