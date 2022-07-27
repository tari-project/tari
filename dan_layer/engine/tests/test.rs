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

use borsh::BorshDeserialize;
use tari_common_types::types::FixedHash;
use tari_crypto::ristretto::RistrettoSecretKey;
use tari_dan_engine::{
    compile::compile_template,
    crypto::create_key_pair,
    instruction::{Instruction, InstructionBuilder, InstructionProcessor},
    package::PackageBuilder,
};
use tari_template_abi::encode_with_len;

#[test]
fn test_hello_world() {
    let template_test = TemplateTest::new("HelloWorld".to_string(), "tests/hello_world".to_string());
    let result: String = template_test.run_instruction("greet".to_string(), vec![]);
    assert_eq!(result, "Hello World!");
}

#[test]
fn test_state() {
    let template_test = TemplateTest::new("State".to_string(), "tests/state".to_string());

    // constructor
    let component_id: u32 = template_test.run_instruction("new".to_string(), vec![]);

    // call the "set" method to update the instance value
    let new_value = 20_u32;
    // TODO: implement "Unit" type empty responses
    let _: u32 = template_test.run_instruction("set".to_string(), vec![
        encode_with_len(&component_id),
        encode_with_len(&new_value),
    ]);

    // call the "get" method to get the current value
    let value: u32 = template_test.run_instruction("get".to_string(), vec![encode_with_len(&component_id)]);
    assert_eq!(value, 1);
}

struct TemplateTest {
    template_name: String,
    package_id: FixedHash,
    processor: InstructionProcessor,
    secret_key: RistrettoSecretKey,
}

impl TemplateTest {
    pub fn new(template_name: String, template_path: String) -> Self {
        let mut processor = InstructionProcessor::new();
        let (secret_key, _pk) = create_key_pair();

        let wasm = compile_template(template_path).unwrap();
        let package = PackageBuilder::new().add_wasm_template(wasm).build().unwrap();
        let package_id = package.id();
        processor.load(package);

        Self {
            template_name,
            package_id,
            processor,
            secret_key,
        }
    }

    pub fn run_instruction<T>(&self, func_name: String, args: Vec<Vec<u8>>) -> T
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
}
