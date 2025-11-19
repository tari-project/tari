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
use tari_node_components::blocks::Block;
use tari_transaction_components::{
    aggregated_body::AggregateBody,
    consensus::ConsensusConstants,
    transaction_components::{TransactionInput, TransactionOutput},
    validation::helpers::{check_tari_encrypted_data_byte_size, check_tari_script_byte_size},
};
use tari_utilities::hex::Hex;

use crate::{
    chain_storage::BlockchainBackend,
    consensus::BaseNodeConsensusManager,
    validation::{
        aggregate_body::{
            check_total_burned,
            validate_input_maturity,
            verify_no_duplicated_inputs_outputs,
            verify_timelocks,
        },
        ValidationError,
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
        validate_kernels_in_db(&block.body, backend).inspect_err(|e| {
            error!(
                target: LOG_TARGET,
                "BlockBodyPartialValidator: validate_kernels_in_db height #{}({}): {:?}",
                block.header.height, block.hash().to_hex(), e
            )
        })?;

        validate_input_hashes_in_db(&block.body, backend).inspect_err(|e| {
            error!(
                target: LOG_TARGET,
                "BlockBodyPartialValidator: validate_input_hashes_in_db height #{}({}): {:?}",
                block.header.height, block.hash().to_hex(), e
            )
        })?;

        if block.header.height < backend.fetch_chain_metadata()?.best_block_height() {
            validate_output_hashes_in_db(&block.body, backend).inspect_err(|e| {
                error!(
                    target: LOG_TARGET,
                    "BlockBodyPartialValidator: validate_output_hashes_in_db height #{}({}): {:?}",
                        block.header.height, block.hash().to_hex(), e
                )
            })?;
        }

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

// This function checks that all kernels in the aggregate body exist in the database.
fn validate_kernels_in_db<B: BlockchainBackend>(body: &AggregateBody, db: &B) -> Result<(), ValidationError> {
    for kernel in body.kernels() {
        if db.fetch_kernel_by_excess_sig(&kernel.excess_sig)?.is_none() {
            let msg = format!(
                "Aggregate body kernel excess {} not found in chain",
                kernel.excess.to_hex(),
            );
            return Err(ValidationError::MissingKernelError(msg));
        };
    }
    Ok(())
}

// This function checks that all inputs in the aggregate body exist in the database, either as spent or unspent.
fn validate_input_hashes_in_db<B: BlockchainBackend>(body: &AggregateBody, db: &B) -> Result<(), ValidationError> {
    let mut inputs: VecDeque<&TransactionInput> = body.inputs().iter().collect();
    let mut to_remove = Vec::new();
    for input in &inputs {
        let input_output_hash = input.output_hash();
        if db.fetch_output(&input_output_hash)?.is_some() || db.fetch_input(&input_output_hash)?.is_some() {
            // Mark for removal
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

// This function checks that all outputs in the aggregate body exist in the database, either as spent or unspent.
fn validate_output_hashes_in_db<B: BlockchainBackend>(body: &AggregateBody, db: &B) -> Result<(), ValidationError> {
    let mut outputs: VecDeque<&TransactionOutput> = body.outputs().iter().collect();
    let mut to_remove = Vec::new();
    for output in &outputs {
        let output_hash = output.hash();
        if db.fetch_output(&output_hash)?.is_some() || db.fetch_input(&output_hash)?.is_some() {
            // Mark for removal
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
            if let Some(vn_reg) = sidechain_features.validator_node_registration() {
                if vn_reg.max_epoch() < current_epoch {
                    return Err(ValidationError::ValidatorNodeRegistrationMaxEpoch {
                        public_key: vn_reg.public_key().to_string(),
                        max_epoch: vn_reg.max_epoch(),
                        current_epoch,
                    });
                };
            }

            if let Some(exit) = sidechain_features.validator_node_exit() {
                if exit.max_epoch() < current_epoch {
                    return Err(ValidationError::ValidatorNodeRegistrationMaxEpoch {
                        public_key: exit.public_key().to_string(),
                        max_epoch: exit.max_epoch(),
                        current_epoch,
                    });
                }
            };

            if let Some(eviction_proof) = sidechain_features.eviction_proof() {
                let epoch = eviction_proof.epoch();
                let tip_epoch = constants.block_height_to_epoch(height);
                if epoch > tip_epoch {
                    return Err(ValidationError::SidechainEvictionProofInvalidEpoch {
                        epoch,
                        tip_height: height,
                    });
                }
            };
        }
    }
    Ok(())
}
