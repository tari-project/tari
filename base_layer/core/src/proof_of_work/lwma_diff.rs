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
use tari_transaction_components::tari_proof_of_work::{Difficulty, DifficultyAdjustment, DifficultyError};
use tari_utilities::epoch_time::EpochTime;

use crate::proof_of_work::pow_backoff::MAX_POW_BACKOFF_MODIFIER;
/// This is the recommended maximum block time ratio for LWMA-1
pub const LWMA_MAX_BLOCK_TIME_RATIO: u64 = 6;

/// Log target for `c::pow::lwma_diff`
pub const LOG_TARGET: &str = "c::pow::lwma_diff";

/// An upper bound on a sane `target_time`, in seconds (about a year).
///
/// This is not a consensus rule, it is a tripwire. Two products in `raw_difficulty` grow with `target_time`:
///
/// - the scaled solve time, `raw_solve_time * MAX_POW_BACKOFF_MODIFIER * target`, where `raw_solve_time` is first
///   capped at `target_time * LWMA_MAX_BLOCK_TIME_RATIO * MAX_POW_BACKOFF_MODIFIER^2` and `target` is at most
///   `u64::MAX`. This reaches `u128::MAX` at a `target_time` of about 9.4e13 seconds.
/// - `ave_difficulty * k`, where `k` is `n * (n + 1) * target_time * MAX_POW_BACKOFF_MODIFIER / 2`. At the largest
///   window the tests exercise (n = 6000) this reaches `u128::MAX` at a `target_time` of about 3.2e10 seconds.
///
/// A year is roughly 3.2e7 seconds, so the tighter of the two bounds still leaves three orders of magnitude of head
/// room; this assertion just makes a nonsensical configuration fail loudly in debug builds.
const MAX_SANE_TARGET_TIME: u64 = 365 * 24 * 60 * 60;

