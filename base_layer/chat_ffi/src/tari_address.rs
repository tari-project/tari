// Copyright 2023, The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{ffi::CStr, ptr, str::FromStr};

use libc::{c_char, c_int};
use tari_common_types::tari_address::TariAddress;

use crate::error::{InterfaceError, LibChatError};

/// Creates a TariAddress and returns a ptr
///
/// ## Arguments
/// `receiver_c_char` - A string containing a tari address hex value
/// `error_out` - Pointer to an int which will be modified
///
/// ## Returns
/// `*mut TariAddress` - A ptr to a TariAddress
///
/// # Safety
/// The ```destroy_tari_address``` function should be called when finished with the TariAddress
#[no_mangle]
pub unsafe extern "C" fn create_tari_address(
    receiver_c_char: *const c_char,
    error_out: *mut c_int,
) -> *mut TariAddress {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    let receiver = match CStr::from_ptr(receiver_c_char).to_str() {
        Ok(str) => match TariAddress::from_str(str) {
            Ok(address) => address,
            Err(e) => {
                error = LibChatError::from(InterfaceError::InvalidArgument(e.to_string())).code;
                ptr::swap(error_out, &mut error as *mut c_int);
                return ptr::null_mut();
            },
        },
        Err(e) => {
            error = LibChatError::from(InterfaceError::NullError(e.to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return ptr::null_mut();
        },
    };

    Box::into_raw(Box::new(receiver))
}

/// Frees memory for a TariAddress
///
/// ## Arguments
/// `address` - The pointer of a TariAddress
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn destroy_tari_address(address: *mut TariAddress) {
    if !address.is_null() {
        drop(Box::from_raw(address))
    }
}
