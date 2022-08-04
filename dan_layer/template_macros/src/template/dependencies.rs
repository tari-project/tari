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

use proc_macro2::TokenStream;
use quote::quote;

pub fn generate_dependencies() -> TokenStream {
    quote! {
        use tari_template_lib::{wrap_ptr, tari_alloc, tari_free};
        // extern "C" {
        //     pub fn tari_engine(op: u32, input_ptr: *const u8, input_len: usize) -> *mut u8;
        // }
        //
        // pub fn wrap_ptr(mut v: Vec<u8>) -> *mut u8 {
        //     use std::mem;
        //
        //     let ptr = v.as_mut_ptr();
        //     mem::forget(v);
        //     ptr
        // }
        //
        // #[no_mangle]
        // pub unsafe extern "C" fn tari_alloc(len: u32) -> *mut u8 {
        //     use std::{mem, intrinsics::copy};
        //
        //     let cap = (len + 4) as usize;
        //     let mut buf = Vec::<u8>::with_capacity(cap);
        //     let ptr = buf.as_mut_ptr();
        //     mem::forget(buf);
        //     copy(len.to_le_bytes().as_ptr(), ptr, 4);
        //     ptr
        // }
        //
        // #[no_mangle]
        // pub unsafe extern "C" fn tari_free(ptr: *mut u8) {
        //     use std::intrinsics::copy;
        //
        //     let mut len = [0u8; 4];
        //     copy(ptr, len.as_mut_ptr(), 4);
        //
        //     let cap = (u32::from_le_bytes(len) + 4) as usize;
        //     let _ = Vec::<u8>::from_raw_parts(ptr, cap, cap);
        // }
    }
}
