// LWMA-1 for BTC & Zcash clones
// Copyright (c) 2017-2019 The Bitcoin Gold developers, Zawy, iamstenman (Microbitcoin)
// MIT License
// Algorithm by Zawy, a modification of WT-144 by Tom Harding
// References:
// https://github.com/zawy12/difficulty-algorithms/issues/3#issuecomment-442129791
// https://github.com/zcash/zcash/issues/4021

use crate::proof_of_work::{
    difficulty::{Difficulty, DifficultyAdjustment},
    error::DifficultyAdjustmentError,
};
use log::*;
use std::cmp;
use tari_crypto::tari_utilities::epoch_time::EpochTime;

pub const LOG_TARGET: &str = "c::pow::lwma_diff";

#[derive(Debug, Clone)]
pub struct LinearWeightedMovingAverage {
    target_difficulties: Vec<(EpochTime, Difficulty)>,
    block_window: usize,
    target_time: u64,
    max_block_time: u64,
}

impl LinearWeightedMovingAverage {
    pub fn new(block_window: usize, target_time: u64, max_block_time: u64) -> Self {
        Self {
            target_difficulties: Vec::with_capacity(block_window + 1),
            block_window,
            target_time,
            max_block_time,
        }
    }

    fn calculate(&self) -> Option<Difficulty> {
        if self.target_difficulties.len() <= 1 {
            return None;
        }

        // Use the array length rather than block_window to include early cases where the no. of pts < block_window
        let n = (self.target_difficulties.len() - 1) as u64;

        let mut weighted_times: u64 = 0;
        let difficulty = self
            .target_difficulties
            .iter()
            .skip(1)
            .fold(0u64, |difficulty, (_, d)| difficulty + d.as_u64());

        let ave_difficulty = difficulty as f64 / n as f64;

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
            let solve_time = cmp::min((this_timestamp - previous_timestamp).as_u64(), self.max_block_time);
            previous_timestamp = this_timestamp;

            // Give linearly higher weight to more recent solve times.
            // Note: This will not overflow for practical values of block_window and solve time.
            weighted_times += solve_time * (i + 1) as u64;
        }
        // k is the sum of weights (1+2+..+n) * target_time
        let k = n * (n + 1) * self.target_time / 2;
        let target = ave_difficulty * k as f64 / weighted_times as f64;
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
        if target > std::u64::MAX as f64 {
            error!(
                target: LOG_TARGET,
                "Difficulty has overflowed, current is: {:?}", target
            );
            panic!("Difficulty target has overflowed. Target is {}", target);
        }
        let target = target.ceil() as u64; // difficulty difference of 1 should not matter much, but difficulty should never be below 1, ceil(0.9) = 1
        trace!(target: LOG_TARGET, "New target difficulty: {}", target);
        Some(target.into())
    }

    #[inline]
    fn capacity(&self) -> usize {
        self.target_difficulties.capacity()
    }

    #[inline]
    pub fn is_at_capacity(&self) -> bool {
        self.num_samples() == self.capacity()
    }

    #[inline]
    pub fn num_samples(&self) -> usize {
        self.target_difficulties.len()
    }

    #[inline]
    pub(super) fn block_window(&self) -> usize {
        self.block_window
    }

    pub fn add_front(&mut self, timestamp: EpochTime, target_difficulty: Difficulty) {
        debug_assert!(
            self.num_samples() <= self.block_window() + 1,
            "LinearWeightedMovingAverage: len exceeded block_window"
        );
        if self.is_at_capacity() {
            self.target_difficulties.pop();
        }
        self.target_difficulties.insert(0, (timestamp, target_difficulty));
    }

    pub fn add_back(&mut self, timestamp: EpochTime, target_difficulty: Difficulty) {
        debug_assert!(
            self.num_samples() <= self.block_window() + 1,
            "LinearWeightedMovingAverage: len exceeded block_window"
        );
        if self.is_at_capacity() {
            self.target_difficulties.remove(0);
        }
        self.target_difficulties.push((timestamp, target_difficulty));
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
    use super::*;

    #[test]
    fn lwma_zero_len() {
        let dif = LinearWeightedMovingAverage::new(90, 120, 1.into(), 120 * 6);
        assert_eq!(dif.get_difficulty(), Difficulty::min());
    }

    #[test]
    fn lwma_is_at_capacity() {
        // This is important to check because using a VecDeque can cause bugs unless the following is accounted for
        // let v = VecDeq::with_capacity(10);
        // assert_eq!(v.capacity(), 11);
        // A Vec was chosen because it ended up being simpler to use
        let dif = LinearWeightedMovingAverage::new(0, 120, 1.into(), 120 * 6);
        assert_eq!(dif.capacity(), 1);
        let mut dif = LinearWeightedMovingAverage::new(1, 120, 1.into(), 120 * 6);
        dif.add_front(60.into(), 100.into());
        assert_eq!(dif.capacity(), 2);
        assert_eq!(dif.num_samples(), 1);
        dif.add_front(60.into(), 100.into());
        assert_eq!(dif.num_samples(), 2);
        assert_eq!(dif.capacity(), 2);
        dif.add_front(60.into(), 100.into());
        assert_eq!(dif.num_samples(), 2);
        assert_eq!(dif.capacity(), 2);
    }

    #[test]
    fn lwma_negative_solve_times() {
        let mut dif = LinearWeightedMovingAverage::new(90, 120, 1.into(), 120 * 6);
        let mut timestamp = 60.into();
        let cum_diff = Difficulty::from(100);
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
            let diff_before = dif.get_difficulty();
            let _ = dif.add(timestamp, cum_diff);
            let diff_after = dif.get_difficulty();
            // Algo should handle this as 1sec solve time thus increase the difficulty constantly
            assert!(diff_after > diff_before);
        }
    }

    #[test]
    fn lwma_limit_difficulty_change() {
        let mut dif = LinearWeightedMovingAverage::new(5, 60, 1.into(), 60 * 6);
        let _ = dif.add(60.into(), 100.into());
        let _ = dif.add(10_000_000.into(), 100.into());
        assert_eq!(dif.get_difficulty(), 17.into());
        let _ = dif.add(20_000_000.into(), 16.into());
        assert_eq!(dif.get_difficulty(), 10.into());
    }

    // Data for 5-period moving average
    // Timestamp: 60, 120, 180, 240, 300, 350, 380, 445, 515, 615, 975, 976, 977, 978, 979
    // Intervals: 60,  60,  60,  60,  60,  50,  30,  65,  70, 100, 360,   1,   1,   1,   1
    // Diff:     100, 100, 100, 100, 100, 105, 128, 123, 116,  94,  39,  46,  55,  75, 148
    // Acum dif: 100, 200, 300, 400, 500, 605, 733, 856, 972,1066,1105,1151,1206,1281,1429
    // Target:     1, 100, 100, 100, 100, 107, 136, 130, 120,  94,  36,  39,  47,  67, 175
    #[test]
    fn lwma_calculate() {
        let mut dif = LinearWeightedMovingAverage::new(5, 60, 1.into(), 60 * 6);
        let _ = dif.add(60.into(), 100.into());
        assert_eq!(dif.get_difficulty(), 1.into());
        let _ = dif.add(120.into(), 100.into());
        assert_eq!(dif.get_difficulty(), 100.into());
        let _ = dif.add(180.into(), 100.into());
        assert_eq!(dif.get_difficulty(), 100.into());
        let _ = dif.add(240.into(), 100.into());
        assert_eq!(dif.get_difficulty(), 100.into());
        let _ = dif.add(300.into(), 100.into());
        assert_eq!(dif.get_difficulty(), 100.into());
        let _ = dif.add(350.into(), 105.into());
        assert_eq!(dif.get_difficulty(), 107.into());
        let _ = dif.add(380.into(), 128.into());
        assert_eq!(dif.get_difficulty(), 136.into());
        let _ = dif.add(445.into(), 123.into());
        assert_eq!(dif.get_difficulty(), 130.into());
        let _ = dif.add(515.into(), 116.into());
        assert_eq!(dif.get_difficulty(), 120.into());
        let _ = dif.add(615.into(), 94.into());
        assert_eq!(dif.get_difficulty(), 94.into());
        let _ = dif.add(975.into(), 39.into());
        assert_eq!(dif.get_difficulty(), 36.into());
        let _ = dif.add(976.into(), 46.into());
        assert_eq!(dif.get_difficulty(), 39.into());
        let _ = dif.add(977.into(), 55.into());
        assert_eq!(dif.get_difficulty(), 47.into());
        let _ = dif.add(978.into(), 75.into());
        assert_eq!(dif.get_difficulty(), 67.into());
        let _ = dif.add(979.into(), 148.into());
        assert_eq!(dif.get_difficulty(), 175.into());
    }
}
