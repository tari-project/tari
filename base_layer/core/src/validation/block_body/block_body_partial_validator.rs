//  Copyright 2021, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::collections::VecDeque;

use log::error;
use tari_common_types::types::FixedHash;
use tari_node_components::blocks::Block;
use tari_transaction_components::{
    aggregated_body::AggregateBody,
    consensus::ConsensusConstants,
    transaction_components::{TransactionInput, TransactionOutput},
    validation::helpers::{check_tari_encrypted_data_byte_size, check_tari_script_byte_size},
};
use tari_utilities::hex::Hex;

use crate::{
    chain_storage::{BlockchainBackend, InputMinedInfo, OutputMinedInfo},
    consensus::BaseNodeConsensusManager,
    validation::{
        ValidationError,
        aggregate_body::{
            check_total_burned,
            validate_input_maturity,
            verify_no_duplicate_validator_node_registrations,
            verify_no_duplicated_inputs_outputs,
            verify_timelocks,
        },
    },
};

pub const LOG_TARGET: &str = "c::val::block_body_internal_consistency_validator";

/// Validates the internal consistency of a block body against the blockchain database for a particular height <- than
/// the tip height.
#[derive(Clone)]
pub struct BlockBodyPartialValidator {
    consensus_manager: BaseNodeConsensusManager,
}

impl BlockBodyPartialValidator {
    /// Create a new BlockBodyPartialValidator
    pub fn new(consensus_manager: BaseNodeConsensusManager) -> Self {
        Self { consensus_manager }
    }

    /// Validate the internal consistency of the block body against the blockchain database.
    pub fn validate<B>(&self, backend: &B, block: &Block) -> Result<(), ValidationError>
    where B: BlockchainBackend + Send + Sync + 'static {
        let constants = self.consensus_manager.consensus_constants(block.header.height);
        let header_hash = block.header.hash();
        validate_kernels_in_db(&block.body, backend, header_hash).inspect_err(|e| {
            error!(
                target: LOG_TARGET,
                "BlockBodyPartialValidator: validate_kernels_in_db height #{}({}): {:?}",
                block.header.height, block.hash().to_hex(), e
            )
        })?;

        validate_input_hashes_in_db(&block.body, backend, header_hash, block.header.height).inspect_err(|e| {
            error!(
                target: LOG_TARGET,
                "BlockBodyPartialValidator: validate_input_hashes_in_db height #{}({}): {:?}",
                block.header.height, block.hash().to_hex(), e
            )
        })?;

        validate_output_hashes_in_db(&block.body, backend, header_hash, block.header.height).inspect_err(|e| {
            error!(
                target: LOG_TARGET,
                "BlockBodyPartialValidator: validate_output_hashes_in_db height #{}({}): {:?}",
                    block.header.height, block.hash().to_hex(), e
            )
        })?;

        validate_input_maturity(&block.body, block.header.height).inspect_err(|e| {
            error!(
                target: LOG_TARGET,
                "BlockBodyPartialValidator: validate_input_maturity height #{}({}): {:?}",
                block.header.height, block.hash().to_hex(), e
            )
        })?;

        check_outputs(constants, &block.body, block.header.height).inspect_err(|e| {
            error!(
                target: LOG_TARGET,
                "BlockBodyPartialValidator: check_outputs height #{}({}): {:?}",
                block.header.height, block.hash().to_hex(), e
            )
        })?;

        verify_no_duplicate_validator_node_registrations(&block.body).inspect_err(|e| {
            error!(
                target: LOG_TARGET,
                "BlockBodyPartialValidator: verify_no_duplicate_validator_node_registrations {}/{}: {:?}",
                block.header.height, block.hash().to_hex(), e
            )
        })?;

        verify_no_duplicated_inputs_outputs(&block.body).inspect_err(|e| {
            error!(
                target: LOG_TARGET,
                "BlockBodyPartialValidator: verify_no_duplicated_inputs_outputs {}/{}: {:?}",
                block.header.height, block.hash().to_hex(), e
            )
        })?;

        check_total_burned(&block.body).inspect_err(|e| {
            error!(
                target: LOG_TARGET,
                "BlockBodyPartialValidator: check_total_burned {}/{}: {:?}",
                block.header.height, block.hash().to_hex(), e
            )
        })?;

