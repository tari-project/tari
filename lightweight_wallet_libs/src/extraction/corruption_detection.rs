// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Corruption detection and error handling for lightweight wallet extraction
//!
//! This module provides functionality to detect and handle corrupted or invalid
//! data during the UTXO extraction process.

use crate::{
    data_structures::{
        encrypted_data::EncryptedData,
        payment_id::PaymentId,
        transaction_output::LightweightTransactionOutput,
        wallet_output::LightweightWalletOutput,
        types::{CompressedCommitment, MicroMinotari},
    },
    errors::{LightweightWalletError, DataStructureError},
};

/// Result of corruption detection
#[derive(Debug, Clone, PartialEq)]
pub struct CorruptionDetectionResult {
    /// Whether corruption was detected
    pub corruption_detected: bool,
    /// The type of corruption detected
    pub corruption_type: Option<CorruptionType>,
    /// Detailed error message
    pub error_message: Option<String>,
    /// Confidence level of the detection (0.0 to 1.0)
    pub confidence: f64,
    /// Whether the data can be recovered
    pub recoverable: bool,
}

impl CorruptionDetectionResult {
    /// Create a clean result (no corruption detected)
    pub fn clean() -> Self {
        Self {
            corruption_detected: false,
            corruption_type: None,
            error_message: None,
            confidence: 1.0,
            recoverable: true,
        }
    }

    /// Create a corruption result
    pub fn corrupted(
        corruption_type: CorruptionType,
        error_message: String,
        confidence: f64,
        recoverable: bool,
    ) -> Self {
        Self {
            corruption_detected: true,
            corruption_type: Some(corruption_type),
            error_message: Some(error_message),
            confidence,
            recoverable,
        }
    }

    /// Check if corruption was detected
    pub fn is_corrupted(&self) -> bool {
        self.corruption_detected
    }

    /// Get the corruption type if detected
    pub fn corruption_type(&self) -> Option<&CorruptionType> {
        self.corruption_type.as_ref()
    }

    /// Get the error message if corruption was detected
    pub fn error_message(&self) -> Option<&str> {
        self.error_message.as_deref()
    }

    /// Check if the data is recoverable
    pub fn is_recoverable(&self) -> bool {
        self.recoverable
    }
}

/// Types of corruption that can be detected
#[derive(Debug, Clone, PartialEq)]
pub enum CorruptionType {
    /// Encrypted data corruption
    EncryptedDataCorruption,
    /// Commitment corruption
    CommitmentCorruption,
    /// Range proof corruption
    RangeProofCorruption,
    /// Signature corruption
    SignatureCorruption,
    /// Script corruption
    ScriptCorruption,
    /// Covenant corruption
    CovenantCorruption,
    /// Payment ID corruption
    PaymentIdCorruption,
    /// Value corruption
    ValueCorruption,
    /// Features corruption
    FeaturesCorruption,
    /// Metadata corruption
    MetadataCorruption,
    /// Structural corruption (malformed data)
    StructuralCorruption,
    /// Checksum mismatch
    ChecksumMismatch,
    /// Version mismatch
    VersionMismatch,
    /// Size corruption
    SizeCorruption,
    /// Format corruption
    FormatCorruption,
    /// Empty data
    EmptyData,
    /// Insufficient data
    InsufficientData,
    /// Zero data
    ZeroData,
}

/// Corruption detection manager
pub struct CorruptionDetector {
    /// Whether to perform deep validation
    deep_validation: bool,
    /// Whether to attempt recovery
    attempt_recovery: bool,
    /// Minimum confidence threshold for corruption detection
    confidence_threshold: f64,
    /// Whether to validate checksums
    validate_checksums: bool,
    /// Whether to validate structural integrity
    validate_structure: bool,
}

impl Default for CorruptionDetector {
    fn default() -> Self {
        Self {
            deep_validation: true,
            attempt_recovery: false,
            confidence_threshold: 0.8,
            validate_checksums: true,
            validate_structure: true,
        }
    }
}

