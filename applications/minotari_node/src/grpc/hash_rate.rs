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

use std::collections::VecDeque;

use minotari_app_grpc::tari_rpc;
use tari_core::consensus::BaseNodeConsensusManager;
use tari_transaction_components::tari_proof_of_work::{Difficulty, PowAlgorithm};

const HASH_RATE_MOVING_AVERAGE_WINDOW: usize = 12;
/// Number of nanos in one unit number
pub const NANOS_PER_UNIT: u64 = 1_000_000_000;
// Maximum number of nanos we choose to represented in a decimal
const MAX_NANOS_PER_DECIMAL: u32 = 999_999_999;

/// Calculates a linear weighted moving average for hash rate calculations
pub struct HashRateMovingAverage {
    pow_algo: PowAlgorithm,
    consensus_manager: BaseNodeConsensusManager,
    window_size: usize,
    hash_rates: VecDeque<u64>,
    average: u64,
    use_scaling: bool,
}

impl HashRateMovingAverage {
    pub fn new(pow_algo: PowAlgorithm, consensus_manager: BaseNodeConsensusManager, use_scaling: bool) -> Self {
        let window_size = HASH_RATE_MOVING_AVERAGE_WINDOW;
        let hash_rates = VecDeque::with_capacity(window_size);

        Self {
            pow_algo,
            consensus_manager,
            window_size,
            hash_rates,
            average: 0,
            use_scaling,
        }
    }

    /// Adds a new hash rate entry in the moving average and recalculates the average
    pub fn add(&mut self, height: u64, difficulty: Difficulty) {
        let consensus_constants = self.consensus_manager.consensus_constants(height);
        // target block time for the current block is provided by the consensus rules
        let target_time = consensus_constants.pow_target_block_interval(self.pow_algo);

        // remove old entries if we are at max block window
        if self.is_full() {
            self.hash_rates.pop_back();
        }

        let additional_multiplier = match self.pow_algo {
            // Cuckaroo is special as we need to multiply by the cycle length to get the hash rate
            PowAlgorithm::Cuckaroo => u128::from(consensus_constants.cuckaroo_cycle_length()),
            _ => 1,
        };
        // add the new hash rate to the list
        let nominator = if self.use_scaling {
            u128::from(difficulty.as_u64())
                .saturating_mul(u128::from(NANOS_PER_UNIT))
                .saturating_mul(additional_multiplier)
        } else {
            u128::from(difficulty.as_u64()).saturating_mul(additional_multiplier)
        };
        let current_hash_rate = nominator
            .checked_div(u128::from(target_time))
            .and_then(|v| u64::try_from(v).ok())
            .unwrap_or(u64::MAX);
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
        // The `is_empty` guard above proves the count is non-zero.
        sum.checked_div(count).unwrap_or(0)
    }

    pub fn average(&self) -> u64 {
        if self.use_scaling {
            self.average.checked_div(NANOS_PER_UNIT).unwrap_or(0)
        } else {
            self.average
        }
    }

    pub fn u_decimal_average(&self) -> tari_rpc::UDecimalValue {
        if self.use_scaling {
            HashRateMovingAverage::average_as_u_decimal(self.average)
        } else {
            tari_rpc::UDecimalValue {
                units: self.average,
                nanos: 0,
            }
        }
    }

    pub fn average_as_u_decimal(average: u64) -> tari_rpc::UDecimalValue {
        tari_rpc::UDecimalValue {
            units: average / NANOS_PER_UNIT,
            nanos: u32::try_from(average % NANOS_PER_UNIT).unwrap_or(MAX_NANOS_PER_DECIMAL),
        }
    }
}