        verify_timelocks(&block.body, block.header.height).inspect_err(|e| {
            error!(
                target: LOG_TARGET,
                "BlockBodyPartialValidator: verify_timelocks {}/{}: {:?}",
                block.header.height, block.hash().to_hex(), e
            )
        })?;

        Ok(())
    }
}

// This function checks that all kernels in the aggregate body exist in the database and that the expected hash
// corresponds.
fn validate_kernels_in_db<B: BlockchainBackend>(
    body: &AggregateBody,
    db: &B,
    header_hash: FixedHash,
) -> Result<(), ValidationError> {
    for kernel in body.kernels() {
        match db.fetch_kernel_by_excess_sig(&kernel.excess_sig)? {
            Some((_kernel, hash)) if hash != header_hash => {
                let msg = format!(
                    "'kernels_db' header hash mismatch - expected {}, found {}",
                    header_hash, hash
                );
                return Err(ValidationError::HeaderHashMismatch(msg));
            },
            Some((db_kernel, _hash)) if &db_kernel != kernel => {
                let msg = format!(
                    "Aggregate body kernel {} does not match stored kernel in chain",
                    kernel.excess.to_hex(),
                );
                return Err(ValidationError::MissingKernelError(msg));
            },
            None => {
                let msg = format!(
                    "Aggregate body kernel excess {} not found in chain",
                    kernel.excess.to_hex(),
                );
                return Err(ValidationError::MissingKernelError(msg));
            },
            _ => {},
        }
    }
    Ok(())
}

// This function checks that all inputs in the aggregate body exist in the database, either as spent or unspent, and
// that the expected height and hash corresponds.
fn validate_input_hashes_in_db<B: BlockchainBackend>(
    body: &AggregateBody,
    db: &B,
    header_hash: FixedHash,
    header_height: u64,
) -> Result<(), ValidationError> {
    let mut inputs: VecDeque<&TransactionInput> = body.inputs().iter().collect();
    let mut to_remove = Vec::new();
    for input in &inputs {
        let input_output_hash = input.output_hash();

        // An input is unique within a block, so select the entry for the block being validated.
        if let Some(InputMinedInfo { spent_height, .. }) = db
            .fetch_inputs(&input_output_hash)?
            .into_iter()
            .find(|info| info.header_hash == header_hash)
        {
            if spent_height != header_height {
                return Err(ValidationError::HeaderHeightMismatch(format!(
                    "Expected {}, found {}",
                    header_height, spent_height
                )));
            }
            // An identical output can be mined again after it is spent, so check the instance this input spent.
            let infos = db.fetch_mined_info_by_output_hash(&input_output_hash)?;
            let spent_instance = infos
                .iter()
                .find(|info| info.input.as_ref().is_some_and(|i| i.header_hash == header_hash))
                .and_then(|info| info.output.as_ref());
            if spent_instance.is_none() {
                // Pairing only matches an instance mined at or before the spend, so report a later one if any.
                if let Some(mined_height) = infos
                    .iter()
                    .filter_map(|info| info.output.as_ref())
                    .map(|output| output.mined_height)
                    .filter(|mined_height| *mined_height > spent_height)
                    .min()
                {
                    return Err(ValidationError::InputSpentBeforeMined(format!(
                        "Input output hash {} spent at height {} before it was mined at height {}",
                        input_output_hash.to_hex(),
                        spent_height,
                        mined_height
                    )));
                }
                return Err(ValidationError::MissingOutputError(format!(
                    "Input output hash {} not found as unspent output",
                    input_output_hash.to_hex()
                )));
            }
            to_remove.push(input_output_hash);
        }
    }
    inputs.retain(|input| !to_remove.contains(&input.output_hash()));

    if !inputs.is_empty() {
        let hashes = inputs
            .iter()
            .map(|i| i.output_hash().to_hex())
            .collect::<Vec<String>>()
            .join(", ");
        return Err(ValidationError::MissingOutputError(format!(
            "Expected to find these either spent or unspent: {}",
            hashes
        )));
    }

    Ok(())
}

