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

//! # Tari WASM module library
//!
//! This library provides primitives and functionality that allows Tari WASM modules to interact with the Tari engine.
//! It is intended to be used by WASM modules that are written in Rust and compiled into WASM.
//!
//! The tari engine itself should never depend on this crate.
//!
//! TODO: no_std support

pub mod models;

// TODO: we should only use stdlib if the template dev needs to include it e.g. use core::mem when stdlib is not
// available
use std::{collections::HashMap, mem, ptr::copy, slice};

use tari_template_abi::{encode_with_len, Decode, Encode, FunctionDef, TemplateDef};

pub fn generate_abi(template_name: String, functions: Vec<FunctionDef>) -> *mut u8 {
    let template = TemplateDef {
        template_name,
        functions,
    };

    let buf = encode_with_len(&template);
    wrap_ptr(buf)
}

type FunctionImpl = Box<dyn Fn(Vec<Vec<u8>>) -> Vec<u8>>;

pub struct TemplateImpl(HashMap<String, FunctionImpl>);

impl TemplateImpl {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    pub fn add_function(&mut self, name: String, implementation: FunctionImpl) {
        self.0.insert(name.clone(), implementation);
    }
}

pub fn generate_main(call_info: *mut u8, call_info_len: usize, template_impl: TemplateImpl) -> *mut u8 {
    use tari_template_abi::{decode, CallInfo};
    if call_info.is_null() {
        panic!("call_info is null");
    }

    let call_data = unsafe { slice::from_raw_parts(call_info, call_info_len) };
    let call_info: CallInfo = decode(call_data).unwrap();

    // get the function
    let function = match template_impl.0.get(&call_info.func_name) {
        Some(f) => f,
        None => panic!("invalid function name"),
    };

    // call the function
    let result = function(call_info.args);

    // return the encoded results of the function call
    wrap_ptr(result)
}

pub fn call_engine<T: Encode, U: Decode + std::fmt::Debug>(op: i32, input: &T) -> Option<U> {
    use tari_template_abi::{decode, decode_len, encode_into};

    let mut encoded = Vec::with_capacity(512);
    encode_into(input, &mut encoded).unwrap();
    let len = encoded.len();
    let input_ptr = wrap_ptr(encoded) as *const _;
    let ptr = unsafe { tari_engine(op, input_ptr, len) };
    if ptr.is_null() {
        return None;
    }

    let slice = unsafe { slice::from_raw_parts(ptr as *const _, 4) };
    let len = decode_len(&slice).unwrap();
    let slice = unsafe { slice::from_raw_parts(ptr.offset(4), len) };
    let ret = decode(&slice).unwrap();
    Some(ret)
}

pub fn wrap_ptr(mut v: Vec<u8>) -> *mut u8 {
    let ptr = v.as_mut_ptr();
    mem::forget(v);
    ptr
}

extern "C" {
    fn tari_engine(op: i32, input_ptr: *const u8, input_len: usize) -> *mut u8;
    fn debug(input_ptr: *const u8, input_len: usize);
}

pub fn call_debug<T: AsRef<[u8]>>(data: T) {
    let ptr = data.as_ref().as_ptr();
    let len = data.as_ref().len();
    unsafe { debug(ptr, len) }
}

#[no_mangle]
pub unsafe extern "C" fn tari_alloc(len: u32) -> *mut u8 {
    let cap = (len + 4) as usize;
    let mut buf = Vec::<u8>::with_capacity(cap);
    let ptr = buf.as_mut_ptr();
    mem::forget(buf);
    copy(len.to_le_bytes().as_ptr(), ptr, 4);
    ptr
}

#[no_mangle]
pub unsafe extern "C" fn tari_free(ptr: *mut u8) {
    let mut len = [0u8; 4];
    copy(ptr, len.as_mut_ptr(), 4);

    let cap = (u32::from_le_bytes(len) + 4) as usize;
    let _ = Vec::<u8>::from_raw_parts(ptr, cap, cap);
}
