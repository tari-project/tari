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
#![allow(non_snake_case)]

use core::ptr;

pub use tari_template_abi::{tari_alloc, tari_free};

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[cfg(any(feature = "call_engine_in_abi", feature = "unexpected_export_function"))]
#[no_mangle]
pub extern "C" fn Buggy_abi() -> *mut u8 {
    use tari_template_abi::*;
    // Call the engine in the ABI code, you aren't allowed to do that *shakes head*
    #[cfg(feature = "call_engine_in_abi")]
    unsafe {
        tari_engine(123, ptr::null_mut(), 0)
    };
    wrap_ptr(encode_with_len(&TemplateDef {
        template_name: "".to_string(),
        functions: vec![],
    }))
}

#[cfg(feature = "return_null_abi")]
#[no_mangle]
pub extern "C" fn Buggy_abi() -> *mut u8 {
    ptr::null_mut()
}

#[no_mangle]
pub extern "C" fn Buggy_main(_call_info: *mut u8, _call_info_len: usize) -> *mut u8 {
    ptr::null_mut()
}

extern "C" {
    pub fn tari_engine(op: i32, input_ptr: *const u8, input_len: usize) -> *mut u8;
    pub fn debug(input_ptr: *const u8, input_len: usize);
}

#[cfg(feature = "unexpected_export_function")]
#[no_mangle]
pub extern "C" fn i_shouldnt_be_here() -> *mut u8 {
    ptr::null_mut()
}
