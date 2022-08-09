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

use std::collections::HashMap;

use digest::Digest;
use rand::{rngs::OsRng, RngCore};
use tari_template_lib::models::PackageId;

use crate::{
    crypto,
    packager::{error::PackageError, PackageModuleLoader},
    wasm::{LoadedWasmModule, WasmModule},
};

#[derive(Debug, Clone)]
pub struct Package {
    id: PackageId,
    wasm_modules: HashMap<String, LoadedWasmModule>,
}

impl Package {
    pub fn builder() -> PackageBuilder {
        PackageBuilder::new()
    }

    pub fn get_module_by_name(&self, name: &str) -> Option<&LoadedWasmModule> {
        self.wasm_modules.get(name)
    }

    pub fn id(&self) -> PackageId {
        self.id
    }
}

#[derive(Debug, Clone, Default)]
pub struct PackageBuilder {
    wasm_modules: Vec<WasmModule>,
}

impl PackageBuilder {
    pub fn new() -> Self {
        Self {
            wasm_modules: Vec::new(),
        }
    }

    pub fn add_wasm_module(&mut self, wasm_module: WasmModule) -> &mut Self {
        self.wasm_modules.push(wasm_module);
        self
    }

    pub fn build(&self) -> Result<Package, PackageError> {
        let mut wasm_modules = HashMap::with_capacity(self.wasm_modules.len());
        let id = new_package_id();
        for wasm in &self.wasm_modules {
            let loaded = wasm.load_module()?;
            wasm_modules.insert(loaded.template_name().to_string(), loaded);
        }

        Ok(Package { id, wasm_modules })
    }
}

fn new_package_id() -> PackageId {
    let v = OsRng.next_u32();
    let hash: [u8; 32] = crypto::hasher("package")
          // TODO: Proper package id
        .chain(&v.to_le_bytes())
        .finalize().into();
    hash.into()
}
