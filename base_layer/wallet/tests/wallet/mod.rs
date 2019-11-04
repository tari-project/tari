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

use crate::support::utils::{event_stream_count, make_input, random_string};
use std::{sync::Arc, time::Duration};
use tari_comms::{
    connection::{net_address::NetAddressWithStats, NetAddress, NetAddressesWithStats},
    control_service::ControlServiceConfig,
    peer_manager::{peer::PeerFlags, NodeId, NodeIdentity, Peer, PeerFeatures},
    types::CommsPublicKey,
};
use tari_p2p::{initialization::CommsConfig, services::liveness::handle::LivenessEvent};
use tari_transactions::tari_amount::MicroTari;
use tari_wallet::{
    output_manager_service::OutputManagerConfig,
    transaction_service::handle::TransactionEvent,
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
        PeerFeatures::COMMUNICATION_NODE,
    )
}

#[test]
fn test_wallet() {
    let runtime = Runtime::new().unwrap();

    let mut rng = rand::OsRng::new().unwrap();

    let alice_identity = NodeIdentity::random(
        &mut rng,
        "127.0.0.1:22523".parse().unwrap(),
        PeerFeatures::COMMUNICATION_NODE,
    )
    .unwrap();
    let bob_identity = NodeIdentity::random(
        &mut rng,
        "127.0.0.1:22145".parse().unwrap(),
        PeerFeatures::COMMUNICATION_NODE,
    )
    .unwrap();
    let comms_config1 = CommsConfig {
        node_identity: Arc::new(alice_identity.clone()),
        host: "127.0.0.1".parse().unwrap(),
        socks_proxy_address: None,
        control_service: ControlServiceConfig {
            listener_address: alice_identity.control_service_address(),
            socks_proxy_address: None,
            requested_connection_timeout: Duration::from_millis(2000),
        },
        datastore_path: TempDir::new(random_string(8).as_str())
            .unwrap()
            .path()
            .to_str()
            .unwrap()
            .to_string(),
        establish_connection_timeout: Duration::from_secs(10),
        peer_database_name: random_string(8),
        inbound_buffer_size: 100,
        outbound_buffer_size: 100,
        dht: Default::default(),
    };
    let comms_config2 = CommsConfig {
        node_identity: Arc::new(bob_identity.clone()),
        host: "127.0.0.1".parse().unwrap(),
        socks_proxy_address: None,
        control_service: ControlServiceConfig {
            listener_address: bob_identity.control_service_address(),
            socks_proxy_address: None,
            requested_connection_timeout: Duration::from_millis(2000),
        },
        datastore_path: TempDir::new(random_string(8).as_str())
            .unwrap()
            .path()
            .to_str()
            .unwrap()
            .to_string(),
        establish_connection_timeout: Duration::from_secs(10),
        peer_database_name: random_string(8),
        inbound_buffer_size: 100,
        outbound_buffer_size: 100,
        dht: Default::default(),
    };
    let config1 = WalletConfig {
        comms_config: comms_config1,
        output_manager_config: OutputManagerConfig {
            master_key: Some(alice_identity.secret_key().clone()),
            seed_words: None,
            branch_seed: "".to_string(),
            primary_key_index: 0,
        },
    };
    let config2 = WalletConfig {
        comms_config: comms_config2,
        output_manager_config: OutputManagerConfig {
            master_key: Some(bob_identity.secret_key().clone()),
            seed_words: None,
            branch_seed: "".to_string(),
            primary_key_index: 0,
        },
    };
    let runtime_node1 = Runtime::new().unwrap();
    let runtime_node2 = Runtime::new().unwrap();
    let mut alice_wallet = Wallet::new(config1, runtime_node1).unwrap();
    let mut bob_wallet = Wallet::new(config2, runtime_node2).unwrap();

    alice_wallet
        .comms
        .peer_manager()
        .add_peer(create_peer(
            bob_identity.public_key().clone(),
            bob_identity.control_service_address(),
        ))
        .unwrap();

    bob_wallet
        .comms
        .peer_manager()
        .add_peer(create_peer(
            alice_identity.public_key().clone(),
            alice_identity.control_service_address(),
        ))
        .unwrap();

    runtime
        .block_on(
            alice_wallet
                .liveness_service
                .send_ping(bob_identity.public_key().clone()),
        )
        .unwrap();

    runtime
        .block_on(
            bob_wallet
                .liveness_service
                .send_ping(alice_identity.public_key().clone()),
        )
        .unwrap();

    let mut result = runtime.block_on(async {
        event_stream_count(
            alice_wallet.liveness_service.get_event_stream_fused(),
            2,
            Duration::from_secs(5),
        )
        .await
    });
    assert_eq!(result.remove(&LivenessEvent::ReceivedPing), Some(1));

    let mut result = runtime.block_on(async {
        event_stream_count(
            bob_wallet.liveness_service.get_event_stream_fused(),
            2,
            Duration::from_secs(5),
        )
        .await
    });

    assert_eq!(result.remove(&LivenessEvent::ReceivedPing), Some(1));

    let alice_event_stream = alice_wallet.transaction_service.get_event_stream_fused();

    let value = MicroTari::from(1000);
    let (_utxo, uo1) = make_input(&mut rng, MicroTari(2500));

    runtime
        .block_on(alice_wallet.output_manager_service.add_output(uo1))
        .unwrap();

    runtime
        .block_on(alice_wallet.transaction_service.send_transaction(
            bob_identity.public_key().clone(),
            value,
            MicroTari::from(20),
        ))
        .unwrap();

    let mut result =
        runtime.block_on(async { event_stream_count(alice_event_stream, 1, Duration::from_secs(10)).await });
    assert_eq!(result.remove(&TransactionEvent::ReceivedTransactionReply), Some(1));
}
