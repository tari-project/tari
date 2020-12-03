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
        block_header::{BlockHeader, BlockHeaderValidationError},
        Block,
        BlockValidationError,
    },
    consensus::{ConsensusConstants, ConsensusManager},
    proof_of_work::{monero_rx::MoneroData, Difficulty, PowAlgorithm, PowError},
    transactions::types::CryptoFactories,
    validation::ValidationError,
};
use log::*;
use tari_crypto::tari_utilities::{epoch_time::EpochTime, hash::Hashable, hex::Hex};

pub const LOG_TARGET: &str = "c::val::helpers";

/// This function tests that the block timestamp is less than the FTL
pub fn check_timestamp_ftl(
    block_header: &BlockHeader,
    consensus_manager: &ConsensusManager,
) -> Result<(), ValidationError>
{
    if block_header.timestamp > consensus_manager.consensus_constants(block_header.height).ftl() {
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

/// Returns the median timestamp for the provided timestamps.
pub fn calc_median_timestamp(timestamps: &[EpochTime]) -> EpochTime {
    assert_eq!(
        timestamps.is_empty(),
        false,
        "calc_median_timestamp: timestamps cannot be empty"
    );
    trace!(
        target: LOG_TARGET,
        "Calculate the median timestamp from {} timestamps",
        timestamps.len()
    );

    let mid_index = timestamps.len() / 2;
    let median_timestamp = if timestamps.len() % 2 == 0 {
        (timestamps[mid_index - 1] + timestamps[mid_index]) / 2
    } else {
        timestamps[mid_index]
    };
    trace!(target: LOG_TARGET, "Median timestamp:{}", median_timestamp);
    median_timestamp
}

pub fn check_header_timestamp_greater_than_median(
    block_header: &BlockHeader,
    timestamps: &[EpochTime],
) -> Result<(), ValidationError>
{
    if timestamps.is_empty() {
        return Err(ValidationError::BlockHeaderError(
            BlockHeaderValidationError::InvalidTimestamp,
        ));
    }

    let median_timestamp = calc_median_timestamp(timestamps);
    if block_header.timestamp <= median_timestamp {
        warn!(
            target: LOG_TARGET,
            "Block header timestamp {} is less than or equal to median timestamp: {} for block:{}",
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

/// Check the PoW data in the BlockHeader. This currently only applies to blocks merged mined with Monero.
pub fn check_pow_data(block_header: &BlockHeader, rules: &ConsensusManager) -> Result<(), ValidationError> {
    use PowAlgorithm::*;
    match block_header.pow.pow_algo {
        Monero => {
            MoneroData::from_header(block_header).map_err(|e| ValidationError::CustomError(e.to_string()))?;
            // TODO: We need some way of getting the seed height and or count.
            // Current proposals are to either store the height of first seed use, or count the seed use.
            let seed_height = 0;
            if (seed_height != 0) &&
                (block_header.height - seed_height >
                    rules.consensus_constants(block_header.height).max_randomx_seed_height())
            {
                return Err(ValidationError::BlockHeaderError(
                    BlockHeaderValidationError::OldSeedHash,
                ));
            }

            Ok(())
        },
        Blake | Sha3 => Ok(()),
/// Calculates the achieved and target difficulties at the specified height and compares them.
pub fn check_achieved_and_target_difficulty<B: BlockchainBackend>(
    db: &B,
    block_header: BlockHeader,
    rules: ConsensusManager,
) -> Result<ChainHeader, ValidationError>
{
    let pow_algo = block_header.pow.pow_algo;
    // Monero has extra data to check.
    if pow_algo == PowAlgorithm::Monero {
        MoneroData::new(&block_header).map_err(|e| ValidationError::CustomError(e.to_string()))?;
        // TODO: We need some way of getting the seed height and or count.
        // Current proposals are to either store the height of first seed use, or count the seed use.
        let seed_height = 0;
        if (seed_height != 0) &&
            (block_header.height - seed_height >
                rules.consensus_constants(block_header.height).max_randomx_seed_height())
        {
            return Err(ValidationError::BlockHeaderError(
                BlockHeaderValidationError::OldSeedHash,
            ));
        }
    }
    let target = if block_header.height == 0 && rules.get_genesis_block_hash() == block_header.hash() {
        Difficulty::from(1)
    } else {
        let target_difficulties = db.fetch_target_difficulties(block_header,rules.consensus_constants(block_header.height).get_difficulty_block_window() as usize)?;

        let constants = rules.consensus_constants(block_header.height);
        get_target_difficulty(
            target_difficulties,
            constants.get_difficulty_block_window() as usize,
            constants.get_diff_target_block_interval(pow_algo),
            constants.min_pow_difficulty(pow_algo),
            constants.max_pow_difficulty(pow_algo),
            constants.get_difficulty_max_block_interval(pow_algo),
        )
        .map_err(|err| {
            error!(
                target: LOG_TARGET,
                "Validation could not get target difficulty: {}", err
            );
            ValidationError::BlockHeaderError(BlockHeaderValidationError::ProofOfWorkError(
                PowError::InvalidProofOfWork,
            ))
        })?
    };
    let prev = db.fetch_last_header()?.ok_or_else(||ValidationError::CustomError("Asked for prev header and got none".to_string()))?;
    let chain_header = ChainHeader::from_previous_with_header(&prev, block_header, target)?;
    Ok (chain_header)
}

pub fn check_target_difficulty(block_header: &BlockHeader, target: Difficulty) -> Result<(), ValidationError> {
    if block_header.pow.target_difficulty != target {
        warn!(
            target: LOG_TARGET,
            "Header target difficulty ({}) differs from the target difficult ({})",
            block_header.pow.target_difficulty,
            target
        );
        return Err(ValidationError::BlockHeaderError(
            BlockHeaderValidationError::ProofOfWorkError(PowError::InvalidTargetDifficulty {
                expected: target,
                got: block_header.pow.target_difficulty,
            }),
        ));
    }

    // Now lets compare the achieved and target.
    let achieved = block_header.achieved_difficulty()?;
    if achieved < target {
        warn!(
            target: LOG_TARGET,
            "Proof of work for {} was below the target difficulty. Achieved: {}, Target:{}",
            block_header.hash().to_hex(),
            achieved,
            target
        );
        return Err(ValidationError::BlockHeaderError(
            BlockHeaderValidationError::ProofOfWorkError(PowError::AchievedDifficultyTooLow { achieved, target }),
        ));
    }
    Ok(())
}

pub fn check_block_weight(block: &Block, consensus_constants: &ConsensusConstants) -> Result<(), ValidationError> {
    // The genesis block has a larger weight than other blocks may have so we have to exclude it here
    let block_weight = block.body.calculate_weight();
    if block_weight <= consensus_constants.get_max_block_transaction_weight() || block.header.height == 0 {
        trace!(
            target: LOG_TARGET,
            "SV - Block contents for block #{} : {}; weight {}.",
            block.header.height,
            block.body.to_counts_string(),
            block_weight,
        );

        Ok(())
    } else {
        Err(BlockValidationError::BlockTooLarge).map_err(ValidationError::from)
    }
}

pub fn check_accounting_balance(
    block: &Block,
    rules: &ConsensusManager,
    factories: &CryptoFactories,
) -> Result<(), ValidationError>
{
    if block.header.height == 0 {
        // Gen block does not need to be checked for this.
        return Ok(());
    }
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

pub fn check_coinbase_output(
    block: &Block,
    rules: &ConsensusManager,
    factories: &CryptoFactories,
) -> Result<(), ValidationError>
{
    let total_coinbase = rules.calculate_coinbase_and_fees(block);
    block
        .check_coinbase_output(
            total_coinbase,
            rules.consensus_constants(block.header.height),
            factories,
        )
        .map_err(ValidationError::from)
}

pub fn check_cut_through(block: &Block) -> Result<(), ValidationError> {
    trace!(
        target: LOG_TARGET,
        "Checking cut through on block with hash {}",
        block.hash().to_hex()
    );
    if !block.body.check_cut_through() {
        warn!(
            target: LOG_TARGET,
            "Block validation for {} failed: block no cut through",
            block.hash().to_hex()
        );
        return Err(ValidationError::BlockError(BlockValidationError::NoCutThrough));
    }
    Ok(())
}

pub fn is_all_unique_and_sorted<I: AsRef<[T]>, T: PartialOrd>(items: I) -> bool {
    let items = items.as_ref();
    if items.is_empty() {
        return true;
    }

    let mut prev_item = &items[0];
    for item in items.iter().skip(1) {
        if item <= prev_item {
            return false;
        }
        prev_item = &item;
    }

    true
}

#[cfg(test)]
mod test {
    use super::*;

    #[cfg(test)]
    mod is_all_unique_and_sorted {
        use super::*;

        #[test]
        fn it_returns_true_when_nothing_to_compare() {
            assert_eq!(is_all_unique_and_sorted::<_, usize>(&[]), true);
            assert_eq!(is_all_unique_and_sorted(&[1]), true);
        }
        #[test]
        fn it_returns_true_when_unique_and_sorted() {
            let v = [1, 2, 3, 4, 5];
            assert_eq!(is_all_unique_and_sorted(&v), true);
        }

        #[test]
        fn it_returns_false_when_unsorted() {
            let v = [2, 1, 3, 4, 5];
            assert_eq!(is_all_unique_and_sorted(&v), false);
        }
        #[test]
        fn it_returns_false_when_duplicate() {
            let v = [1, 2, 3, 4, 4];
            assert_eq!(is_all_unique_and_sorted(&v), false);
        }
        #[test]
        fn it_returns_false_when_duplicate_and_unsorted() {
            let v = [4, 2, 3, 0, 4];
            assert_eq!(is_all_unique_and_sorted(&v), false);
        }
    }
}
