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

use crate::proof_of_work::{Difficulty, PowAlgorithm};

/// Immutable struct that is guaranteed to have achieved the target difficulty
#[derive(Debug, Clone, Copy)]
pub struct AchievedTargetDifficulty {
    pow_algo: PowAlgorithm,
    achieved: Difficulty,
    target: Difficulty,
}

impl AchievedTargetDifficulty {
    /// Checks if the achieved difficulty is higher than the target difficulty. If not, None is returned because a valid
    /// AchievedTargetDifficulty cannot be constructed.
    pub fn try_construct(pow_algo: PowAlgorithm, target: Difficulty, achieved: Difficulty) -> Option<Self> {
        if achieved < target {
            return None;
        }
        Some(Self {
            pow_algo,
            achieved,
            target,
        })
    }

    /// Returns the achieved difficulty
    pub fn achieved(&self) -> Difficulty {
        self.achieved
    }

    /// Returns the target difficulty
    pub fn target(&self) -> Difficulty {
        self.target
    }

    /// Returns the PoW algorithm
    pub fn pow_algo(&self) -> PowAlgorithm {
        self.pow_algo
    }
}
