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

use tari_common_types::chain_metadata::ChainMetadata;
use tari_utilities::hex::Hex;

use super::BlockInternalConsistencyValidator;
use crate::{
    blocks::ChainBlock,
    chain_storage::BlockchainBackend,
    consensus::ConsensusManager,
    transactions::CryptoFactories,
    validation::{aggregate_body::AggregateBodyChainLinkedValidator, CandidateBlockValidator, ValidationError},
};

pub struct BodyOnlyValidator {
    block_internal_validator: BlockInternalConsistencyValidator,
    aggregate_body_chain_validator: AggregateBodyChainLinkedValidator,
}

impl BodyOnlyValidator {
    pub fn new(rules: ConsensusManager) -> Self {
        let factories = CryptoFactories::default();
        let block_internal_validator = BlockInternalConsistencyValidator::new(rules.clone(), true, factories);
        let aggregate_body_chain_validator = AggregateBodyChainLinkedValidator::new(rules);
        Self {
            block_internal_validator,
            aggregate_body_chain_validator,
        }
    }
}

impl<B: BlockchainBackend> CandidateBlockValidator<B> for BodyOnlyValidator {
    fn validate_body(&self, backend: &B, block: &ChainBlock, metadata: &ChainMetadata) -> Result<(), ValidationError> {
        // TODO: these validations should not be neccesary, as they are part of header validation
        // but some of the test break because of it
        if block.header().prev_hash != *metadata.best_block() {
            return Err(ValidationError::IncorrectPreviousHash {
                expected: metadata.best_block().to_hex(),
                block_hash: block.hash().to_hex(),
            });
        }
        if block.height() != metadata.height_of_longest_chain() + 1 {
            return Err(ValidationError::IncorrectHeight {
                expected: metadata.height_of_longest_chain() + 1,
                block_height: block.height(),
            });
        }

        let height = block.header().height;
        let body = &block.block().body;

        self.block_internal_validator.validate(block.block())?;
        self.aggregate_body_chain_validator.validate(body, height, backend)?;

        Ok(())
    }
}
