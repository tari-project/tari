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
use std::{mem, ptr::copy, vec::Vec, collections::HashMap};

use tari_template_abi::{FunctionDef, Type, TemplateDef, encode_with_len, CallInfo, decode};

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

// TODO: ------ Everything below here should be in a common wasm lib ------

fn generate_abi(template_name: String, functions: Vec<FunctionDef>) -> *mut u8 {
    let template = TemplateDef {
        template_name,
        functions,
    };

    let buf = encode_with_len(&template);
    wrap_ptr(buf)
}

type FunctionImpl = Box<dyn Fn(Vec<Vec<u8>>) -> Vec<u8>>;

struct TemplateImpl(HashMap<String, FunctionImpl>);

impl TemplateImpl {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    pub fn add_function(&mut self, name: String, implementation: FunctionImpl) {
        self.0.insert(name.clone(), implementation);
    }
}

fn generate_main(call_info: *mut u8, call_info_len: usize, template_impl: TemplateImpl) -> *mut u8 {
    if call_info.is_null() {
        panic!("call_info is null");
    }

    let call_data = unsafe { Vec::from_raw_parts(call_info, call_info_len, call_info_len) };
    let call_info: CallInfo = decode(&call_data).unwrap();

    // get the function
    let function = match template_impl.0.get(&call_info.func_name) {
        Some(f) => f.clone(),
        None => panic!("invalid function name"),
    };

    // call the function
    let result = function(call_info.args);

    // return the encoded results of the function call
    wrap_ptr(result)
}

fn wrap_ptr(mut v: Vec<u8>) -> *mut u8 {
    let ptr = v.as_mut_ptr();
    mem::forget(v);
    ptr
}

extern "C" {
    pub fn tari_engine(op: u32, input_ptr: *const u8, input_len: usize) -> *mut u8;
}

#[no_mangle]
unsafe extern "C" fn tari_alloc(len: u32) -> *mut u8 {
    let cap = (len + 4) as usize;
    let mut buf = Vec::<u8>::with_capacity(cap);
    let ptr = buf.as_mut_ptr();
    mem::forget(buf);
    copy(len.to_le_bytes().as_ptr(), ptr, 4);
    ptr
}

#[no_mangle]
unsafe extern "C" fn tari_free(ptr: *mut u8) {
    let mut len = [0u8; 4];
    copy(ptr, len.as_mut_ptr(), 4);

    let cap = (u32::from_le_bytes(len) + 4) as usize;
    let _ = Vec::<u8>::from_raw_parts(ptr, cap, cap);
}
