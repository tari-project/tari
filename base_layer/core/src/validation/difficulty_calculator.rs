// Copyright 2021. The Tari Project
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
    blocks::BlockHeader,
    chain_storage::{fetch_target_difficulty_for_next_block, BlockchainBackend},
    consensus::ConsensusManager,
    proof_of_work::{randomx_factory::RandomXFactory, AchievedTargetDifficulty},
    validation::{helpers::check_target_difficulty, ValidationError},
};

#[derive(Clone)]
pub struct DifficultyCalculator {
    pub rules: ConsensusManager,
    pub randomx_factory: RandomXFactory,
}

impl DifficultyCalculator {
    pub fn new(rules: ConsensusManager, randomx_factory: RandomXFactory) -> Self {
        Self { rules, randomx_factory }
    }

    pub fn check_achieved_and_target_difficulty<B: BlockchainBackend>(
        &self,
        db: &B,
        block_header: &BlockHeader,
    ) -> Result<AchievedTargetDifficulty, ValidationError> {
        let difficulty_window =
            fetch_target_difficulty_for_next_block(db, &self.rules, block_header.pow_algo(), &block_header.prev_hash)?;
        let constants = self.rules.consensus_constants(block_header.height);
        let target = difficulty_window.calculate(
            constants.min_pow_difficulty(block_header.pow.pow_algo),
            constants.max_pow_difficulty(block_header.pow.pow_algo),
        );
        let gen_hash = *self.rules.get_genesis_block().hash();
        let achieved_target = check_target_difficulty(block_header, target, &self.randomx_factory, &gen_hash)?;

        Ok(achieved_target)
    }
}
