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

use crate::support::{
    factories::{self, TestFactory},
    helpers::database::init_datastore,
};
use futures::{channel::mpsc, SinkExt, StreamExt};
use std::{sync::Arc, time::Duration};
use tari_comms::{
    connection::NetAddress,
    inbound_message_service::inbound_message_service::InboundMessageService,
    message::{Envelope, FrameSet, MessageExt, MessageFlags},
    peer_manager::{NodeIdentity, Peer, PeerFeatures, PeerFlags},
};
use tari_shutdown::Shutdown;
use tari_storage::LMDBWrapper;
use tari_utilities::ByteArray;
use tokio::{future::FutureExt, runtime::Runtime};

/// A utility function that will construct a Comms layer message that would typically arrive from a PeerConnection as a
/// FrameSet that deserializes into a MessageEnvelope
/// ## Returns:
/// - `FrameSet`: Two frames, the node id of the source peer and the encoded envelope
fn construct_message(message_body: Vec<u8>, node_identity: Arc<NodeIdentity>) -> FrameSet {
    // Construct test message
    let envelope = Envelope::construct_signed(
        node_identity.secret_key(),
        node_identity.public_key(),
        message_body,
        MessageFlags::NONE,
    )
    .unwrap();
    let mut frames = Vec::new();
    frames.push(node_identity.node_id().to_vec());
    frames.push(envelope.to_encoded_bytes().unwrap());
    frames
}

#[test]
fn smoke_test() {
    let rt = Runtime::new().unwrap();

    let node_identity = factories::node_identity::create().build().map(Arc::new).unwrap();

    let peer = Peer::new(
        node_identity.public_key().clone(),
        node_identity.node_id().clone(),
        "127.0.0.1:9000".parse::<NetAddress>().unwrap().into(),
        PeerFlags::empty(),
        PeerFeatures::empty(),
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
    let peer = peer_manager.find_by_node_id(&peer.node_id).unwrap();

    let (mut inbound_message_sink_tx, inbound_message_sink_rx) = mpsc::channel(100);

    // Send some messages NodeDestination::Unknown and unencrypted
    let mut sent_messages = Vec::new();
    let body = "First message".as_bytes().to_vec();
    let msg_body = construct_message(body.clone(), Arc::clone(&node_identity));
    sent_messages.push(body);
    rt.block_on(async {
        inbound_message_sink_tx.send(msg_body.clone()).await.unwrap();
        // Send it twice to check the duplicate rejection is working
        inbound_message_sink_tx.send(msg_body).await.unwrap();
    });

    // Construct Pipeline
    let (inbound_tx, inbound_rx) = mpsc::channel(100);
    let shutdown = Shutdown::new();
    let inbound_message_service =
        InboundMessageService::new(inbound_message_sink_rx, inbound_tx, peer_manager, shutdown.to_signal());

    rt.spawn(inbound_message_service.run());

    let num_messages = sent_messages.len();
    let messages = rt
        .block_on(
            inbound_rx
                .take(num_messages as u64)
                .collect::<Vec<_>>()
                .timeout(Duration::from_secs(3)),
        )
        .unwrap();

    assert_eq!(messages.len(), sent_messages.len());
    for i in 0..sent_messages.len() {
        assert_eq!(messages[i].body, sent_messages[i]);
        assert_eq!(messages[i].source_peer, peer);
        assert_eq!(messages[i].envelope_header.public_key, peer.public_key);
    }
}
