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

use std::{convert::TryFrom, ptr};

use libc::{c_int, c_uint};
use tari_contacts::contacts_service::types::{Message, MessageMetadata, MessageMetadataType};

use crate::{
    error::{InterfaceError, LibChatError},
    types::{chat_byte_vector_get_at, chat_byte_vector_get_length, ChatByteVector, ChatFFIMessage},
};

#[derive(Debug, PartialEq, Clone)]
#[repr(C)]
pub struct ChatFFIMessageMetadata {
    pub data: ChatByteVector,
    pub metadata_type: c_int,
}

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
    data: *mut ChatByteVector,
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

    let chat_byte_vector_length = chat_byte_vector_get_length(data, error_out);
    let mut bytes: Vec<u8> = Vec::new();
    for c in 0..chat_byte_vector_length {
        let byte = chat_byte_vector_get_at(data, c as c_uint, error_out);
        assert_eq!(error, 0);
        bytes.push(byte);
    }

    let metadata = MessageMetadata {
        metadata_type,
        data: bytes,
    };
    (*message).push(metadata);
}

#[allow(dead_code)] // Not dead code? False positive
pub unsafe extern "C" fn read_chat_metadata_at_position(
    message: *mut ChatFFIMessage,
    position: c_uint,
    error_out: *mut c_int,
) -> *mut ChatFFIMessageMetadata {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if message.is_null() {
        error = LibChatError::from(InterfaceError::NullError("message".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    let message = &(*message);

    let len = message.metadata_len - 1;
    if len < 0 || position > len as c_uint {
        error = LibChatError::from(InterfaceError::PositionInvalidError).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }
    Box::into_raw(Box::new(message.metadata.0[len as usize].clone()))
}

#[cfg(test)]
mod test {
    use std::convert::TryFrom;

    use libc::{c_int, c_uint};
    use tari_common_types::tari_address::TariAddress;
    use tari_contacts::contacts_service::types::MessageBuilder;

    use super::add_chat_message_metadata;
    use crate::{
        message_metadata::read_chat_metadata_at_position,
        types::{chat_byte_vector_create, ChatFFIMessage},
    };

    #[test]
    fn test_metadata_adding() {
        let message_ptr = Box::into_raw(Box::default());
        let error_out = Box::into_raw(Box::new(0));

        let data = "hello".to_string();
        let data_bytes = data.as_bytes();
        let len = u32::try_from(data.len()).expect("Can't cast from usize");
        let data = unsafe { chat_byte_vector_create(data_bytes.as_ptr(), len as c_uint, error_out) };

        unsafe { add_chat_message_metadata(message_ptr, 0 as c_int, data, error_out) }

        let message = unsafe { Box::from_raw(message_ptr) };
        assert_eq!(message.metadata.len(), 1);
        assert_eq!(message.metadata[0].data, data_bytes);
    }

    #[test]
    fn test_reading_metadata() {
        let address = TariAddress::default();
        let message_ptr = Box::into_raw(Box::new(
            MessageBuilder::new()
                .message("hello".to_string())
                .address(address)
                .build(),
        ));
        let error_out = Box::into_raw(Box::new(0));

        unsafe {
            let data = "hello".to_string();
            let data_bytes = data.as_bytes();
            let len = u32::try_from(data.len()).expect("Can't cast from usize");
            let data = chat_byte_vector_create(data_bytes.as_ptr(), len as c_uint, error_out);

            add_chat_message_metadata(message_ptr, 0 as c_int, data, error_out);

            let chat_ffi_msg =
                ChatFFIMessage::try_from((*message_ptr).clone()).expect("A ChatFFI Message from a Message");
            let chat_ffi_msg_ptr = Box::into_raw(Box::new(chat_ffi_msg));

            let metadata = &(*read_chat_metadata_at_position(chat_ffi_msg_ptr, 0, error_out));

            assert_eq!(metadata.data.0, data_bytes);
        }
    }
}
