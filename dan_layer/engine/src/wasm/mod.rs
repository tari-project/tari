// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

mod wasm_module_definition;
mod wasm_module_factory;

pub use wasm_module_definition::WasmModuleDefinition;
pub use wasm_module_factory::WasmModuleFactory;

pub mod compile;

mod error;
pub use error::{WasmError, WasmExecutionError};

mod environment;

mod module;
pub use module::{LoadedWasmModule, WasmModule};

mod process;
pub use process::{ExecutionResult, Process};
