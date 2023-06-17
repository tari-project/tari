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

use num_format::{Locale, ToFormattedString};
use serde::{Deserialize, Serialize};
use tari_utilities::epoch_time::EpochTime;

use crate::proof_of_work::{error::DifficultyError, DifficultyAdjustmentError};

/// Minimum difficulty, enforced in diff retargeting
/// avoids getting stuck when trying to increase difficulty subject to dampening
pub const MIN_DIFFICULTY: u64 = 1;

/// The difficulty is defined as the maximum target divided by the block hash.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Deserialize, Serialize)]
pub struct Difficulty(u64);

impl Difficulty {
    /// A const constructor for Difficulty
    pub const fn from_u64(d: u64) -> Result<Self, DifficultyError> {
        if d < MIN_DIFFICULTY {
            return Err(DifficultyError::InvalidDifficulty);
        }
        Ok(Self(d))
    }

    /// Difficulty of MIN_DIFFICULTY
    pub const fn min() -> Difficulty {
        Difficulty(MIN_DIFFICULTY)
    }

    /// Maximum Difficulty
    pub const fn max() -> Difficulty {
        Difficulty(u64::MAX)
    }

    /// Return the difficulty as a u64
    pub fn as_u64(self) -> u64 {
        self.0
    }
}

impl Default for Difficulty {
    fn default() -> Self {
        Difficulty::min()
    }
}

pub trait CheckedAdd<T> {
    fn checked_add(&self, other: T) -> Option<Self>
    where Self: Sized;
}

impl CheckedAdd<Difficulty> for Difficulty {
    fn checked_add(&self, other: Difficulty) -> Option<Self> {
        self.0.checked_add(other.0).map(Difficulty)
    }
}

impl CheckedAdd<u64> for Difficulty {
    fn checked_add(&self, other: u64) -> Option<Self> {
        self.checked_add(Difficulty(other))
    }
}

pub trait CheckedSub<T> {
    fn checked_sub(&self, other: T) -> Option<Self>
    where Self: Sized;
}

impl CheckedSub<Difficulty> for Difficulty {
    fn checked_sub(&self, other: Difficulty) -> Option<Self> {
        if let Some(val) = self.0.checked_sub(other.0) {
            if val < MIN_DIFFICULTY {
                return None;
            }
            Some(Difficulty(val))
        } else {
            None
        }
    }
}

