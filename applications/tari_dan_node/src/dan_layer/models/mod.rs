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

mod instruction;

pub use instruction::Instruction;


pub struct InstructionId(u64);



pub struct InstructionCaller {
    owner_token_id: TokenId
}


impl InstructionCaller {
    pub fn owner_token_id(&self) -> &TokenId {
        &self.owner_token_id
    }
}

pub enum TemplateId {
    EditableMetadata
}

#[derive(Clone)]
pub struct TokenId(pub Vec<u8>);

impl AsRef<[u8]> for TokenId {
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}

#[derive(Copy, Clone)]
pub struct ViewId(pub u64);

impl ViewId{
    pub fn current_leader(&self, committee_size: usize) -> usize {
        (self.0 % committee_size as u64) as usize
    }
}

// TODO: Encapsulate
#[derive()]
pub struct View{ pub view_id: ViewId,  pub is_leader: bool}
