// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Special output handling for lightweight wallets
//!
//! This module provides functionality to handle coinbase and burn outputs
//! appropriately during the UTXO extraction process.

use crate::{
    data_structures::{
        transaction_output::LightweightTransactionOutput,
        wallet_output::LightweightWalletOutput,
    },
    extraction::{
        range_proof_extraction::RangeProofExtractor,
    },
};

/// Result of special output handling
#[derive(Debug, Clone, PartialEq)]
pub struct SpecialOutputHandlingResult {
    /// Whether the handling was successful
    pub success: bool,
    /// The type of special output that was handled
    pub output_type: SpecialOutputType,
    /// The reconstructed wallet output (if applicable)
    pub wallet_output: Option<LightweightWalletOutput>,
    /// Error message if handling failed
    pub error: Option<String>,
}

impl SpecialOutputHandlingResult {
    /// Create a successful result
    pub fn success(output_type: SpecialOutputType, wallet_output: Option<LightweightWalletOutput>) -> Self {
        Self {
            success: true,
            output_type,
            wallet_output,
            error: None,
        }
    }

    /// Create a failure result
    pub fn failure(output_type: SpecialOutputType, error: String) -> Self {
        Self {
            success: false,
            output_type,
            wallet_output: None,
            error: Some(error),
        }
    }

    /// Check if the result indicates success
    pub fn is_success(&self) -> bool {
        self.success
    }

