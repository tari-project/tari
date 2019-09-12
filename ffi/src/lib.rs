// Copyright 2019. The Tari Project
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
extern crate libc;

use libc::{c_char, c_int};
use pnet::datalink::{self, NetworkInterface};
use serde::{Deserialize, Serialize};
use std::{
    boxed::Box,
    ffi::{CStr, CString},
    str,
    sync::Arc,
    time::Duration,
};
use tari_comms::{
    connection::{net_address::ip::SocketAddress, NetAddress},
    control_service::ControlServiceConfig,
    peer_manager::Peer,
    types::{CommsPublicKey, CommsSecretKey},
};
use tari_crypto::keys::PublicKey;
use tari_p2p::{initialization::CommsConfig, sync_services::ServiceError};
use tari_utilities::hex::Hex;
use tari_wallet::{text_message_service::Contact, wallet::WalletConfig};
/// TODO: Replace expect() methods within functions.

/// Once bindings are generated via cbindgen, change the using to struct, remove the equals sign and anything after it
/// on the line. These are used as opaque pointers
pub type Wallet = tari_wallet::Wallet;
pub type ReceivedTextMessage = tari_wallet::text_message_service::ReceivedTextMessage;

/// Received Messages
#[derive(Debug)]
pub struct ReceivedMessages(Vec<ReceivedTextMessage>);

/// Wallet Settings
#[derive(Debug, Default, Deserialize)]
pub struct Settings {
    control_port: Option<u32>,
    grpc_port: Option<u32>,
    secret_key: Option<String>,
    data_path: Option<String>,
    database_path: Option<String>,
    screen_name: Option<String>,
}

/// ConfigPeer
#[derive(Debug, Serialize, Deserialize)]
pub struct ConfigPeer {
    screen_name: String,
    pub_key: String,
    address: String,
}

/// Vector of ConfigPeer
#[derive(Debug, Serialize, Deserialize)]
pub struct Peers {
    peers: Vec<ConfigPeer>,
}

/// Returns the timestamp of a ReceivedText Message as string
#[no_mangle]
pub unsafe extern "C" fn receivedtextmessage_get_timestamp(o: *const ReceivedTextMessage) -> *mut c_char {
    let mut m = CString::new("").unwrap();
    if !o.is_null() {
        m = CString::new((*o).timestamp.to_string().clone()).unwrap();
    }
    CString::into_raw(m)
}

/// Returns the display name from the ReceivedTextMessage for the the peer as string
#[no_mangle]
pub unsafe extern "C" fn receivedtextmessage_get_screenname(
    o: *const ReceivedTextMessage,
    w: *mut Wallet,
) -> *mut c_char
{
    let mut m = CString::new("").unwrap();
    if !o.is_null() {
        let contacts = (*w)
            .text_message_service
            .get_contacts()
            .expect("Could not read contacts");
        let contact = contacts
            .iter()
            .find(|c| c.pub_key == (*o).source_pub_key)
            .expect("Message from unknown peer");

        m = CString::new(contact.screen_name.to_string().clone()).unwrap();
    }
    CString::into_raw(m)
}

/// Returns the identifier from the ReceivedTextMessage for the the peer as char*
#[no_mangle]
pub unsafe extern "C" fn receivedtextmessage_get_public_key(o: *const ReceivedTextMessage) -> *mut c_char {
    let mut m = CString::new("").unwrap();
    if !o.is_null() {
        m = CString::new((*o).source_pub_key.to_string().clone()).unwrap();
    }
    CString::into_raw(m)
}

/// Returns the message from the ReceivedTextMessage for the the peer as char*
#[no_mangle]
pub unsafe extern "C" fn receivedtextmessage_get_message(o: *const ReceivedTextMessage) -> *mut c_char {
    let mut m = CString::new("").unwrap();
    if !o.is_null() {
        m = CString::new((*o).message.clone()).unwrap();
    }
    CString::into_raw(m)
}

/// Frees memory for ReceivedMessages pointer
#[no_mangle]
pub unsafe extern "C" fn destroy_receivedmessages(obj: *mut ReceivedMessages) {
    // as a rule of thumb, freeing a null pointer is just a noop.
    if obj.is_null() {
        return;
    }

    Box::from_raw(obj);
}

