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

// TODO: we should only use stdlib if the template dev needs to include it e.g. use core::mem when stdlib is not
// available

use common::{generate_abi, TemplateImpl, generate_main};
use tari_template_abi::{FunctionDef, Type, encode_with_len, decode};

// that's what the example should look like from the user's perspective
#[allow(dead_code)]
mod rust {
    // #[tari::template]
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

    let functions = vec![FunctionDef {
        name: "new".to_string(),
        arguments: vec![],
        output: Type::U32, // the component_id
    }, FunctionDef {
        name: "set".to_string(),
        arguments: vec![Type::U32, Type::U32], // the component_id and the new value
        output: Type::Unit, // does not return anything
    }, FunctionDef {
        name: "get".to_string(),
        arguments: vec![Type::U32], // the component_id
        output: Type::U32,  // the stored value
    }];

    generate_abi(template_name, functions)
}

#[no_mangle]
extern "C" fn State_main(call_info: *mut u8, call_info_len: usize) -> *mut u8 {
    let mut template_impl = TemplateImpl::new();

    // constructor
    template_impl.add_function("new".to_string(), Box::new(|_| {
        // Call the engine to create a new component
        // TODO: use a real op code (not "123") when they are implemented       
        let _component_id = unsafe { tari_engine(123, std::ptr::null(), 0) };

        // TODO: decode the returning value into a real component id
        let component_id = 1_u32;
        encode_with_len(&component_id)
    }));

    template_impl.add_function("set".to_string(), Box::new(|args| {
        // read the function paramenters
        let _component_id: u32 = decode(&args[0]).unwrap();
        let _new_value: u32 = decode(&args[1]).unwrap();

        // update the component value
        // TODO: use a real op code (not "123") when they are implemented
        unsafe { tari_engine(123, std::ptr::null(), 0) };

        // the function does not return any value
        // TODO: implement "Unit" type empty responses. Right now this fails: wrap_ptr(vec![])
        encode_with_len(&0)
    }));

    template_impl.add_function("get".to_string(), Box::new(|args| {
        // read the function paramenters
        let _component_id: u32 = decode(&args[0]).unwrap();

        // get the component state
        // TODO: use a real op code (not "123") when they are implemented
        let _state = unsafe { tari_engine(123, std::ptr::null(), 0) };

        // return the value
        let value = 1_u32;  // TODO: read from the component state
        encode_with_len(&value)
    }));

    generate_main(call_info, call_info_len, template_impl)
}

extern "C" {
    pub fn tari_engine(op: u32, input_ptr: *const u8, input_len: usize) -> *mut u8;
}

#[no_mangle]
unsafe extern "C" fn tari_alloc(len: u32) -> *mut u8 {
    common::tari_alloc(len)
}

#[no_mangle]
unsafe extern "C" fn tari_free(ptr: *mut u8) {
    common::tari_free(ptr)
}
