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

use tari_comms::{
    connection::{Connection, Context, CurveEncryption, Direction, InprocAddress, NetAddress},
    connection_manager::{ConnectionManager, ConnectionManagerError, PeerConnectionConfig},
    peer_manager::PeerFlags,
    test_support::{
        factories::{self, Factory},
        helpers::ConnectionMessageCounter,
    },
};

use tari_storage::lmdb::LMDBStore;

use std::{sync::Arc, time::Duration};

fn make_peer_connection_config(context: &Context, consumer_address: InprocAddress) -> PeerConnectionConfig {
    PeerConnectionConfig {
        context: context.clone(),
        establish_timeout: Duration::from_millis(2000),
        max_message_size: 1024,
        host: "127.0.0.1".parse().unwrap(),
        max_connect_retries: 3,
        consumer_address,
        socks_proxy_address: None,
    }
}
