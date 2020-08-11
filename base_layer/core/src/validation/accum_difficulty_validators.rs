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
    chain_storage::BlockchainBackend,
    proof_of_work::Difficulty,
    validation::{Validation, ValidationError},
};

/// This validator will check if a provided accumulated difficulty is stronger than the chain tip.
#[derive(Clone)]
pub struct AccumDifficultyValidator {}

impl<B: BlockchainBackend> Validation<Difficulty, B> for AccumDifficultyValidator {
    fn validate(&self, accum_difficulty: &Difficulty, db: &B) -> Result<(), ValidationError> {
        let tip_header = db
            .fetch_last_header()?
            .ok_or_else(|| ValidationError::custom_error("Cannot retrieve tip header. Blockchain DB is empty"))?;
        if *accum_difficulty <= tip_header.total_accumulated_difficulty_inclusive() {
            return Err(ValidationError::WeakerAccumulatedDifficulty);
        }
        Ok(())
    }
}

/// This a mock validator that can be used for testing, it will check if a provided accumulated difficulty is equal or
/// stronger than the chain tip. This will simplify testing where small testing blockchains need to be constructed as
/// the accumulated difficulty of preceding blocks don't have to have an increasing accumulated difficulty.
#[derive(Clone)]
pub struct MockAccumDifficultyValidator;

impl<B: BlockchainBackend> Validation<Difficulty, B> for MockAccumDifficultyValidator {
    fn validate(&self, accum_difficulty: &Difficulty, db: &B) -> Result<(), ValidationError> {
        let tip_header = db
            .fetch_last_header()?
            .ok_or_else(|| ValidationError::custom_error("Cannot retrieve tip header. Blockchain DB is empty"))?;
        if *accum_difficulty < tip_header.total_accumulated_difficulty_inclusive() {
            return Err(ValidationError::WeakerAccumulatedDifficulty);
        }
        Ok(())
    }
}
