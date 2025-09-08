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

use crate::tari_proof_of_work::{Difficulty, DifficultyError, MIN_DIFFICULTY};
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

    pub fn checked_sub_difficulty(&self, d: Difficulty) -> Option<AccumulatedDifficulty> {
        self.0.checked_sub(u128::from(d.as_u64())).map(AccumulatedDifficulty)
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

impl fmt::Display for AccumulatedDifficulty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let formatted = self.0;
        write!(f, "{formatted}")
    }
}
