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

use crate::proof_of_work::error::DifficultyAdjustmentError;
use newtype_ops::newtype_ops;
use serde::{Deserialize, Serialize};
use std::{fmt, ops::Div};
use tari_crypto::tari_utilities::epoch_time::EpochTime;

/// Minimum difficulty, enforced in diff retargetting
/// avoids getting stuck when trying to increase difficulty subject to dampening
pub const MIN_DIFFICULTY: u64 = 1;

/// The difficulty is defined as the maximum target divided by the block hash.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Deserialize, Serialize)]
pub struct Difficulty(u64);

impl Difficulty {
    /// Difficulty of MIN_DIFFICULTY
    pub const fn min() -> Difficulty {
        Difficulty(MIN_DIFFICULTY)
    }

    /// Return the difficulty as a u64
    pub fn as_u64(self) -> u64 {
        self.0
    }

    pub fn checked_sub(self, other: Difficulty) -> Option<Difficulty> {
        match self.0.checked_sub(other.0) {
            None => None,
            Some(v) => Some(Difficulty(v)),
        }
    }
}

impl Default for Difficulty {
    fn default() -> Self {
        Difficulty::min()
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

impl From<Difficulty> for u64 {
    fn from(value: Difficulty) -> Self {
        value.0
    }
}

/// General difficulty adjustment algorithm trait. The key method is `get_difficulty`, which returns the target
/// difficulty given a set of historical achieved difficulties; supplied through the `add` method.
pub trait DifficultyAdjustment {
    /// Adds the latest block timestamp (in seconds) and total accumulated difficulty. If the new data point violates
    /// some difficulty criteria, then `add` returns an error with the type of failure indicated
    fn add(
        &mut self,
        timestamp: EpochTime,
        accumulated_difficulty: Difficulty,
    ) -> Result<(), DifficultyAdjustmentError>;

    /// Return the calculated target difficulty for the next block.
    fn get_difficulty(&self) -> Difficulty;
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
        assert_eq!(Difficulty::default() + Difficulty::from(42), Difficulty::from(43));
        assert_eq!(&Difficulty::from(15) + &Difficulty::from(5), Difficulty::from(20));
    }
}
