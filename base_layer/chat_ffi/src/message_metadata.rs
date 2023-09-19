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

use std::{convert::TryFrom, ffi::CStr, ptr};

use libc::{c_char, c_int};
use tari_contacts::contacts_service::types::{Message, MessageMetadata, MessageMetadataType};

use crate::error::{InterfaceError, LibChatError};

/// Creates message metadata and appends it to a Message
///
/// ## Arguments
/// `message` - A pointer to a message
/// `metadata_type` - An int8 that maps to MessageMetadataType enum
///     '0' -> Reply
///     '1' -> TokenRequest
/// `data` - contents for the metadata in string format
/// `error_out` - Pointer to an int which will be modified
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// ## Safety
/// `message` should be destroyed eventually
#[no_mangle]
pub unsafe extern "C" fn add_chat_message_metadata(
    message: *mut Message,
    metadata_type: c_int,
    data: *const c_char,
    error_out: *mut c_int,
) {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if message.is_null() {
        error = LibChatError::from(InterfaceError::NullError("message".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return;
    }

    let metadata_byte = match u8::try_from(metadata_type) {
        Ok(byte) => byte,
        Err(e) => {
            error = LibChatError::from(InterfaceError::InvalidArgument(e.to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return;
        },
    };

    let metadata_type = match MessageMetadataType::from_byte(metadata_byte) {
        Some(t) => t,
        None => {
            error = LibChatError::from(InterfaceError::InvalidArgument(
                "Couldn't convert byte to Metadata type".to_string(),
            ))
            .code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return;
        },
    };

    if data.is_null() {
        error = LibChatError::from(InterfaceError::NullError("data".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return;
    }

    let data: Vec<u8> = match CStr::from_ptr(data).to_str() {
        Ok(str) => str.as_bytes().into(),
        Err(e) => {
            error = LibChatError::from(InterfaceError::InvalidArgument(e.to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return;
        },
    };

    let metadata = MessageMetadata { metadata_type, data };
    (*message).push(metadata);
}

#[cfg(test)]
mod test {
    use std::ffi::CString;

    use libc::{c_char, c_int};

    use super::add_chat_message_metadata;

    #[test]
    fn test_metadata_adding() {
        let message_ptr = Box::into_raw(Box::default());

        let data_c_str = CString::new("hello".to_string()).unwrap();
        let data_char: *const c_char = CString::into_raw(data_c_str) as *const c_char;

        let error_out = Box::into_raw(Box::new(0));

        unsafe { add_chat_message_metadata(message_ptr, 0 as c_int, data_char, error_out) }

        let message = unsafe { Box::from_raw(message_ptr) };
        assert_eq!(message.metadata.len(), 1)
    }
}
