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

use crate::support::comms_and_services::setup_comms_services;
use rand::rngs::OsRng;
use std::{sync::Arc, time::Duration};
use tari_comms::{
    peer_manager::{NodeIdentity, PeerFeatures},
    transports::MemoryTransport,
    CommsNode,
};
use tari_comms_dht::Dht;
use tari_p2p::{
    comms_connector::pubsub_connector,
    services::{
        comms_outbound::CommsOutboundServiceInitializer,
        liveness::{LivenessEvent, LivenessHandle, LivenessInitializer},
    },
};
use tari_service_framework::StackBuilder;
use tari_test_utils::{collect_stream, random::string};
use tempdir::TempDir;
use tokio::runtime;

pub async fn setup_liveness_service(
    node_identity: Arc<NodeIdentity>,
    peers: Vec<Arc<NodeIdentity>>,
    data_path: &str,
) -> (LivenessHandle, CommsNode, Dht)
{
    let rt_handle = runtime::Handle::current();
    let (publisher, subscription_factory) = pubsub_connector(rt_handle.clone(), 100);
    let subscription_factory = Arc::new(subscription_factory);
    let (comms, dht) = setup_comms_services(node_identity.clone(), peers, publisher, data_path).await;

    let handles = StackBuilder::new(rt_handle.clone(), comms.shutdown_signal())
        .add_initializer(CommsOutboundServiceInitializer::new(dht.outbound_requester()))
        .add_initializer(LivenessInitializer::new(
            Default::default(),
            Arc::clone(&subscription_factory),
            dht.dht_requester(),
        ))
        .finish()
        .await
        .expect("Service initialization failed");

    let liveness_handle = handles.get_handle::<LivenessHandle>().unwrap();

    (liveness_handle, comms, dht)
}

fn make_node_identity() -> Arc<NodeIdentity> {
    let next_port = MemoryTransport::acquire_next_memsocket_port();
    Arc::new(
        NodeIdentity::random(
            &mut OsRng,
            format!("/memory/{}", next_port).parse().unwrap(),
            PeerFeatures::COMMUNICATION_NODE,
        )
        .unwrap(),
    )
}

#[tokio_macros::test_basic]
async fn end_to_end() {
    let node_1_identity = make_node_identity();
    let node_2_identity = make_node_identity();

    let alice_temp_dir = TempDir::new(string(8).as_str()).unwrap();
    let (mut liveness1, comms_1, _dht_1) = setup_liveness_service(
        node_1_identity.clone(),
        vec![node_2_identity.clone()],
        alice_temp_dir.path().to_str().unwrap(),
    )
    .await;
    let bob_temp_dir = TempDir::new(string(8).as_str()).unwrap();
    let (mut liveness2, comms_2, _dht_2) = setup_liveness_service(
        node_2_identity.clone(),
        vec![node_1_identity.clone()],
        bob_temp_dir.path().to_str().unwrap(),
    )
    .await;

    let mut liveness1_event_stream = liveness1.get_event_stream_fused();
    let mut liveness2_event_stream = liveness2.get_event_stream_fused();

    for _ in 0..5 {
        liveness2.send_ping(node_1_identity.node_id().clone()).await.unwrap();
    }

    for _ in 0..4 {
        liveness1.send_ping(node_2_identity.node_id().clone()).await.unwrap();
    }

    for _ in 0..5 {
        liveness2.send_ping(node_1_identity.node_id().clone()).await.unwrap();
    }

    for _ in 0..4 {
        liveness1.send_ping(node_2_identity.node_id().clone()).await.unwrap();
    }

    let events = collect_stream!(liveness1_event_stream, take = 18, timeout = Duration::from_secs(20),);

    let ping_count = events
        .iter()
        .filter(|event| match **(**event).as_ref().unwrap() {
            LivenessEvent::ReceivedPing(_) => true,
            _ => false,
        })
        .count();

    assert_eq!(ping_count, 10);

    let pong_count = events
        .iter()
        .filter(|event| match **(**event).as_ref().unwrap() {
            LivenessEvent::ReceivedPong(_) => true,
            _ => false,
        })
        .count();

    assert_eq!(pong_count, 8);

    let events = collect_stream!(liveness2_event_stream, take = 18, timeout = Duration::from_secs(10),);

    let ping_count = events
        .iter()
        .filter(|event| match **(**event).as_ref().unwrap() {
            LivenessEvent::ReceivedPing(_) => true,
            _ => false,
        })
        .count();

    assert_eq!(ping_count, 8);

    let pong_count = events
        .iter()
        .filter(|event| match **(**event).as_ref().unwrap() {
            LivenessEvent::ReceivedPong(_) => true,
            _ => false,
        })
        .count();

    assert_eq!(pong_count, 10);

    let pingcount1 = liveness1.get_ping_count().await.unwrap();
    let pongcount1 = liveness1.get_pong_count().await.unwrap();
    let pingcount2 = liveness2.get_ping_count().await.unwrap();
    let pongcount2 = liveness2.get_pong_count().await.unwrap();

    assert_eq!(pingcount1, 10);
    assert_eq!(pongcount1, 8);
    assert_eq!(pingcount2, 8);
    assert_eq!(pongcount2, 10);

    comms_1.shutdown().await;
    comms_2.shutdown().await;
}
