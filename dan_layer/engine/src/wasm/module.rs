//  Copyright 2022. The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use tari_template_abi::{FunctionDef, TemplateDef};
use wasmer::{Extern, Function, Instance, Module, Store, Val, WasmerEnv};

use crate::{
    packager::{PackageError, PackageModuleLoader},
    wasm::{environment::WasmEnv, WasmExecutionError},
};

#[derive(Debug, Clone)]
pub struct WasmModule {
    code: Vec<u8>,
}

impl WasmModule {
    pub fn from_code(code: Vec<u8>) -> Self {
        Self { code }
    }

    pub fn code(&self) -> &[u8] {
        &self.code
    }
}

impl PackageModuleLoader for WasmModule {
    type Error = PackageError;
    type Loaded = LoadedWasmModule;

    fn load_module(&self) -> Result<Self::Loaded, Self::Error> {
        let store = Store::default();
        let module = Module::new(&store, &self.code)?;
        let violation_flag = Arc::new(AtomicBool::new(false));
        let mut env = WasmEnv::new(violation_flag.clone());

        fn stub(env: &WasmEnv<Arc<AtomicBool>>, _op: i32, _arg_ptr: i32, _arg_len: i32) -> i32 {
            env.state().store(true, Ordering::Relaxed);
            0
        }

        let stub = Function::new_native_with_env(&store, env.clone(), stub);
        let imports = env.create_resolver(&store, stub);
        let instance = Instance::new(&module, &imports)?;
        env.init_with_instance(&instance)?;
        validate_instance(&instance)?;
        validate_environment(&env)?;

        let template = initialize_and_load_template_abi(&instance, &env)?;
        if violation_flag.load(Ordering::Relaxed) {
            return Err(PackageError::TemplateCalledEngineDuringInitialization);
        }
        Ok(LoadedWasmModule::new(template, module))
    }
}

fn initialize_and_load_template_abi(
    instance: &Instance,
    env: &WasmEnv<Arc<AtomicBool>>,
) -> Result<TemplateDef, WasmExecutionError> {
    let abi_func = instance
        .exports
        .iter()
        .find_map(|(name, export)| match export {
            Extern::Function(f) if name.ends_with("_abi") && f.param_arity() == 0 && f.result_arity() == 1 => Some(f),
            _ => None,
        })
        .ok_or(WasmExecutionError::NoAbiDefinition)?;

    // Initialize ABI memory
    let ret = abi_func.call(&[])?;
    let ptr = match ret.get(0) {
        Some(Val::I32(ptr)) => *ptr as u32,
        Some(_) | None => return Err(WasmExecutionError::InvalidReturnTypeFromAbiFunc),
    };

    // Load ABI from memory
    let data = env.read_memory_with_embedded_len(ptr)?;
    let decoded = tari_template_abi::decode(&data).map_err(|_| WasmExecutionError::AbiDecodeError)?;
    Ok(decoded)
}

#[derive(Debug, Clone)]
pub struct LoadedWasmModule {
    template: TemplateDef,
    module: wasmer::Module,
}

impl LoadedWasmModule {
    pub fn new(template: TemplateDef, module: wasmer::Module) -> Self {
        Self { template, module }
    }

    pub fn wasm_module(&self) -> &wasmer::Module {
        &self.module
    }

    pub fn template_name(&self) -> &str {
        &self.template.template_name
    }

    pub fn template_def(&self) -> &TemplateDef {
        &self.template
    }

    pub fn find_func_by_name(&self, function_name: &str) -> Option<&FunctionDef> {
        self.template.functions.iter().find(|f| f.name == *function_name)
    }
}

fn validate_environment(env: &WasmEnv<Arc<AtomicBool>>) -> Result<(), WasmExecutionError> {
    const MAX_MEM_SIZE: usize = 2 * 1024 * 1024;
    let mem_size = env.mem_size();
    if mem_size.bytes().0 > MAX_MEM_SIZE {
        return Err(WasmExecutionError::MaxMemorySizeExceeded);
    }

    Ok(())
}

fn validate_instance(instance: &Instance) -> Result<(), WasmExecutionError> {
    // Enforce that only permitted functions are allowed
    let unexpected_abi_func = instance
        .exports
        .iter()
        .functions()
        .find(|(name, _)| !is_func_permitted(name));
    if let Some((name, _)) = unexpected_abi_func {
        return Err(WasmExecutionError::UnexpectedAbiFunction { name: name.to_string() });
    }

    Ok(())
}

fn is_func_permitted(name: &str) -> bool {
    name.ends_with("_abi") || name.ends_with("_main") || name == "tari_alloc" || name == "tari_free"
}
