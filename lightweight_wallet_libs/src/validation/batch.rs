use crate::data_structures::transaction_output::LightweightTransactionOutput;
use crate::errors::ValidationError;

/// Batch validation result containing validation status for multiple outputs
#[derive(Debug, Clone)]
pub struct BatchValidationResult {
    /// Overall validation success
    pub is_valid: bool,
    /// Individual validation results for each output
    pub results: Vec<OutputValidationResult>,
    /// Summary statistics
    pub summary: BatchValidationSummary,
}

/// Individual output validation result
#[derive(Debug, Clone)]
pub struct OutputValidationResult {
    /// Output index in the batch
    pub index: usize,
    /// Whether this specific output is valid
    pub is_valid: bool,
    /// Specific validation errors for this output
    pub errors: Vec<ValidationError>,
}

/// Summary statistics for batch validation
#[derive(Debug, Clone)]
pub struct BatchValidationSummary {
    /// Total number of outputs validated
    pub total_outputs: usize,
    /// Number of valid outputs
    pub valid_outputs: usize,
    /// Number of invalid outputs
    pub invalid_outputs: usize,
    /// Validation success rate as a percentage
    pub success_rate: f64,
}

impl BatchValidationSummary {
    /// Create a new summary from validation results
    pub fn new(results: &[OutputValidationResult]) -> Self {
        let total_outputs = results.len();
        let valid_outputs = results.iter().filter(|r| r.is_valid).count();
        let invalid_outputs = total_outputs - valid_outputs;
        let success_rate = if total_outputs > 0 {
            (valid_outputs as f64 / total_outputs as f64) * 100.0
        } else {
            0.0
        };

        Self {
            total_outputs,
            valid_outputs,
            invalid_outputs,
            success_rate,
        }
    }
}

/// Batch validation options for configuring validation behavior
#[derive(Debug, Clone)]
pub struct BatchValidationOptions {
    /// Whether to continue validation after encountering errors
    pub continue_on_error: bool,
    /// Maximum number of errors to collect per output
    pub max_errors_per_output: usize,
    /// Whether to validate range proofs (can be expensive)
    pub validate_range_proofs: bool,
    /// Whether to validate signatures (can be expensive)
    pub validate_signatures: bool,
    /// Whether to validate commitments
    pub validate_commitments: bool,
}

impl Default for BatchValidationOptions {
    fn default() -> Self {
        Self {
            continue_on_error: true,
            max_errors_per_output: 5,
            validate_range_proofs: true,
            validate_signatures: true,
            validate_commitments: true,
        }
    }
}

/// Validate a batch of transaction outputs with optimized performance
pub fn validate_output_batch(
    outputs: &[LightweightTransactionOutput],
    options: &BatchValidationOptions,
) -> BatchValidationResult {
    let mut results = Vec::with_capacity(outputs.len());

    for (index, output) in outputs.iter().enumerate() {
        let mut errors = Vec::new();
        let mut is_valid = true;

        // Validate commitment integrity
        if options.validate_commitments {
            if let Err(e) = validate_commitment_integrity(output) {
                errors.push(e);
                is_valid = false;
                if !options.continue_on_error || errors.len() >= options.max_errors_per_output {
                    results.push(OutputValidationResult {
                        index,
                        is_valid,
                        errors,
                    });
                    continue;
                }
            }
        }

        // Validate range proofs
        if options.validate_range_proofs {
            if let Some(proof) = output.proof() {
                if let Err(e) = validate_range_proof(proof, output.commitment(), output.minimum_value_promise()) {
                    errors.push(e);
                    is_valid = false;
                    if !options.continue_on_error || errors.len() >= options.max_errors_per_output {
                        results.push(OutputValidationResult {
                            index,
                            is_valid,
                            errors,
                        });
                        continue;
                    }
                }
            }
        }

        // Validate metadata signature
        if options.validate_signatures {
            if let Err(e) = validate_metadata_signature(output) {
                errors.push(e);
                is_valid = false;
                if !options.continue_on_error || errors.len() >= options.max_errors_per_output {
                    results.push(OutputValidationResult {
                        index,
                        is_valid,
                        errors,
                    });
                    continue;
                }
            }
        }

        results.push(OutputValidationResult {
            index,
            is_valid,
            errors,
        });
    }

    let summary = BatchValidationSummary::new(&results);
    let is_valid = summary.invalid_outputs == 0;

    BatchValidationResult {
        is_valid,
        results,
        summary,
    }
}

