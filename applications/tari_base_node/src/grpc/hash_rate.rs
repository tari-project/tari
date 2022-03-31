// Copyright 2022. The Tari Project
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

use tari_core::{
    blocks::ChainHeader,
    consensus::ConsensusManager,
    proof_of_work::{lwma_diff::LinearWeightedMovingAverage, Difficulty, DifficultyAdjustment, PowAlgorithm},
};

/// The number of past blocks to be used on moving averages for (smooth) estimated hashrate
/// The hash rate is calculated as the difficulty divided by the target block time
const SHA3_HASH_RATE_MOVING_AVERAGE_WINDOW: usize = 12;
const MONERO_HASH_RATE_MOVING_AVERAGE_WINDOW: usize = 15;

/// Calculates a linear weighted moving average for hash rate calculations
pub struct HashRateMovingAverage {
    pow_algo: PowAlgorithm,
    consensus_rules: ConsensusManager,
    moving_average: LinearWeightedMovingAverage,
}

impl HashRateMovingAverage {
    pub fn new(pow_algo: PowAlgorithm, consensus_rules: ConsensusManager, start_height: u64) -> Self {
        let window_size = match pow_algo {
            PowAlgorithm::Monero => MONERO_HASH_RATE_MOVING_AVERAGE_WINDOW,
            PowAlgorithm::Sha3 => SHA3_HASH_RATE_MOVING_AVERAGE_WINDOW,
        };

        let consensus_constants = consensus_rules.consensus_constants(start_height);

        let moving_average = LinearWeightedMovingAverage::new(
            window_size,
            consensus_constants.get_diff_target_block_interval(pow_algo),
            consensus_constants.get_difficulty_max_block_interval(pow_algo),
        );

        Self {
            pow_algo,
            consensus_rules,
            moving_average,
        }
    }

    /// Adds a new entry in the moving average to update it
    pub fn add(&mut self, chain_header: &ChainHeader) {
        let target_difficulty = chain_header.accumulated_data().target_difficulty;
        let target_time = self
            .consensus_rules
            .consensus_constants(chain_header.header().height)
            .get_diff_target_block_interval(self.pow_algo);
        self.moving_average
            .add_back(chain_header.header().timestamp, target_difficulty / target_time);
    }

    /// Gets the most recent hash rate value in the moving average
    pub fn get_hash_rate(&self) -> u64 {
        self.moving_average
            .get_difficulty()
            .map(Difficulty::as_u64)
            .unwrap_or(0)
    }
}
