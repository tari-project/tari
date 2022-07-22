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

use std::io;

use borsh::{BorshDeserialize, BorshSerialize};
use tari_template_abi::{encode_into, CallInfo};
use wasmer::{Module, Val};

use crate::{
    traits::Invokable,
    wasm::{
        error::WasmExecutionError,
        vm::{AllocPtr, VmInstance},
        LoadedWasmModule,
    },
};

#[derive(Debug)]
pub struct Process {
    module: LoadedWasmModule,
    vm: VmInstance,
}

pub struct ExecutionResult {
    pub value: wasmer::Value,
    pub raw: Vec<u8>,
}

impl ExecutionResult {
    pub fn decode<T: BorshDeserialize>(&self) -> io::Result<T> {
        tari_template_abi::decode(&self.raw)
    }
}

impl Process {
    pub fn new(module: LoadedWasmModule, vm: VmInstance) -> Self {
        Self { module, vm }
    }

    fn alloc_and_write<T: BorshSerialize>(&self, val: &T) -> Result<AllocPtr, WasmExecutionError> {
        let mut buf = Vec::with_capacity(512);
        encode_into(val, &mut buf).unwrap();
        let ptr = self.vm.alloc(buf.len() as u32)?;
        self.vm.write_to_memory(&ptr, &buf)?;

        Ok(ptr)
    }

    pub fn wasm_module(&self) -> &Module {
        self.module.wasm_module()
    }
}

impl Invokable for Process {
    type Error = WasmExecutionError;

    fn invoke_by_name(&self, name: &str, args: Vec<Vec<u8>>) -> Result<ExecutionResult, Self::Error> {
        let func_def = self
            .module
            .find_func_by_name(name)
            .ok_or_else(|| WasmExecutionError::FunctionNotFound { name: name.into() })?;

        let call_info = CallInfo {
            func_name: func_def.name.clone(),
            args,
        };

        let main_name = format!("{}_main", self.module.template_name());
        let func = self.vm.get_function(&main_name)?;

        let call_info_ptr = self.alloc_and_write(&call_info)?;
        let res = func.call(&[call_info_ptr.as_val_i32(), Val::I32(call_info_ptr.len() as i32)])?;
        self.vm.free(call_info_ptr)?;
        let ptr = res
            .get(0)
            .and_then(|v| v.i32())
            .ok_or(WasmExecutionError::ExpectedPointerReturn { function: main_name })?;

        // Read response from memory
        let raw = self.vm.read_from_memory(ptr as u32)?;

        // TODO: decode raw as per function def
        Ok(ExecutionResult {
            value: wasmer::Value::I32(ptr),
            raw,
        })
    }
}
