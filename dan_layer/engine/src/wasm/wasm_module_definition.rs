// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct WasmModuleDefinition {
    pub name: String,
    pub path: PathBuf,
}
