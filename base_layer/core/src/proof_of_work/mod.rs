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

#[cfg(any(feature = "base_node", feature = "transactions"))]
pub(crate) mod difficulty;
#[cfg(any(feature = "base_node", feature = "transactions"))]
pub use difficulty::{Difficulty, DifficultyAdjustment};

#[cfg(any(feature = "base_node", feature = "transactions"))]
mod error;
#[cfg(any(feature = "base_node", feature = "transactions"))]
pub use error::{DifficultyAdjustmentError, PowError};

#[cfg(feature = "base_node")]
pub mod monero_rx;
#[cfg(feature = "base_node")]
pub use monero_rx::monero_difficulty;

#[cfg(feature = "base_node")]
#[allow(clippy::module_inception)]
mod proof_of_work;
#[cfg(feature = "base_node")]
pub use proof_of_work::ProofOfWork;

#[cfg(any(feature = "base_node", feature = "transactions"))]
mod proof_of_work_algorithm;
#[cfg(any(feature = "base_node", feature = "transactions"))]
pub use proof_of_work_algorithm::PowAlgorithm;

#[cfg(feature = "base_node")]
mod sha3_pow;
#[cfg(feature = "base_node")]
pub use sha3_pow::sha3_difficulty;
#[cfg(all(test, feature = "base_node"))]
pub use sha3_pow::test as sha3_test;

#[cfg(feature = "base_node")]
mod target_difficulty;
#[cfg(feature = "base_node")]
pub use target_difficulty::TargetDifficultyWindow;

#[cfg(feature = "base_node")]
pub mod lwma_diff;

#[cfg(feature = "base_node")]
pub mod randomx_factory;
