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
use tari_contacts::contacts_service::types::{Message, MessageMetadata};
use tari_utilities::ByteArray;

use crate::{
    byte_vector::{chat_byte_vector_create, process_vector, ChatByteVector},
    error::{InterfaceError, LibChatError},
};

/// Creates message metadata and appends it to a Message
///
/// ## Arguments
/// `message` - A pointer to a message
/// `key` - A pointer to a byte vector containing bytes for the key field
/// `data` - A pointer to a byte vector containing bytes for the data field
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
    key: *mut ChatByteVector,
    data: *mut ChatByteVector,
    error_out: *mut c_int,
) {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    for (name, d) in [("key", key), ("data", data)] {
        if d.is_null() {
            error = LibChatError::from(InterfaceError::NullError(name.to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return;
        }
    }

    let metadata = MessageMetadata {
        key: process_vector(key, error_out),
        data: process_vector(data, error_out),
    };

    (*message).push(metadata);
}

/// Returns the c_int representation of a metadata type enum
///
/// ## Arguments
/// `msg_metadata` - A pointer to a MessageMetadata
/// `error_out` - Pointer to an int which will be modified
///
/// ## Returns
/// `*mut ChatByteVector` - A ptr to a ChatByteVector
///
/// ## Safety
/// `msg_metadata` should be destroyed eventually
/// the returned `ChatByteVector` should be destroyed eventually
#[no_mangle]
pub unsafe extern "C" fn read_chat_metadata_key(
    msg_metadata: *mut MessageMetadata,
    error_out: *mut c_int,
) -> *mut ChatByteVector {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if msg_metadata.is_null() {
        error = LibChatError::from(InterfaceError::NullError("message".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    let data = (*msg_metadata).key.clone();
    let data_bytes = data.as_bytes();
    let len = match c_uint::try_from(data_bytes.len()) {
        Ok(num) => num,
        Err(_e) => {
            error = LibChatError::from(InterfaceError::PositionInvalidError).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return ptr::null_mut();
        },
    };

    chat_byte_vector_create(data_bytes.as_ptr(), len, error_out)
}

/// Returns a ptr to a ByteVector
///
/// ## Arguments
/// `msg_metadata` - A pointer to a MessageMetadata
/// `error_out` - Pointer to an int which will be modified
///
/// ## Returns
/// `*mut ChatByteVector` - A ptr to a ChatByteVector
///
/// ## Safety
/// `msg_metadata` should be destroyed eventually
/// the returned `ChatByteVector` should be destroyed eventually
#[no_mangle]
pub unsafe extern "C" fn read_chat_metadata_data(
    msg_metadata: *mut MessageMetadata,
    error_out: *mut c_int,
) -> *mut ChatByteVector {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if msg_metadata.is_null() {
        error = LibChatError::from(InterfaceError::NullError("message".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    let data = (*msg_metadata).data.clone();
    let data_bytes = data.as_bytes();
    let len = match c_uint::try_from(data_bytes.len()) {
        Ok(num) => num,
        Err(_e) => {
            error = LibChatError::from(InterfaceError::PositionInvalidError).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return ptr::null_mut();
        },
    };

    chat_byte_vector_create(data_bytes.as_ptr(), len, error_out)
}

/// Frees memory for MessageMetadata
///
/// ## Arguments
/// `ptr` - The pointer of a MessageMetadata
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn destroy_chat_message_metadata(ptr: *mut MessageMetadata) {
    if !ptr.is_null() {
        drop(Box::from_raw(ptr))
    }
}

#[cfg(test)]
mod test {
    use std::convert::TryFrom;

    use libc::c_uint;
    use tari_common_types::tari_address::TariAddress;
    use tari_contacts::contacts_service::types::MessageBuilder;

    use super::*;
    use crate::{
        byte_vector::{
            chat_byte_vector_create,
            chat_byte_vector_destroy,
            chat_byte_vector_get_at,
            chat_byte_vector_get_length,
        },
        message::{chat_metadata_get_at, destroy_chat_message},
    };

    #[test]
    fn test_metadata_adding() {
        let message_ptr = Box::into_raw(Box::default());
        let error_out = Box::into_raw(Box::new(0));

        let data = "hello".to_string();
        let data_bytes = data.as_bytes();
        let data_len = u32::try_from(data.len()).expect("Can't cast from usize");
        let data = unsafe { chat_byte_vector_create(data_bytes.as_ptr(), data_len as c_uint, error_out) };

        let key = "gif".to_string();
        let key_bytes = key.as_bytes();
        let key_len = u32::try_from(key.len()).expect("Can't cast from usize");
        let key = unsafe { chat_byte_vector_create(key_bytes.as_ptr(), key_len as c_uint, error_out) };

        unsafe { add_chat_message_metadata(message_ptr, key, data, error_out) }

        let message = unsafe { Box::from_raw(message_ptr) };
        assert_eq!(message.metadata.len(), 1);
        assert_eq!(message.metadata[0].data, data_bytes);

        unsafe {
            chat_byte_vector_destroy(data);
            drop(Box::from_raw(error_out));
        }
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
            let data = "metadata".to_string();
            let data_bytes = data.as_bytes();
            let len = u32::try_from(data.len()).expect("Can't cast from usize");
            let data = chat_byte_vector_create(data_bytes.as_ptr(), len as c_uint, error_out);

            let key = "gif".to_string();
            let key_bytes = key.as_bytes();
            let len = u32::try_from(key.len()).expect("Can't cast from usize");
            let key = chat_byte_vector_create(key_bytes.as_ptr(), len as c_uint, error_out);

            add_chat_message_metadata(message_ptr, key, data, error_out);

            let metadata_ptr = chat_metadata_get_at(message_ptr, 0, error_out);

            let metadata_key_vector = read_chat_metadata_key(metadata_ptr, error_out);
            let metadata_key_vector_len = chat_byte_vector_get_length(metadata_key_vector, error_out);

            let mut metadata_key = vec![];

            for i in 0..metadata_key_vector_len {
                metadata_key.push(chat_byte_vector_get_at(metadata_key_vector, i, error_out));
            }

            let metadata_data_vector = read_chat_metadata_data(metadata_ptr, error_out);
            let metadata_data_vector_len = chat_byte_vector_get_length(metadata_data_vector, error_out);

            let mut metadata_data = vec![];

            for i in 0..metadata_data_vector_len {
                metadata_data.push(chat_byte_vector_get_at(metadata_data_vector, i, error_out));
            }

            assert_eq!(metadata_key, key_bytes);
            assert_eq!(metadata_data, data_bytes);

            destroy_chat_message_metadata(metadata_ptr);
            destroy_chat_message(message_ptr);
            chat_byte_vector_destroy(metadata_key_vector);
            chat_byte_vector_destroy(metadata_data_vector);
            drop(Box::from_raw(error_out));
        }
    }
}
