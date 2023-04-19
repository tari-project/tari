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

use std::{fs::File, io::Read, path::PathBuf, str::FromStr};

use libc::c_char;
use tari_chat_client::Client;
use tari_common::configuration::Network;
use tari_comms::NodeIdentity;
use tari_p2p::{P2pConfig, PeerSeedsConfig};

const LOG_TARGET: &str = "chat_ffi";

/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn create_chat_client(
    config: *mut P2pConfig,
    identity_file_path: *const c_char,
    peer_seeds_config: *mut PeerSeedsConfig,
    network_str: *const c_char,
) {
    let identity_path = PathBuf::from((*identity_file_path).to_string());
    let mut buf = Vec::new();
    File::open(identity_path)
        .expect("Can't open the identity file")
        .read_to_end(&mut buf)
        .expect("Can't read the identity file into buffer");
    let identity: NodeIdentity = serde_json::from_slice(&buf).expect("Can't parse identity file as json");

    let network = Network::from_str(&(*network_str).to_string()).expect("Network is invalid");
    let mut client = Client::new(identity, (*config).clone(), vec![], network);

    client.initialize();
}
