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

use super::BlockBodyInternalConsistencyValidator;
use crate::{
    blocks::ChainBlock,
    chain_storage::{self, BlockchainBackend},
    consensus::ConsensusManager,
    transactions::CryptoFactories,
    validation::{
        aggregate_body::AggregateBodyChainLinkedValidator,
        helpers::check_mmr_roots,
        CandidateBlockValidator,
        ValidationError,
    },
};

pub struct BlockBodyFullValidator {
    consensus_manager: ConsensusManager,
    block_internal_validator: BlockBodyInternalConsistencyValidator,
    aggregate_body_chain_validator: AggregateBodyChainLinkedValidator,
}

impl BlockBodyFullValidator {
    pub fn new(rules: ConsensusManager) -> Self {
        let factories = CryptoFactories::default();
        let block_internal_validator = BlockBodyInternalConsistencyValidator::new(rules.clone(), true, factories);
        let aggregate_body_chain_validator = AggregateBodyChainLinkedValidator::new(rules.clone());
        Self {
            consensus_manager: rules,
            block_internal_validator,
            aggregate_body_chain_validator,
        }
    }

    pub fn validate<B: BlockchainBackend>(
        &self,
        backend: &B,
        block: &ChainBlock,
        metadata: &ChainMetadata,
    ) -> Result<(), ValidationError> {
        // TODO: this validation should not be neccesary, as it's overlaps with header validation
        // but some of the test break without it
        validate_block_metadata(block, metadata)?;

        // validate the internal consistency of the block body
        self.block_internal_validator.validate(block.block())?;

        // validate the block body against the current db
        let body = &block.block().body;
        let height = block.header().height;
        self.aggregate_body_chain_validator.validate(body, height, backend)?;

        // validate the merkle mountain range roots
        let mmr_roots = chain_storage::calculate_mmr_roots(backend, &self.consensus_manager, block.block())?;
        check_mmr_roots(&block.block().header, &mmr_roots)?;

        Ok(())
    }
}

impl<B: BlockchainBackend> CandidateBlockValidator<B> for BlockBodyFullValidator {
    fn validate_body(&self, backend: &B, block: &ChainBlock, metadata: &ChainMetadata) -> Result<(), ValidationError> {
        self.validate(backend, block, metadata)
    }
}

fn validate_block_metadata(block: &ChainBlock, metadata: &ChainMetadata) -> Result<(), ValidationError> {
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

    Ok(())
}