/// Validate a batch of transaction outputs with parallel processing (when available)
#[cfg(feature = "parallel")]
pub fn validate_output_batch_parallel(
    outputs: &[LightweightTransactionOutput],
    options: &BatchValidationOptions,
) -> BatchValidationResult {
    use rayon::prelude::*;

    let results: Vec<OutputValidationResult> = outputs
        .par_iter()
        .enumerate()
        .map(|(index, output)| {
            let mut errors = Vec::new();
            let mut is_valid = true;

            // Validate commitment integrity
            if options.validate_commitments {
                if let Err(e) = validate_commitment_integrity(output) {
                    errors.push(e);
                    is_valid = false;
                    if !options.continue_on_error || errors.len() >= options.max_errors_per_output {
                        return OutputValidationResult {
                            index,
                            is_valid,
                            errors,
                        };
                    }
                }
            }

            // Validate range proofs
            if options.validate_range_proofs {
                if let Some(proof) = output.proof() {
                    if let Err(e) = validate_range_proof(proof, output.commitment(), output.minimum_value_promise()) {
                        errors.push(e);
                        is_valid = false;
                        if !options.continue_on_error || errors.len() >= options.max_errors_per_output {
                            return OutputValidationResult {
                                index,
                                is_valid,
                                errors,
                            };
                        }
                    }
                }
            }

            // Validate metadata signature
            if options.validate_signatures {
                if let Err(e) = validate_metadata_signature(output) {
                    errors.push(e);
                    is_valid = false;
                    if !options.continue_on_error || errors.len() >= options.max_errors_per_output {
                        return OutputValidationResult {
                            index,
                            is_valid,
                            errors,
                        };
                    }
                }
            }

            OutputValidationResult {
                index,
                is_valid,
                errors,
            }
        })
        .collect();

    let summary = BatchValidationSummary::new(&results);
    let is_valid = summary.invalid_outputs == 0;

    BatchValidationResult {
        is_valid,
        results,
        summary,
    }
}

// Helper functions for validation
fn validate_commitment_integrity(output: &LightweightTransactionOutput) -> Result<(), ValidationError> {
    // Basic commitment validation
    let commitment_bytes = output.commitment().as_bytes();
    if commitment_bytes.len() != 33 {
        return Err(ValidationError::commitment_validation_failed(
            "Commitment must be 33 bytes",
        ));
    }
    
    // Check commitment prefix (should be 0x08 for valid commitments)
    if commitment_bytes[0] != 0x08 {
        return Err(ValidationError::commitment_validation_failed(
            "Invalid commitment prefix",
        ));
    }
    
    Ok(())
}

fn validate_range_proof(
    proof: &crate::data_structures::wallet_output::LightweightRangeProof,
    commitment: &crate::data_structures::types::CompressedCommitment,
    minimum_value_promise: crate::data_structures::types::MicroMinotari,
) -> Result<(), ValidationError> {
    // Basic range proof validation
    if proof.bytes.is_empty() {
        return Err(ValidationError::range_proof_validation_failed(
            "Range proof cannot be empty",
        ));
    }
    
    // Check that the proof has a reasonable size
    if proof.bytes.len() > 10000 { // 10KB as a reasonable upper bound
        return Err(ValidationError::range_proof_validation_failed(
            "Range proof is unreasonably large",
        ));
    }
    
    // For now, we'll do basic structure validation
    // In a full implementation, this would validate the actual proof
    
    Ok(())
}

