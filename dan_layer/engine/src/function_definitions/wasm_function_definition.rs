// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use serde::{Deserialize, Serialize};

use crate::function_definitions::FunctionArgDefinition;

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct WasmFunctionDefinition {
    pub name: String,
    pub args: Vec<FunctionArgDefinition>,
    pub in_module: String,
}
