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

use std::{collections::HashMap, convert::TryInto, fs};

use d3ne::{engine::Engine, node::Node, workers::Workers};
use prost::bytes::Buf;
use serde_json::Value as JsValue;
use tari_core::transactions::transaction_components::TemplateParameter;
use tari_dan_common_types::proto::tips;
use wasmer::{imports, Instance, Module, Store, Type, Val, Value, WasmPtr};

use crate::{
    digital_assets_error::DigitalAssetError,
    models::{ArgType, AssetDefinition, FlowFunctionDef, FlowNodeDef, Instruction, InstructionSet, TemplateId},
    services::{infrastructure_services::NodeAddressable, CommitteeManager},
    storage::state::{StateDbUnitOfWork, StateDbUnitOfWorkReader},
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

pub struct WasmModule {}

impl WasmModule {}

fn load_workers() -> Workers {
    let mut workers = Workers::new();
    workers
}

#[derive(Clone)]
pub struct FunctionInterface {}

impl FunctionInterface {
    fn find_executor(&self, instruction: &Instruction) -> Result<InstructionExecutor, DigitalAssetError> {
        match instruction.template_id() {
            TemplateId::Tip6000 => {
                // let req: tips::Tip6000::InvokeWasmRequest::decode(instruction.args())?;
                Ok(InstructionExecutor::WasmModule {
                    name: instruction.method().to_string(),
                })
            },
            TemplateId::Tip7000 => Ok(InstructionExecutor::Flow {
                name: instruction.method().to_string(),
            }),
            _ => Ok(InstructionExecutor::Template {
                template_id: instruction.template_id(),
            }),
        }
    }
}

pub enum InstructionExecutor {
    WasmModule { name: String },
    Template { template_id: TemplateId },
    Flow { name: String },
}

// fn find_node_by_name(func_def: &FlowFunctionDef, name: &str) -> Result<FlowNodeDef, DigitalAssetError> {
//     for n in func_def.flow.nodes.values() {
//         if n.title == name {
//             return Ok(n.clone());
//         }
//     }
//     panic!("could not find node")
// }

#[derive(Clone, Debug)]
pub struct FlowInstance {
    // engine: Engine,
    // TODO: engine is not Send so can't be added here
    // process: JsValue,
    start_node: i64,
    nodes: HashMap<i64, Node>,
}

impl FlowInstance {
    pub fn try_build(value: JsValue, workers: Workers) -> Result<Self, DigitalAssetError> {
        let engine = Engine::new("tari@0.1.0", workers);
        dbg!(&value);
        let nodes = engine.parse_value(value.clone()).expect("could not create engine");
        Ok(FlowInstance {
            // process: value,
            nodes,
            start_node: 1,
        })
    }

    pub fn process(&self, args: &[u8]) -> Result<(), DigitalAssetError> {
        let engine = Engine::new("tari@0.1.0", load_workers());
        let output = engine.process(&self.nodes, self.start_node);
        dbg!(&output);
        let od = output.expect("engine process failed");
        Ok(())
    }
}

#[derive(Clone)]
pub struct FlowFactory {
    flows: HashMap<String, (Vec<ArgType>, FlowInstance)>,
}
impl FlowFactory {
    pub fn new(asset_definition: &AssetDefinition) -> Self {
        // let workers = load_workers();
        let mut flows = HashMap::new();
        for func_def in &asset_definition.flow_functions {
            // build_instance(&mut instance, &func_def);
            flows.insert(
                func_def.name.clone(),
                (
                    func_def.args.iter().map(|at| at.arg_type.clone()).collect(),
                    FlowInstance::try_build(func_def.flow.clone(), load_workers()).expect("Could not build flow"),
                ),
            );
        }
        Self { flows }
    }

    pub fn invoke_write_method<TUnitOfWork: StateDbUnitOfWork>(
        &self,
        name: String,
        instruction: &Instruction,
        state_db: &mut TUnitOfWork,
    ) -> Result<(), DigitalAssetError> {
        dbg!("INvoke write");
        dbg!(&self.flows);
        dbg!(&name);
        if let Some((_args, engine)) = self.flows.get(&name) {
            engine.process(instruction.args())
        } else {
            todo!("could not find engine")
        }
    }
}
#[derive(Clone)]
pub struct WasmModuleFactory {
    modules: HashMap<String, Instance>,
    functions: HashMap<String, (Vec<ArgType>, String)>,
}

mod wasm_funcs {
    pub fn create_bucket() -> u32 {
        1
    }
}

impl WasmModuleFactory {
    pub fn new(asset_definition: &AssetDefinition) -> Self {
        let mut modules = HashMap::new();
        for mod_def in &asset_definition.wasm_modules {
            let store = Store::default();
            let file = fs::read(&mod_def.path).expect("could not read all bytes");
            let module = Module::new(&store, file).expect("Did not compile");
            let import_object = imports! {}; // <- SDK for interacting with block chain
            let declared_imps: Vec<_> = module.imports().functions().collect();
            dbg!(declared_imps);
            // TODO: Does wasm code auto run at this point
            let instance = Instance::new(&module, &import_object).expect("Could not create instance");
            modules.insert(mod_def.name.clone(), instance);
        }
        let mut functions = HashMap::new();
        for func_def in &asset_definition.wasm_functions {
            if let Some(instance) = modules.get(&func_def.in_module) {
                let function = instance.exports.get_function(&func_def.name).unwrap();
                dbg!(function);
                // todo: check that imported function is actually present in wasm

                functions.insert(
                    func_def.name.clone(),
                    (
                        func_def.args.iter().map(|at| at.arg_type.clone()).collect(),
                        func_def.in_module.clone(),
                    ),
                );
            } else {
                panic!("module {} does not exist", func_def.in_module)
            }
        }
        Self { modules, functions }
    }

    pub fn invoke_write_method<TUnitOfWork: StateDbUnitOfWork>(
        &self,
        name: String,
        instruction: &Instruction,
        state_db: &mut TUnitOfWork,
    ) -> Result<(), DigitalAssetError> {
        dbg!(&self.functions);
        // TODO: We should probably create a new instance each time, so that
        // there's no stale memory
        if let Some((arg_types, module_name)) = self.functions.get(&name) {
            if let Some(instance) = self.modules.get(module_name) {
                let func_pointer = instance.exports.get_function(&name).expect("Could not find function");
                let type_params = func_pointer.ty().params();
                let mut remaining_args = instruction.args().clone();
                // dbg!(&remaining_args);
                // let memory = instance.get_memory("mem");
                let mut offset = 0;
                // TODO: better iteration
                let mut remaining_instruction_args = Vec::from(instruction.args());
                let args: Vec<Vec<Val>> = arg_types
                    .iter()
                    .enumerate()
                    .map(|(position, param)| {
                        match param {
                            ArgType::String => {
                                // if remaining_args.len() < 3 {
                                //     return Err(DigitalAssetError::MissingArgument {
                                //         position,
                                //         argument_name: "Wasm string".to_string(),
                                //     });
                                // }
                                //
                                // let len = remaining_instruction_args.pop().expect("can't take length") as usize;
                                // let instruction_arg =
                                //     String::from_utf8(remaining_instruction_args.drain(len)).expect("invalid utf8");
                                // let ptr = WasmPtr::<String>::new(offset);
                                // let derefed = ptr.deref(&memory).expect("could not get derefed pointer");
                                // derefed.set(instruction_arg);
                                //
                                // Ok(vec![Value::I32()])
                                todo!()
                            },
                            ArgType::Byte => {
                                if remaining_instruction_args.len() < 1 {
                                    return Err(DigitalAssetError::MissingArgument {
                                        position,
                                        argument_name: "Wasm byte".to_string(),
                                    });
                                }
                                let byte = remaining_instruction_args.pop().expect("not enough length");
                                Ok(vec![Value::I32(byte as i32)])
                            },
                            ArgType::PublicKey => {
                                if remaining_instruction_args.len() < 32 {
                                    return Err(DigitalAssetError::MissingArgument {
                                        position,
                                        argument_name: "Wasm public key".to_string(),
                                    });
                                }
                                let bytes: Vec<u8> = remaining_instruction_args.drain(..32).collect();
                                let mut result = Vec::with_capacity(8);
                                for i in 0..8 {
                                    let mut data = [0u8; 4];
                                    data.copy_from_slice(&bytes[i * 4..i * 4 + 4]);
                                    result.push(Value::I32(i32::from_le_bytes(data)));
                                }
                                // write as 8 * bytes
                                Ok(result)
                            },
                            // F32,
                            // F64,
                            // V128,
                            // ExternRef,
                            // FuncRef,
                            _ => {
                                todo!()
                            },
                        }
                    })
                    .collect::<Result<_, _>>()?;

                let args: Vec<Val> = args.into_iter().flatten().collect();
                let result = func_pointer.call(args.as_slice()).expect("invokation error");
                dbg!(&result);
                Ok(())
            } else {
                todo!("No module found")
            }
        } else {
            todo!("function not found")
        }
        // let store = Store::default();
        // let module = Module::new(&store, wat_file.as_str());
        // let import_object = imports! {};
        // let instance = Instance::new(&module, &import_object)?;
    }
}

#[derive(Clone)]
pub struct ConcreteAssetProcessor {
    asset_definition: AssetDefinition,
    template_factory: TemplateFactory,
    wasm_factory: WasmModuleFactory,
    function_interface: FunctionInterface,
    flow_factory: FlowFactory,
}

impl ConcreteAssetProcessor {
    pub fn new(asset_definition: AssetDefinition) -> Self {
        Self {
            wasm_factory: WasmModuleFactory::new(&asset_definition),
            flow_factory: FlowFactory::new(&asset_definition),
            asset_definition,
            template_factory: Default::default(),
            function_interface: FunctionInterface {},
        }
    }
}

impl AssetProcessor for ConcreteAssetProcessor {
    fn execute_instruction<TUnitOfWork: StateDbUnitOfWork>(
        &self,
        instruction: &Instruction,
        state_db: &mut TUnitOfWork,
    ) -> Result<(), DigitalAssetError> {
        match self.function_interface.find_executor(instruction)? {
            InstructionExecutor::WasmModule { name } => {
                self.wasm_factory.invoke_write_method(name, instruction, state_db)
            },
            InstructionExecutor::Template { .. } => self.template_factory.invoke_write_method(instruction, state_db),
            InstructionExecutor::Flow { name } => self.flow_factory.invoke_write_method(name, instruction, state_db),
        }
    }

    fn invoke_read_method<TUnitOfWork: StateDbUnitOfWorkReader>(
        &self,
        instruction: &Instruction,
        state_db: &TUnitOfWork,
    ) -> Result<Option<Vec<u8>>, DigitalAssetError> {
        self.template_factory.invoke_read_method(instruction, state_db)
    }
}

#[derive(Default, Clone)]
pub struct TemplateFactory {}

impl TemplateFactory {
    pub fn initial_instructions(&self, template_param: &TemplateParameter) -> InstructionSet {
        use TemplateId::{Tip002, Tip003, Tip004, Tip721};
        // TODO: We may want to use the TemplateId type, so that we know it is known/valid
        let template_id = template_param.template_id.try_into().unwrap();
        match template_id {
            Tip002 => tip002_template::initial_instructions(template_param),
            Tip003 => todo!(),
            Tip004 => tip004_template::initial_instructions(template_param),
            Tip721 => tip721_template::initial_instructions(template_param),
            Tip6000 => InstructionSet::empty(),
            _ => todo!(),
        }
    }

    pub fn invoke_read_method<TUnitOfWork: StateDbUnitOfWorkReader>(
        &self,
        instruction: &Instruction,
        state_db: &TUnitOfWork,
    ) -> Result<Option<Vec<u8>>, DigitalAssetError> {
        use TemplateId::{Tip002, Tip003, Tip004, Tip721};
        match instruction.template_id() {
            Tip002 => tip002_template::invoke_read_method(instruction.method(), instruction.args(), state_db),
            Tip003 => todo!(),
            Tip004 => tip004_template::invoke_read_method(instruction.method(), instruction.args(), state_db),
            Tip721 => tip721_template::invoke_read_method(instruction.method(), instruction.args(), state_db),
            _ => {
                todo!()
            },
        }
    }

    pub fn invoke_write_method<TUnitOfWork: StateDbUnitOfWork>(
        &self,
        instruction: &Instruction,
        state_db: &mut TUnitOfWork,
    ) -> Result<(), DigitalAssetError> {
        use TemplateId::{Tip002, Tip003, Tip004, Tip721};
        match instruction.template_id() {
            Tip002 => tip002_template::invoke_write_method(instruction.method(), instruction.args(), state_db),
            Tip003 => todo!(),
            Tip004 => tip004_template::invoke_write_method(instruction.method(), instruction.args(), state_db),
            Tip721 => tip721_template::invoke_write_method(instruction.method(), instruction.args(), state_db),
            _ => {
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
