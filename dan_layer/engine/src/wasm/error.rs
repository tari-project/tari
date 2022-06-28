// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use thiserror::Error;

#[derive(Debug, Error)]
pub enum WasmError {
    #[error("Missing argument at position {position} (name: {argument_name}")]
    MissingArgument { argument_name: String, position: usize },
}
