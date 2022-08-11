// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::io;

use thiserror::Error;
use wasmer::{ExportError, HostEnvInitError, InstantiationError};

use crate::runtime::RuntimeError;

#[derive(Debug, Error)]
pub enum WasmError {
    #[error("Missing argument at position {position} (name: {argument_name}")]
    MissingArgument { argument_name: String, position: usize },
}

#[derive(Debug, thiserror::Error)]
pub enum WasmExecutionError {
    #[error(transparent)]
    InstantiationError(#[from] InstantiationError),
    #[error(transparent)]
    ExportError(#[from] ExportError),
    #[error(transparent)]
    WasmRuntimeError(#[from] wasmer::RuntimeError),
    #[error(transparent)]
    HostEnvInitError(#[from] HostEnvInitError),
    #[error("Function {name} not found")]
    FunctionNotFound { name: String },
    #[error("Expected function {function} to return a pointer")]
    ExpectedPointerReturn { function: String },
    #[error("Attempted to write {requested} bytes but pointer allocated {allocated}")]
    InvalidWriteLength { allocated: u32, requested: u32 },
    #[error("memory underflow: {required} bytes required but {remaining} remaining")]
    MemoryUnderflow { required: usize, remaining: usize },
    #[error("memory pointer out of range: memory size of {size} but pointer is {pointer}")]
    MemoryPointerOutOfRange { size: u64, pointer: u64, len: u64 },
    #[error("Memory allocation failed")]
    MemoryAllocationFailed,
    #[error("Memory not initialized")]
    MemoryNotInitialized,
    #[error("Invalid operation {op}")]
    InvalidOperation { op: i32 },
    #[error("Missing function {function}")]
    MissingAbiFunction { function: String },
    #[error("Runtime error: {0}")]
    RuntimeError(#[from] RuntimeError),
    #[error("Failed to decode argument for engine call: {0}")]
    EngineArgDecodeFailed(io::Error),
    #[error("maximum module memory size exceeded")]
    MaxMemorySizeExceeded,
    #[error("Failed to decode ABI")]
    AbiDecodeError,
    #[error("package ABI function returned an invalid type")]
    InvalidReturnTypeFromAbiFunc,
    #[error("package did not contain an ABI definition")]
    NoAbiDefinition,
    #[error("Unexpected ABI function {name}")]
    UnexpectedAbiFunction { name: String },
}
