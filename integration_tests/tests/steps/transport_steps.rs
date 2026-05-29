//   Copyright 2026. The Tari Project
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

use cucumber::{given, then};
use tari_common_sqlite::connection::DbConnection;
use tari_comms::{
    multiaddr::Multiaddr,
    net_address::{MultiaddressesWithStats, PeerAddressSource},
    peer_manager::{
        NodeId,
        Peer,
        PeerFeatures,
        PeerFlags,
        database::{MIGRATIONS, PeerDatabaseSql},
    },
    types::{CommsPublicKey, TransportProtocol, sort_multiaddr_by_transport_preference, transport_protocol_rank},
};
use tari_integration_tests::TariWorld;
use tari_p2p::{TcpTransportConfig, TorTransportConfig, TransportConfig, TransportType};

const ONION3_HOST: &str = "abcdefghijklmnopqrstuvwxyz234567abcdefghijklmnopqrstuvwx";

#[given("transport peer selection fixtures are available")]
async fn transport_peer_selection_fixtures_are_available(_world: &mut TariWorld) {}

#[then(regex = r"^transport mode (tor|tcp|tor_tcp|tcp_tor) selects peers in order ([a-z,]+)$")]
async fn transport_mode_selects_peers_in_order(_world: &mut TariWorld, mode: String, expected: String) {
    let transport_config = transport_config_from_name(&mode);
    let (peers_db, tcp_peer, onion_peer) = peer_selection_fixture();

    let candidates = peers_db
        .get_available_dial_candidates(&[], None, &transport_config.get_supported_protocols(), false, false)
        .unwrap();
    let actual = candidates
        .iter()
        .map(|peer| peer_protocol_name(peer, &tcp_peer.node_id, &onion_peer.node_id))
        .collect::<Vec<_>>();

    assert_eq!(actual, expected_names(&expected));
}

#[then(regex = r"^transport mode (tor|tcp|tor_tcp|tcp_tor) dials addresses in order ([a-z,]+)$")]
async fn transport_mode_dials_addresses_in_order(_world: &mut TariWorld, mode: String, expected: String) {
    let transport_config = transport_config_from_name(&mode);
    let preferred_protocols = transport_config.get_supported_protocols();
    let mut addresses = vec![onion_address(), tcp_address()];

    addresses.retain(|address| transport_protocol_rank(address, &preferred_protocols).is_some());
    sort_multiaddr_by_transport_preference(&mut addresses, &preferred_protocols);

    let actual = addresses.iter().map(address_protocol_name).collect::<Vec<_>>();
    assert_eq!(actual, expected_names(&expected));
}

fn transport_config_from_name(name: &str) -> TransportConfig {
    match name {
        "tor" => TransportConfig::new_tor(TorTransportConfig::default()),
        "tcp" => TransportConfig::new_tcp(TcpTransportConfig::default()),
        "tor_tcp" => TransportConfig {
            transport_type: TransportType::TorTcp,
            ..Default::default()
        },
        "tcp_tor" => TransportConfig::default(),
        other => panic!("unknown transport type: {other}"),
    }
}

fn peer_selection_fixture() -> (PeerDatabaseSql, Peer, Peer) {
    let db_connection = DbConnection::connect_temp_file_and_migrate(MIGRATIONS).unwrap();
    let peers_db = PeerDatabaseSql::new(db_connection, &peer_with_addresses(vec![tcp_address()])).unwrap();

    let tcp_peer = peer_with_addresses(vec![tcp_address()]);
    let onion_peer = peer_with_addresses(vec![onion_address()]);
    peers_db.add_or_update_peer(tcp_peer.clone()).unwrap();
    peers_db.add_or_update_peer(onion_peer.clone()).unwrap();

    (peers_db, tcp_peer, onion_peer)
}

fn peer_with_addresses(addresses: Vec<Multiaddr>) -> Peer {
    let (_sk, pk) = CommsPublicKey::random_keypair(&mut rand::rng());
    let node_id = NodeId::from_key(&pk);
    let addresses = MultiaddressesWithStats::from_addresses_with_source(addresses, &PeerAddressSource::Config);

    Peer::new(
        pk,
        node_id,
        addresses,
        PeerFlags::default(),
        PeerFeatures::COMMUNICATION_NODE,
        Default::default(),
        Default::default(),
    )
}

fn tcp_address() -> Multiaddr {
    "/ip4/1.2.3.4/tcp/18189".parse().unwrap()
}

fn onion_address() -> Multiaddr {
    format!("/onion3/{ONION3_HOST}:18141").parse().unwrap()
}

fn peer_protocol_name(peer: &Peer, tcp_node_id: &NodeId, onion_node_id: &NodeId) -> &'static str {
    if &peer.node_id == tcp_node_id {
        "tcp"
    } else if &peer.node_id == onion_node_id {
        "onion"
    } else {
        panic!("unexpected peer selected: {}", peer.node_id)
    }
}

fn address_protocol_name(address: &Multiaddr) -> &'static str {
    match TransportProtocol::from(address) {
        TransportProtocol::Ipv4 | TransportProtocol::Ipv6 => "tcp",
        TransportProtocol::Onion => "onion",
        TransportProtocol::Memory => "memory",
    }
}

fn expected_names(expected: &str) -> Vec<&str> {
    expected.split(',').collect()
}
