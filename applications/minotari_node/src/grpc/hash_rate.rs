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

use std::{cmp::max, collections::VecDeque};

use minotari_app_grpc::tari_rpc;
use tari_core::consensus::BaseNodeConsensusManager;
use tari_transaction_components::tari_proof_of_work::{Difficulty, PowAlgorithm};

const HASH_RATE_MOVING_AVERAGE_WINDOW: usize = 12;
// Number of nanos in one unit number
pub const NANOS_PER_UNIT: u64 = 1_000_000_000;
// Maximum number of nanos we choose to represented in a decimal
pub const MAX_NANOS_PER_DECIMAL: u32 = 999_999_999;

/// Calculates a linear weighted moving average for hash rate calculations
pub struct HashRateMovingAverage {
    pow_algo: PowAlgorithm,
    consensus_manager: BaseNodeConsensusManager,
    window_size: usize,
    hash_rates: VecDeque<u64>,
    average: u64,
    scaling: u64,
}

impl HashRateMovingAverage {
    pub fn new(pow_algo: PowAlgorithm, consensus_manager: BaseNodeConsensusManager, scaling: Option<u64>) -> Self {
        let window_size = HASH_RATE_MOVING_AVERAGE_WINDOW;
        let hash_rates = VecDeque::with_capacity(window_size);

        Self {
            pow_algo,
            consensus_manager,
            window_size,
            hash_rates,
            average: 0,
            scaling: max(scaling.unwrap_or(1), 1),
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
        let current_hash_rate = difficulty.as_u64().saturating_mul(self.scaling) / target_time;
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
        self.average / self.scaling
    }

    pub fn u_decimal_average(&self) -> tari_rpc::UDecimalValue {
        HashRateMovingAverage::average_as_u_decimal(self.average, self.scaling)
    }

    pub fn average_as_u_decimal(average: u64, scaling: u64) -> tari_rpc::UDecimalValue {
        tari_rpc::UDecimalValue {
            units: average / scaling,
            nanos: u32::try_from((average % scaling) * NANOS_PER_UNIT / scaling).unwrap_or(MAX_NANOS_PER_DECIMAL),
        }
    }
}

#[cfg(test)]
mod test {
    #![allow(clippy::indexing_slicing)]
    use tari_core::consensus::BaseNodeConsensusManagerBuilder;
    use tari_p2p::Network;
    use tari_transaction_components::{
        consensus::ConsensusConstants,
        tari_proof_of_work::{Difficulty, PowAlgorithm},
    };

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
    // We assumed a constant target block time of 240 secs (the SHA3 target time for Esmeralda)
    // These expected hash rate values where calculated in a spreadsheet
    #[test]
    fn correct_moving_average_calculation() {
        let mut hash_rate_ma = create_hash_rate_ma(PowAlgorithm::Sha3x);

        assert_hash_rate(&mut hash_rate_ma, 0, 100_000, 416);
        assert_hash_rate(&mut hash_rate_ma, 1, 120_100, 458);
        assert_hash_rate(&mut hash_rate_ma, 2, 110_090, 458);
        assert_hash_rate(&mut hash_rate_ma, 3, 121_090, 469);
        assert_hash_rate(&mut hash_rate_ma, 4, 150_000, 500);
        assert_hash_rate(&mut hash_rate_ma, 5, 155_000, 524);
        assert_hash_rate(&mut hash_rate_ma, 6, 159_999, 544);
        assert_hash_rate(&mut hash_rate_ma, 7, 160_010, 560);
        assert_hash_rate(&mut hash_rate_ma, 8, 159_990, 571);
        assert_hash_rate(&mut hash_rate_ma, 9, 140_000, 572);
        assert_hash_rate(&mut hash_rate_ma, 10, 137_230, 572);
        assert_hash_rate(&mut hash_rate_ma, 11, 130_000, 570);
        assert_hash_rate(&mut hash_rate_ma, 12, 120_000, 577);
        assert_hash_rate(&mut hash_rate_ma, 13, 140_000, 584);
    }

