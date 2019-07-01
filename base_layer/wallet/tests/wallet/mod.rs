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

use crate::support::utils::assert_change;
use std::time::Duration;
use tari_comms::{
    connection::{net_address::NetAddressWithStats, NetAddress, NetAddressesWithStats},
    control_service::ControlServiceConfig,
    peer_manager::{peer::PeerFlags, NodeId, Peer},
    types::{CommsPublicKey, CommsSecretKey},
};
use tari_crypto::keys::{PublicKey, SecretKey};
use tari_p2p::{
    initialization::CommsConfig,
    tari_message::{NetMessage, TariMessageType},
};
use tari_wallet::{wallet::WalletConfig, Wallet};

fn create_peer(public_key: CommsPublicKey, net_address: NetAddress) -> Peer {
    Peer::new(
        public_key.clone(),
        NodeId::from_key(&public_key).unwrap(),
        NetAddressesWithStats::new(vec![NetAddressWithStats::new(net_address.clone())]),
        PeerFlags::empty(),
    )
}

#[test]
fn test_wallet() {
    let mut rng = rand::OsRng::new().unwrap();

    let listener_address1: NetAddress = "127.0.0.1:32775".parse().unwrap();
    let secret_key1 = CommsSecretKey::random(&mut rng);
    let public_key1 = CommsPublicKey::from_secret_key(&secret_key1);
    let config1 = WalletConfig {
        comms: CommsConfig {
            control_service: ControlServiceConfig {
                listener_address: listener_address1.clone(),
                socks_proxy_address: None,
                accept_message_type: TariMessageType::new(NetMessage::Accept),
                requested_outbound_connection_timeout: Duration::from_millis(5000),
            },
            socks_proxy_address: None,
            host: "127.0.0.1".parse().unwrap(),
            public_key: public_key1.clone(),
            secret_key: secret_key1,
        },
        public_key: public_key1.clone(),
    };
    let wallet1 = Wallet::new(config1).unwrap();

    let listener_address2: NetAddress = "127.0.0.1:32776".parse().unwrap();
    let secret_key2 = CommsSecretKey::random(&mut rng);
    let public_key2 = CommsPublicKey::from_secret_key(&secret_key2);
    let config2 = WalletConfig {
        comms: CommsConfig {
            control_service: ControlServiceConfig {
                listener_address: listener_address2.clone(),
                socks_proxy_address: None,
                accept_message_type: TariMessageType::new(NetMessage::Accept),
                requested_outbound_connection_timeout: Duration::from_millis(5000),
            },
            socks_proxy_address: None,
            host: "127.0.0.1".parse().unwrap(),
            public_key: public_key2.clone(),
            secret_key: secret_key2,
        },
        public_key: public_key2.clone(),
    };

    let wallet2 = Wallet::new(config2).unwrap();

    wallet1
        .comms_services
        .peer_manager()
        .add_peer(create_peer(public_key2.clone(), listener_address2))
        .unwrap();

    wallet2
        .comms_services
        .peer_manager()
        .add_peer(create_peer(public_key1.clone(), listener_address1))
        .unwrap();

    wallet1
        .text_message_service
        .send_text_message(public_key2.clone(), "Say Hello,".to_string())
        .unwrap();

    wallet2
        .text_message_service
        .send_text_message(public_key1.clone(), "hello?".to_string())
        .unwrap();

    wallet1
        .text_message_service
        .send_text_message(public_key2.clone(), "to my little friend!".to_string())
        .unwrap();

    assert_change(
        || {
            let msgs = wallet1.text_message_service.get_text_messages().unwrap();
            (msgs.sent_messages.len(), msgs.received_messages.len())
        },
        (2, 1),
        50,
    );

    assert_change(
        || {
            let msgs = wallet2.text_message_service.get_text_messages().unwrap();
            (msgs.sent_messages.len(), msgs.received_messages.len())
        },
        (1, 2),
        50,
    );

    wallet1.ping_pong_service.ping(public_key2.clone()).unwrap();
    wallet2.ping_pong_service.ping(public_key1.clone()).unwrap();

    assert_change(|| wallet1.ping_pong_service.ping_count().unwrap(), 1, 20);
    assert_change(|| wallet1.ping_pong_service.pong_count().unwrap(), 1, 20);
    assert_change(|| wallet2.ping_pong_service.ping_count().unwrap(), 1, 20);
    assert_change(|| wallet2.ping_pong_service.pong_count().unwrap(), 1, 20);
}
