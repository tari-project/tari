// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

mod error;
mod wasm_module_definition;
mod wasm_module_factory;

pub use error::{WasmError, WasmExecutionError};
pub use wasm_module_definition::WasmModuleDefinition;
pub use wasm_module_factory::WasmModuleFactory;

mod module;
pub use module::LoadedWasmModule;

mod process;
pub use process::{ExecutionResult, Process};

mod vm;
pub use vm::VmInstance;
