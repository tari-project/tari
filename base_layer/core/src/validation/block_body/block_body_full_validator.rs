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

use std::sync::{Arc, RwLock};

use log::error;
use tari_common_types::chain_metadata::ChainMetadata;
use tari_utilities::hex::Hex;

use super::BlockBodyInternalConsistencyValidator;
use crate::{
    blocks::{Block, ChainBlock},
    chain_storage::{self, BlockchainBackend, ChainStorageError},
    consensus::ConsensusManager,
    transactions::CryptoFactories,
    validation::{
        aggregate_body::AggregateBodyChainLinkedValidator,
        helpers::check_mmr_roots,
        BlockBodyValidator,
        CandidateBlockValidator,
        ValidationError,
    },
    OutputSmt,
};

const LOG_TARGET: &str = "c::val::block_body_full_validator";

pub struct BlockBodyFullValidator {
    consensus_manager: ConsensusManager,
    block_internal_validator: BlockBodyInternalConsistencyValidator,
    aggregate_body_chain_validator: AggregateBodyChainLinkedValidator,
}

impl BlockBodyFullValidator {
    pub fn new(rules: ConsensusManager, bypass_range_proof_verification: bool) -> Self {
        let factories = CryptoFactories::default();
        let block_internal_validator =
            BlockBodyInternalConsistencyValidator::new(rules.clone(), bypass_range_proof_verification, factories);
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
        block: &Block,
        metadata_option: Option<&ChainMetadata>,
        smt: Arc<RwLock<OutputSmt>>,
    ) -> Result<Block, ValidationError> {
        if let Some(metadata) = metadata_option {
            validate_block_metadata(block, metadata)?;
        }

        // validate the block body against the current db
        let body = &block.body;
        let height = block.header.height;
        // the inputs may be only references to outputs, that's why the validator returns a new body and we need a new
        // block
        let body = self.aggregate_body_chain_validator.validate(body, height, backend)?;
        let block = Block::new(block.header.clone(), body);

        // validate the internal consistency of the block body
        self.block_internal_validator.validate(&block)?;

        // validate the merkle mountain range roots+
        let mut output_smt = smt.write().map_err(|e| {
            error!(
                target: LOG_TARGET,
                "Validator could not get a write lock on the smt {:?}", e
            );
            ChainStorageError::AccessError("write lock on smt".into())
        })?;
        let mmr_roots = chain_storage::calculate_mmr_roots(backend, &self.consensus_manager, &block, &mut output_smt)?;
        check_mmr_roots(&block.header, &mmr_roots)?;

        Ok(block)
    }
}

impl<B: BlockchainBackend> CandidateBlockValidator<B> for BlockBodyFullValidator {
    fn validate_body_with_metadata(
        &self,
        backend: &B,
        block: &ChainBlock,
        metadata: &ChainMetadata,
        smt: Arc<RwLock<OutputSmt>>,
    ) -> Result<(), ValidationError> {
        self.validate(backend, block.block(), Some(metadata), smt)?;
        Ok(())
    }
}

impl<B: BlockchainBackend> BlockBodyValidator<B> for BlockBodyFullValidator {
    fn validate_body(&self, backend: &B, block: &Block, smt: Arc<RwLock<OutputSmt>>) -> Result<Block, ValidationError> {
        self.validate(backend, block, None, smt)
    }
}

fn validate_block_metadata(block: &Block, metadata: &ChainMetadata) -> Result<(), ValidationError> {
    if block.header.prev_hash != *metadata.best_block_hash() {
        return Err(ValidationError::IncorrectPreviousHash {
            expected: metadata.best_block_hash().to_hex(),
            block_hash: block.hash().to_hex(),
        });
    }
    if block.header.height != metadata.best_block_height() + 1 {
        return Err(ValidationError::IncorrectHeight {
            expected: metadata.best_block_height() + 1,
            block_height: block.header.height,
        });
    }

    Ok(())
}
