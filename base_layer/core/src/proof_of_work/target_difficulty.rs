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
    consensus::ConsensusConstants,
    proof_of_work::{
        difficulty::DifficultyAdjustment,
        lwma_diff::LinearWeightedMovingAverage,
        Difficulty,
        PowAlgorithm,
    },
};
use std::cmp;
use tari_crypto::tari_utilities::epoch_time::EpochTime;

#[derive(Debug, Clone)]
pub struct TargetDifficultyWindow {
    lwma: LinearWeightedMovingAverage,
    min_difficulty: Difficulty,
    max_difficulty: Difficulty,
}

impl TargetDifficultyWindow {
    /// Initialize a new `TargetDifficultyWindow`
    ///
    /// # Panics
    ///
    /// Panics if block_window is 0
    pub(crate) fn new(
        block_window: usize,
        target_time: u64,
        min_difficulty: Difficulty,
        max_difficulty: Difficulty,
        max_block_time: u64,
    ) -> Self
    {
        assert!(
            block_window > 0,
            "TargetDifficulty::new expected block_window to be greater than 0, but 0 was given"
        );
        Self {
            lwma: LinearWeightedMovingAverage::new(block_window, target_time, min_difficulty, max_block_time),
            min_difficulty,
            max_difficulty,
        }
    }

    /// Appends a target difficulty. If the number of stored difficulties exceeds the block window, the oldest block
    /// window is removed keeping the size of the stored difficulties equal to the block window.
    #[inline]
    pub fn add_back(&mut self, time: EpochTime, difficulty: Difficulty) {
        self.lwma.add_back(time, difficulty);
    }

    #[inline]
    pub fn add_front(&mut self, time: EpochTime, difficulty: Difficulty) {
        self.lwma.add_front(time, difficulty);
    }

    /// Returns true of the TargetDifficulty has `block_window` data points, otherwise false
    #[inline]
    pub fn is_full(&self) -> bool {
        self.lwma.num_samples() == self.lwma.block_window() + 1
    }

    pub fn len(&self) -> usize {
        self.lwma.num_samples()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.lwma.num_samples() == 0
    }

    /// Calculates the target difficulty for the current set of target difficulties.
    pub fn calculate(&self) -> Difficulty {
        let difficulty = self.lwma.get_difficulty();
        cmp::min(self.max_difficulty, cmp::max(difficulty, self.min_difficulty))
    }

    pub fn update_consensus_constants(&mut self, constants: &ConsensusConstants, pow_algo: PowAlgorithm) {
        self.lwma.update_consensus_constants(constants, pow_algo);
        self.min_difficulty = constants.min_pow_difficulty(pow_algo);
        self.max_difficulty = constants.max_pow_difficulty(pow_algo);
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn it_calculates_the_target_difficulty() {
        let mut target_difficulties = TargetDifficultyWindow::new(5, 60, 1.into(), 200.into(), 60 * 6);
        let mut time = 60.into();
        target_difficulties.add_back(time, 100.into());
        time += 60.into();
        target_difficulties.add_back(time, 100.into());
        time += 60.into();
        target_difficulties.add_back(time, 100.into());
        time += 60.into();
        target_difficulties.add_back(time, 100.into());

        assert_eq!(target_difficulties.calculate(), 100.into());
    }
}
