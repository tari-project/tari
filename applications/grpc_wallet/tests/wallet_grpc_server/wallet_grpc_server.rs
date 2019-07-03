// Copyright 2019. The Tari Project
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

use crossbeam_channel::bounded;
use futures::future::Future;
use hyper::client::connect::{Destination, HttpConnector};
use log::{Level, *};
use rand::{distributions::Alphanumeric, rngs::OsRng, Rng};
use std::{iter, sync::Arc, thread, time::Duration};
use tari_comms::{
    connection::{net_address::NetAddressWithStats, NetAddress, NetAddressesWithStats},
    control_service::ControlServiceConfig,
    peer_manager::{peer::PeerFlags, NodeId, Peer},
    types::{CommsPublicKey, CommsSecretKey},
};
use tari_crypto::keys::{PublicKey, SecretKey};
use tari_grpc_wallet::grpc_interface::wallet_rpc::{
    client::WalletRpc,
    Contact as ContactRpc,
    RpcResponse,
    ScreenName as ScreenNameRpc,
    TextMessageToSend as TextMessageToSendRpc,
    VoidParams,
};

use tari_grpc_wallet::wallet_server::WalletServer;
use tari_p2p::{
    initialization::CommsConfig,
    tari_message::{NetMessage, TariMessageType},
};
use tari_utilities::message_format::MessageFormat;
use tari_wallet::{wallet::WalletConfig, Wallet};
use tempdir::TempDir;
use tower_grpc::Request;
use tower_hyper::{client, util};
use tower_util::MakeService;

const LOG_TARGET: &'static str = "applications::grpc_wallet";
const WALLET_GRPC_PORT: u32 = 26778;

pub fn init() {
    let _ = simple_logger::init_with_level(Level::Debug);
}

fn send_text_message_request(msg: TextMessageToSendRpc, desired_response: RpcResponse) {
    let (tx, rx) = bounded(1);

    let uri: http::Uri = format!("http://127.0.0.1:{}", WALLET_GRPC_PORT).parse().unwrap();

    let dst = Destination::try_from_uri(uri.clone()).unwrap();
    let connector = util::Connector::new(HttpConnector::new(1));
    let settings = client::Builder::new().http2_only(true).clone();
    let mut make_client = client::Connect::with_builder(connector, settings);

    let send_text_message = make_client
        .make_service(dst.clone())
        .map_err(|e| panic!("connect error: {:?}", e))
        .and_then(move |conn| {
            let conn = tower_request_modifier::Builder::new()
                .set_origin(uri.clone())
                .build(conn)
                .unwrap();

            // Wait until the client is ready...
            WalletRpc::new(conn).ready()
        })
        .and_then(|mut client| client.send_text_message(Request::new(msg)))
        .and_then(move |response| {
            info!(target: LOG_TARGET, "SendTextMessage Response received: {:?}", response);
            let inbound = response.into_inner();

            let _ = tx.send(inbound);

            Ok(())
        })
        .map_err(|e| {
            panic!("RPC Client error = {:?}", e);
        });

    tokio::run(send_text_message);
    thread::sleep(Duration::from_millis(100));

    let inbound = rx.recv().unwrap();

    assert_eq!(inbound.success, desired_response.success);
    assert_eq!(inbound.message, desired_response.message);
}

