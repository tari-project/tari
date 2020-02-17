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
    blocks::{
        blockheader::{BlockHeader, BlockHeaderValidationError},
        Block,
        BlockValidationError,
        NewBlockTemplate,
    },
    chain_storage::{BlockchainBackend, BlockchainDatabase},
    consensus::{ConsensusConstants, ConsensusManager},
    transactions::{transaction::OutputFlags, types::CryptoFactories},
    validation::{
        helpers::{check_achieved_difficulty_at_chain_tip, check_median_timestamp_at_chain_tip},
        Validation,
        ValidationError,
    },
};
use log::*;
use tari_crypto::tari_utilities::hash::Hashable;
pub const LOG_TARGET: &str = "c::val::block_validators";

/// This validator tests whether a candidate block is internally consistent
pub struct StatelessValidator {}

impl StatelessValidator {
    pub fn new() -> Self {
        Self {}
    }
}

impl<B: BlockchainBackend> Validation<Block, B> for StatelessValidator {
    /// The consensus checks that are done (in order of cheapest to verify to most expensive):
    /// 1. Is there precisely one Coinbase output and is it correctly defined?
    /// 1. Is the accounting correct?
    /// 1. Are all inputs allowed to be spent (Are the feature flags satisfied)
    fn validate(&self, block: &Block) -> Result<(), ValidationError> {
        check_coinbase_output(block)?;
        // Check that the inputs are are allowed to be spent
        block.check_stxo_rules().map_err(BlockValidationError::from)?;
        check_cut_through(block)?;
        Ok(())
    }
}

/// This block checks whether a block satisfies *all* consensus rules. If a block passes this validator, it is the
/// next block on the blockchain.
pub struct FullConsensusValidator<B: BlockchainBackend> {
    rules: ConsensusManager<B>,
    factories: CryptoFactories,
    db: BlockchainDatabase<B>,
}

impl<B: BlockchainBackend> FullConsensusValidator<B>
where B: BlockchainBackend
{
    pub fn new(rules: ConsensusManager<B>, factories: CryptoFactories, db: BlockchainDatabase<B>) -> Self {
        Self { rules, factories, db }
    }

    fn db(&self) -> Result<BlockchainDatabase<B>, ValidationError> {
        Ok(self.db.clone())
    }
}

impl<B: BlockchainBackend> Validation<Block, B> for FullConsensusValidator<B> {
    /// The consensus checks that are done (in order of cheapest to verify to most expensive):
    /// 1. Does the block satisfy the stateless checks?
    /// 1. Are all inputs currently in the UTXO set?
    /// 1. Is the block header timestamp less than the ftl?
    /// 1. Is the block header timestamp greater than the median timestamp?
    /// 1. Is the Proof of Work valid?
    /// 1. Is the achieved difficulty of this block >= the target difficulty for this block?
    fn validate(&self, block: &Block) -> Result<(), ValidationError> {
        check_coinbase_output(block)?;
        check_cut_through(block)?;
        block.check_stxo_rules().map_err(BlockValidationError::from)?;
        check_accounting_balance(block, self.rules.clone(), &self.factories)?;
        check_inputs_are_utxos(block, self.db()?)?;
        check_timestamp_ftl(&block.header)?;
        check_median_timestamp_at_chain_tip(&block.header, self.db()?, self.rules.clone())?;
        check_achieved_difficulty_at_chain_tip(&block.header, self.db()?, self.rules.clone())?; // Update function signature once diff adjuster is complete
        Ok(())
    }
}

//-------------------------------------     Block validator helper functions     -------------------------------------//
fn check_accounting_balance<B: BlockchainBackend>(
    block: &Block,
    rules: ConsensusManager<B>,
    factories: &CryptoFactories,
) -> Result<(), ValidationError>
{
    let offset = &block.header.total_kernel_offset;
    let total_coinbase = rules.calculate_coinbase_and_fees(block);
    block
        .body
        .validate_internal_consistency(&offset, total_coinbase, factories)
        .map_err(|e| ValidationError::TransactionError(e))
}

fn check_coinbase_output(block: &Block) -> Result<(), ValidationError> {
    block.check_coinbase_output().map_err(ValidationError::from)
}

/// This function checks that all inputs in the blocks are valid UTXO's to be spend
fn check_inputs_are_utxos<B: BlockchainBackend>(
    block: &Block,
    db: BlockchainDatabase<B>,
) -> Result<(), ValidationError>
{
    for utxo in block.body.inputs() {
        if !(utxo.features.flags.contains(OutputFlags::COINBASE_OUTPUT)) &&
            !(db.is_utxo(utxo.hash())).map_err(|e| ValidationError::CustomError(e.to_string()))?
        {
            warn!(
                target: LOG_TARGET,
                "Block validation failed because the block has invalid input: {}", utxo
            );
            return Err(ValidationError::BlockError(BlockValidationError::InvalidInput));
        }
    }
    Ok(())
}

/// This function tests that the block timestamp is less than the ftl.
fn check_timestamp_ftl(block_header: &BlockHeader) -> Result<(), ValidationError> {
    if block_header.timestamp > ConsensusConstants::current().ftl() {
        return Err(ValidationError::BlockHeaderError(
            BlockHeaderValidationError::InvalidTimestampFutureTimeLimit,
        ));
    }
    Ok(())
}

fn check_mmr_roots<B: BlockchainBackend>(block: &Block, db: BlockchainDatabase<B>) -> Result<(), ValidationError> {
    let template = NewBlockTemplate::from(block.clone());
    let tmp_block = db
        .calculate_mmr_roots(template)
        .map_err(|e| ValidationError::CustomError(e.to_string()))?;
    let tmp_header = &tmp_block.header;
    let header = &block.header;
    if header.kernel_mr != tmp_header.kernel_mr ||
        header.output_mr != tmp_header.output_mr ||
        header.range_proof_mr != tmp_header.range_proof_mr
    {
        Err(ValidationError::BlockError(BlockValidationError::MismatchedMmrRoots))
    } else {
        Ok(())
    }
}

fn check_cut_through(block: &Block) -> Result<(), ValidationError> {
    if !block.body.cut_through_check() {
        return Err(ValidationError::BlockError(BlockValidationError::NoCutThrough));
    }
    return Ok(());
}
