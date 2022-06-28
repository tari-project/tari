//  Copyright 2021. The Tari Project
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

use serde::{self, Deserialize, Serialize};
use tari_common_types::types::FixedHash;
use tari_core::transactions::transaction_components::TemplateParameter;
use tari_dan_engine::{
    function_definitions::{FlowFunctionDefinition, WasmFunctionDefinition},
    state::models::SchemaState,
    wasm::WasmModuleDefinition,
};

use crate::helpers::deserialize_from_hex;

#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct AssetDefinition {
    #[serde(deserialize_with = "deserialize_from_hex")]
    pub contract_id: FixedHash,
    // TODO: remove and read from base layer
    pub committee: Vec<String>,
    pub phase_timeout: u64,
    // TODO: Better name? lock time/peg time? (in number of blocks)
    pub base_layer_confirmation_time: u64,
    // TODO: remove
    pub checkpoint_unique_id: Vec<u8>,
    pub initial_state: InitialState,
    pub template_parameters: Vec<TemplateParameter>,
    pub wasm_modules: Vec<WasmModuleDefinition>,
    pub wasm_functions: Vec<WasmFunctionDefinition>,
    pub flow_functions: Vec<FlowFunctionDefinition>,
}

impl Default for AssetDefinition {
    fn default() -> Self {
        Self {
            base_layer_confirmation_time: 5,
            checkpoint_unique_id: vec![],
            contract_id: Default::default(),
            committee: vec![],
            phase_timeout: 30,
            initial_state: Default::default(),
            template_parameters: vec![],
            wasm_modules: vec![],
            wasm_functions: vec![],
            flow_functions: vec![],
        }
    }
}

impl AssetDefinition {
    pub fn initial_state(&self) -> &InitialState {
        &self.initial_state
    }
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct InitialState {
    pub schemas: Vec<SchemaState>,
}
