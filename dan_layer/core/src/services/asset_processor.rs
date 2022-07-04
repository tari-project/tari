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

use std::convert::TryInto;

use tari_core::transactions::transaction_components::TemplateParameter;
use tari_dan_engine::{
    flow::FlowFactory,
    instructions::Instruction,
    state::{StateDbUnitOfWork, StateDbUnitOfWorkReader},
    wasm::WasmModuleFactory,
};

use crate::{
    digital_assets_error::DigitalAssetError,
    models::{AssetDefinition, InstructionSet},
    template_command::ExecutionResult,
    templates::{tip002_template, tip004_template, tip721_template},
};

pub trait AssetProcessor: Sync + Send + 'static {
    // purposefully made sync, because instructions should be run in order, and complete before the
    // next one starts. There may be a better way to enforce this though...
    fn execute_instruction<TUnitOfWork: StateDbUnitOfWork>(
        &self,
        instruction: &Instruction,
        db: &mut TUnitOfWork,
    ) -> Result<(), DigitalAssetError>;

    fn invoke_read_method<TUnitOfWorkReader: StateDbUnitOfWorkReader>(
        &self,
        instruction: &Instruction,
        state_db: &TUnitOfWorkReader,
    ) -> Result<Option<Vec<u8>>, DigitalAssetError>;
}

#[derive(Default, Clone)]
pub struct ConcreteAssetProcessor {
    _asset_definition: AssetDefinition,
    template_factory: TemplateFactory,
    _wasm_factory: WasmModuleFactory,
    _function_interface: FunctionInterface,
    _flow_factory: FlowFactory,
}

impl ConcreteAssetProcessor {
    pub fn new(asset_definition: AssetDefinition) -> Self {
        Self {
            _wasm_factory: WasmModuleFactory::new(&asset_definition.wasm_modules, &asset_definition.wasm_functions),
            _flow_factory: FlowFactory::new(&asset_definition.flow_functions),
            _asset_definition: asset_definition,
            template_factory: Default::default(),
            _function_interface: FunctionInterface {},
        }
    }
}

impl AssetProcessor for ConcreteAssetProcessor {
    fn execute_instruction<TUnitOfWork: StateDbUnitOfWork>(
        &self,
        instruction: &Instruction,
        state_db: &mut TUnitOfWork,
    ) -> Result<(), DigitalAssetError> {
        self.template_factory.invoke_write_method(instruction, state_db)
    }

    fn invoke_read_method<TUnitOfWork: StateDbUnitOfWorkReader>(
        &self,
        instruction: &Instruction,
        state_db: &TUnitOfWork,
    ) -> Result<Option<Vec<u8>>, DigitalAssetError> {
        self.template_factory.invoke_read_method(instruction, state_db)
    }
}

#[derive(Clone, Default)]
pub struct FunctionInterface {}

// impl FunctionInterface {
//     #[allow(dead_code)]
//     fn find_executor(&self, instruction: &Instruction) -> Result<InstructionExecutor, DigitalAssetError> {
//         match instruction.template_id() {
//             // TODO: Put these back
//             // TemplateId::Tip6000 => Ok(InstructionExecutor::WasmModule {
//             //     name: instruction.method().to_string(),
//             // }),
//             // TemplateId::Tip7000 => Ok(InstructionExecutor::Flow {
//             //     name: instruction.method().to_string(),
//             // }),
//             _ => Ok(InstructionExecutor::Template {
//                 template_id: instruction.template_id(),
//             }),
//         }
//     }
// }
//
// pub enum InstructionExecutor {
//     WasmModule { name: String },
//     Template { template_id: TemplateId },
//     Flow { name: String },
// }

#[derive(Default, Clone)]
pub struct TemplateFactory {}

impl TemplateFactory {
    pub fn initial_instructions(&self, template_param: &TemplateParameter) -> InstructionSet {
        use tari_dan_common_types::TemplateId::{EditableMetadata, Tip002, Tip003, Tip004, Tip721};
        // TODO: We may want to use the TemplateId type, so that we know it is known/valid
        let template_id = template_param.template_id.try_into().unwrap();
        match template_id {
            Tip002 => tip002_template::initial_instructions(template_param),
            Tip003 => todo!(),
            Tip004 => tip004_template::initial_instructions(template_param),
            Tip721 => tip721_template::initial_instructions(template_param),
            EditableMetadata => {
                todo!()
            },
        }
    }

    pub fn invoke_read_method<TUnitOfWork: StateDbUnitOfWorkReader>(
        &self,
        instruction: &Instruction,
        state_db: &TUnitOfWork,
    ) -> Result<Option<Vec<u8>>, DigitalAssetError> {
        use tari_dan_common_types::TemplateId::{EditableMetadata, Tip002, Tip003, Tip004, Tip721};
        match instruction.template_id() {
            Tip002 => tip002_template::invoke_read_method(instruction.method(), instruction.args(), state_db),
            Tip003 => todo!(),
            Tip004 => tip004_template::invoke_read_method(instruction.method(), instruction.args(), state_db),
            Tip721 => tip721_template::invoke_read_method(instruction.method(), instruction.args(), state_db),
            EditableMetadata => {
                todo!()
            },
        }
    }

    pub fn invoke_write_method<TUnitOfWork: StateDbUnitOfWork>(
        &self,
        instruction: &Instruction,
        state_db: &mut TUnitOfWork,
    ) -> Result<(), DigitalAssetError> {
        use tari_dan_common_types::TemplateId::{EditableMetadata, Tip002, Tip003, Tip004, Tip721};
        match instruction.template_id() {
            Tip002 => tip002_template::invoke_write_method(instruction.method(), instruction.args(), state_db),
            Tip003 => todo!(),
            Tip004 => tip004_template::invoke_write_method(instruction.method(), instruction.args(), state_db),
            Tip721 => tip721_template::invoke_write_method(instruction.method(), instruction.args(), state_db),
            EditableMetadata => {
                todo!()
            },
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
