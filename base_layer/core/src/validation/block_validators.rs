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
    blocks::{Block, BlockValidationError},
    chain_storage,
    chain_storage::{BlockchainBackend, ChainBlock, MmrTree},
    consensus::ConsensusManager,
    transactions::{
        aggregated_body::AggregateBody,
        transaction::{KernelFeatures, OutputFlags, TransactionError},
        types::CryptoFactories,
    },
    validation::{
        helpers::{check_accounting_balance, check_block_weight, check_coinbase_output, is_all_unique_and_sorted},
        traits::PostOrphanBodyValidation,
        CandidateBlockBodyValidation,
        OrphanValidation,
        ValidationError,
    },
};
use log::*;
use std::marker::PhantomData;
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    tari_utilities::{hash::Hashable, hex::Hex},
};

pub const LOG_TARGET: &str = "c::val::block_validators";

/// This validator tests whether a candidate block is internally consistent
#[derive(Clone)]
pub struct OrphanBlockValidator {
    rules: ConsensusManager,
    factories: CryptoFactories,
}

impl OrphanBlockValidator {
    pub fn new(rules: ConsensusManager, factories: CryptoFactories) -> Self {
        Self { rules, factories }
    }
}

impl OrphanValidation for OrphanBlockValidator {
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
        block.check_stxo_rules()?;
        trace!(target: LOG_TARGET, "SV - Output constraints are ok for {} ", &block_id);
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
#[derive(Default)]
pub struct BodyOnlyValidator {}

impl<B: BlockchainBackend> PostOrphanBodyValidation<B> for BodyOnlyValidator {
    /// The consensus checks that are done (in order of cheapest to verify to most expensive):
    /// 1. Does the block satisfy the stateless checks?
    /// 1. Are all inputs currently in the UTXO set?
    /// 1. Are all inputs and outputs not in the STXO set?
    /// 1. Are the block header MMR roots valid?
    fn validate_body_for_valid_orphan(&self, block: &ChainBlock, backend: &B) -> Result<(), ValidationError> {
        let block_id = format!("block #{} ({})", block.block.header.height, block.hash().to_hex());
        check_inputs_are_utxos(&block.block, backend)?;
        check_not_duplicate_txos(&block.block, backend)?;
        trace!(
            target: LOG_TARGET,
            "Block validation: All inputs and outputs are valid for {}",
            block_id
        );
        check_mmr_roots(&block.block, backend)?;
        trace!(
            target: LOG_TARGET,
            "Block validation: MMR roots are valid for {}",
            block_id
        );
        debug!(target: LOG_TARGET, "Block validation: Block is VALID for {}", block_id);
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
        if let Some((_, index, height)) = db.fetch_output(&input.hash())? {
            if data.deleted().contains(index) {
                warn!(
                    target: LOG_TARGET,
                    "Block validation failed due to already spent input: {}", input
                );
                return Err(ValidationError::ContainsSTxO);
            }
            if height != input.height {
                warn!(
                    target: LOG_TARGET,
                    "Block validation failed due to input not having correct mined height({}): {}", height, input
                );
                return Err(ValidationError::InvalidMinedHeight);
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
    if header.kernel_mmr_size != mmr_roots.kernel_mmr_size {
        warn!(
            target: LOG_TARGET,
            "Block header kernel MMR size in {} does not match. Expected: {}, Actual:{}",
            block.hash().to_hex(),
            header.kernel_mmr_size,
            mmr_roots.kernel_mmr_size
        );
        return Err(ValidationError::BlockError(BlockValidationError::MismatchedMmrSize {
            mmr_tree: MmrTree::Kernel,
            expected: mmr_roots.kernel_mmr_size,
            actual: header.kernel_mmr_size,
        }));
    }
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
    if header.output_mmr_size != mmr_roots.output_mmr_size {
        warn!(
            target: LOG_TARGET,
            "Block header output MMR size in {} does not match. Expected: {}, Actual:{}",
            block.hash().to_hex(),
            header.output_mmr_size,
            mmr_roots.output_mmr_size
        );
        return Err(ValidationError::BlockError(BlockValidationError::MismatchedMmrSize {
            mmr_tree: MmrTree::Utxo,
            expected: mmr_roots.output_mmr_size,
            actual: header.output_mmr_size,
        }));
    }
    Ok(())
}

/// This validator checks whether a block satisfies consensus rules.
/// It implements two validators: one for the `BlockHeader` and one for `Block`. The `Block` validator ONLY validates
/// the block body using the header. It is assumed that the `BlockHeader` has already been validated.
pub struct BlockValidator<B: BlockchainBackend> {
    rules: ConsensusManager,
    factories: CryptoFactories,
    phantom_data: PhantomData<B>,
}

impl<B: BlockchainBackend> BlockValidator<B> {
    pub fn new(rules: ConsensusManager, factories: CryptoFactories) -> Self {
        Self {
            rules,
            factories,
            phantom_data: Default::default(),
        }
    }

    /// This function checks that all inputs in the blocks are valid UTXO's to be spend
    fn check_inputs(&self, block: &Block) -> Result<(), ValidationError> {
        let inputs = block.body.inputs();
        let outputs = block.body.outputs();
        for (i, input) in inputs.iter().enumerate() {
            // Check for duplicates and/or incorrect sorting
            if i > 0 && input <= &inputs[i - 1] {
                return Err(ValidationError::UnsortedOrDuplicateInput);
            }

            // Check maturity
            if input.features.maturity > block.header.height {
                warn!(
                    target: LOG_TARGET,
                    "Input found that has not yet matured to spending height: {}", input
                );
                return Err(TransactionError::InputMaturity.into());
            }
        }
        Ok(())
    }

    fn check_outputs(&self, block: &Block) -> Result<(), ValidationError> {
        let outputs = block.body.outputs();
        let mut coinbase_output = None;
        for (j, output) in outputs.iter().enumerate() {
            if output.features.flags.contains(OutputFlags::COINBASE_OUTPUT) {
                if coinbase_output.is_some() {
                    return Err(ValidationError::TransactionError(TransactionError::MoreThanOneCoinbase));
                }
                coinbase_output = Some(output);
            }

            if j > 0 && output <= &outputs[j - 1] {
                return Err(ValidationError::UnsortedOrDuplicateOutput);
            }
        }

        let coinbase_output = match coinbase_output {
            Some(output) => output,
            // No coinbase found
            None => {
                warn!(
                    target: LOG_TARGET,
                    "Block #{} failed to validate: no coinbase UTXO", block.header.height
                );
                return Err(ValidationError::TransactionError(TransactionError::NoCoinbase));
            },
        };

        let mut coinbase_kernel = None;
        for kernel in block.body.kernels() {
            if kernel.features.contains(KernelFeatures::COINBASE_KERNEL) {
                if coinbase_kernel.is_some() {
                    return Err(ValidationError::TransactionError(TransactionError::MoreThanOneCoinbase));
                }
                coinbase_kernel = Some(kernel);
            }
        }

        let coinbase_kernel = match coinbase_kernel {
            Some(kernel) => kernel,
            // No coinbase found
            None => {
                warn!(
                    target: LOG_TARGET,
                    "Block #{} failed to validate: no coinbase kernel", block.header.height
                );
                return Err(ValidationError::TransactionError(TransactionError::NoCoinbase));
            },
        };

        let reward = self.rules.calculate_coinbase_and_fees(block);
        let rhs = &coinbase_kernel.excess +
            &self
                .factories
                .commitment
                .commit_value(&Default::default(), reward.into());
        if rhs != coinbase_output.commitment {
            warn!(
                target: LOG_TARGET,
                "Coinbase {} amount validation failed", coinbase_output
            );
            return Err(ValidationError::TransactionError(TransactionError::InvalidCoinbase));
        }

        Ok(())
    }

    fn check_mmr_roots(&self, db: &B, block: &Block) -> Result<(), ValidationError> {
        let mmr_roots = chain_storage::calculate_mmr_roots(db, block)?;
        let header = &block.header;
        if header.kernel_mr != mmr_roots.kernel_mr {
            warn!(
                target: LOG_TARGET,
                "Block header kernel MMR roots in {} do not match calculated roots",
                block.hash().to_hex()
            );
            return Err(ValidationError::BlockError(BlockValidationError::MismatchedMmrRoots));
        }
        if header.kernel_mmr_size != mmr_roots.kernel_mmr_size {
            warn!(
                target: LOG_TARGET,
                "Block header kernel MMR size in {} does not match MMR size",
                block.hash().to_hex()
            );
            return Err(ValidationError::BlockError(BlockValidationError::MismatchedMmrSize {
                mmr_tree: MmrTree::Kernel,
                expected: header.kernel_mmr_size,
                actual: mmr_roots.kernel_mmr_size,
            }));
        }
        if header.output_mr != mmr_roots.output_mr {
            warn!(
                target: LOG_TARGET,
                "Block header output MMR roots in {} do not match calculated roots",
                block.hash().to_hex()
            );
            return Err(ValidationError::BlockError(BlockValidationError::MismatchedMmrRoots));
        }
        if header.range_proof_mr != mmr_roots.range_proof_mr {
            warn!(
                target: LOG_TARGET,
                "Block header range_proof MMR roots in {} do not match calculated roots",
                block.hash().to_hex()
            );
            return Err(ValidationError::BlockError(BlockValidationError::MismatchedMmrRoots));
        }
        if header.output_mmr_size != mmr_roots.output_mmr_size {
            warn!(
                target: LOG_TARGET,
                "Block header output MMR size in {} does not match MMR size",
                block.hash().to_hex()
            );
            return Err(ValidationError::BlockError(BlockValidationError::MismatchedMmrSize {
                mmr_tree: MmrTree::Utxo,
                expected: header.output_mmr_size,
                actual: mmr_roots.output_mmr_size,
            }));
        }
        Ok(())
    }
}

impl<B: BlockchainBackend> CandidateBlockBodyValidation<B> for BlockValidator<B> {
    /// The following consensus checks are done:
    /// 1. Does the block satisfy the stateless checks?
    /// 1. Are the block header MMR roots valid?
    fn validate_body(&self, block: &ChainBlock, backend: &B) -> Result<(), ValidationError> {
        let block_id = format!("block #{}", block.block.header.height);
        trace!(target: LOG_TARGET, "Validating {}", block_id);

        let constants = self.rules.consensus_constants(block.block.header.height);
        check_block_weight(&block.block, &constants)?;
        trace!(target: LOG_TARGET, "SV - Block weight is ok for {} ", &block_id);

        self.check_inputs(&block.block)?;
        self.check_outputs(&block.block)?;

        check_accounting_balance(&block.block, &self.rules, &self.factories)?;
        trace!(target: LOG_TARGET, "SV - accounting balance correct for {}", &block_id);
        debug!(
            target: LOG_TARGET,
            "{} has PASSED stateless VALIDATION check.", &block_id
        );

        self.check_mmr_roots(backend, &block.block)?;
        trace!(
            target: LOG_TARGET,
            "Block validation: MMR roots are valid for {}",
            block_id
        );

        debug!(target: LOG_TARGET, "Block validation: Block is VALID for {}", block_id);
        Ok(())
    }
}
