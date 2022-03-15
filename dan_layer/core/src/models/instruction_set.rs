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

use std::{convert::TryFrom, hash::Hash, iter::FromIterator};

use tari_crypto::common::Blake256;
use tari_mmr::MerkleMountainRange;

use crate::{
    fixed_hash::FixedHash,
    models::{ConsensusHash, Instruction},
};

#[derive(PartialEq, Clone, Debug, Hash)]
pub struct InstructionSetHash(FixedHash);

impl InstructionSetHash {
    pub fn zero() -> InstructionSetHash {
        Self(FixedHash::zero())
    }
}

impl InstructionSetHash {
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_slice()
    }
}

impl From<FixedHash> for InstructionSetHash {
    fn from(hash: FixedHash) -> Self {
        Self(hash)
    }
}

// TODO: Implement hash properly
#[allow(clippy::derive_hash_xor_eq)]
#[derive(Clone, Debug)]
pub struct InstructionSet {
    hash: InstructionSetHash,
    instructions: Vec<Instruction>,
}

impl InstructionSet {
    pub fn empty() -> Self {
        Self::from_vec(vec![])
    }

    pub fn from_vec(instructions: Vec<Instruction>) -> Self {
        let mut result = Self {
            instructions,
            hash: InstructionSetHash::zero(),
        };
        result.hash = result.calculate_hash();
        result
    }

    pub fn calculate_hash(&self) -> InstructionSetHash {
        let mut mmr = MerkleMountainRange::<Blake256, _>::new(Vec::default());
        // assume instructions are sorted
        for instruction in &self.instructions {
            mmr.push(instruction.calculate_hash().to_vec()).unwrap();
        }

        FixedHash::try_from(mmr.get_merkle_root().unwrap()).unwrap().into()
    }

    pub fn instructions(&self) -> &[Instruction] {
        self.instructions.as_slice()
    }
}

impl FromIterator<Instruction> for InstructionSet {
    fn from_iter<T: IntoIterator<Item = Instruction>>(iter: T) -> Self {
        let instructions = iter.into_iter().collect();
        Self::from_vec(instructions)
    }
}

impl From<Vec<Instruction>> for InstructionSet {
    fn from(instructions: Vec<Instruction>) -> Self {
        Self::from_vec(instructions)
    }
}

impl ConsensusHash for InstructionSet {
    fn consensus_hash(&self) -> &[u8] {
        self.hash.as_bytes()
    }
}

impl IntoIterator for InstructionSet {
    type IntoIter = <Vec<Instruction> as IntoIterator>::IntoIter;
    type Item = Instruction;

    fn into_iter(self) -> Self::IntoIter {
        self.instructions.into_iter()
    }
}

impl Extend<Instruction> for InstructionSet {
    fn extend<T: IntoIterator<Item = Instruction>>(&mut self, iter: T) {
        self.instructions.extend(iter);
        self.hash = self.calculate_hash();
    }
}
