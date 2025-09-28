// Copyright 2019. The Tari Project
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
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use tari_script::TariScript;

use crate::{
    consensus::consensus_constants::ConsensusConstants,
    helpers::borsh::SerializedSize,
    transaction_components::{
        covenants::Covenant,
        encrypted_data::STATIC_ENCRYPTED_DATA_SIZE_TOTAL,
        EncryptedData,
        TransactionInput,
        TransactionKernel,
        TransactionOutput,
    },
    validation::AggregatedBodyValidationError,
};

pub const LOG_TARGET: &str = "c::val::helpers";

pub fn is_all_unique_and_sorted<'a, I: IntoIterator<Item = &'a T>, T: PartialOrd + 'a>(items: I) -> bool {
    let mut items = items.into_iter();
    let prev_item = items.next();
    if prev_item.is_none() {
        return true;
    }
    let mut prev_item = prev_item.unwrap();
    for item in items {
        if item <= prev_item {
            return false;
        }
        prev_item = item;
    }

    true
}

/// Checks the byte size of TariScript is less than or equal to the given size, otherwise returns an error.
pub fn check_tari_script_byte_size(
    script: &TariScript,
    max_script_size: usize,
) -> Result<(), AggregatedBodyValidationError> {
    let script_size = script.get_serialized_size().map_err(|e| {
        AggregatedBodyValidationError::SerializationError(format!("Failed to get serialized script size: {e}"))
    })?;
    if script_size > max_script_size {
        return Err(AggregatedBodyValidationError::TariScriptExceedsMaxSize {
            max_script_size,
            actual_script_size: script_size,
        });
    }
    Ok(())
}

/// Checks the byte size of EncryptedData is less than or equal to the given size, otherwise returns an error.
pub fn check_tari_encrypted_data_byte_size(
    encrypted_data: &EncryptedData,
    max_encrypted_data_size: usize,
) -> Result<(), AggregatedBodyValidationError> {
    let encrypted_data_size = encrypted_data.as_bytes().len();
    if encrypted_data_size > max_encrypted_data_size + STATIC_ENCRYPTED_DATA_SIZE_TOTAL {
        return Err(AggregatedBodyValidationError::EncryptedDataExceedsMaxSize {
            max_encrypted_data_size: max_encrypted_data_size + STATIC_ENCRYPTED_DATA_SIZE_TOTAL,
            actual_encrypted_data_size: encrypted_data_size,
        });
    }
    Ok(())
}

pub fn check_permitted_output_types(
    constants: &ConsensusConstants,
    output: &TransactionOutput,
) -> Result<(), AggregatedBodyValidationError> {
    if !constants
        .permitted_output_types()
        .contains(&output.features.output_type)
    {
        return Err(AggregatedBodyValidationError::OutputTypeNotPermitted {
            output_type: output.features.output_type,
        });
    }

    Ok(())
}

pub fn check_covenant_length(covenant: &Covenant, max_token_len: u32) -> Result<(), AggregatedBodyValidationError> {
    if covenant.num_tokens() > max_token_len as usize {
        return Err(AggregatedBodyValidationError::CovenantTooLarge {
            max_size: max_token_len as usize,
            actual_size: covenant.num_tokens(),
        });
    }

    Ok(())
}

pub fn check_permitted_range_proof_types(
    constants: &ConsensusConstants,
    output: &TransactionOutput,
) -> Result<(), AggregatedBodyValidationError> {
    let binding = constants.permitted_range_proof_types();
    let permitted_range_proof_types = binding.iter().find(|&&t| t.0 == output.features.output_type).ok_or(
        AggregatedBodyValidationError::OutputTypeNotMatchedToRangeProofType {
            output_type: output.features.output_type,
        },
    )?;

    if !permitted_range_proof_types
        .1
        .contains(&output.features.range_proof_type)
    {
        return Err(AggregatedBodyValidationError::RangeProofTypeNotPermitted {
            range_proof_type: output.features.range_proof_type,
        });
    }

    Ok(())
}

pub fn validate_input_version(
    consensus_constants: &ConsensusConstants,
    input: &TransactionInput,
) -> Result<(), AggregatedBodyValidationError> {
    if !consensus_constants.input_version_range().contains(&input.version) {
        let msg = format!(
            "Transaction input contains a version not allowed by consensus ({:?})",
            input.version
        );
        return Err(AggregatedBodyValidationError::ConsensusError(msg));
    }

    Ok(())
}

pub fn validate_output_version(
    consensus_constants: &ConsensusConstants,
    output: &TransactionOutput,
) -> Result<(), AggregatedBodyValidationError> {
    let valid_output_version = consensus_constants
        .output_version_range()
        .outputs
        .contains(&output.version);

    if !valid_output_version {
        let msg = format!(
            "Transaction output version is not allowed by consensus ({:?})",
            output.version
        );
        return Err(AggregatedBodyValidationError::ConsensusError(msg));
    }

    let valid_features_version = consensus_constants
        .output_version_range()
        .features
        .contains(&output.features.version);

    if !valid_features_version {
        let msg = format!(
            "Transaction output features version is not allowed by consensus ({:?})",
            output.features.version
        );
        return Err(AggregatedBodyValidationError::ConsensusError(msg));
    }

    for opcode in output.script.as_slice() {
        if !consensus_constants
            .output_version_range()
            .opcode
            .contains(&opcode.get_version())
        {
            let msg = format!("Transaction output script opcode is not allowed by consensus ({opcode})");
            return Err(AggregatedBodyValidationError::ConsensusError(msg));
        }
    }

    Ok(())
}

