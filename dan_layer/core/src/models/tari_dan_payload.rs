//  Copyright 2021. The Tari Project
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

use std::fmt::Debug;

use digest::Digest;
use tari_crypto::common::Blake256;

use crate::models::{ConsensusHash, Instruction, InstructionSet, Payload};

#[derive(Debug, Clone)]
pub struct TariDanPayload {
    hash: Vec<u8>,
    instruction_set: InstructionSet,
    checkpoint: Option<CheckpointData>,
}

impl TariDanPayload {
    pub fn new(instruction_set: InstructionSet, checkpoint: Option<CheckpointData>) -> Self {
        let mut result = Self {
            hash: vec![],
            instruction_set,
            checkpoint,
        };
        result.hash = result.calculate_hash();
        result
    }

    pub fn destruct(self) -> (InstructionSet, Option<CheckpointData>) {
        (self.instruction_set, self.checkpoint)
    }

    pub fn instructions(&self) -> &[Instruction] {
        self.instruction_set.instructions()
    }

    fn calculate_hash(&self) -> Vec<u8> {
        let result = Blake256::new().chain(self.instruction_set.consensus_hash());
        if let Some(ref ck) = self.checkpoint {
            result.chain(ck.consensus_hash()).finalize().to_vec()
        } else {
            result.finalize().to_vec()
        }
    }
}

impl ConsensusHash for TariDanPayload {
    fn consensus_hash(&self) -> &[u8] {
        self.hash.as_slice()
    }
}

impl Payload for TariDanPayload {}

#[derive(Debug, Clone, Default)]
pub struct CheckpointData {
    hash: Vec<u8>,
}

impl ConsensusHash for CheckpointData {
    fn consensus_hash(&self) -> &[u8] {
        self.hash.as_slice()
    }
}
