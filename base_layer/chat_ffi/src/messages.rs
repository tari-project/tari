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
use tari_chat_client::ChatClient as ChatClientTrait;
use tari_common_types::tari_address::TariAddress;
use tari_contacts::contacts_service::types::Message;

use crate::{
    error::{InterfaceError, LibChatError},
    ChatClient,
};

#[derive(Clone)]
pub struct MessageVector(pub Vec<Message>);

/// Get a ptr to all messages from or to an address
///
/// ## Arguments
/// `client` - The ChatClient pointer
/// `address` - A TariAddress pointer
/// `limit` - The amount of messages you want to fetch. Default to 35, max 2500
/// `page` - The page of results you'd like returned. Default to 0, maximum of u64 max
/// `error_out` - Pointer to an int which will be modified
///
/// ## Returns
/// `*mut MessageVector` - A pointer to a Vector of Messages
///
/// # Safety
/// The returned pointer to ```MessageVector``` should be destroyed after use
/// ```client``` should be destroyed after use
/// ```address``` should be destroyed after use
#[no_mangle]
pub unsafe extern "C" fn get_chat_messages(
    client: *mut ChatClient,
    address: *mut TariAddress,
    limit: c_uint,
    page: c_uint,
    error_out: *mut c_int,
) -> *mut MessageVector {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if client.is_null() {
        error = LibChatError::from(InterfaceError::NullError("client".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    }

    if address.is_null() {
        error = LibChatError::from(InterfaceError::NullError("address".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    }

    let mlimit = u64::from(limit);
    let mpage = u64::from(page);

    let result = (*client)
        .runtime
        .block_on((*client).client.get_messages(&*address, mlimit, mpage));

    match result {
        Ok(messages) => Box::into_raw(Box::new(MessageVector(messages))),
        Err(e) => {
            error = LibChatError::from(InterfaceError::ContactServiceError(e.to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            ptr::null_mut()
        },
    }
}

/// Returns the length of the MessageVector
///
/// ## Arguments
/// `messages` - A pointer to a MessageVector
/// `error_out` - Pointer to an int which will be modified
///
/// ## Returns
/// `c_uint` - The length of the metadata vector for a Message. May return 0 if something goes wrong
///
/// ## Safety
/// `messages` should be destroyed eventually
#[no_mangle]
pub unsafe extern "C" fn message_vector_len(messages: *mut MessageVector, error_out: *mut c_int) -> c_uint {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if messages.is_null() {
        error = LibChatError::from(InterfaceError::NullError("message".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }

    let messages = &(*messages);
    match c_uint::try_from(messages.0.len()) {
        Ok(l) => l,
        Err(e) => {
            error = LibChatError::from(InterfaceError::ConversionError(e.to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            0
        },
    }
}

/// Reads the MessageVector and returns a Message at a given position
///
/// ## Arguments
/// `messages` - A pointer to a MessageVector
/// `position` - The index of the vector for a Message
/// `error_out` - Pointer to an int which will be modified
///
/// ## Returns
/// `*mut ptr Message` - A pointer to a Message
///
/// ## Safety
/// `messages` should be destroyed eventually
/// the returned `Message` should be destroyed eventually
#[no_mangle]
pub unsafe extern "C" fn message_vector_get_at(
    messages: *mut MessageVector,
    position: c_uint,
    error_out: *mut c_int,
) -> *mut Message {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if messages.is_null() {
        error = LibChatError::from(InterfaceError::NullError("message".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    let messages = &(*messages);
    let position = position as usize;
    let len = messages.0.len();

    if messages.0.is_empty() || position > len - 1 {
        error = LibChatError::from(InterfaceError::PositionInvalidError).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    Box::into_raw(Box::new(messages.0[position].clone()))
}

/// Frees memory for MessagesVector
///
/// ## Arguments
/// `ptr` - The pointer of a MessagesVector
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn destroy_message_vector(ptr: *mut MessageVector) {
    if !ptr.is_null() {
        drop(Box::from_raw(ptr))
    }
}

#[cfg(test)]
mod test {
    use tari_contacts::contacts_service::types::MessageBuilder;

    use super::*;
    use crate::{
        byte_vector::{chat_byte_vector_destroy, chat_byte_vector_get_at, chat_byte_vector_get_length},
        message::{destroy_chat_message, read_chat_message_id},
    };

    #[test]
    fn test_retrieving_messages_from_vector() {
        let m = MessageBuilder::new().message("hello 2".to_string()).build();
        let messages = MessageVector(vec![
            MessageBuilder::new().message("hello 0".to_string()).build(),
            MessageBuilder::new().message("hello 1".to_string()).build(),
            m.clone(),
            MessageBuilder::new().message("hello 3".to_string()).build(),
            MessageBuilder::new().message("hello 4".to_string()).build(),
        ]);

        let messages_len = messages.0.len();
        let message_vector_ptr = Box::into_raw(Box::new(messages));
        let error_out = Box::into_raw(Box::new(0));

        unsafe {
            let message_vector_len = message_vector_len(message_vector_ptr, error_out);
            assert_eq!(message_vector_len as usize, messages_len);

            let message_ptr = message_vector_get_at(message_vector_ptr, 2, error_out);
            let message_byte_vector = read_chat_message_id(message_ptr, error_out);
            let len = chat_byte_vector_get_length(message_byte_vector, error_out);

            let mut message_id = vec![];
            for i in 0..len {
                message_id.push(chat_byte_vector_get_at(message_byte_vector, i, error_out));
            }

            assert_eq!(m.message_id, message_id);

            destroy_message_vector(message_vector_ptr);
            destroy_chat_message(message_ptr);
            chat_byte_vector_destroy(message_byte_vector);
            drop(Box::from_raw(error_out));
        }
    }
}
