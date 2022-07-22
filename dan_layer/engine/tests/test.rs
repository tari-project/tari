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

use tari_dan_engine::{
    compile::compile_template,
    crypto::create_key_pair,
    instruction::{Instruction, InstructionBuilder, InstructionProcessor},
    package::PackageBuilder,
};

#[test]
fn test_metro() {
    let mut processor = InstructionProcessor::new();
    let (sk, _pk) = create_key_pair();

    let wasm = compile_template("tests/test_template").unwrap();
    let package = PackageBuilder::new().add_wasm_template(wasm).build().unwrap();
    let package_id = package.id();
    processor.load(package);

    let instruction = InstructionBuilder::new()
        .add_instruction(Instruction::CallFunction {
            package_id,
            template: "TestTemplate".to_string(),
            function: "initialize".to_string(),
            args: vec![],
        })
        .sign(&sk)
        .build();

    let result = processor.execute(instruction).unwrap();
    let result = result[0].decode::<String>().unwrap();
    assert_eq!(result, "'initialize' was called");

    // let instruction = InstructionBuilder::new().method("hello_world").sign(sk).build();
    // let result = processor.execute(instruction).unwrap();
}
