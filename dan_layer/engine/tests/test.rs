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

mod mock_runtime_interface;

use borsh::BorshDeserialize;
use mock_runtime_interface::MockRuntimeInterface;
use tari_crypto::ristretto::RistrettoSecretKey;
use tari_dan_engine::{
    crypto::create_key_pair,
    instruction::{Instruction, InstructionBuilder, InstructionProcessor},
    packager::Package,
    state_store::{memory::MemoryStateStore, AtomicDb, StateReader},
    wasm::compile::compile_template,
};
use tari_template_abi::encode;
use tari_template_lib::models::{ComponentId, ComponentInstance, PackageId};

#[test]
fn test_hello_world() {
    let template_test = TemplateTest::new("HelloWorld".to_string(), "tests/hello_world".to_string());
    let result: String = template_test.call_function("greet".to_string(), vec![]);

    assert_eq!(result, "Hello World!");
}

#[test]
fn test_state() {
    let template_test = TemplateTest::new("State".to_string(), "tests/state".to_string());
    let store = template_test.state_store();

    // constructor
    let component_id1: ComponentId = template_test.call_function("new".to_string(), vec![]);
    template_test.assert_calls(&["emit_log", "create_component"]);
    template_test.clear_calls();

    let component_id2: ComponentId = template_test.call_function("new".to_string(), vec![]);
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
    template_test.call_method::<()>(component_id2, "set".to_string(), vec![encode(&new_value).unwrap()]);

    // call the "get" method to get the current value
    let value: u32 = template_test.call_method(component_id2, "get".to_string(), vec![]);

    assert_eq!(value, new_value);
}

struct TemplateTest {
    template_name: String,
    package_id: PackageId,
    processor: InstructionProcessor<MockRuntimeInterface>,
    secret_key: RistrettoSecretKey,
    runtime_interface: MockRuntimeInterface,
}

impl TemplateTest {
    pub fn new(template_name: String, template_path: String) -> Self {
        let runtime_interface = MockRuntimeInterface::new();
        let mut processor = InstructionProcessor::new(runtime_interface.clone());
        let (secret_key, _pk) = create_key_pair();

        let wasm = compile_template(template_path).unwrap();
        let package = Package::builder().add_wasm_module(wasm).build().unwrap();
        let package_id = package.id();
        processor.load(package);

        Self {
            template_name,
            package_id,
            processor,
            secret_key,
            runtime_interface,
        }
    }

    pub fn state_store(&self) -> MemoryStateStore {
        self.runtime_interface.state_store()
    }

    pub fn assert_calls(&self, expected: &[&'static str]) {
        let calls = self.runtime_interface.get_calls();
        assert_eq!(calls, expected);
    }

    pub fn clear_calls(&self) {
        self.runtime_interface.clear_calls();
    }

    pub fn call_function<T>(&self, func_name: String, args: Vec<Vec<u8>>) -> T
    where T: BorshDeserialize {
        let instruction = InstructionBuilder::new()
            .add_instruction(Instruction::CallFunction {
                package_id: self.package_id,
                template: self.template_name.clone(),
                function: func_name,
                args,
            })
            .sign(&self.secret_key)
            .build();
        let result = self.processor.execute(instruction).unwrap();

        result[0].decode::<T>().unwrap()
    }

    pub fn call_method<T>(&self, component_id: ComponentId, method_name: String, args: Vec<Vec<u8>>) -> T
    where T: BorshDeserialize {
        let instruction = InstructionBuilder::new()
            .add_instruction(Instruction::CallMethod {
                package_id: self.package_id,
                component_id,
                method: method_name,
                args,
            })
            .sign(&self.secret_key)
            .build();
        let result = self.processor.execute(instruction).unwrap();

        result[0].decode::<T>().unwrap()
    }
}
