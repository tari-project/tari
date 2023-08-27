// Copyright 2022. The Taiji Project
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

use std::collections::VecDeque;

use taiji_core::{
    consensus::ConsensusManager,
    proof_of_work::{Difficulty, PowAlgorithm},
};

/// The number of past blocks to be used on moving averages for (smooth) estimated hashrate
/// We consider a 60 minute time window reasonable, that means 12 SHA3 blocks and 18 Monero blocks
const SHA3X_HASH_RATE_MOVING_AVERAGE_WINDOW: usize = 12;
const RANDOMX_HASH_RATE_MOVING_AVERAGE_WINDOW: usize = 18;

/// Calculates a linear weighted moving average for hash rate calculations
pub struct HashRateMovingAverage {
    pow_algo: PowAlgorithm,
    consensus_manager: ConsensusManager,
    window_size: usize,
    hash_rates: VecDeque<u64>,
    average: u64,
}

impl HashRateMovingAverage {
    pub fn new(pow_algo: PowAlgorithm, consensus_manager: ConsensusManager) -> Self {
        let window_size = match pow_algo {
            PowAlgorithm::RandomX => RANDOMX_HASH_RATE_MOVING_AVERAGE_WINDOW,
            PowAlgorithm::Sha3x => SHA3X_HASH_RATE_MOVING_AVERAGE_WINDOW,
        };
        let hash_rates = VecDeque::with_capacity(window_size);

        Self {
            pow_algo,
            consensus_manager,
            window_size,
            hash_rates,
            average: 0,
        }
    }

    /// Adds a new hash rate entry in the moving average and recalculates the average
    pub fn add(&mut self, height: u64, difficulty: Difficulty) {
        // target block time for the current block is provided by the consensus rules
        let target_time = self
            .consensus_manager
            .consensus_constants(height)
            .pow_target_block_interval(self.pow_algo);

        // remove old entries if we are at max block window
        if self.is_full() {
            self.hash_rates.pop_back();
        }

        // add the new hash rate to the list
        let current_hash_rate = difficulty.as_u64() / target_time;
        self.hash_rates.push_front(current_hash_rate);

        // after adding the hash rate we need to recalculate the average
        self.average = self.calculate_average();
    }

    fn is_full(&self) -> bool {
        self.hash_rates.len() >= self.window_size
    }

    fn calculate_average(&self) -> u64 {
        // this check is not strictly necessary as this is only called after adding an item
        // but let's be on the safe side for future changes
        if self.hash_rates.is_empty() {
            return 0;
        }

        let sum: u64 = self.hash_rates.iter().sum();
        let count = self.hash_rates.len() as u64;
        sum / count
    }

    pub fn average(&self) -> u64 {
        self.average
    }
}

#[cfg(test)]
mod test {
    use taiji_core::{
        consensus::{ConsensusConstants, ConsensusManagerBuilder},
        proof_of_work::{Difficulty, PowAlgorithm},
    };
    use taiji_p2p::Network;

    use super::HashRateMovingAverage;

    #[test]
    fn window_is_empty() {
        let hash_rate_ma = create_hash_rate_ma(PowAlgorithm::Sha3x);
        assert!(!hash_rate_ma.is_full());
        assert_eq!(hash_rate_ma.calculate_average(), 0);
        assert_eq!(hash_rate_ma.average(), 0);
    }

    #[test]
    fn window_is_full() {
        let mut hash_rate_ma = create_hash_rate_ma(PowAlgorithm::Sha3x);
        let window_size = hash_rate_ma.window_size;

        // we check that the window is not full when we insert less items than the window size
        for _ in 0..window_size - 1 {
            hash_rate_ma.add(0, Difficulty::min());
            assert!(!hash_rate_ma.is_full());
        }

        // from this point onwards, the window should be always full
        for _ in 0..10 {
            hash_rate_ma.add(0, Difficulty::min());
            assert!(hash_rate_ma.is_full());
        }
    }

    // Checks that the moving average hash rate at every block is correct
    // We use larger sample data than the SHA window size (12 periods) to check bounds
    // We assumed a constant target block time of 300 secs (the SHA3 target time for Esmeralda)
    // These expected hash rate values where calculated in a spreadsheet
    #[test]
    fn correct_moving_average_calculation() {
        let mut hash_rate_ma = create_hash_rate_ma(PowAlgorithm::Sha3x);

        assert_hash_rate(&mut hash_rate_ma, 0, 100_000, 333);
        assert_hash_rate(&mut hash_rate_ma, 1, 120_100, 366);
        assert_hash_rate(&mut hash_rate_ma, 2, 110_090, 366);
        assert_hash_rate(&mut hash_rate_ma, 3, 121_090, 375);
        assert_hash_rate(&mut hash_rate_ma, 4, 150_000, 400);
        assert_hash_rate(&mut hash_rate_ma, 5, 155_000, 419);
        assert_hash_rate(&mut hash_rate_ma, 6, 159_999, 435);
        assert_hash_rate(&mut hash_rate_ma, 7, 160_010, 448);
        assert_hash_rate(&mut hash_rate_ma, 8, 159_990, 457);
        assert_hash_rate(&mut hash_rate_ma, 9, 140_000, 458);
        assert_hash_rate(&mut hash_rate_ma, 10, 137_230, 458);
        assert_hash_rate(&mut hash_rate_ma, 11, 130_000, 456);
        assert_hash_rate(&mut hash_rate_ma, 12, 120_000, 461);
        assert_hash_rate(&mut hash_rate_ma, 13, 140_000, 467);
    }

    // Our moving average windows are very small (12 and 15 depending on PoW algorithm)
    // So we will never get an overflow when we do the sums for the average calculation (we divide by target time)
    // Anyways, just in case we go with huge windows in the future, this test should fail with a panic due to overflow
    #[test]
    fn should_not_overflow() {
        let mut sha3x_hash_rate_ma = create_hash_rate_ma(PowAlgorithm::Sha3x);
        let mut randomx_hash_rate_ma = create_hash_rate_ma(PowAlgorithm::RandomX);
        try_to_overflow(&mut sha3x_hash_rate_ma);
        try_to_overflow(&mut randomx_hash_rate_ma);
    }

    fn try_to_overflow(hash_rate_ma: &mut HashRateMovingAverage) {
        let window_size = hash_rate_ma.window_size;

        for _ in 0..window_size {
            hash_rate_ma.add(0, Difficulty::max());
        }
    }

    fn create_hash_rate_ma(pow_algo: PowAlgorithm) -> HashRateMovingAverage {
        let consensus_manager = ConsensusManagerBuilder::new(Network::Esmeralda)
            .add_consensus_constants(ConsensusConstants::esmeralda()[0].clone())
            .build()
            .unwrap();
        HashRateMovingAverage::new(pow_algo, consensus_manager)
    }

    fn assert_hash_rate(
        moving_average: &mut HashRateMovingAverage,
        height: u64,
        difficulty: u64,
        expected_hash_rate: u64,
    ) {
        moving_average.add(height, Difficulty::from_u64(difficulty).unwrap());
        assert_eq!(moving_average.average(), expected_hash_rate);
    }
}
