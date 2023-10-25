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

use tari_utilities::epoch_time::EpochTime;

use crate::proof_of_work::{difficulty::DifficultyAdjustment, lwma_diff::LinearWeightedMovingAverage, Difficulty};

/// A window of target difficulties
#[derive(Debug, Clone)]
pub struct TargetDifficultyWindow {
    lwma: LinearWeightedMovingAverage,
}

impl TargetDifficultyWindow {
    /// Initialize a new `TargetDifficultyWindow`
    pub(crate) fn new(block_window: usize, target_time: u64) -> Result<Self, String> {
        Ok(Self {
            lwma: LinearWeightedMovingAverage::new(block_window, target_time)?,
        })
    }

    /// Appends a target difficulty. If the number of stored difficulties exceeds the block window, the stored
    /// difficulty at the front is removed keeping the size of the stored difficulties equal to the block window.
    #[inline]
    pub fn add_back(&mut self, time: EpochTime, difficulty: Difficulty) {
        self.lwma.add_back(time, difficulty);
    }

    /// Prepends a target difficulty. If the number of stored difficulties exceeds the block window, the stored
    /// difficulty at the back is removed keeping the size of the stored difficulties equal to the block window.
    #[inline]
    pub fn add_front(&mut self, time: EpochTime, difficulty: Difficulty) {
        self.lwma.add_front(time, difficulty);
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

    /// Calculates the target difficulty for the current set of target difficulties.
    pub fn calculate(&self, min: Difficulty, max: Difficulty) -> Difficulty {
        let difficulty = self.lwma.get_difficulty().unwrap_or(min);
        cmp::max(min, cmp::min(max, difficulty))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn it_calculates_the_target_difficulty() {
        let mut target_difficulties = TargetDifficultyWindow::new(5, 60).unwrap();
        let mut time = Difficulty::from_u64(60).unwrap().as_u64().into();
        target_difficulties.add_back(time, Difficulty::from_u64(100).unwrap());
        time += Difficulty::from_u64(60).unwrap().as_u64().into();
        target_difficulties.add_back(time, Difficulty::from_u64(100).unwrap());
        time += Difficulty::from_u64(60).unwrap().as_u64().into();
        target_difficulties.add_back(time, Difficulty::from_u64(100).unwrap());
        time += Difficulty::from_u64(60).unwrap().as_u64().into();
        target_difficulties.add_back(time, Difficulty::from_u64(100).unwrap());

        assert_eq!(
            target_difficulties.calculate(Difficulty::from_u64(1).unwrap(), Difficulty::from_u64(400).unwrap()),
            Difficulty::from_u64(100).unwrap()
        );
    }
}