pub fn validate_kernel_version(
    consensus_constants: &ConsensusConstants,
    kernel: &TransactionKernel,
) -> Result<(), AggregatedBodyValidationError> {
    if !consensus_constants.kernel_version_range().contains(&kernel.version) {
        let msg = format!(
            "Transaction kernel version is not allowed by consensus ({:?})",
            kernel.version
        );
        return Err(AggregatedBodyValidationError::ConsensusError(msg));
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use tari_test_utils::unpack_enum;

    use super::*;
    use crate::{crypto_factories::CryptoFactories, test_helpers, test_helpers::TestParams};

    mod is_all_unique_and_sorted {
        use super::*;

        #[test]
        fn it_returns_true_when_nothing_to_compare() {
            assert!(is_all_unique_and_sorted::<_, usize>(&[]));
            assert!(is_all_unique_and_sorted(&[1]));
        }

        #[test]
        fn it_returns_true_when_unique_and_sorted() {
            let v = [1, 2, 3, 4, 5];
            assert!(is_all_unique_and_sorted(&v));
        }

        #[test]
        fn it_returns_false_when_unsorted() {
            let v = [2, 1, 3, 4, 5];
            assert!(!is_all_unique_and_sorted(&v));
        }

        #[test]
        fn it_returns_false_when_duplicate() {
            let v = [1, 2, 3, 4, 4];
            assert!(!is_all_unique_and_sorted(&v));
        }

        #[test]
        fn it_returns_false_when_duplicate_and_unsorted() {
            let v = [4, 2, 3, 0, 4];
            assert!(!is_all_unique_and_sorted(&v));
        }
    }

    mod check_coinbase_maturity {

        use super::*;
        use crate::{
            aggregated_body::AggregateBody,
            key_manager::create_memory_key_manager,
            transaction_components::{RangeProofType, TransactionError},
        };

        #[tokio::test]
        async fn it_succeeds_for_valid_coinbase() {
            let height = 1;
            let key_manager = create_memory_key_manager().await.unwrap();
            let test_params = TestParams::new(&key_manager).await;
            let rules = test_helpers::create_consensus_manager();
            let key_manager = create_memory_key_manager().await.unwrap();
            let coinbase = test_helpers::create_coinbase_wallet_output(
                &test_params,
                height,
                None,
                RangeProofType::RevealedValue,
                &key_manager,
            )
            .await;
            let coinbase_output = coinbase.to_transaction_output().unwrap();
            let coinbase_kernel =
                test_helpers::create_coinbase_kernel(coinbase.commitment_mask_key_id(), &key_manager).await;

            let body = AggregateBody::new(vec![], vec![coinbase_output], vec![coinbase_kernel]);

            let reward = rules.calculate_coinbase_and_fees(height, body.kernels()).unwrap();
            let coinbase_lock_height = rules.consensus_constants(height).coinbase_min_maturity();
            body.check_coinbase_output(reward, coinbase_lock_height, &CryptoFactories::default(), height, 1)
                .unwrap();
        }

        #[tokio::test]
        async fn it_returns_error_for_invalid_coinbase_maturity() {
            let height = 1;
            let key_manager = create_memory_key_manager().await.unwrap();
            let test_params = TestParams::new(&key_manager).await;
            let rules = test_helpers::create_consensus_manager();
            let mut coinbase = test_helpers::create_coinbase_wallet_output(
                &test_params,
                height,
                None,
                RangeProofType::RevealedValue,
                &key_manager,
            )
            .await;
            let mut features = coinbase.features().clone();
            features.maturity = 0;
            coinbase.set_features(features);
            let coinbase_output = coinbase.to_transaction_output().unwrap();
            let coinbase_kernel =
                test_helpers::create_coinbase_kernel(coinbase.commitment_mask_key_id(), &key_manager).await;

            let body = AggregateBody::new(vec![], vec![coinbase_output], vec![coinbase_kernel]);

            let reward = rules.calculate_coinbase_and_fees(height, body.kernels()).unwrap();
            let coinbase_lock_height = rules.consensus_constants(height).coinbase_min_maturity();

            let err = body
                .check_coinbase_output(reward, coinbase_lock_height, &CryptoFactories::default(), height, 1)
                .unwrap_err();
            unpack_enum!(TransactionError::InvalidCoinbaseMaturity = err);
        }

        #[tokio::test]
        async fn it_returns_error_for_invalid_coinbase_reward() {
            let height = 1;
            let key_manager = create_memory_key_manager().await.unwrap();
            let test_params = TestParams::new(&key_manager).await;
            let rules = test_helpers::create_consensus_manager();
            let mut coinbase = test_helpers::create_coinbase_wallet_output(
                &test_params,
                height,
                None,
                RangeProofType::BulletProofPlus,
                &key_manager,
            )
            .await;
            coinbase.set_value(123.into(), &key_manager).await.unwrap();
            let coinbase_output = coinbase.to_transaction_output().unwrap();
            let coinbase_kernel =
                test_helpers::create_coinbase_kernel(coinbase.commitment_mask_key_id(), &key_manager).await;

            let body = AggregateBody::new(vec![], vec![coinbase_output], vec![coinbase_kernel]);
            let reward = rules.calculate_coinbase_and_fees(height, body.kernels()).unwrap();
            let coinbase_lock_height = rules.consensus_constants(height).coinbase_min_maturity();

            let err = body
                .check_coinbase_output(reward, coinbase_lock_height, &CryptoFactories::default(), height, 1)
                .unwrap_err();
            unpack_enum!(TransactionError::InvalidCoinbase = err);
        }
    }
}
