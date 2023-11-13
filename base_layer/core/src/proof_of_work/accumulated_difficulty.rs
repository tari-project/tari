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

use serde::{Deserialize, Serialize};
use tari_utilities::ByteArray;

use crate::proof_of_work::{difficulty::MIN_DIFFICULTY, error::DifficultyError, Difficulty};

/// The difficulty is defined as the maximum target divided by the block hash.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Deserialize, Serialize)]
pub struct AccumulatedDifficulty(u128);

impl AccumulatedDifficulty {
    /// A const constructor for Difficulty
    pub fn from_u128(d: u128) -> Result<Self, DifficultyError> {
        if d < u128::from(MIN_DIFFICULTY) {
            return Err(DifficultyError::InvalidDifficulty);
        }
        Ok(Self(d))
    }

    /// Return the difficulty as a `u128`
    pub fn as_u128(self) -> u128 {
        self.0
    }

    /// Difficulty of MIN_DIFFICULTY
    pub fn min() -> AccumulatedDifficulty {
        AccumulatedDifficulty(MIN_DIFFICULTY.into())
    }

    /// Maximum Difficulty
    pub fn max() -> AccumulatedDifficulty {
        AccumulatedDifficulty(u128::MAX)
    }

    pub fn checked_add_difficulty(&self, d: Difficulty) -> Option<AccumulatedDifficulty> {
        self.0.checked_add(u128::from(d.as_u64())).map(AccumulatedDifficulty)
    }

    pub fn to_be_bytes(&self) -> Vec<u8> {
        self.0.to_be_bytes().to_vec()
    }
}

impl Default for AccumulatedDifficulty {
    fn default() -> Self {
        AccumulatedDifficulty::min()
    }
}

// impl From<Difficulty> for u64 {
//     fn from(value: Difficulty) -> Self {
//         value.0
//     }
// }
//
impl fmt::Display for AccumulatedDifficulty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let formatted = self.0;
        write!(f, "{}", formatted)
    }
}

// #[cfg(test)]
// mod test {
//     use crate::{
//         proof_of_work::{
//             difficulty::{CheckedAdd, CheckedSub, MIN_DIFFICULTY},
//             Difficulty,
//         },
//         U256,
//     };
//
//     #[test]
//     fn add_difficulty() {
//         assert_eq!(
//             Difficulty::from_u64(1_000).unwrap().checked_add(8_000).unwrap(),
//             Difficulty::from_u64(9_000).unwrap()
//         );
//         assert_eq!(
//             Difficulty::default().checked_add(42).unwrap(),
//             Difficulty::from_u64(MIN_DIFFICULTY + 42).unwrap()
//         );
//         assert_eq!(
//             Difficulty::from_u64(15).unwrap().checked_add(5).unwrap(),
//             Difficulty::from_u64(20).unwrap()
//         );
//     }
//
//     #[test]
//     fn test_format() {
//         let d = Difficulty::from_u64(1_000_000).unwrap();
//         assert_eq!("1,000,000", format!("{}", d));
//     }
//
//     #[test]
//     fn difficulty_converts_correctly_at_its_limits() {
//         for d in 0..=MIN_DIFFICULTY + 1 {
//             if d < MIN_DIFFICULTY {
//                 assert!(Difficulty::from_u64(d).is_err());
//             } else {
//                 assert!(Difficulty::from_u64(d).is_ok());
//             }
//         }
//         assert_eq!(Difficulty::min().as_u64(), MIN_DIFFICULTY);
//         assert_eq!(Difficulty::max().as_u64(), u64::MAX);
//     }
//
//     #[test]
//     fn addition_does_not_overflow() {
//         let d1 = Difficulty::from_u64(100).unwrap();
//         assert!(d1.checked_add(1).is_some());
//         let d2 = Difficulty::max();
//         assert!(d2.checked_add(1).is_none());
//     }
//
//     #[test]
//     fn subtraction_does_not_underflow() {
//         let d1 = Difficulty::from_u64(100).unwrap();
//         assert!(d1.checked_sub(1).is_some());
//         let d2 = Difficulty::max();
//         assert!(d1.checked_sub(d2).is_none());
//     }
//
//     #[test]
//     fn be_high_target() {
//         let target: &[u8] = &[
//             0xff, 0xff, 0xff, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0,
// 0x0,             0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0,
//         ];
//         let expected = Difficulty::min();
//         assert_eq!(Difficulty::big_endian_difficulty(target).unwrap(), expected);
//     }
//
//     #[test]
//     fn be_max_difficulty() {
//         let target = U256::MAX / U256::from(u64::MAX);
//         let mut bytes = [0u8; 32];
//         target.to_big_endian(&mut bytes);
//         assert_eq!(Difficulty::big_endian_difficulty(&bytes).unwrap(), Difficulty::max());
//     }
//
//     #[test]
//     fn be_stop_overflow() {
//         let target: u64 = 64;
//         let expected = u64::MAX;
//         assert_eq!(
//             Difficulty::big_endian_difficulty(&target.to_be_bytes()).unwrap(),
//             Difficulty::from_u64(expected).unwrap()
//         );
//     }
//
//     #[test]
//     fn le_high_target() {
//         let target: &[u8] = &[
//             0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0,
//             0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0xff, 0xff, 0xff,
//         ];
//         let expected = Difficulty::min();
//         assert_eq!(Difficulty::little_endian_difficulty(target).unwrap(), expected);
//     }
//
//     #[test]
//     fn le_max_difficulty() {
//         let target = U256::MAX / U256::from(u64::MAX);
//         let mut bytes = [0u8; 32];
//         target.to_little_endian(&mut bytes);
//         assert_eq!(Difficulty::little_endian_difficulty(&bytes).unwrap(), Difficulty::max());
//     }
//
//     #[test]
//     fn le_stop_overflow() {
//         let target: u64 = 64;
//         let expected = u64::MAX;
//         assert_eq!(
//             Difficulty::little_endian_difficulty(&target.to_be_bytes()).unwrap(),
//             Difficulty::from_u64(expected).unwrap()
//         );
//     }
//
//     #[test]
//     fn u256_scalar_to_difficulty_division_by_zero() {
//         let bytes = [];
//         assert!(Difficulty::little_endian_difficulty(&bytes).is_err());
//         assert!(Difficulty::big_endian_difficulty(&bytes).is_err());
//         let bytes = [0u8; 32];
//         assert!(Difficulty::little_endian_difficulty(&bytes).is_err());
//         assert!(Difficulty::big_endian_difficulty(&bytes).is_err());
//     }
// }
