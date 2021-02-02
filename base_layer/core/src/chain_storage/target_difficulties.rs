//  Copyright 2020, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use crate::{
    blocks::BlockHeader,
    consensus::ConsensusManager,
    proof_of_work::{Difficulty, PowAlgorithm, TargetDifficultyWindow},
};

#[derive(Debug, Clone)]
pub struct TargetDifficulties {
    consensus_manager: ConsensusManager,
    monero: TargetDifficultyWindow,
    sha3: TargetDifficultyWindow,
}

impl TargetDifficulties {
    pub fn new(consensus_rules: &ConsensusManager, height: u64) -> Self {
        Self {
            consensus_manager: consensus_rules.clone(),
            monero: consensus_rules.new_target_difficulty(PowAlgorithm::Monero, height),
            sha3: consensus_rules.new_target_difficulty(PowAlgorithm::Sha3, height),
        }
    }

    pub fn add_back(&mut self, header: &BlockHeader, target_difficulty: Difficulty) {
        if self
            .consensus_manager
            .consensus_constants(header.height)
            .effective_from_height() ==
            header.height
        {
            // If this matches we need to update the constants, as on this height a new height is present.
            self.update_consensus_constants(header.height);
        }
        self.get_mut(header.pow_algo())
            .add_back(header.timestamp(), target_difficulty);
    }

    pub fn add_front(&mut self, header: &BlockHeader, target_difficulty: Difficulty) {
        if self
            .consensus_manager
            .consensus_constants(header.height)
            .effective_from_height() ==
            header.height
        {
            // If this matches we need to update the constants, as on this height a new height is present.
            self.update_consensus_constants(header.height);
        }
        self.get_mut(header.pow_algo())
            .add_front(header.timestamp(), target_difficulty);
    }

    pub fn is_algo_full(&self, algo: PowAlgorithm) -> bool {
        self.get(algo).is_full()
    }

    pub fn is_full(&self) -> bool {
        self.sha3.is_full() && self.monero.is_full()
    }

    pub fn get(&self, algo: PowAlgorithm) -> &TargetDifficultyWindow {
        use PowAlgorithm::*;
        match algo {
            Monero => &self.monero,
            // TODO: remove
            Blake => unimplemented!(),
            Sha3 => &self.sha3,
        }
    }

    fn get_mut(&mut self, algo: PowAlgorithm) -> &mut TargetDifficultyWindow {
        use PowAlgorithm::*;
        match algo {
            Monero => &mut self.monero,
            // TODO: remove
            Blake => unimplemented!(),
            Sha3 => &mut self.sha3,
        }
    }

    fn update_consensus_constants(&mut self, height: u64) {
        self.monero.update_consensus_constants(
            &self.consensus_manager.consensus_constants(height),
            PowAlgorithm::Monero,
        );
        self.sha3
            .update_consensus_constants(&self.consensus_manager.consensus_constants(height), PowAlgorithm::Sha3);
    }
}
