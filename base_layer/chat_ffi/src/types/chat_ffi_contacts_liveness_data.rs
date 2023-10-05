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

use std::{convert::TryFrom, ffi::CString};

use libc::c_char;
use tari_contacts::contacts_service::handle::ContactsLivenessData;

#[repr(C)]
pub struct ChatFFIContactsLivenessData {
    pub address: *const c_char,
    pub last_seen: u64,
    pub online_status: u8,
}

impl TryFrom<ContactsLivenessData> for ChatFFIContactsLivenessData {
    type Error = String;

    fn try_from(v: ContactsLivenessData) -> Result<Self, Self::Error> {
        let address = match CString::new(v.address().to_bytes()) {
            Ok(s) => s,
            Err(e) => return Err(e.to_string()),
        };

        let last_seen = match v.last_ping_pong_received() {
            Some(ts) => match u64::try_from(ts.timestamp_micros()) {
                Ok(num) => num,
                Err(e) => return Err(e.to_string()),
            },
            None => 0,
        };

        Ok(Self {
            address: address.as_ptr(),
            last_seen,
            online_status: v.online_status().as_u8(),
        })
    }
}

/// Frees memory for a ChatFFIContactsLivenessData
///
/// ## Arguments
/// `address` - The pointer of a ChatFFIContactsLivenessData
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn destroy_chat_ffi_liveness_data(address: *mut ChatFFIContactsLivenessData) {
    if !address.is_null() {
        drop(Box::from_raw(address))
    }
}
