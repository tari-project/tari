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

use std::{ffi::CStr, num::NonZeroU16, ptr};

use libc::{c_char, c_int, c_ushort};
use tari_p2p::{SocksAuthentication, TorControlAuthentication, TorTransportConfig, TransportConfig, TransportType};
use tari_utilities::hex;

use crate::{
    byte_vector::ChatByteVector,
    error::{InterfaceError, LibChatError},
};

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
/// `ptr` - The pointer to a TransportConfig
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn destroy_chat_tor_transport_config(ptr: *mut TransportConfig) {
    if !ptr.is_null() {
        drop(Box::from_raw(ptr))
    }
}

#[cfg(test)]
mod test {
    use std::ffi::CString;

    use libc::c_char;

    use super::{create_chat_tor_transport_config, destroy_chat_tor_transport_config};
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
}
