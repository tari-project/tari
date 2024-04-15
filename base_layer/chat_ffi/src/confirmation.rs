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

use libc::{c_int, c_uint, c_ulonglong};
use tari_chat_client::ChatClient as ChatClientTrait;
use tari_contacts::contacts_service::types::{Confirmation, Message};

use crate::{
    byte_vector::{chat_byte_vector_create, ChatByteVector},
    error::{InterfaceError, LibChatError},
    ChatClient,
};

/// Send a read confirmation for a given message
///
/// ## Arguments
/// `client` - Pointer to the ChatClient
/// `message` - Pointer to the Message that was read
/// `error_out` - Pointer to an int which will be modified
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// The `client` When done with the ChatClient it should be destroyed
/// The `message` When done with the Message it should be destroyed
#[no_mangle]
pub unsafe extern "C" fn send_read_confirmation_for_message(
    client: *mut ChatClient,
    message: *mut Message,
    error_out: *mut c_int,
) {
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

    let result = (*client)
        .runtime
        .block_on((*client).client.send_read_receipt((*message).clone()));

    if let Err(e) = result {
        error = LibChatError::from(InterfaceError::ContactServiceError(e.to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    }
}

/// Get a pointer to a ChatByteVector representation of the message id associated to the confirmation
///
/// ## Arguments
/// `confirmation` - A pointer to the Confirmation you'd like to read from
/// `error_out` - Pointer to an int which will be modified
///
/// ## Returns
/// `*mut ChatByteVector` - A ptr to a ChatByteVector
///
/// # Safety
/// `confirmation` should be destroyed when finished
/// ```ChatByteVector``` When done with the returned ChatByteVector it should be destroyed
#[no_mangle]
pub unsafe extern "C" fn read_confirmation_message_id(
    confirmation: *mut Confirmation,
    error_out: *mut c_int,
) -> *mut ChatByteVector {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if confirmation.is_null() {
        error = LibChatError::from(InterfaceError::NullError("client".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    }

    let c = &(*confirmation);
    let data_bytes = c.message_id.clone();

    let len = match u32::try_from(data_bytes.len()) {
        Ok(l) => l,
        Err(e) => {
            error = LibChatError::from(InterfaceError::ConversionError(e.to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            0
        },
    };

    chat_byte_vector_create(data_bytes.as_ptr(), len as c_uint, error_out)
}

/// Get a c_ulonglong timestamp for the Confirmation
///
/// ## Arguments
/// `confirmation` - A pointer to the Confirmation
/// `error_out` - Pointer to an int which will be modified
///
/// ## Returns
/// `c_ulonglong` - A uint representation of time since epoch. May return 0 on error
///
/// # Safety
/// The ```confirmation``` When done with the Confirmation it should be destroyed
#[no_mangle]
pub unsafe extern "C" fn read_confirmation_timestamp(
    confirmation: *mut Confirmation,
    error_out: *mut c_int,
) -> c_ulonglong {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if confirmation.is_null() {
        error = LibChatError::from(InterfaceError::NullError("client".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    }

    (*confirmation).timestamp as c_ulonglong
}

/// Frees memory for a Confirmation
///
/// ## Arguments
/// `ptr` - The pointer of a Confirmation
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn destroy_confirmation(ptr: *mut Confirmation) {
    if !ptr.is_null() {
        drop(Box::from_raw(ptr))
    }
}

#[cfg(test)]
mod test {
    use tari_contacts::contacts_service::types::{Confirmation, MessageBuilder};
    use tari_utilities::epoch_time::EpochTime;

    use crate::{
        byte_vector::{chat_byte_vector_get_at, chat_byte_vector_get_length},
        confirmation::{destroy_confirmation, read_confirmation_message_id, read_confirmation_timestamp},
    };

    #[test]
    fn test_reading_from_confrimation() {
        let message_id = MessageBuilder::new().build().message_id;
        let timestamp = EpochTime::now().as_u64();
        let confirmation = Confirmation {
            message_id: message_id.clone(),
            timestamp,
        };

        let confirmation_ptr = Box::into_raw(Box::new(confirmation));
        let error_out = Box::into_raw(Box::new(0));

        unsafe {
            let id_byte_vec = read_confirmation_message_id(confirmation_ptr, error_out);
            let len = chat_byte_vector_get_length(id_byte_vec, error_out);

            let mut read_id = vec![];
            for i in 0..len {
                read_id.push(chat_byte_vector_get_at(id_byte_vec, i, error_out));
            }

            assert_eq!(message_id, read_id)
        }

        unsafe {
            let read_timestamp = read_confirmation_timestamp(confirmation_ptr, error_out);
            assert_eq!(timestamp, read_timestamp as u64)
        }

        unsafe { destroy_confirmation(confirmation_ptr) }
    }
}
