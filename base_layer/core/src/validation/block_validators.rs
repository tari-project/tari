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
    chain_storage::{calculate_mmr_roots, is_utxo, BlockchainBackend},
    consensus::{ConsensusConstants, ConsensusManager},
    transactions::{transaction::OutputFlags, types::CryptoFactories},
    validation::{
        helpers::{check_achieved_and_target_difficulty, check_median_timestamp},
        StatelessValidation,
        Validation,
        ValidationError,
    },
};
use log::*;
use tari_crypto::tari_utilities::{hash::Hashable, hex::Hex};

pub const LOG_TARGET: &str = "c::val::block_validators";

/// This validator tests whether a candidate block is internally consistent
#[derive(Clone)]
pub struct StatelessBlockValidator {
    consensus_constants: ConsensusConstants,
}

impl StatelessBlockValidator {
    pub fn new(consensus_constants: &ConsensusConstants) -> Self {
        Self {
            consensus_constants: consensus_constants.clone(),
        }
    }
}

impl StatelessValidation<Block> for StatelessBlockValidator {
    /// The consensus checks that are done (in order of cheapest to verify to most expensive):
    /// 1. Is the block weight of the block under the prescribed limit?
    /// 1. Does it contain only unique inputs and outputs?
    /// 1. Where all the rules for the spent outputs followed?
    /// 1. Was cut through applied in the block?
    fn validate(&self, block: &Block) -> Result<(), ValidationError> {
        let block_id = format!("block #{} ({})", block.header.height, block.hash().to_hex());
        check_coinbase_output(block, &self.consensus_constants)?;
        trace!(target: LOG_TARGET, "SV - Coinbase output is ok for {} ", &block_id);
        check_block_weight(block, &self.consensus_constants)?;
        trace!(target: LOG_TARGET, "SV - Block weight is ok for {} ", &block_id);
        check_duplicate_transactions_inputs(block)?;
        trace!(
            target: LOG_TARGET,
            "SV - No duplicate inputs or outputs for {} ",
            &block_id
        );
        // Check that the inputs are are allowed to be spent
        block.check_stxo_rules().map_err(BlockValidationError::from)?;
        trace!(target: LOG_TARGET, "SV - Output constraints are ok for {} ", &block_id);
        check_cut_through(block)?;
        trace!(target: LOG_TARGET, "SV - Cut-through is ok for {} ", &block_id);
        info!(
            target: LOG_TARGET,
            "{} has PASSED stateless VALIDATION check.", &block_id
        );
        Ok(())
    }
}

/// This validator checks whether a block satisfies *all* consensus rules. If a block passes this validator, it is the
/// next block on the blockchain.
pub struct FullConsensusValidator {
    rules: ConsensusManager,
    factories: CryptoFactories,
}

impl FullConsensusValidator {
    pub fn new(rules: ConsensusManager, factories: CryptoFactories) -> Self {
        Self { rules, factories }
    }
}

impl<B: BlockchainBackend> Validation<Block, B> for FullConsensusValidator {
    /// The consensus checks that are done (in order of cheapest to verify to most expensive):
    /// 1. Does the block satisfy the stateless checks?
    /// 1. Are all inputs currently in the UTXO set?
    /// 1. Are the block header MMR roots valid?
    /// 1. Is the block header timestamp less than the ftl?
    /// 1. Is the block header timestamp greater than the median timestamp?
    /// 1. Is the Proof of Work valid?
    /// 1. Is the achieved difficulty of this block >= the target difficulty for this block?
    fn validate(&self, block: &Block, db: &B) -> Result<(), ValidationError> {
        let block_id = format!("block #{} ({})", block.header.height, block.hash().to_hex());
        check_coinbase_output(block, &self.rules.consensus_constants())?;
        trace!(target: LOG_TARGET, "FCV - Coinbase output ok for {}", &block_id);
        check_block_weight(block, &self.rules.consensus_constants())?;
        trace!(target: LOG_TARGET, "FCV - Block weight ok for {}", &block_id);
        check_duplicate_transactions_inputs(block)?;
        trace!(
            target: LOG_TARGET,
            "FCV - No duplicate inputs or outputs for {} ",
            &block_id
        );
        check_cut_through(block)?;
        trace!(target: LOG_TARGET, "FCV - Cut-though correct for {}", &block_id);
        block.check_stxo_rules().map_err(BlockValidationError::from)?;
        trace!(target: LOG_TARGET, "FCV - STxO rules correct for {}", &block_id);
        check_accounting_balance(block, self.rules.clone(), &self.factories)?;
        trace!(target: LOG_TARGET, "FCV - accounting balance correct for {}", &block_id);
        check_inputs_are_utxos(block, db)?;
        trace!(target: LOG_TARGET, "FCV - All inputs are valid for {}", &block_id);
        check_mmr_roots(block, db)?;
        trace!(target: LOG_TARGET, "FCV - MMR roots are valid for {}", &block_id);
        check_timestamp_ftl(&block.header, &self.rules)?;
        trace!(target: LOG_TARGET, "FCV - FTL timestamp is ok for {} ", &block_id);
        let tip_height = db
            .fetch_metadata()
            .map_err(|e| ValidationError::CustomError(e.to_string()))?
            .height_of_longest_chain
            .unwrap_or(0);
        check_median_timestamp(db, &block.header, tip_height, self.rules.clone())?;
        trace!(target: LOG_TARGET, "FCV - Median timestamp is ok for {} ", &block_id);
        check_achieved_and_target_difficulty(db, &block.header, tip_height, self.rules.clone())?;
        trace!(target: LOG_TARGET, "FCV - Achieved difficulty is ok for {} ", &block_id);
        info!(target: LOG_TARGET, "FCV - Block is VALID for {}", &block_id);
        Ok(())
    }
}

