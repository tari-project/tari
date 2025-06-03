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

use std::collections::HashMap;

use crate::{
    blocks::BlockHeader,
    consensus::ConsensusManager,
    proof_of_work::{Difficulty, PowAlgorithm, TargetDifficultyWindow},
};

#[derive(Debug, Clone)]
pub struct TargetDifficulties {
    algos: HashMap<PowAlgorithm, TargetDifficultyWindow>,
}

impl TargetDifficulties {
    pub fn new(consensus_rules: &ConsensusManager, height: u64) -> Result<Self, String> {
        let permitted_algos = consensus_rules
            .consensus_constants(height)
            .current_permitted_pow_algos();
        let mut algos = HashMap::new();
        for algo in permitted_algos {
            let target_difficulty_window = consensus_rules.new_target_difficulty(algo, height)?;
            algos.insert(algo, target_difficulty_window);
        }
        Ok(Self { algos })
    }

    pub fn update_algos(&mut self, consensus_rules: &ConsensusManager, height: u64) -> Result<(), String> {
        let consensus_constants = consensus_rules.consensus_constants(height);
        let permitted_algos = consensus_constants.current_permitted_pow_algos();
        let current_keys: Vec<PowAlgorithm> = self.algos.keys().copied().collect();
        for algo in current_keys {
            if !permitted_algos.contains(&algo) {
                self.algos.remove(&algo);
            }
        }
        for algo in permitted_algos {
            if let std::collections::hash_map::Entry::Vacant(e) = self.algos.entry(algo) {
                let target_difficulty_window = consensus_rules.new_target_difficulty(algo, height)?;
                e.insert(target_difficulty_window);
            } else if let Some(target_diff) = self.algos.get_mut(&algo) {
                target_diff.update_target_time(consensus_constants.pow_target_block_interval(algo))?
            } else {
                // clippy, this else should never be hit
            }
        }
        Ok(())
    }

    pub fn add_back(&mut self, header: &BlockHeader, target_difficulty: Difficulty) -> Result<(), String> {
        self.get_mut(header.pow_algo())?
            .add_back(header.timestamp(), target_difficulty);
        Ok(())
    }

    pub fn add_front(&mut self, header: &BlockHeader, target_difficulty: Difficulty) -> Result<(), String> {
        self.get_mut(header.pow_algo())?
            .add_front(header.timestamp(), target_difficulty);
        Ok(())
    }

    pub fn is_algo_full(&self, algo: PowAlgorithm) -> Result<bool, String> {
        Ok(self.get(algo)?.is_full())
    }

    pub fn is_full(&self) -> bool {
        let mut result = true;
        for algo in self.algos.values() {
            result = result && algo.is_full();
        }
        result
    }

    pub fn get(&self, algo: PowAlgorithm) -> Result<&TargetDifficultyWindow, String> {
        self.algos.get(&algo).ok_or("Algorithm not found".to_string())
    }

    fn get_mut(&mut self, algo: PowAlgorithm) -> Result<&mut TargetDifficultyWindow, String> {
        self.algos.get_mut(&algo).ok_or("Algorithm not found".to_string())
    }

    pub fn algo_count(&self) -> usize {
        self.algos.len()
    }
}
