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
        chain_header::ChainHeader,
        Block,
        BlockValidationError,
        NewBlockTemplate,
    },
    chain_storage::{calculate_mmr_roots, BlockchainBackend, DbKey},
    consensus::{ConsensusConstants, ConsensusManager},
    transactions::types::CryptoFactories,
    validation::{
        helpers,
        helpers::{
            check_accounting_balance,
            check_coinbase_output,
            check_cut_through,
            check_header_timestamp_greater_than_median,
            check_pow_data,
            check_target_difficulty,
            check_timestamp_ftl,
            is_all_unique_and_sorted,
        },
        StatefulValidation,
        Validation,
        ValidationConvert,
        ValidationError,
    },
};
use log::*;
use tari_crypto::tari_utilities::{hash::Hashable, hex::Hex};

pub const LOG_TARGET: &str = "c::val::block_validators";

/// This validator tests whether a candidate block is internally consistent
#[derive(Clone)]
pub struct StatelessBlockValidator {
    rules: ConsensusManager,
    factories: CryptoFactories,
}

impl StatelessBlockValidator {
    pub fn new(rules: ConsensusManager, factories: CryptoFactories) -> Self {
        Self { rules, factories }
    }
}

impl Validation<Block> for StatelessBlockValidator {
    /// The consensus checks that are done (in order of cheapest to verify to most expensive):
    /// 1. Is the block weight of the block under the prescribed limit?
    /// 1. Does it contain only unique inputs and outputs?
    /// 1. Where all the rules for the spent outputs followed?
    /// 1. Was cut through applied in the block?
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

        helpers::check_block_weight(&block, &self.rules.consensus_constants(block.header.height))?;
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
        block.check_stxo_rules()?;
        trace!(target: LOG_TARGET, "SV - Output constraints are ok for {} ", &block_id);
        check_cut_through(block)?;
        trace!(target: LOG_TARGET, "SV - Cut-through is ok for {} ", &block_id);
        check_coinbase_output(block, &self.rules, &self.factories)?;
        trace!(target: LOG_TARGET, "SV - Coinbase output is ok for {} ", &block_id);
        check_accounting_balance(block, &self.rules, &self.factories)?;
        trace!(target: LOG_TARGET, "SV - accounting balance correct for {}", &block_id);
        debug!(
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
}

impl FullConsensusValidator {
    pub fn new(rules: ConsensusManager) -> Self {
        Self { rules }
    }

    /// Calculates the achieved and target difficulties at the specified height and compares them.
    fn check_achieved_and_target_difficulty<B: BlockchainBackend>(
        &self,
        db: &B,
        block_header: &BlockHeader,
    ) -> Result<(), ValidationError>
    {
        let pow_algo = block_header.pow.pow_algo;
        debug!(
            target: LOG_TARGET,
            "check_achieved_and_target_difficulty: pow_algo = {}, height = {}", pow_algo, block_header.height,
        );

        let target = if block_header.height == 0 {
            1.into()
        } else {
            let height = block_header.height;
            let mut target_difficulties = self.rules.new_target_difficulty(pow_algo, height);
            // Fetch the target difficulty window for `height - 1` to 0 (or until there are enough data points for the
            // pow algo)
            // TODO: This should be removed in favour of `BlockchainDatabase::fetch_target_difficulty`
            for height in (0..height).rev() {
                let header = fetch_header(&*db, height)?;
                if header.pow.pow_algo == pow_algo {
                    target_difficulties.add_front(header.timestamp(), header.target_difficulty());
                    if target_difficulties.is_full() {
                        break;
                    }
                }
            }

            // This is assertion cannot fail because fetch_header returns an error if a header is not found and the loop
            // always runs at least once (even if height == 0)
            debug_assert_eq!(
                target_difficulties.is_empty(),
                false,
                "fetch_target_difficulties returned an empty vec. "
            );

            target_difficulties.calculate()
        };

        check_target_difficulty(block_header, target)?;

        Ok(())
    }

