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

use libc::{c_char, c_int, c_uint, c_ulonglong};
use tari_chat_client::ChatClient;
use tari_common_types::tari_address::TariAddress;
use tari_contacts::contacts_service::types::{Message, MessageBuilder, MessageMetadata};
use tari_utilities::ByteArray;

use crate::{
    byte_vector::{chat_byte_vector_create, ChatByteVector},
    error::{InterfaceError, LibChatError},
    ChatClientFFI,
};

/// Creates a message and returns a ptr to it
///
/// ## Arguments
/// `receiver` - A string containing a tari address
/// `message` - The peer seeds config for the node
/// `error_out` - Pointer to an int which will be modified
///
/// ## Returns
/// `*mut Message` - A pointer to a message object
///
/// # Safety
/// The ```receiver``` should be destroyed after use
/// The ```Message``` received should be destroyed after use
#[no_mangle]
pub unsafe extern "C" fn create_chat_message(
    receiver: *mut TariAddress,
    message: *const c_char,
    error_out: *mut c_int,
) -> *mut Message {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if receiver.is_null() {
        error = LibChatError::from(InterfaceError::NullError("receiver".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    }

    let message_str = match CStr::from_ptr(message).to_str() {
        Ok(str) => str.to_string(),
        Err(e) => {
            error = LibChatError::from(InterfaceError::InvalidArgument(e.to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return ptr::null_mut();
        },
    };

    let message_out = MessageBuilder::new()
        .address((*receiver).clone())
        .message(message_str)
        .build();

    Box::into_raw(Box::new(message_out))
}

/// Frees memory for Message
///
/// ## Arguments
/// `messages_ptr` - The pointer of a Message
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn destroy_chat_message(ptr: *mut Message) {
    if !ptr.is_null() {
        drop(Box::from_raw(ptr))
    }
}

/// Sends a message over a client
///
/// ## Arguments
/// `client` - The Client pointer
/// `message` - Pointer to a Message struct
/// `error_out` - Pointer to an int which will be modified
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// The ```message``` should be destroyed after use
#[no_mangle]
pub unsafe extern "C" fn send_chat_message(client: *mut ChatClientFFI, message: *mut Message, error_out: *mut c_int) {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if client.is_null() {
        error = LibChatError::from(InterfaceError::NullError("client".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    }

    if message.is_null() {
        error = LibChatError::from(InterfaceError::NullError("message".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    }

    (*client)
        .runtime
        .block_on((*client).client.send_message((*message).clone()));
}

/// Reads the message metadata of a message and returns a ptr to the metadata at the given position
///
/// ## Arguments
/// `message` - A pointer to a Message
/// `position` - The index of the array of metadata
/// `error_out` - Pointer to an int which will be modified
///
/// ## Returns
/// `*mut MessageMetadata` - A pointer to to MessageMetadata
///
/// ## Safety
/// `message` should be destroyed eventually
/// the returned `MessageMetadata` should be destroyed eventually
#[no_mangle]
pub unsafe extern "C" fn chat_metadata_get_at(
    message: *mut Message,
    position: c_uint,
    error_out: *mut c_int,
) -> *mut MessageMetadata {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if message.is_null() {
        error = LibChatError::from(InterfaceError::NullError("message".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    let message = &(*message);

    let len = message.metadata.len() - 1;
    if position as usize > len {
        error = LibChatError::from(InterfaceError::PositionInvalidError).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    let message_metadata_vec = &(*(message).metadata);
    let message_metadata = Box::new(message_metadata_vec[position as usize].clone());

    Box::into_raw(message_metadata)
}

/// Returns the length of the Metadata Vector a chat Message contains
///
/// ## Arguments
/// `message` - A pointer to a Message
/// `error_out` - Pointer to an int which will be modified
///
/// ## Returns
/// `c_int` - The length of the metadata vector for a Message. May return -1 if something goes wrong
///
/// ## Safety
/// `message` should be destroyed eventually
#[no_mangle]
pub unsafe extern "C" fn chat_message_metadata_len(message: *mut Message, error_out: *mut c_int) -> c_int {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if message.is_null() {
        error = LibChatError::from(InterfaceError::NullError("message".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return -1;
    }

    let message = &(*message);
    c_int::try_from(message.metadata.len()).unwrap_or(-1)
}

/// Returns a pointer to a ChatByteVector representing the data of the Message
///
/// ## Arguments
/// `message` - A pointer to a message metadata
/// `error_out` - Pointer to an int which will be modified
///
/// ## Returns
/// `*mut ChatByteVector` - A ptr to a ChatByteVector
///
/// ## Safety
/// `message` should be destroyed eventually
/// the returned `ChatByteVector` should be destroyed eventually
#[no_mangle]
pub unsafe extern "C" fn read_chat_message_body(message: *mut Message, error_out: *mut c_int) -> *mut ChatByteVector {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if message.is_null() {
        error = LibChatError::from(InterfaceError::NullError("message".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    let data = (*message).body.clone();
    let data_bytes = data.as_bytes();
    let len = match c_uint::try_from(data_bytes.len()) {
        Ok(num) => num,
        Err(_e) => {
            error = LibChatError::from(InterfaceError::AllocationError).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return ptr::null_mut();
        },
    };

    chat_byte_vector_create(data_bytes.as_ptr(), len, error_out)
}

/// Returns a pointer to a TariAddress
///
/// ## Arguments
/// `message` - A pointer to a message metadata
/// `error_out` - Pointer to an int which will be modified
///
/// ## Returns
/// `*mut TariAddress` - A ptr to a TariAddress
///
/// ## Safety
/// `message` should be destroyed eventually
/// the returned `TariAddress` should be destroyed eventually
#[no_mangle]
pub unsafe extern "C" fn read_chat_message_address(message: *mut Message, error_out: *mut c_int) -> *mut TariAddress {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if message.is_null() {
        error = LibChatError::from(InterfaceError::NullError("message".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    let address = (*message).address.clone();
    Box::into_raw(Box::new(address))
}

/// Returns a c_char representation of the Direction enum
///
/// ## Arguments
/// `message` - A pointer to a message metadata
/// `error_out` - Pointer to an int which will be modified
///
/// ## Returns
/// `c_char` - A c_uint rep of the direction enum. May return -1 if anything goes wrong
///     0 => Inbound
///     1 => Outbound
///
/// ## Safety
/// `message` should be destroyed eventually
#[no_mangle]
pub unsafe extern "C" fn read_chat_message_direction(message: *mut Message, error_out: *mut c_int) -> c_char {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if message.is_null() {
        error = LibChatError::from(InterfaceError::NullError("message".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return -1;
    }

    c_char::try_from((*message).direction.as_byte()).unwrap_or(-1)
}

/// Returns a c_ulonglong representation of the stored at timestamp as seconds since epoch
///
/// ## Arguments
/// `message` - A pointer to a message metadata
/// `error_out` - Pointer to an int which will be modified
///
/// ## Returns
/// `c_ulonglong` - The stored_at timestamp, seconds since epoch. Returns 0 if message is null.
///
/// ## Safety
/// `message` should be destroyed eventually
#[no_mangle]
pub unsafe extern "C" fn read_chat_message_stored_at(message: *mut Message, error_out: *mut c_int) -> c_ulonglong {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if message.is_null() {
        error = LibChatError::from(InterfaceError::NullError("message".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }

    (*message).stored_at as c_ulonglong
}

/// Returns a c_ulonglong representation of the delivery confirmation timestamp as seconds since epoch
///
/// ## Arguments
/// `message` - A pointer to a message metadata
/// `error_out` - Pointer to an int which will be modified
///
/// ## Returns
/// `c_ulonglong` - The delivery_confirmation_at timestamp, seconds since epoch. Returns 0 if message
/// is null or if no confirmation is stored.
///
/// ## Safety
/// `message` should be destroyed eventually
#[no_mangle]
pub unsafe extern "C" fn read_chat_message_delivery_confirmation_at(
    message: *mut Message,
    error_out: *mut c_int,
) -> c_ulonglong {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if message.is_null() {
        error = LibChatError::from(InterfaceError::NullError("message".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }

    (*message).delivery_confirmation_at.unwrap_or(0) as c_ulonglong
}

/// Returns a c_ulonglong representation of the read confirmation timestamp as seconds since epoch
///
/// ## Arguments
/// `message` - A pointer to a message metadata
/// `error_out` - Pointer to an int which will be modified
///
/// ## Returns
/// `c_ulonglong` - The read_confirmation_at timestamp, seconds since epoch. Returns 0 if message is
/// null or if no confirmation is stored.
///
/// ## Safety
/// `message` should be destroyed eventually
#[no_mangle]
pub unsafe extern "C" fn read_chat_message_read_confirmation_at(
    message: *mut Message,
    error_out: *mut c_int,
) -> c_ulonglong {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if message.is_null() {
        error = LibChatError::from(InterfaceError::NullError("message".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }

    (*message).read_confirmation_at.unwrap_or(0) as c_ulonglong
}

/// Returns a pointer to a ChatByteVector representation of the message_id
///
/// ## Arguments
/// `message` - A pointer to a message metadata
/// `error_out` - Pointer to an int which will be modified
///
/// ## Returns
/// `*mut ChatByteVector` - A ChatByteVector for the message id
///
/// ## Safety
/// `message` should be destroyed eventually
/// The returned ```ChatByteVector``` should be destroyed eventually
#[no_mangle]
pub unsafe extern "C" fn read_chat_message_id(message: *mut Message, error_out: *mut c_int) -> *mut ChatByteVector {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if message.is_null() {
        error = LibChatError::from(InterfaceError::NullError("message".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    let data_bytes = (*message).message_id.clone();
    let len = u32::try_from(data_bytes.len()).expect("Can't cast from usize");
    chat_byte_vector_create(data_bytes.as_ptr(), len as c_uint, error_out)
}

#[cfg(test)]
mod test {
    use tari_contacts::contacts_service::types::{Direction, MessageBuilder};
    use tari_utilities::epoch_time::EpochTime;

    use super::*;
    use crate::{
        byte_vector::{chat_byte_vector_destroy, chat_byte_vector_get_at, chat_byte_vector_get_length},
        message::read_chat_message_id,
        tari_address::destroy_tari_address,
    };

    #[test]
    fn test_reading_message_id() {
        let message = MessageBuilder::new().build();

        let message_ptr = Box::into_raw(Box::new(message.clone()));
        let error_out = Box::into_raw(Box::new(0));

        unsafe {
            let message_byte_vector = read_chat_message_id(message_ptr, error_out);
            let len = chat_byte_vector_get_length(message_byte_vector, error_out);

            let mut message_id = vec![];
            for i in 0..len {
                message_id.push(chat_byte_vector_get_at(message_byte_vector, i, error_out));
            }

            assert_eq!(message.message_id, message_id);

            destroy_chat_message(message_ptr);
            chat_byte_vector_destroy(message_byte_vector);
            drop(Box::from_raw(error_out));
        }
    }

    #[test]
    fn test_reading_message_body() {
        let body = "Hey there!";
        let body_bytes = body.as_bytes();
        let message = MessageBuilder::new().message(body.into()).build();

        let message_ptr = Box::into_raw(Box::new(message));
        let error_out = Box::into_raw(Box::new(0));

        unsafe {
            let message_byte_vector = read_chat_message_body(message_ptr, error_out);
            let len = chat_byte_vector_get_length(message_byte_vector, error_out);

            let mut message_body = vec![];
            for i in 0..len {
                message_body.push(chat_byte_vector_get_at(message_byte_vector, i, error_out));
            }

            assert_eq!(body_bytes, message_body);

            destroy_chat_message(message_ptr);
            chat_byte_vector_destroy(message_byte_vector);
            drop(Box::from_raw(error_out));
        }
    }

    #[test]
    fn test_reading_message_address() {
        let address =
            TariAddress::from_hex("0c017c5cd01385f34ac065e3b05948326dc55d2494f120c6f459a07389011b4ec1").unwrap();
        let message = MessageBuilder::new().address(address.clone()).build();

        let message_ptr = Box::into_raw(Box::new(message));
        let error_out = Box::into_raw(Box::new(0));

        unsafe {
            let address_ptr = read_chat_message_address(message_ptr, error_out);

            assert_eq!(address.to_bytes(), (*address_ptr).to_bytes());

            destroy_chat_message(message_ptr);
            destroy_tari_address(address_ptr);
            drop(Box::from_raw(error_out));
        }
    }

    #[test]
    fn test_reading_message_direction() {
        let error_out = Box::into_raw(Box::new(0));

        unsafe {
            let message = MessageBuilder::new().build();
            let message_ptr = Box::into_raw(Box::new(message));
            let direction = read_chat_message_direction(message_ptr, error_out);
            assert_eq!(1, direction); // Default Outbound => 1
            destroy_chat_message(message_ptr);
        };

        unsafe {
            let message = Message {
                direction: Direction::Inbound,
                ..Message::default()
            };
            let message_ptr = Box::into_raw(Box::new(message));
            let direction = read_chat_message_direction(message_ptr, error_out);
            assert_eq!(0, direction); // Default Inbound => 0
            destroy_chat_message(message_ptr);
        };

        unsafe {
            drop(Box::from_raw(error_out));
        }
    }

    #[test]
    fn test_reading_message_timestamps() {
        let error_out = Box::into_raw(Box::new(0));

        unsafe {
            let timestamp = EpochTime::now().as_u64();
            let message = Message {
                stored_at: timestamp.clone(),
                delivery_confirmation_at: None,
                read_confirmation_at: None,
                ..Message::default()
            };

            let message_ptr = Box::into_raw(Box::new(message));

            let stored_at = read_chat_message_stored_at(message_ptr, error_out);
            assert_eq!(timestamp, stored_at);

            let delivered_at = read_chat_message_delivery_confirmation_at(message_ptr, error_out);
            assert_eq!(0, delivered_at);

            let read_at = read_chat_message_read_confirmation_at(message_ptr, error_out);
            assert_eq!(0, read_at);

            destroy_chat_message(message_ptr);
        };

        unsafe {
            let timestamp = EpochTime::now().as_u64();
            let message = Message {
                stored_at: timestamp.clone(),
                delivery_confirmation_at: Some(timestamp.clone()),
                read_confirmation_at: Some(timestamp.clone()),
                ..Message::default()
            };

            let message_ptr = Box::into_raw(Box::new(message));

            let stored_at = read_chat_message_stored_at(message_ptr, error_out);
            assert_eq!(timestamp, stored_at);

            let delivered_at = read_chat_message_delivery_confirmation_at(message_ptr, error_out);
            assert_eq!(timestamp, delivered_at);

            let read_at = read_chat_message_read_confirmation_at(message_ptr, error_out);
            assert_eq!(timestamp, read_at);

            destroy_chat_message(message_ptr);
        };

        unsafe {
            drop(Box::from_raw(error_out));
        }
    }
}
