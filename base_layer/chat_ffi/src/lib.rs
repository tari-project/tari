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

#![recursion_limit = "1024"]

use std::ptr;

use callback_handler::CallbackContactStatusChange;
use libc::c_int;
use log::info;
use minotari_app_utilities::identity_management::setup_node_identity;
use tari_chat_client::{config::ApplicationConfig, networking::PeerFeatures, ChatClient, Client};
use tokio::runtime::Runtime;

use crate::{
    callback_handler::{
        CallbackDeliveryConfirmationReceived,
        CallbackHandler,
        CallbackMessageReceived,
        CallbackReadConfirmationReceived,
    },
    error::{InterfaceError, LibChatError},
    logging::init_logging,
};

mod application_config;
mod byte_vector;
mod callback_handler;
mod confirmation;
mod contacts;
mod contacts_liveness_data;
mod error;
mod logging;
mod message;
mod message_metadata;
mod messages;
mod tansport_config;
mod tari_address;

const LOG_TARGET: &str = "chat_ffi";

mod consts {
    // Import the auto-generated const values from the Manifest and Git
    include!(concat!(env!("OUT_DIR"), "/consts.rs"));
}

pub struct ChatClientFFI {
    client: Client,
    runtime: Runtime,
}

/// Creates a Chat Client
///
/// ## Arguments
/// `config` - The ApplicationConfig pointer
/// `error_out` - Pointer to an int which will be modified
/// `callback_contact_status_change` - A callback function pointer. this is called whenever a
/// contacts liveness event comes in.
/// `callback_message_received` - A callback function pointer. This is called whenever a chat
/// message is received.
/// `callback_delivery_confirmation_received` - A callback function pointer. This is called when the
/// client receives a confirmation of message delivery.
/// `callback_read_confirmation_received` - A callback function pointer. This is called when the
/// client receives a confirmation of message read.
///
/// ## Returns
/// `*mut ChatClient` - Returns a pointer to a ChatClient, note that it returns ptr::null_mut()
/// if any error was encountered or if the runtime could not be created.
///
/// # Safety
/// The ```destroy_chat_client``` method must be called when finished with a ClientFFI to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn create_chat_client(
    config: *mut ApplicationConfig,
    error_out: *mut c_int,
    callback_contact_status_change: CallbackContactStatusChange,
    callback_message_received: CallbackMessageReceived,
    callback_delivery_confirmation_received: CallbackDeliveryConfirmationReceived,
    callback_read_confirmation_received: CallbackReadConfirmationReceived,
) -> *mut ChatClientFFI {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if config.is_null() {
        error = LibChatError::from(InterfaceError::NullError("config".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    if let Some(log_path) = (*config).clone().chat_client.log_path {
        init_logging(log_path, (*config).clone().chat_client.log_verbosity, error_out);

        if error > 0 {
            return ptr::null_mut();
        }
    }
    info!(
        target: LOG_TARGET,
        "Starting Tari Chat FFI version: {}",
        consts::APP_VERSION
    );

    let mut bad_identity = |e| {
        error = LibChatError::from(InterfaceError::InvalidArgument(e)).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    };

    let identity = match setup_node_identity(
        (*config).chat_client.identity_file.clone(),
        (*config).chat_client.p2p.public_addresses.clone().into_vec(),
        true,
        PeerFeatures::COMMUNICATION_NODE,
    ) {
        Ok(node_id) => node_id,
        _ => {
            bad_identity("No identity loaded".to_string());
            return ptr::null_mut();
        },
    };

    let runtime = match Runtime::new() {
        Ok(r) => r,
        Err(e) => {
            error = LibChatError::from(InterfaceError::TokioError(e.to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return ptr::null_mut();
        },
    };

    let mut client = Client::new(identity, (*config).clone());
    runtime.block_on(client.initialize());

    let mut callback_handler = CallbackHandler::new(
        client.contacts.clone().expect("No contacts service loaded yet"),
        client.shutdown.to_signal(),
        callback_contact_status_change,
        callback_message_received,
        callback_delivery_confirmation_received,
        callback_read_confirmation_received,
    );

    runtime.spawn(async move {
        callback_handler.start().await;
    });

    let client_ffi = ChatClientFFI { client, runtime };

    Box::into_raw(Box::new(client_ffi))
}

/// Frees memory for a ChatClientFFI
///
/// ## Arguments
/// `client` - The pointer of a ChatClientFFI
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn destroy_chat_client_ffi(client: *mut ChatClientFFI) {
    if !client.is_null() {
        let mut c = Box::from_raw(client);
        c.client.shutdown();
    }
}