/// Returns a pointer to a wallet
#[no_mangle]
pub unsafe extern "C" fn create_wallet(
    host_s: *const c_char,           // listener
    public_address_s: *const c_char, // public_address
    settings_p: *mut Settings,       // Settings
    listener_s: *const c_char,       // public
    socks_s: *const c_char,          // socks
    duration_n: u64,                 // duration
) -> *mut Wallet
{
    let listener = if !listener_s.is_null() {
        CStr::from_ptr(listener_s)
            .to_str()
            .unwrap()
            .to_owned()
            .parse::<NetAddress>()
            .unwrap()
    } else {
        CStr::from_bytes_with_nul(b"0.0.0.0:10000\0")
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned()
            .parse::<NetAddress>()
            .unwrap()
    };
    let public = if !public_address_s.is_null() {
        CStr::from_ptr(public_address_s).to_str().unwrap().to_owned()
    } else {
        CStr::from_bytes_with_nul(b"127.0.0.1\0")
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned()
    };

    let local_net_address = match format!("{}:{}", public, (*settings_p).control_port.unwrap()).parse() {
        Ok(na) => na,
        Err(_) => {
            std::process::exit(1);
        },
    };

    let host_address = if !host_s.is_null() {
        CStr::from_ptr(host_s).to_str().unwrap().to_owned()
    } else {
        CStr::from_bytes_with_nul(b"0.0.0.0\0")
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned()
    };

    let socks = if !socks_s.is_null() {
        Some(
            CStr::from_ptr(socks_s)
                .to_str()
                .unwrap()
                .to_owned()
                .parse::<SocketAddress>()
                .unwrap(),
        )
    } else {
        None
    };

    let secret_key = CommsSecretKey::from_hex((*settings_p).secret_key.clone().unwrap().as_str()).unwrap();
    let public_key = CommsPublicKey::from_secret_key(&secret_key);

    let config = WalletConfig {
        comms: CommsConfig {
            control_service: ControlServiceConfig {
                listener_address: listener.clone(),
                socks_proxy_address: socks.clone(),
                requested_connection_timeout: Duration::from_millis(duration_n),
            },
            socks_proxy_address: socks.clone(),
            host: host_address.parse().unwrap(),
            public_key: public_key.clone(),
            secret_key: secret_key.clone(),
            public_address: local_net_address,
            datastore_path: (*settings_p).data_path.clone().unwrap(),
            peer_database_name: public_key.to_hex(),
        },
        public_key: public_key.clone(),
        database_path: (*settings_p).database_path.clone().unwrap(),
    };

    Box::into_raw(Box::new(Wallet::new(config).unwrap()))
}

/// Shuts down services and frees memory for wallet pointer
#[no_mangle]
pub unsafe extern "C" fn destroy_wallet(w: *mut Wallet) {
    if !w.is_null() {
        let wallet = Box::from_raw(w);
        wallet.service_executor.shutdown().unwrap();
        wallet
            .service_executor
            .join_timeout(Duration::from_millis(3000))
            .unwrap();
        let comms = Arc::try_unwrap(wallet.comms_services)
            .map_err(|_| ServiceError::CommsServiceOwnershipError)
            .unwrap();

        comms.shutdown().unwrap();
    }
}

/// Adds peer to wallet and adds peer as contact to wallet message service
#[no_mangle]
pub unsafe extern "C" fn wallet_add_peer(o: *mut ConfigPeer, w: *mut Wallet) {
    let pk = CommsPublicKey::from_hex((*o).pub_key.as_str()).expect("Error parsing pub key from Hex");
    if let Ok(na) = (*o).address.clone().parse::<NetAddress>() {
        let peer = Peer::from_public_key_and_address(pk.clone(), na.clone()).unwrap();
        (*w).comms_services.peer_manager().add_peer(peer).unwrap();

        if let Err(e) = (*w).text_message_service.add_contact(Contact {
            screen_name: (*o).screen_name.clone(),
            pub_key: pk.clone(),
            address: na.clone(),
        }) {
            println!("{:?}", e);
        };
    }
}

