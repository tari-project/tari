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

use std::{fmt, ops::Div};

use newtype_ops::newtype_ops;
use num_format::{Locale, ToFormattedString};
use serde::{Deserialize, Serialize};
use tari_utilities::epoch_time::EpochTime;

use crate::proof_of_work::error::DifficultyAdjustmentError;

/// Minimum difficulty, enforced in diff retargeting
/// avoids getting stuck when trying to increase difficulty subject to dampening
pub const MIN_DIFFICULTY: u64 = 1;

/// The difficulty is defined as the maximum target divided by the block hash.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Deserialize, Serialize)]
pub struct Difficulty(u64);

impl Difficulty {
    /// A const constructor for Difficulty
    pub const fn from_u64(d: u64) -> Self {
        Self(d)
    }

    /// Difficulty of MIN_DIFFICULTY
    pub const fn min() -> Difficulty {
        Difficulty(MIN_DIFFICULTY)
    }

    /// Return the difficulty as a u64
    pub fn as_u64(self) -> u64 {
        self.0
    }

    /// Subtract difficulty without overflowing
    pub fn checked_sub(self, other: Difficulty) -> Option<Difficulty> {
        self.0.checked_sub(other.0).map(Difficulty)
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
        let formatted = self.0.to_formatted_string(&Locale::en);
        write!(f, "{}", formatted)
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
    fn get_difficulty(&self) -> Option<Difficulty>;
}

#[cfg(feature = "base_node")]
pub mod util {
    use super::*;
    use crate::U256;

    /// This will provide the difficulty of the hash assuming the hash is big_endian
    pub(crate) fn big_endian_difficulty(hash: &[u8]) -> Difficulty {
        let scalar = U256::from_big_endian(hash); // Big endian so the hash has leading zeroes
        let result = U256::MAX / scalar;
        let result = result.min(u64::MAX.into());
        result.low_u64().into()
    }

    /// This will provide the difficulty of the hash assuming the hash is little_endian
    pub(crate) fn little_endian_difficulty(hash: &[u8]) -> Difficulty {
        let scalar = U256::from_little_endian(hash); // Little endian so the hash has trailing zeroes
        let result = U256::MAX / scalar;
        let result = result.min(u64::MAX.into());
        result.low_u64().into()
    }

    #[cfg(test)]
    mod test {
        use super::*;

        #[test]
        fn be_high_target() {
            let target: &[u8] = &[
                0xff, 0xff, 0xff, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0,
                0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0,
            ];
            let expected = Difficulty::from(1);
            assert_eq!(big_endian_difficulty(target), expected);
        }

        #[test]
        fn be_max_difficulty() {
            let target = U256::MAX / U256::from(u64::MAX);
            let mut bytes = [0u8; 32];
            target.to_big_endian(&mut bytes);
            assert_eq!(big_endian_difficulty(&bytes), Difficulty::from(u64::MAX));
        }

        #[test]
        fn be_stop_overflow() {
            let target: u64 = 64;
            let expected = u64::MAX;
            assert_eq!(big_endian_difficulty(&target.to_be_bytes()), Difficulty::from(expected));
        }

        #[test]
        fn le_high_target() {
            let target: &[u8] = &[
                0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0,
                0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0xff, 0xff, 0xff,
            ];
            let expected = Difficulty::from(1);
            assert_eq!(little_endian_difficulty(target), expected);
        }

        #[test]
        fn le_max_difficulty() {
            let target = U256::MAX / U256::from(u64::MAX);
            let mut bytes = [0u8; 32];
            target.to_little_endian(&mut bytes);
            assert_eq!(little_endian_difficulty(&bytes), Difficulty::from(u64::MAX));
        }

        #[test]
        fn le_stop_overflow() {
            let target: u64 = 64;
            let expected = u64::MAX;
            assert_eq!(
                little_endian_difficulty(&target.to_be_bytes()),
                Difficulty::from(expected)
            );
        }
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
        assert_eq!(Difficulty::default() + Difficulty::from(42), Difficulty::from(43));
        assert_eq!(Difficulty::from(15) + Difficulty::from(5), Difficulty::from(20));
    }

    #[test]
    fn test_format() {
        let d = Difficulty::from(1_000_000);
        assert_eq!("1,000,000", format!("{}", d));
    }
}
