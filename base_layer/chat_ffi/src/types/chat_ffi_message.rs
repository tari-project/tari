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

use libc::{c_char, c_int, c_uchar};
use tari_contacts::contacts_service::types::Message;

use crate::{
    message_metadata::ChatFFIMessageMetadata,
    types::{ChatByteVector, ChatMessageMetadataVector},
};

#[repr(C)]
pub struct ChatFFIMessage {
    pub body: *const c_char,
    pub from_address: *const c_char,
    pub stored_at: u64,
    pub message_id: *const c_char,
    pub metadata: ChatMessageMetadataVector,
    pub metadata_len: c_int,
}

impl TryFrom<Message> for ChatFFIMessage {
    type Error = String;

    fn try_from(v: Message) -> Result<Self, Self::Error> {
        let body = match CString::new(v.body) {
            Ok(s) => s,
            Err(e) => return Err(e.to_string()),
        };

        let address = match CString::new(v.address.to_hex()) {
            Ok(s) => s,
            Err(e) => return Err(e.to_string()),
        };

        let id = match CString::new(v.message_id) {
            Ok(s) => s,
            Err(e) => return Err(e.to_string()),
        };

        let mut chat_message_metadata_bytes = vec![];
        for md in v.metadata.clone() {
            chat_message_metadata_bytes.push(ChatFFIMessageMetadata {
                data: ChatByteVector(md.data.clone().into_iter().map(|f| f as c_uchar).collect()),
                metadata_type: md.metadata_type.as_byte() as c_int,
            });
        }

        Ok(Self {
            body: body.as_ptr(),
            from_address: address.as_ptr(),
            stored_at: v.stored_at,
            message_id: id.as_ptr(),
            metadata: ChatMessageMetadataVector(chat_message_metadata_bytes),
            metadata_len: v.metadata.len() as c_int,
        })
    }
}

/// Frees memory for a ChatFFIMessage
///
/// ## Arguments
/// `transport` - The pointer to a ChatFFIMessage
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn destroy_chat_ffi_message(address: *mut ChatFFIMessage) {
    if !address.is_null() {
        drop(Box::from_raw(address))
    }
}
