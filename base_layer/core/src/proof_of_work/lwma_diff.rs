// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

// Portions of the code:

// LWMA-1 for BTC & Zcash clones
// Copyright (c) 2017-2019 The Bitcoin Gold developers, Zawy, iamstenman (Microbitcoin)
// MIT License
// Algorithm by Zawy, a modification of WT-144 by Tom Harding
// References:
// https://github.com/zawy12/difficulty-algorithms/issues/3#issuecomment-442129791
// https://github.com/zcash/zcash/issues/4021

use std::{cmp::min, collections::VecDeque, convert::TryFrom};

use log::*;
use tari_utilities::epoch_time::EpochTime;

use crate::proof_of_work::{
    difficulty::{Difficulty, DifficultyAdjustment},
    error::DifficultyAdjustmentError,
    DifficultyError,
};

/// This is the recommended maximum block time ratio for LWMA-1
pub const LWMA_MAX_BLOCK_TIME_RATIO: u64 = 6;

/// Log target for `c::pow::lwma_diff`
pub const LOG_TARGET: &str = "c::pow::lwma_diff";

/// Struct for the Linear Weighted Moving Average (LWMA) difficulty adjustment algorithm
#[derive(Debug, Clone)]
pub struct LinearWeightedMovingAverage {
    target_difficulties: VecDeque<(EpochTime, Difficulty)>,
    block_window: usize,
    target_time: u128,
    max_block_time: u64,
}

impl LinearWeightedMovingAverage {
    /// Initialize a new `LinearWeightedMovingAverage`
    pub fn new(block_window: usize, target_time: u64) -> Result<Self, String> {
        if target_time == 0 {
            return Err(
                "LinearWeightedMovingAverage::new(...) expected `target_time` to be greater than 0, but 0 was given"
                    .into(),
            );
        }
        if block_window == 0 {
            return Err(
                "LinearWeightedMovingAverage::new(...) expected `block_window` to be greater than 0, but 0 was given"
                    .into(),
            );
        }
        if target_time.checked_mul(LWMA_MAX_BLOCK_TIME_RATIO).is_none() {
            return Err(format!(
                "LinearWeightedMovingAverage::new(...) expected `target_time` to be at least {} times smaller than \
                 `u64::MAX`",
                LWMA_MAX_BLOCK_TIME_RATIO,
            ));
        }
        Ok(Self {
            target_difficulties: VecDeque::with_capacity(block_window + 1),
            block_window,
            target_time: u128::from(target_time),
            max_block_time: target_time * LWMA_MAX_BLOCK_TIME_RATIO,
        })
    }

    /// Helper function to calculate the maximum block time for a given target time
    pub fn max_block_time(target_time: u64) -> Result<u64, DifficultyError> {
        target_time
            .checked_mul(LWMA_MAX_BLOCK_TIME_RATIO)
            .ok_or(DifficultyError::MaxBlockTimeOverflow)
    }

    fn calculate(&self) -> Option<Difficulty> {
        // This function uses u128 internally for most of the math as its possible to have an overflow with large
        // difficulties and large block windows
        if self.target_difficulties.len() <= 1 {
            return None;
        }

        // Use the array length rather than block_window to include early cases where the no. of pts < block_window
        let n = (self.target_difficulties.len() - 1) as u128;

        let mut weighted_times: u128 = 0;
        let difficulty_sum = self
            .target_difficulties
            .iter()
            .skip(1)
            .fold(0u128, |difficulty, (_, d)| difficulty + u128::from(d.as_u64()));

        let ave_difficulty = difficulty_sum / n;

        let (mut previous_timestamp, _) = self.target_difficulties[0];
        let mut this_timestamp;
        // Loop through N most recent blocks.
        for (i, (timestamp, _)) in self.target_difficulties.iter().skip(1).enumerate() {
            // We cannot have if solve_time < 1 then solve_time = 1, this will greatly increase the next timestamp
            // difficulty which will lower the difficulty
            if *timestamp > previous_timestamp {
                this_timestamp = *timestamp;
            } else {
                this_timestamp = previous_timestamp.increase(1);
            }
            let solve_time = min((this_timestamp - previous_timestamp).as_u64(), self.max_block_time);
            previous_timestamp = this_timestamp;

            // Give linearly higher weight to more recent solve times.
            // Note: This will not overflow for practical values of block_window and solve time.
            weighted_times += u128::from(solve_time * (i + 1) as u64);
        }
        // k is the sum of weights (1+2+..+n) * target_time
        let k = n * (n + 1) * self.target_time / 2;
        let target = u64::try_from(ave_difficulty * k / weighted_times).unwrap_or(u64::MAX);
        trace!(
            target: LOG_TARGET,
            "DiffCalc; t={}; bw={}; n={}; ts[0]={}; ts[n]={}; weighted_ts={}; k={}; diff[0]={}; diff[n]={}; \
             ave_difficulty={}; target={}",
            self.target_time,
            self.block_window,
            n,
            self.target_difficulties[0].0,
            self.target_difficulties[n as usize].0,
            weighted_times,
            k,
            self.target_difficulties[0].1,
            self.target_difficulties[n as usize].1,
            ave_difficulty,
            target
        );
        trace!(target: LOG_TARGET, "New target difficulty: {}", target);
        if target < Difficulty::min().as_u64() {
            None
        } else {
            Some(Difficulty::from_u64(target).expect("Difficulty is valid"))
        }
    }

