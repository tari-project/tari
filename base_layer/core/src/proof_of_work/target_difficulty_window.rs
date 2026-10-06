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

use std::cmp;

use tari_transaction_components::tari_proof_of_work::{Difficulty, DifficultyAdjustment};
use tari_utilities::epoch_time::EpochTime;

use crate::proof_of_work::{lwma_diff::LinearWeightedMovingAverage, target_difficulty::AdjustedTarget};
/// A window of target difficulties
#[derive(Debug, Clone)]
pub struct TargetDifficultyWindow {
    lwma: LinearWeightedMovingAverage,
    /// The PoW backoff modifier that the next block of this window's algorithm would pay. `1` means no penalty,
    /// which is also the pre-fork value.
    next_modifier: u64,
}

impl TargetDifficultyWindow {
    /// Initialize a new `TargetDifficultyWindow`
    pub(crate) fn new(block_window: usize, target_time: u64) -> Result<Self, String> {
        Ok(Self {
            lwma: LinearWeightedMovingAverage::new(block_window, target_time)?,
            next_modifier: 1,
        })
    }

    /// Appends a target difficulty together with the adjusted target the block's proof of work actually had to
    /// clear. If the number of stored difficulties exceeds the block window, the stored difficulty at the front is
    /// removed keeping the size of the stored difficulties equal to the block window.
    #[inline]
    pub fn add_back(&mut self, time: EpochTime, target: Difficulty, adjusted_target: Difficulty) {
        self.lwma.add_back(time, target, adjusted_target);
    }

    /// Prepends a target difficulty together with the adjusted target the block's proof of work actually had to
    /// clear. If the number of stored difficulties exceeds the block window, the stored difficulty at the back is
    /// removed keeping the size of the stored difficulties equal to the block window.
    #[inline]
    pub fn add_front(&mut self, time: EpochTime, target: Difficulty, adjusted_target: Difficulty) {
        self.lwma.add_front(time, target, adjusted_target);
    }

    /// Sets the PoW backoff modifier that the next block of this window's algorithm would pay.
    #[inline]
    pub fn set_next_modifier(&mut self, modifier: u64) {
        self.next_modifier = modifier;
    }

    /// The PoW backoff modifier that the next block of this window's algorithm would pay.
    #[inline]
    pub fn next_modifier(&self) -> u64 {
        self.next_modifier
    }

    /// Resizes the block window, dropping the oldest data points if the window shrank.
    pub fn update_block_window(&mut self, block_window: usize) -> Result<(), String> {
        self.lwma.update_block_window(block_window)
    }

    /// Returns true of the TargetDifficulty has `block_window` data points, otherwise false
    #[inline]
    pub fn is_full(&self) -> bool {
        self.lwma.is_full()
    }

    /// Returns the number of target difficulties in the window
    pub fn len(&self) -> usize {
        self.lwma.num_samples()
    }

    /// Returns true if the window is empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.lwma.num_samples() == 0
    }

    /// Calculates the (unadjusted) target difficulty for the current set of target difficulties. This is the value
    /// that accumulates into the total accumulated difficulty and that feeds the LWMA window.
    pub fn calculate(&self, min: Difficulty, max: Difficulty) -> Difficulty {
        clamp(self.lwma.get_difficulty().unwrap_or(min), min, max)
    }

    /// Calculates the target difficulty that the next block's proof of work must actually clear, i.e. the target
    /// multiplied by the same-algorithm backoff modifier and then clamped.
    pub fn calculate_adjusted(&self, min: Difficulty, max: Difficulty) -> Difficulty {
        self.calculate_pair(min, max).adjusted
    }

    /// Calculates both the unadjusted and the adjusted target from a single LWMA computation.
    ///
    /// The adjusted target is `clamp(base * m, min, max)` where `base` is the clamped, unadjusted target. Applying
    /// the modifier to the already clamped `base` (rather than to the raw LWMA output) is what makes the adjusted
    /// target recomputable from `base` alone during a database replay, which in turn lets the LWMA record the
    /// *effective* modifier `adjusted / base` for every historical block. The two orders can only differ when the
    /// `min` clamp binds, i.e. when the chain is already sitting on the difficulty floor. Where `min == max` (e.g.
    /// LocalNet) the backoff remains a complete no-op.
    pub fn calculate_pair(&self, min: Difficulty, max: Difficulty) -> AdjustedTarget {
        let base = clamp(self.lwma.get_difficulty().unwrap_or(min), min, max);
        AdjustedTarget {
            base,
            adjusted: adjust(base, self.next_modifier, min, max),
        }
    }

    pub fn update_target_time(&mut self, target_time: u64) -> Result<(), String> {
        self.lwma.update_target_time(target_time)
    }
}

