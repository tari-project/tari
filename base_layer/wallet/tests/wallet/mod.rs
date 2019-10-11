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

use crate::support::{
    data::{clean_up_sql_database, get_path, init_sql_database},
    utils::{event_stream_count, random_string},
};
use log::*;
use std::{sync::Arc, time::Duration};
use tari_comms::{
    connection::{net_address::NetAddressWithStats, NetAddress, NetAddressesWithStats},
    control_service::ControlServiceConfig,
    peer_manager::{peer::PeerFlags, NodeId, NodeIdentity, Peer, PeerFeatures},
    types::CommsPublicKey,
};
use tari_p2p::{initialization::CommsConfig, services::liveness::handle::LivenessEvent};
use tari_wallet::{
    text_message_service::{handle::TextMessageEvent, model::Contact},
    wallet::WalletConfig,
    Wallet,
};
use tempdir::TempDir;
use tokio::runtime::Runtime;

fn create_peer(public_key: CommsPublicKey, net_address: NetAddress) -> Peer {
    Peer::new(
        public_key.clone(),
        NodeId::from_key(&public_key).unwrap(),
        NetAddressesWithStats::new(vec![NetAddressWithStats::new(net_address.clone())]),
        PeerFlags::empty(),
        PeerFeatures::communication_node_default(),
    )
}

#[test]
fn test_wallet() {
    let runtime = Runtime::new().unwrap();

    let mut rng = rand::OsRng::new().unwrap();

    let db_name1 = "test_wallet1.sqlite3";
    let db_path1 = get_path(Some(db_name1));
    init_sql_database(db_name1);

    let db_name2 = "test_wallet2.sqlite3";
    let db_path2 = get_path(Some(db_name2));
    init_sql_database(db_name2);

    let node_1_identity = NodeIdentity::random(
        &mut rng,
        "127.0.0.1:22523".parse().unwrap(),
        PeerFeatures::communication_node_default(),
    )
    .unwrap();
    let node_2_identity = NodeIdentity::random(
        &mut rng,
        "127.0.0.1:22145".parse().unwrap(),
        PeerFeatures::communication_node_default(),
    )
    .unwrap();
    let comms_config1 = CommsConfig {
        node_identity: Arc::new(node_1_identity.clone()),
        host: "127.0.0.1".parse().unwrap(),
        socks_proxy_address: None,
        control_service: ControlServiceConfig {
            listener_address: node_1_identity.control_service_address(),
            socks_proxy_address: None,
            requested_connection_timeout: Duration::from_millis(2000),
        },
        datastore_path: TempDir::new(random_string(8).as_str())
            .unwrap()
            .path()
            .to_str()
            .unwrap()
            .to_string(),
        peer_database_name: random_string(8),
        inbound_buffer_size: 100,
        outbound_buffer_size: 100,
        dht: Default::default(),
    };
    let comms_config2 = CommsConfig {
        node_identity: Arc::new(node_2_identity.clone()),
        host: "127.0.0.1".parse().unwrap(),
        socks_proxy_address: None,
        control_service: ControlServiceConfig {
            listener_address: node_2_identity.control_service_address(),
            socks_proxy_address: None,
            requested_connection_timeout: Duration::from_millis(2000),
        },
        datastore_path: TempDir::new(random_string(8).as_str())
            .unwrap()
            .path()
            .to_str()
            .unwrap()
            .to_string(),
        peer_database_name: random_string(8),
        inbound_buffer_size: 100,
        outbound_buffer_size: 100,
        dht: Default::default(),
    };
    let config1 = WalletConfig {
        comms_config: comms_config1,
        inbound_message_buffer_size: 100,
        public_key: node_1_identity.identity.public_key.clone(),
        database_path: db_path1,
    };
    let config2 = WalletConfig {
        comms_config: comms_config2,
        inbound_message_buffer_size: 100,
        public_key: node_1_identity.identity.public_key.clone(),
        database_path: db_path2,
    };
    let runtime_node1 = Runtime::new().unwrap();
    let runtime_node2 = Runtime::new().unwrap();
    let mut wallet1 = Wallet::new(config1, runtime_node1).unwrap();
    let mut wallet2 = Wallet::new(config2, runtime_node2).unwrap();

    wallet1
        .comms_service
        .peer_manager()
        .add_peer(create_peer(
            node_2_identity.identity.public_key.clone(),
            node_2_identity.control_service_address(),
        ))
        .unwrap();

    wallet2
        .comms_service
        .peer_manager()
        .add_peer(create_peer(
            node_1_identity.identity.public_key.clone(),
            node_1_identity.control_service_address(),
        ))
        .unwrap();
    error!("Starting tests");
    runtime
        .block_on(wallet2.text_message_service.add_contact(Contact::new(
            "Alice".to_string(),
            node_1_identity.identity.public_key.clone(),
            node_1_identity.control_service_address(),
        )))
        .unwrap();

    runtime
        .block_on(wallet1.text_message_service.add_contact(Contact::new(
            "Bob".to_string(),
            node_2_identity.identity.public_key.clone(),
            node_2_identity.control_service_address(),
        )))
        .unwrap();

    runtime
        .block_on(
            wallet1
                .text_message_service
                .send_text_message(node_2_identity.identity.public_key.clone(), "Say Hello,".to_string()),
        )
        .unwrap();

    runtime
        .block_on(
            wallet2
                .text_message_service
                .send_text_message(node_1_identity.identity.public_key.clone(), "hello?".to_string()),
        )
        .unwrap();

    runtime
        .block_on(wallet1.text_message_service.send_text_message(
            node_2_identity.identity.public_key.clone(),
            "to my little friend!".to_string(),
        ))
        .unwrap();

    let mut result = runtime.block_on(async {
        event_stream_count(
            wallet1.text_message_service.get_event_stream_fused(),
            3,
            Duration::from_secs(10),
        )
        .await
    });
    assert_eq!(result.remove(&TextMessageEvent::ReceivedTextMessage), Some(1));
    assert_eq!(result.remove(&TextMessageEvent::ReceivedTextMessageAck), Some(2));

    runtime
        .block_on(
            wallet1
                .liveness_service
                .send_ping(node_2_identity.identity.public_key.clone()),
        )
        .unwrap();

    runtime
        .block_on(
            wallet2
                .liveness_service
                .send_ping(node_1_identity.identity.public_key.clone()),
        )
        .unwrap();

    let mut result = runtime.block_on(async {
        event_stream_count(
            wallet1.liveness_service.get_event_stream_fused(),
            2,
            Duration::from_secs(3),
        )
        .await
    });
    assert_eq!(result.remove(&LivenessEvent::ReceivedPing), Some(1));

    let mut result = runtime.block_on(async {
        event_stream_count(
            wallet2.liveness_service.get_event_stream_fused(),
            2,
            Duration::from_secs(3),
        )
        .await
    });

    assert_eq!(result.remove(&LivenessEvent::ReceivedPing), Some(1));

    clean_up_sql_database(db_name1);
    clean_up_sql_database(db_name2);
}
