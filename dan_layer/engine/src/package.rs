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

use std::{cell::Cell, collections::HashMap, convert::TryInto};

use rand::{rngs::OsRng, RngCore};
use tari_common_types::types::FixedHash;
use tari_smart_contract_abi::TemplateDef;
use wasmer::{imports, Extern, Function, Instance, Memory, Module, Store, Val};

use crate::{crypto, wasm::LoadedWasmModule};

#[derive(Debug, Clone, Default)]
pub struct PackageBuilder {
    wasm_code: Vec<Vec<u8>>,
}

impl PackageBuilder {
    pub fn new() -> Self {
        Self { wasm_code: Vec::new() }
    }

    pub fn add_wasm_template(&mut self, wasm_code: Vec<u8>) -> &mut Self {
        self.wasm_code.push(wasm_code);
        self
    }

    pub fn build(&self) -> Result<Package, PackageError> {
        let mut wasm_modules = HashMap::with_capacity(self.wasm_code.len());
        let store = Store::default();
        let id = new_package_id();
        for code in &self.wasm_code {
            let module = load_wasm_module(&store, code)?;
            wasm_modules.insert(module.template_name().to_string(), module);
        }

        Ok(Package {
            id,
            wasm_modules,
            _store: store,
        })
    }
}

fn new_package_id() -> PackageId {
    let v = OsRng.next_u32();
    crypto::domain_separated_hasher("package")
        // TODO: Proper package id
        .chain(&v.to_le_bytes())
        .finalize()
        .as_ref()
        .try_into()
        .unwrap()
}

fn load_wasm_module(store: &Store, code: &[u8]) -> Result<LoadedWasmModule, PackageError> {
    let module = Module::new(store, code)?;

    fn stub(_op: i32, _args_ptr: i32, _args_len: i32) -> i32 {
        0
    }

    let imports = imports! {
        "env" => {
            "tari_engine" => Function::new_native(store, stub),
        }
    };
    let instance = Instance::new(&module, &imports)?;
    validate_instance(&instance)?;

    let template = initialize_and_load_template_abi(&instance)?;
    Ok(LoadedWasmModule::new(template, module))
}

fn initialize_and_load_template_abi(instance: &Instance) -> Result<TemplateDef, PackageError> {
    let abi_func = instance
        .exports
        .iter()
        .find_map(|(name, export)| match export {
            Extern::Function(f) if name.ends_with("_abi") => Some(f),
            _ => None,
        })
        .ok_or(PackageError::NoAbiDefinition)?;

    // Initialize ABI memory
    let ret = abi_func.call(&[])?;
    let ptr = match ret.get(0) {
        Some(Val::I32(ptr)) => *ptr as u32,
        Some(_) | None => return Err(PackageError::InvalidReturnTypeFromAbiFunc),
    };

    // Load ABI from memory
    let memory = instance.exports.get_memory("memory")?;
    let data = copy_abi_data_from_memory_checked(memory, ptr)?;
    let decoded = tari_smart_contract_abi::decode(&data).map_err(|_| PackageError::AbiDecodeError)?;
    Ok(decoded)
}

fn copy_abi_data_from_memory_checked(memory: &Memory, ptr: u32) -> Result<Vec<u8>, PackageError> {
    // Check memory bounds
    if memory.data_size() < u64::from(ptr) {
        return Err(PackageError::AbiPointerOutOfBounds);
    }

    let view = memory.uint8view().subarray(ptr, memory.data_size() as u32 - 1);
    let data = &*view;
    if data.len() < 4 {
        return Err(PackageError::MemoryUnderflow {
            required: 4,
            remaining: data.len(),
        });
    }

    fn copy_from_cell_slice(src: &[Cell<u8>], dest: &mut [u8], len: usize) {
        for i in 0..len {
            dest[i] = src[i].get();
        }
    }

    let mut buf = [0u8; 4];
    copy_from_cell_slice(data, &mut buf, 4);
    let len = u32::from_le_bytes(buf) as usize;
    const MAX_ABI_DATA_LEN: usize = 1024 * 1024;
    if len > MAX_ABI_DATA_LEN {
        return Err(PackageError::AbiDataTooLarge {
            max: MAX_ABI_DATA_LEN,
            size: len,
        });
    }
    if data.len() < 4 + len {
        return Err(PackageError::MemoryUnderflow {
            required: 4 + len,
            remaining: data.len(),
        });
    }

    let mut data = vec![0u8; len];
    let src = view.subarray(4, 4 + len as u32);
    copy_from_cell_slice(&*src, &mut data, len);
    Ok(data)
}

pub fn validate_instance(instance: &Instance) -> Result<(), PackageError> {
    if let Ok(mem) = instance.exports.get_memory("memory") {
        if mem.size().bytes().0 > 2 * 1024 * 1024 {
            return Err(PackageError::MaxMemorySizeExceeded);
        }
    }
    // TODO other package validations

    Ok(())
}

pub type PackageId = FixedHash;

#[derive(Debug, Clone)]
pub struct Package {
    id: PackageId,
    wasm_modules: HashMap<String, LoadedWasmModule>,
    _store: Store,
}

impl Package {
    pub fn get_module_by_name(&self, name: &str) -> Option<&LoadedWasmModule> {
        self.wasm_modules.get(name)
    }

    pub fn id(&self) -> PackageId {
        self.id
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PackageError {
    #[error(transparent)]
    CompileError(#[from] wasmer::CompileError),
    #[error(transparent)]
    InstantiationError(#[from] wasmer::InstantiationError),
    #[error(transparent)]
    RuntimeError(#[from] wasmer::RuntimeError),
    #[error(transparent)]
    ExportError(#[from] wasmer::ExportError),
    #[error("Failed to decode ABI")]
    AbiDecodeError,
    #[error("maximum module memory size exceeded")]
    MaxMemorySizeExceeded,
    #[error("package did not contain an ABI definition")]
    NoAbiDefinition,
    #[error("package ABI function returned an invalid type")]
    InvalidReturnTypeFromAbiFunc,
    #[error("package ABI function returned an out of bounds pointer")]
    AbiPointerOutOfBounds,
    #[error("memory underflow: {required} bytes required but {remaining} remaining")]
    MemoryUnderflow { required: usize, remaining: usize },
    #[error("ABI data is too large: a maximum of {max} bytes allowed but size is {size}")]
    AbiDataTooLarge { max: usize, size: usize },
}
