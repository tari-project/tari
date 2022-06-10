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

use std::{collections::HashMap, fmt, marker::PhantomData, path::PathBuf};

use serde::{self, de, Deserialize, Deserializer, Serialize};
use serde_json::Value as JsValue;
use tari_common_types::types::{PublicKey, ASSET_CHECKPOINT_ID};
use tari_core::transactions::transaction_components::TemplateParameter;
use tari_utilities::hex::Hex;

#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct AssetDefinition {
    #[serde(deserialize_with = "AssetDefinition::deserialize_pub_key_from_hex")]
    pub public_key: PublicKey,
    // TODO: remove and read from base layer
    // pub committee: Vec<String>,
    pub phase_timeout: u64,
    // TODO: Better name? lock time/peg time? (in number of blocks)
    pub base_layer_confirmation_time: u64,
    pub checkpoint_unique_id: Vec<u8>,
    pub initial_state: InitialState,
    pub template_parameters: Vec<TemplateParameter>,
    pub wasm_modules: Vec<WasmModuleDef>,
    pub wasm_functions: Vec<WasmFunctionDef>,
    pub flow_functions: Vec<FlowFunctionDef>,
}

impl Default for AssetDefinition {
    fn default() -> Self {
        Self {
            base_layer_confirmation_time: 5,
            checkpoint_unique_id: ASSET_CHECKPOINT_ID.into(),
            public_key: Default::default(),
            // committee: vec![],
            phase_timeout: 1,
            initial_state: Default::default(),
            template_parameters: vec![],
            wasm_modules: vec![],
            wasm_functions: vec![],
            flow_functions: vec![],
        }
    }
}

impl AssetDefinition {
    pub fn deserialize_pub_key_from_hex<'de, D>(des: D) -> Result<PublicKey, D::Error>
    where D: Deserializer<'de> {
        struct KeyStringVisitor<K> {
            marker: PhantomData<K>,
        }

        impl<'de> de::Visitor<'de> for KeyStringVisitor<PublicKey> {
            type Value = PublicKey;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a public key in hex format")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where E: de::Error {
                PublicKey::from_hex(v).map_err(E::custom)
            }
        }

        des.deserialize_str(KeyStringVisitor { marker: PhantomData })
    }

    pub fn initial_state(&self) -> &InitialState {
        &self.initial_state
    }
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct WasmModuleDef {
    pub name: String,
    pub path: PathBuf,
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct WasmFunctionDef {
    pub name: String,
    pub args: Vec<WasmFunctionArgDef>,
    pub in_module: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FlowFunctionDef {
    pub name: String,
    pub args: Vec<WasmFunctionArgDef>,
    pub flow: JsValue,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FlowDef {
    pub nodes: HashMap<String, FlowNodeDef>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FlowNodeDef {
    pub id: u32,
    pub title: String,
    pub data: HashMap<String, String>,
    pub inputs: Vec<FlowInputConnectionsDef>,
    pub outputs: Vec<FlowOutputConnectionsDef>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FlowInputConnectionsDef {
    pub connections: Vec<FlowInputConnectionDef>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FlowInputConnectionDef {
    pub node: u32,
    pub output: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FlowOutputConnectionsDef {
    pub connections: Vec<FlowOutputConnectionDef>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FlowOutputConnectionDef {
    pub node: u32,
    pub input: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WasmFunctionArgDef {
    pub name: String,
    #[serde(rename = "type")]
    pub arg_type: ArgType,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ArgType {
    String,
    Byte,
    PublicKey,
    Uint,
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct InitialState {
    pub schemas: Vec<SchemaState>,
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct SchemaState {
    pub name: String,
    pub items: Vec<KeyValue>,
}

impl SchemaState {
    pub fn new(name: String, items: Vec<KeyValue>) -> Self {
        Self { name, items }
    }

    pub fn push_key_value(&mut self, key_value: KeyValue) -> &mut Self {
        self.items.push(key_value);
        self
    }
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct KeyValue {
    pub key: Vec<u8>,
    pub value: Vec<u8>,
}
