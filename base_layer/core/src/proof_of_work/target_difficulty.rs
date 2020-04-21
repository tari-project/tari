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
    blocks::blockheader::BlockHeader,
    proof_of_work::{
        difficulty::DifficultyAdjustment,
        error::DifficultyAdjustmentError,
        lwma_diff::LinearWeightedMovingAverage,
        Difficulty,
        PowAlgorithm,
    },
};
use log::*;
use std::cmp;

pub const LOG_TARGET: &str = "c::pow::target_difficulty";

/// Returns the estimated target difficulty for the specified PoW algorithm and provided header set.
pub fn get_target_difficulty(
    headers: Vec<BlockHeader>,
    pow_algo: PowAlgorithm,
    block_window: usize,
    target_time: u64,
    max_block_time: u64,
    min_pow_difficulty: Difficulty,
) -> Result<Difficulty, DifficultyAdjustmentError>
{
    let height = headers.last().expect("Header set should not be empty").height;
    debug!(target: LOG_TARGET, "Calculating target difficulty to height:{}", height);
    let mut monero_lwma =
        LinearWeightedMovingAverage::new(block_window, target_time, min_pow_difficulty, max_block_time);
    let mut blake_lwma =
        LinearWeightedMovingAverage::new(block_window, target_time, min_pow_difficulty, max_block_time);

    // TODO: Store the target difficulty so that we don't have to calculate it for the whole chain
    for header in headers {
        match header.pow.pow_algo {
            PowAlgorithm::Monero => monero_lwma.add(header.timestamp, monero_lwma.get_difficulty())?,
            PowAlgorithm::Blake => blake_lwma.add(
                header.timestamp,
                cmp::max(min_pow_difficulty, blake_lwma.get_difficulty()),
            )?,
        }
    }

    let target_difficulty = match pow_algo {
        PowAlgorithm::Monero => monero_lwma.get_difficulty(),
        PowAlgorithm::Blake => cmp::max(min_pow_difficulty, blake_lwma.get_difficulty()),
    };
    debug!(
        target: LOG_TARGET,
        "Target difficulty:{} at height:{} for PoW:{}", target_difficulty, height, pow_algo
    );
    Ok(target_difficulty)
}
