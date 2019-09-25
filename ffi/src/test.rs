//  Copyright 2019 The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
extern crate libc;
use super::*;
use libc::c_char;
use std::{ffi::CString, ptr};
use tari_comms::types::{CommsPublicKey, CommsSecretKey};
use tari_crypto::keys::{PublicKey, SecretKey};
use tari_utilities::hex::Hex;

#[test]
fn ffi_test_free_string() {
    unsafe {
        let m = CString::new("Test").unwrap();
        let m_ptr: *mut c_char = CString::into_raw(m) as *mut c_char;
        assert_ne!(m_ptr.is_null(), true);
        assert!(*m_ptr > 0); // dereference will return first character as integer, T as i8 = 84 > 0 = true
        free_string(m_ptr);
        assert_eq!(*m_ptr, 0); // dereference will return zero, avoids malloc error if attempting to evaluate by other
                               // means.
    }
}

#[test]
fn ffi_test_local_ip() {
    unsafe {
        let ip = get_local_ip_();
        assert_ne!(ip.is_null(), true);
        free_string(ip);
    }
}

#[test]
fn ffi_test_settings() {
    unsafe {
        let mut rng = rand::OsRng::new().unwrap();
        let secret_key1 = CommsSecretKey::random(&mut rng);

        let grpc_port: u32 = 10001;
        let control_port: u32 = 10000;
        let secret_key = CString::new(secret_key1.to_hex()).unwrap();
        let secret_key_ptr: *const c_char = CString::into_raw(secret_key.clone()) as *const c_char;
        let data_path = CString::new("./data").unwrap();
        let data_path_ptr: *const c_char = CString::into_raw(data_path.clone()) as *const c_char;
        let database = CString::new("./data/text_message_service.sqlite3").unwrap();
        let database_ptr: *const c_char = CString::into_raw(database.clone()) as *const c_char;
        let name = CString::new("Test").unwrap();
        let name_ptr: *const c_char = CString::into_raw(name.clone()) as *const c_char;
        let settings = create_settings(
            control_port,
            grpc_port,
            secret_key_ptr,
            data_path_ptr,
            database_ptr,
            name_ptr,
        );
        assert_ne!(settings.is_null(), true);
        let cp: u32 = settings_get_control_port(settings);
        assert_eq!(cp, control_port);
        let gp: u32 = settings_get_grpc_port(settings);
        assert_eq!(gp, grpc_port);
        let sk_ptr = settings_get_secret_key(settings);
        assert_eq!(CString::from_raw(sk_ptr), secret_key);
        let dp_ptr = settings_get_data_path(settings);
        assert_eq!(CString::from_raw(dp_ptr), data_path);
        let db_ptr = settings_get_database_path(settings);
        assert_eq!(CString::from_raw(db_ptr), database);
        let sn_ptr = settings_get_screen_name(settings);
        assert_eq!(CString::from_raw(sn_ptr), name);
        destroy_settings(settings);
    }
}

#[test]
fn ffi_test_configpeer() {
    unsafe {
        let mut rng = rand::OsRng::new().unwrap();
        let secret_key1 = CommsSecretKey::random(&mut rng);

        let name = CString::new("Test").unwrap();
        let name_ptr: *const c_char = CString::into_raw(name.clone()) as *const c_char;
        let public_key = CString::new(secret_key1.to_hex()).unwrap();
        let public_key_ptr: *const c_char = CString::into_raw(public_key.clone()) as *const c_char;
        let address = CString::new("127.0.0.1:10000").unwrap();
        let address_ptr: *const c_char = CString::into_raw(address.clone()) as *const c_char;
        let config_peer = create_configpeer(name_ptr, public_key_ptr, address_ptr);
        assert_ne!(config_peer.is_null(), true);
        let n_ptr = configpeer_get_screen_name(config_peer);
        assert_eq!(CString::from_raw(n_ptr), name);
        let p_ptr = configpeer_get_public_key(config_peer);
        assert_eq!(CString::from_raw(p_ptr), public_key);
        let a_ptr = configpeer_get_address(config_peer);
        assert_eq!(CString::from_raw(a_ptr), address);
        destroy_configpeer(config_peer);
    }
}

#[test]
fn ffi_test_wallet() {
    unsafe {
        let mut rng = rand::OsRng::new().unwrap();
        let sk1 = CommsSecretKey::random(&mut rng);
        let sk2 = CommsSecretKey::random(&mut rng);
        let pk2 = CommsPublicKey::from_secret_key(&sk2);

        let listener = CString::new("127.0.0.1:10000").unwrap();
        let listener_ptr = CString::into_raw(listener.clone()) as *const c_char;
        let public_address = CString::new("127.0.0.1").unwrap();
        let public_address_ptr = CString::into_raw(public_address.clone()) as *const c_char;
        let host_address = CString::new("127.0.0.1").unwrap();
        let host_address_ptr = CString::into_raw(host_address.clone()) as *const c_char;
        let socks = ptr::null();
        let duration: u64 = 5000;
        let grpc_port: u32 = 10001;
        let control_port: u32 = 10000;
        let secret_key = CString::new(sk1.to_hex()).unwrap();
        let secret_key_ptr: *const c_char = CString::into_raw(secret_key.clone()) as *const c_char;
        let data_path = CString::new("./data").unwrap();
        let data_path_ptr: *const c_char = CString::into_raw(data_path.clone()) as *const c_char;
        let database = CString::new("./data/text_message_service.sqlite3").unwrap();
        let database_ptr: *const c_char = CString::into_raw(database.clone()) as *const c_char;
        let name = CString::new("User1").unwrap();
        let name_ptr: *const c_char = CString::into_raw(name.clone()) as *const c_char;
        let settings = create_settings(
            control_port,
            grpc_port,
            secret_key_ptr,
            data_path_ptr,
            database_ptr,
            name_ptr,
        );
        let wallet = create_wallet(
            host_address_ptr,
            public_address_ptr,
            settings,
            listener_ptr,
            socks,
            duration,
        );
        assert_ne!(wallet.is_null(), true);
        let peername = CString::new("User2").unwrap();
        let peername_ptr: *const c_char = CString::into_raw(peername.clone()) as *const c_char;
        let peerpublic_key = CString::new(pk2.to_hex()).unwrap();
        let peerpublic_key_ptr: *const c_char = CString::into_raw(peerpublic_key.clone()) as *const c_char;
        let peeraddress = CString::new("127.0.0.1:20000").unwrap();
        let peeraddress_ptr: *const c_char = CString::into_raw(peeraddress.clone()) as *const c_char;
        let configpeer = create_configpeer(peername_ptr, peerpublic_key_ptr, peeraddress_ptr);
        wallet_add_peer(configpeer, wallet);
        let msg = CString::new("Test").unwrap();
        let msg_ptr: *mut c_char = CString::into_raw(msg.clone()) as *mut c_char;
        wallet_send_message(wallet, configpeer, msg_ptr);
        assert!(
            (*wallet)
                .text_message_service
                .get_text_messages()
                .unwrap()
                .sent_messages
                .len() >
                0
        );
        free_string(msg_ptr);
        destroy_wallet(wallet);
        destroy_configpeer(configpeer);
    }
}

#[test]
fn ffi_test_messages() {
    // TODO: implementation
}
