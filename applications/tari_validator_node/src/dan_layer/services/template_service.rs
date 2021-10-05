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
    dan_layer::{
        models::{Instruction, InstructionCaller, InstructionId, TemplateId},
        storage::AssetStore,
        template_command::{ExecutionResult, TemplateCommand},
        templates::editable_metadata_template::EditableMetadataTemplate,
    },
    digital_assets_error::DigitalAssetError,
};
use async_trait::async_trait;
use std::collections::VecDeque;

// TODO: Better name needed
#[async_trait]
pub trait TemplateService {
    async fn execute_instruction(&mut self, instruction: &Instruction) -> Result<(), DigitalAssetError>;
}

pub struct ConcreteTemplateService<TAssetStore, TInstructionLog> {
    template_id: TemplateId,
    template_factory: TemplateFactory,
    instruction_log: TInstructionLog,
    data_store: TAssetStore,
}

#[async_trait]
impl<TAssetStore: AssetStore + Send, TInstructionLog: InstructionLog + Send> TemplateService
    for ConcreteTemplateService<TAssetStore, TInstructionLog>
{
    async fn execute_instruction(&mut self, instruction: &Instruction) -> Result<(), DigitalAssetError> {
        // TODO: This is thread blocking
        self.execute(
            instruction.method().to_owned(),
            instruction.args().to_vec().into(),
            InstructionCaller {
                owner_token_id: instruction.from_owner().to_owned(),
            },
            // TODO: put in instruction
            InstructionId(0),
        )
    }
}

impl<TAssetStore: AssetStore, TInstructionLog: InstructionLog> ConcreteTemplateService<TAssetStore, TInstructionLog> {
    pub fn new(data_store: TAssetStore, instruction_log: TInstructionLog, template_id: TemplateId) -> Self {
        Self {
            template_factory: TemplateFactory {},
            instruction_log,
            template_id,
            data_store,
        }
    }

    pub fn execute(
        &mut self,
        method: String,
        args: VecDeque<Vec<u8>>,
        caller: InstructionCaller,
        id: InstructionId,
    ) -> Result<(), DigitalAssetError> {
        let instruction = self
            .template_factory
            .create_command(self.template_id, method, args, caller)?;
        let result = instruction.try_execute(&mut self.data_store)?;
        self.instruction_log.store(id, result);
        Ok(())
    }
}

pub struct TemplateFactory {}

impl TemplateFactory {
    pub fn create_command(
        &self,
        template: TemplateId,
        method: String,
        args: VecDeque<Vec<u8>>,
        caller: InstructionCaller,
    ) -> Result<impl TemplateCommand, DigitalAssetError> {
        match template {
            TemplateId::EditableMetadata => EditableMetadataTemplate::create_command(method, args, caller),
        }
    }
}

pub trait InstructionLog {
    fn store(&mut self, id: InstructionId, result: ExecutionResult);
}

#[derive(Default)]
pub struct MemoryInstructionLog {
    log: Vec<(InstructionId, ExecutionResult)>,
}

impl InstructionLog for MemoryInstructionLog {
    fn store(&mut self, id: InstructionId, result: ExecutionResult) {
        self.log.push((id, result))
    }
}
