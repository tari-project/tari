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

use crate::{
    error::{InterfaceError, LibChatError},
    ChatClient,
};

#[derive(Debug, PartialEq, Clone)]
pub struct ConversationalistsVector(pub Vec<TariAddress>);

/// Return a ptr to a ConversationalistsVector
///
/// ## Arguments
/// `client` - The ChatClient
/// `error_out` - Pointer to an int which will be modified
///
/// ## Returns
/// `*mut ptr ConversationalistsVector` - a pointer to a ConversationalistsVector
///
/// ## Safety
/// The `ConversationalistsVector` should be destroyed after use
#[no_mangle]
pub unsafe extern "C" fn get_conversationalists(
    client: *mut ChatClient,
    error_out: *mut c_int,
) -> *mut ConversationalistsVector {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if client.is_null() {
        error = LibChatError::from(InterfaceError::NullError("client".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    }

    let result = (*client).runtime.block_on((*client).client.get_conversationalists());

    match result {
        Ok(conversationalists) => Box::into_raw(Box::new(ConversationalistsVector(conversationalists))),
        Err(e) => {
            error = LibChatError::from(InterfaceError::ContactServiceError(e.to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            ptr::null_mut()
        },
    }
}

/// Returns the length of the ConversationalistsVector
///
/// ## Arguments
/// `conversationalists` - A pointer to a ConversationalistsVector
/// `error_out` - Pointer to an int which will be modified
///
/// ## Returns
/// `c_int` - The length of the vector. May return -1 if something goes wrong
///
/// ## Safety
/// `conversationalists` should be destroyed eventually
#[no_mangle]
pub unsafe extern "C" fn conversationalists_vector_len(
    conversationalists: *mut ConversationalistsVector,
    error_out: *mut c_int,
) -> c_int {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if conversationalists.is_null() {
        error = LibChatError::from(InterfaceError::NullError("conversationalists".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return -1;
    }

    let conversationalists = &(*conversationalists);
    c_int::try_from(conversationalists.0.len()).unwrap_or(-1)
}

/// Reads the ConversationalistsVector and returns a pointer to a TariAddress at a given position
///
/// ## Arguments
/// `conversationalists` - A pointer to a ConversationalistsVector
/// `position` - The index of the vector for a TariAddress
/// `error_out` - Pointer to an int which will be modified
///
/// ## Returns
/// `*mut ptr TariAddress` - A pointer to a TariAddress
///
/// ## Safety
/// `conversationalists` should be destroyed eventually
/// the returned `TariAddress` should be destroyed eventually
#[no_mangle]
pub unsafe extern "C" fn conversationalists_vector_get_at(
    conversationalists: *mut ConversationalistsVector,
    position: c_uint,
    error_out: *mut c_int,
) -> *mut TariAddress {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if conversationalists.is_null() {
        error = LibChatError::from(InterfaceError::NullError("conversationalists".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    let conversationalists = &(*conversationalists);

    let len = conversationalists.0.len();
    let position = position as usize;
    if conversationalists.0.is_empty() || position > len - 1 {
        error = LibChatError::from(InterfaceError::PositionInvalidError).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    Box::into_raw(Box::new(conversationalists.0[position].clone()))
}

/// Frees memory for ConversationalistsVector
///
/// ## Arguments
/// `ptr` - The pointer of a ConversationalistsVector
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn destroy_conversationalists_vector(ptr: *mut ConversationalistsVector) {
    if !ptr.is_null() {
        drop(Box::from_raw(ptr))
    }
}

#[cfg(test)]
mod test {
    use rand::rngs::OsRng;
    use tari_common::configuration::Network;
    use tari_common_types::types::PublicKey;
    use tari_crypto::keys::PublicKey as PubKeyTrait;

    use super::*;
    use crate::tari_address::destroy_tari_address;

    #[test]
    fn test_retrieving_conversationalists_from_vector() {
        let (_, pk) = PublicKey::random_keypair(&mut OsRng);
        let a = TariAddress::from_public_key(&pk, Network::LocalNet);
        let conversationalists =
            ConversationalistsVector(vec![TariAddress::default(), TariAddress::default(), a.clone()]);

        let conversationalists_len = conversationalists.0.len();
        let conversationalist_vector_ptr = Box::into_raw(Box::new(conversationalists));
        let error_out = Box::into_raw(Box::new(0));

        unsafe {
            let conversationalist_vector_len = conversationalists_vector_len(conversationalist_vector_ptr, error_out);
            assert_eq!(conversationalist_vector_len as usize, conversationalists_len);

            let address_ptr = conversationalists_vector_get_at(conversationalist_vector_ptr, 2, error_out);
            let address = (*address_ptr).clone();
            assert_eq!(a, address);

            destroy_conversationalists_vector(conversationalist_vector_ptr);
            destroy_tari_address(address_ptr);
            drop(Box::from_raw(error_out));
        }
    }
}
