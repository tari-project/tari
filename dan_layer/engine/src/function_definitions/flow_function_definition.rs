// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause
use serde::{Deserialize, Serialize};
use serde_json::Value as JsValue;

use crate::function_definitions::FunctionArgDefinition;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FlowFunctionDefinition {
    pub name: String,
    pub args: Vec<FunctionArgDefinition>,
    pub flow: JsValue,
}