/// Struct for the Linear Weighted Moving Average (LWMA) difficulty adjustment algorithm
#[derive(Debug, Clone)]
pub struct LinearWeightedMovingAverage {
    /// `(timestamp, target difficulty, adjusted target difficulty)`.
    ///
    /// The adjusted target is the bar the block's proof of work actually had to clear, i.e. the target with the
    /// TIP-RFC-MT-0004 same-algorithm backoff applied and then clamped. The ratio `adjusted / target` is the
    /// *effective* modifier that was in force for that block, which is what the solve time must be normalised by.
    /// Recording the pair rather than the nominal modifier matters whenever a clamp binds: a block whose adjusted
    /// target was capped at `max_pow_difficulty` was not actually mined against the full nominal modifier, and
    /// de-normalising by the nominal value would over-correct and drive the target away from the real hash rate.
    target_difficulties: VecDeque<(EpochTime, Difficulty, Difficulty)>,
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
                "LinearWeightedMovingAverage::new(...) expected `target_time` to be at least \
                 {LWMA_MAX_BLOCK_TIME_RATIO} times smaller than `u64::MAX`",
            ));
        }
        debug_assert!(
            target_time <= MAX_SANE_TARGET_TIME,
            "target_time {target_time} is far beyond any sane block interval; the scaled u128 arithmetic in \
             `raw_difficulty` assumes it stays well below this bound"
        );
        Ok(Self {
            target_difficulties: VecDeque::with_capacity(block_window.saturating_add(1)),
            block_window,
            target_time: u128::from(target_time),
            // The `checked_mul` guard above proves this cannot overflow.
            max_block_time: target_time.saturating_mul(LWMA_MAX_BLOCK_TIME_RATIO),
        })
    }

    /// Helper function to calculate the maximum block time for a given target time
    pub fn max_block_time(target_time: u64) -> Result<u64, DifficultyError> {
        target_time
            .checked_mul(LWMA_MAX_BLOCK_TIME_RATIO)
            .ok_or(DifficultyError::MaxBlockTimeOverflow)
    }

    /// Calculates the raw (unclamped) LWMA target difficulty for the next block, or `None` if there is insufficient
    /// data or the computed value is below the minimum difficulty.
    ///
    /// The integer arithmetic is scaled by the compile-time constant [`MAX_POW_BACKOFF_MODIFIER`] *always*, never by
    /// the per-height cap. With every modifier equal to 1 (i.e. all pre-fork blocks) `weighted_times` and `k` are
    /// both multiplied by the same constant, which cancels, so the result is bit-identical to the unscaled formula.
    pub fn raw_difficulty(&self) -> Option<u64> {
        // This function uses u128 internally for most of the math as its possible to have an overflow with large
        // difficulties and large block windows
        if self.target_difficulties.len() <= 1 {
            return None;
        }

        // Use the array length rather than block_window to include early cases where the no. of pts < block_window
        // The guard above proves the length is at least 2.
        let n = self.target_difficulties.len().saturating_sub(1) as u128;

        let mut weighted_times: u128 = 0;
        let difficulty_sum = self
            .target_difficulties
            .iter()
            .skip(1)
            .fold(0u128, |difficulty, (_, d, _)| {
                difficulty.saturating_add(u128::from(d.as_u64()))
            });

        // NOTE: the front entry is deliberately excluded from `difficulty_sum` (and from the solve time loop below,
        // which also skips it and only uses it as the first `previous_timestamp`). That is what makes a front entry
        // whose target difficulty sits below `min_pow_difficulty` - the genesis block, whose recorded target is
        // `Difficulty::min()` - harmless: it never contributes to `ave_difficulty` and its own modifier is never
        // read, only the gap it opens.
        // `n >= 1` because the length is at least 2.
        let ave_difficulty = difficulty_sum.checked_div(n)?;

        let &(mut previous_timestamp, _, _) = self.target_difficulties.front().expect("Already checked");
        let mut this_timestamp;
        let max_scaled_block_time = u128::from(self.max_block_time) * u128::from(MAX_POW_BACKOFF_MODIFIER);
        // Normalising can only ever shrink a solve time, and never by more than `MAX_POW_BACKOFF_MODIFIER` (see
        // `PowBackoffTracker::modifier_for`). Anything above this bound therefore normalises to at least
        // `max_scaled_block_time` and is clamped regardless, so capping the raw value first is lossless and keeps the
        // multiplication below well inside u128.
        let max_raw_solve_time = max_scaled_block_time * u128::from(MAX_POW_BACKOFF_MODIFIER);
        // Loop through N most recent blocks.
        for (i, (timestamp, target, adjusted_target)) in self.target_difficulties.iter().skip(1).enumerate() {
            // We cannot have if solve_time < 1 then solve_time = 1, this will greatly increase the next timestamp
            // difficulty which will lower the difficulty
            if *timestamp > previous_timestamp {
                this_timestamp = *timestamp;
            } else {
                this_timestamp = previous_timestamp.checked_add(EpochTime::from(1))?;
            }
            let raw_solve_time = min(
                u128::from(
                    this_timestamp
                        .checked_sub(previous_timestamp)
                        .unwrap_or(EpochTime::from(1)) // this should never occur
                        .as_u64(),
                ),
                max_raw_solve_time,
            );
            previous_timestamp = this_timestamp;

            // Normalise the solve time by the *effective* modifier `adjusted_target / target` that was in force for
            // the block that closes this gap, so that a block mined against an inflated target is not misread as a
            // hash rate drop. The division by `adjusted_target` is done last to keep the scaled integer arithmetic as
            // precise as possible. Pre-fork blocks have `adjusted_target == target`, so this reduces to
            // `raw_solve_time * MAX_POW_BACKOFF_MODIFIER` exactly.
            let target = u128::from(target.as_u64());
            let adjusted_target = u128::from(adjusted_target.as_u64()).max(target);
            let scaled_solve_time = min(
                raw_solve_time * u128::from(MAX_POW_BACKOFF_MODIFIER) * target / adjusted_target,
                max_scaled_block_time,
            );

            // Give linearly higher weight to more recent solve times.
            // Note: This will not overflow for practical values of block_window and solve time.
            weighted_times =
                weighted_times.saturating_add(scaled_solve_time.saturating_mul(i.saturating_add(1) as u128));
        }
        // k is the sum of weights (1+2+..+n) * target_time, scaled by the same constant as the solve times
        let k = n
            .saturating_mul(n.saturating_add(1))
            .saturating_mul(self.target_time)
            .saturating_mul(u128::from(MAX_POW_BACKOFF_MODIFIER)) /
            2;
        // Each scaled solve time is at least 1, so `weighted_times` is non-zero.
        let target = u64::try_from(ave_difficulty.saturating_mul(k).checked_div(weighted_times)?).unwrap_or(u64::MAX);
        trace!(
            target: LOG_TARGET,
            "DiffCalc; t={}; bw={}; n={}; ts[0]={}; ts[n]={}; weighted_ts={}; k={}; diff[0]={}; diff[n]={}; \
             ave_difficulty={}; target={}",
            self.target_time,
            self.block_window,
            n,
            self.target_difficulties.front().expect("Already checked").0,
            self.target_difficulties.get(n as usize).expect("Already checked").0,
            weighted_times,
            k,
            self.target_difficulties.front().expect("Already checked").1,
            self.target_difficulties.get(n as usize).expect("Already checked").1,
            ave_difficulty,
            target
        );
        trace!(target: LOG_TARGET, "New target difficulty: {target}");
        if target < Difficulty::min().as_u64() {
            None
        } else {
            Some(target)
        }
    }

    fn calculate(&self) -> Option<Difficulty> {
        self.raw_difficulty()
            .map(|target| Difficulty::from_u64(target).expect("Difficulty is valid"))
    }

    /// Indicates if the `LinearWeightedMovingAverage` is full
    pub fn is_full(&self) -> bool {
        self.num_samples() == self.block_window().saturating_add(1)
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

    /// Adds a new timestamp, target difficulty and adjusted target difficulty in front of the queue
    pub fn add_front(&mut self, timestamp: EpochTime, target: Difficulty, adjusted_target: Difficulty) {
        if self.is_full() {
            self.target_difficulties.pop_back();
        }
        self.target_difficulties
            .push_front((timestamp, target, adjusted_target));
    }

    /// Adds a new timestamp, target difficulty and adjusted target difficulty at the back of the queue
    pub fn add_back(&mut self, timestamp: EpochTime, target: Difficulty, adjusted_target: Difficulty) {
        if self.is_full() {
            self.target_difficulties.pop_front();
        }
        self.target_difficulties.push_back((timestamp, target, adjusted_target));
    }

    /// Resizes the block window, dropping the oldest entries if the window shrank. This is needed because the window
    /// size changes at a hard fork while a header sync `TargetDifficulties` is live.
    pub fn update_block_window(&mut self, block_window: usize) -> Result<(), String> {
        if block_window == 0 {
            return Err(
                "LinearWeightedMovingAverage::update_block_window(...) expected `block_window` to be greater than 0, \
                 but 0 was given"
                    .into(),
            );
        }
        self.block_window = block_window;
        while self.target_difficulties.len() > block_window + 1 {
            self.target_difficulties.pop_front();
        }
        Ok(())
    }

    pub fn update_target_time(&mut self, target_time: u64) -> Result<(), String> {
        if target_time == 0 {
            return Err(
                "LinearWeightedMovingAverage::update_target_time(...) expected `target_time` to be greater than 0, \
                 but 0 was given"
                    .into(),
            );
        }
        if target_time.checked_mul(LWMA_MAX_BLOCK_TIME_RATIO).is_none() {
            return Err(format!(
                "LinearWeightedMovingAverage::update_target_time(...) expected `target_time` to be at least \
                 {LWMA_MAX_BLOCK_TIME_RATIO} times smaller than `u64::MAX`",
            ));
        }
        debug_assert!(
            target_time <= MAX_SANE_TARGET_TIME,
            "target_time {target_time} is far beyond any sane block interval; the scaled u128 arithmetic in \
             `raw_difficulty` assumes it stays well below this bound"
        );
        self.target_time = u128::from(target_time);
        // The `checked_mul` guard above proves this cannot overflow.
        self.max_block_time = target_time.saturating_mul(LWMA_MAX_BLOCK_TIME_RATIO);
        Ok(())
    }
}