    /// This function tests that the block timestamp is greater than the median timestamp at the specified height.
    fn check_median_timestamp<B: BlockchainBackend>(
        &self,
        db: &B,
        block_header: &BlockHeader,
    ) -> Result<(), ValidationError>
    {
        if block_header.height == 0 || self.rules.get_genesis_block_hash() == block_header.hash() {
            return Ok(()); // Its the genesis block, so we dont have to check median
        }

        let height = block_header.height - 1;
        let min_height = height.saturating_sub(
            self.rules
                .consensus_constants(block_header.height)
                .get_median_timestamp_count() as u64,
        );
        let timestamps = fetch_headers(db, min_height, height)?
            .iter()
            .map(|h| h.timestamp)
            .collect::<Vec<_>>();

        check_header_timestamp_greater_than_median(block_header, &timestamps)?;

        Ok(())
    }
}

impl<B: BlockchainBackend> StatefulValidation<Block, B> for FullConsensusValidator {
    /// The consensus checks that are done (in order of cheapest to verify to most expensive):
    /// 1. Does the block satisfy the stateless checks?
    /// 1. Are all inputs currently in the UTXO set?
    /// 1. Are all inputs and outputs not in the STXO set?
    /// 1. Are the block header MMR roots valid?
    /// 1. Is the block header timestamp less than the ftl?
    /// 1. Is the block header timestamp greater than the median timestamp?
    /// 1. Is the Proof of Work valid?
    /// 1. Is the achieved difficulty of this block >= the target difficulty for this block?
    fn validate(&self, block: &Block, backend: &B) -> Result<(), ValidationError> {
        let block_id = format!("block #{} ({})", block.header.height, block.hash().to_hex());
        let tip_height = db.fetch_chain_metadata()?.height_of_longest_chain();
        if block.header.height != tip_height + 1 {
            return Err(ValidationError::BlockHeaderError(
                BlockHeaderValidationError::InvalidChaining,
            ));
        }
        check_inputs_are_utxos(block, backend)?;
        check_not_duplicate_txos(block, backend)?;
        trace!(
            target: LOG_TARGET,
            "Block validation: All inputs and outputs are valid for {}",
            block_id
        );
        check_mmr_roots(block, backend)?;
        trace!(
            target: LOG_TARGET,
            "Block validation: MMR roots are valid for {}",
            block_id
        );
        // Validate the block header (PoW etc.)
        self.validate(&block.header, backend)?;
        debug!(target: LOG_TARGET, "Block validation: Block is VALID for {}", block_id);
        Ok(())
    }