//-------------------------------------     Block validator helper functions     -------------------------------------//
fn check_accounting_balance(
    block: &Block,
    rules: ConsensusManager,
    factories: &CryptoFactories,
) -> Result<(), ValidationError>
{
    let offset = &block.header.total_kernel_offset;
    let total_coinbase = rules.calculate_coinbase_and_fees(block);
    block
        .body
        .validate_internal_consistency(&offset, total_coinbase, factories)
        .map_err(|err| {
            warn!(
                target: LOG_TARGET,
                "Internal validation failed on block:{}:{}",
                block.hash().to_hex(),
                err
            );
            ValidationError::TransactionError(err)
        })
}

fn check_block_weight(block: &Block, consensus_constants: &ConsensusConstants) -> Result<(), ValidationError> {
    // The genesis block has a larger weight than other blocks may have so we have to exclude it here
    if block.body.calculate_weight() <= consensus_constants.get_max_block_transaction_weight() ||
        block.header.height == 0
    {
        Ok(())
    } else {
        Err(BlockValidationError::BlockTooLarge).map_err(ValidationError::from)
    }
}

fn check_coinbase_output(block: &Block, consensus_constants: &ConsensusConstants) -> Result<(), ValidationError> {
    trace!(
        target: LOG_TARGET,
        "Checking coinbase output on block with hash {}",
        block.hash().to_hex()
    );
    block
        .check_coinbase_output(consensus_constants)
        .map_err(ValidationError::from)
}

// This function checks for duplicate inputs and outputs. There should be no duplicate inputs or outputs in a block
fn check_duplicate_transactions_inputs(block: &Block) -> Result<(), ValidationError> {
    trace!(
        target: LOG_TARGET,
        "Checking duplicate inputs and outputs on block with hash {}",
        block.hash().to_hex()
    );
    for i in 1..block.body.inputs().len() {
        if block.body.inputs()[i..].contains(&block.body.inputs()[i - 1]) {
            return Err(ValidationError::CustomError("Duplicate Input".to_string()));
        }
    }
    for i in 1..block.body.outputs().len() {
        if block.body.outputs()[i..].contains(&block.body.outputs()[i - 1]) {
            return Err(ValidationError::CustomError("Duplicate Output".to_string()));
        }
    }
    Ok(())
}

/// This function checks that all inputs in the blocks are valid UTXO's to be spend
fn check_inputs_are_utxos<B: BlockchainBackend>(block: &Block, db: &B) -> Result<(), ValidationError> {
    for utxo in block.body.inputs() {
        if !(utxo.features.flags.contains(OutputFlags::COINBASE_OUTPUT)) &&
            !(is_utxo(db, utxo.hash())).map_err(|e| ValidationError::CustomError(e.to_string()))?
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
fn check_timestamp_ftl(
    block_header: &BlockHeader,
    consensus_manager: &ConsensusManager,
) -> Result<(), ValidationError>
{
    if block_header.timestamp > consensus_manager.consensus_constants().ftl() {
        warn!(
            target: LOG_TARGET,
            "Invalid Future Time Limit on block:{}",
            block_header.hash().to_hex()
        );
        return Err(ValidationError::BlockHeaderError(
            BlockHeaderValidationError::InvalidTimestampFutureTimeLimit,
        ));
    }
    Ok(())
}

fn check_mmr_roots<B: BlockchainBackend>(block: &Block, db: &B) -> Result<(), ValidationError> {
    let template = NewBlockTemplate::from(block.clone());
    let tmp_block = calculate_mmr_roots(db, template).map_err(|e| ValidationError::CustomError(e.to_string()))?;
    let tmp_header = &tmp_block.header;
    let header = &block.header;
    if header.kernel_mr != tmp_header.kernel_mr {
        warn!(
            target: LOG_TARGET,
            "Block header kernel MMR roots in {} do not match calculated roots",
            block.hash().to_hex()
        );
        return Err(ValidationError::BlockError(BlockValidationError::MismatchedMmrRoots));
    };
    if header.output_mr != tmp_header.output_mr {
        warn!(
            target: LOG_TARGET,
            "Block header output MMR roots in {} do not match calculated roots",
            block.hash().to_hex()
        );
        return Err(ValidationError::BlockError(BlockValidationError::MismatchedMmrRoots));
    };
    if header.range_proof_mr != tmp_header.range_proof_mr {
        warn!(
            target: LOG_TARGET,
            "Block header range_proof MMR roots in {} do not match calculated roots",
            block.hash().to_hex()
        );
        return Err(ValidationError::BlockError(BlockValidationError::MismatchedMmrRoots));
    };
    Ok(())
}

fn check_cut_through(block: &Block) -> Result<(), ValidationError> {
    trace!(
        target: LOG_TARGET,
        "Checking cut through on block with hash {}",
        block.hash().to_hex()
    );
    if !block.body.cut_through_check() {
        warn!(
            target: LOG_TARGET,
            "Block validation for {} failed: block no cut through",
            block.hash().to_hex()
        );
        return Err(ValidationError::BlockError(BlockValidationError::NoCutThrough));
    }
    Ok(())
}
