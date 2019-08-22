// Copyright 2019 The Tari Project
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

use crate::support::TestCommsOutboundInitializer;
use futures::Sink;
use rand::rngs::OsRng;
use std::{
    sync::{mpsc, Arc},
    time::Duration,
};
use tari_comms::{
    message::{DomainMessageContext, Message, MessageHeader},
    peer_manager::{NodeId, PeerNodeIdentity},
    pub_sub_channel::{pubsub_channel, TopicPayload},
    types::CommsPublicKey,
};
use tari_crypto::keys::PublicKey;
use tari_p2p::{
    executor::StackBuilder,
    services::{
        comms_outbound::CommsOutboundRequest,
        liveness::{LivenessHandle, LivenessInitializer, LivenessRequest, LivenessResponse, PingPong},
        ServiceName,
    },
    tari_message::{NetMessage, TariMessageType},
};
use tari_utilities::message_format::MessageFormat;
use tokio::runtime::Runtime;
use tower_service::Service;

fn create_domain_message<T: MessageFormat>(message_type: TariMessageType, inner_msg: T) -> DomainMessageContext {
    let mut rng = OsRng::new().unwrap();
    let (_, pk) = CommsPublicKey::random_keypair(&mut rng);
    let peer_source = PeerNodeIdentity::new(NodeId::from_key(&pk).unwrap(), pk.clone());
    let header = MessageHeader::new(message_type).unwrap();
    let msg = Message::from_message_format(header, inner_msg).unwrap();
    DomainMessageContext::new(peer_source, pk, msg)
}

/// Receive a Ping message and query the PingCount
#[test]
fn send_ping_query_count() {
    let mut rt = Runtime::new().unwrap();
    let (mut publisher, subscriber) = pubsub_channel(2);
    let (tx, rx) = mpsc::channel();

    // Setup the stack
    let stack = StackBuilder::new()
        .add_initializer(LivenessInitializer::from_inbound_message_subscriber(Arc::new(
            subscriber,
        )))
        .add_initializer(TestCommsOutboundInitializer::new(tx));
    let handles = rt.block_on(stack.finish()).unwrap();

    // Publish a Ping message
    let msg = create_domain_message(TariMessageType::new(NetMessage::PingPong), PingPong::Ping);
    let payload = TopicPayload::new(TariMessageType::new(NetMessage::PingPong), msg);
    assert!(publisher.start_send(payload).unwrap().is_ready());

    // Check that the CommsOutbound service received a SendMsg request
    let outbound_req = rx.recv_timeout(Duration::from_millis(100)).unwrap();
    match outbound_req {
        CommsOutboundRequest::SendMsg { .. } => {},
        _ => panic!("Unexpected request sent to comms outbound service"),
    }

    // Query the ping count using the Liveness service handle
    let mut liveness_handle = handles.get_handle::<LivenessHandle>(ServiceName::Liveness).unwrap();
    let resp = rt
        .block_on(liveness_handle.call(LivenessRequest::GetPingCount))
        .unwrap();

    match resp.unwrap() {
        LivenessResponse::Count(n) => assert_eq!(n, 1),
        _ => panic!("unexpected response from liveness service"),
    }
}
