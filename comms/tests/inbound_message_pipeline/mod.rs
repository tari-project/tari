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

use crate::support::factories::{self, TestFactory};
use futures::{channel::mpsc::channel, executor::block_on, SinkExt, StreamExt};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{sync::Arc, thread, time::Duration};

use crate::support::{
    factories::node_identity::create,
    helpers::database::{clean_up_datastore, init_datastore},
};
use crossbeam_channel;
use tari_comms::{
    connection::NetAddress,
    inbound_message_pipeline::inbound_message_pipeline::InboundMessagePipeline,
    message::{
        FrameSet,
        InboundMessage,
        Message,
        MessageData,
        MessageEnvelope,
        MessageFlags,
        MessageHeader,
        NodeDestination,
    },
    outbound_message_service::OutboundMessageService,
    peer_manager::{NodeId, NodeIdentity, Peer, PeerFlags},
    types::CommsPublicKey,
};
use tari_storage::LMDBWrapper;
use tari_utilities::message_format::MessageFormat;

/// A utility function that will construct a Comms layer message that would typically arrive from a PeerConnection as a
/// FrameSet that deserializes into a MessageEnvelope ## Returns:
/// - (`Message`, `FrameSet`): (Frameset representing the MessageEnvelope body, Serialized MessageEnvelope)
fn construct_message<MType: Serialize + DeserializeOwned>(
    message_type: MType,
    message_body: Vec<u8>,
    source_node_id: NodeIdentity,
    dest_node_id: NodeIdentity,
    destination: NodeDestination<CommsPublicKey>,
    encrypted: bool,
) -> (Message, FrameSet)
{
    // Construct test message
    let message_header = MessageHeader::new(message_type).unwrap();
    let message_body = message_body;
    let message_envelope_body = Message::from_message_format(message_header, message_body).unwrap();
    let dest_public_key = dest_node_id.identity.public_key.clone();
    let message_envelope = MessageEnvelope::construct(
        &source_node_id,
        dest_public_key.clone(),
        destination,
        message_envelope_body.to_binary().unwrap(),
        if encrypted {
            MessageFlags::ENCRYPTED
        } else {
            MessageFlags::NONE
        },
    )
    .unwrap();
    let message_data = MessageData::new(
        NodeId::from_key(&source_node_id.identity.public_key).unwrap(),
        true,
        message_envelope,
    );
    let mut message_frame_set = Vec::new();
    message_frame_set.extend(message_data.clone().into_frame_set());
    (message_envelope_body, message_frame_set)
}

