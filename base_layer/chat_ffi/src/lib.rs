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

use std::{convert::TryFrom, ffi::CStr, num::NonZeroU16, path::PathBuf, ptr, str::FromStr};

use callback_handler::CallbackContactStatusChange;
use libc::{c_char, c_int, c_uchar, c_ushort};
use log::{debug, info, warn, LevelFilter};
use log4rs::{
    append::{
        rolling_file::{
            policy::compound::{roll::fixed_window::FixedWindowRoller, trigger::size::SizeTrigger, CompoundPolicy},
            RollingFileAppender,
        },
        Append,
    },
    config::{Appender, Config, Logger, Root},
    encode::pattern::PatternEncoder,
};
use minotari_app_utilities::identity_management::setup_node_identity;
use tari_chat_client::{
    config::{ApplicationConfig, ChatClientConfig},
    networking::PeerFeatures,
    ChatClient,
    Client,
};
use tari_common::configuration::{MultiaddrList, Network};
use tari_common_types::tari_address::TariAddress;
use tari_comms::multiaddr::Multiaddr;
use tari_contacts::contacts_service::{
    handle::{DEFAULT_MESSAGE_LIMIT, DEFAULT_MESSAGE_PAGE},
    types::Message,
};
use tari_p2p::{SocksAuthentication, TorControlAuthentication, TorTransportConfig, TransportConfig, TransportType};
use tari_utilities::hex;
use tokio::runtime::Runtime;

use crate::{
    callback_handler::{CallbackHandler, CallbackMessageReceived, ChatFFIContactsLivenessData, ChatFFIMessage},
    error::{InterfaceError, LibChatError},
};

mod callback_handler;
mod error;

const LOG_TARGET: &str = "chat_ffi";

mod consts {
    // Import the auto-generated const values from the Manifest and Git
    include!(concat!(env!("OUT_DIR"), "/consts.rs"));
}

#[derive(Debug, PartialEq, Clone)]
pub struct ChatByteVector(Vec<c_uchar>); // declared like this so that it can be exposed to external header

#[derive(Clone)]
pub struct ChatMessages(Vec<Message>);

pub struct ChatClientFFI {
    client: Client,
    runtime: Runtime,
}

