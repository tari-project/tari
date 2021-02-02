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
    proof_of_work::{Difficulty, PowAlgorithm},
    tari_utilities::epoch_time::EpochTime,
};

#[derive(Debug, Clone)]
pub struct TargetDifficulties {
    consensus_manager: ConsensusManager,
    target_difficulties_monero: Vec<(EpochTime, Difficulty)>,
    target_difficulties_sha3: Vec<(EpochTime, Difficulty)>,
}

impl TargetDifficulties {
    pub fn new(consensus_rules: &ConsensusManager) -> Self {
        Self {
            consensus_manager: consensus_rules.clone(),
            target_difficulties_monero: Vec::new(),
            target_difficulties_sha3: Vec::new(),
        }
    }

    pub fn add_back(&mut self, header: &BlockHeader, target_difficulty: Difficulty) {
        match header.pow_algo() {
            PowAlgorithm::Monero => {
                if self.target_difficulties_monero.len() as u64 >= self.consensus_manager.max_block_window() + 1 {
                    self.target_difficulties_monero.remove(0);
                }
                self.target_difficulties_monero
                    .push((header.timestamp, target_difficulty));
            },
            // TODO: remove
            PowAlgorithm::Blake => unimplemented!(),
            PowAlgorithm::Sha3 => {
                if self.target_difficulties_sha3.len() as u64 >= self.consensus_manager.max_block_window() + 1 {
                    self.target_difficulties_sha3.remove(0);
                }
                self.target_difficulties_sha3
                    .push((header.timestamp, target_difficulty));
            },
        }
    }

    pub fn add_front(&mut self, header: &BlockHeader, target_difficulty: Difficulty) {
        match header.pow_algo() {
            PowAlgorithm::Monero => {
                if self.target_difficulties_monero.len() as u64 >= self.consensus_manager.max_block_window() + 1 {
                    self.target_difficulties_monero.pop();
                }
                self.target_difficulties_monero
                    .insert(0, (header.timestamp, target_difficulty));
            },
            // TODO: remove
            PowAlgorithm::Blake => unimplemented!(),
            PowAlgorithm::Sha3 => {
                if self.target_difficulties_sha3.len() as u64 >= self.consensus_manager.max_block_window() + 1 {
                    self.target_difficulties_sha3.pop();
                }
                self.target_difficulties_sha3
                    .insert(0, (header.timestamp, target_difficulty));
            },
        }
    }

    pub fn is_algo_full(&self, algo: PowAlgorithm) -> bool {
        match algo {
            PowAlgorithm::Monero => {
                self.target_difficulties_monero.len() as u64 == self.consensus_manager.max_block_window() + 1
            },
            PowAlgorithm::Blake => unimplemented!(),
            PowAlgorithm::Sha3 => {
                self.target_difficulties_sha3.len() as u64 == self.consensus_manager.max_block_window() + 1
            },
        }
    }

    pub fn len(&self, algo: PowAlgorithm) -> usize {
        match algo {
            PowAlgorithm::Monero => self.target_difficulties_monero.len(),
            PowAlgorithm::Blake => unimplemented!(),
            PowAlgorithm::Sha3 => self.target_difficulties_sha3.len(),
        }
    }

    pub fn is_full(&self) -> bool {
        self.target_difficulties_monero.len() as u64 == self.consensus_manager.max_block_window() + 1 &&
            self.target_difficulties_sha3.len() as u64 == self.consensus_manager.max_block_window() + 1
    }

    pub fn calculate(&self, header: &BlockHeader) -> Difficulty {
        let target_diff_window = match header.pow_algo() {
            PowAlgorithm::Monero => {
                let mut window = self
                    .consensus_manager
                    .new_target_difficulty(PowAlgorithm::Monero, header.height);
                let mut counter = self.target_difficulties_monero.len() - 1;
                for _ in 0..self
                    .consensus_manager
                    .consensus_constants(header.height)
                    .get_difficulty_block_window()
                {
                    window.add_back(
                        self.target_difficulties_monero[counter].0,
                        self.target_difficulties_monero[counter].1,
                    );
                    counter = counter - 1;
                }
                window
            },
            PowAlgorithm::Blake => unimplemented!(),
            PowAlgorithm::Sha3 => {
                let mut window = self
                    .consensus_manager
                    .new_target_difficulty(PowAlgorithm::Sha3, header.height);
                let mut counter = self.target_difficulties_sha3.len() - 1;
                for _ in 0..self
                    .consensus_manager
                    .consensus_constants(header.height)
                    .get_difficulty_block_window()
                {
                    window.add_back(
                        self.target_difficulties_sha3[counter].0,
                        self.target_difficulties_sha3[counter].1,
                    );
                    counter = counter - 1;
                }
                window
            },
        };
        target_diff_window.calculate()
    }
}
