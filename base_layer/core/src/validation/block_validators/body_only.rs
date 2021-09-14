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

use super::LOG_TARGET;
use crate::{
    chain_storage,
    chain_storage::{BlockchainBackend, ChainBlock},
    crypto::tari_utilities::hex::Hex,
    validation::{helpers, PostOrphanBodyValidation, ValidationError},
};
use log::*;
use tari_common_types::chain_metadata::ChainMetadata;

/// This validator tests whether a candidate block is internally consistent.
/// This does not check that the orphan block has the correct mined height of utxos

/// This validator checks whether a block satisfies *all* consensus rules. If a block passes this validator, it is the
/// next block on the blockchain.
#[derive(Default)]
pub struct BodyOnlyValidator;

impl<B: BlockchainBackend> PostOrphanBodyValidation<B> for BodyOnlyValidator {
    /// The consensus checks that are done (in order of cheapest to verify to most expensive):
    /// 1. Does the block satisfy the stateless checks?
    /// 1. Are all inputs currently in the UTXO set?
    /// 1. Are all inputs and outputs not in the STXO set?
    /// 1. Are the block header MMR roots valid?
    fn validate_body_for_valid_orphan(
        &self,
        block: &ChainBlock,
        backend: &B,
        metadata: &ChainMetadata,
    ) -> Result<(), ValidationError> {
        if block.header().height != metadata.height_of_longest_chain() + 1 {
            return Err(ValidationError::IncorrectNextTipHeight {
                expected: metadata.height_of_longest_chain() + 1,
                block_height: block.height(),
            });
        }
        if block.header().prev_hash != *metadata.best_block() {
            return Err(ValidationError::IncorrectPreviousHash {
                expected: metadata.best_block().to_hex(),
                block_hash: block.hash().to_hex(),
            });
        }

        let block_id = format!("block #{} ({})", block.header().height, block.hash().to_hex());
        helpers::check_inputs_are_utxos(backend, &block.block().body)?;
        helpers::check_not_duplicate_txos(backend, &block.block().body)?;
        trace!(
            target: LOG_TARGET,
            "Block validation: All inputs and outputs are valid for {}",
            block_id
        );
        let mmr_roots = chain_storage::calculate_mmr_roots(backend, block.block())?;
        helpers::check_mmr_roots(block.header(), &mmr_roots)?;
        trace!(
            target: LOG_TARGET,
            "Block validation: MMR roots are valid for {}",
            block_id
        );
        debug!(target: LOG_TARGET, "Block validation: Block is VALID for {}", block_id);
        Ok(())
    }
}