fn clamp(difficulty: Difficulty, min: Difficulty, max: Difficulty) -> Difficulty {
    cmp::max(min, cmp::min(max, difficulty))
}

/// Applies a PoW backoff modifier to an already clamped target difficulty and clamps the result. See
/// [`TargetDifficultyWindow::calculate_pair`] for why the modifier is applied to the clamped target.
pub fn adjust(base: Difficulty, modifier: u64, min: Difficulty, max: Difficulty) -> Difficulty {
    let adjusted = Difficulty::from_u64(base.as_u64().saturating_mul(modifier)).unwrap_or_else(|_| Difficulty::max());
    clamp(adjusted, min, max)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn it_calculates_the_target_difficulty() {
        let mut target_difficulties = TargetDifficultyWindow::new(5, 60).unwrap();
        let mut time = Difficulty::from_u64(60).unwrap().as_u64().into();
        target_difficulties.add_back(
            time,
            Difficulty::from_u64(100).unwrap(),
            Difficulty::from_u64(100).unwrap(),
        );
        time = time
            .checked_add(EpochTime::from(Difficulty::from_u64(60).unwrap().as_u64()))
            .unwrap();
        target_difficulties.add_back(
            time,
            Difficulty::from_u64(100).unwrap(),
            Difficulty::from_u64(100).unwrap(),
        );
        time = time
            .checked_add(EpochTime::from(Difficulty::from_u64(60).unwrap().as_u64()))
            .unwrap();
        target_difficulties.add_back(
            time,
            Difficulty::from_u64(100).unwrap(),
            Difficulty::from_u64(100).unwrap(),
        );
        time = time
            .checked_add(EpochTime::from(Difficulty::from_u64(60).unwrap().as_u64()))
            .unwrap();
        target_difficulties.add_back(
            time,
            Difficulty::from_u64(100).unwrap(),
            Difficulty::from_u64(100).unwrap(),
        );

        assert_eq!(
            target_difficulties.calculate(Difficulty::from_u64(1).unwrap(), Difficulty::from_u64(400).unwrap()),
            Difficulty::from_u64(100).unwrap()
        );
    }

    #[test]
    fn it_applies_the_backoff_modifier_before_clamping() {
        let mut target_difficulties = TargetDifficultyWindow::new(5, 60).unwrap();
        let mut time = EpochTime::from(60);
        for _ in 0..5 {
            target_difficulties.add_back(
                time,
                Difficulty::from_u64(100).unwrap(),
                Difficulty::from_u64(100).unwrap(),
            );
            time = time.checked_add(EpochTime::from(60)).unwrap();
        }
        let min = Difficulty::from_u64(1).unwrap();
        let max = Difficulty::from_u64(400).unwrap();

        // No modifier: adjusted == unadjusted
        assert_eq!(target_difficulties.next_modifier(), 1);
        assert_eq!(
            target_difficulties.calculate(min, max),
            Difficulty::from_u64(100).unwrap()
        );
        assert_eq!(
            target_difficulties.calculate_adjusted(min, max),
            Difficulty::from_u64(100).unwrap()
        );

        target_difficulties.set_next_modifier(2);
        // The unadjusted target is untouched, only the adjusted one moves
        assert_eq!(
            target_difficulties.calculate(min, max),
            Difficulty::from_u64(100).unwrap()
        );
        assert_eq!(
            target_difficulties.calculate_adjusted(min, max),
            Difficulty::from_u64(200).unwrap()
        );
        let pair = target_difficulties.calculate_pair(min, max);
        assert_eq!(pair.base, Difficulty::from_u64(100).unwrap());
        assert_eq!(pair.adjusted, Difficulty::from_u64(200).unwrap());

        // The modifier is applied before the clamp, so it saturates at max
        target_difficulties.set_next_modifier(32);
        assert_eq!(target_difficulties.calculate_adjusted(min, max), max);

        // ... and where min == max (e.g. LocalNet) the backoff is a complete no-op
        let fixed = Difficulty::from_u64(1).unwrap();
        assert_eq!(target_difficulties.calculate_adjusted(fixed, fixed), fixed);
        assert_eq!(target_difficulties.calculate(fixed, fixed), fixed);
    }
}
