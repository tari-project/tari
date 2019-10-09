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

use crate::support::{comms_and_services::setup_comms_services, utils::event_stream_count};
use std::{sync::Arc, time::Duration};
use tari_comms::{builder::CommsNode, peer_manager::NodeIdentity};
use tari_comms_dht::Dht;
use tari_p2p::{
    comms_connector::pubsub_connector,
    services::{
        comms_outbound::CommsOutboundServiceInitializer,
        liveness::{
            handle::{LivenessEvent, LivenessHandle},
            LivenessInitializer,
        },
    },
};
use tari_service_framework::StackBuilder;
use tokio::runtime::Runtime;

pub fn setup_liveness_service(
    runtime: &Runtime,
    node_identity: NodeIdentity,
    peers: Vec<NodeIdentity>,
) -> (LivenessHandle, Arc<CommsNode>, Dht)
{
    let (publisher, subscription_factory) = pubsub_connector(runtime.executor(), 100);
    let subscription_factory = Arc::new(subscription_factory);
    let (comms, dht) = setup_comms_services(runtime.executor(), Arc::new(node_identity.clone()), peers, publisher);

    let fut = StackBuilder::new(runtime.executor())
        .add_initializer(CommsOutboundServiceInitializer::new(dht.outbound_requester()))
        .add_initializer(LivenessInitializer::new(Arc::clone(&subscription_factory)))
        .finish();

    let handles = runtime.block_on(fut).expect("Service initialization failed");

    let liveness_handle = handles.get_handle::<LivenessHandle>().unwrap();

    (liveness_handle, comms, dht)
}

#[test]
fn end_to_end() {
    let runtime = Runtime::new().unwrap();

    let mut rng = rand::rngs::OsRng::new().unwrap();

    let node_1_identity = NodeIdentity::random(&mut rng, "127.0.0.1:31593".parse().unwrap()).unwrap();
    let node_2_identity = NodeIdentity::random(&mut rng, "127.0.0.1:31195".parse().unwrap()).unwrap();

    let (mut liveness1, _comms_1, _dht_1) =
        setup_liveness_service(&runtime, node_1_identity.clone(), vec![node_2_identity.clone()]);
    let (mut liveness2, _comms_2, _dht_2) =
        setup_liveness_service(&runtime, node_2_identity.clone(), vec![node_1_identity.clone()]);

    let mut pingpong1_total = (0, 0);
    let mut pingpong2_total = (0, 0);

    for _ in 0..5 {
        let _ = runtime
            .block_on(liveness2.send_ping(node_1_identity.identity.public_key.clone()))
            .unwrap();
        pingpong1_total = (pingpong1_total.0 + 1, pingpong1_total.1);
        pingpong2_total = (pingpong2_total.0, pingpong2_total.1 + 1);
    }

    for _ in 0..4 {
        let _ = runtime
            .block_on(liveness1.send_ping(node_2_identity.identity.public_key.clone()))
            .unwrap();
        pingpong2_total = (pingpong2_total.0 + 1, pingpong2_total.1);
        pingpong1_total = (pingpong1_total.0, pingpong1_total.1 + 1);
    }

    for _ in 0..5 {
        let _ = runtime
            .block_on(liveness2.send_ping(node_1_identity.identity.public_key.clone()))
            .unwrap();
        pingpong1_total = (pingpong1_total.0 + 1, pingpong1_total.1);
        pingpong2_total = (pingpong2_total.0, pingpong2_total.1 + 1);
    }

    for _ in 0..4 {
        let _ = runtime
            .block_on(liveness1.send_ping(node_2_identity.identity.public_key.clone()))
            .unwrap();
        pingpong2_total = (pingpong2_total.0 + 1, pingpong2_total.1);
        pingpong1_total = (pingpong1_total.0, pingpong1_total.1 + 1);
    }

    let mut result = runtime
        .block_on(async { event_stream_count(liveness1.get_event_stream_fused(), 18, Duration::from_secs(10)).await });
    assert_eq!(result.remove(&LivenessEvent::ReceivedPong), Some(8));
    assert_eq!(result.remove(&LivenessEvent::ReceivedPing), Some(10));

    let mut result = runtime
        .block_on(async { event_stream_count(liveness2.get_event_stream_fused(), 18, Duration::from_secs(10)).await });
    assert_eq!(result.remove(&LivenessEvent::ReceivedPong), Some(10));
    assert_eq!(result.remove(&LivenessEvent::ReceivedPing), Some(8));

    let pingcount1 = runtime.block_on(liveness1.get_ping_count()).unwrap();
    let pongcount1 = runtime.block_on(liveness1.get_pong_count()).unwrap();
    let pingcount2 = runtime.block_on(liveness2.get_ping_count()).unwrap();
    let pongcount2 = runtime.block_on(liveness2.get_pong_count()).unwrap();

    assert_eq!(pingcount1, 10);
    assert_eq!(pongcount1, 8);
    assert_eq!(pingcount2, 8);
    assert_eq!(pongcount2, 10);
}
