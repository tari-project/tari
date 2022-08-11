//  Copyright 2022. The Tari Project
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

mod tooling;

use tari_dan_engine::{
    packager::{Package, PackageError},
    state_store::{AtomicDb, StateReader},
    wasm::{compile::compile_template, WasmExecutionError},
};
use tari_template_lib::{
    args,
    models::{ComponentId, ComponentInstance},
};
use tooling::TemplateTest;

#[test]
fn test_hello_world() {
    let template_test = TemplateTest::new(vec!["tests/templates/hello_world"]);
    let result: String = template_test.call_function("HelloWorld", "greet", args![]);

    assert_eq!(result, "Hello World!");
}

#[test]
fn test_state() {
    let template_test = TemplateTest::new(vec!["tests/templates/state"]);
    let store = template_test.state_store();

    // constructor
    let component_id1: ComponentId = template_test.call_function("State", "new", args![]);
    template_test.assert_calls(&["emit_log", "create_component"]);
    template_test.clear_calls();

    let component_id2: ComponentId = template_test.call_function("State", "new", args![]);
    assert_ne!(component_id1, component_id2);

    let component: ComponentInstance = store
        .read_access()
        .unwrap()
        .get_state(&component_id1)
        .unwrap()
        .expect("component1 not found");
    assert_eq!(component.module_name, "State");
    let component: ComponentInstance = store
        .read_access()
        .unwrap()
        .get_state(&component_id2)
        .unwrap()
        .expect("component2 not found");
    assert_eq!(component.module_name, "State");

    // call the "set" method to update the instance value
    let new_value = 20_u32;
    template_test.call_method::<()>(component_id2, "set", args![new_value]);

    // call the "get" method to get the current value
    let value: u32 = template_test.call_method(component_id2, "get", args![]);

    assert_eq!(value, new_value);
}

#[test]
fn test_composed() {
    let template_test = TemplateTest::new(vec!["tests/templates/state", "tests/templates/hello_world"]);

    let functions = template_test
        .get_module("HelloWorld")
        .template_def()
        .functions
        .iter()
        .map(|f| f.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(functions, vec!["greet", "new", "custom_greeting"]);

    let functions = template_test
        .get_module("State")
        .template_def()
        .functions
        .iter()
        .map(|f| f.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(functions, vec!["new", "set", "get"]);

    let component_state: ComponentId = template_test.call_function("State", "new", args![]);
    let component_hw: ComponentId = template_test.call_function("HelloWorld", "new", args!["أهلا"]);

    let result: String = template_test.call_method(component_hw, "custom_greeting", args!["Wasm"]);
    assert_eq!(result, "أهلا Wasm!");

    // call the "set" method to update the instance value
    let new_value = 20_u32;
    template_test.call_method::<()>(component_state, "set", args![new_value]);

    // call the "get" method to get the current value
    let value: u32 = template_test.call_method(component_state, "get", args![]);

    assert_eq!(value, new_value);
}

#[test]
fn test_dodgy_template() {
    let wasm = compile_template("tests/templates/buggy", &["call_engine_in_abi"]).unwrap();
    let err = Package::builder().add_wasm_module(wasm).build().unwrap_err();
    assert!(matches!(err, PackageError::TemplateCalledEngineDuringInitialization));

    let wasm = compile_template("tests/templates/buggy", &["return_null_abi"]).unwrap();
    let err = Package::builder().add_wasm_module(wasm).build().unwrap_err();
    assert!(matches!(
        err,
        PackageError::WasmModuleError(WasmExecutionError::AbiDecodeError)
    ));

    let wasm = compile_template("tests/templates/buggy", &["unexpected_export_function"]).unwrap();
    let err = Package::builder().add_wasm_module(wasm).build().unwrap_err();
    assert!(matches!(
        err,
        PackageError::WasmModuleError(WasmExecutionError::UnexpectedAbiFunction { .. })
    ));
}
