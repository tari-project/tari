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

pub use crate::consensus::ConsensusManager;
use crate::{
    blocks::{
        blockheader::{BlockHeader, BlockHeaderValidationError},
        genesis_block::get_gen_block_hash,
    },
    chain_storage::{BlockchainBackend, BlockchainDatabase, MmrTree},
    transactions::types::CryptoFactories,
    validation::{error::ValidationError, traits::Validation},
};
use tari_crypto::commitment::HomomorphicCommitmentFactory;
use tari_utilities::hash::Hashable;

/// This validator checks that the synced state satisfies *all* consensus rules and should only be performed on
/// the chain tip header.
pub struct ChainTipValidator<B: BlockchainBackend> {
    rules: ConsensusManager<B>,
    factories: CryptoFactories,
    db: BlockchainDatabase<B>,
}

impl<B: BlockchainBackend> ChainTipValidator<B>
where B: BlockchainBackend
{
    pub fn new(rules: ConsensusManager<B>, factories: CryptoFactories, db: BlockchainDatabase<B>) -> Self {
        Self { rules, factories, db }
    }

    fn db(&self) -> Result<BlockchainDatabase<B>, ValidationError> {
        Ok(self.db.clone())
    }
}

impl<B: BlockchainBackend> Validation<BlockHeader, B> for ChainTipValidator<B> {
    /// The consensus checks that are done (in order of cheapest to verify to most expensive):
    /// 1. Does the MMR roots calculated from the chain state match that roots provided in the tip header?
    /// 1. Is the full accounting balance of the current chain state valid?
    fn validate(&self, block_header: &BlockHeader) -> Result<(), ValidationError> {
        check_mmr_roots(block_header, self.db()?)?;
        check_chain_accounting_balance(block_header, self.db()?, self.rules.clone(), &self.factories)?;

        Ok(())
    }
}

/// This validator checks that the synced chain builds on the correct genesis block and should only be performed on the
/// genesis block header.
pub struct GenesisBlockValidator {}

impl GenesisBlockValidator {
    pub fn new() -> Self {
        Self {}
    }
}

impl<B: BlockchainBackend> Validation<BlockHeader, B> for GenesisBlockValidator {
    /// The consensus checks that are done (in order of cheapest to verify to most expensive):
    /// Does the genesis block hash match the provided block hash?
    fn validate(&self, block_header: &BlockHeader) -> Result<(), ValidationError> {
        check_genesis_block_hash(block_header)?;

        Ok(())
    }
}

/// Check the full accounting balance of the current chain state. This check should only be performed on chain tip
/// headers.
fn check_chain_accounting_balance<B: BlockchainBackend>(
    block_header: &BlockHeader,
    db: BlockchainDatabase<B>,
    rules: ConsensusManager<B>,
    factories: &CryptoFactories,
) -> Result<(), ValidationError>
{
    let total_coinbase = rules.emission_schedule().supply_at_block(block_header.height);
    let total_kernel_excess = db
        .total_kernel_excess()
        .map_err(|e| ValidationError::CustomError(e.to_string()))?;
    let total_kernel_offset = db
        .total_kernel_offset()
        .map_err(|e| ValidationError::CustomError(e.to_string()))?;
    let kernel_offset_and_coinbase = factories
        .commitment
        .commit_value(&total_kernel_offset, total_coinbase.0);
    let sum_utxo_commitments = db
        .total_utxo_commitment()
        .map_err(|e| ValidationError::CustomError(e.to_string()))?;
    if sum_utxo_commitments != &total_kernel_excess + &kernel_offset_and_coinbase {
        return Err(ValidationError::InvalidAccountingBalance);
    }
    Ok(())
}

/// This function checks that the MMR roots calculated from the chain state matches that MMR roots provided in the tip
/// header.
fn check_mmr_roots<B: BlockchainBackend>(
    block_header: &BlockHeader,
    db: BlockchainDatabase<B>,
) -> Result<(), ValidationError>
{
    if (block_header.output_mr !=
        db.fetch_mmr_root(MmrTree::Utxo)
            .map_err(|e| ValidationError::CustomError(e.to_string()))?) ||
        (block_header.range_proof_mr !=
            db.fetch_mmr_root(MmrTree::RangeProof)
                .map_err(|e| ValidationError::CustomError(e.to_string()))?) ||
        (block_header.kernel_mr !=
            db.fetch_mmr_root(MmrTree::Kernel)
                .map_err(|e| ValidationError::CustomError(e.to_string()))?)
    {
        return Err(ValidationError::BlockHeaderError(
            BlockHeaderValidationError::MismatchedMmrRoots,
        ));
    }
    Ok(())
}

/// This function checks that the synced genesis block header is the correct block header.
fn check_genesis_block_hash(block_header: &BlockHeader) -> Result<(), ValidationError> {
    if block_header.hash() != get_gen_block_hash() {
        return Err(ValidationError::BlockHeaderError(
            BlockHeaderValidationError::IncorrectGenesisBlockHeader,
        ));
    }
    Ok(())
}
