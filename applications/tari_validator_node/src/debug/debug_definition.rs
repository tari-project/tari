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

use std::{fs, path::PathBuf};

use prost::Message;
use serde::{Deserialize, Serialize};
use tari_common::configuration::CommonConfig;
use tari_common_types::types::PublicKey;
use tari_core::transactions::transaction_components::TemplateParameter;
use tari_dan_common_types::proto::tips::tip6000;
use tari_dan_core::{
    models::{ArgType, FlowFunctionDef, WasmFunctionArgDef},
    DigitalAssetError,
};

#[derive(Serialize, Deserialize)]
pub struct DebugDefinition {
    pub contract_name: String,
    pub committee: Vec<PublicKey>,
    // TODO: change to contract od
    pub public_key: PublicKey,
    pub initialization: Vec<InitializationDef>,
    pub wasm_modules: Vec<WasmModuleDef>,
    pub functions: Vec<FunctionDef>,
}

impl DebugDefinition {
    pub fn get_template_parameters(&self, config: &CommonConfig) -> Result<Vec<TemplateParameter>, DigitalAssetError> {
        let mut result = vec![];
        // for def in &self.initialization {
        //     match def {
        //         InitializationDef::Wasm { wat_path } => {
        //             let wat_path = if wat_path.is_absolute() {
        //                 wat_path.to_path_buf()
        //             } else {
        //                 config.base_path().join(wat_path)
        //             };
        //             let wat_file = fs::read_to_string(&wat_path).expect("Can't read file");
        //             // let store = Store::default();
        //             // let module = Module::new(&store, wat_file.as_str());
        //             // let import_object = imports! {};
        //             // let instance = Instance::new(&module, &import_object)?;
        //
        //             let data = tip6000::InitRequest { wat: wat_file }.encode_to_vec();
        //
        //             result.push(TemplateParameter {
        //                 template_id: 6000,
        //                 template_data_version: 0,
        //                 template_data: data,
        //             });
        //         },
        //     }
        // }
        Ok(result)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum InitializationDef {
    Wasm { wat_path: PathBuf },
}

#[derive(Serialize, Deserialize)]
pub struct FunctionDef {
    // pub template_id: String,
    pub name: String,
    pub in_module: Option<String>,
    pub args: Vec<WasmFunctionArgDef>,
    pub flow: Option<FlowFunctionDef>,
}

#[derive(Serialize, Deserialize)]
pub struct WasmModuleDef {
    pub name: String,
    pub path: PathBuf,
}
