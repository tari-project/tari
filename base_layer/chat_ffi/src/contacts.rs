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

use std::ptr;

use libc::{c_int, c_uchar};
use tari_chat_client::ChatClient as ChatClientTrait;
use tari_common_types::tari_address::TariAddress;

use crate::{
    error::{InterfaceError, LibChatError},
    ChatClient,
};

/// Add a contact
///
/// ## Arguments
/// `client` - The ChatClient pointer
/// `address` - A TariAddress ptr
/// `error_out` - Pointer to an int which will be modified
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// The ```receiver``` should be destroyed after use
#[no_mangle]
pub unsafe extern "C" fn add_chat_contact(client: *mut ChatClient, address: *mut TariAddress, error_out: *mut c_int) {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if client.is_null() {
        error = LibChatError::from(InterfaceError::NullError("client".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    }

    if address.is_null() {
        error = LibChatError::from(InterfaceError::NullError("receiver".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    }

    let result = (*client).runtime.block_on((*client).client.add_contact(&(*address)));

    if let Err(e) = result {
        error = LibChatError::from(InterfaceError::ContactServiceError(e.to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    }
}

/// Check the online status of a contact
///
/// ## Arguments
/// `client` - The ChatClient pointer
/// `address` - A TariAddress ptr
/// `error_out` - Pointer to an int which will be modified
///
/// ## Returns
/// `status` - Returns an c_uchar representing of the online status
///            Online = 1,
///            Offline = 2,
///            NeverSeen = 3,
///            Banned = 4,
///
/// # Safety
/// The ```address``` should be destroyed after use
#[no_mangle]
pub unsafe extern "C" fn check_online_status(
    client: *mut ChatClient,
    receiver: *mut TariAddress,
    error_out: *mut c_int,
) -> c_uchar {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if client.is_null() {
        error = LibChatError::from(InterfaceError::NullError("client".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    }

    if receiver.is_null() {
        error = LibChatError::from(InterfaceError::NullError("receiver".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    }

    let rec = (*receiver).clone();
    let result = (*client).runtime.block_on((*client).client.check_online_status(&rec));

    match result {
        Ok(status) => status.as_u8(),
        Err(e) => {
            error = LibChatError::from(InterfaceError::ContactServiceError(e.to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            0
        },
    }
}
