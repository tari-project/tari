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
        models::{AssetDefinition, Instruction, InstructionCaller, TemplateId},
        storage::{AssetStore, ChainDbUnitOfWork, StateDb, StateDbUnitOfWork, UnitOfWork},
        template_command::{ExecutionResult, TemplateCommand},
        templates::editable_metadata_template::_EditableMetadataTemplate,
    },
    digital_assets_error::DigitalAssetError,
};
use async_trait::async_trait;
use std::{collections::VecDeque, sync::Arc};
use tokio::sync::RwLock;

pub trait AssetProcessor {
    // purposefully made sync, because instructions should be run in order, and complete before the
    // next one starts. There may be a better way to enforce this though...
    fn execute_instruction<TUnitOfWork: UnitOfWork>(
        &self,
        instruction: &Instruction,
        db: TUnitOfWork,
    ) -> Result<(), DigitalAssetError>;
}

pub struct ConcreteAssetProcessor<TInstructionLog> {
    asset_definition: AssetDefinition,
    template_factory: TemplateFactory,
    instruction_log: TInstructionLog,
}

impl<TInstructionLog: InstructionLog + Send> AssetProcessor for ConcreteAssetProcessor<TInstructionLog> {
    fn execute_instruction<TUnitOfWork: UnitOfWork>(
        &self,
        instruction: &Instruction,
        db: TUnitOfWork,
    ) -> Result<(), DigitalAssetError> {
        self.execute(
            instruction.template_id(),
            instruction.method().to_owned(),
            instruction.args().to_vec().into(),
            // InstructionCaller {
            //     owner_token_id: instruction.from_owner().to_owned(),
            // },
            instruction.hash().into(),
            db,
        )
    }
}

impl<TInstructionLog: InstructionLog> ConcreteAssetProcessor<TInstructionLog> {
    pub fn new(instruction_log: TInstructionLog, asset_definition: AssetDefinition) -> Self {
        Self {
            template_factory: TemplateFactory {},
            instruction_log,
            asset_definition,
        }
    }

    pub fn execute<TUnitOfWork: UnitOfWork>(
        &self,
        template_id: TemplateId,
        method: String,
        args: VecDeque<Vec<u8>>,
        // caller: InstructionCaller,
        hash: Vec<u8>,
        db: TUnitOfWork,
    ) -> Result<(), DigitalAssetError> {
        let instruction = self.template_factory.create_command(template_id, method, args)?;
        // let unit_of_work = state_db.new_unit_of_work();
        let result = instruction.try_execute(db)?;
        // unit_of_work.commit()?;
        // self.instruction_log.store(hash, result);
        Ok(())
    }
}

pub struct TemplateFactory {}

impl TemplateFactory {
    pub fn _create_command(
        &self,
        template: TemplateId,
        method: String,
        args: VecDeque<Vec<u8>>,
        // caller: InstructionCaller,
    ) -> Result<impl TemplateCommand, DigitalAssetError> {
        match template {
            TemplateId::_EditableMetadata => _EditableMetadataTemplate::_create_command(method, args),
        }
    }
}

pub trait InstructionLog {
    fn store(&mut self, hash: Vec<u8>, result: ExecutionResult);
}

#[derive(Default)]
pub struct MemoryInstructionLog {
    log: Vec<(Vec<u8>, ExecutionResult)>,
}

impl InstructionLog for MemoryInstructionLog {
    fn store(&mut self, hash: Vec<u8>, result: ExecutionResult) {
        self.log.push((hash, result))
    }
}
