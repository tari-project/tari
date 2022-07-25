// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use thiserror::Error;
use wasmer::{ExportError, InstantiationError, RuntimeError};

#[derive(Debug, Error)]
pub enum WasmError {
    #[error("Missing argument at position {position} (name: {argument_name}")]
    MissingArgument { argument_name: String, position: usize },
}

#[derive(Debug, thiserror::Error)]
pub enum WasmExecutionError {
    #[error("Function {name} not found")]
    FunctionNotFound { name: String },
    #[error("Expected function {function} to return a pointer")]
    ExpectedPointerReturn { function: String },
    #[error("Attempted to write {requested} bytes but pointer allocated {allocated}")]
    InvalidWriteLength { allocated: u32, requested: u32 },
    #[error("memory underflow: {required} bytes required but {remaining} remaining")]
    MemoryUnderflow { required: usize, remaining: usize },
    #[error(transparent)]
    InstantiationError(#[from] InstantiationError),
    #[error(transparent)]
    ExportError(#[from] ExportError),
    #[error(transparent)]
    RuntimeError(#[from] RuntimeError),
}
