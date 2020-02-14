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
        genesis_block::get_gen_block_hash,
    },
    chain_storage::{BlockchainBackend, BlockchainDatabase},
    consensus::ConsensusManager,
    proof_of_work::PowError,
    validation::ValidationError,
};
use tari_crypto::tari_utilities::hash::Hashable;

/// This function tests that the block timestamp is greater than the median timestamp at the chain tip.
pub fn check_median_timestamp_at_chain_tip<B: BlockchainBackend>(
    block_header: &BlockHeader,
    db: BlockchainDatabase<B>,
    rules: ConsensusManager<B>,
) -> Result<(), ValidationError>
{
    let tip_height = db
        .get_metadata()
        .map_err(|e| ValidationError::CustomError(e.to_string()))?
        .height_of_longest_chain
        .unwrap_or(0);
    check_median_timestamp(&block_header, tip_height, rules)
}

/// This function tests that the block timestamp is greater than the median timestamp at the specified height.
pub fn check_median_timestamp<B: BlockchainBackend>(
    block_header: &BlockHeader,
    height: u64,
    rules: ConsensusManager<B>,
) -> Result<(), ValidationError>
{
    if block_header.height == 0 || get_gen_block_hash() == block_header.hash() {
        return Ok(()); // Its the genesis block, so we dont have to check median
    }
    let median_timestamp = rules
        .get_median_timestamp_at_height(height)
        .map_err(|_| ValidationError::BlockHeaderError(BlockHeaderValidationError::InvalidTimestamp))?;
    if block_header.timestamp < median_timestamp {
        return Err(ValidationError::BlockHeaderError(
            BlockHeaderValidationError::InvalidTimestamp,
        ));
    }
    Ok(())
}

/// Calculates the achieved and target difficulties at the chain tip and compares them.
pub fn check_achieved_difficulty_at_chain_tip<B: BlockchainBackend>(
    block_header: &BlockHeader,
    db: BlockchainDatabase<B>,
    rules: ConsensusManager<B>,
) -> Result<(), ValidationError>
{
    let tip_height = db
        .get_metadata()
        .map_err(|e| ValidationError::CustomError(e.to_string()))?
        .height_of_longest_chain
        .unwrap_or(0);
    check_achieved_difficulty(&block_header, tip_height, rules)
}

/// Calculates the achieved and target difficulties at the specified height and compares them.
pub fn check_achieved_difficulty<B: BlockchainBackend>(
    block_header: &BlockHeader,
    height: u64,
    rules: ConsensusManager<B>,
) -> Result<(), ValidationError>
{
    let achieved = block_header.achieved_difficulty();
    let mut target = 1.into();
    if block_header.height > 0 || get_gen_block_hash() != block_header.hash() {
        target = rules
            .get_target_difficulty_with_height(&block_header.pow.pow_algo, height)
            .map_err(|_| {
                ValidationError::BlockHeaderError(BlockHeaderValidationError::ProofOfWorkError(
                    PowError::InvalidProofOfWork,
                ))
            })?;
    }
    if achieved < target {
        return Err(ValidationError::BlockHeaderError(
            BlockHeaderValidationError::ProofOfWorkError(PowError::AchievedDifficultyTooLow),
        ));
    }
    Ok(())
}
