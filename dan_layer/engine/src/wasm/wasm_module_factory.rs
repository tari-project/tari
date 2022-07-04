// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{collections::HashMap, fs};

use wasmer::{imports, Instance, Module, Store, Val, Value};

use crate::{
    function_definitions::{ArgType, WasmFunctionDefinition},
    instructions::Instruction,
    state::StateDbUnitOfWork,
    wasm::{WasmError, WasmModuleDefinition},
};

#[derive(Clone, Default)]
pub struct WasmModuleFactory {
    modules: HashMap<String, Instance>,
    functions: HashMap<String, (Vec<ArgType>, String)>,
}

impl WasmModuleFactory {
    pub fn new(wasm_modules: &[WasmModuleDefinition], wasm_functions: &[WasmFunctionDefinition]) -> Self {
        let mut modules = HashMap::new();
        for mod_def in wasm_modules {
            let store = Store::default();
            let file = fs::read(&mod_def.path).expect("could not read all bytes");
            let module = Module::new(&store, file).expect("Did not compile");
            let import_object = imports! {}; // <- SDK for interacting with block chain
            let _declared_imps: Vec<_> = module.imports().functions().collect();
            // TODO: Does wasm code auto run at this point
            let instance = Instance::new(&module, &import_object).expect("Could not create instance");
            modules.insert(mod_def.name.clone(), instance);
        }
        let mut functions = HashMap::new();
        for func_def in wasm_functions {
            if let Some(instance) = modules.get(&func_def.in_module) {
                // check that imported function is actually present in wasm
                let _function = instance.exports.get_function(&func_def.name).unwrap();

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
        state_db: TUnitOfWork,
    ) -> Result<TUnitOfWork, WasmError> {
        // TODO: We should probably create a new instance each time, so that
        // there's no stale memory
        if let Some((arg_types, module_name)) = self.functions.get(&name) {
            if let Some(instance) = self.modules.get(module_name) {
                let func_pointer = instance.exports.get_function(&name).expect("Could not find function");
                let _type_params = func_pointer.ty().params();
                let _remaining_args = Vec::from(instruction.args());
                // dbg!(&remaining_args);
                // let memory = instance.get_memory("mem");
                let _offset = 0;
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
                                if remaining_instruction_args.is_empty() {
                                    return Err(WasmError::MissingArgument {
                                        position,
                                        argument_name: "Wasm byte".to_string(),
                                    });
                                }
                                let byte = remaining_instruction_args.pop().expect("not enough length");
                                Ok(vec![Value::I32(i32::from(byte))])
                            },
                            ArgType::PublicKey => {
                                if remaining_instruction_args.len() < 32 {
                                    return Err(WasmError::MissingArgument {
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
                let _result = func_pointer.call(args.as_slice()).expect("invokation error");
                Ok(state_db)
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
