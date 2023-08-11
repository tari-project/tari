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

use std::ptr::null_mut;

use libc::c_void;

use super::ffi_import;

pub struct Balance {
    ptr: *mut c_void,
}

impl Drop for Balance {
    fn drop(&mut self) {
        unsafe { ffi_import::balance_destroy(self.ptr) };
        self.ptr = null_mut();
    }
}
impl Balance {
    pub fn from_ptr(ptr: *mut c_void) -> Self {
        Self { ptr }
    }

    pub fn get_available(&self) -> u64 {
        let available;
        let mut error = 0;
        unsafe {
            available = ffi_import::balance_get_available(self.ptr, &mut error);
            if error > 0 {
                println!("balance_get_available error {}", error);
            }
        }
        available
    }

    pub fn get_time_locked(&self) -> u64 {
        let time_locked;
        let mut error = 0;
        unsafe {
            time_locked = ffi_import::balance_get_time_locked(self.ptr, &mut error);
            if error > 0 {
                println!("balance_get_time_locked error {}", error);
            }
        }
        time_locked
    }

    pub fn get_pending_incoming(&self) -> u64 {
        let pending_incoming;
        let mut error = 0;
        unsafe {
            pending_incoming = ffi_import::balance_get_pending_incoming(self.ptr, &mut error);
            if error > 0 {
                println!("balance_get_pending_incoming error {}", error);
            }
        }
        pending_incoming
    }

    pub fn get_pending_outgoing(&self) -> u64 {
        let pending_outgoing;
        let mut error = 0;
        unsafe {
            pending_outgoing = ffi_import::balance_get_pending_outgoing(self.ptr, &mut error);
            if error > 0 {
                println!("balance_get_pending_outgoing error {}", error);
            }
        }
        pending_outgoing
    }
}
