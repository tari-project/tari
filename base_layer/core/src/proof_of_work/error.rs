// Copyright 2019. The Taiji Project
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

use thiserror::Error;

#[cfg(feature = "base_node")]
use crate::proof_of_work::monero_rx::MergeMineError;
use crate::proof_of_work::Difficulty;

/// Errors that can occur when validating a proof of work
#[derive(Debug, Error)]
pub enum PowError {
    #[error("ProofOfWorkFailed")]
    InvalidProofOfWork,
    #[error("Achieved difficulty is below the minimum")]
    AchievedDifficultyBelowMin,
    #[error("Target difficulty {target} not achieved. Achieved difficulty: {achieved}")]
    AchievedDifficultyTooLow { target: Difficulty, achieved: Difficulty },
    #[error("Invalid target difficulty (expected: {expected}, got: {got})")]
    InvalidTargetDifficulty { expected: Difficulty, got: Difficulty },
    #[cfg(feature = "base_node")]
    #[error("Invalid merge mining data or operation: {0}")]
    MergeMineError(#[from] MergeMineError),
}

/// Errors that can occur when adjusting the difficulty
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DifficultyAdjustmentError {
    #[error("Accumulated difficulty values can only strictly increase")]
    DecreasingAccumulatedDifficulty,
    #[error("Other difficulty algorithm error")]
    Other,
}

/// Errors that can occur when converting a difficulty
#[derive(Debug, Error)]
pub enum DifficultyError {
    #[error("Difficulty conversion less than the minimum difficulty")]
    InvalidDifficulty,
    #[error("Maximum block time overflowed u64")]
    MaxBlockTimeOverflow,
}
