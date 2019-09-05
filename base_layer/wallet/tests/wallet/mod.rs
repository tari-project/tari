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
use futures::executor::ThreadPool;
use std::{path::PathBuf, time::Duration};
use tari_comms::{
    connection::{net_address::NetAddressWithStats, NetAddress, NetAddressesWithStats},
    control_service::ControlServiceConfig,
    peer_manager::{peer::PeerFlags, NodeId, Peer},
    types::{CommsPublicKey, CommsSecretKey},
};
use tari_crypto::keys::{PublicKey, SecretKey};
use tari_p2p::initialization::CommsConfig;
use tari_wallet::{text_message_service::Contact, wallet::WalletConfig, Wallet};

fn create_peer(public_key: CommsPublicKey, net_address: NetAddress) -> Peer {
    Peer::new(
        public_key.clone(),
        NodeId::from_key(&public_key).unwrap(),
        NetAddressesWithStats::new(vec![NetAddressWithStats::new(net_address.clone())]),
        PeerFlags::empty(),
    )
}

fn get_path(name: Option<&str>) -> String {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/data");
    path.push(name.unwrap_or(""));
    path.to_str().unwrap().to_string()
}

fn clean_up_datastore(name: &str) {
    std::fs::remove_dir_all(get_path(Some(name))).unwrap();
}

fn clean_up_sql_database(name: &str) {
    if std::fs::metadata(get_path(Some(name))).is_ok() {
        std::fs::remove_file(get_path(Some(name))).unwrap();
    }
}

fn init_sql_database(name: &str) {
    clean_up_sql_database(name);
    let path = get_path(None);
    let _ = std::fs::create_dir(&path).unwrap_or_default();
}

#[test]
fn test_wallet() {
    let mut rng = rand::OsRng::new().unwrap();

    let db_name1 = "test_wallet1.sqlite3";
    let db_path1 = get_path(Some(db_name1));
    init_sql_database(db_name1);

    let db_name2 = "test_wallet2.sqlite3";
    let db_path2 = get_path(Some(db_name2));
    init_sql_database(db_name2);

    let listener_address1: NetAddress = "127.0.0.1:32775".parse().unwrap();
    let secret_key1 = CommsSecretKey::random(&mut rng);
    let public_key1 = CommsPublicKey::from_secret_key(&secret_key1);
    let wallet1_peer_database_name = "wallet1_peer_database".to_string();
    let config1 = WalletConfig {
        comms: CommsConfig {
            control_service: ControlServiceConfig {
                listener_address: listener_address1.clone(),
                socks_proxy_address: None,
                requested_connection_timeout: Duration::from_millis(5000),
            },
            socks_proxy_address: None,
            host: "127.0.0.1".parse().unwrap(),
            public_key: public_key1.clone(),
            secret_key: secret_key1,
            public_address: listener_address1.clone(),
            datastore_path: get_path(Some(&wallet1_peer_database_name)),
            peer_database_name: wallet1_peer_database_name.clone(),
        },
        public_key: public_key1.clone(),
        database_path: db_path1,
    };
    let mut thread_pool = ThreadPool::new().expect("Could not start Futures ThreadPool");

    let wallet1 = Wallet::new(config1, &mut thread_pool).unwrap();

    let listener_address2: NetAddress = "127.0.0.1:32776".parse().unwrap();
    let secret_key2 = CommsSecretKey::random(&mut rng);
    let public_key2 = CommsPublicKey::from_secret_key(&secret_key2);
    let wallet2_peer_database_name = "wallet2_peer_database".to_string();
    let config2 = WalletConfig {
        comms: CommsConfig {
            control_service: ControlServiceConfig {
                listener_address: listener_address2.clone(),
                socks_proxy_address: None,
                requested_connection_timeout: Duration::from_millis(5000),
            },
            socks_proxy_address: None,
            host: "127.0.0.1".parse().unwrap(),
            public_key: public_key2.clone(),
            secret_key: secret_key2,
            public_address: listener_address2.clone(),
            datastore_path: get_path(Some(&wallet2_peer_database_name)),
            peer_database_name: wallet2_peer_database_name.clone(),
        },
        public_key: public_key2.clone(),
        database_path: db_path2,
    };

    let wallet2 = Wallet::new(config2, &mut thread_pool).unwrap();

    wallet1
        .comms_services
        .peer_manager()
        .add_peer(create_peer(public_key2.clone(), listener_address2.clone()))
        .unwrap();

    wallet2
        .comms_services
        .peer_manager()
        .add_peer(create_peer(public_key1.clone(), listener_address1.clone()))
        .unwrap();

    wallet1
        .text_message_service
        .add_contact(Contact::new(
            "Alice".to_string(),
            public_key2.clone(),
            listener_address2,
        ))
        .unwrap();

    wallet2
        .text_message_service
        .add_contact(Contact::new("Bob".to_string(), public_key1.clone(), listener_address1))
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

    assert_change(|| wallet1.ping_pong_service.ping_count().unwrap(), 2, 20);
    assert_change(|| wallet1.ping_pong_service.pong_count().unwrap(), 2, 20);
    assert_change(|| wallet2.ping_pong_service.ping_count().unwrap(), 2, 20);
    assert_change(|| wallet2.ping_pong_service.pong_count().unwrap(), 2, 20);

    clean_up_datastore(&wallet1_peer_database_name);
    clean_up_datastore(&wallet2_peer_database_name);
    clean_up_sql_database(db_name1);
    clean_up_sql_database(db_name2);
}