    // Our moving average windows are very small (12 and 15 depending on PoW algorithm)
    // So we will never get an overflow when we do the sums for the average calculation (we divide by target time)
    // Anyways, just in case we go with huge windows in the future, this test should fail with a panic due to overflow
    #[test]
    fn should_not_overflow() {
        let mut sha3x_hash_rate_ma = create_hash_rate_ma(PowAlgorithm::Sha3x);
        let mut randomx_hash_rate_ma = create_hash_rate_ma(PowAlgorithm::RandomXM);
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
        let mut constants = ConsensusConstants::esmeralda()[0].clone();
        constants.set_pow_target_block_interval(pow_algo, 240);
        let consensus_manager = BaseNodeConsensusManagerBuilder::new(Network::Esmeralda)
            .add_consensus_constants(constants)
            .build()
            .unwrap();
        HashRateMovingAverage::new(pow_algo, consensus_manager, None)
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

    #[test]
    fn scaling_is_applied_correctly() {
        // Set up consensus manager
        let mut constants = ConsensusConstants::esmeralda()[ConsensusConstants::esmeralda().len() - 1].clone();
        constants.set_pow_target_block_interval(PowAlgorithm::Sha3x, 100);
        constants.set_pow_target_block_interval(PowAlgorithm::RandomXM, 100);
        constants.set_pow_target_block_interval(PowAlgorithm::RandomXT, 100);
        constants.set_pow_target_block_interval(PowAlgorithm::Cuckaroo, 100);
        let consensus_manager = BaseNodeConsensusManagerBuilder::new(Network::Esmeralda)
            .add_consensus_constants(constants)
            .build()
            .unwrap();

        let mut sha_hash_rate_ma = HashRateMovingAverage::new(PowAlgorithm::Sha3x, consensus_manager.clone(), None);
        let mut rxm_hash_rate_ma = HashRateMovingAverage::new(PowAlgorithm::RandomXM, consensus_manager.clone(), None);
        let mut rxt_hash_rate_ma =
            HashRateMovingAverage::new(PowAlgorithm::RandomXT, consensus_manager.clone(), Some(100));
        let mut c29_hash_rate_ma =
            HashRateMovingAverage::new(PowAlgorithm::Cuckaroo, consensus_manager.clone(), Some(10_000));

        let start_height = 100_000;
        assert_eq!(consensus_manager.consensus_constants(start_height).pow_algo_count(), 4);
        for i in 0..20 {
            sha_hash_rate_ma.add(start_height + i, Difficulty::from_u64(1 + (i % 10) * 10_000).unwrap());
            rxm_hash_rate_ma.add(start_height + i, Difficulty::from_u64(1 + (i % 10) * 1_000).unwrap());
            rxt_hash_rate_ma.add(start_height + i, Difficulty::from_u64(1 + (i % 10) * 100).unwrap());
            c29_hash_rate_ma.add(start_height + i, Difficulty::from_u64(1 + i % 10).unwrap());
        }

        // Check integer output
        assert_eq!(sha_hash_rate_ma.average(), 516);
        assert_eq!(rxm_hash_rate_ma.average(), 51);
        assert_eq!(rxt_hash_rate_ma.average(), 5);
        assert_eq!(c29_hash_rate_ma.average(), 0);

        // Check decimal output
        let decimal = sha_hash_rate_ma.u_decimal_average();
        assert_eq!(decimal.units, 516);
        assert_eq!(decimal.nanos, 0);
        let decimal = rxm_hash_rate_ma.u_decimal_average();
        assert_eq!(decimal.units, 51);
        assert_eq!(decimal.nanos, 0);
        let decimal = rxt_hash_rate_ma.u_decimal_average();
        assert_eq!(decimal.units, 5);
        assert_eq!(decimal.nanos, 170000000);
        let decimal = c29_hash_rate_ma.u_decimal_average();
        assert_eq!(decimal.units, 0);
        assert_eq!(decimal.nanos, 61600000);
    }

    #[test]
    fn conversion_to_average_as_u_decimal_is_applied_correctly() {
        use super::HashRateMovingAverage;

        // Case 1: average = 0, scaling = 1
        let dec = HashRateMovingAverage::average_as_u_decimal(0, 1);
        assert_eq!(dec.units, 0);
        assert_eq!(dec.nanos, 0);

        // Case 2: average = scaling (should be 1 unit, 0 nanos)
        let dec = HashRateMovingAverage::average_as_u_decimal(10_000, 10_000);
        assert_eq!(dec.units, 1);
        assert_eq!(dec.nanos, 0);

        // Case 3: average = half scaling (should be 0 units, 500_000_000 nanos)
        let dec = HashRateMovingAverage::average_as_u_decimal(5_000, 10_000);
        assert_eq!(dec.units, 0);
        assert_eq!(dec.nanos, 500_000_000);

        // Case 4: average = scaling + half scaling (should be 1 unit, 500_000_000 nanos)
        let dec = HashRateMovingAverage::average_as_u_decimal(15_000, 10_000);
        assert_eq!(dec.units, 1);
        assert_eq!(dec.nanos, 500_000_000);

        // Case 5: average = scaling - 1 (should be 0 units, 900_000_000 nanos)
        let dec = HashRateMovingAverage::average_as_u_decimal(9_000, 10_000);
        assert_eq!(dec.units, 0);
        assert_eq!(dec.nanos, 900_000_000);

        // Case 6: average = u64::MAX, scaling = u64::MAX (should be 1 unit, 0 nanos)
        let dec = HashRateMovingAverage::average_as_u_decimal(u64::MAX, u64::MAX);
        assert_eq!(dec.units, 1);
        assert_eq!(dec.nanos, 0);

        // Case 7a: average = 12_345, scaling = 10_000 (should be 1 unit, 234_500_000 nanos)
        let dec = HashRateMovingAverage::average_as_u_decimal(12_345, 10_000);
        assert_eq!(dec.units, 1);
        assert_eq!(dec.nanos, 234_500_000);

        // Case 7b: average = 12_345, scaling = 1_000_000 (should be 1 unit, 12_345_000 nanos)
        let dec = HashRateMovingAverage::average_as_u_decimal(12_345, 1_000_000);
        assert_eq!(dec.units, 0);
        assert_eq!(dec.nanos, 12_345_000);

        // Case 7c: average = 12_345, scaling = 100_000_000_000 (should be 1 unit, 123 nanos)
        let dec = HashRateMovingAverage::average_as_u_decimal(12_345, 100_000_000_000);
        assert_eq!(dec.units, 0);
        assert_eq!(dec.nanos, 123);

        // Case 8: average = 1, scaling = 10_000 (should be 0 units, 100_000 nanos)
        let dec = HashRateMovingAverage::average_as_u_decimal(1, 10_000);
        assert_eq!(dec.units, 0);
        assert_eq!(dec.nanos, 100_000);

        // Case 9: average = 0, scaling = 10_000 (should be 0 units, 0 nanos)
        let dec = HashRateMovingAverage::average_as_u_decimal(0, 10_000);
        assert_eq!(dec.units, 0);
        assert_eq!(dec.nanos, 0);

        // Case 10: average = 4, scaling = 3 (should be 1 units, 333_333_333 nanos)
        let dec = HashRateMovingAverage::average_as_u_decimal(4, 3);
        assert_eq!(dec.units, 1);
        assert_eq!(dec.nanos, 333_333_333);
    }
}