#[test]
fn pipeline_test_handle_route() {
    #[derive(Debug, Hash, Eq, PartialEq, Serialize, Deserialize)]
    pub enum DomainMessageType {
        Type1,
        Type2,
    }

    let node_identity: NodeIdentity = create().build().unwrap();

    let peer = Peer::new(
        node_identity.identity.public_key.clone(),
        node_identity.identity.node_id.clone(),
        "127.0.0.1:9000".parse::<NetAddress>().unwrap().into(),
        PeerFlags::empty(),
    );

    let database_name = "pipeline_test1"; // Note: every test should have unique database
    let datastore = init_datastore(database_name).unwrap();
    let peer_database = datastore.get_handle(database_name).unwrap();
    let peer_database = LMDBWrapper::new(Arc::new(peer_database));
    let peer_manager = Arc::new(
        factories::peer_manager::create()
            .with_database(peer_database)
            .with_peers(vec![peer.clone()])
            .build()
            .unwrap(),
    );
    let (outbound_message_sender, _) = crossbeam_channel::unbounded();
    let outbound_message_service = Arc::new(
        OutboundMessageService::new(
            Arc::new(node_identity.clone()),
            outbound_message_sender,
            peer_manager.clone(),
        )
        .unwrap(),
    );

    let (mut inbound_message_sink_tx, inbound_message_sink_rx) = channel(100);

    // Give worker sufficient time to spinup thread and create a socket
    std::thread::sleep(Duration::from_millis(100));

    // Send some messages NodeDestination::Unknown and unencrypted
    let mut type1_message_bodies = Vec::new();
    for i in 0..2 {
        let (msg_body, msg) = construct_message(
            DomainMessageType::Type1,
            format!("Message Body {}", i).as_bytes().to_vec(),
            node_identity.clone(),
            node_identity.clone(),
            NodeDestination::Unknown,
            false,
        );
        type1_message_bodies.push(msg_body);
        block_on(async {
            inbound_message_sink_tx.send(msg.clone()).await.unwrap();
        });
        // Send it twice to check the duplicate rejection is working
        block_on(async {
            inbound_message_sink_tx.send(msg).await.unwrap();
        });
    }

    // NodeDestination::Unknown and encrypted
    let (msg_body, msg) = construct_message(
        DomainMessageType::Type1,
        "Message Body Unknown and Encrypted".as_bytes().to_vec(),
        node_identity.clone(),
        node_identity.clone(),
        NodeDestination::Unknown,
        true,
    );
    type1_message_bodies.push(msg_body);
    block_on(async {
        inbound_message_sink_tx.send(msg.clone()).await.unwrap();
    });

    // NodeDestination::PublicKey with the correct pub_key
    let (msg_body, msg) = construct_message(
        DomainMessageType::Type1,
        "Message Body To Pubkey and Encrypted".as_bytes().to_vec(),
        node_identity.clone(),
        node_identity.clone(),
        NodeDestination::PublicKey(node_identity.identity.public_key.clone()),
        true,
    );
    type1_message_bodies.push(msg_body);
    block_on(async {
        inbound_message_sink_tx.send(msg.clone()).await.unwrap();
    });

    // NodeDestination::PublicKey with incorrect pubkey, should not be delivered
    let wrong_node_identity: NodeIdentity = create().build().unwrap();
    let (_msg_body, msg) = construct_message(
        DomainMessageType::Type1,
        "Message Body TO Wrong Pubkey and Encrypted".as_bytes().to_vec(),
        node_identity.clone(),
        node_identity.clone(),
        NodeDestination::PublicKey(wrong_node_identity.identity.public_key.clone()),
        true,
    );

    block_on(async {
        inbound_message_sink_tx.send(msg.clone()).await.unwrap();
    });

    // Send some messages NodeDestination::Unknown and unencrypted
    let mut type2_message_bodies = Vec::new();
    for i in 2..4 {
        let (msg_body, msg) = construct_message(
            DomainMessageType::Type2,
            format!("Message Body {}", i).as_bytes().to_vec(),
            node_identity.clone(),
            node_identity.clone(),
            NodeDestination::Unknown,
            false,
        );

        type2_message_bodies.push(msg_body);
        block_on(async {
            inbound_message_sink_tx.send(msg.clone()).await.unwrap();
        });
        // Send it twice to check the duplicate rejection is working
        block_on(async {
            inbound_message_sink_tx.send(msg.clone()).await.unwrap();
        });
    }

    // Send some messages NodeDestination::NodeId in Network Region and Encrypted
    let (msg_body, msg) = construct_message(
        DomainMessageType::Type2,
        "To Node ID and Encrypted".as_bytes().to_vec(),
        node_identity.clone(),
        node_identity.clone(),
        NodeDestination::NodeId(NodeId::from_key(&node_identity.identity.public_key).unwrap()),
        true,
    );
    type2_message_bodies.push(msg_body);
    block_on(async {
        inbound_message_sink_tx.send(msg.clone()).await.unwrap();
    });

    thread::sleep(Duration::from_millis(500));

    // This means that the pipeline run() future will read the message_sink stream until it is empty and because the
    // stream has completed the run function will return. If you don't do this the run will block waiting for the next
    // message to arrive on the stream
    drop(inbound_message_sink_tx);

    // Construct Pipeline

    let (pipeline, subscription_factories): (InboundMessagePipeline<DomainMessageType>, _) =
        InboundMessagePipeline::new(
            Arc::new(node_identity),
            inbound_message_sink_rx,
            peer_manager,
            outbound_message_service,
            100,
        );

    let mut message_subscription_type1 = subscription_factories
        .handle_message_subscription_factory
        .get_subscription(DomainMessageType::Type1)
        .fuse();
    let mut message_subscription_type2 = subscription_factories
        .handle_message_subscription_factory
        .get_subscription(DomainMessageType::Type2)
        .fuse();

    block_on(pipeline.run());
    let msgs_type1: Vec<InboundMessage> = block_on(async {
        let mut result = Vec::new();

        loop {
            futures::select!(
                item = message_subscription_type1.next() => {if let Some(i) = item {result.push(i)}},
                complete => break,
                default => break,
            );
        }
        result
    });

    let msgs_type2: Vec<InboundMessage> = block_on(async {
        let mut result = Vec::new();

        loop {
            futures::select!(
                item = message_subscription_type2.next() => {if let Some(i) = item {result.push(i)}},
                complete => break,
                default => break,
            );
        }
        result
    });

    assert_eq!(msgs_type1.len(), type1_message_bodies.len());
    for i in 0..type1_message_bodies.len() {
        assert_eq!(msgs_type1[i].message, type1_message_bodies[i]);
    }
    assert_eq!(msgs_type2.len(), type2_message_bodies.len());
    for i in 0..type2_message_bodies.len() {
        assert_eq!(msgs_type2[i].message, type2_message_bodies[i]);
    }

    clean_up_datastore(database_name);
}

// TODO Test the Forward route once the DHT and SAF services have been ported to connect to the Forward route pub-sub