impl CheckedSub<u64> for Difficulty {
    fn checked_sub(&self, other: u64) -> Option<Self> {
        self.checked_sub(Difficulty(other))
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

#[cfg(any(test))]
use std::ops::{Add, Div, Mul, Sub};

// This trait should not be implemented for runtime, here only for testing
#[cfg(any(test))]
impl Add<Self> for Difficulty {
    type Output = Self;

    fn add(self, _rhs: Self) -> Self {
        unimplemented!("Sub::sub<rhs> must not be used, use `.checked_add(value)` instead")
    }
}

// This trait should not be implemented for runtime, here only for testing
#[cfg(any(test))]
impl Sub<Self> for Difficulty {
    type Output = Self;

    fn sub(self, _rhs: Self) -> Self {
        unimplemented!("Sub::sub<rhs> must not be used, use `.checked_sub(value)` instead")
    }
}

// This trait should not be implemented for runtime, here only for testing
#[cfg(any(test))]
impl Mul for Difficulty {
    type Output = u64;

    fn mul(self, _rhs: Self) -> Self::Output {
        unimplemented!("Mul::mul<rhs> must not be used; difficulties should only be added to or subtracted from")
    }
}

// This trait should not be implemented for runtime, here only for testing
#[cfg(any(test))]
impl Div for Difficulty {
    type Output = u64;

    fn div(self, _rhs: Self) -> Self::Output {
        unimplemented!("Div::div<rhs> must not be used; difficulties should only be added to or subtracted from")
    }
}

// This trait should not be implemented for runtime, here only for testing
#[cfg(any(test))]
impl From<u64> for Difficulty {
    fn from(_value: u64) -> Self {
        unimplemented!("Difficulty::from<u64> must not be used, use `Difficulty::from_u64(value)` instead")
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
    pub(crate) fn big_endian_difficulty(hash: &[u8]) -> Result<Difficulty, DifficultyError> {
        let scalar = U256::from_big_endian(hash); // Big endian so the hash has leading zeroes
        let result = U256::MAX / scalar;
        let result = result.min(u64::MAX.into());
        Difficulty::from_u64(result.low_u64())
    }

    /// This will provide the difficulty of the hash assuming the hash is little_endian
    pub(crate) fn little_endian_difficulty(hash: &[u8]) -> Result<Difficulty, DifficultyError> {
        let scalar = U256::from_little_endian(hash); // Little endian so the hash has trailing zeroes
        let result = U256::MAX / scalar;
        let result = result.min(u64::MAX.into());
        Difficulty::from_u64(result.low_u64())
    }

    #[cfg(test)]
    mod test {
        use std::panic::catch_unwind;

        use crate::{
            proof_of_work::{
                difficulty::{
                    util::{big_endian_difficulty, little_endian_difficulty},
                    CheckedAdd,
                    CheckedSub,
                    MIN_DIFFICULTY,
                },
                Difficulty,
            },
            U256,
        };

        #[test]
        fn difficulty_converts_correctly_at_its_minimum() {
            for d in 0..=MIN_DIFFICULTY + 1 {
                if d < MIN_DIFFICULTY {
                    assert!(Difficulty::from_u64(d).is_err());
                } else {
                    assert!(Difficulty::from_u64(d).is_ok());
                }
            }
            assert_eq!(Difficulty::min().as_u64(), MIN_DIFFICULTY);
        }

        #[test]
        fn addition_does_not_overflow() {
            let d1 = Difficulty::from_u64(100).unwrap();
            assert!(d1.checked_add(1).is_some());
            let d2 = Difficulty::max();
            assert!(d2.checked_add(1).is_none());
        }

        #[test]
        fn it_blocks_unsupported_traits() {
            let add_result = catch_unwind(|| {
                let _ = Difficulty::min() + Difficulty::min();
            });
            assert!(add_result.is_err());
            let sub_result = catch_unwind(|| {
                let _ = Difficulty::max() - Difficulty::min();
            });
            assert!(sub_result.is_err());
            let mul_result = catch_unwind(|| {
                let _ = Difficulty::max() * Difficulty::min();
            });
            assert!(mul_result.is_err());
            let div_result = catch_unwind(|| {
                let _ = Difficulty::max() / Difficulty::min();
            });
            assert!(div_result.is_err());
            let from_result = catch_unwind(|| {
                let _ = Difficulty::from(10u64);
            });
            assert!(from_result.is_err());
        }

        #[test]
        fn subtraction_does_not_underflow() {
            let d1 = Difficulty::from_u64(100).unwrap();
            assert!(d1.checked_sub(1).is_some());
            let d2 = Difficulty::max();
            assert!(d1.checked_sub(d2).is_none());
        }

        #[test]
        fn be_high_target() {
            let target: &[u8] = &[
                0xff, 0xff, 0xff, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0,
                0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0,
            ];
            let expected = Difficulty::min();
            assert_eq!(big_endian_difficulty(target).unwrap(), expected);
        }

        #[test]
        fn be_max_difficulty() {
            let target = U256::MAX / U256::from(u64::MAX);
            let mut bytes = [0u8; 32];
            target.to_big_endian(&mut bytes);
            assert_eq!(big_endian_difficulty(&bytes).unwrap(), Difficulty::max());
        }

        #[test]
        fn be_stop_overflow() {
            let target: u64 = 64;
            let expected = u64::MAX;
            assert_eq!(
                big_endian_difficulty(&target.to_be_bytes()).unwrap(),
                Difficulty::from_u64(expected).unwrap()
            );
        }

        #[test]
        fn le_high_target() {
            let target: &[u8] = &[
                0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0,
                0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0xff, 0xff, 0xff,
            ];
            let expected = Difficulty::min();
            assert_eq!(little_endian_difficulty(target).unwrap(), expected);
        }

        #[test]
        fn le_max_difficulty() {
            let target = U256::MAX / U256::from(u64::MAX);
            let mut bytes = [0u8; 32];
            target.to_little_endian(&mut bytes);
            assert_eq!(little_endian_difficulty(&bytes).unwrap(), Difficulty::max());
        }

        #[test]
        fn le_stop_overflow() {
            let target: u64 = 64;
            let expected = u64::MAX;
            assert_eq!(
                little_endian_difficulty(&target.to_be_bytes()).unwrap(),
                Difficulty::from_u64(expected).unwrap()
            );
        }
    }
}

#[cfg(test)]
mod test {
    use crate::proof_of_work::difficulty::{CheckedAdd, Difficulty};

    #[test]
    fn add_difficulty() {
        assert_eq!(
            Difficulty::from_u64(1_000).unwrap().checked_add(8_000).unwrap(),
            Difficulty::from_u64(9_000).unwrap()
        );
        assert_eq!(
            Difficulty::default().checked_add(42).unwrap(),
            Difficulty::from_u64(43).unwrap()
        );
        assert_eq!(
            Difficulty::from_u64(15).unwrap().checked_add(5).unwrap(),
            Difficulty::from_u64(20).unwrap()
        );
    }

    #[test]
    fn test_format() {
        let d = Difficulty::from_u64(1_000_000).unwrap();
        assert_eq!("1,000,000", format!("{}", d));
    }
}
