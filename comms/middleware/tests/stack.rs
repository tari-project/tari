// Copyright 2019, The Tari Project
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

use futures::{channel::mpsc, StreamExt};
use rand::rngs::OsRng;
use std::sync::Arc;
use tari_comms::{
    connection::NetAddress,
    message::{Message, MessageEnvelopeHeader, MessageFlags, MessageHeader, NodeDestination},
    outbound_message_service::{OutboundRequest, OutboundServiceRequester},
    peer_manager::{peer_storage::PeerStorage, NodeIdentity, Peer, PeerFlags, PeerManager},
    types::CommsDatabase,
};
use tari_comms_middleware::{
    encryption::{encrypt, generate_ecdh_secret, DecryptionLayer},
    forward::ForwardLayer,
    inbound_message::InboundMessage,
    pubsub::pubsub_service,
};
use tari_storage::{lmdb_store::LMDBBuilder, LMDBWrapper};
use tari_test_utils::random;
use tari_utilities::message_format::MessageFormat;
use tempdir::TempDir;
use tokio::runtime::Runtime;
use tower::{Service, ServiceBuilder};

pub fn make_node_identity() -> NodeIdentity {
    NodeIdentity::random(&mut OsRng::new().unwrap(), "127.0.0.1:9000".parse().unwrap()).unwrap()
}

pub fn make_inbound_message(node_identity: &NodeIdentity, message: Vec<u8>, flags: MessageFlags) -> InboundMessage {
    InboundMessage::new(
        Peer::new(
            node_identity.identity.public_key.clone(),
            node_identity.identity.node_id.clone(),
            Vec::<NetAddress>::new().into(),
            PeerFlags::empty(),
        ),
        MessageEnvelopeHeader {
            version: 0,
            origin_source: node_identity.identity.public_key.clone(),
            peer_source: node_identity.identity.public_key.clone(),
            destination: NodeDestination::Unknown,
            origin_signature: Vec::new(),
            peer_signature: Vec::new(),
            flags,
        },
        0,
        message,
    )
}
fn create_peer_storage(tmpdir: &TempDir, database_name: &str, peers: Vec<Peer>) -> CommsDatabase {
    let datastore = LMDBBuilder::new()
        .set_path(tmpdir.path().to_str().unwrap())
        .set_environment_size(10)
        .set_max_number_of_databases(1)
        .add_database(database_name, lmdb_zero::db::CREATE)
        .build()
        .unwrap();

    let peer_database = datastore.get_handle(database_name).unwrap();
    let peer_database = LMDBWrapper::new(Arc::new(peer_database));
    let mut storage = PeerStorage::new(peer_database).unwrap();
    for peer in peers {
        storage.add_peer(peer).unwrap();
    }

    storage.into_datastore()
}

#[test]
fn stack_unencrypted() {
    let node_identity = Arc::new(make_node_identity());
    let rt = Runtime::new().unwrap();
    let (service, subscription_factory) = pubsub_service(rt.executor(), 1);

    let tmpdir = TempDir::new(random::string(8).as_str()).unwrap();
    let database_name = "middleware_stack";
    let peer_manager = PeerManager::new(create_peer_storage(&tmpdir, database_name, vec![]))
        .map(Arc::new)
        .unwrap();
    let (oms_sender, _) = mpsc::unbounded();
    let oms = OutboundServiceRequester::new(oms_sender);

    let mut stack = ServiceBuilder::new()
        .layer(DecryptionLayer::new(Arc::clone(&node_identity)))
        .layer(ForwardLayer::new(
            Arc::clone(&peer_manager),
            Arc::clone(&node_identity),
            oms.clone(),
        ))
        .service(service);

    let header = MessageHeader::new("fake_type".to_string()).unwrap();
    let msg = Message::from_message_format(header, "secret".to_string()).unwrap();
    let inbound_message = make_inbound_message(&node_identity, msg.to_binary().unwrap(), MessageFlags::empty());

    let mut subscriber = subscription_factory.get_subscription("fake_type".to_string());

    let msg = rt.block_on(async move {
        stack.call(inbound_message).await.unwrap();
        let msg = subscriber.next().await.unwrap();
        msg.message.deserialize_message::<String>().unwrap()
    });

    assert_eq!(msg, "secret");
}

#[test]
fn stack_encrypted() {
    let rt = Runtime::new().unwrap();
    let (pubsub, subscription_factory) = pubsub_service(rt.executor(), 1);

    let node_identity = Arc::new(make_node_identity());
    let tmpdir = TempDir::new(random::string(8).as_str()).unwrap();
    let database_name = "middleware_stack";
    let peer_manager = PeerManager::new(create_peer_storage(&tmpdir, database_name, vec![]))
        .map(Arc::new)
        .unwrap();
    let (oms_sender, _) = mpsc::unbounded();
    let oms = OutboundServiceRequester::new(oms_sender);

    let mut stack = ServiceBuilder::new()
        .layer(DecryptionLayer::new(Arc::clone(&node_identity)))
        .layer(ForwardLayer::new(
            Arc::clone(&peer_manager),
            Arc::clone(&node_identity),
            oms.clone(),
        ))
        .service(pubsub);

    let header = MessageHeader::new("fake_type".to_string()).unwrap();
    let msg = Message::from_message_format(header, "secret".to_string()).unwrap();
    // Encrypt for self
    let ecdh_key = generate_ecdh_secret(&node_identity.secret_key, &node_identity.identity.public_key);
    let encrypted_bytes = encrypt(&ecdh_key, &msg.to_binary().unwrap()).unwrap();
    let inbound_message = make_inbound_message(&node_identity, encrypted_bytes, MessageFlags::ENCRYPTED);

    let mut subscriber = subscription_factory.get_subscription("fake_type".to_string());

    let msg = rt.block_on(async move {
        stack.call(inbound_message).await.unwrap();
        let msg = subscriber.next().await.unwrap();
        msg.message.deserialize_message::<String>().unwrap()
    });

    assert_eq!(msg, "secret");
}

#[test]
fn stack_forward() {
    let rt = Runtime::new().unwrap();
    let (pubsub, _) = pubsub_service::<()>(rt.executor(), 1);

    let node_identity = Arc::new(make_node_identity());
    let tmpdir = TempDir::new(random::string(8).as_str()).unwrap();
    let database_name = "middleware_stack";
    let peer_manager = PeerManager::new(create_peer_storage(&tmpdir, database_name, vec![]))
        .map(Arc::new)
        .unwrap();
    let (oms_sender, mut oms_receiver) = mpsc::unbounded();
    let oms = OutboundServiceRequester::new(oms_sender);

    let mut stack = ServiceBuilder::new()
        .layer(DecryptionLayer::new(Arc::clone(&node_identity)))
        .layer(ForwardLayer::new(
            Arc::clone(&peer_manager),
            Arc::clone(&node_identity),
            oms.clone(),
        ))
        .service(pubsub);

    let msg = "garbage".as_bytes().to_vec();
    // Encrypt for self
    let ecdh_key = generate_ecdh_secret(&node_identity.secret_key, &node_identity.identity.public_key);
    let encrypted_bytes = encrypt(&ecdh_key, &msg).unwrap();
    let inbound_message = make_inbound_message(&node_identity, encrypted_bytes, MessageFlags::ENCRYPTED);

    let msg = rt.block_on(async move {
        stack.call(inbound_message).await.unwrap();
        oms_receiver.next().await.unwrap()
    });

    match msg {
        OutboundRequest::Forward { .. } => {},
        _ => panic!("unexpected message"),
    }
}