impl CorruptionDetector {
    /// Create a new corruption detector with default settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new corruption detector with custom settings
    pub fn with_settings(
        deep_validation: bool,
        attempt_recovery: bool,
        confidence_threshold: f64,
        validate_checksums: bool,
        validate_structure: bool,
    ) -> Self {
        Self {
            deep_validation,
            attempt_recovery,
            confidence_threshold,
            validate_checksums,
            validate_structure,
        }
    }

    /// Detect corruption in encrypted data
    pub fn detect_encrypted_data_corruption(
        &self,
        encrypted_data: &EncryptedData,
    ) -> CorruptionDetectionResult {
        if encrypted_data.as_bytes().is_empty() {
            return CorruptionDetectionResult::corrupted(
                CorruptionType::EmptyData,
                "Encrypted data is empty".to_string(),
                0.8,
                false,
            );
        }

        // Check for insufficient data
        if encrypted_data.as_bytes().len() < 32 {
            return CorruptionDetectionResult::corrupted(
                CorruptionType::InsufficientData,
                format!("Encrypted data too small: {} bytes", encrypted_data.as_bytes().len()),
                0.7,
                false,
            );
        }

        // Check for all-zero data
        if encrypted_data.as_bytes().iter().all(|&b| b == 0) {
            return CorruptionDetectionResult::corrupted(
                CorruptionType::ZeroData,
                "Encrypted data contains only zeros".to_string(),
                0.9,
                false,
            );
        }

        // Check for suspicious patterns
        if self.detect_suspicious_patterns(encrypted_data.as_bytes()) {
            return CorruptionDetectionResult::corrupted(
                CorruptionType::EncryptedDataCorruption,
                "Encrypted data contains suspicious patterns".to_string(),
                0.7,
                true,
            );
        }

        CorruptionDetectionResult::clean()
    }

    /// Detect corruption in transaction output
    pub fn detect_transaction_output_corruption(
        &self,
        transaction_output: &LightweightTransactionOutput,
    ) -> CorruptionDetectionResult {
        // Check encrypted data corruption
        let encrypted_data_result = self.detect_encrypted_data_corruption(transaction_output.encrypted_data());
        if encrypted_data_result.is_corrupted() {
            return encrypted_data_result;
        }

        // Check commitment corruption
        let commitment_result = self.detect_commitment_corruption(transaction_output.commitment());
        if commitment_result.is_corrupted() {
            return commitment_result;
        }

        // Check range proof corruption
        if let Some(proof) = transaction_output.proof() {
            let proof_result = self.detect_range_proof_corruption(proof);
            if proof_result.is_corrupted() {
                return proof_result;
            }
        }

        // Check signature corruption
        let signature_result = self.detect_signature_corruption(transaction_output.metadata_signature());
        if signature_result.is_corrupted() {
            return signature_result;
        }

        // Check script corruption
        let script_result = self.detect_script_corruption(transaction_output.script());
        if script_result.is_corrupted() {
            return script_result;
        }

        // Check covenant corruption
        let covenant_result = self.detect_covenant_corruption(transaction_output.covenant());
        if covenant_result.is_corrupted() {
            return covenant_result;
        }

        // Check value corruption
        let value_result = self.detect_value_corruption(&transaction_output.minimum_value_promise());
        if value_result.is_corrupted() {
            return value_result;
        }

        // Check features corruption
        let features_result = self.detect_features_corruption(transaction_output.features());
        if features_result.is_corrupted() {
            return features_result;
        }

        CorruptionDetectionResult::clean()
    }

