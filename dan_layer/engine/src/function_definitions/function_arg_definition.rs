// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FunctionArgDefinition {
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
