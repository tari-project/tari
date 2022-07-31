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

use tari_template_abi::{decode, encode_with_len, FunctionDef, Type};
use tari_template_lib::{call_engine, generate_abi, generate_main, TemplateImpl};

// that's what the example should look like from the user's perspective
#[allow(dead_code)]
mod state_template {
    use tari_template_abi::{borsh, Decode, Encode};

    // #[tari::template]
    #[derive(Encode, Decode)]
    pub struct State {
        value: u32,
    }

    // #[tari::impl]
    impl State {
        // #[tari::constructor]
        pub fn new() -> Self {
            Self { value: 0 }
        }

        pub fn set(&mut self, value: u32) {
            self.value = value;
        }

        pub fn get(&self) -> u32 {
            self.value
        }
    }
}

// TODO: Macro generated code
#[no_mangle]
extern "C" fn State_abi() -> *mut u8 {
    let template_name = "State".to_string();

    let functions = vec![
        FunctionDef {
            name: "new".to_string(),
            arguments: vec![],
            output: Type::U32, // the component_id
        },
        FunctionDef {
            name: "set".to_string(),
            arguments: vec![Type::U32, Type::U32], // the component_id and the new value
            output: Type::Unit,                    // does not return anything
        },
        FunctionDef {
            name: "get".to_string(),
            arguments: vec![Type::U32], // the component_id
            output: Type::U32,          // the stored value
        },
    ];

    generate_abi(template_name, functions)
}

#[no_mangle]
extern "C" fn State_main(call_info: *mut u8, call_info_len: usize) -> *mut u8 {
    let mut template_impl = TemplateImpl::new();
    use tari_template_abi::{ops::*, CreateComponentArg, EmitLogArg, LogLevel};
    use tari_template_lib::models::ComponentId;

    tari_template_lib::call_engine::<_, ()>(OP_EMIT_LOG, &EmitLogArg {
        message: "This is a log message from State_main!".to_string(),
        level: LogLevel::Info,
    });

    // constructor
    template_impl.add_function(
        "new".to_string(),
        Box::new(|_| {
            let ret = state_template::State::new();
            let encoded = encode_with_len(&ret);
            // Call the engine to create a new component
            // TODO: proper component id
            // The macro will know to generate this call because of the #[tari(constructor)] attribute
            // TODO: what happens if the user wants to return multiple components/types?
            let component_id = call_engine::<_, ComponentId>(OP_CREATE_COMPONENT, &CreateComponentArg {
                name: "State".to_string(),
                quantity: 1,
                metadata: Default::default(),
                state: encoded,
            });
            let component_id = component_id.expect("no asset id returned");
            encode_with_len(&component_id)
        }),
    );

    template_impl.add_function(
        "set".to_string(),
        Box::new(|args| {
            // read the function paramenters
            let _component_id: u32 = decode(&args[0]).unwrap();
            let _new_value: u32 = decode(&args[1]).unwrap();

            // update the component value
            // TODO: use a real op code (not "123") when they are implemented
            call_engine::<_, ()>(123, &());

            // the function does not return any value
            // TODO: implement "Unit" type empty responses. Right now this fails: wrap_ptr(vec![])
            encode_with_len(&0)
        }),
    );

    template_impl.add_function(
        "get".to_string(),
        Box::new(|args| {
            // read the function paramenters
            let _component_id: u32 = decode(&args[0]).unwrap();

            // get the component state
            // TODO: use a real op code (not "123") when they are implemented
            let _state = call_engine::<_, ()>(123, &());

            // return the value
            let value = 1_u32; // TODO: read from the component state
            encode_with_len(&value)
        }),
    );

    generate_main(call_info, call_info_len, template_impl)
}
