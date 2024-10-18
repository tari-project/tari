//   Copyright 2022. The Tari Project
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

use std::{ffi::CString, ptr::null_mut};

use libc::c_void;

use super::ffi_import;

pub struct CommsConfig {
    ptr: *mut c_void,
}

impl Drop for CommsConfig {
    fn drop(&mut self) {
        unsafe { ffi_import::comms_config_destroy(self.ptr) };
        self.ptr = null_mut();
    }
}

impl CommsConfig {
    pub fn create(port: u64) -> Self {
        let mut error = 0;
        let ptr;
        unsafe {
            ptr = ffi_import::comms_config_create(
                CString::new(format!("/ip4/127.0.0.1/tcp/{}", port)).unwrap().into_raw(),
                CString::new(format!("/ip4/127.0.0.1/tcp/{}", port)).unwrap().into_raw(),
                &mut error,
            );
            if error > 0 {
                println!("comms config error {}", error);
                panic!("comms config error");
            }
        }
        Self { ptr }
    }

    pub fn get_ptr(&self) -> *mut c_void {
        self.ptr
    }
}
