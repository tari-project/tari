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
    blocks::blockheader::{BlockHeader, BlockHeaderValidationError},
    chain_storage::BlockchainBackend,
    consensus::ConsensusManager,
    proof_of_work::PowError,
    validation::ValidationError,
};
use log::*;
use tari_crypto::tari_utilities::hash::Hashable;
pub const LOG_TARGET: &str = "c::val::helpers";
use tari_crypto::tari_utilities::hex::Hex;

/// This function tests that the block timestamp is greater than the median timestamp at the specified height.
pub fn check_median_timestamp<B: BlockchainBackend>(
    db: &B,
    block_header: &BlockHeader,
    height: u64,
    rules: ConsensusManager,
) -> Result<(), ValidationError>
{
    trace!(target: LOG_TARGET, "Checking timestamp is not too far in the past",);
    if block_header.height == 0 || rules.get_genesis_block_hash() == block_header.hash() {
        return Ok(()); // Its the genesis block, so we dont have to check median
    }
    let median_timestamp = rules
        .get_median_timestamp_at_height(db, height)
        .or_else(|e| {
            error!(target: LOG_TARGET, "Validation could not get median timestamp");

            Err(e)
        })
        .map_err(|_| ValidationError::BlockHeaderError(BlockHeaderValidationError::InvalidTimestamp))?;
    if block_header.timestamp < median_timestamp {
        warn!(
            target: LOG_TARGET,
            "Block header timestamp {} is less than median timestamp: {} for block:{}",
            block_header.timestamp,
            median_timestamp,
            block_header.hash().to_hex()
        );
        return Err(ValidationError::BlockHeaderError(
            BlockHeaderValidationError::InvalidTimestamp,
        ));
    }
    Ok(())
}

/// Calculates the achieved and target difficulties at the specified height and compares them.
pub fn check_achieved_difficulty<B: BlockchainBackend>(
    db: &B,
    block_header: &BlockHeader,
    height: u64,
    rules: ConsensusManager,
) -> Result<(), ValidationError>
{
    trace!(
        target: LOG_TARGET,
        "Checking block has acheived the required difficulty",
    );
    let achieved = block_header.achieved_difficulty();
    let mut target = 1.into();
    if block_header.height > 0 || rules.get_genesis_block_hash() != block_header.hash() {
        target = rules
            .get_target_difficulty_with_height(db, block_header.pow.pow_algo, height)
            .or_else(|e| {
                error!(target: LOG_TARGET, "Validation could not get achieved difficulty");
                Err(e)
            })
            .map_err(|_| {
                ValidationError::BlockHeaderError(BlockHeaderValidationError::ProofOfWorkError(
                    PowError::InvalidProofOfWork,
                ))
            })?;
    }
    if achieved < target {
        warn!(
            target: LOG_TARGET,
            "Proof of work for {} was below the target difficulty. Achieved: {}, Target:{}",
            block_header.hash().to_hex(),
            achieved,
            target
        );
        return Err(ValidationError::BlockHeaderError(
            BlockHeaderValidationError::ProofOfWorkError(PowError::AchievedDifficultyTooLow),
        ));
    }
    Ok(())
}
