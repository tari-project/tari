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

use bitflags::_core::ops::Div;
use newtype_ops::newtype_ops;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Minimum difficulty, enforced in diff retargetting
/// avoids getting stuck when trying to increase difficulty subject to dampening
pub const MIN_DIFFICULTY: u64 = 1;
/// This is the time in seconds that should be the ideal time between making new blocks
pub const BLOCK_INTERVAL: u64 = 60;
/// This is the amount of blocks between difficulty adjustments.
pub const BLOCKS_PER_ADJUSTMENT: u64 = 2016;

/// The difficulty is defined as the maximum target divided by the block hash.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Deserialize, Serialize)]
pub struct Difficulty(u64);

impl Difficulty {
    /// Difficulty of MIN_DIFFICULTY
    pub fn min() -> Difficulty {
        Difficulty(MIN_DIFFICULTY)
    }

    /// This function will calculate the required difficulty given the time taken to calculate 2016 blocks
    /// The interval is the difference in seconds between the two headers
    pub fn calculate_req_difficulty(interval: i64, difficulty: Difficulty) -> Difficulty {
        let target_time = (BLOCK_INTERVAL * BLOCKS_PER_ADJUSTMENT) as f32; // 60 seconds per block, 2016 blocks
        let deviation = (interval as f32) - target_time;
        let mut difficulty_multiplier = 1.0 - (deviation / target_time);
        // cap the max adjustment to 50%
        if difficulty_multiplier >= 1.5 {
            difficulty_multiplier = 1.5;
        };
        if difficulty_multiplier <= 0.5 {
            difficulty_multiplier = 0.5;
        };
        // return a new difficulty that is proportionally larger or smaller depending on the time diff.
        Difficulty((difficulty.0 as f32 * (difficulty_multiplier)) as u64)
    }

    /// Return the difficulty as a u64
    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

impl Default for Difficulty {
    fn default() -> Self {
        Difficulty(0)
    }
}

// You can only add or subtract Difficulty from Difficulty
newtype_ops! { [Difficulty] {add sub} {:=} Self Self }
newtype_ops! { [Difficulty] {add sub} {:=} &Self &Self }
newtype_ops! { [Difficulty] {add sub} {:=} Self &Self }

// Multiplication and division of difficulty by scalar is Difficulty
newtype_ops! { [Difficulty] {mul div rem} {:=} Self u64 }

// Division of difficulty by difficulty is a difficulty ratio (scalar) (newtype_ops doesn't handle this case)
impl Div for Difficulty {
    type Output = u64;

    fn div(self, rhs: Self) -> Self::Output {
        self.0 / rhs.0
    }
}

impl fmt::Display for Difficulty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<u64> for Difficulty {
    fn from(value: u64) -> Self {
        Difficulty(value)
    }
}

#[cfg(test)]
mod test {
    use crate::proof_of_work::difficulty::Difficulty;

    #[test]
    fn add_difficulty() {
        assert_eq!(
            Difficulty::from(1_000) + Difficulty::from(8_000),
            Difficulty::from(9_000)
        );
        assert_eq!(Difficulty::default() + Difficulty::from(42), Difficulty::from(42));
        assert_eq!(&Difficulty::from(15) + &Difficulty::from(5), Difficulty::from(20));
    }

    #[test]
    fn calc_difficulty() {
        assert_eq!(
            Difficulty::min(),
            Difficulty::calculate_req_difficulty(0, Difficulty::min()),
        );
        let diff = Difficulty::from(9_000);
        let new_diff = Difficulty::calculate_req_difficulty(60 * 2016, diff);
        assert_eq!(diff, new_diff);
        let new_diff = Difficulty::calculate_req_difficulty(60 * 2016 + 3000, diff);
        assert_eq!(diff > new_diff, true);
        let new_diff = Difficulty::calculate_req_difficulty(60 * 2016 - 3000, diff);
        assert_eq!(diff < new_diff, true);
    }

    #[test]
    fn calc_difficulty_max_min() {
        assert_eq!(
            Difficulty::min(),
            Difficulty::calculate_req_difficulty(0, Difficulty::min()),
        );
        let diff = Difficulty::from(1000);
        let new_diff = Difficulty::calculate_req_difficulty(60 * 2016 * 1000, diff);
        assert_eq!(new_diff, Difficulty::from(500));
        let new_diff = Difficulty::calculate_req_difficulty(60, diff);
        assert_eq!(new_diff, Difficulty::from(1500));
    }
}