// This function checks that all outputs in the aggregate body exist in the database, either as spent or unspent, and
// that the expected height and hash corresponds.
fn validate_output_hashes_in_db<B: BlockchainBackend>(
    body: &AggregateBody,
    db: &B,
    header_hash: FixedHash,
    header_height: u64,
) -> Result<(), ValidationError> {
    let mut outputs: VecDeque<&TransactionOutput> = body.outputs().iter().collect();
    let mut to_remove = Vec::new();
    for output in &outputs {
        let output_hash = output.hash();
        // An output is unique within a block, so select the entry for the block being validated.
        if let Some(OutputMinedInfo {
            mined_height: height, ..
        }) = db
            .fetch_outputs(&output_hash)?
            .into_iter()
            .find(|info| info.header_hash == header_hash)
        {
            if height != header_height {
                return Err(ValidationError::HeaderHeightMismatch(format!(
                    "Expected {}, found {}",
                    header_height, height
                )));
            }
            to_remove.push(output_hash);
        }
    }
    outputs.retain(|output| !to_remove.contains(&output.hash()));
    if !outputs.is_empty() {
        let hashes = outputs
            .iter()
            .map(|i| i.hash().to_hex())
            .collect::<Vec<String>>()
            .join(", ");
        return Err(ValidationError::MissingOutputError(format!(
            "Expected to find these either spent or unspent: {}",
            hashes
        )));
    }

    Ok(())
}

