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

use std::fmt;

use borsh::{BorshDeserialize, BorshSerialize};
use num_format::{Locale, ToFormattedString};
use primitive_types::U256;
use serde::{Deserialize, Serialize};
use tari_utilities::epoch_time::EpochTime;

use crate::proof_of_work::{error::DifficultyError, DifficultyAdjustmentError};

/// Minimum difficulty, enforced in diff retargeting
/// avoids getting stuck when trying to increase difficulty subject to dampening
pub const MIN_DIFFICULTY: u64 = 1;

/// The difficulty is defined as the maximum target divided by the block hash.
#[derive(
    Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Deserialize, Serialize, BorshSerialize, BorshDeserialize,
)]
pub struct Difficulty(u64);

impl Difficulty {
    /// A const constructor for Difficulty
    pub const fn from_u64(d: u64) -> Result<Self, DifficultyError> {
        if d < MIN_DIFFICULTY {
            return Err(DifficultyError::InvalidDifficulty);
        }
        Ok(Self(d))
    }

    /// Return the difficulty as a u64
    pub fn as_u64(self) -> u64 {
        self.0
    }

    /// Difficulty of MIN_DIFFICULTY
    pub const fn min() -> Difficulty {
        Difficulty(MIN_DIFFICULTY)
    }

    /// Maximum Difficulty
    pub const fn max() -> Difficulty {
        Difficulty(u64::MAX)
    }

    /// Helper function to provide the difficulty of the hash assuming the hash is big_endian
    pub fn big_endian_difficulty(hash: &[u8]) -> Result<Difficulty, DifficultyError> {
        let scalar = U256::from_big_endian(hash); // Big endian so the hash has leading zeroes
        Difficulty::u256_scalar_to_difficulty(scalar)
    }

    /// Helper function to provide the difficulty of the hash assuming the hash is little_endian
    pub fn little_endian_difficulty(hash: &[u8]) -> Result<Difficulty, DifficultyError> {
        let scalar = U256::from_little_endian(hash); // Little endian so the hash has trailing zeroes
        Difficulty::u256_scalar_to_difficulty(scalar)
    }

    fn u256_scalar_to_difficulty(scalar: U256) -> Result<Difficulty, DifficultyError> {
        if scalar == U256::zero() {
            return Err(DifficultyError::DivideByZero);
        }
        let result = U256::MAX / scalar;
        let result = result.min(u64::MAX.into());
        Difficulty::from_u64(result.low_u64())
    }

    pub fn checked_div_u64(&self, other: u64) -> Option<Difficulty> {
        match self.0.checked_div(other) {
            None => None,
            Some(n) => {
                if n < MIN_DIFFICULTY {
                    None
                } else {
                    Some(Difficulty(n))
                }
            },
        }
    }
}

/// These traits should not be implemented for `Difficulty`:
/// - `Add<Self> for Difficulty` "`+` must not be used, use `checked_add(value)` instead; to prevent overflow
/// - `Sub<Self> for Difficulty` `-` must not be used, use `checked_sub(value)` instead; to prevent underflow
/// - `Mul for Difficulty` `*` must not be used at all; difficulties should only be added to or subtracted from
/// - `Div for Difficulty` `/` must not be used at all; difficulties should only be added to or subtracted from
/// - `From<u64> for Difficulty` `Difficulty::from<u64>` must not be used, use `from_u64(value)` instead; to prevent
///   assignment `< MIN_DIFFICULTY`
impl Default for Difficulty {
    fn default() -> Self {
        Difficulty::min()
    }
}

impl From<Difficulty> for u64 {
    fn from(value: Difficulty) -> Self {
        value.0
    }
}

impl fmt::Display for Difficulty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let formatted = self.0.to_formatted_string(&Locale::en);
        write!(f, "{}", formatted)
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

#[cfg(test)]
mod test {
    use primitive_types::U256;

    use crate::proof_of_work::{difficulty::MIN_DIFFICULTY, Difficulty};

    #[test]
    fn test_format() {
        let d = Difficulty::from_u64(1_000_000).unwrap();
        assert_eq!("1,000,000", format!("{}", d));
    }

    #[test]
    fn difficulty_converts_correctly_at_its_limits() {
        for d in 0..=MIN_DIFFICULTY + 1 {
            if d < MIN_DIFFICULTY {
                assert!(Difficulty::from_u64(d).is_err());
            } else {
                assert!(Difficulty::from_u64(d).is_ok());
            }
        }
        assert_eq!(Difficulty::min().as_u64(), MIN_DIFFICULTY);
        assert_eq!(Difficulty::max().as_u64(), u64::MAX);
    }

    #[test]
    fn be_high_target() {
        let target: &[u8] = &[
            0xff, 0xff, 0xff, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0,
            0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0,
        ];
        let expected = Difficulty::min();
        assert_eq!(Difficulty::big_endian_difficulty(target).unwrap(), expected);
    }

    #[test]
    fn be_max_difficulty() {
        let target = U256::MAX / U256::from(u64::MAX);
        let bytes = target.to_big_endian();
        assert_eq!(Difficulty::big_endian_difficulty(&bytes).unwrap(), Difficulty::max());
    }

    #[test]
    fn be_stop_overflow() {
        let target: u64 = 64;
        let expected = u64::MAX;
        assert_eq!(
            Difficulty::big_endian_difficulty(&target.to_be_bytes()).unwrap(),
            Difficulty::from_u64(expected).unwrap()
        );
    }

    #[test]
    fn le_high_target() {
        let target: &[u8] = &[
            0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0,
            0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0xff, 0xff, 0xff,
        ];
        let expected = Difficulty::min();
        assert_eq!(Difficulty::little_endian_difficulty(target).unwrap(), expected);
    }

    #[test]
    fn le_max_difficulty() {
        let target = U256::MAX / U256::from(u64::MAX);
        let bytes = target.to_little_endian();
        assert_eq!(Difficulty::little_endian_difficulty(&bytes).unwrap(), Difficulty::max());
    }

    #[test]
    fn le_stop_overflow() {
        let target: u64 = 64;
        let expected = u64::MAX;
        assert_eq!(
            Difficulty::little_endian_difficulty(&target.to_be_bytes()).unwrap(),
            Difficulty::from_u64(expected).unwrap()
        );
    }

    #[test]
    fn u256_scalar_to_difficulty_division_by_zero() {
        let bytes = [];
        assert!(Difficulty::little_endian_difficulty(&bytes).is_err());
        assert!(Difficulty::big_endian_difficulty(&bytes).is_err());
        let bytes = [0u8; 32];
        assert!(Difficulty::little_endian_difficulty(&bytes).is_err());
        assert!(Difficulty::big_endian_difficulty(&bytes).is_err());
    }
}