    /// Detect corruption in wallet output
    pub fn detect_wallet_output_corruption(
        &self,
        wallet_output: &LightweightWalletOutput,
    ) -> CorruptionDetectionResult {
        // Check encrypted data corruption
        let encrypted_data_result = self.detect_encrypted_data_corruption(wallet_output.encrypted_data());
        if encrypted_data_result.is_corrupted() {
            return encrypted_data_result;
        }

        // Check payment ID corruption
        let payment_id_result = self.detect_payment_id_corruption(wallet_output.payment_id());
        if payment_id_result.is_corrupted() {
            return payment_id_result;
        }

        // Check value corruption
        let value_result = self.detect_value_corruption(&wallet_output.value());
        if value_result.is_corrupted() {
            return value_result;
        }

        // Check range proof corruption
        if let Some(proof) = wallet_output.range_proof() {
            let proof_result = self.detect_range_proof_corruption(proof);
            if proof_result.is_corrupted() {
                return proof_result;
            }
        }

        CorruptionDetectionResult::clean()
    }

    /// Detect corruption in commitment
    fn detect_commitment_corruption(&self, commitment: &CompressedCommitment) -> CorruptionDetectionResult {
        // Check if commitment is all zeros
        if commitment.as_bytes().iter().all(|&b| b == 0) {
            return CorruptionDetectionResult::corrupted(
                CorruptionType::CommitmentCorruption,
                "Commitment is all zeros".to_string(),
                0.95,
                false,
            );
        }

        // Check if commitment has wrong length
        if commitment.as_bytes().len() != 33 {
            return CorruptionDetectionResult::corrupted(
                CorruptionType::CommitmentCorruption,
                "Commitment has wrong length".to_string(),
                1.0,
                false,
            );
        }

        CorruptionDetectionResult::clean()
    }

    /// Detect corruption in range proof
    fn detect_range_proof_corruption(&self, proof: &crate::data_structures::wallet_output::LightweightRangeProof) -> CorruptionDetectionResult {
        // Check if range proof is empty
        if proof.bytes.is_empty() {
            return CorruptionDetectionResult::corrupted(
                CorruptionType::RangeProofCorruption,
                "Range proof is empty".to_string(),
                0.8,
                true,
            );
        }

        // Check if range proof is too small
        if proof.bytes.len() < 64 {
            return CorruptionDetectionResult::corrupted(
                CorruptionType::RangeProofCorruption,
                "Range proof is too small".to_string(),
                0.7,
                true,
            );
        }

        // Check if range proof is all zeros
        if proof.bytes.iter().all(|&b| b == 0) {
            return CorruptionDetectionResult::corrupted(
                CorruptionType::RangeProofCorruption,
                "Range proof is all zeros".to_string(),
                0.9,
                false,
            );
        }

        CorruptionDetectionResult::clean()
    }

    /// Detect corruption in signature
    fn detect_signature_corruption(&self, signature: &crate::data_structures::wallet_output::LightweightSignature) -> CorruptionDetectionResult {
        // Check if signature is empty
        if signature.bytes.is_empty() {
            return CorruptionDetectionResult::corrupted(
                CorruptionType::SignatureCorruption,
                "Signature is empty".to_string(),
                0.8,
                true,
            );
        }

        // Check if signature is all zeros
        if signature.bytes.iter().all(|&b| b == 0) {
            return CorruptionDetectionResult::corrupted(
                CorruptionType::SignatureCorruption,
                "Signature is all zeros".to_string(),
                0.9,
                false,
            );
        }

        CorruptionDetectionResult::clean()
    }

    /// Detect corruption in script
    fn detect_script_corruption(&self, script: &crate::data_structures::wallet_output::LightweightScript) -> CorruptionDetectionResult {
        // Script can be empty, so no corruption detection needed
        CorruptionDetectionResult::clean()
    }

    /// Detect corruption in covenant
    fn detect_covenant_corruption(&self, covenant: &crate::data_structures::wallet_output::LightweightCovenant) -> CorruptionDetectionResult {
        // Covenant can be empty, so no corruption detection needed
        CorruptionDetectionResult::clean()
    }

