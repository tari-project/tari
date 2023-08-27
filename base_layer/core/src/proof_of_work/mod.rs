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

/// Crates for proof of work difficulty
#[cfg(any(feature = "base_node", feature = "transactions"))]
pub(crate) mod difficulty;
#[cfg(any(feature = "base_node", feature = "transactions"))]
pub use difficulty::{Difficulty, DifficultyAdjustment};

/// Crates for proof of work error
#[cfg(any(feature = "base_node", feature = "transactions"))]
mod error;
#[cfg(any(feature = "base_node", feature = "transactions"))]
pub use error::{DifficultyAdjustmentError, DifficultyError, PowError};

/// Crates for proof of work monero_rx
#[cfg(feature = "base_node")]
pub mod monero_rx;
#[cfg(feature = "base_node")]
pub use monero_rx::randomx_difficulty;

/// Crate for proof of work itself
#[cfg(any(feature = "base_node", feature = "transactions"))]
#[allow(clippy::module_inception)]
mod proof_of_work;
#[cfg(any(feature = "base_node", feature = "transactions"))]
pub use proof_of_work::ProofOfWork;

/// Crates for proof of work proof_of_work_algorithm
#[cfg(any(feature = "base_node", feature = "transactions"))]
mod proof_of_work_algorithm;
#[cfg(any(feature = "base_node", feature = "transactions"))]
pub use proof_of_work_algorithm::PowAlgorithm;

/// Crates for proof of work sha3_pow
#[cfg(feature = "base_node")]
mod sha3x_pow;
#[cfg(feature = "base_node")]
pub use sha3x_pow::sha3x_difficulty;
#[cfg(all(test, feature = "base_node"))]
pub use sha3x_pow::test as sha3x_test;

/// Crates for proof of work target_difficulty
mod target_difficulty;
pub use target_difficulty::AchievedTargetDifficulty;

/// Crates for proof of work target_difficulty_window
#[cfg(feature = "base_node")]
mod target_difficulty_window;
#[cfg(feature = "base_node")]
pub use target_difficulty_window::TargetDifficultyWindow;

/// Crates for proof of work lwma_diff
pub mod lwma_diff;

/// Crates for proof of work randomx_factory
#[cfg(feature = "base_node")]
pub mod randomx_factory;
