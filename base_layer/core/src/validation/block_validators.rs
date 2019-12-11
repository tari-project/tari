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
    blocks::{blockheader::BlockHeader, Block, BlockValidationError},
    chain_storage::{BlockchainBackend, BlockchainDatabase},
    consensus::{ConsensusConstants, ConsensusManager},
    proof_of_work::PowError,
    validation::{Validation, ValidationError},
};
use std::sync::Arc;
use tari_transactions::types::CryptoFactories;

/// This validator tests whether a candidate block is internally consistent
pub struct StatelessValidator {
    factories: Arc<CryptoFactories>,
}

impl StatelessValidator {
    pub fn new(factories: Arc<CryptoFactories>) -> Self {
        Self { factories }
    }
}

impl<B: BlockchainBackend> Validation<Block, B> for StatelessValidator {
    /// The consensus checks that are done (in order of cheapest to verify to most expensive):
    /// 1. Is there precisely one Coinbase output and is it correctly defined?
    /// 1. Is the accounting correct?
    /// 1. Are all inputs allowed to be spend (Are the feature flags satisfied)
    fn validate(&self, block: &Block) -> Result<(), ValidationError> {
        check_coinbase_output(block)?;
        // Check that the inputs are are allowed to be spent
        block.check_stxo_rules().map_err(BlockValidationError::from)?;
        Ok(())
    }
}

/// This block checks whether a block satisfies *all* consensus rules. If a block passes this validator, it is the
/// next block on the blockchain.
pub struct FullConsensusValidator<B: BlockchainBackend> {
    rules: ConsensusManager<B>,
    factories: Arc<CryptoFactories>,
    db: Option<BlockchainDatabase<B>>,
}

impl<B: BlockchainBackend> FullConsensusValidator<B>
where B: BlockchainBackend
{
    pub fn new(rules: ConsensusManager<B>, factories: Arc<CryptoFactories>) -> Self {
        Self {
            rules,
            factories,
            db: None,
        }
    }

    fn db(&self) -> Result<BlockchainDatabase<B>, ValidationError> {
        match &self.db {
            Some(db) => Ok(db.clone()),
            None => Err(ValidationError::NoDatabaseConfigured),
        }
    }
}

impl<B: BlockchainBackend> Validation<Block, B> for FullConsensusValidator<B> {
    /// The consensus checks that are done (in order of cheapest to verify to most expensive):
    /// 1. Does the block satisfy the stateless checks?
    /// 1. Are all inputs currently in the UTXO set?
    /// 1. Is the block header timestamp within range?
    /// 1. Is the Proof of Work valid?
    /// 1. Is the achieved difficulty of this block >= the target difficulty for this block?
    fn validate(&self, block: &Block) -> Result<(), ValidationError> {
        check_coinbase_output(block)?;
        block.check_stxo_rules().map_err(BlockValidationError::from)?;
        check_accounting_balance(block, self.rules.clone(), &self.factories)?;
        check_inputs_are_utxos(block, self.db()?)?;
        check_timestamp_range(&block.header, self.rules.clone())?;
        check_achieved_difficulty(&block.header, self.rules.clone())?; // Update function signature once diff adjuster is complete
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
        .map_err(ValidationError::from)
}

fn check_coinbase_output(block: &Block) -> Result<(), ValidationError> {
    block.check_coinbase_output().map_err(ValidationError::from)
}

fn check_inputs_are_utxos<B: BlockchainBackend>(
    _block: &Block,
    _db: BlockchainDatabase<B>,
) -> Result<(), ValidationError>
{
    // TODO --implement Issue #1092
    Ok(())
}

/// Calculates the achieved and target difficulties and compares them

fn check_achieved_difficulty<B: BlockchainBackend>(
    block_header: &BlockHeader,
    rules: ConsensusManager<B>,
) -> Result<(), ValidationError>
{
    let target = rules.get_target_difficulty(&block_header.pow.pow_algo).map_err(|_| {
        ValidationError::BlockError(BlockValidationError::ProofOfWorkError(PowError::InvalidProofOfWork))
    })?;
    let achieved = block_header.achieved_difficulty();
    if achieved < target {
        return Err(ValidationError::BlockError(BlockValidationError::ProofOfWorkError(
            PowError::AchievedDifficultyTooLow,
        )));
    }
    Ok(())
}

/// This function test that the block timestamp is less than the ftl and greater than the median timestamp
fn check_timestamp_range<B: BlockchainBackend>(
    block_header: &BlockHeader,
    rules: ConsensusManager<B>,
) -> Result<(), ValidationError>
{
    if block_header.timestamp > ConsensusConstants::current().ftl() {
        return Err(ValidationError::BlockError(BlockValidationError::InvalidTimestamp));
    }
    let median_timestamp = rules
        .get_median_timestamp()
        .map_err(|_| ValidationError::BlockError(BlockValidationError::InvalidTimestamp))?;
    if block_header.timestamp < median_timestamp {
        return Err(ValidationError::BlockError(BlockValidationError::InvalidTimestamp));
    }
    Ok(())
}
