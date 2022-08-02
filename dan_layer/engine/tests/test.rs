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
use tari_common_types::types::FixedHash;
use tari_crypto::ristretto::RistrettoSecretKey;
use tari_dan_engine::{
    crypto::create_key_pair,
    instruction::{Instruction, InstructionBuilder, InstructionProcessor},
    packager::Package,
    wasm::compile::build_wasm_module_from_source,
};
use tari_template_abi::encode_with_len;

#[test]
fn test_hello_world() {
    let template_test = TemplateTest::new("HelloWorld".to_string(), "tests/hello_world".to_string());
    let result: String = template_test.call_function("greet".to_string(), vec![]);

    // FIXME: without the "encode_with_len" calls, the strings are different because of added padding characters
    assert_eq!(encode_with_len(&result), encode_with_len(&"Hello World!"));
}

#[test]
fn test_state() {
    let template_test = TemplateTest::new("State".to_string(), "tests/state".to_string());

    let component_id: u32 = template_test.call_function("new".to_string(), vec![]);
    assert_eq!(component_id, 0);

    let new_value = 20_u32;
    template_test.call_method::<()>("State".to_string(), "set".to_string(), vec![
        encode_with_len(&component_id),
        encode_with_len(&new_value),
    ]);
    let value: u32 = template_test.call_method("State".to_string(), "get".to_string(), vec![encode_with_len(
        &component_id,
    )]);
    assert_eq!(value, 0);

    // TODO: use the Component and ComponentId types in the template
    // let template_test = TemplateTest::new("State".to_string(), "tests/state".to_string());
    //
    // constructor
    // let component: ComponentId = template_test.call_function("new".to_string(), vec![]);
    // assert_eq!(component.1, 0);
    // let component: ComponentId = template_test.call_function("new".to_string(), vec![]);
    // assert_eq!(component.1, 1);
    //
    // call the "set" method to update the instance value
    // let new_value = 20_u32;
    // template_test.call_method::<()>("State".to_string(), "set".to_string(), vec![
    // encode_with_len(&component),
    // encode_with_len(&new_value),
    // ]);
    // call the "get" method to get the current value
    // let value: u32 = template_test.call_method("State".to_string(), "get".to_string(), vec![encode_with_len(
    // &component,
    // )]);
    // assert_eq!(value, 1);
}

struct TemplateTest {
    template_name: String,
    package_id: FixedHash,
    processor: InstructionProcessor<MockRuntimeInterface>,
    secret_key: RistrettoSecretKey,
}

impl TemplateTest {
    pub fn new(template_name: String, template_path: String) -> Self {
        let mut processor = InstructionProcessor::new(MockRuntimeInterface::new());
        let (secret_key, _pk) = create_key_pair();

        let wasm = build_wasm_module_from_source(template_path).unwrap();
        let package = Package::builder().add_wasm_module(wasm).build().unwrap();
        let package_id = package.id();
        processor.load(package);

        Self {
            template_name,
            package_id,
            processor,
            secret_key,
        }
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

    pub fn call_method<T>(&self, component_id: String, method_name: String, args: Vec<Vec<u8>>) -> T
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
