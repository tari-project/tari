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
use std::{mem, ptr::copy, vec::Vec};

// that's what the example should look like from the user's perspective
#[allow(dead_code)]
mod rust {
    // #[tari::template]
    pub struct TestState {
        value: u32,
    }
    
    // #[tari::impl]
    impl TestState {
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
extern "C" fn TestTemplate_abi() -> *mut u8 {
    use tari_template_abi::{encode_with_len, FunctionDef, TemplateDef, Type};

    let template = TemplateDef {
        template_name: "TestTemplate".to_string(),
        functions: vec![FunctionDef {
            name: "new".to_string(),
            arguments: vec![],
            output: Type::U32, // the component_id
        }, FunctionDef {
            name: "set".to_string(),
            arguments: vec![Type::U32, Type::U32], // the component_id and the new value
            output: Type::U32, // does not return anything
        }, FunctionDef {
            name: "get".to_string(),
            arguments: vec![Type::U32], // the component_id
            output: Type::Unit,  // the stored value
        }],
    };

    let buf = encode_with_len(&template);
    wrap_ptr(buf)
}

#[no_mangle]
extern "C" fn TestTemplate_main(call_info: *mut u8, call_info_len: usize) -> *mut u8 {
    use tari_template_abi::{decode, encode_with_len, CallInfo};
    if call_info.is_null() {
        panic!("call_info is null");
    }

    let call_data = unsafe { Vec::from_raw_parts(call_info, call_info_len, call_info_len) };
    let call_info: CallInfo = decode(&call_data).unwrap();

    match call_info.func_name.as_str() {
        "new" => {
            // Call the engine to create a new component
            // TODO: use a real op code (not "123") when they are implemented       
            let _component_id = unsafe { tari_engine(123, std::ptr::null(), 0) };

            // TODO: decode the returning value into a real component id
            let component_id = 1_u32;
            wrap_ptr(encode_with_len(&component_id))
        },
        "set" => {
            // read the function paramenters
            let _component_id: u32 = decode(&call_info.args[0]).unwrap();
            let _new_value: u32 = decode(&call_info.args[1]).unwrap();

            // update the component value
            // TODO: use a real op code (not "123") when they are implemented
            unsafe { tari_engine(123, std::ptr::null(), 0) };

            // the function does not return any value
            // wrap_ptr(vec![])
            wrap_ptr(encode_with_len(&0))
        },
        "get" => {
            // read the function paramenters
            let _component_id: u32 = decode(&call_info.args[0]).unwrap();

            // get the component state
            // TODO: use a real op code (not "123") when they are implemented
            let _state = unsafe { tari_engine(123, std::ptr::null(), 0) };

            // return the value
            let value = 1_u32;  // TODO: read from the component state
            wrap_ptr(encode_with_len(&value))
        },
        &_ => panic!("invalid function name"),
    }
}

// TODO: ------ Everything below here should be in a common wasm lib ------
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