fn validate_metadata_signature(output: &LightweightTransactionOutput) -> Result<(), ValidationError> {
    // Basic metadata signature validation
    let signature_bytes = &output.metadata_signature().bytes;
    if signature_bytes.len() != 64 {
        return Err(ValidationError::metadata_signature_validation_failed(
            "Metadata signature must be 64 bytes",
        ));
    }
    
    // Check that signature is not all zeros
    if signature_bytes.iter().all(|&b| b == 0) {
        return Err(ValidationError::metadata_signature_validation_failed(
            "Metadata signature cannot be all zeros",
        ));
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_structures::{
        encrypted_data::EncryptedData,
        transaction_output::LightweightTransactionOutput,
        types::{CompressedCommitment, CompressedPublicKey, MicroMinotari},
        wallet_output::{LightweightCovenant, LightweightOutputFeatures, LightweightRangeProof, LightweightScript, LightweightSignature},
    };

    fn create_test_output(value: u64, is_valid: bool) -> LightweightTransactionOutput {
        let commitment = if is_valid {
            CompressedCommitment::new([0x08; 33]) // Valid commitment prefix
        } else {
            CompressedCommitment::new([0x00; 33]) // Invalid commitment prefix
        };

        let encrypted_data = EncryptedData::from_hex("0102030405060708090a0b0c0d0e0f10").unwrap();

        let range_proof = if is_valid {
            Some(LightweightRangeProof { bytes: vec![0x01, 0x02, 0x03, 0x04] })
        } else {
            Some(LightweightRangeProof { bytes: vec![] }) // Invalid empty proof
        };

        LightweightTransactionOutput::new(
            0,
            LightweightOutputFeatures::default(),
            commitment,
            range_proof,
            LightweightScript::default(),
            CompressedPublicKey::new([0x01; 32]),
            LightweightSignature { bytes: vec![0x01; 64] },
            LightweightCovenant::default(),
            encrypted_data,
            MicroMinotari::from(0),
        )
    }

    #[test]
    fn test_batch_validation_success() {
        let outputs = vec![
            create_test_output(100, true),
            create_test_output(200, true),
            create_test_output(300, true),
        ];

        let options = BatchValidationOptions::default();
        let result = validate_output_batch(&outputs, &options);

        assert!(result.is_valid);
        assert_eq!(result.summary.total_outputs, 3);
        assert_eq!(result.summary.valid_outputs, 3);
        assert_eq!(result.summary.invalid_outputs, 0);
        assert_eq!(result.summary.success_rate, 100.0);

        for output_result in &result.results {
            assert!(output_result.is_valid);
            assert!(output_result.errors.is_empty());
        }
    }

    #[test]
    fn test_batch_validation_with_errors() {
        let outputs = vec![
            create_test_output(100, true),
            create_test_output(200, false), // Invalid
            create_test_output(300, true),
        ];

        let options = BatchValidationOptions::default();
        let result = validate_output_batch(&outputs, &options);

        assert!(!result.is_valid);
        assert_eq!(result.summary.total_outputs, 3);
        assert_eq!(result.summary.valid_outputs, 2);
        assert_eq!(result.summary.invalid_outputs, 1);
        assert!((result.summary.success_rate - 66.67).abs() < 0.01);

        assert!(result.results[0].is_valid);
        assert!(!result.results[1].is_valid);
        assert!(result.results[2].is_valid);
    }

    #[test]
    fn test_batch_validation_options() {
        let outputs = vec![
            create_test_output(100, false), // Invalid
            create_test_output(200, false), // Invalid
        ];

        // Test with continue_on_error = false
        let mut options = BatchValidationOptions::default();
        options.continue_on_error = false;
        options.max_errors_per_output = 1;

        let result = validate_output_batch(&outputs, &options);

        assert!(!result.is_valid);
        assert_eq!(result.summary.total_outputs, 2);
        assert_eq!(result.summary.valid_outputs, 0);
        assert_eq!(result.summary.invalid_outputs, 2);

        // Verify that errors are limited per output
        for output_result in &result.results {
            assert!(!output_result.is_valid);
            assert!(output_result.errors.len() <= 1);
        }
    }

    #[test]
    fn test_batch_validation_disabled_checks() {
        let outputs = vec![create_test_output(100, false)]; // Invalid

        let mut options = BatchValidationOptions::default();
        options.validate_range_proofs = false;
        options.validate_signatures = false;
        options.validate_commitments = false;

        let result = validate_output_batch(&outputs, &options);

        // Should pass since all validation is disabled
        assert!(result.is_valid);
        assert_eq!(result.summary.valid_outputs, 1);
        assert_eq!(result.summary.invalid_outputs, 0);
    }

    #[test]
    fn test_empty_batch() {
        let outputs = vec![];
        let options = BatchValidationOptions::default();
        let result = validate_output_batch(&outputs, &options);

        assert!(result.is_valid);
        assert_eq!(result.summary.total_outputs, 0);
        assert_eq!(result.summary.valid_outputs, 0);
        assert_eq!(result.summary.invalid_outputs, 0);
        assert_eq!(result.summary.success_rate, 0.0);
        assert!(result.results.is_empty());
    }

    #[test]
    fn test_batch_validation_summary() {
        let results = vec![
            OutputValidationResult {
                index: 0,
                is_valid: true,
                errors: vec![],
            },
            OutputValidationResult {
                index: 1,
                is_valid: false,
                errors: vec![ValidationError::CommitmentValidationFailed("Invalid commitment".to_string())],
            },
            OutputValidationResult {
                index: 2,
                is_valid: true,
                errors: vec![],
            },
        ];

        let summary = BatchValidationSummary::new(&results);

        assert_eq!(summary.total_outputs, 3);
        assert_eq!(summary.valid_outputs, 2);
        assert_eq!(summary.invalid_outputs, 1);
        assert!((summary.success_rate - 66.67).abs() < 0.01);
    }
} 