/// Creates a Chat Client
///
/// ## Arguments
/// `config` - The ApplicationConfig pointer
/// `identity_file_path` - The path to the node identity file
/// `error_out` - Pointer to an int which will be modified
///
/// ## Returns
/// `*mut ChatClient` - Returns a pointer to a ChatClient, note that it returns ptr::null_mut()
/// if any error was encountered or if the runtime could not be created.
///
/// # Safety
/// The ```destroy_client``` method must be called when finished with a ClientFFI to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn create_chat_client(
    config: *mut ApplicationConfig,
    error_out: *mut c_int,
    callback_contact_status_change: CallbackContactStatusChange,
    callback_message_received: CallbackMessageReceived,
) -> *mut ChatClientFFI {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if config.is_null() {
        error = LibChatError::from(InterfaceError::NullError("config".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    if let Some(log_path) = (*config).clone().chat_client.log_path {
        init_logging(log_path, error_out);

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
        drop(Box::from_raw(client))
    }
}

/// Creates a Chat Client config
///
/// ## Arguments
/// `network` - The network to run on
/// `public_address` - The nodes public address
/// `error_out` - Pointer to an int which will be modified
///
/// ## Returns
/// `*mut ApplicationConfig` - Returns a pointer to an ApplicationConfig
///
/// # Safety
/// The ```destroy_config``` method must be called when finished with a Config to prevent a memory leak
#[allow(clippy::too_many_lines)]
#[no_mangle]
pub unsafe extern "C" fn create_chat_config(
    network_str: *const c_char,
    public_address: *const c_char,
    datastore_path: *const c_char,
    identity_file_path: *const c_char,
    tor_transport_config: *mut TransportConfig,
    log_path: *const c_char,
    error_out: *mut c_int,
) -> *mut ApplicationConfig {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    let mut bad_network = |e| {
        error = LibChatError::from(InterfaceError::InvalidArgument(e)).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    };

    let network = if network_str.is_null() {
        bad_network("network is missing".to_string());
        return ptr::null_mut();
    } else {
        match CStr::from_ptr(network_str).to_str() {
            Ok(str) => match Network::from_str(str) {
                Ok(network) => network,
                Err(e) => {
                    bad_network(e.to_string());
                    return ptr::null_mut();
                },
            },
            Err(e) => {
                bad_network(e.to_string());
                return ptr::null_mut();
            },
        }
    };

    if tor_transport_config.is_null() {
        error = LibChatError::from(InterfaceError::NullError("tor_transport_config".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    }

    let datastore_path_string;
    if datastore_path.is_null() {
        error = LibChatError::from(InterfaceError::NullError("datastore_path".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    } else {
        match CStr::from_ptr(datastore_path).to_str() {
            Ok(v) => {
                datastore_path_string = v.to_owned();
            },
            _ => {
                error = LibChatError::from(InterfaceError::InvalidArgument("datastore_path".to_string())).code;
                ptr::swap(error_out, &mut error as *mut c_int);
                return ptr::null_mut();
            },
        }
    }
    let datastore_path = PathBuf::from(datastore_path_string);

    let public_address_string;
    if public_address.is_null() {
        error = LibChatError::from(InterfaceError::NullError("public_address".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    } else {
        match CStr::from_ptr(public_address).to_str() {
            Ok(v) => {
                public_address_string = v.to_owned();
            },
            _ => {
                error = LibChatError::from(InterfaceError::InvalidArgument("public_address".to_string())).code;
                ptr::swap(error_out, &mut error as *mut c_int);
                return ptr::null_mut();
            },
        }
    }
    let address = match Multiaddr::from_str(&public_address_string) {
        Ok(a) => a,
        Err(e) => {
            error = LibChatError::from(InterfaceError::InvalidArgument(e.to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return ptr::null_mut();
        },
    };

    let log_path_string;
    if log_path.is_null() {
        error = LibChatError::from(InterfaceError::NullError("log_path".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    } else {
        match CStr::from_ptr(log_path).to_str() {
            Ok(v) => {
                log_path_string = v.to_owned();
            },
            _ => {
                error = LibChatError::from(InterfaceError::InvalidArgument("log_path".to_string())).code;
                ptr::swap(error_out, &mut error as *mut c_int);
                return ptr::null_mut();
            },
        }
    }
    let log_path = PathBuf::from(log_path_string);

    let mut bad_identity = |e| {
        error = LibChatError::from(InterfaceError::InvalidArgument(e)).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    };

    let identity_path = match CStr::from_ptr(identity_file_path).to_str() {
        Ok(str) => PathBuf::from(str),
        Err(e) => {
            bad_identity(e.to_string());
            return ptr::null_mut();
        },
    };

    let mut chat_client_config = ChatClientConfig::default();
    chat_client_config.network = network;
    chat_client_config.p2p.transport = (*tor_transport_config).clone();
    chat_client_config.p2p.public_addresses = MultiaddrList::from(vec![address]);
    chat_client_config.log_path = Some(log_path);
    chat_client_config.identity_file = identity_path;
    chat_client_config.set_base_path(datastore_path);

    let config = ApplicationConfig {
        chat_client: chat_client_config,
        ..ApplicationConfig::default()
    };

    Box::into_raw(Box::new(config))
}

/// Frees memory for an ApplicationConfig
///
/// ## Arguments
/// `config` - The pointer of an ApplicationConfig
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn destroy_chat_config(config: *mut ApplicationConfig) {
    if !config.is_null() {
        drop(Box::from_raw(config))
    }
}

/// Inits logging, this function is deliberately not exposed externally in the header
///
/// ## Arguments
/// `log_path` - Path to where the log will be stored
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
#[allow(clippy::too_many_lines)]
unsafe fn init_logging(log_path: PathBuf, error_out: *mut c_int) {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    let num_rolling_log_files = 2;
    let size_per_log_file_bytes: u64 = 10 * 1024 * 1024;

    let path = log_path.to_str().expect("Convert path to string");
    let encoder = PatternEncoder::new("{d(%Y-%m-%d %H:%M:%S.%f)} [{t}] {l:5} {m}{n}");

    let mut pattern;
    let split_str: Vec<&str> = path.split('.').collect();
    if split_str.len() <= 1 {
        pattern = format!("{}{}", path, "{}");
    } else {
        pattern = split_str[0].to_string();
        for part in split_str.iter().take(split_str.len() - 1).skip(1) {
            pattern = format!("{}.{}", pattern, part);
        }

        pattern = format!("{}{}", pattern, ".{}.");
        pattern = format!("{}{}", pattern, split_str[split_str.len() - 1]);
    }
    let roller = FixedWindowRoller::builder()
        .build(pattern.as_str(), num_rolling_log_files)
        .expect("Should be able to create a Roller");
    let size_trigger = SizeTrigger::new(size_per_log_file_bytes);
    let policy = CompoundPolicy::new(Box::new(size_trigger), Box::new(roller));

    let log_appender: Box<dyn Append> = Box::new(
        RollingFileAppender::builder()
            .encoder(Box::new(encoder))
            .append(true)
            .build(path, Box::new(policy))
            .expect("Should be able to create an appender"),
    );

    let lconfig = Config::builder()
        .appender(Appender::builder().build("logfile", log_appender))
        .logger(
            Logger::builder()
                .appender("logfile")
                .additive(false)
                .build("comms", LevelFilter::Warn),
        )
        .logger(
            Logger::builder()
                .appender("logfile")
                .additive(false)
                .build("comms::noise", LevelFilter::Warn),
        )
        .logger(
            Logger::builder()
                .appender("logfile")
                .additive(false)
                .build("tokio_util", LevelFilter::Warn),
        )
        .logger(
            Logger::builder()
                .appender("logfile")
                .additive(false)
                .build("tracing", LevelFilter::Warn),
        )
        .logger(
            Logger::builder()
                .appender("logfile")
                .additive(false)
                .build("chat_ffi::callback_handler", LevelFilter::Warn),
        )
        .logger(
            Logger::builder()
                .appender("logfile")
                .additive(false)
                .build("chat_ffi", LevelFilter::Warn),
        )
        .logger(
            Logger::builder()
                .appender("logfile")
                .additive(false)
                .build("contacts", LevelFilter::Warn),
        )
        .logger(
            Logger::builder()
                .appender("logfile")
                .additive(false)
                .build("p2p", LevelFilter::Warn),
        )
        .logger(
            Logger::builder()
                .appender("logfile")
                .additive(false)
                .build("yamux", LevelFilter::Warn),
        )
        .logger(
            Logger::builder()
                .appender("logfile")
                .additive(false)
                .build("dht", LevelFilter::Warn),
        )
        .logger(
            Logger::builder()
                .appender("logfile")
                .additive(false)
                .build("mio", LevelFilter::Warn),
        )
        .build(Root::builder().appender("logfile").build(LevelFilter::Warn))
        .expect("Should be able to create a Config");

    match log4rs::init_config(lconfig) {
        Ok(_) => debug!(target: LOG_TARGET, "Logging started"),
        Err(_) => warn!(target: LOG_TARGET, "Logging has already been initialized"),
    }
}

/// Sends a message over a client
///
/// ## Arguments
/// `client` - The Client pointer
/// `receiver` - A string containing a tari address
/// `message` - The peer seeds config for the node
/// `error_out` - Pointer to an int which will be modified
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// The ```receiver``` should be destroyed after use
#[no_mangle]
pub unsafe extern "C" fn send_chat_message(
    client: *mut ChatClientFFI,
    receiver: *mut TariAddress,
    message_c_char: *const c_char,
    error_out: *mut c_int,
) {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if client.is_null() {
        error = LibChatError::from(InterfaceError::NullError("client".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    }

    if receiver.is_null() {
        error = LibChatError::from(InterfaceError::NullError("receiver".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    }

    let message = match CStr::from_ptr(message_c_char).to_str() {
        Ok(str) => str.to_string(),
        Err(e) => {
            error = LibChatError::from(InterfaceError::InvalidArgument(e.to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return;
        },
    };

    (*client)
        .runtime
        .block_on((*client).client.send_message((*receiver).clone(), message));
}

/// Add a contact
///
/// ## Arguments
/// `client` - The Client pointer
/// `address` - A TariAddress ptr
/// `error_out` - Pointer to an int which will be modified
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// The ```address``` should be destroyed after use
#[no_mangle]
pub unsafe extern "C" fn add_chat_contact(
    client: *mut ChatClientFFI,
    receiver: *mut TariAddress,
    error_out: *mut c_int,
) {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if client.is_null() {
        error = LibChatError::from(InterfaceError::NullError("client".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    }

    if receiver.is_null() {
        error = LibChatError::from(InterfaceError::NullError("receiver".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    }

    (*client).runtime.block_on((*client).client.add_contact(&(*receiver)));
}

/// Check the online status of a contact
///
/// ## Arguments
/// `client` - The Client pointer
/// `address` - A TariAddress ptr
/// `error_out` - Pointer to an int which will be modified
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// The ```address``` should be destroyed after use
#[no_mangle]
pub unsafe extern "C" fn check_online_status(
    client: *mut ChatClientFFI,
    receiver: *mut TariAddress,
    error_out: *mut c_int,
) -> c_int {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    if client.is_null() {
        error = LibChatError::from(InterfaceError::NullError("client".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    }

    if receiver.is_null() {
        error = LibChatError::from(InterfaceError::NullError("receiver".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
    }

    let rec = (*receiver).clone();
    let status = (*client).runtime.block_on((*client).client.check_online_status(&rec));

    status.as_u8().into()
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
    limit: *mut c_int,
    page: *mut c_int,
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

    let mlimit = u64::try_from(*limit).unwrap_or(DEFAULT_MESSAGE_LIMIT);
    let mpage = u64::try_from(*page).unwrap_or(DEFAULT_MESSAGE_PAGE);

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
/// `messages_ptr` - The pointer of a Vec<Message>
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn destroy_chat_messages(messages_ptr: *mut ChatMessages) {
    if !messages_ptr.is_null() {
        drop(Box::from_raw(messages_ptr))
    }
}

/// Creates a TariAddress and returns a ptr
///
/// ## Arguments
/// `receiver_c_char` - A string containing a tari address hex value
/// `error_out` - Pointer to an int which will be modified
///
/// ## Returns
/// `*mut TariAddress` - A ptr to a TariAddress
///
/// # Safety
/// The ```destroy_tari_address``` function should be called when finished with the TariAddress
#[no_mangle]
pub unsafe extern "C" fn create_tari_address(
    receiver_c_char: *const c_char,
    error_out: *mut c_int,
) -> *mut TariAddress {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    let receiver = match CStr::from_ptr(receiver_c_char).to_str() {
        Ok(str) => match TariAddress::from_str(str) {
            Ok(address) => address,
            Err(e) => {
                error = LibChatError::from(InterfaceError::InvalidArgument(e.to_string())).code;
                ptr::swap(error_out, &mut error as *mut c_int);
                return ptr::null_mut();
            },
        },
        Err(e) => {
            error = LibChatError::from(InterfaceError::NullError(e.to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return ptr::null_mut();
        },
    };

    Box::into_raw(Box::new(receiver))
}

/// Frees memory for a TariAddress
///
/// ## Arguments
/// `address` - The pointer of a TariAddress
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn destroy_tari_address(address: *mut TariAddress) {
    if !address.is_null() {
        drop(Box::from_raw(address))
    }
}

/// Creates a tor transport config
///
/// ## Arguments
/// `control_server_address` - The pointer to a char array
/// `tor_cookie` - The pointer to a ChatByteVector containing the contents of the tor cookie file, can be null
/// `tor_port` - The tor port
/// `tor_proxy_bypass_for_outbound` - Whether tor will use a direct tcp connection for a given bypass address instead of
/// the tor proxy if tcp is available, if not it has no effect
/// `socks_password` - The pointer to a char array containing the socks password, can be null
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `*mut TransportConfig` - Returns a pointer to a tor TransportConfig, null on error.
///
/// # Safety
/// The ```destroy_chat_tor_transport_config``` method must be called when finished with a TransportConfig to prevent a
/// memory leak
#[no_mangle]
pub unsafe extern "C" fn create_chat_tor_transport_config(
    control_server_address: *const c_char,
    tor_cookie: *const ChatByteVector,
    tor_port: c_ushort,
    tor_proxy_bypass_for_outbound: bool,
    socks_username: *const c_char,
    socks_password: *const c_char,
    error_out: *mut c_int,
) -> *mut TransportConfig {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    let control_address_str;
    if control_server_address.is_null() {
        error = LibChatError::from(InterfaceError::NullError("control_server_address".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return ptr::null_mut();
    } else {
        match CStr::from_ptr(control_server_address).to_str() {
            Ok(v) => {
                control_address_str = v.to_owned();
            },
            _ => {
                error = LibChatError::from(InterfaceError::InvalidArgument("control_server_address".to_string())).code;
                ptr::swap(error_out, &mut error as *mut c_int);
                return ptr::null_mut();
            },
        }
    }

    let username_str;
    let password_str;
    let socks_authentication = if !socks_username.is_null() && !socks_password.is_null() {
        match CStr::from_ptr(socks_username).to_str() {
            Ok(v) => {
                username_str = v.to_owned();
            },
            _ => {
                error = LibChatError::from(InterfaceError::InvalidArgument("socks_username".to_string())).code;
                ptr::swap(error_out, &mut error as *mut c_int);
                return ptr::null_mut();
            },
        }
        match CStr::from_ptr(socks_password).to_str() {
            Ok(v) => {
                password_str = v.to_owned();
            },
            _ => {
                error = LibChatError::from(InterfaceError::InvalidArgument("socks_password".to_string())).code;
                ptr::swap(error_out, &mut error as *mut c_int);
                return ptr::null_mut();
            },
        };
        SocksAuthentication::UsernamePassword {
            username: username_str,
            password: password_str,
        }
    } else {
        SocksAuthentication::None
    };

    let tor_authentication = if tor_cookie.is_null() {
        TorControlAuthentication::None
    } else {
        let cookie_hex = hex::to_hex((*tor_cookie).0.as_slice());
        TorControlAuthentication::hex(cookie_hex)
    };

    let onion_port = match NonZeroU16::new(tor_port) {
        Some(p) => p,
        None => {
            error = LibChatError::from(InterfaceError::InvalidArgument(
                "onion_port must be greater than 0".to_string(),
            ))
            .code;
            ptr::swap(error_out, &mut error as *mut c_int);
            return ptr::null_mut();
        },
    };

    match control_address_str.parse() {
        Ok(v) => {
            let transport = TransportConfig {
                transport_type: TransportType::Tor,
                tor: TorTransportConfig {
                    control_address: v,
                    control_auth: tor_authentication,
                    // The wallet will populate this from the db
                    identity: None,
                    onion_port,
                    socks_auth: socks_authentication,
                    proxy_bypass_for_outbound_tcp: tor_proxy_bypass_for_outbound,
                    ..Default::default()
                },
                ..Default::default()
            };

            Box::into_raw(Box::new(transport))
        },
        Err(_) => {
            error = LibChatError::from(InterfaceError::InvalidArgument("control_address".to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            ptr::null_mut()
        },
    }
}

/// Frees memory for a TransportConfig
///
/// ## Arguments
/// `transport` - The pointer to a TransportConfig
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn destroy_chat_tor_transport_config(transport: *mut TransportConfig) {
    if !transport.is_null() {
        drop(Box::from_raw(transport))
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

/// Frees memory for a ChatFFIContactsLivenessData
///
/// ## Arguments
/// `address` - The pointer of a ChatFFIContactsLivenessData
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn destroy_chat_ffi_liveness_data(address: *mut ChatFFIContactsLivenessData) {
    if !address.is_null() {
        drop(Box::from_raw(address))
    }
}

#[cfg(test)]
mod test {
    use std::ffi::CString;

    use libc::c_char;

    use crate::*;

    #[test]
    fn test_create_chat_tor_transport_config() {
        let mut error = 0;
        let error_ptr = &mut error as *mut c_int;
        let address_control = CString::new("/ip4/127.0.0.1/tcp/8080").unwrap();
        let address_control_str: *const c_char = CString::into_raw(address_control) as *const c_char;

        unsafe {
            let transport = create_chat_tor_transport_config(
                address_control_str,
                ptr::null(),
                8080,
                false,
                ptr::null(),
                ptr::null(),
                error_ptr,
            );

            assert_eq!(error, 0);
            destroy_chat_tor_transport_config(transport);
        }
    }

    #[test]
    fn test_create_chat_config() {
        let mut error = 0;
        let error_ptr = &mut error as *mut c_int;
        let address_control = CString::new("/ip4/127.0.0.1/tcp/8080").unwrap();
        let address_control_str: *const c_char = CString::into_raw(address_control) as *const c_char;

        let network = CString::new("localnet").unwrap();
        let data_path = CString::new("data/chat_ffi_client/").unwrap();
        let identity_path = CString::new("id_file.json").unwrap();
        let log_path = CString::new("logs/").unwrap();

        unsafe {
            let transport_config = create_chat_tor_transport_config(
                address_control_str,
                ptr::null(),
                8080,
                false,
                ptr::null(),
                ptr::null(),
                error_ptr,
            );

            assert_eq!(error, 0);

            let chat_config = create_chat_config(
                network.as_ptr(),
                address_control_str,
                data_path.as_ptr(),
                identity_path.as_ptr(),
                transport_config,
                log_path.as_ptr(),
                error_ptr,
            );

            assert_eq!(error, 0);

            destroy_chat_config(chat_config);
            destroy_chat_tor_transport_config(transport_config);
        }
    }
}
