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

use crate::{
    consensus::ConsensusManagerError,
    proof_of_work::{difficulty::DifficultyAdjustment, lwma_diff::LinearWeightedMovingAverage, Difficulty},
};
use tari_crypto::tari_utilities::epoch_time::EpochTime;

/// Returns the estimated target difficulty for the provided set of target difficulties.
pub fn get_target_difficulty(
    target_difficulties: Vec<(EpochTime, Difficulty)>,
    block_window: usize,
    target_time: u64,
    initial_difficulty: Difficulty,
    max_block_time: u64,
) -> Result<Difficulty, ConsensusManagerError>
{
    let mut lwma = LinearWeightedMovingAverage::new(block_window, target_time, initial_difficulty, max_block_time);
    for target_difficulty in target_difficulties {
        lwma.add(target_difficulty.0, target_difficulty.1)?
    }
    let target_difficulty = lwma.get_difficulty();
    Ok(target_difficulty)
}
