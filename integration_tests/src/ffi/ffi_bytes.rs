//   Copyright 2022. The Taiji Project
//
//   Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//   following conditions are met:
//
//   1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//   disclaimer.
//
//   2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//   following disclaimer in the documentation and/or other materials provided with the distribution.
//
//   3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//   products derived from this software without specific prior written permission.
//
//   THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//   INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//   DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//   SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//   SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//   WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//   USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{convert::TryFrom, ptr::null_mut};

use libc::c_void;
use tari_utilities::{hex, ByteArray};

use super::ffi_import;

pub struct FFIBytes {
    ptr: *mut c_void,
}

impl Drop for FFIBytes {
    fn drop(&mut self) {
        unsafe { ffi_import::byte_vector_destroy(self.ptr) };
        self.ptr = null_mut();
    }
}

impl FFIBytes {
    pub fn from_ptr(ptr: *mut c_void) -> Self {
        Self { ptr }
    }

    fn get_length(&self) -> usize {
        let mut error = 0;
        let length;
        unsafe {
            length = ffi_import::byte_vector_get_length(self.ptr, &mut error) as usize;
            if error > 0 {
                println!("byte_vector_get_length error {}", error);
            }
        }
        length
    }

    fn get_at(&self, i: u32) -> u8 {
        let mut error = 0;
        let byte;
        unsafe {
            byte = ffi_import::byte_vector_get_at(self.ptr, i, &mut error);
            if error > 0 {
                println!("byte_vector_get_at error {}", error);
            }
        }
        byte
    }

    pub fn get_vec(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(self.get_length());
        for i in 0..self.get_length() {
            data.push(self.get_at(u32::try_from(i).unwrap()));
        }
        data
    }

    pub fn get_as_hex(&self) -> String {
        let data = self.get_vec();
        hex::to_hex(data.as_bytes())
    }

    pub fn get_ptr(&self) -> *mut c_void {
        self.ptr
    }
}
