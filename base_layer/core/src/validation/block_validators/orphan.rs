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
use log::*;
use tari_utilities::hex::Hex;

use super::LOG_TARGET;
use crate::{
    blocks::Block,
    consensus::{ConsensusConstants, ConsensusManager},
    transactions::{aggregated_body::AggregateBody, CryptoFactories},
    validation::{
        aggregate_body::AggregateBodyInternalConsistencyValidator,
        helpers::{check_coinbase_output, check_sorting_and_duplicates},
        InternalConsistencyValidator,
        ValidationError,
    },
};

/// This validator tests whether a candidate block is internally consistent
#[derive(Clone)]
pub struct OrphanBlockValidator {
    rules: ConsensusManager,
    bypass_range_proof_verification: bool,
    factories: CryptoFactories,
}

impl OrphanBlockValidator {
    pub fn new(rules: ConsensusManager, bypass_range_proof_verification: bool, factories: CryptoFactories) -> Self {
        Self {
            rules,
            bypass_range_proof_verification,
            factories,
        }
    }
}

impl InternalConsistencyValidator for OrphanBlockValidator {
    fn validate_internal_consistency(&self, block: &Block) -> Result<(), ValidationError> {
        // TODO: maybe some/all of these validations should be moved to AggregateBodyInternalConsistencyValidator
        // but many test fails in that case, need to take a look why
        if block.header.height == 0 {
            warn!(target: LOG_TARGET, "Attempt to validate genesis block");
            return Err(ValidationError::ValidatingGenesis);
        }
        check_sorting_and_duplicates(&block.body)?;
        check_coinbase_output(block, &self.rules, &self.factories)?;
        check_output_features(&block.body, self.rules.consensus_constants(block.header.height))?;

        // reusing the AggregateBodyInternalConsistencyValidator
        let offset = &block.header.total_kernel_offset;
        let script_offset = &block.header.total_script_offset;
        let total_coinbase = self
            .rules
            .calculate_coinbase_and_fees(block.header.height, block.body.kernels());
        let body_validator = AggregateBodyInternalConsistencyValidator::new(
            self.bypass_range_proof_verification,
            self.rules.clone(),
            self.factories.clone(),
        );
        body_validator
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
}

fn check_output_features(
    body: &AggregateBody,
    consensus_constants: &ConsensusConstants,
) -> Result<(), ValidationError> {
    let max_coinbase_metadata_size = consensus_constants.coinbase_output_features_extra_max_length();
    body.check_output_features(max_coinbase_metadata_size)
        .map_err(ValidationError::from)
}