    /// Detect corruption in payment ID
    fn detect_payment_id_corruption(&self, payment_id: &PaymentId) -> CorruptionDetectionResult {
        match payment_id {
            PaymentId::Empty => {
                // Empty payment ID is always valid
                CorruptionDetectionResult::clean()
            }
            PaymentId::U256 { value } => {
                // Check if U256 value is zero
                if value.is_zero() {
                    return CorruptionDetectionResult::corrupted(
                        CorruptionType::PaymentIdCorruption,
                        "U256 payment ID value is zero".to_string(),
                        0.8,
                        true,
                    );
                }
                CorruptionDetectionResult::clean()
            }
            PaymentId::Open { data } => {
                // Check if open data is empty
                if data.is_empty() {
                    return CorruptionDetectionResult::corrupted(
                        CorruptionType::PaymentIdCorruption,
                        "Open payment ID data is empty".to_string(),
                        0.8,
                        true,
                    );
                }
                CorruptionDetectionResult::clean()
            }
            PaymentId::AddressAndData { address, data } => {
                // Check if address is empty
                if address.is_empty() {
                    return CorruptionDetectionResult::corrupted(
                        CorruptionType::PaymentIdCorruption,
                        "AddressAndData payment ID address is empty".to_string(),
                        0.8,
                        true,
                    );
                }
                // Check if data is empty
                if data.is_empty() {
                    return CorruptionDetectionResult::corrupted(
                        CorruptionType::PaymentIdCorruption,
                        "AddressAndData payment ID data is empty".to_string(),
                        0.8,
                        true,
                    );
                }
                CorruptionDetectionResult::clean()
            }
            PaymentId::TransactionInfo { tx_id, output_index: _ } => {
                // Check if transaction ID is empty
                if tx_id.is_empty() {
                    return CorruptionDetectionResult::corrupted(
                        CorruptionType::PaymentIdCorruption,
                        "TransactionInfo payment ID transaction ID is empty".to_string(),
                        0.8,
                        true,
                    );
                }
                CorruptionDetectionResult::clean()
            }
            PaymentId::Raw { data } => {
                // Check if raw data is empty
                if data.is_empty() {
                    return CorruptionDetectionResult::corrupted(
                        CorruptionType::PaymentIdCorruption,
                        "Raw payment ID data is empty".to_string(),
                        0.8,
                        true,
                    );
                }
                CorruptionDetectionResult::clean()
            }
        }
    }

    /// Detect corruption in value
    fn detect_value_corruption(&self, value: &MicroMinotari) -> CorruptionDetectionResult {
        // Check if value is unreasonably large (more than 1 billion Tari)
        if value.as_u64() > 1_000_000_000_000_000 {
            return CorruptionDetectionResult::corrupted(
                CorruptionType::ValueCorruption,
                "Value is unreasonably large".to_string(),
                0.8,
                true,
            );
        }

        CorruptionDetectionResult::clean()
    }

    /// Detect corruption in features
    fn detect_features_corruption(&self, features: &crate::data_structures::wallet_output::LightweightOutputFeatures) -> CorruptionDetectionResult {
        // Check if maturity is unreasonably large (more than 1 million blocks)
        if features.maturity > 1_000_000 {
            return CorruptionDetectionResult::corrupted(
                CorruptionType::FeaturesCorruption,
                "Maturity is unreasonably large".to_string(),
                0.7,
                true,
            );
        }

        CorruptionDetectionResult::clean()
    }

    /// Detect suspicious patterns in data
    fn detect_suspicious_patterns(&self, data: &[u8]) -> bool {
        // Check for repeated patterns
        if data.len() >= 8 {
            let mut pattern_count = 0;
            for i in 0..data.len() - 4 {
                if data[i] == data[i + 1] && data[i] == data[i + 2] && data[i] == data[i + 3] {
                    pattern_count += 1;
                }
            }
            if pattern_count > data.len() / 8 {
                return true;
            }
        }

        // Check for all same byte values
        if data.len() > 0 {
            let first_byte = data[0];
            if data.iter().all(|&b| b == first_byte) {
                return true;
            }
        }

        false
    }

