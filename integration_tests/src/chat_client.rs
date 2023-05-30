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

use std::str::FromStr;

use rand::rngs::OsRng;
use tari_chat_client::{database, Client};
use tari_common::configuration::{MultiaddrList, Network};
use tari_comms::{
    multiaddr::Multiaddr,
    peer_manager::{Peer, PeerFeatures},
    NodeIdentity,
};
use tari_comms_dht::{store_forward::SafConfig, DbConnectionUrl, DhtConfig, NetworkDiscoveryConfig};
use tari_p2p::{P2pConfig, TcpTransportConfig, TransportConfig};

use crate::{get_base_dir, get_port};

pub async fn spawn_chat_client(name: &str, seed_peers: Vec<Peer>) -> Client {
    let port = get_port(18000..18499).unwrap();
    let identity = identity_file(port);
    let config = test_config(name, port, &identity);
    let network = Network::LocalNet;
    let db_path = database::create_chat_storage(&config.datastore_path).unwrap();
    database::create_peer_storage(&config.datastore_path);

    let mut client = Client::new(identity, config, seed_peers, db_path, network);
    client.initialize().await;

    client
}

fn test_config(name: &str, port: u64, identity: &NodeIdentity) -> P2pConfig {
    let temp_dir_path = get_base_dir()
        .join("chat_clients")
        .join(format!("port_{}", port))
        .join(name);

    let mut config = P2pConfig {
        datastore_path: temp_dir_path.clone(),
        dht: DhtConfig {
            database_url: DbConnectionUrl::file("dht.sqlite"),
            network_discovery: NetworkDiscoveryConfig {
                enabled: true,
                ..NetworkDiscoveryConfig::default()
            },
            saf: SafConfig {
                auto_request: true,
                ..Default::default()
            },
            ..DhtConfig::default_local_test()
        },
        transport: TransportConfig::new_tcp(TcpTransportConfig {
            listener_address: identity.first_public_address().expect("No public address"),
            ..TcpTransportConfig::default()
        }),
        allow_test_addresses: true,
        public_addresses: MultiaddrList::from(vec![identity.first_public_address().expect("No public address")]),
        user_agent: "tari/chat-client/0.0.1".to_string(),
        ..P2pConfig::default()
    };
    config.set_base_path(temp_dir_path);
    config
}

fn identity_file(port: u64) -> NodeIdentity {
    let address = Multiaddr::from_str(&format!("/ip4/127.0.0.1/tcp/{}", port)).unwrap();
    NodeIdentity::random(&mut OsRng, address, PeerFeatures::COMMUNICATION_NODE)
}
