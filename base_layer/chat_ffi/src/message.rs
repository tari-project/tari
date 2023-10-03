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
use tari_chat_client::ChatClient;
use tari_common_types::tari_address::TariAddress;
use tari_contacts::contacts_service::{
    handle::{DEFAULT_MESSAGE_LIMIT, DEFAULT_MESSAGE_PAGE},
    types::{Message, MessageBuilder},
};

use crate::{
    error::{InterfaceError, LibChatError},
    types::ChatMessages,
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

/// Frees memory for message
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
pub unsafe extern "C" fn destroy_chat_message(messages_ptr: *mut Message) {
    if !messages_ptr.is_null() {
        drop(Box::from_raw(messages_ptr))
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

/// Get a ptr to all messages from or to address
///
/// ## Arguments
/// `client` - The Client pointer
/// `address` - A TariAddress ptr
/// `limit` - The amount of messages you want to fetch. Default to 35, max 2500
/// `page` - The page of results you'd like returned. Default to 0, maximum of u64 max
/// `error_out` - Pointer to an int which will be modified
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// The ```address``` should be destroyed after use
/// The returned pointer to ```*mut ChatMessages``` should be destroyed after use
#[no_mangle]
pub unsafe extern "C" fn get_chat_messages(
    client: *mut ChatClientFFI,
    address: *mut TariAddress,
    limit: c_int,
    page: c_int,
    error_out: *mut c_int,
) -> *mut ChatMessages {
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

    let mlimit = u64::try_from(limit).unwrap_or(DEFAULT_MESSAGE_LIMIT);
    let mpage = u64::try_from(page).unwrap_or(DEFAULT_MESSAGE_PAGE);

    let mut messages = Vec::new();

    let mut retrieved_messages = (*client)
        .runtime
        .block_on((*client).client.get_messages(&*address, mlimit, mpage));
    messages.append(&mut retrieved_messages);

    Box::into_raw(Box::new(ChatMessages(messages)))
}

/// Frees memory for messages
///
/// ## Arguments
/// `ptr` - The pointer of a Message
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn destroy_chat_messages(ptr: *mut ChatMessages) {
    if !ptr.is_null() {
        drop(Box::from_raw(ptr))
    }
}