    /// Attempt to recover corrupted data
    pub fn attempt_recovery(
        &self,
        corruption_result: &CorruptionDetectionResult,
        data: &[u8],
    ) -> Result<Vec<u8>, LightweightWalletError> {
        if !corruption_result.is_corrupted() {
            return Ok(data.to_vec());
        }

        match corruption_result.corruption_type() {
            Some(CorruptionType::EmptyData) => {
                return Err(DataStructureError::InvalidDataFormat(
                    "Cannot recover from empty data".to_string()
                ).into());
            }
            Some(CorruptionType::InsufficientData) => {
                return Err(DataStructureError::InvalidDataFormat(
                    "Cannot recover from insufficient data".to_string()
                ).into());
            }
            Some(CorruptionType::ZeroData) => {
                // Try to find non-zero data in surrounding context
                // This is a placeholder - in practice, you'd need more context
                return Err(DataStructureError::InvalidDataFormat(
                    "Cannot recover from zero data without additional context".to_string()
                ).into());
            }
            _ => {
                // For other corruption types, return original data
                // In a real implementation, you might try various recovery strategies
                Ok(data.to_vec())
            }
        }
    }

    /// Check if recovery is possible for a given corruption type
    pub fn can_recover(&self) -> bool {
        // This is a simplified implementation
        // In practice, recovery possibility would depend on the specific corruption type
        false
    }

    /// Set whether to perform deep validation
    pub fn set_deep_validation(&mut self, deep_validation: bool) {
        self.deep_validation = deep_validation;
    }

    /// Set whether to attempt recovery
    pub fn set_attempt_recovery(&mut self, attempt_recovery: bool) {
        self.attempt_recovery = attempt_recovery;
    }

    /// Set the confidence threshold
    pub fn set_confidence_threshold(&mut self, confidence_threshold: f64) {
        self.confidence_threshold = confidence_threshold.max(0.0).min(1.0);
    }

    /// Set whether to validate checksums
    pub fn set_validate_checksums(&mut self, validate_checksums: bool) {
        self.validate_checksums = validate_checksums;
    }

    /// Set whether to validate structural integrity
    pub fn set_validate_structure(&mut self, validate_structure: bool) {
        self.validate_structure = validate_structure;
    }

    /// Get whether deep validation is enabled
    pub fn deep_validation(&self) -> bool {
        self.deep_validation
    }

    /// Get whether recovery is attempted
    pub fn recovery_enabled(&self) -> bool {
        self.attempt_recovery
    }

    /// Get the confidence threshold
    pub fn confidence_threshold(&self) -> f64 {
        self.confidence_threshold
    }

    /// Get whether checksums are validated
    pub fn validate_checksums(&self) -> bool {
        self.validate_checksums
    }

