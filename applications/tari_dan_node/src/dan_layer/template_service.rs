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

use crate::dan_layer::{TemplateId, InstructionId, TokenId};
use crate::dan_layer::template_command::{TemplateCommand, ExecutionResult};
use crate::types::PublicKey;
use crate::dan_layer::TemplateId::EditableMetadata;
use crate::dan_layer::templates::editable_metadata_template::EditableMetadataTemplate;
use crate::digital_assets_error::DigitalAssetError;
use crate::dan_layer::asset_data_store::{AssetDataStore, FileAssetDataStore};

pub struct TemplateService {
 template_factory: TemplateFactory,
    instruction_log: Box<dyn InstructionLog>,
    data_store: Box<dyn AssetDataStore>
}




impl TemplateService {

    pub fn new(data_store: Box<dyn AssetDataStore>) -> Self {
        Self{
            template_factory: TemplateFactory{},
            instruction_log: Box::new(MemoryInstructionLog::default()),
            data_store
        }
    }

    pub fn execute_instruction(&mut self, template: TemplateId, method: String, args: Vec<Vec<u8>>, caller: InstructionCaller, id : InstructionId) -> Result<(), DigitalAssetError>{
        let instruction = self.template_factory.create_command(template, method, args, caller)?;
        let result = instruction.try_execute(self.data_store.as_mut())?;
        self.instruction_log.store(id, result);
        Ok(())
    }
}

pub struct TemplateFactory {

}

impl TemplateFactory {
    pub fn create_command(&self, template: TemplateId, method: String, args: Vec<Vec<u8>>, caller: InstructionCaller) -> Result<impl TemplateCommand, DigitalAssetError> {
        match template {
           TemplateId::EditableMetadata => EditableMetadataTemplate::create_command(method, args, caller)
        }
    }
}

pub struct InstructionCaller {
    owner_token_id: TokenId
}


impl InstructionCaller {
    pub fn owner_token_id(&self) -> &TokenId {
        &self.owner_token_id
    }
}

pub trait InstructionLog {
    fn store(&mut self, id: InstructionId, result: ExecutionResult);
}

#[derive(Default)]
pub struct MemoryInstructionLog {
    log: Vec<(InstructionId, ExecutionResult)>
}



impl InstructionLog for MemoryInstructionLog {
    fn store(&mut self, id: InstructionId, result: ExecutionResult) {
        self.log.push((id, result))
    }
}