/// Display a UDecimalValue as a string
pub(crate) fn display_u_decimal_value(value: &tari_rpc::UDecimalValue) -> String {
    if value.nanos == 0 {
        format!("{}", value.units)
    } else {
        format!("{}.{:09}", value.units, value.nanos)
            .trim_end_matches('0')
            .to_string()
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

    use super::{HashRateMovingAverage, NANOS_PER_UNIT, display_u_decimal_value};

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
        HashRateMovingAverage::new(pow_algo, consensus_manager, false)
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

        let mut sha_hash_rate_ma = HashRateMovingAverage::new(PowAlgorithm::Sha3x, consensus_manager.clone(), false);
        let mut rxm_hash_rate_ma = HashRateMovingAverage::new(PowAlgorithm::RandomXM, consensus_manager.clone(), false);
        let mut rxt_hash_rate_ma = HashRateMovingAverage::new(PowAlgorithm::RandomXT, consensus_manager.clone(), true);
        let mut c29_hash_rate_ma = HashRateMovingAverage::new(PowAlgorithm::Cuckaroo, consensus_manager.clone(), true);

        let start_height = 100_000;
        assert_eq!(consensus_manager.consensus_constants(start_height).pow_algo_count(), 4);
        for i in 0..20 {
            sha_hash_rate_ma.add(
                start_height + i,
                Difficulty::from_u64(1 + (i % 10) * 1_100_000).unwrap(),
            );
            rxm_hash_rate_ma.add(start_height + i, Difficulty::from_u64(1 + (i % 10) * 10_000).unwrap());
            rxt_hash_rate_ma.add(start_height + i, Difficulty::from_u64(1 + (i % 10) * 100).unwrap());
            c29_hash_rate_ma.add(start_height + i, Difficulty::from_u64(1 + i % 10).unwrap());
        }

        // Check integer and decimal output
        let decimal = sha_hash_rate_ma.u_decimal_average();
        assert_eq!(decimal.units, sha_hash_rate_ma.average());
        assert_eq!(decimal.units, 56833);
        assert_eq!(decimal.nanos, 0);
        let decimal = rxm_hash_rate_ma.u_decimal_average();
        assert_eq!(decimal.units, rxm_hash_rate_ma.average());
        assert_eq!(decimal.units, 516);
        assert_eq!(decimal.nanos, 0);
        let decimal = rxt_hash_rate_ma.u_decimal_average();
        assert_eq!(decimal.units, rxt_hash_rate_ma.average());
        assert_eq!(decimal.units, 5);
        assert_eq!(decimal.nanos, 176666666);
        let decimal = c29_hash_rate_ma.u_decimal_average();
        assert_eq!(decimal.units, c29_hash_rate_ma.average());
        assert_eq!(decimal.units, 2);
        assert_eq!(decimal.nanos, 590000000);
    }

    #[test]
    fn conversion_to_average_as_u_decimal_is_applied_correctly() {
        use super::HashRateMovingAverage;

        // Simple decimal cases
        let dec = HashRateMovingAverage::average_as_u_decimal(0);
        assert_eq!(dec.units, 0);
        assert_eq!(dec.nanos, 0);
        assert_eq!(&display_u_decimal_value(&dec), "0");

        let dec = HashRateMovingAverage::average_as_u_decimal(NANOS_PER_UNIT);
        assert_eq!(dec.units, 1);
        assert_eq!(dec.nanos, 0);
        assert_eq!(&display_u_decimal_value(&dec), "1");

        let dec = HashRateMovingAverage::average_as_u_decimal(NANOS_PER_UNIT / 2);
        assert_eq!(dec.units, 0);
        assert_eq!(dec.nanos, 500_000_000);
        assert_eq!(&display_u_decimal_value(&dec), "0.5");

        let dec = HashRateMovingAverage::average_as_u_decimal(NANOS_PER_UNIT * 3 / 2);
        assert_eq!(dec.units, 1);
        assert_eq!(dec.nanos, 500_000_000);
        assert_eq!(&display_u_decimal_value(&dec), "1.5");

        let dec = HashRateMovingAverage::average_as_u_decimal(NANOS_PER_UNIT / 10_000);
        assert_eq!(dec.units, 0);
        assert_eq!(dec.nanos, 100_000);
        assert_eq!(&display_u_decimal_value(&dec), "0.0001");

        // Huge numbers cases
        let dec = HashRateMovingAverage::average_as_u_decimal(u64::MAX / 123);
        assert_eq!(dec.units, 149_973_529);
        assert_eq!(dec.nanos, 54_549_200);
        assert_eq!(&display_u_decimal_value(&dec), "149973529.0545492"); // Actual value is 149973529,0545492001219....

        let dec = HashRateMovingAverage::average_as_u_decimal(u64::MAX / 12345);
        assert_eq!(dec.units, 1_494_268);
        assert_eq!(dec.nanos, 454_735_484);
        assert_eq!(&display_u_decimal_value(&dec), "1494268.454735484"); // Actual value is 1494268,4547354841324...

        // Digits approaching the accuracy limit cases
        let dec = HashRateMovingAverage::average_as_u_decimal(NANOS_PER_UNIT * 12_345 / 10_000);
        assert_eq!(dec.units, 1);
        assert_eq!(dec.nanos, 234_500_000);
        assert_eq!(&display_u_decimal_value(&dec), "1.2345");

        let dec = HashRateMovingAverage::average_as_u_decimal(NANOS_PER_UNIT * 12_345 / 1_000_000);
        assert_eq!(dec.units, 0);
        assert_eq!(dec.nanos, 12_345_000);
        assert_eq!(&display_u_decimal_value(&dec), "0.012345");

        let dec = HashRateMovingAverage::average_as_u_decimal(NANOS_PER_UNIT * 12_345 / 100_000_000_000);
        assert_eq!(dec.units, 0);
        assert_eq!(dec.nanos, 123); // Digits '4' and '5' are lost due to accuracy limit
        assert_eq!(&display_u_decimal_value(&dec), "0.000000123"); // Actual value is 0,00000012345

        // Repetitive digits cases
        let dec = HashRateMovingAverage::average_as_u_decimal(NANOS_PER_UNIT * 4 / 3);
        assert_eq!(dec.units, 1);
        assert_eq!(dec.nanos, 333_333_333);
        assert_eq!(&display_u_decimal_value(&dec), "1.333333333"); // Actual value is 1,3333333333333333333...

        let dec = HashRateMovingAverage::average_as_u_decimal(NANOS_PER_UNIT * 13 / 11);
        assert_eq!(dec.units, 1);
        assert_eq!(dec.nanos, 181_818_181);
        assert_eq!(&display_u_decimal_value(&dec), "1.181818181"); // Actual value is 1,1818181818181818181...
    }
}
