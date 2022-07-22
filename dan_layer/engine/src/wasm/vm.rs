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

use std::cell::Cell;

use wasmer::{imports, Function, Instance, Memory, Module, Store, Val};

use crate::{
    env::{tari_engine, EngineEnvironment},
    wasm::error::WasmExecutionError,
};

#[derive(Debug)]
pub struct VmInstance {
    memory: Memory,
    instance: Instance,
    _store: Store,
}

impl VmInstance {
    pub fn instantiate(module: &Module) -> Result<Self, WasmExecutionError> {
        let store = Store::default();
        // TODO: proper environment
        let env = EngineEnvironment::default();
        let imports = imports! {
            "env" => {
                "tari_engine" => Function::new_native_with_env(&store, env, tari_engine),
            }
        };
        let instance = Instance::new(module, &imports)?;
        let memory = instance.exports.get_memory("memory")?;
        Ok(Self {
            memory: memory.clone(),
            _store: store,
            instance,
        })
    }

    pub(super) fn alloc(&self, len: u32) -> Result<AllocPtr, WasmExecutionError> {
        let alloc = self.instance.exports.get_function("tari_alloc")?;
        let ret = alloc.call(&[Val::I32(len as i32)])?;
        match ret.get(0) {
            Some(Val::I32(ptr)) => Ok(AllocPtr(*ptr as u32, len)),
            _ => Err(WasmExecutionError::ExpectedPointerReturn {
                function: "tari_alloc".into(),
            }),
        }
    }

    pub(super) fn free(&self, ptr: AllocPtr) -> Result<(), WasmExecutionError> {
        let alloc = self.instance.exports.get_function("tari_free")?;
        alloc.call(&[ptr.as_val_i32()])?;
        Ok(())
    }

    pub(super) fn write_to_memory(&self, ptr: &AllocPtr, data: &[u8]) -> Result<(), WasmExecutionError> {
        if data.len() != ptr.len() as usize {
            return Err(WasmExecutionError::InvalidWriteLength {
                allocated: ptr.len(),
                requested: data.len() as u32,
            });
        }
        // SAFETY: The VM owns the only memory instance, and the pointer has been allocated by alloc above so data races
        // are not possible.
        unsafe {
            self.memory.uint8view().subarray(ptr.get(), ptr.end()).copy_from(data);
        }
        Ok(())
    }

    pub(super) fn read_from_memory(&self, ptr: u32) -> Result<Vec<u8>, WasmExecutionError> {
        // TODO: DRY this up
        let view = self
            .memory
            .uint8view()
            .subarray(ptr, self.memory.data_size() as u32 - 1);
        let view_bytes = &*view;
        if view_bytes.len() < 4 {
            return Err(WasmExecutionError::MemoryUnderflow {
                required: 4,
                remaining: view_bytes.len(),
            });
        }

        fn copy_from_cell_slice(src: &[Cell<u8>], dest: &mut [u8], len: usize) {
            // TODO: Is there a more efficient way to do this?
            for i in 0..len {
                dest[i] = src[i].get();
            }
        }

        let mut buf = [0u8; 4];
        copy_from_cell_slice(view_bytes, &mut buf, 4);
        let len = u32::from_le_bytes(buf) as usize;
        if view_bytes.len() < 4 + len {
            return Err(WasmExecutionError::MemoryUnderflow {
                required: 4 + len,
                remaining: view_bytes.len(),
            });
        }

        let mut data = vec![0u8; len];
        let src = view.subarray(4, 4 + len as u32);
        copy_from_cell_slice(&*src, &mut data, len);
        Ok(data)
    }

    pub fn get_function(&self, name: &str) -> Result<&Function, WasmExecutionError> {
        let func = self.instance.exports.get_function(name)?;
        Ok(func)
    }
}

#[derive(Debug)]
pub struct AllocPtr(u32, u32);

impl AllocPtr {
    pub fn get(&self) -> u32 {
        self.0
    }

    pub fn len(&self) -> u32 {
        self.1
    }

    pub fn end(&self) -> u32 {
        self.get() + self.len()
    }

    pub fn as_val_i32(&self) -> Val {
        // We want the 'u32 as i32' conversion to wrap
        Val::I32(self.get() as i32)
    }
}
