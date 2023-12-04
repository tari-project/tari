//   Copyright 2023. The Tari Project
//
//   Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//   following conditions are met:
//
//   1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//   disclaimer.
//
//   2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//   following disclaimer in the documentation and/or other materials provided with the distribution.
//
//   3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//   products derived from this software without specific prior written permission.
//
//   THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//   INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//   DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//   SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//   SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//   WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//   USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{path::PathBuf, str::FromStr};

use minotari_app_utilities::identity_management::setup_node_identity;
use tari_chat_client::{
    config::{ApplicationConfig, ChatClientConfig},
    database,
    Client,
};
use tari_common::configuration::MultiaddrList;
use tari_comms::{
    multiaddr::Multiaddr,
    peer_manager::{Peer, PeerFeatures},
};

use crate::get_port;

pub async fn spawn_chat_client(name: &str, seed_peers: Vec<Peer>, base_dir: PathBuf) -> Client {
    let port = get_port(18000..18499).unwrap();
    let address = Multiaddr::from_str(&format!("/ip4/127.0.0.1/tcp/{}", port)).unwrap();

    let base_dir = base_dir.join("chat_clients").join(format!("{}_port_{}", name, port));

    let mut config = test_config(address.clone());
    config.chat_client.set_base_path(base_dir);

    let identity = setup_node_identity(
        &config.chat_client.identity_file,
        vec![address],
        true,
        PeerFeatures::COMMUNICATION_NODE,
    )
    .unwrap();

    database::create_chat_storage(&config.chat_client.db_file).unwrap();
    database::create_peer_storage(&config.chat_client.data_dir).unwrap();

    config.peer_seeds.peer_seeds = seed_peers
        .iter()
        .map(|p| p.to_short_string())
        .collect::<Vec<String>>()
        .into();

    let mut client = Client::new(identity, config);

    client.initialize().await.expect("the chat client to spawn");
    client
}

pub fn test_config(address: Multiaddr) -> ApplicationConfig {
    let mut chat_client_config = ChatClientConfig::default_local_test();
    chat_client_config.p2p.transport.tcp.listener_address = address.clone();
    chat_client_config.p2p.public_addresses = MultiaddrList::from(vec![address]);
    chat_client_config.log_path = Some(PathBuf::from("log/chat_client/chat_client.log"));
    chat_client_config.log_verbosity = Some(11);

    ApplicationConfig {
        chat_client: chat_client_config,
        ..ApplicationConfig::default()
    }
}
