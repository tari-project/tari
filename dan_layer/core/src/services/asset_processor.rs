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
    digital_assets_error::DigitalAssetError,
    models::{AssetDefinition, Instruction, TemplateId},
    storage::state::StateDbUnitOfWork,
    template_command::ExecutionResult,
    templates::tip002_template,
};

use std::collections::VecDeque;
use tari_core::transactions::transaction::TemplateParameter;

pub trait AssetProcessor {
    fn init_template<TUnitOfWork: StateDbUnitOfWork>(
        &self,
        template_parameter: &TemplateParameter,
        asset_definition: &AssetDefinition,
        state_db: &mut TUnitOfWork,
    ) -> Result<(), DigitalAssetError>;

    // purposefully made sync, because instructions should be run in order, and complete before the
    // next one starts. There may be a better way to enforce this though...
    fn execute_instruction<TUnitOfWork: StateDbUnitOfWork>(
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
    fn init_template<TUnitOfWork: StateDbUnitOfWork>(
        &self,
        template_parameter: &TemplateParameter,
        asset_definition: &AssetDefinition,
        state_db: &mut TUnitOfWork,
    ) -> Result<(), DigitalAssetError> {
        self.template_factory
            .init(template_parameter, asset_definition, state_db)
    }

    fn execute_instruction<TUnitOfWork: StateDbUnitOfWork>(
        &self,
        instruction: &Instruction,
        db: TUnitOfWork,
    ) -> Result<(), DigitalAssetError> {
        self.execute(
            instruction.template_id(),
            instruction.method().to_owned(),
            instruction.args().into(),
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

    pub fn execute<TUnitOfWork: StateDbUnitOfWork>(
        &self,
        _template_id: TemplateId,
        _method: String,
        _args: Vec<u8>,
        // caller: InstructionCaller,
        _hash: Vec<u8>,
        _db: TUnitOfWork,
    ) -> Result<(), DigitalAssetError> {
        todo!()
        // let instruction = self.template_factory.create_command(template_id, method, args)?;
        // let unit_of_work = state_db.new_unit_of_work();
        // let result = instruction.try_execute(db)?;
        // unit_of_work.commit()?;
        // self.instruction_log.store(hash, result);
        // Ok(())
    }
}

pub struct TemplateFactory {}

impl TemplateFactory {
    pub fn init<TUnitOfWork: StateDbUnitOfWork>(
        &self,
        template: &TemplateParameter,
        asset_definition: &AssetDefinition,
        state_db: &mut TUnitOfWork,
    ) -> Result<(), DigitalAssetError> {
        match TemplateId::from(template.template_id) {
            TemplateId::Tip002 => tip002_template::init(template, asset_definition, state_db)?,
            _ => unimplemented!(),
        }
        Ok(())
    }

    pub fn create_command(
        &self,
        _template: TemplateId,
        _method: String,
        _args: VecDeque<Vec<u8>>,
        // caller: InstructionCaller,
    ) -> Result<(), DigitalAssetError> {
        todo!()
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