    // /// This will validate the proof of work, and convert to a chainheader
    // fn validate_and_convert(&self, header: BlockHeader, db: &B) -> Result<ChainHeader, ValidationError> {
    //     let header_id = format!("header #{} ({})", header.height, header.hash().to_hex());
    //     let chain_header = check_achieved_and_target_difficulty(db, header, self.rules.clone())?;
    //     trace!(
    //         target: LOG_TARGET,
    //         "BlockHeader validation: Achieved difficulty is ok for {} ",
    //         &header_id
    //     );
    //     debug!(
    //         target: LOG_TARGET,
    //         "Block header validation: BlockHeader is VALID for {}", &header_id
    //     );
    //     Ok(chain_header)
    // }
}

impl<B: BlockchainBackend> ValidationConvert<BlockHeader, ChainHeader, B> for FullConsensusValidator {
    fn validate_and_convert(&self, header: BlockHeader, db: &B) -> Result<ChainHeader, ValidationError> {
        let header_id = format!("header #{} ({})", header.height, header.hash().to_hex());
        trace!(
            target: LOG_TARGET,
            "Calculating and verifying target and achieved difficulty {} ",
            &header_id
        );
        let chain_header = check_achieved_and_target_difficulty(db, header, self.rules.clone())?;
        trace!(
            target: LOG_TARGET,
            "BlockHeader validation: Achieved difficulty is ok for {} ",
            &header_id
        );
        debug!(
            target: LOG_TARGET,
            "Block header validation: BlockHeader is VALID for {}", &header_id
        );
        Ok(chain_header)
    }
}

impl<B: BlockchainBackend> StatefulValidation<BlockHeader, B> for FullConsensusValidator {
    /// The consensus checks that are done (in order of cheapest to verify to most expensive):
    /// 1. Is the block timestamp within the Future Time Limit (FTL)?
    /// 1. Is the Proof of Work valid?
    /// 1. Is the achieved difficulty of this block >= the target difficulty for this block?
    fn validate(&self, header: &BlockHeader, backend: &B) -> Result<(), ValidationError> {
        check_timestamp_ftl(&header, &self.rules)?;
        let header_id = format!("header #{} ({})", header.height, header.hash().to_hex());
        trace!(
            target: LOG_TARGET,
            "BlockHeader validation: FTL timestamp is ok for {} ",
            header_id
        );
        self.check_median_timestamp(backend, header)?;
        trace!(
            target: LOG_TARGET,
            "BlockHeader validation: Median timestamp is ok for {} ",
            header_id
        );
        check_pow_data(header, &self.rules)?;
        self.check_achieved_and_target_difficulty(backend, header)?;
        trace!(
            target: LOG_TARGET,
            "BlockHeader validation: Achieved difficulty is ok for {} ",
            header_id
        );
        debug!(
            target: LOG_TARGET,
            "Block header validation: BlockHeader is VALID for {}", header_id
        );
        Ok(())
    }
}

// This function checks for duplicate inputs and outputs. There should be no duplicate inputs or outputs in a block
fn check_sorting_and_duplicates(body: &AggregateBody) -> Result<(), ValidationError> {
    if !is_all_unique_and_sorted(body.inputs()) {
        return Err(ValidationError::UnsortedOrDuplicateInput);
    }
    if !is_all_unique_and_sorted(body.outputs()) {
        return Err(ValidationError::UnsortedOrDuplicateOutput);
    }

    Ok(())
}

/// This function checks that all inputs in the blocks are valid UTXO's to be spend
fn check_inputs_are_utxos<B: BlockchainBackend>(block: &Block, db: &B) -> Result<(), ValidationError> {
    let data = db
        .fetch_block_accumulated_data(&block.header.prev_hash)?
        .ok_or_else(|| ValidationError::PreviousHashNotFound)?;

    for input in block.body.inputs() {
        if let Some(index) = db.fetch_mmr_leaf_index(MmrTree::Utxo, &input.hash())? {
            if data.deleted().contains(index) {
                warn!(
                    target: LOG_TARGET,
                    "Block validation failed due to already spent input: {}", input
                );
                return Err(ValidationError::ContainsSTxO);
            }
        } else {
            warn!(
                target: LOG_TARGET,
                "Block validation failed because the block has invalid input: {} which does not exist", input
            );
            return Err(ValidationError::BlockError(BlockValidationError::InvalidInput));
        }
    }

    Ok(())
}

// This function checks that the inputs and outputs do not exist in the STxO set.
fn check_not_duplicate_txos<B: BlockchainBackend>(block: &Block, db: &B) -> Result<(), ValidationError> {
    for output in block.body.outputs() {
        if db.fetch_mmr_leaf_index(MmrTree::Utxo, &output.hash())?.is_some() {
            warn!(
                target: LOG_TARGET,
                "Block validation failed due to previously spent output: {}", output
            );
            return Err(ValidationError::ContainsTxO);
        }
    }
    Ok(())
}

fn check_mmr_roots<B: BlockchainBackend>(block: &Block, db: &B) -> Result<(), ValidationError> {
    let mmr_roots = chain_storage::calculate_mmr_roots(db, &block)?;
    let header = &block.header;
    if header.kernel_mr != mmr_roots.kernel_mr {
        warn!(
            target: LOG_TARGET,
            "Block header kernel MMR roots in {} do not match calculated roots. Expected: {}, Actual:{}",
            block.hash().to_hex(),
            header.kernel_mr.to_hex(),
            mmr_roots.kernel_mr.to_hex()
        );
        return Err(ValidationError::BlockError(BlockValidationError::MismatchedMmrRoots));
    };
    if header.output_mr != mmr_roots.output_mr {
        warn!(
            target: LOG_TARGET,
            "Block header output MMR roots in {} do not match calculated roots. Expected: {}, Actual:{}",
            block.hash().to_hex(),
            header.output_mr.to_hex(),
            mmr_roots.output_mr.to_hex()
        );
        return Err(ValidationError::BlockError(BlockValidationError::MismatchedMmrRoots));
    };
    if header.range_proof_mr != mmr_roots.range_proof_mr {
        warn!(
            target: LOG_TARGET,
            "Block header range_proof MMR roots in {} do not match calculated roots",
            block.hash().to_hex()
        );
        return Err(ValidationError::BlockError(BlockValidationError::MismatchedMmrRoots));
    };
    Ok(())
}
