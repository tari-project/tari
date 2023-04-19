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

use std::{ffi::CStr, fs::File, io::Read, path::PathBuf, str::FromStr};

use libc::c_char;
use tari_chat_client::{ChatClient, Client};
use tari_common::configuration::Network;
use tari_common_types::tari_address::TariAddress;
use tari_comms::{peer_manager::Peer, NodeIdentity};
use tari_p2p::P2pConfig;
use tokio::runtime::Runtime;

pub struct ClientFFI {
    client: Client,
    runtime: Runtime,
}

/// Creates a Chat Client
///
/// ## Arguments
/// `config` - The P2PConfig pointer
/// `identity_file_path` - The path to the node identity file
/// `db_path` - The path to the db file
/// `seed_peers` - A ptr to a collection of seed peers
/// `network_str` - The network to connect to
/// ## Returns
/// `*mut ChatClient` - Returns a pointer to a ChatClient, note that it returns ptr::null_mut()
/// if config is null, an error was encountered or if the runtime could not be created
///
/// # Safety
/// The ```destroy_client``` method must be called when finished with a ClientFFI to prevent a memory leak
#[no_mangle]
pub unsafe extern "C" fn create_chat_client(
    config: *mut P2pConfig,
    identity_file_path: *const c_char,
    db_path: *const c_char,
    seed_peers: *mut *mut Peer,
    network_str: *const c_char,
) -> *mut ClientFFI {
    let identity_path = PathBuf::from(
        CStr::from_ptr(identity_file_path)
            .to_str()
            .expect("A non-null identity path should be able to convert to a string"),
    );
    let mut buf = Vec::new();
    File::open(identity_path)
        .expect("Can't open the identity file")
        .read_to_end(&mut buf)
        .expect("Can't read the identity file into buffer");
    let identity: NodeIdentity = serde_json::from_slice(&buf).expect("Can't parse identity file as json");

    let network = Network::from_str(
        CStr::from_ptr(network_str)
            .to_str()
            .expect("A non-null network should be able to be converted to string"),
    )
    .expect("Network is invalid");

    let db_path = PathBuf::from(
        CStr::from_ptr(db_path)
            .to_str()
            .expect("A non-null db path should be able to convert to a string"),
    );

    let mut seed_peers_vec = Vec::new();

    let mut i = 0;
    while !(*seed_peers.offset(i)).is_null() {
        let peer = (**seed_peers.offset(i)).clone();
        seed_peers_vec.push(peer);
        i += 1;
    }

    let runtime = Runtime::new().unwrap();
    let mut client = Client::new(identity, (*config).clone(), seed_peers_vec, db_path, network);

    runtime.block_on(client.initialize());

    let client_ffi = ClientFFI { client, runtime };

    Box::into_raw(Box::new(client_ffi))
}

/// Frees memory for a ClientFFI
///
/// ## Arguments
/// `client` - The pointer of a ClientFFI
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn destroy_client_ffi(client: *mut ClientFFI) {
    if !client.is_null() {
        drop(Box::from_raw(client))
    }
}

/// Sends a message over a client
///
/// ## Arguments
/// `client` - The Client pointer
/// `receiver` - A string containing a tari address
/// `message` - The peer seeds config for the node
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// The ```receiver``` should be destroyed after use
#[no_mangle]
pub unsafe extern "C" fn send_message(
    client: *mut ClientFFI,
    receiver: *mut TariAddress,
    message_c_char: *const c_char,
) {
    let message = CStr::from_ptr(message_c_char)
        .to_str()
        .expect("A non-null message should be able to be converted to string")
        .to_string();

    (*client)
        .runtime
        .block_on((*client).client.send_message((*receiver).clone(), message));
}

/// Sends a message over a client
///
/// ## Arguments
/// `receiver_c_char` - A string containing a tari address hex value
///
/// ## Returns
/// `*mut TariAddress` - A ptr to a TariAddress
///
/// # Safety
/// The ```destroy_tari_address``` function should be called when finished with the TariAddress
#[no_mangle]
pub unsafe extern "C" fn create_tari_address(receiver_c_char: *const c_char) -> *mut TariAddress {
    let receiver = TariAddress::from_str(
        CStr::from_ptr(receiver_c_char)
            .to_str()
            .expect("A non-null receiver should be able to be converted to string"),
    )
    .expect("A TariAddress from str");

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
