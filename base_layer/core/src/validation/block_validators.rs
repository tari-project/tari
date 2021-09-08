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
use crate::{
    blocks::Block,
    chain_storage::{BlockchainBackend, ChainBlock},
    consensus::ConsensusManager,
    transactions::CryptoFactories,
    validation::{
        helpers::{
            check_accounting_balance,
            check_block_weight,
            check_coinbase_output,
            check_inputs_are_utxos,
            check_mmr_roots,
            check_not_duplicate_txos,
            check_sorting_and_duplicates,
        },
        traits::PostOrphanBodyValidation,
        BlockSyncBodyValidation,
        OrphanValidation,
        ValidationError,
    },
};
use std::marker::PhantomData;

use log::*;
use tari_crypto::tari_utilities::{hash::Hashable, hex::Hex};

use tari_common_types::chain_metadata::ChainMetadata;

pub const LOG_TARGET: &str = "c::val::block_validators";

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

impl OrphanValidation for OrphanBlockValidator {
    /// The consensus checks that are done (in order of cheapest to verify to most expensive):
    /// 1. Is the block weight of the block under the prescribed limit?
    /// 1. Does it contain only unique inputs and outputs?
    /// 1. Where all the rules for the spent outputs followed?
    /// 1. Is there precisely one Coinbase output and is it correctly defined with the correct amount?
    /// 1. Is the accounting correct?
    fn validate(&self, block: &Block) -> Result<(), ValidationError> {
        if block.header.height == 0 {
            warn!(target: LOG_TARGET, "Attempt to validate genesis block");
            return Err(ValidationError::ValidatingGenesis);
        }

        let block_id = if cfg!(debug_assertions) {
            format!("block #{} ({})", block.header.height, block.hash().to_hex())
        } else {
            format!("block #{}", block.header.height)
        };
        trace!(target: LOG_TARGET, "Validating {}", block_id);

        check_block_weight(&block, &self.rules.consensus_constants(block.header.height))?;
        trace!(target: LOG_TARGET, "SV - Block weight is ok for {} ", &block_id);

        trace!(
            target: LOG_TARGET,
            "Checking duplicate inputs and outputs on {}",
            block_id
        );
        check_sorting_and_duplicates(&block.body)?;
        trace!(
            target: LOG_TARGET,
            "SV - No duplicate inputs or outputs for {} ",
            &block_id
        );

        // Check that the inputs are are allowed to be spent
        block.check_spend_rules()?;
        trace!(target: LOG_TARGET, "SV - Output constraints are ok for {} ", &block_id);
        check_coinbase_output(block, &self.rules, &self.factories)?;
        trace!(target: LOG_TARGET, "SV - Coinbase output is ok for {} ", &block_id);
        check_accounting_balance(
            block,
            &self.rules,
            self.bypass_range_proof_verification,
            &self.factories,
        )?;
        trace!(target: LOG_TARGET, "SV - accounting balance correct for {}", &block_id);
        debug!(
            target: LOG_TARGET,
            "{} has PASSED stateless VALIDATION check.", &block_id
        );
        Ok(())
    }
}

/// This validator tests whether a candidate block is internally consistent.
/// This does not check that the orphan block has the correct mined height of utxos

/// This validator checks whether a block satisfies *all* consensus rules. If a block passes this validator, it is the
/// next block on the blockchain.
#[derive(Default)]
pub struct BodyOnlyValidator {}

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
        check_inputs_are_utxos(&block.block().body, backend)?;
        check_not_duplicate_txos(&block.block().body, backend)?;
        trace!(
            target: LOG_TARGET,
            "Block validation: All inputs and outputs are valid for {}",
            block_id
        );
        check_mmr_roots(block.block(), backend)?;
        trace!(
            target: LOG_TARGET,
            "Block validation: MMR roots are valid for {}",
            block_id
        );
        debug!(target: LOG_TARGET, "Block validation: Block is VALID for {}", block_id);
        Ok(())
    }
}

/// This validator checks whether a block satisfies consensus rules.
/// It implements two validators: one for the `BlockHeader` and one for `Block`. The `Block` validator ONLY validates
/// the block body using the header. It is assumed that the `BlockHeader` has already been validated.
pub struct BlockValidator<B: BlockchainBackend> {
    rules: ConsensusManager,
    bypass_range_proof_verification: bool,
    factories: CryptoFactories,
    phantom_data: PhantomData<B>,
}

impl<B: BlockchainBackend> BlockValidator<B> {
    pub fn new(rules: ConsensusManager, bypass_range_proof_verification: bool, factories: CryptoFactories) -> Self {
        Self {
            rules,
            factories,
            bypass_range_proof_verification,
            phantom_data: Default::default(),
        }
    }
}

impl<B: BlockchainBackend> BlockSyncBodyValidation<B> for BlockValidator<B> {
    /// The following consensus checks are done:
    /// 1. Does the block satisfy the stateless checks?
    /// 1. Are the block header MMR roots valid?
    fn validate_body(&self, block: &Block, backend: &B) -> Result<(), ValidationError> {
        let block_id = format!("block #{}", block.header.height);
        trace!(target: LOG_TARGET, "Validating {}", block_id);

        let constants = self.rules.consensus_constants(block.header.height);
        check_block_weight(block, &constants)?;
        trace!(target: LOG_TARGET, "SV - Block weight is ok for {} ", &block_id);
        // Check that the inputs are are allowed to be spent
        block.check_spend_rules()?;
        trace!(target: LOG_TARGET, "SV - Output constraints are ok for {} ", &block_id);

        check_sorting_and_duplicates(&block.body)?;
        check_inputs_are_utxos(&block.body, backend)?;
        check_not_duplicate_txos(&block.body, backend)?;
        check_coinbase_output(block, &self.rules, &self.factories)?;
        trace!(
            target: LOG_TARGET,
            "Block validation: All inputs and outputs are valid for {}",
            block_id
        );

        check_accounting_balance(
            block,
            &self.rules,
            self.bypass_range_proof_verification,
            &self.factories,
        )?;
        trace!(target: LOG_TARGET, "SV - accounting balance correct for {}", &block_id);
        debug!(
            target: LOG_TARGET,
            "{} has PASSED stateless VALIDATION check.", &block_id
        );

        check_mmr_roots(&block, backend)?;
        trace!(
            target: LOG_TARGET,
            "Block validation: MMR roots are valid for {}",
            block_id
        );

        debug!(target: LOG_TARGET, "Block validation: Block is VALID for {}", block_id);
        Ok(())
    }
}