    /// Get whether structural integrity is validated
    pub fn validate_structure(&self) -> bool {
        self.validate_structure
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_structures::{
        encrypted_data::EncryptedData,
        payment_id::PaymentId,
        types::{CompressedCommitment, MicroMinotari, PrivateKey},
    };

    #[test]
    fn test_corruption_detector_creation() {
        let detector = CorruptionDetector::new();
        assert!(detector.deep_validation());
        assert!(!detector.recovery_enabled());
        assert_eq!(detector.confidence_threshold(), 0.8);
        assert!(detector.validate_checksums());
        assert!(detector.validate_structure());
    }

    #[test]
    fn test_detect_encrypted_data_corruption_empty() {
        let encrypted_data = EncryptedData::from_bytes(&vec![]).unwrap();
        let detector = CorruptionDetector::new();
        let result = detector.detect_encrypted_data_corruption(&encrypted_data);
        
        assert!(result.is_corrupted());
        assert_eq!(result.corruption_type(), Some(&CorruptionType::EmptyData));
        assert_eq!(result.error_message(), Some("Encrypted data is empty"));
        assert!(!result.is_recoverable());
    }

    #[test]
    fn test_detect_encrypted_data_corruption_all_zeros() {
        let encrypted_data = EncryptedData::from_bytes(&vec![0u8; 64]).unwrap();
        let detector = CorruptionDetector::new();
        let result = detector.detect_encrypted_data_corruption(&encrypted_data);
        
        assert!(result.is_corrupted());
        assert_eq!(result.corruption_type(), Some(&CorruptionType::ZeroData));
        assert_eq!(result.error_message(), Some("Encrypted data contains only zeros"));
        assert!(!result.is_recoverable());
    }

    #[test]
    fn test_detect_encrypted_data_corruption_suspicious_patterns() {
        let mut test_data = vec![1u8, 2u8, 3u8, 4u8, 5u8];
        for _ in 0..15 {
            test_data.extend_from_slice(&[1u8, 2u8, 3u8, 4u8, 5u8]);
        }
        let encrypted_data = EncryptedData::from_bytes(&test_data).unwrap();
        let detector = CorruptionDetector::new();
        let result = detector.detect_encrypted_data_corruption(&encrypted_data);
        
        // This should not be corrupted since it's not a suspicious pattern
        assert!(!result.is_corrupted());
    }

    #[test]
    fn test_detect_payment_id_corruption_empty_open() {
        let detector = CorruptionDetector::new();
        let payment_id = PaymentId::Open { data: vec![] };
        let result = detector.detect_payment_id_corruption(&payment_id);
        
        assert!(result.is_corrupted());
        assert_eq!(result.corruption_type(), Some(&CorruptionType::PaymentIdCorruption));
        assert_eq!(result.error_message(), Some("Open payment ID data is empty"));
        assert!(result.is_recoverable());
    }

    #[test]
    fn test_detect_payment_id_corruption_clean() {
        let detector = CorruptionDetector::new();
        let payment_id = PaymentId::Open { data: b"test_data".to_vec() };
        let result = detector.detect_payment_id_corruption(&payment_id);
        
        assert!(!result.is_corrupted());
        assert!(result.corruption_type().is_none());
        assert!(result.error_message().is_none());
        assert!(result.is_recoverable());
    }

    #[test]
    fn test_detect_commitment_corruption_all_zeros() {
        let detector = CorruptionDetector::new();
        let commitment = CompressedCommitment::new([0u8; 33]);
        let result = detector.detect_commitment_corruption(&commitment);
        
        assert!(result.is_corrupted());
        assert_eq!(result.corruption_type(), Some(&CorruptionType::CommitmentCorruption));
        assert_eq!(result.error_message(), Some("Commitment is all zeros"));
        assert!(!result.is_recoverable());
    }

    #[test]
    fn test_detect_value_corruption_large() {
        let detector = CorruptionDetector::new();
        let value = MicroMinotari::new(2_000_000_000_000_000); // 2 billion Tari
        let result = detector.detect_value_corruption(&value);
        
        assert!(result.is_corrupted());
        assert_eq!(result.corruption_type(), Some(&CorruptionType::ValueCorruption));
        assert_eq!(result.error_message(), Some("Value is unreasonably large"));
        assert!(result.is_recoverable());
    }

    #[test]
    fn test_detect_suspicious_patterns() {
        let detector = CorruptionDetector::new();
        
        // Test repeated pattern
        let data = vec![1u8, 1u8, 1u8, 1u8, 2u8, 2u8, 2u8, 2u8, 3u8, 3u8, 3u8, 3u8];
        assert!(detector.detect_suspicious_patterns(&data));
        
        // Test all same bytes
        let data = vec![5u8; 10];
        assert!(detector.detect_suspicious_patterns(&data));
        
        // Test normal data
        let data = vec![1u8, 2u8, 3u8, 4u8, 5u8, 6u8, 7u8, 8u8];
        assert!(!detector.detect_suspicious_patterns(&data));
    }

    #[test]
    fn test_attempt_recovery() {
        let detector = CorruptionDetector::new();
        let corruption_result = CorruptionDetectionResult::corrupted(
            CorruptionType::EmptyData,
            "Test corruption".to_string(),
            0.8,
            false,
        );
        
        let result = detector.attempt_recovery(&corruption_result, &[1, 2, 3, 4]);
        assert!(result.is_err());
    }
} 