/// Returns a pointer to the received messages
#[no_mangle]
pub unsafe extern "C" fn wallet_get_receivedmessages(w: *mut Wallet) -> *mut ReceivedMessages {
    let contacts = (*w)
        .text_message_service
        .get_contacts()
        .expect("Could not read contacts");

    let mut rx_messages: Vec<ReceivedTextMessage> = (*w)
        .text_message_service
        .get_text_messages()
        .expect("Error retrieving text messages from TMS")
        .received_messages;

    rx_messages.sort();

    let mut messages = ReceivedMessages(Vec::new());

    for i in 0..rx_messages.len() {
        if !contacts
            .iter()
            .find(|c| c.pub_key == rx_messages[i].source_pub_key)
            .is_none()
        //.expect("Message from unknown peer");
        {
            messages.0.push(rx_messages[i].clone());
        }
    }

    let boxed = Box::new(messages);
    Box::into_raw(boxed)
}

/// Returns the number of received messages, zero-indexed
#[no_mangle]
pub unsafe extern "C" fn wallet_get_receivedmessages_length(vec: *const ReceivedMessages) -> c_int {
    if vec.is_null() {
        return 0;
    }

    (&*vec).0.len() as c_int
}

/// Returns a pointer to the received messages vector
#[no_mangle]
pub unsafe extern "C" fn wallet_get_receivedmessages_contents(
    msgs: *mut ReceivedMessages,
    i: c_int,
) -> *const ReceivedTextMessage
{
    if msgs.is_null() {
        return std::ptr::null_mut();
    }
    let list = &mut *msgs;
    &((list.0)[i as usize])
}

/// Sends a message from the wallet to the peers wallet
#[no_mangle]
pub unsafe extern "C" fn wallet_send_message(w: *mut Wallet, o: *mut ConfigPeer, s: *mut c_char) {
    if !w.is_null() {
        if !o.is_null() {
            if !s.is_null() {
                let c_str = CStr::from_ptr(s);
                let r_str = c_str.to_str().unwrap();
                let destination = CommsPublicKey::from_hex(r_str).unwrap();
                (*w).text_message_service
                    .send_text_message(destination, r_str.to_string())
                    .unwrap()
            }
        }
    }
}

/// Returns ip address as char*
#[no_mangle]
pub unsafe extern "C" fn get_local_ip_() -> *mut c_char {
    let mut m = CString::new("").unwrap();
    let mut error = false;
    // get and filter interfaces
    let interfaces: Vec<NetworkInterface> = datalink::interfaces()
        .into_iter()
        .filter(|interface| {
            !interface.is_loopback() && interface.is_up() && interface.ips.iter().any(|addr| addr.is_ipv4())
        })
        .collect();
    // select first interface
    if interfaces.first().is_none() {
        error = true;
    }

    if !error {
        // get network interface and retrieve ipv4 address
        let interface = interfaces.first().unwrap().clone();
        let local_ip = interface
            .ips
            .iter()
            .find(|addr| addr.is_ipv4())
            .unwrap()
            .ip()
            .to_string();

        m = CString::new(local_ip).unwrap();
    }
    CString::into_raw(m)
}

/// Returns a pointer to ConfigPeer
#[no_mangle]
pub unsafe extern "C" fn create_configpeer(s: *const c_char, p: *const c_char, a: *const c_char) -> *mut ConfigPeer {
    let name = if !s.is_null() {
        CStr::from_ptr(s).to_str().unwrap().to_owned()
    } else {
        CStr::from_bytes_with_nul(b"\0").unwrap().to_str().unwrap().to_owned()
    };

    let key = if !p.is_null() {
        CStr::from_ptr(p).to_str().unwrap().to_owned()
    } else {
        CStr::from_bytes_with_nul(b"\0").unwrap().to_str().unwrap().to_owned()
    };

    let addr = if !a.is_null() {
        CStr::from_ptr(a).to_str().unwrap().to_owned()
    } else {
        CStr::from_bytes_with_nul(b"\0").unwrap().to_str().unwrap().to_owned()
    };

    Box::into_raw(Box::new(ConfigPeer {
        screen_name: name,
        pub_key: key,
        address: addr,
    }))
}

/// Returns display name for peer as char*
#[no_mangle]
pub unsafe extern "C" fn configpeer_get_screen_name(o: *mut ConfigPeer) -> *mut c_char {
    let mut m = CString::new("").unwrap();
    if !o.is_null() {
        m = CString::new((*o).screen_name.clone()).unwrap();
    }
    CString::into_raw(m)
}

