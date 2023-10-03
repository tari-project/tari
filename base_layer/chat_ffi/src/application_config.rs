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

use std::{convert::TryFrom, ffi::CStr, path::PathBuf, ptr, str::FromStr};

use libc::{c_char, c_int};
use tari_chat_client::{
    config::{ApplicationConfig, ChatClientConfig},
    networking::Multiaddr,
};
use tari_common::configuration::{MultiaddrList, Network, StringList};
use tari_p2p::{PeerSeedsConfig, TransportConfig, DEFAULT_DNS_NAME_SERVER};

use crate::error::{InterfaceError, LibChatError};

/// Creates a ChatClient config
///
/// ## Arguments
/// `network` - The network to run on
/// `public_address` - The nodes public address
/// `datastore_path` - The directory for config and db files
/// `identity_file_path` - The location of the identity file
/// `tor_transport_config` - A pointer to the TransportConfig
/// `log_path` - directory for storing log files
/// `log_verbosity` - how verbose should logging be as a c_int 0-5, or 11
///        0 => Off
///        1 => Error
///        2 => Warn
///        3 => Info
///        4 => Debug
///        5 | 11 => Trace // Cranked up to 11
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
    log_verbosity: c_int,
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

    let log_verbosity = u8::try_from(log_verbosity).unwrap_or(2); // 2 == WARN

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
    chat_client_config.log_verbosity = Some(log_verbosity);
    chat_client_config.identity_file = identity_path;
    chat_client_config.set_base_path(datastore_path);

    let config = ApplicationConfig {
        chat_client: chat_client_config,
        peer_seeds: PeerSeedsConfig {
            dns_seeds_use_dnssec: false,
            dns_seeds_name_server: DEFAULT_DNS_NAME_SERVER.parse().unwrap(),
            dns_seeds: StringList::from(vec![format!("seeds.{}.tari.com", network.as_key_str())]),
            ..PeerSeedsConfig::default()
        },
        ..ApplicationConfig::default()
    };

    Box::into_raw(Box::new(config))
}

/// Frees memory for an ApplicationConfig
///
/// ## Arguments
/// `ptr` - The pointer of an ApplicationConfig
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn destroy_chat_config(ptr: *mut ApplicationConfig) {
    if !ptr.is_null() {
        drop(Box::from_raw(ptr))
    }
}

#[cfg(test)]
mod test {
    use std::ffi::CString;

    use libc::c_char;

    use crate::{
        application_config::{create_chat_config, destroy_chat_config},
        tansport_config::{create_chat_tor_transport_config, destroy_chat_tor_transport_config},
        *,
    };

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
                5,
                error_ptr,
            );

            assert_eq!(error, 0);

            destroy_chat_config(chat_config);
            destroy_chat_tor_transport_config(transport_config);
        }
    }
}