    /// Indicates if the `LinearWeightedMovingAverage` is full
    pub fn is_full(&self) -> bool {
        self.num_samples() == self.block_window() + 1
    }

    /// Returns the number of samples in the `LinearWeightedMovingAverage`
    #[inline]
    pub fn num_samples(&self) -> usize {
        self.target_difficulties.len()
    }

    /// Returns the block window size
    #[inline]
    pub(super) fn block_window(&self) -> usize {
        self.block_window
    }

    /// Adds a new timestamp and target difficulty in front of the queue
    pub fn add_front(&mut self, timestamp: EpochTime, target_difficulty: Difficulty) {
        if self.is_full() {
            self.target_difficulties.pop_back();
        }
        self.target_difficulties.push_front((timestamp, target_difficulty));
    }

    /// Adds a new timestamp and target difficulty at the back of the queue
    pub fn add_back(&mut self, timestamp: EpochTime, target_difficulty: Difficulty) {
        if self.is_full() {
            self.target_difficulties.pop_front();
        }
        self.target_difficulties.push_back((timestamp, target_difficulty));
    }
}

impl DifficultyAdjustment for LinearWeightedMovingAverage {
    fn add(&mut self, timestamp: EpochTime, target_difficulty: Difficulty) -> Result<(), DifficultyAdjustmentError> {
        self.add_back(timestamp, target_difficulty);
        Ok(())
    }

    fn get_difficulty(&self) -> Option<Difficulty> {
        self.calculate()
    }
}

#[cfg(test)]
mod test {
    use crate::proof_of_work::{lwma_diff::LinearWeightedMovingAverage, Difficulty, DifficultyAdjustment};

    #[test]
    fn lwma_zero_len() {
        let dif = LinearWeightedMovingAverage::new(90, 120).unwrap();
        assert_eq!(dif.get_difficulty(), None);
    }

    #[test]
    fn lwma_is_full() {
        // This is important to check because using a VecDeque can cause bugs unless the following is accounted for
        // let v = VecDeq::with_capacity(10);
        // assert_eq!(v.capacity(), 11);
        // A Vec was chosen because it ended up being simpler to use
        let dif = LinearWeightedMovingAverage::new(0, 120);
        assert!(dif.is_err());
        let mut dif = LinearWeightedMovingAverage::new(1, 120).unwrap();
        dif.add_front(60.into(), Difficulty::from_u64(100).unwrap());
        assert!(!dif.is_full());
        assert_eq!(dif.num_samples(), 1);
        dif.add_front(60.into(), Difficulty::from_u64(100).unwrap());
        assert_eq!(dif.num_samples(), 2);
        assert!(dif.is_full());
        dif.add_front(60.into(), Difficulty::from_u64(100).unwrap());
        assert_eq!(dif.num_samples(), 2);
        assert!(dif.is_full());
    }

    #[test]
    fn lwma_negative_solve_times() {
        let mut dif = LinearWeightedMovingAverage::new(90, 120).unwrap();
        let mut timestamp = 60.into();
        let cum_diff = Difficulty::from_u64(100).unwrap();
        let _ = dif.add(timestamp, cum_diff);
        timestamp = timestamp.increase(60);
        let _ = dif.add(timestamp, cum_diff);
        // Lets create a history and populate the vecs
        for _i in 0..150 {
            timestamp = timestamp.increase(60);
            let _ = dif.add(timestamp, cum_diff);
        }
        // lets create chaos by having 60 blocks as negative solve times. This should never be allowed in practice by
        // having checks on the block times.
        for _i in 0..60 {
            timestamp = (timestamp.as_u64() - 1).into(); // Only choosing -1 here since we are testing negative solve times and we cannot have 0 time
            let diff_before = dif.get_difficulty().unwrap();
            let _ = dif.add(timestamp, cum_diff);
            let diff_after = dif.get_difficulty().unwrap();
            // Algo should handle this as 1sec solve time thus increase the difficulty constantly
            assert!(diff_after > diff_before);
        }
    }