    /// Get the error message if any
    pub fn error_message(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

/// Types of special outputs that can be handled
#[derive(Debug, Clone, PartialEq)]
pub enum SpecialOutputType {
    /// Regular payment output (not special)
    Payment,
    /// Coinbase output (mining reward)
    Coinbase,
    /// Burn output (destroyed coins)
    Burn,
    /// Validator node registration output
    ValidatorNodeRegistration,
    /// Code template registration output
    CodeTemplateRegistration,
}

/// Special output handling manager
pub struct SpecialOutputHandler {
    /// Whether to attempt decryption for special outputs
    attempt_decryption: bool,
    /// Whether to extract payment IDs for special outputs
    extract_payment_id: bool,
    /// Whether to validate range proofs for special outputs
    validate_range_proofs: bool,
    /// Whether to reconstruct wallet outputs for special outputs
    reconstruct_wallet_outputs: bool,
}

impl Default for SpecialOutputHandler {
    fn default() -> Self {
        Self {
            attempt_decryption: true,
            extract_payment_id: true,
            validate_range_proofs: true,
            reconstruct_wallet_outputs: true,
        }
    }
}

impl SpecialOutputHandler {
    /// Create a new special output handler with default settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new special output handler with custom settings
    pub fn with_settings(
        attempt_decryption: bool,
        extract_payment_id: bool,
        validate_range_proofs: bool,
        reconstruct_wallet_outputs: bool,
    ) -> Self {
        Self {
            attempt_decryption,
            extract_payment_id,
            validate_range_proofs,
            reconstruct_wallet_outputs,
        }
    }

    /// Handle a transaction output appropriately based on its type
    pub fn handle_transaction_output(
        &self,
        transaction_output: &LightweightTransactionOutput,
        current_block_height: u64,
    ) -> SpecialOutputHandlingResult {
        let output_type = self.determine_output_type(transaction_output);

        match output_type {
            SpecialOutputType::Payment => {
                // Regular payment outputs are handled by the main extraction pipeline
                SpecialOutputHandlingResult::success(SpecialOutputType::Payment, None)
            }
            SpecialOutputType::Coinbase => {
                self.handle_coinbase_output(transaction_output, current_block_height)
            }
            SpecialOutputType::Burn => {
                self.handle_burn_output(transaction_output)
            }
            SpecialOutputType::ValidatorNodeRegistration => {
                self.handle_validator_node_registration_output(transaction_output)
            }
            SpecialOutputType::CodeTemplateRegistration => {
                self.handle_code_template_registration_output(transaction_output)
            }
        }
    }

    /// Handle a wallet output appropriately based on its type
    pub fn handle_wallet_output(
        &self,
        wallet_output: &LightweightWalletOutput,
        current_block_height: u64,
    ) -> SpecialOutputHandlingResult {
        let output_type = self.determine_wallet_output_type(wallet_output);

        match output_type {
            SpecialOutputType::Payment => {
                // Regular payment outputs are handled by the main extraction pipeline
                SpecialOutputHandlingResult::success(SpecialOutputType::Payment, None)
            }
            SpecialOutputType::Coinbase => {
                self.handle_coinbase_wallet_output(wallet_output, current_block_height)
            }
            SpecialOutputType::Burn => {
                self.handle_burn_wallet_output(wallet_output)
            }
            SpecialOutputType::ValidatorNodeRegistration => {
                self.handle_validator_node_registration_wallet_output(wallet_output)
            }
            SpecialOutputType::CodeTemplateRegistration => {
                self.handle_code_template_registration_wallet_output(wallet_output)
            }
        }
    }

    /// Determine the output type from a transaction output
    fn determine_output_type(&self, transaction_output: &LightweightTransactionOutput) -> SpecialOutputType {
        match transaction_output.features().output_type {
            crate::data_structures::wallet_output::LightweightOutputType::Payment => {
                SpecialOutputType::Payment
            }
            crate::data_structures::wallet_output::LightweightOutputType::Coinbase => {
                SpecialOutputType::Coinbase
            }
            crate::data_structures::wallet_output::LightweightOutputType::Burn => {
                SpecialOutputType::Burn
            }
            crate::data_structures::wallet_output::LightweightOutputType::ValidatorNodeRegistration => {
                SpecialOutputType::ValidatorNodeRegistration
            }
            crate::data_structures::wallet_output::LightweightOutputType::CodeTemplateRegistration => {
                SpecialOutputType::CodeTemplateRegistration
            }
        }
    }

    /// Determine the output type from a wallet output
    fn determine_wallet_output_type(&self, wallet_output: &LightweightWalletOutput) -> SpecialOutputType {
        match wallet_output.output_type() {
            crate::data_structures::wallet_output::LightweightOutputType::Payment => {
                SpecialOutputType::Payment
            }
            crate::data_structures::wallet_output::LightweightOutputType::Coinbase => {
                SpecialOutputType::Coinbase
            }
            crate::data_structures::wallet_output::LightweightOutputType::Burn => {
                SpecialOutputType::Burn
            }
            crate::data_structures::wallet_output::LightweightOutputType::ValidatorNodeRegistration => {
                SpecialOutputType::ValidatorNodeRegistration
            }
            crate::data_structures::wallet_output::LightweightOutputType::CodeTemplateRegistration => {
                SpecialOutputType::CodeTemplateRegistration
            }
        }
    }

    /// Handle a coinbase output
    fn handle_coinbase_output(
        &self,
        transaction_output: &LightweightTransactionOutput,
        current_block_height: u64,
    ) -> SpecialOutputHandlingResult {
        // Check if the coinbase output is mature
        if !self.is_coinbase_mature(transaction_output, current_block_height) {
            return SpecialOutputHandlingResult::failure(
                SpecialOutputType::Coinbase,
                "Coinbase output is not yet mature".to_string(),
            );
        }

        // For coinbase outputs, we typically don't need to decrypt encrypted data
        // as the value is usually revealed in the minimum value promise
        if self.reconstruct_wallet_outputs {
            // Create a simplified wallet output for coinbase
            let wallet_output = self.create_coinbase_wallet_output(transaction_output);
            SpecialOutputHandlingResult::success(SpecialOutputType::Coinbase, Some(wallet_output))
        } else {
            SpecialOutputHandlingResult::success(SpecialOutputType::Coinbase, None)
        }
    }

    /// Handle a burn output
    fn handle_burn_output(&self, transaction_output: &LightweightTransactionOutput) -> SpecialOutputHandlingResult {
        // Burn outputs are typically not spendable, so we just acknowledge them
        // and potentially extract metadata for tracking purposes
        
        if self.extract_payment_id {
            // Try to extract payment ID for burn tracking
            // This would typically be done with a known key or public data
        }

        SpecialOutputHandlingResult::success(SpecialOutputType::Burn, None)
    }

    /// Handle a validator node registration output
    fn handle_validator_node_registration_output(
        &self,
        transaction_output: &LightweightTransactionOutput,
    ) -> SpecialOutputHandlingResult {
        // Validator node registration outputs have special validation requirements
        // They typically contain registration data in the script or covenant
        
        if self.validate_range_proofs {
            // Validate the range proof for the registration fee
            let range_proof_extractor = RangeProofExtractor::new();
            let range_proof_result = range_proof_extractor.extract_from_transaction_output(transaction_output);
            
            if !range_proof_result.is_success() {
                return SpecialOutputHandlingResult::failure(
                    SpecialOutputType::ValidatorNodeRegistration,
                    format!("Range proof validation failed: {}", 
                        range_proof_result.error_message().unwrap_or("Unknown error")),
                );
            }
        }

        SpecialOutputHandlingResult::success(SpecialOutputType::ValidatorNodeRegistration, None)
    }

    /// Handle a code template registration output
    fn handle_code_template_registration_output(
        &self,
        transaction_output: &LightweightTransactionOutput,
    ) -> SpecialOutputHandlingResult {
        // Code template registration outputs contain template data
        // They typically have special validation requirements
        
        if self.validate_range_proofs {
            // Validate the range proof for the registration fee
            let range_proof_extractor = RangeProofExtractor::new();
            let range_proof_result = range_proof_extractor.extract_from_transaction_output(transaction_output);
            
            if !range_proof_result.is_success() {
                return SpecialOutputHandlingResult::failure(
                    SpecialOutputType::CodeTemplateRegistration,
                    format!("Range proof validation failed: {}", 
                        range_proof_result.error_message().unwrap_or("Unknown error")),
                );
            }
        }

        SpecialOutputHandlingResult::success(SpecialOutputType::CodeTemplateRegistration, None)
    }

    /// Handle a coinbase wallet output
    fn handle_coinbase_wallet_output(
        &self,
        wallet_output: &LightweightWalletOutput,
        current_block_height: u64,
    ) -> SpecialOutputHandlingResult {
        // Check if the coinbase output is mature
        if !wallet_output.is_mature_at(current_block_height) {
            return SpecialOutputHandlingResult::failure(
                SpecialOutputType::Coinbase,
                "Coinbase output is not yet mature".to_string(),
            );
        }

        SpecialOutputHandlingResult::success(SpecialOutputType::Coinbase, Some(wallet_output.clone()))
    }

    /// Handle a burn wallet output
    fn handle_burn_wallet_output(&self, wallet_output: &LightweightWalletOutput) -> SpecialOutputHandlingResult {
        // Burn outputs are typically not spendable
        // We just acknowledge them for tracking purposes
        
        SpecialOutputHandlingResult::success(SpecialOutputType::Burn, Some(wallet_output.clone()))
    }

    /// Handle a validator node registration wallet output
    fn handle_validator_node_registration_wallet_output(
        &self,
        wallet_output: &LightweightWalletOutput,
    ) -> SpecialOutputHandlingResult {
        // Validator node registration outputs have special validation requirements
        
        if self.validate_range_proofs {
            // Validate the range proof for the registration fee
            let range_proof_extractor = RangeProofExtractor::new();
            let range_proof_result = range_proof_extractor.extract_from_wallet_output(wallet_output);
            
            if !range_proof_result.is_success() {
                return SpecialOutputHandlingResult::failure(
                    SpecialOutputType::ValidatorNodeRegistration,
                    format!("Range proof validation failed: {}", 
                        range_proof_result.error_message().unwrap_or("Unknown error")),
                );
            }
        }

        SpecialOutputHandlingResult::success(SpecialOutputType::ValidatorNodeRegistration, Some(wallet_output.clone()))
    }

    /// Handle a code template registration wallet output
    fn handle_code_template_registration_wallet_output(
        &self,
        wallet_output: &LightweightWalletOutput,
    ) -> SpecialOutputHandlingResult {
        // Code template registration outputs contain template data
        
        if self.validate_range_proofs {
            // Validate the range proof for the registration fee
            let range_proof_extractor = RangeProofExtractor::new();
            let range_proof_result = range_proof_extractor.extract_from_wallet_output(wallet_output);
            
            if !range_proof_result.is_success() {
                return SpecialOutputHandlingResult::failure(
                    SpecialOutputType::CodeTemplateRegistration,
                    format!("Range proof validation failed: {}", 
                        range_proof_result.error_message().unwrap_or("Unknown error")),
                );
            }
        }

        SpecialOutputHandlingResult::success(SpecialOutputType::CodeTemplateRegistration, Some(wallet_output.clone()))
    }

    /// Check if a coinbase output is mature at the given block height
    fn is_coinbase_mature(&self, transaction_output: &LightweightTransactionOutput, current_block_height: u64) -> bool {
        let maturity_height = transaction_output.features().maturity;
        current_block_height >= maturity_height
    }

    /// Create a coinbase wallet output from a transaction output
    fn create_coinbase_wallet_output(&self, transaction_output: &LightweightTransactionOutput) -> LightweightWalletOutput {
        // For coinbase outputs, we create a simplified wallet output
        // The value is typically revealed in the minimum value promise
        let value = transaction_output.minimum_value_promise();
        
        LightweightWalletOutput::new(
            transaction_output.version(),
            value,
            crate::data_structures::wallet_output::LightweightKeyId::Zero, // Coinbase outputs typically use zero key
            transaction_output.features().clone(),
            transaction_output.script().clone(),
            crate::data_structures::wallet_output::LightweightExecutionStack::default(),
            crate::data_structures::wallet_output::LightweightKeyId::Zero,
            transaction_output.sender_offset_public_key().clone(),
            transaction_output.metadata_signature().clone(),
            0, // No script lock for coinbase
            transaction_output.covenant().clone(),
            transaction_output.encrypted_data().clone(),
            value,
            transaction_output.proof().cloned(),
            crate::data_structures::payment_id::PaymentId::Empty, // Coinbase outputs typically have no payment ID
        )
    }

    /// Batch handle multiple transaction outputs
    pub fn handle_batch_transaction_outputs(
        &self,
        transaction_outputs: &[LightweightTransactionOutput],
        current_block_height: u64,
    ) -> Vec<SpecialOutputHandlingResult> {
        transaction_outputs
            .iter()
            .map(|output| self.handle_transaction_output(output, current_block_height))
            .collect()
    }

    /// Batch handle multiple wallet outputs
    pub fn handle_batch_wallet_outputs(
        &self,
        wallet_outputs: &[LightweightWalletOutput],
        current_block_height: u64,
    ) -> Vec<SpecialOutputHandlingResult> {
        wallet_outputs
            .iter()
            .map(|output| self.handle_wallet_output(output, current_block_height))
            .collect()
    }

    /// Set whether to attempt decryption for special outputs
    pub fn set_attempt_decryption(&mut self, attempt: bool) {
        self.attempt_decryption = attempt;
    }

    /// Set whether to extract payment IDs for special outputs
    pub fn set_extract_payment_id(&mut self, extract: bool) {
        self.extract_payment_id = extract;
    }

    /// Set whether to validate range proofs for special outputs
    pub fn set_validate_range_proofs(&mut self, validate: bool) {
        self.validate_range_proofs = validate;
    }

    /// Set whether to reconstruct wallet outputs for special outputs
    pub fn set_reconstruct_wallet_outputs(&mut self, reconstruct: bool) {
        self.reconstruct_wallet_outputs = reconstruct;
    }

    /// Get whether decryption is attempted for special outputs
    pub fn attempt_decryption(&self) -> bool {
        self.attempt_decryption
    }

    /// Get whether payment IDs are extracted for special outputs
    pub fn extract_payment_id(&self) -> bool {
        self.extract_payment_id
    }

    /// Get whether range proofs are validated for special outputs
    pub fn validate_range_proofs(&self) -> bool {
        self.validate_range_proofs
    }

    /// Get whether wallet outputs are reconstructed for special outputs
    pub fn reconstruct_wallet_outputs(&self) -> bool {
        self.reconstruct_wallet_outputs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_structures::{
        encrypted_data::EncryptedData,
        payment_id::PaymentId,
        types::{CompressedCommitment, MicroMinotari},
        wallet_output::{
            LightweightOutputFeatures, LightweightOutputType, LightweightRangeProof,
            LightweightScript, LightweightSignature, LightweightCovenant, LightweightExecutionStack,
        },
    };

    fn create_test_transaction_output(output_type: LightweightOutputType, maturity: u64) -> LightweightTransactionOutput {
        let mut features = LightweightOutputFeatures::default();
        features.output_type = output_type;
        features.maturity = maturity;

        LightweightTransactionOutput::new(
            1,
            features,
            CompressedCommitment::new([1u8; 33]),
            Some(LightweightRangeProof::default()),
            LightweightScript::default(),
            crate::data_structures::types::CompressedPublicKey::new([3u8; 32]),
            LightweightSignature::default(),
            LightweightCovenant::default(),
            EncryptedData::default(),
            MicroMinotari::new(1000),
        )
    }

    fn create_test_wallet_output(output_type: LightweightOutputType, maturity: u64) -> LightweightWalletOutput {
        let mut features = LightweightOutputFeatures::default();
        features.output_type = output_type;
        features.maturity = maturity;

        LightweightWalletOutput::new(
            1,
            MicroMinotari::new(1000),
            crate::data_structures::wallet_output::LightweightKeyId::String("test".to_string()),
            features,
            LightweightScript::default(),
            LightweightExecutionStack::default(),
            crate::data_structures::wallet_output::LightweightKeyId::String("test".to_string()),
            crate::data_structures::types::CompressedPublicKey::new([3u8; 32]),
            LightweightSignature::default(),
            0,
            LightweightCovenant::default(),
            EncryptedData::default(),
            MicroMinotari::new(1000),
            Some(LightweightRangeProof::default()),
            PaymentId::Empty,
        )
    }

    #[test]
    fn test_special_output_handler_creation() {
        let handler = SpecialOutputHandler::new();
        assert!(handler.attempt_decryption());
        assert!(handler.extract_payment_id());
        assert!(handler.validate_range_proofs());
        assert!(handler.reconstruct_wallet_outputs());
    }

    #[test]
    fn test_handle_payment_output() {
        let handler = SpecialOutputHandler::new();
        let output = create_test_transaction_output(LightweightOutputType::Payment, 0);
        let result = handler.handle_transaction_output(&output, 1000);

        assert!(result.is_success());
        assert_eq!(result.output_type, SpecialOutputType::Payment);
        assert!(result.wallet_output.is_none());
    }

    #[test]
    fn test_handle_coinbase_output_mature() {
        let handler = SpecialOutputHandler::new();
        let output = create_test_transaction_output(LightweightOutputType::Coinbase, 500);
        let result = handler.handle_transaction_output(&output, 1000);

        assert!(result.is_success());
        assert_eq!(result.output_type, SpecialOutputType::Coinbase);
        assert!(result.wallet_output.is_some());
    }

    #[test]
    fn test_handle_coinbase_output_immature() {
        let handler = SpecialOutputHandler::new();
        let output = create_test_transaction_output(LightweightOutputType::Coinbase, 1500);
        let result = handler.handle_transaction_output(&output, 1000);

        assert!(!result.is_success());
        assert_eq!(result.output_type, SpecialOutputType::Coinbase);
        assert_eq!(result.error_message(), Some("Coinbase output is not yet mature"));
    }

    #[test]
    fn test_handle_burn_output() {
        let handler = SpecialOutputHandler::new();
        let output = create_test_transaction_output(LightweightOutputType::Burn, 0);
        let result = handler.handle_transaction_output(&output, 1000);

        assert!(result.is_success());
        assert_eq!(result.output_type, SpecialOutputType::Burn);
        assert!(result.wallet_output.is_none());
    }

    #[test]
    fn test_handle_validator_node_registration_output() {
        let mut handler = SpecialOutputHandler::new();
        handler.set_validate_range_proofs(false); // Disable range proof validation for test
        let output = create_test_transaction_output(LightweightOutputType::ValidatorNodeRegistration, 0);
        let result = handler.handle_transaction_output(&output, 1000);

        assert!(result.is_success());
        assert_eq!(result.output_type, SpecialOutputType::ValidatorNodeRegistration);
        assert!(result.wallet_output.is_none());
    }

    #[test]
    fn test_handle_code_template_registration_output() {
        let mut handler = SpecialOutputHandler::new();
        handler.set_validate_range_proofs(false); // Disable range proof validation for test
        let output = create_test_transaction_output(LightweightOutputType::CodeTemplateRegistration, 0);
        let result = handler.handle_transaction_output(&output, 1000);

        assert!(result.is_success());
        assert_eq!(result.output_type, SpecialOutputType::CodeTemplateRegistration);
        assert!(result.wallet_output.is_none());
    }

    #[test]
    fn test_handle_wallet_output_coinbase_mature() {
        let handler = SpecialOutputHandler::new();
        let output = create_test_wallet_output(LightweightOutputType::Coinbase, 500);
        let result = handler.handle_wallet_output(&output, 1000);

        assert!(result.is_success());
        assert_eq!(result.output_type, SpecialOutputType::Coinbase);
        assert!(result.wallet_output.is_some());
    }

    #[test]
    fn test_handle_wallet_output_coinbase_immature() {
        let handler = SpecialOutputHandler::new();
        let output = create_test_wallet_output(LightweightOutputType::Coinbase, 1500);
        let result = handler.handle_wallet_output(&output, 1000);

        assert!(!result.is_success());
        assert_eq!(result.output_type, SpecialOutputType::Coinbase);
        assert_eq!(result.error_message(), Some("Coinbase output is not yet mature"));
    }

    #[test]
    fn test_batch_handling() {
        let handler = SpecialOutputHandler::new();
        let outputs = vec![
            create_test_transaction_output(LightweightOutputType::Payment, 0),
            create_test_transaction_output(LightweightOutputType::Coinbase, 500),
            create_test_transaction_output(LightweightOutputType::Burn, 0),
        ];

        let results = handler.handle_batch_transaction_outputs(&outputs, 1000);

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].output_type, SpecialOutputType::Payment);
        assert_eq!(results[1].output_type, SpecialOutputType::Coinbase);
        assert_eq!(results[2].output_type, SpecialOutputType::Burn);
        assert!(results[0].is_success());
        assert!(results[1].is_success());
        assert!(results[2].is_success());
    }
} 