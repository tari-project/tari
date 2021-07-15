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

use crate::dan_layer::models::{Instruction, Payload};
use tari_crypto::common::Blake256;
use tari_mmr::MerkleMountainRange;

#[derive(PartialEq, Clone, Debug, Hash)]
pub struct InstructionSetHash(Vec<u8>);

impl InstructionSetHash {
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_slice()
    }
}

#[derive(Clone, Debug, Hash)]
pub struct InstructionSet {
    hash: InstructionSetHash,
    instructions: Vec<Instruction>,
}

impl InstructionSet {
    pub fn empty() -> Self {
        Self {
            instructions: vec![],
            hash: InstructionSetHash(vec![]),
        }
    }

    pub fn calculate_hash(&self) -> InstructionSetHash {
        let mut mmr = MerkleMountainRange::<Blake256, _>::new(Vec::default());
        // assume instructions are sorted
        for instruction in &self.instructions {
            mmr.push(instruction.calculate_hash());
        }

        InstructionSetHash(mmr.get_merkle_root().unwrap())
    }
}

impl Payload for InstructionSet {}

// TODO: Not really the correct trait, it should be AsHash
impl AsRef<[u8]> for InstructionSet {
    fn as_ref(&self) -> &[u8] {
        self.hash.as_bytes()
    }
}

impl PartialEq for InstructionSet {
    fn eq(&self, other: &Self) -> bool {
        self.hash.eq(&other.hash)
    }
}