    #[test]
    fn lwma_limit_difficulty_change() {
        let mut dif = LinearWeightedMovingAverage::new(5, 60).unwrap();
        let _ = dif.add(60.into(), Difficulty::from_u64(100).unwrap());
        let _ = dif.add(10_000_000.into(), Difficulty::from_u64(100).unwrap());
        assert_eq!(dif.get_difficulty().unwrap(), Difficulty::from_u64(16).unwrap());
        let _ = dif.add(20_000_000.into(), Difficulty::from_u64(16).unwrap());
        assert_eq!(dif.get_difficulty().unwrap(), Difficulty::from_u64(9).unwrap());
    }

    // Data for 5-period moving average
    // Timestamp: 60, 120, 180, 240, 300, 350, 380, 445, 515, 615, 975, 976, 977, 978, 979
    // Intervals: 60,  60,  60,  60,  60,  50,  30,  65,  70, 100, 360,   1,   1,   1,   1
    // Diff:     100, 100, 100, 100, 100, 105, 128, 123, 116,  94,  39,  46,  55,  75, 148
    // Acum dif: 100, 200, 300, 400, 500, 605, 733, 856, 972,1066,1105,1151,1206,1281,1429
    // Target:     1, 100, 100, 100, 100, 106 134,  128, 119,  93,  35,  38,  46,  65, 173
    // These values where calculated in excel to confirm they are correct
    #[test]
    fn lwma_calculate() {
        let mut dif = LinearWeightedMovingAverage::new(5, 60).unwrap();
        let _ = dif.add(60.into(), Difficulty::from_u64(100).unwrap());
        assert_eq!(dif.get_difficulty(), None);
        let _ = dif.add(120.into(), Difficulty::from_u64(100).unwrap());
        assert_eq!(dif.get_difficulty().unwrap(), Difficulty::from_u64(100).unwrap());
        let _ = dif.add(180.into(), Difficulty::from_u64(100).unwrap());
        assert_eq!(dif.get_difficulty().unwrap(), Difficulty::from_u64(100).unwrap());
        let _ = dif.add(240.into(), Difficulty::from_u64(100).unwrap());
        assert_eq!(dif.get_difficulty().unwrap(), Difficulty::from_u64(100).unwrap());
        let _ = dif.add(300.into(), Difficulty::from_u64(100).unwrap());
        assert_eq!(dif.get_difficulty().unwrap(), Difficulty::from_u64(100).unwrap());
        let _ = dif.add(350.into(), Difficulty::from_u64(105).unwrap());
        assert_eq!(dif.get_difficulty().unwrap(), Difficulty::from_u64(106).unwrap());
        let _ = dif.add(380.into(), Difficulty::from_u64(128).unwrap());
        assert_eq!(dif.get_difficulty().unwrap(), Difficulty::from_u64(134).unwrap());
        let _ = dif.add(445.into(), Difficulty::from_u64(123).unwrap());
        assert_eq!(dif.get_difficulty().unwrap(), Difficulty::from_u64(128).unwrap());
        let _ = dif.add(515.into(), Difficulty::from_u64(116).unwrap());
        assert_eq!(dif.get_difficulty().unwrap(), Difficulty::from_u64(119).unwrap());
        let _ = dif.add(615.into(), Difficulty::from_u64(94).unwrap());
        assert_eq!(dif.get_difficulty().unwrap(), Difficulty::from_u64(93).unwrap());
        let _ = dif.add(975.into(), Difficulty::from_u64(39).unwrap());
        assert_eq!(dif.get_difficulty().unwrap(), Difficulty::from_u64(35).unwrap());
        let _ = dif.add(976.into(), Difficulty::from_u64(46).unwrap());
        assert_eq!(dif.get_difficulty().unwrap(), Difficulty::from_u64(38).unwrap());
        let _ = dif.add(977.into(), Difficulty::from_u64(55).unwrap());
        assert_eq!(dif.get_difficulty().unwrap(), Difficulty::from_u64(46).unwrap());
        let _ = dif.add(978.into(), Difficulty::from_u64(75).unwrap());
        assert_eq!(dif.get_difficulty().unwrap(), Difficulty::from_u64(65).unwrap());
        let _ = dif.add(979.into(), Difficulty::from_u64(148).unwrap());
        assert_eq!(dif.get_difficulty().unwrap(), Difficulty::from_u64(173).unwrap());
    }

    #[test]
    fn ensure_calculate_does_not_overflow_with_large_block_window() {
        let mut dif = LinearWeightedMovingAverage::new(6000, 60).unwrap();
        for _i in 0..6000 {
            let _ = dif.add(60.into(), Difficulty::max());
        }
        // We don't care about the value, we just want to test that get_difficulty does not panic with an overflow.
        dif.get_difficulty().unwrap();
    }
}