// This function checks that the output type is permitted, that the TariScript and encrypted data byte size does not
// exceed the maximum, and performs basic Ootle-specific checks for sidechain features.
fn check_outputs(constants: &ConsensusConstants, body: &AggregateBody, height: u64) -> Result<(), ValidationError> {
    let max_script_size = constants.max_script_byte_size();
    let max_encrypted_data_size = constants.max_extra_encrypted_data_byte_size();
    for output in body.outputs() {
        check_tari_script_byte_size(&output.script, max_script_size)?;
        check_tari_encrypted_data_byte_size(&output.encrypted_data, max_encrypted_data_size)?;

        // Ootle checks
        if let Some(sidechain_features) = output.features.sidechain_feature.as_ref() {
            let current_epoch = constants.block_height_to_epoch(height);
            if let Some(vn_reg) = sidechain_features.validator_node_registration() &&
                vn_reg.max_epoch() < current_epoch
            {
                return Err(ValidationError::ValidatorNodeRegistrationMaxEpoch {
                    public_key: vn_reg.public_key().to_string(),
                    max_epoch: vn_reg.max_epoch(),
                    current_epoch,
                });
            }

            if let Some(exit) = sidechain_features.validator_node_exit() {
                if constants.vn_registration_max_exits_per_epoch() == 0 {
                    return Err(ValidationError::ValidatorNodeExitNotPermitted);
                }
                if exit.max_epoch() < current_epoch {
                    return Err(ValidationError::ValidatorNodeRegistrationMaxEpoch {
                        public_key: exit.public_key().to_string(),
                        max_epoch: exit.max_epoch(),
                        current_epoch,
                    });
                }
                if exit.activation_epoch() > current_epoch {
                    return Err(ValidationError::ValidatorNodeNotRegistered {
                        public_key: exit.public_key().to_string(),
                        details: format!(
                            "exit invalid for validator node that only activates in {} (current epoch {current_epoch})",
                            exit.activation_epoch()
                        ),
                    });
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use tari_common::configuration::Network;
    use tari_common_types::{
        epoch::VnEpoch,
        types::{CompressedCommitment, CompressedPublicKey, CompressedSignature, PrivateKey},
    };
    use tari_crypto::keys::SecretKey;
    use tari_node_components::blocks::BlockHeader;
    use tari_transaction_components::{
        MicroMinotari,
        consensus::ConsensusManagerBuilder,
        transaction_components::{
            KernelFeatures,
            OutputFeatures,
            TransactionKernel,
            TransactionOutput,
            ValidatorNodeSignature,
        },
    };

    use super::*;
    use crate::{
        chain_storage::DbTransaction,
        test_helpers::{blockchain::create_new_blockchain, new_public_key},
    };

    const NETWORK: u8 = 0x10;

    fn exit_output(vn_secret_key: &PrivateKey, activation_epoch: VnEpoch, max_epoch: VnEpoch) -> TransactionOutput {
        let signature =
            ValidatorNodeSignature::sign_for_exit(vn_secret_key, NETWORK, None, activation_epoch, max_epoch);
        TransactionOutput {
            features: OutputFeatures::for_validator_node_exit(signature, None, activation_epoch, max_epoch),
            commitment: CompressedCommitment::from_compressed_key(new_public_key()),
            ..Default::default()
        }
    }

    fn registration_output(vn_secret_key: &PrivateKey, max_epoch: VnEpoch) -> TransactionOutput {
        let claim_public_key = CompressedPublicKey::from_secret_key(vn_secret_key);
        let signature =
            ValidatorNodeSignature::sign_for_registration(vn_secret_key, NETWORK, None, &claim_public_key, max_epoch);
        TransactionOutput {
            features: OutputFeatures::for_validator_node_registration(signature, claim_public_key, None, max_epoch),
            commitment: CompressedCommitment::from_compressed_key(new_public_key()),
            ..Default::default()
        }
    }

    #[test]
    fn check_outputs_rejects_exit_with_future_activation_epoch() {
        let rules = ConsensusManagerBuilder::new(Network::LocalNet).build();
        let constants = rules.consensus_constants(0);
        let sk = PrivateKey::random(&mut rand::rng());
        let body = AggregateBody::new_unsorted(vec![], vec![exit_output(&sk, VnEpoch(5), VnEpoch(10))], vec![]);
        let err = check_outputs(constants, &body, 0).unwrap_err();
        assert!(matches!(err, ValidationError::ValidatorNodeNotRegistered { .. }));
    }

    #[test]
    fn partial_validator_rejects_duplicate_validator_node_registrations() {
        let db = create_new_blockchain();
        let rules = BaseNodeConsensusManager::builder(Network::LocalNet).build().unwrap();
        let validator = BlockBodyPartialValidator::new(rules);

        let sk = PrivateKey::random(&mut rand::rng());
        let out1 = registration_output(&sk, VnEpoch(10));
        let out2 = registration_output(&sk, VnEpoch(11));

        let mut header = BlockHeader::new(0);
        header.height = 1;
        let header_hash = header.hash();

        let mut txn = DbTransaction::new();
        txn.insert_utxo(out1.clone(), header_hash, header.height, header.timestamp.as_u64());
        txn.insert_utxo(out2.clone(), header_hash, header.height, header.timestamp.as_u64());
        db.write(txn).unwrap();

        let block = Block::new(header, AggregateBody::new_unsorted(vec![], vec![out1, out2], vec![]));
        let backend = db.db_read_access().unwrap();
        let err = validator.validate(&*backend, &block).unwrap_err();
        assert!(matches!(
            err,
            ValidationError::DuplicateValidatorNodeRegistration { .. }
        ));
    }

    #[test]
    fn validate_kernels_in_db_rejects_modified_kernel_with_matching_excess_sig() {
        let db = create_new_blockchain();
        let header_hash = FixedHash::from([0x42u8; 32]);

        let excess = CompressedCommitment::from_compressed_key(new_public_key());
        let excess_sig = CompressedSignature::new(new_public_key(), PrivateKey::random(&mut rand::rng()));
        let stored_kernel = TransactionKernel::new_current_version(
            KernelFeatures::empty(),
            MicroMinotari::from(100),
            0,
            excess,
            excess_sig,
            None,
        );

        let mut txn = DbTransaction::new();
        txn.insert_kernel(stored_kernel.clone(), header_hash, 0);
        db.write(txn).unwrap();

        let backend = db.db_read_access().unwrap();
        let unchanged_body = AggregateBody::new_unsorted(vec![], vec![], vec![stored_kernel.clone()]);
        validate_kernels_in_db(&unchanged_body, &*backend, header_hash).unwrap();

        let mut mutated_kernel = stored_kernel;
        mutated_kernel.fee = MicroMinotari::from(200);
        let mutated_body = AggregateBody::new_unsorted(vec![], vec![], vec![mutated_kernel]);
        let err = validate_kernels_in_db(&mutated_body, &*backend, header_hash).unwrap_err();
        assert!(matches!(err, ValidationError::MissingKernelError(_)));
    }
}
