// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Integration tests for the lightweight wallet extraction pipeline

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extraction::*;
    use crate::data_structures::{
        encrypted_data::EncryptedData,
        payment_id::PaymentId,
        types::{CompressedCommitment, MicroMinotari, PrivateKey},
        wallet_output::{
            LightweightOutputFeatures, LightweightOutputType, LightweightRangeProof,
            LightweightScript, LightweightSignature, LightweightCovenant, LightweightExecutionStack,
        },
        transaction_output::LightweightTransactionOutput,
    };
    use crate::key_management::{ConcreteKeyManager, ImportedPrivateKey};

    fn create_test_transaction_output_with_key(
        key: &PrivateKey,
        value: u64,
        payment_id: PaymentId,
        output_type: LightweightOutputType,
    ) -> LightweightTransactionOutput {
        let mut features = LightweightOutputFeatures::default();
        features.output_type = output_type;
        let encrypted_data = EncryptedData::encrypt_data(
            key,
            &CompressedCommitment::new([1u8; 33]),
            MicroMinotari::new(value),
            &PrivateKey::random(),
            payment_id,
        ).unwrap();
        LightweightTransactionOutput::new(
            1,
            features,
            CompressedCommitment::new([1u8; 33]),
            Some(LightweightRangeProof::default()),
            LightweightScript::default(),
            crate::data_structures::types::CompressedPublicKey::new([2u8; 32]),
            LightweightSignature::default(),
            LightweightCovenant::default(),
            encrypted_data,
            MicroMinotari::new(value),
        )
    }

    #[test]
    fn test_end_to_end_wallet_output_extraction_success() {
        // Setup key manager and import key
        let mut key_manager = ConcreteKeyManager::new([1u8; 32]);
        let test_key = PrivateKey::random();
        let imported_key = ImportedPrivateKey::new(test_key.clone(), Some("test_key".to_string()));
        key_manager.import_private_key(imported_key).unwrap();

        // Create a transaction output
        let payment_id = PaymentId::Open { data: b"integration".to_vec() };
        let tx_output = create_test_transaction_output_with_key(&test_key, 1234, payment_id.clone(), LightweightOutputType::Payment);

        // Decrypt
        let decryptor = EncryptedDataDecryptor::new(key_manager.key_store().clone());
        let decryption_result = decryptor.decrypt_transaction_output(&tx_output, None).unwrap();
        assert!(decryption_result.is_success());
        assert_eq!(decryption_result.value.unwrap(), MicroMinotari::new(1234));
        assert_eq!(decryption_result.payment_id.as_ref().unwrap(), &payment_id);

        // Extract payment ID
        let payment_id_result = PaymentIdExtractor::extract(
            tx_output.encrypted_data(),
            &test_key,
            tx_output.commitment(),
        );
        assert!(payment_id_result.is_success());
        assert_eq!(payment_id_result.payment_id.as_ref().unwrap(), &payment_id);

        // Reconstruct wallet output
        let reconstructor = WalletOutputReconstructor::new(key_manager);
        let reconstruction_result = reconstructor.reconstruct(&tx_output).unwrap();
        assert_eq!(reconstruction_result.value, MicroMinotari::new(1234));
        assert_eq!(reconstruction_result.payment_id, payment_id);
    }

    #[test]
    fn test_end_to_end_wallet_output_extraction_failure_corrupted_data() {
        // Setup key manager and import key
        let mut key_manager = ConcreteKeyManager::new([1u8; 32]);
        let test_key = PrivateKey::random();
        let imported_key = ImportedPrivateKey::new(test_key.clone(), Some("test_key".to_string()));
        key_manager.import_private_key(imported_key).unwrap();

        // Create a transaction output with corrupted encrypted data
        let mut tx_output = create_test_transaction_output_with_key(&test_key, 1234, PaymentId::Empty, LightweightOutputType::Payment);
        // Corrupt the encrypted data
        tx_output.encrypted_data.bytes[0] ^= 0xFF;

        // Attempt to reconstruct wallet output
        let reconstructor = WalletOutputReconstructor::new(key_manager);
        let result = reconstructor.reconstruct(&tx_output);
        assert!(result.is_err());
    }

    #[test]
    fn test_end_to_end_coinbase_output_handling() {
        // Coinbase outputs are handled specially
        let test_key = PrivateKey::random();
        let maturity = 100;
        let mut features = LightweightOutputFeatures::default();
        features.output_type = LightweightOutputType::Coinbase;
        features.maturity = maturity;
        let tx_output = LightweightTransactionOutput::new(
            1,
            features,
            CompressedCommitment::new([1u8; 33]),
            Some(LightweightRangeProof::default()),
            LightweightScript::default(),
            crate::data_structures::types::CompressedPublicKey::new([2u8; 32]),
            LightweightSignature::default(),
            LightweightCovenant::default(),
            EncryptedData::default(),
            MicroMinotari::new(5000),
        );
        let handler = SpecialOutputHandler::new();
        // Immature
        let result_immature = handler.handle_transaction_output(&tx_output, 50);
        assert!(!result_immature.is_success());
        // Mature
        let result_mature = handler.handle_transaction_output(&tx_output, 150);
        assert!(result_mature.is_success());
        assert_eq!(result_mature.output_type, SpecialOutputType::Coinbase);
    }

    #[test]
    fn test_end_to_end_burn_output_handling() {
        let mut features = LightweightOutputFeatures::default();
        features.output_type = LightweightOutputType::Burn;
        let tx_output = LightweightTransactionOutput::new(
            1,
            features,
            CompressedCommitment::new([1u8; 33]),
            Some(LightweightRangeProof::default()),
            LightweightScript::default(),
            crate::data_structures::types::CompressedPublicKey::new([2u8; 32]),
            LightweightSignature::default(),
            LightweightCovenant::default(),
            EncryptedData::default(),
            MicroMinotari::new(0),
        );
        let handler = SpecialOutputHandler::new();
        let result = handler.handle_transaction_output(&tx_output, 1000);
        assert!(result.is_success());
        assert_eq!(result.output_type, SpecialOutputType::Burn);
    }
} 