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

// TODO: Macro generated code
#[no_mangle]
extern "C" fn HelloWorld_abi() -> *mut u8 {
    use tari_template_abi::{encode_with_len, FunctionDef, TemplateDef, Type};

    let template = TemplateDef {
        template_name: "HelloWorld".to_string(),
        functions: vec![FunctionDef {
            name: "greet".to_string(),
            arguments: vec![],
            output: Type::String,
        }],
    };

    let buf = encode_with_len(&template);
    wrap_ptr(buf)
}

#[no_mangle]
extern "C" fn HelloWorld_main(call_info: *mut u8, call_info_len: usize) -> *mut u8 {
    use tari_template_abi::{decode, encode_with_len, CallInfo};
    if call_info.is_null() {
        panic!("call_info is null");
    }

    let call_data = unsafe { Vec::from_raw_parts(call_info, call_info_len, call_info_len) };
    let call_info: CallInfo = decode(&call_data).unwrap();

    match call_info.func_name.as_str() {
        "greet" => {
            let v = encode_with_len(&"Hello World!");
            wrap_ptr(v)
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
