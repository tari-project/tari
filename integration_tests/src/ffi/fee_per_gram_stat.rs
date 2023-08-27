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

use std::ptr::null_mut;

use libc::c_void;

use super::ffi_import;

pub struct FeePerGramStat {
    ptr: *mut c_void,
}

impl Drop for FeePerGramStat {
    fn drop(&mut self) {
        unsafe { ffi_import::fee_per_gram_stat_destroy(self.ptr) };
        self.ptr = null_mut();
    }
}
impl FeePerGramStat {
    pub fn from_ptr(ptr: *mut c_void) -> Self {
        Self { ptr }
    }

    pub fn get_order(&self) -> u64 {
        let order;
        let mut error = 0;
        unsafe {
            order = ffi_import::fee_per_gram_stat_get_order(self.ptr, &mut error);
            if error > 0 {
                println!("fee_per_gram_stat_get_order error {}", error);
            }
        }
        order
    }

    pub fn get_min_fee_per_gram(&self) -> u64 {
        let min;
        let mut error = 0;
        unsafe {
            min = ffi_import::fee_per_gram_stat_get_min_fee_per_gram(self.ptr, &mut error);
            if error > 0 {
                println!("fee_per_gram_stat_get_min_fee_per_gram error {}", error);
            }
        }
        min
    }

    pub fn get_avg_fee_per_gram(&self) -> u64 {
        let avg;
        let mut error = 0;
        unsafe {
            avg = ffi_import::fee_per_gram_stat_get_avg_fee_per_gram(self.ptr, &mut error);
            if error > 0 {
                println!("fee_per_gram_stat_get_avg_fee_per_gram error {}", error);
            }
        }
        avg
    }

    pub fn get_max_fee_per_gram(&self) -> u64 {
        let max;
        let mut error = 0;
        unsafe {
            max = ffi_import::fee_per_gram_stat_get_max_fee_per_gram(self.ptr, &mut error);
            if error > 0 {
                println!("fee_per_gram_stat_get_max_fee_per_gram error {}", error);
            }
        }
        max
    }
}