fn get_text_messages_request(sent_messages: Vec<String>, received_messages: Vec<String>, contact: Option<ContactRpc>) {
    let mut recv_msg: Vec<String> = Vec::new();
    let mut send_msg: Vec<String> = Vec::new();

    // Check for new text messages up to 40 times with 100ms wait in between = 4000ms Timeout before moving on
    for _ in 0..40 {
        let move_contact = contact.clone();
        let (tx, rx) = bounded(2);
        let uri: http::Uri = format!("http://127.0.0.1:{}", WALLET_GRPC_PORT).parse().unwrap();
        let dst = Destination::try_from_uri(uri.clone()).unwrap();
        let connector = util::Connector::new(HttpConnector::new(1));
        let settings = client::Builder::new().http2_only(true).clone();

        let mut make_client = client::Connect::with_builder(connector, settings.clone());
        let get_text_messages = make_client
            .make_service(dst.clone())
            .map_err(|e| panic!("connect error: {:?}", e))
            .and_then(move |conn| {
                let conn = tower_request_modifier::Builder::new()
                    .set_origin(uri.clone())
                    .build(conn)
                    .unwrap();

                // Wait until the client is ready...
                WalletRpc::new(conn).ready()
            })
            .and_then(|mut client| {
                if move_contact.is_some() {
                    client.get_text_messages_by_contact(Request::new(move_contact.unwrap()))
                } else {
                    client.get_text_messages(Request::new(VoidParams {}))
                }
            })
            .and_then(move |response| {
                info!(target: LOG_TARGET, "GetTextMessages Response received: {:?}", response);
                let inbound = response.into_inner();

                let recv_msg = inbound.received_messages.iter().map(|m| m.message.clone()).collect();

                let sent_msg = inbound.sent_messages.iter().map(|m| m.message.clone()).collect();

                let _ = tx.send(recv_msg);
                let _ = tx.send(sent_msg);

                Ok(())
            })
            .map_err(|e| {
                panic!("RPC Client error = {:?}", e);
            });

        tokio::run(get_text_messages);

        recv_msg = rx.recv().unwrap();
        send_msg = rx.recv().unwrap();

        if recv_msg.len() > 0 && send_msg.len() > 0 {
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }

    assert_eq!(recv_msg.len(), received_messages.len());
    assert_eq!(send_msg.len(), sent_messages.len());

    recv_msg
        .iter()
        .for_each(|m| assert!(received_messages.iter().any(|m2| m == m2)));
    send_msg
        .iter()
        .for_each(|m| assert!(sent_messages.iter().any(|m2| m == m2)));
}

fn set_get_screen_name(name: String) {
    let requested_name = name.clone();
    let uri: http::Uri = format!("http://127.0.0.1:{}", WALLET_GRPC_PORT).parse().unwrap();

    let dst = Destination::try_from_uri(uri.clone()).unwrap();
    let connector = util::Connector::new(HttpConnector::new(1));
    let settings = client::Builder::new().http2_only(true).clone();
    let mut make_client = client::Connect::with_builder(connector, settings.clone());

    let set_screen_name = make_client
        .make_service(dst.clone())
        .map_err(|e| panic!("connect error: {:?}", e))
        .and_then(move |conn| {
            let conn = tower_request_modifier::Builder::new()
                .set_origin(uri.clone())
                .build(conn)
                .unwrap();

            // Wait until the client is ready...
            WalletRpc::new(conn).ready()
        })
        .and_then(|mut client| client.set_screen_name(Request::new(ScreenNameRpc { screen_name: name })))
        .and_then(move |response| {
            info!(target: LOG_TARGET, "SetScreenName Response received: {:?}", response);
            let inbound = response.into_inner();
            assert_eq!(inbound.success, true);
            Ok(())
        })
        .map_err(|e| {
            panic!("RPC Client error = {:?}", e);
        });

    tokio::run(set_screen_name);
    thread::sleep(Duration::from_millis(100));
    let (tx, rx) = bounded(1);
    let uri: http::Uri = format!("http://127.0.0.1:{}", WALLET_GRPC_PORT).parse().unwrap();
    let connector = util::Connector::new(HttpConnector::new(1));
    let settings = client::Builder::new().http2_only(true).clone();
    let mut make_client = client::Connect::with_builder(connector, settings);

    let get_screen_name = make_client
        .make_service(dst.clone())
        .map_err(|e| panic!("connect error: {:?}", e))
        .and_then(move |conn| {
            let conn = tower_request_modifier::Builder::new()
                .set_origin(uri.clone())
                .build(conn)
                .unwrap();

            // Wait until the client is ready...
            WalletRpc::new(conn).ready()
        })
        .and_then(|mut client| client.get_screen_name(Request::new(VoidParams {})))
        .and_then(move |response| {
            info!(target: LOG_TARGET, "GetScreenName Response received: {:?}", response);

            let _ = tx.send(response.into_inner());

            Ok(())
        })
        .map_err(|e| {
            panic!("RPC Client error = {:?}", e);
        });

    tokio::run(get_screen_name);

    let recv_screen_name = rx.recv().unwrap();
    assert_eq!(recv_screen_name.screen_name, requested_name);
}

fn get_pub_key(pub_key: String) {
    let (tx, rx) = bounded(1);
    let uri: http::Uri = format!("http://127.0.0.1:{}", WALLET_GRPC_PORT).parse().unwrap();
    let dst = Destination::try_from_uri(uri.clone()).unwrap();
    let connector = util::Connector::new(HttpConnector::new(1));
    let settings = client::Builder::new().http2_only(true).clone();
    let mut make_client = client::Connect::with_builder(connector, settings);

    let get_pub_key = make_client
        .make_service(dst.clone())
        .map_err(|e| panic!("connect error: {:?}", e))
        .and_then(move |conn| {
            let conn = tower_request_modifier::Builder::new()
                .set_origin(uri.clone())
                .build(conn)
                .unwrap();

            // Wait until the client is ready...
            WalletRpc::new(conn).ready()
        })
        .and_then(|mut client| client.get_public_key(Request::new(VoidParams {})))
        .and_then(move |response| {
            info!(target: LOG_TARGET, "GetPubKey Response received: {:?}", response);

            let _ = tx.send(response.into_inner());

            Ok(())
        })
        .map_err(|e| {
            panic!("RPC Client error = {:?}", e);
        });

    tokio::run(get_pub_key);

    let recv_pub_key = rx.recv().unwrap();
    assert_eq!(recv_pub_key.pub_key, pub_key);
}

fn add_contact(contact: ContactRpc) {
    let uri: http::Uri = format!("http://127.0.0.1:{}", WALLET_GRPC_PORT).parse().unwrap();
    let dst = Destination::try_from_uri(uri.clone()).unwrap();
    let connector = util::Connector::new(HttpConnector::new(1));
    let settings = client::Builder::new().http2_only(true).clone();
    let mut make_client = client::Connect::with_builder(connector, settings.clone());

    let add_contact = make_client
        .make_service(dst.clone())
        .map_err(|e| panic!("connect error: {:?}", e))
        .and_then(move |conn| {
            let conn = tower_request_modifier::Builder::new()
                .set_origin(uri.clone())
                .build(conn)
                .unwrap();

            // Wait until the client is ready...
            WalletRpc::new(conn).ready()
        })
        .and_then(|mut client| client.add_contact(Request::new(contact)))
        .and_then(move |response| {
            info!(target: LOG_TARGET, "AddContact Response received: {:?}", response);
            let inbound = response.into_inner();
            assert_eq!(inbound.success, true);
            Ok(())
        })
        .map_err(|e| {
            panic!("RPC Client error = {:?}", e);
        });

    tokio::run(add_contact);
}

fn contacts_crud() {
    let mut rng = rand::OsRng::new().unwrap();

    let mut contacts: Vec<ContactRpc> = Vec::new();
    let screen_names = vec!["Andy".to_string(), "Bob".to_string(), "Carol".to_string()];
    for i in 0..3 {
        let contact_secret_key = CommsSecretKey::random(&mut rng);
        let contact_public_key = CommsPublicKey::from_secret_key(&contact_secret_key);
        contacts.push(ContactRpc {
            screen_name: screen_names[i].clone(),
            pub_key: contact_public_key.to_base64().unwrap(),
            address: "127.0.0.1:37522".to_string(),
        });
    }

    add_contact(contacts[0].clone());
    thread::sleep(Duration::from_millis(50));

    add_contact(contacts[1].clone());
    thread::sleep(Duration::from_millis(50));

    add_contact(contacts[2].clone());
    thread::sleep(Duration::from_millis(50));

    // Remove a contact
    let move_contact = contacts[1].clone();
    let uri: http::Uri = format!("http://127.0.0.1:{}", WALLET_GRPC_PORT).parse().unwrap();
    let dst = Destination::try_from_uri(uri.clone()).unwrap();
    let connector = util::Connector::new(HttpConnector::new(1));
    let settings = client::Builder::new().http2_only(true).clone();
    let mut make_client = client::Connect::with_builder(connector, settings);

    let remove_contact = make_client
        .make_service(dst.clone())
        .map_err(|e| panic!("connect error: {:?}", e))
        .and_then(move |conn| {
            let conn = tower_request_modifier::Builder::new()
                .set_origin(uri.clone())
                .build(conn)
                .unwrap();

            // Wait until the client is ready...
            WalletRpc::new(conn).ready()
        })
        .and_then(|mut client| client.remove_contact(Request::new(move_contact)))
        .and_then(move |response| {
            info!(target: LOG_TARGET, "RemoveContact Response received: {:?}", response);

            let inbound = response.into_inner();
            assert_eq!(inbound.success, true);

            Ok(())
        })
        .map_err(|e| {
            panic!("RPC Client error = {:?}", e);
        });

    tokio::run(remove_contact);
    thread::sleep(Duration::from_millis(100));

    // Update a contact
    let updated_contact = ContactRpc {
        screen_name: "Updated".to_string(),
        pub_key: contacts[0].pub_key.clone(),
        address: contacts[0].address.clone(),
    };
    let uri: http::Uri = format!("http://127.0.0.1:{}", WALLET_GRPC_PORT).parse().unwrap();
    let dst = Destination::try_from_uri(uri.clone()).unwrap();
    let connector = util::Connector::new(HttpConnector::new(1));
    let settings = client::Builder::new().http2_only(true).clone();
    let mut make_client = client::Connect::with_builder(connector, settings);

    let update_contact = make_client
        .make_service(dst.clone())
        .map_err(|e| panic!("connect error: {:?}", e))
        .and_then(move |conn| {
            let conn = tower_request_modifier::Builder::new()
                .set_origin(uri.clone())
                .build(conn)
                .unwrap();

            // Wait until the client is ready...
            WalletRpc::new(conn).ready()
        })
        .and_then(|mut client| client.update_contact(Request::new(updated_contact)))
        .and_then(move |response| {
            info!(target: LOG_TARGET, "UpdateContact Response received: {:?}", response);

            let inbound = response.into_inner();
            assert_eq!(inbound.success, true);

            Ok(())
        })
        .map_err(|e| {
            panic!("RPC Client error = {:?}", e);
        });

    tokio::run(update_contact);
    thread::sleep(Duration::from_millis(100));

    // check contacts
    let (tx, rx) = bounded(1);
    let uri: http::Uri = format!("http://127.0.0.1:{}", WALLET_GRPC_PORT).parse().unwrap();
    let dst = Destination::try_from_uri(uri.clone()).unwrap();
    let connector = util::Connector::new(HttpConnector::new(1));
    let settings = client::Builder::new().http2_only(true).clone();
    let mut make_client = client::Connect::with_builder(connector, settings);

    let get_contacts = make_client
        .make_service(dst.clone())
        .map_err(|e| panic!("connect error: {:?}", e))
        .and_then(move |conn| {
            let conn = tower_request_modifier::Builder::new()
                .set_origin(uri.clone())
                .build(conn)
                .unwrap();

            // Wait until the client is ready...
            WalletRpc::new(conn).ready()
        })
        .and_then(|mut client| client.get_contacts(Request::new(VoidParams {})))
        .and_then(move |response| {
            info!(target: LOG_TARGET, "RemoveContact Response received: {:?}", response);

            let _ = tx.send(response.into_inner());

            Ok(())
        })
        .map_err(|e| {
            panic!("RPC Client error = {:?}", e);
        });

    tokio::run(get_contacts);

    let recv_contacts = rx.recv().unwrap();
    assert_eq!(recv_contacts.contacts.len(), 3);
    assert_eq!(recv_contacts.contacts[1], ContactRpc {
        screen_name: "Updated".to_string(),
        pub_key: contacts[0].pub_key.clone(),
        address: contacts[0].address.clone(),
    });
    assert_eq!(recv_contacts.contacts[2], contacts[2]);
}

fn create_peer(public_key: CommsPublicKey, net_address: NetAddress) -> Peer {
    Peer::new(
        public_key.clone(),
        NodeId::from_key(&public_key).unwrap(),
        NetAddressesWithStats::new(vec![NetAddressWithStats::new(net_address.clone())]),
        PeerFlags::empty(),
    )
}

pub fn random_string(len: usize) -> String {
    let mut rng = OsRng::new().unwrap();
    iter::repeat(()).map(|_| rng.sample(Alphanumeric)).take(len).collect()
}

#[test]
fn test_rpc_text_message_service() {
    init();
    let mut rng = rand::OsRng::new().unwrap();
    let listener_address1: NetAddress = "127.0.0.1:32775".parse().unwrap();
    let secret_key1 = CommsSecretKey::random(&mut rng);
    let public_key1 = CommsPublicKey::from_secret_key(&secret_key1);

    let listener_address2: NetAddress = "127.0.0.1:32776".parse().unwrap();
    let secret_key2 = CommsSecretKey::random(&mut rng);
    let public_key2 = CommsPublicKey::from_secret_key(&secret_key2);

    let config1 = WalletConfig {
        comms: CommsConfig {
            control_service: ControlServiceConfig {
                listener_address: listener_address1.clone(),
                socks_proxy_address: None,
                accept_message_type: TariMessageType::new(NetMessage::Accept),
                requested_outbound_connection_timeout: Duration::from_millis(5000),
            },
            socks_proxy_address: None,
            host: "0.0.0.0".parse().unwrap(),
            public_key: public_key1.clone(),
            secret_key: secret_key1,
            datastore_path: TempDir::new(random_string(8).as_str())
                .unwrap()
                .path()
                .to_str()
                .unwrap()
                .to_string(),
            peer_database_name: random_string(8),
        },
        public_key: public_key1.clone(),
    };

    let config2 = WalletConfig {
        comms: CommsConfig {
            control_service: ControlServiceConfig {
                listener_address: listener_address2.clone(),
                socks_proxy_address: None,
                accept_message_type: TariMessageType::new(NetMessage::Accept),
                requested_outbound_connection_timeout: Duration::from_millis(5000),
            },
            socks_proxy_address: None,
            host: "0.0.0.0".parse().unwrap(),
            public_key: public_key2.clone(),
            secret_key: secret_key2,
            datastore_path: TempDir::new(random_string(8).as_str())
                .unwrap()
                .path()
                .to_str()
                .unwrap()
                .to_string(),
            peer_database_name: random_string(8),
        },
        public_key: public_key2.clone(),
    };

    let wallet1 = Wallet::new(config1).unwrap();

    thread::spawn(move || {
        let wallet_server = WalletServer::new(WALLET_GRPC_PORT, Arc::new(wallet1));
        let _ = wallet_server.start().unwrap();
    });

    let screen_name = "Alice".to_string();
    let alice_contact = ContactRpc {
        screen_name: screen_name.clone(),
        pub_key: public_key2.to_base64().unwrap(),
        address: format!("{}", listener_address2.clone()),
    };

    add_contact(alice_contact.clone());

    thread::sleep(Duration::from_millis(100));

    let wallet2 = Wallet::new(config2).unwrap();

    wallet2
        .comms_services
        .peer_manager()
        .add_peer(create_peer(public_key1.clone(), listener_address1))
        .unwrap();

    let test_msg = TextMessageToSendRpc {
        dest_pub_key: public_key2.clone().to_base64().unwrap(),
        message: "Hey!".to_string(),
    };

    let test_msg2 = TextMessageToSendRpc {
        dest_pub_key: public_key2.clone().to_base64().unwrap(),
        message: "Hoh!".to_string(),
    };

    let resp = RpcResponse {
        success: true,
        message: "Text Message Sent".to_string(),
    };

    send_text_message_request(test_msg, resp.clone());
    send_text_message_request(test_msg2, resp);
    wallet2
        .text_message_service
        .send_text_message(public_key1.clone(), "Here we go!".to_string())
        .unwrap();

    let sent_messages = vec!["Hey!".to_string(), "Hoh!".to_string()];
    let received_messages = vec!["Here we go!".to_string()];

    get_text_messages_request(sent_messages.clone(), received_messages.clone(), None);
    get_text_messages_request(sent_messages, received_messages, Some(alice_contact));
    set_get_screen_name("Alice".to_string());
    get_pub_key(public_key1.to_base64().unwrap());
    contacts_crud();
}
