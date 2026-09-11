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

use std::{sync::Arc, time::Duration};

use tari_comms::{
    CommsNode,
    peer_manager::{NodeIdentity, PeerFeatures},
    transports::MemoryTransport,
};
use tari_comms_dht::Dht;
use tari_p2p::{
    comms_connector::pubsub_connector,
    services::liveness::{LivenessEvent, LivenessHandle, LivenessInitializer},
};
use tari_service_framework::{RegisterHandle, StackBuilder};
use tari_shutdown::Shutdown;
use tari_test_utils::collect_try_recv;
use tempfile::tempdir;
use tokio::time;

use crate::support::comms_and_services::setup_comms_services;

pub async fn setup_liveness_service(
    node_identity: Arc<NodeIdentity>,
    peers: Vec<Arc<NodeIdentity>>,
    data_path: &str,
) -> (LivenessHandle, CommsNode, Dht, Shutdown) {
    let (publisher, subscription_factory) = pubsub_connector(100);
    let subscription_factory = Arc::new(subscription_factory);
    let shutdown = Shutdown::new();
    let (comms, dht, _) =
        setup_comms_services(node_identity.clone(), peers, publisher, data_path, shutdown.to_signal()).await;

    let handles = StackBuilder::new(comms.shutdown_signal())
        .add_initializer(RegisterHandle::new(dht.clone()))
        .add_initializer(RegisterHandle::new(comms.connectivity()))
        .add_initializer(RegisterHandle::new(comms.peer_manager()))
        .add_initializer(LivenessInitializer::new(
            Default::default(),
            Arc::clone(&subscription_factory),
        ))
        .build()
        .await
        .expect("Service initialization failed");

    let liveness_handle = handles.get_handle::<LivenessHandle>().unwrap();

    (liveness_handle, comms, dht, shutdown)
}

/// Waits until `comms` is connected to at least one peer and the connection churn around start-up has settled.
///
/// A node will re-dial a peer while its own dial is still in flight, because the in-progress dial is not yet in the
/// connection pool. The extra connection that lands is resolved by a tie break which silently disconnects the loser,
/// including any message already written to it. Liveness counts every ping, so the test may only start pinging once
/// that has stopped happening.
async fn wait_until_connections_settle(comms: &CommsNode) {
    let mut connectivity = comms.connectivity();
    let mut events = connectivity.get_event_subscription();
    connectivity
        .wait_for_connectivity(Duration::from_secs(30))
        .await
        .expect("Node did not come online");
    // Quiet period: a tie break is reported as a connectivity event, so no events for this long means the peer
    // connection that messaging will use is the one that survived.
    while time::timeout(Duration::from_millis(500), events.recv()).await.is_ok() {}
}

fn make_node_identity() -> Arc<NodeIdentity> {
    let next_port = MemoryTransport::acquire_next_memsocket_port();
    Arc::new(NodeIdentity::random(
        &mut rand::rng(),
        format!("/memory/{next_port}").parse().unwrap(),
        PeerFeatures::COMMUNICATION_NODE,
    ))
}

#[tokio::test]
#[allow(clippy::similar_names)]
async fn end_to_end() {
    let node_1_identity = make_node_identity();
    let node_2_identity = make_node_identity();

    let alice_temp_dir = tempdir().unwrap();
    let (mut liveness1, comms_1, _dht_1, _shutdown) = setup_liveness_service(
        node_1_identity.clone(),
        vec![node_2_identity.clone()],
        alice_temp_dir.path().to_str().unwrap(),
    )
    .await;
    let bob_temp_dir = tempdir().unwrap();
    // Only node 1 is seeded with its counterpart, so only node 1 dials. If both nodes dial each other they end up
    // with two connections and the tie break silently disconnects the loser, taking any message already written to
    // it with it - and this test counts every single ping. Node 2 learns about node 1 from the inbound connection,
    // which is all it needs to ping back.
    let (mut liveness2, comms_2, _dht_2, _shutdown) =
        setup_liveness_service(node_2_identity.clone(), vec![], bob_temp_dir.path().to_str().unwrap()).await;

    wait_until_connections_settle(&comms_1).await;
    wait_until_connections_settle(&comms_2).await;

    let mut liveness1_event_stream = liveness1.get_event_stream();
    let mut liveness2_event_stream = liveness2.get_event_stream();

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

    let events = collect_try_recv!(liveness1_event_stream, take = 18, timeout = Duration::from_secs(20));

    let ping_count = events
        .iter()
        .filter(|event| matches!(&***event, LivenessEvent::ReceivedPing(_)))
        .count();

    assert_eq!(ping_count, 10);

    let pong_count = events
        .iter()
        .filter(|event| matches!(&***event, LivenessEvent::ReceivedPong(_)))
        .count();

    assert_eq!(pong_count, 8);

    let events = collect_try_recv!(liveness2_event_stream, take = 18, timeout = Duration::from_secs(10));

    let ping_count = events
        .iter()
        .filter(|event| matches!(&***event, LivenessEvent::ReceivedPing(_)))
        .count();

    assert_eq!(ping_count, 8);

    let pong_count = events
        .iter()
        .filter(|event| matches!(&***event, LivenessEvent::ReceivedPong(_)))
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
}
