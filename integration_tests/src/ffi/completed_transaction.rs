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

use super::{ffi_import, FFIString, Kernel, WalletAddress};

pub struct CompletedTransaction {
    ptr: *mut c_void,
}

impl Drop for CompletedTransaction {
    fn drop(&mut self) {
        unsafe { ffi_import::completed_transaction_destroy(self.ptr) };
        self.ptr = null_mut();
    }
}
impl CompletedTransaction {
    pub fn from_ptr(ptr: *mut c_void) -> Self {
        Self { ptr }
    }

    pub fn get_transaction_id(&self) -> u64 {
        let tx_id;
        let mut error = 0;
        unsafe {
            tx_id = ffi_import::completed_transaction_get_transaction_id(self.ptr, &mut error);
            if error > 0 {
                println!("completed_transaction_get_transaction_id error {}", error);
                panic!("completed_transaction_get_transaction_id error");
            }
        }
        tx_id
    }

    #[allow(dead_code)]
    pub fn get_destination_tari_address(&self) -> WalletAddress {
        let ptr;
        let mut error = 0;
        unsafe {
            ptr = ffi_import::completed_transaction_get_destination_tari_address(self.ptr, &mut error);
            if error > 0 {
                println!("completed_transaction_get_destination_tari_address error {}", error);
                panic!("completed_transaction_get_destination_tari_address error");
            }
        }
        WalletAddress::from_ptr(ptr)
    }

    #[allow(dead_code)]
    pub fn get_source_tari_address(&self) -> WalletAddress {
        let ptr;
        let mut error = 0;
        unsafe {
            ptr = ffi_import::completed_transaction_get_source_tari_address(self.ptr, &mut error);
            if error > 0 {
                println!("completed_transaction_get_source_tari_address error {}", error);
                panic!("completed_transaction_get_source_tari_address error");
            }
        }
        WalletAddress::from_ptr(ptr)
    }

    pub fn get_transaction_kernel(&self) -> Kernel {
        let ptr;
        let mut error = 0;
        unsafe {
            ptr = ffi_import::completed_transaction_get_transaction_kernel(self.ptr, &mut error);
            if error > 0 {
                println!("completed_transaction_get_transaction_kernel error {}", error);
                panic!("completed_transaction_get_transaction_kernel error");
            }
        }
        Kernel::from_ptr(ptr)
    }

    #[allow(dead_code)]
    pub fn get_amount(&self) -> u64 {
        let amount;
        let mut error = 0;
        unsafe {
            amount = ffi_import::completed_transaction_get_amount(self.ptr, &mut error);
            if error > 0 {
                println!("completed_transaction_get_amount error {}", error);
                panic!("completed_transaction_get_amount error");
            }
        }
        amount
    }

    #[allow(dead_code)]
    pub fn get_fee(&self) -> u64 {
        let fee;
        let mut error = 0;
        unsafe {
            fee = ffi_import::completed_transaction_get_fee(self.ptr, &mut error);
            if error > 0 {
                println!("completed_transaction_get_fee error {}", error);
                panic!("completed_transaction_get_fee error");
            }
        }
        fee
    }

    #[allow(dead_code)]
    pub fn get_timestamp(&self) -> u64 {
        let timestamp;
        let mut error = 0;
        unsafe {
            timestamp = ffi_import::completed_transaction_get_timestamp(self.ptr, &mut error);
            if error > 0 {
                println!("completed_transaction_get_timestamp error {}", error);
                panic!("completed_transaction_get_timestamp error");
            }
        }
        timestamp
    }

    #[allow(dead_code)]
    pub fn get_message(&self) -> String {
        let ptr;
        let mut error = 0;
        unsafe {
            ptr = ffi_import::completed_transaction_get_message(self.ptr, &mut error);
            if error > 0 {
                println!("completed_transaction_get_message error {}", error);
                panic!("completed_transaction_get_message error");
            }
        }
        FFIString::from_ptr(ptr as *mut i8).as_string()
    }

    #[allow(dead_code)]
    pub fn get_status(&self) -> i32 {
        let status;
        let mut error = 0;
        unsafe {
            status = ffi_import::completed_transaction_get_status(self.ptr, &mut error);
            if error > 0 {
                println!("completed_transaction_get_status error {}", error);
                panic!("completed_transaction_get_status error");
            }
        }
        status
    }

    pub fn is_outbound(&self) -> bool {
        let is_outbound;
        let mut error = 0;
        unsafe {
            is_outbound = ffi_import::completed_transaction_is_outbound(self.ptr, &mut error);
            if error > 0 {
                println!("completed_transaction_is_outbound error {}", error);
                panic!("completed_transaction_is_outbound error");
            }
        }
        is_outbound
    }

    #[allow(dead_code)]
    pub fn completed_transaction_get_confirmations(&self) -> u64 {
        let confirmations_cnt;
        let mut error = 0;
        unsafe {
            confirmations_cnt = ffi_import::completed_transaction_get_confirmations(self.ptr, &mut error);
            if error > 0 {
                println!("completed_transaction_get_confirmations error {}", error);
                panic!("completed_transaction_get_confirmations error");
            }
        }
        confirmations_cnt
    }

    #[allow(dead_code)]
    pub fn get_cancellation_reason(&self) -> i32 {
        let reason;
        let mut error = 0;
        unsafe {
            reason = ffi_import::completed_transaction_get_cancellation_reason(self.ptr, &mut error);
            if error > 0 {
                println!("completed_transaction_get_cancellation_reason error {}", error);
                panic!("completed_transaction_get_cancellation_reason error");
            }
        }
        reason
    }
}
