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

use log::warn;
use tari_utilities::hex::Hex;

use crate::{
    blocks::Block,
    consensus::ConsensusManager,
    transactions::{aggregated_body::AggregateBody, CryptoFactories},
    validation::{
        aggregate_body::AggregateBodyInternalConsistencyValidator,
        InternalConsistencyValidator,
        ValidationError,
    },
};

pub const LOG_TARGET: &str = "c::val::block_body_internal_consistency_validator";

#[derive(Clone)]
pub struct BlockBodyInternalConsistencyValidator {
    consensus_manager: ConsensusManager,
    factories: CryptoFactories,
    aggregate_body_validator: AggregateBodyInternalConsistencyValidator,
}

impl BlockBodyInternalConsistencyValidator {
    pub fn new(
        consensus_manager: ConsensusManager,
        bypass_range_proof_verification: bool,
        factories: CryptoFactories,
    ) -> Self {
        let aggregate_body_validator = AggregateBodyInternalConsistencyValidator::new(
            bypass_range_proof_verification,
            consensus_manager.clone(),
            factories.clone(),
        );
        Self {
            consensus_manager,
            factories,
            aggregate_body_validator,
        }
    }

    pub fn validate(&self, block: &Block) -> Result<(), ValidationError> {
        validate_block_specific_checks(block, &self.consensus_manager, &self.factories)?;
        validate_block_aggregate_body(block, &self.aggregate_body_validator, &self.consensus_manager)?;

        Ok(())
    }
}

impl InternalConsistencyValidator for BlockBodyInternalConsistencyValidator {
    fn validate_internal_consistency(&self, block: &Block) -> Result<(), ValidationError> {
        self.validate(block)
    }
}

fn validate_block_specific_checks(
    block: &Block,
    consensus_manager: &ConsensusManager,
    factories: &CryptoFactories,
) -> Result<(), ValidationError> {
    if block.header.height == 0 {
        warn!(target: LOG_TARGET, "Attempt to validate genesis block");
        return Err(ValidationError::ValidatingGenesis);
    }
    check_coinbase_output(block, consensus_manager, factories)?;
    check_coinbase_output_features(&block.body)?;

    Ok(())
}

fn check_coinbase_output_features(body: &AggregateBody) -> Result<(), ValidationError> {
    body.verify_non_coinbase_has_coinbase_extra_empty()
        .map_err(ValidationError::from)
}

fn validate_block_aggregate_body(
    block: &Block,
    validator: &AggregateBodyInternalConsistencyValidator,
    consensus_manager: &ConsensusManager,
) -> Result<(), ValidationError> {
    let offset = &block.header.total_kernel_offset;
    let script_offset = &block.header.total_script_offset;
    let total_coinbase = consensus_manager
        .calculate_coinbase_and_fees(block.header.height, block.body.kernels())
        .map_err(|err| {
            warn!(
                target: LOG_TARGET,
                "Validation failed on block:{}:{:?}",
                block.hash().to_hex(),
                err
            );
            ValidationError::CoinbaseExceedsMaxLimit
        })?;
    validator
        .validate(
            &block.body,
            offset,
            script_offset,
            Some(total_coinbase),
            Some(block.header.prev_hash),
            block.header.height,
        )
        .map_err(|err| {
            warn!(
                target: LOG_TARGET,
                "Validation failed on block:{}:{:?}",
                block.hash().to_hex(),
                err
            );
            err
        })?;

    Ok(())
}

fn check_coinbase_output(
    block: &Block,
    rules: &ConsensusManager,
    factories: &CryptoFactories,
) -> Result<(), ValidationError> {
    let total_coinbase = rules
        .calculate_coinbase_and_fees(block.header.height, block.body.kernels())
        .map_err(|err| {
            warn!(
                target: LOG_TARGET,
                "Validation failed on block:{}:{:?}",
                block.hash().to_hex(),
                err
            );
            ValidationError::CoinbaseExceedsMaxLimit
        })?;

    block
        .check_coinbase_output(
            total_coinbase,
            rules.consensus_constants(block.header.height),
            factories,
        )
        .map_err(ValidationError::from)
}