/// Returns public key for peer as char*
#[no_mangle]
pub unsafe extern "C" fn configpeer_get_public_key(o: *mut ConfigPeer) -> *mut c_char {
    let mut m = CString::new("").unwrap();
    if !o.is_null() {
        m = CString::new((*o).pub_key.clone()).unwrap();
    }
    CString::into_raw(m)
}

/// Returns ip address for peer as char*
#[no_mangle]
pub unsafe extern "C" fn configpeer_get_address(o: *mut ConfigPeer) -> *mut c_char {
    let mut m = CString::new("").unwrap();
    if !o.is_null() {
        m = CString::new((*o).address.clone()).unwrap();
    }
    CString::into_raw(m)
}

/// Frees memory for ConfigPeer pointer
#[no_mangle]
pub unsafe extern "C" fn destroy_configpeer(o: *mut ConfigPeer) {
    if !o.is_null() {
        Box::from_raw(o);
    }
}

/// Returns pointer to wallet settings
#[no_mangle]
pub unsafe extern "C" fn create_settings(
    c: u32,
    g: u32,
    sk: *const c_char,
    d: *const c_char,
    db: *const c_char,
    n: *const c_char,
) -> *mut Settings
{
    let secret = if !sk.is_null() {
        Some(CStr::from_ptr(sk).to_str().unwrap().to_owned())
    } else {
        None
    };

    let data = if !d.is_null() {
        Some(CStr::from_ptr(d).to_str().unwrap().to_owned())
    } else {
        None
    };

    let database = if !db.is_null() {
        Some(CStr::from_ptr(db).to_str().unwrap().to_owned())
    } else {
        None
    };

    let name = if !n.is_null() {
        Some(CStr::from_ptr(n).to_str().unwrap().to_owned())
    } else {
        None
    };

    Box::into_raw(Box::new(Settings {
        control_port: Some(c),
        grpc_port: Some(g),
        secret_key: secret,
        data_path: data,
        database_path: database,
        screen_name: name,
    }))
}

/// Returns control port for wallet settings as integer
#[no_mangle]
pub unsafe extern "C" fn settings_get_control_port(o: *mut Settings) -> u32 {
    let mut m = 0u32;
    if !o.is_null() {
        m = (*o).control_port.unwrap();
    }
    m
}

/// Returns grpc port for wallet settings as integer
#[no_mangle]
pub unsafe extern "C" fn settings_get_grpc_port(o: *mut Settings) -> u32 {
    let mut m = 0u32;
    if !o.is_null() {
        m = (*o).grpc_port.unwrap();
    }
    m
}

/// Returns secret key for wallet settings as char*
#[no_mangle]
pub unsafe extern "C" fn settings_get_secret_key(o: *mut Settings) -> *mut c_char {
    let mut m = CString::new("").unwrap();
    if !o.is_null() {
        m = CString::new((*o).secret_key.clone().unwrap()).unwrap();
    }
    CString::into_raw(m)
}

/// Returns data path for wallet settings as char*
#[no_mangle]
pub unsafe extern "C" fn settings_get_data_path(o: *mut Settings) -> *mut c_char {
    let mut m = CString::new("").unwrap();
    if !o.is_null() {
        m = CString::new((*o).data_path.clone().unwrap()).unwrap();
    }
    CString::into_raw(m)
}

/// Returns database path for wallet settings as char*
#[no_mangle]
pub unsafe extern "C" fn settings_get_database_path(o: *mut Settings) -> *mut c_char {
    let mut m = CString::new("").unwrap();
    if !o.is_null() {
        m = CString::new((*o).database_path.clone().unwrap()).unwrap();
    }
    CString::into_raw(m)
}

/// Returns display name for wallet settings as char*
#[no_mangle]
pub unsafe extern "C" fn settings_get_screen_name(o: *mut Settings) -> *mut c_char {
    let mut m = CString::new("").unwrap();
    if !o.is_null() {
        m = CString::new((*o).screen_name.clone().unwrap()).unwrap();
    }
    CString::into_raw(m)
}

/// Frees memory for wallet settings pointer
#[no_mangle]
pub unsafe extern "C" fn destroy_settings(o: *mut Settings) {
    if !o.is_null() {
        Box::from_raw(o);
    }
}

/// Frees memory for string pointer
#[no_mangle]
pub unsafe extern "C" fn free_string(o: *mut c_char) {
    if !o.is_null() {
        let _ = CString::from_raw(o);
    }
}
