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

use crate::{dan_layer::models::Instruction, digital_assets_error::DigitalAssetError};
use std::{
    sync::{Arc, Mutex},
};

pub trait MempoolService {
    fn submit_instruction(&mut self, instruction: Instruction) -> Result<(), DigitalAssetError>;
    fn read_block(&self, limit: usize) -> Result<Vec<Instruction>, DigitalAssetError>;
    fn remove_instructions(&mut self, instructions: &[Instruction]) -> Result<(), DigitalAssetError>;
    fn size(&self) -> usize;
}

pub struct ConcreteMempoolService {
    instructions: Vec<Instruction>,
}

impl ConcreteMempoolService {
    pub fn new() -> Self {
        Self { instructions: vec![] }
    }
}

impl MempoolService for ConcreteMempoolService {
    fn submit_instruction(&mut self, instruction: Instruction) -> Result<(), DigitalAssetError> {
        self.instructions.push(instruction);
        Ok(())
    }

    fn read_block(&self, _limit: usize) -> Result<Vec<Instruction>, DigitalAssetError> {
        Ok(self.instructions.clone())
    }

    fn remove_instructions(&mut self, instructions: &[Instruction]) -> Result<(), DigitalAssetError> {
        let mut result = self.instructions.clone();
        for i in instructions {
            if let Some(position) = result.iter().position(|r| r == i) {
                result.remove(position);
            }
        }
        self.instructions = result;
        Ok(())
    }

    fn size(&self) -> usize {
        self.instructions.len()
    }
}

#[derive(Clone)]
pub struct MempoolServiceHandle {
    mempool: Arc<Mutex<ConcreteMempoolService>>,
}

impl MempoolServiceHandle {
    pub fn new(mempool: Arc<Mutex<ConcreteMempoolService>>) -> Self {
        Self { mempool }
    }
}

impl MempoolService for MempoolServiceHandle {
    fn submit_instruction(&mut self, instruction: Instruction) -> Result<(), DigitalAssetError> {
        self.mempool.lock().unwrap().submit_instruction(instruction)
    }

    fn read_block(&self, limit: usize) -> Result<Vec<Instruction>, DigitalAssetError> {
        self.mempool.lock().unwrap().read_block(limit)
    }

    fn remove_instructions(&mut self, instructions: &[Instruction]) -> Result<(), DigitalAssetError> {
        self.mempool.lock().unwrap().remove_instructions(instructions)
    }

    fn size(&self) -> usize {
        self.mempool.lock().unwrap().size()
    }
}