impl DifficultyAdjustment for LinearWeightedMovingAverage {
    fn add(&mut self, timestamp: EpochTime, target: Difficulty, adjusted_target: Difficulty) -> Result<(), String> {
        self.add_back(timestamp, target, adjusted_target);
        Ok(())
    }

    fn get_difficulty(&self) -> Option<Difficulty> {
        self.calculate()
    }
}

#[cfg(test)]
mod test {
    use tari_transaction_components::tari_proof_of_work::{Difficulty, DifficultyAdjustment};
    use tari_utilities::epoch_time::EpochTime;

    use crate::proof_of_work::lwma_diff::LinearWeightedMovingAverage;
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
        dif.add_front(
            60.into(),
            Difficulty::from_u64(100).unwrap(),
            Difficulty::from_u64(100).unwrap(),
        );
        assert!(!dif.is_full());
        assert_eq!(dif.num_samples(), 1);
        dif.add_front(
            60.into(),
            Difficulty::from_u64(100).unwrap(),
            Difficulty::from_u64(100).unwrap(),
        );
        assert_eq!(dif.num_samples(), 2);
        assert!(dif.is_full());
        dif.add_front(
            60.into(),
            Difficulty::from_u64(100).unwrap(),
            Difficulty::from_u64(100).unwrap(),
        );
        assert_eq!(dif.num_samples(), 2);
        assert!(dif.is_full());
    }

    #[test]
    fn lwma_negative_solve_times() {
        let mut dif = LinearWeightedMovingAverage::new(90, 120).unwrap();
        let mut timestamp = 60.into();
        let cum_diff = Difficulty::from_u64(100).unwrap();
        dif.add(timestamp, cum_diff, cum_diff).unwrap();
        timestamp = timestamp.checked_add(EpochTime::from(60)).unwrap();
        dif.add(timestamp, cum_diff, cum_diff).unwrap();
        // Lets create a history and populate the vecs
        for _i in 0..150 {
            timestamp = timestamp.checked_add(EpochTime::from(60)).unwrap();
            dif.add(timestamp, cum_diff, cum_diff).unwrap();
        }
        // lets create chaos by having 60 blocks as negative solve times. This should never be allowed in practice by
        // having checks on the block times.
        for _i in 0..60 {
            timestamp = (timestamp.as_u64() - 1).into(); // Only choosing -1 here since we are testing negative solve times and we cannot have 0 time
            let diff_before = dif.get_difficulty().unwrap();
            dif.add(timestamp, cum_diff, cum_diff).unwrap();
            let diff_after = dif.get_difficulty().unwrap();
            // Algo should handle this as 1sec solve time thus increase the difficulty constantly
            assert!(diff_after > diff_before);
        }
    }

    #[test]
    fn lwma_limit_difficulty_change() {
        let mut dif = LinearWeightedMovingAverage::new(5, 60).unwrap();
        dif.add(
            60.into(),
            Difficulty::from_u64(100).unwrap(),
            Difficulty::from_u64(100).unwrap(),
        )
        .unwrap();
        dif.add(
            10_000_000.into(),
            Difficulty::from_u64(100).unwrap(),
            Difficulty::from_u64(100).unwrap(),
        )
        .unwrap();
        assert_eq!(dif.get_difficulty().unwrap(), Difficulty::from_u64(16).unwrap());
        dif.add(
            20_000_000.into(),
            Difficulty::from_u64(16).unwrap(),
            Difficulty::from_u64(16).unwrap(),
        )
        .unwrap();
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
        // (timestamp, target difficulty, expected next target difficulty)
        const EXPECTED: [(u64, u64, Option<u64>); 15] = [
            (60, 100, None),
            (120, 100, Some(100)),
            (180, 100, Some(100)),
            (240, 100, Some(100)),
            (300, 100, Some(100)),
            (350, 105, Some(106)),
            (380, 128, Some(134)),
            (445, 123, Some(128)),
            (515, 116, Some(119)),
            (615, 94, Some(93)),
            (975, 39, Some(35)),
            (976, 46, Some(38)),
            (977, 55, Some(46)),
            (978, 75, Some(65)),
            (979, 148, Some(173)),
        ];

        let mut dif = LinearWeightedMovingAverage::new(5, 60).unwrap();
        for (timestamp, target, expected) in EXPECTED {
            let target = Difficulty::from_u64(target).unwrap();
            // Pre-fork: the adjusted target equals the target
            dif.add(timestamp.into(), target, target).unwrap();
            assert_eq!(
                dif.get_difficulty(),
                expected.map(|d| Difficulty::from_u64(d).unwrap()),
                "at timestamp {timestamp}"
            );
        }
    }

    /// Verbatim copy of the pre-TIP-0004 LWMA calculation, used to prove that the scaled implementation is
    /// bit-identical when every backoff modifier is 1 (i.e. for all pre-fork blocks).
    fn legacy_calculate(data: &[(EpochTime, Difficulty)], target_time: u64, max_block_time: u64) -> Option<u64> {
        use std::cmp::min;
        if data.len() <= 1 {
            return None;
        }
        let n = (data.len() - 1) as u128;
        let mut weighted_times: u128 = 0;
        let difficulty_sum = data
            .iter()
            .skip(1)
            .fold(0u128, |difficulty, (_, d)| difficulty + u128::from(d.as_u64()));
        let ave_difficulty = difficulty_sum / n;
        let &(mut previous_timestamp, _) = data.first().unwrap();
        let mut this_timestamp;
        for (i, (timestamp, _)) in data.iter().skip(1).enumerate() {
            if *timestamp > previous_timestamp {
                this_timestamp = *timestamp;
            } else {
                this_timestamp = previous_timestamp.checked_add(EpochTime::from(1))?;
            }
            let solve_time = min(
                this_timestamp
                    .checked_sub(previous_timestamp)
                    .unwrap_or(EpochTime::from(1))
                    .as_u64(),
                max_block_time,
            );
            previous_timestamp = this_timestamp;
            weighted_times += u128::from(solve_time * (i + 1) as u64);
        }
        let k = n * (n + 1) * u128::from(target_time) / 2;
        let target = u64::try_from(ave_difficulty * k / weighted_times).unwrap_or(u64::MAX);
        if target < Difficulty::min().as_u64() {
            None
        } else {
            Some(target)
        }
    }

    /// A tiny deterministic PRNG so that the invariant is checked over a wide spread of inputs without a dev
    /// dependency.
    fn next_rand(state: &mut u64) -> u64 {
        *state ^= *state << 13;
        *state ^= *state >> 7;
        *state ^= *state << 17;
        *state
    }

    #[test]
    fn pre_fork_result_is_bit_identical_to_the_legacy_formula() {
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        for target_time in [60u64, 120, 240, 360, 480] {
            for block_window in [2usize, 5, 45, 90] {
                for _round in 0..20 {
                    let mut dif = LinearWeightedMovingAverage::new(block_window, target_time).unwrap();
                    let mut legacy = Vec::new();
                    let mut timestamp = 1_000_000u64;
                    for _ in 0..=block_window {
                        // Solve times spanning below, around, and far above the max block time, including
                        // zero/negative steps to exercise the monotonicity fix.
                        let step = next_rand(&mut state) % (target_time * 10);
                        timestamp = timestamp.saturating_add(step).saturating_sub(target_time / 2);
                        let difficulty = Difficulty::from_u64(1 + next_rand(&mut state) % 100_000_000_000).unwrap();
                        // The adjusted target equals the target, exactly like every pre-fork block
                        dif.add_back(timestamp.into(), difficulty, difficulty);
                        legacy.push((EpochTime::from(timestamp), difficulty));
                    }
                    let expected = legacy_calculate(
                        &legacy,
                        target_time,
                        LinearWeightedMovingAverage::max_block_time(target_time).unwrap(),
                    );
                    assert_eq!(dif.raw_difficulty(), expected);
                }
            }
        }
    }

    /// Convenience for a window entry that paid the nominal modifier `m` with no clamp binding.
    fn scaled(target: u64, modifier: u64) -> (Difficulty, Difficulty) {
        (
            Difficulty::from_u64(target).unwrap(),
            Difficulty::from_u64(target * modifier).unwrap(),
        )
    }

    #[test]
    fn a_uniform_modifier_scales_the_target_by_that_modifier() {
        // With every block in the window mined against a target inflated by `m`, the normalised solve times shrink
        // by `m`, so the computed target is exactly `m` times the unmodified target.
        for modifier in [1u64, 2, 4, 8, 16, 32] {
            let mut dif = LinearWeightedMovingAverage::new(5, 60).unwrap();
            let mut timestamp = 60u64;
            let (target, adjusted) = scaled(100, modifier);
            for _ in 0..6 {
                dif.add_back(timestamp.into(), target, adjusted);
                timestamp += 60;
            }
            assert_eq!(dif.raw_difficulty().unwrap(), 100 * modifier);
        }
    }

    #[test]
    fn only_the_entry_that_closes_the_gap_is_used() {
        // The front entry's adjusted target is never read
        let mut a = LinearWeightedMovingAverage::new(5, 60).unwrap();
        let mut b = LinearWeightedMovingAverage::new(5, 60).unwrap();
        let mut timestamp = 60u64;
        for i in 0..6 {
            let (target, adjusted) = if i == 0 { scaled(100, 32) } else { scaled(100, 1) };
            a.add_back(timestamp.into(), target, adjusted);
            let (target, adjusted) = scaled(100, 1);
            b.add_back(timestamp.into(), target, adjusted);
            timestamp += 60;
        }
        assert_eq!(a.raw_difficulty(), b.raw_difficulty());
    }

    #[test]
    fn the_effective_modifier_is_used_when_a_clamp_binds() {
        // A block whose adjusted target was capped by `max_pow_difficulty` was not actually mined against the full
        // nominal modifier. Normalising by the nominal 32 rather than the effective 6 over-corrects, which is what
        // drives the runaway this representation exists to prevent.
        let target_time = 240u64;
        let mut dif = LinearWeightedMovingAverage::new(5, target_time).unwrap();
        // target 10_000_000, nominal modifier 32 would be 320_000_000 but max_pow_difficulty caps it at 60_000_000,
        // so the effective modifier is 6.
        let target = Difficulty::from_u64(10_000_000).unwrap();
        let adjusted = Difficulty::from_u64(60_000_000).unwrap();
        // Steady state: blocks arrive every 6 * target_time because they must clear 6x the target
        let mut timestamp = 1_000u64;
        for _ in 0..6 {
            dif.add_back(timestamp.into(), target, adjusted);
            timestamp += 6 * target_time;
        }
        // Normalised solve time is 6 * target_time / 6 == target_time, so the target is unchanged
        assert_eq!(dif.raw_difficulty().unwrap(), 10_000_000);

        // For contrast, de-normalising by the nominal 32 would have produced a target ~5.3x too high
        let mut nominal = LinearWeightedMovingAverage::new(5, target_time).unwrap();
        let mut timestamp = 1_000u64;
        for _ in 0..6 {
            nominal.add_back(timestamp.into(), target, Difficulty::from_u64(10_000_000 * 32).unwrap());
            timestamp += 6 * target_time;
        }
        assert_eq!(nominal.raw_difficulty().unwrap(), 10_000_000 * 32 / 6);
    }

    #[test]
    fn an_adjusted_target_below_the_target_is_treated_as_no_penalty() {
        // Defence in depth: `adjusted >= target` always holds by construction, but a malformed pair must not make
        // the solve time grow.
        let mut dif = LinearWeightedMovingAverage::new(5, 60).unwrap();
        let mut reference = LinearWeightedMovingAverage::new(5, 60).unwrap();
        let mut timestamp = 60u64;
        for _ in 0..6 {
            dif.add_back(
                timestamp.into(),
                Difficulty::from_u64(100).unwrap(),
                Difficulty::from_u64(1).unwrap(),
            );
            reference.add_back(
                timestamp.into(),
                Difficulty::from_u64(100).unwrap(),
                Difficulty::from_u64(100).unwrap(),
            );
            timestamp += 60;
        }
        assert_eq!(dif.raw_difficulty(), reference.raw_difficulty());
    }

    #[test]
    fn clamping_happens_after_normalisation() {
        // A 32x modifier and a solve time far above `max_block_time`: the normalised solve time is
        // raw * (1/32), which is then clamped to max_block_time * 32 in scaled units.
        let target_time = 60u64;
        let max_block_time = LinearWeightedMovingAverage::max_block_time(target_time).unwrap();
        let mut dif = LinearWeightedMovingAverage::new(2, target_time).unwrap();
        let (target, adjusted) = scaled(100, 32);
        dif.add_back(0.into(), target, adjusted);
        dif.add_back((max_block_time * 3200).into(), target, adjusted);
        dif.add_back((max_block_time * 6400).into(), target, adjusted);
        // Both normalised gaps (max_block_time * 100) still exceed max_block_time, so both clamp
        let mut reference = LinearWeightedMovingAverage::new(2, target_time).unwrap();
        let (target, adjusted) = scaled(100, 1);
        reference.add_back(0.into(), target, adjusted);
        reference.add_back(max_block_time.into(), target, adjusted);
        reference.add_back((max_block_time * 2).into(), target, adjusted);
        assert_eq!(dif.raw_difficulty(), reference.raw_difficulty());
    }

    #[test]
    fn an_enormous_solve_time_does_not_overflow() {
        // The raw solve time is capped before the scaled multiplication; check the extreme still clamps cleanly
        let mut dif = LinearWeightedMovingAverage::new(2, 480).unwrap();
        let target = Difficulty::from_u64(u64::MAX / 32).unwrap();
        let adjusted = Difficulty::max();
        dif.add_back(0.into(), target, adjusted);
        dif.add_back((u64::MAX / 2).into(), target, adjusted);
        dif.add_back(u64::MAX.into(), target, adjusted);
        assert!(dif.raw_difficulty().is_some());
    }

    #[test]
    fn update_block_window_shrinks_from_the_front() {
        let mut dif = LinearWeightedMovingAverage::new(90, 60).unwrap();
        let mut timestamp = 60u64;
        for i in 0..91 {
            let d = Difficulty::from_u64(100 + i).unwrap();
            dif.add_back(timestamp.into(), d, d);
            timestamp += 60;
        }
        assert!(dif.is_full());
        assert_eq!(dif.num_samples(), 91);
        dif.update_block_window(45).unwrap();
        assert_eq!(dif.block_window(), 45);
        assert_eq!(dif.num_samples(), 46);
        assert!(dif.is_full());
        // The newest entry survived, the oldest was dropped
        assert_eq!(
            dif.target_difficulties.back().unwrap().1,
            Difficulty::from_u64(190).unwrap()
        );
        assert_eq!(
            dif.target_difficulties.front().unwrap().1,
            Difficulty::from_u64(145).unwrap()
        );
        assert!(dif.update_block_window(0).is_err());
    }

    #[test]
    fn ensure_calculate_does_not_overflow_with_large_block_window() {
        let mut dif = LinearWeightedMovingAverage::new(6000, 60).unwrap();
        for _i in 0..6000 {
            dif.add(60.into(), Difficulty::max(), Difficulty::max()).unwrap();
        }
        // We don't care about the value, we just want to test that get_difficulty does not panic with an overflow.
        dif.get_difficulty().unwrap();
    }
}
