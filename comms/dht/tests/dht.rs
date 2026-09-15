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

mod harness;
use std::time::Duration;

use harness::*;
use tari_comms::{
    RefKind,
    connectivity::ConnectivityEvent,
    message::MessageExt,
    peer_manager::{NodeId, PeerFeatures},
    protocol::messaging::MessagingEvent,
};
use tari_comms_dht::{
    domain_message::OutboundDomainMessage,
    envelope::{DhtMessageType, NodeDestination},
    outbound::{OutboundEncryption, SendMessageParams},
};
use tari_test_utils::{async_assert_eventually, collect_try_recv, streams, unpack_enum};
use tokio::{
    sync::{broadcast, mpsc},
    time::{self, Instant},
};

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[allow(non_snake_case)]
async fn test_dht_join_propagation() {
    // Create 3 nodes where only Node B knows A and C, but A and C want to talk to each other

    // Node C knows no one
    let node_C = make_node("node_C", PeerFeatures::COMMUNICATION_NODE, dht_config(), None).await;
    // Node B knows about Node C
    let node_B = make_node(
        "node_B",
        PeerFeatures::COMMUNICATION_NODE,
        dht_config(),
        Some(node_C.to_peer()),
    )
    .await;
    // Node A knows about Node B
    let node_A = make_node(
        "node_A",
        PeerFeatures::COMMUNICATION_NODE,
        dht_config(),
        Some(node_B.to_peer()),
    )
    .await;

    wait_for_connectivity(&[&node_A, &node_B, &node_C]).await;
    // Send a join request from Node A, through B to C. As all Nodes are in the same network region, once
    // Node C receives the join request from Node A, it will send a direct join request back
    // to A.
    node_A.dht.dht_requester().send_join().await.unwrap();

    let node_B_peer_manager = node_B.comms.peer_manager();
    let node_C_peer_manager = node_C.comms.peer_manager();

    // Check that Node B and C know node A
    async_assert_eventually!(
        node_B_peer_manager
            .exists(node_A.node_identity().public_key())
            .await
            .unwrap(),
        expect = true,
        max_attempts = 10,
        interval = Duration::from_millis(1000)
    );
    async_assert_eventually!(
        node_C_peer_manager
            .exists(node_A.node_identity().public_key())
            .await
            .unwrap(),
        expect = true,
        max_attempts = 10,
        interval = Duration::from_millis(500)
    );

    let node_A_peer = node_C_peer_manager
        .find_by_public_key(node_A.node_identity().public_key())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(node_A_peer.features, node_A.comms.node_identity().features());

    node_A.shutdown().await;
    node_B.shutdown().await;
    node_C.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[allow(non_snake_case)]
async fn test_dht_wallet_discover_propagation() {
    // env_logger::init(); // Set `$env:RUST_LOG = "trace"`
    // Create 4 nodes where A knows B, B knows A and C, C knows B and D, and D knows C

    // client_D knows no one
    let client_D = make_node("client_D", PeerFeatures::COMMUNICATION_CLIENT, dht_config(), None).await;
    // Node C knows about Node D
    let node_C = make_node(
        "node_C",
        PeerFeatures::COMMUNICATION_NODE,
        dht_config(),
        Some(client_D.to_peer()),
    )
    .await;
    // Node B knows about Node C
    let node_B = make_node(
        "node_B",
        PeerFeatures::COMMUNICATION_NODE,
        dht_config(),
        Some(node_C.to_peer()),
    )
    .await;
    // Node A knows about Node B
    let node_A = make_node(
        "node_A",
        PeerFeatures::COMMUNICATION_NODE,
        dht_config(),
        Some(node_B.to_peer()),
    )
    .await;
    log::info!(
        "Node A = {}, Node B = {}, Node C = {}, Client D = {}",
        node_A.node_identity().node_id().short_str(),
        node_B.node_identity().node_id().short_str(),
        node_C.node_identity().node_id().short_str(),
        client_D.node_identity().node_id().short_str(),
    );

    // To receive messages, clients have to connect
    client_D
        .comms
        .peer_manager()
        .add_or_update_peer(node_C.to_peer())
        .await
        .unwrap();
    client_D
        .comms
        .connectivity()
        .dial_peer(node_C.comms.node_identity().node_id().clone(), RefKind::Weak)
        .await
        .unwrap();

    wait_for_connectivity(&[&node_A, &node_B, &node_C, &client_D]).await;

    // Send a discover request from Node A, through B and C, to D. Once Client D
    // receives the discover request from Node A, it should send a  discovery response
    // request back to A at which time this call will resolve (or timeout).
    node_A
        .dht
        .discovery_service_requester()
        .discover_peer(
            client_D.node_identity().public_key().clone(),
            client_D.node_identity().public_key().clone().into(),
        )
        .await
        .unwrap();

    let node_A_peer_manager = node_A.comms.peer_manager();
    let node_B_peer_manager = node_B.comms.peer_manager();
    let node_C_peer_manager = node_C.comms.peer_manager();
    let client_D_peer_manager = client_D.comms.peer_manager();

    // Check that all the nodes know about each other in the chain and the discovery worked
    assert!(
        node_A_peer_manager
            .exists(client_D.node_identity().public_key())
            .await
            .unwrap()
    );
    assert!(
        node_B_peer_manager
            .exists(node_A.node_identity().public_key())
            .await
            .unwrap()
    );
    assert!(
        node_C_peer_manager
            .exists(node_B.node_identity().public_key())
            .await
            .unwrap()
    );
    assert!(
        client_D_peer_manager
            .exists(node_C.node_identity().public_key())
            .await
            .unwrap()
    );
    assert!(
        client_D_peer_manager
            .exists(node_A.node_identity().public_key())
            .await
            .unwrap()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[allow(non_snake_case)]
async fn test_dht_node_discover_propagation() {
    // env_logger::init(); // Set `$env:RUST_LOG = "trace"`
    // Create 4 nodes where A knows B, B knows A and C, C knows B and D, and D knows C

    // Node D knows no one
    let node_D = make_node("node_D", PeerFeatures::COMMUNICATION_NODE, dht_config(), None).await;
    // Node C knows about Node D
    let node_C = make_node(
        "node_C",
        PeerFeatures::COMMUNICATION_NODE,
        dht_config(),
        Some(node_D.to_peer()),
    )
    .await;
    // Node B knows about Node C
    let node_B = make_node(
        "node_B",
        PeerFeatures::COMMUNICATION_NODE,
        dht_config(),
        Some(node_C.to_peer()),
    )
    .await;
    // Node A knows about Node B
    let node_A = make_node(
        "node_A",
        PeerFeatures::COMMUNICATION_NODE,
        dht_config(),
        Some(node_B.to_peer()),
    )
    .await;
    log::info!(
        "Node A = {}, Node B = {}, Node C = {}, Node D = {}",
        node_A.node_identity().node_id().short_str(),
        node_B.node_identity().node_id().short_str(),
        node_C.node_identity().node_id().short_str(),
        node_D.node_identity().node_id().short_str(),
    );

    // To receive messages, clients have to connect
    node_D
        .comms
        .peer_manager()
        .add_or_update_peer(node_C.to_peer())
        .await
        .unwrap();
    node_D
        .comms
        .connectivity()
        .dial_peer(node_C.comms.node_identity().node_id().clone(), RefKind::Weak)
        .await
        .unwrap();

    wait_for_connectivity(&[&node_A, &node_B, &node_C, &node_D]).await;

    // Send a discover request from Node A, through B and C, to D. Once Node D
    // receives the discover request from Node A, it should send a  discovery response
    // request back to A at which time this call will resolve (or timeout).
    node_A
        .dht
        .discovery_service_requester()
        .discover_peer(
            node_D.node_identity().public_key().clone(),
            node_D.node_identity().public_key().clone().into(),
        )
        .await
        .unwrap();

    let node_A_peer_manager = node_A.comms.peer_manager();
    let node_B_peer_manager = node_B.comms.peer_manager();
    let node_C_peer_manager = node_C.comms.peer_manager();
    let node_D_peer_manager = node_D.comms.peer_manager();

    // Check that all the nodes know about each other in the chain and the discovery worked
    assert!(
        node_A_peer_manager
            .exists(node_D.node_identity().public_key())
            .await
            .unwrap()
    );
    assert!(
        node_B_peer_manager
            .exists(node_A.node_identity().public_key())
            .await
            .unwrap()
    );
    assert!(
        node_C_peer_manager
            .exists(node_B.node_identity().public_key())
            .await
            .unwrap()
    );
    assert!(
        node_D_peer_manager
            .exists(node_C.node_identity().public_key())
            .await
            .unwrap()
    );
    assert!(
        node_D_peer_manager
            .exists(node_A.node_identity().public_key())
            .await
            .unwrap()
    );
}

#[tokio::test]
#[allow(non_snake_case)]
#[allow(clippy::too_many_lines)]
async fn test_dht_propagate_dedup() {
    let mut config = dht_config();
    // For this test we want to exactly measure the path of a message, so we disable repropagation of messages (i.e
    // allow 1 occurrence)
    config.dedup_allowed_message_occurrences = 1;
    // Node D knows no one
    let mut node_D = make_node("node_D", PeerFeatures::COMMUNICATION_NODE, config.clone(), None).await;
    // Node C knows about Node D
    let node_C = make_node(
        "node_C",
        PeerFeatures::COMMUNICATION_NODE,
        config.clone(),
        Some(node_D.to_peer()),
    )
    .await;
    // Node B knows about Node C
    let node_B = make_node(
        "node_B",
        PeerFeatures::COMMUNICATION_NODE,
        config.clone(),
        Some(node_C.to_peer()),
    )
    .await;
    // Node A knows about Node B and C
    let node_A = make_node("node_A", PeerFeatures::COMMUNICATION_NODE, config.clone(), [
        node_B.to_peer(),
        node_C.to_peer(),
    ])
    .await;
    log::info!(
        "NodeA = {}, NodeB = {}, Node C = {}, Node D = {}",
        node_A.node_identity().node_id().short_str(),
        node_B.node_identity().node_id().short_str(),
        node_C.node_identity().node_id().short_str(),
        node_D.node_identity().node_id().short_str(),
    );

    // Each node is connected to the peers it has to *send* to, which is the pool lookup propagation actually
    // performs. A receiver does not need the connection in its own pool to be delivered to, so requiring that
    // as well would only add a way to time out on something the test never uses.
    ensure_connected(&node_A, &[
        node_B.node_identity().node_id(),
        node_C.node_identity().node_id(),
    ])
    .await;
    ensure_connected(&node_B, &[node_C.node_identity().node_id()]).await;
    ensure_connected(&node_C, &[node_D.node_identity().node_id()]).await;
    wait_for_connectivity_to_settle(&[&node_A, &node_B, &node_C, &node_D]).await;

    let mut node_A_messaging = node_A.messaging_events.subscribe();
    let mut node_B_messaging = node_B.messaging_events.subscribe();
    let mut node_B_messaging2 = node_B.messaging_events.subscribe();
    let mut node_C_messaging = node_C.messaging_events.subscribe();
    let mut node_C_messaging2 = node_C.messaging_events.subscribe();
    let mut node_D_messaging = node_D.messaging_events.subscribe();
    let mut node_D_messaging2 = node_D.messaging_events.subscribe();

    #[derive(Clone, PartialEq, ::prost::Message)]
    struct Person {
        #[prost(string, tag = "1")]
        name: String,
        #[prost(uint32, tag = "2")]
        age: u32,
    }

    let out_msg = OutboundDomainMessage::new(&123, Person {
        name: "John Conway".into(),
        age: 82,
    });
    node_A
        .dht
        .outbound_requester()
        .propagate(
            node_D.node_identity().public_key().clone().into(),
            OutboundEncryption::encrypt_for(node_D.node_identity().public_key().clone()),
            vec![],
            out_msg,
            String::new(),
        )
        .await
        .unwrap();

    let msg = node_D
        .next_inbound_message(Duration::from_secs(10))
        .await
        .expect("Node D expected an inbound message but it never arrived");
    assert!(msg.decryption_succeeded());
    log::info!("Received message {}", msg.tag);
    let person = msg
        .decryption_result
        .unwrap()
        .decode_part::<Person>(1)
        .unwrap()
        .unwrap();
    assert_eq!(person.name, "John Conway");

    let node_A_id = node_A.node_identity().node_id().clone();
    let node_B_id = node_B.node_identity().node_id().clone();
    let node_C_id = node_C.node_identity().node_id().clone();
    let node_D_id = node_D.node_identity().node_id().clone();

    // Ensure that the message has propagated before disconnecting everyone. Node C sits on two propagation
    // paths (A->C directly, and A->B->C), so it must be given time to receive *both* messages - waiting for a
    // single event here let the second one still be in flight when the nodes were shut down below, which is
    // what the assertions further down then reported as a missing message.
    wait_for_messages_received(&mut node_B_messaging2, 1, "node B").await;
    wait_for_messages_received(&mut node_C_messaging2, 2, "node C").await;
    wait_for_messages_received(&mut node_D_messaging2, 1, "node D").await;

    node_A.shutdown().await;
    node_B.shutdown().await;
    node_C.shutdown().await;
    node_D.shutdown().await;

    // Check the message flow BEFORE deduping
    let received = filter_received(collect_try_recv!(node_A_messaging, timeout = Duration::from_secs(20)));
    // Expected race condition: If A->(B|C)->(C|B) before A->(C|B) then (C|B)->A
    if !received.is_empty() {
        assert_eq!(count_messages_received(&received, &[&node_B_id, &node_C_id]), 2);
    }

    let received = filter_received(collect_try_recv!(node_B_messaging, timeout = Duration::from_secs(20)));
    let recv_count = count_messages_received(&received, &[&node_A_id, &node_C_id]);
    // Expected race condition: If A->B->C before A->C then C->B does not happen
    assert!(
        (1..=2).contains(&recv_count),
        "expected recv_count to be in [1-2] but was {recv_count}"
    );

    let received = filter_received(collect_try_recv!(node_C_messaging, timeout = Duration::from_secs(20)));
    let recv_count = count_messages_received(&received, &[&node_A_id, &node_B_id]);
    assert_eq!(recv_count, 2);
    assert_eq!(count_messages_received(&received, &[&node_D_id]), 0);

    let received = filter_received(collect_try_recv!(node_D_messaging, timeout = Duration::from_secs(20)));
    assert_eq!(received.len(), 1);
    assert_eq!(count_messages_received(&received, &[&node_C_id]), 1);
}

#[tokio::test]
#[allow(non_snake_case)]
#[allow(clippy::too_many_lines)]
async fn test_dht_do_not_store_invalid_message_in_dedup() {
    let mut config = dht_config();
    config.dedup_allowed_message_occurrences = 1;

    // Node C receives messages from A and B
    let mut node_C = make_node("node_B", PeerFeatures::COMMUNICATION_NODE, config.clone(), None).await;

    // Node B forwards a message from A but modifies it
    let mut node_B = make_node(
        "node_B",
        PeerFeatures::COMMUNICATION_NODE,
        config.clone(),
        Some(node_C.to_peer()),
    )
    .await;

    // Node A creates a message sends it to B, B modifies it, sends it to C; Node A sends message to C
    let node_A = make_node("node_A", PeerFeatures::COMMUNICATION_NODE, config.clone(), [
        node_B.to_peer(),
        node_C.to_peer(),
    ])
    .await;

    log::info!(
        "NodeA = {}, NodeB = {}, NodeC = {}",
        node_A.node_identity().node_id().short_str(),
        node_B.node_identity().node_id().short_str(),
        node_C.node_identity().node_id().short_str(),
    );

    // Connect the peers that should be connected
    node_A
        .comms
        .connectivity()
        .dial_peer(node_B.node_identity().node_id().clone(), RefKind::Weak)
        .await
        .unwrap();

    node_A
        .comms
        .connectivity()
        .dial_peer(node_C.node_identity().node_id().clone(), RefKind::Weak)
        .await
        .unwrap();

    node_B
        .comms
        .connectivity()
        .dial_peer(node_C.node_identity().node_id().clone(), RefKind::Weak)
        .await
        .unwrap();

    let mut node_C_messaging = node_C.messaging_events.subscribe();

    #[derive(Clone, PartialEq, ::prost::Message)]
    struct Person {
        #[prost(string, tag = "1")]
        name: String,
        #[prost(uint32, tag = "2")]
        age: u32,
    }

    // Just a message to test connectivity between Node A -> Node C, and to get the header from
    let out_msg = OutboundDomainMessage::new(&123, Person {
        name: "John Conway".into(),
        age: 82,
    });

    node_A
        .dht
        .outbound_requester()
        .send_message(
            SendMessageParams::new()
                .direct_node_id(node_B.node_identity().node_id().clone())
                .with_destination(node_C.node_identity().public_key().clone().into())
                .force_origin()
                .finish(),
            out_msg,
        )
        .await
        .unwrap();

    // Get the message that was received by Node B
    let mut msg = node_B.next_inbound_message(Duration::from_secs(10)).await.unwrap();
    let bytes = msg.decryption_result.unwrap().encode_into_bytes_mut();

    // Clone header without modification
    let header_unmodified = msg.dht_header.clone();

    // Modify the header
    msg.dht_header.message_type = DhtMessageType::try_from(3i32).unwrap();

    // Forward modified message to Node C - Should get us banned
    node_B
        .dht
        .outbound_requester()
        .send_raw(
            SendMessageParams::new()
                .direct_node_id(node_C.node_identity().node_id().clone())
                .with_dht_header(msg.dht_header)
                .finish(),
            bytes.clone(),
        )
        .await
        .unwrap();

    async_assert_eventually!(
        {
            let n = node_C
                .comms
                .peer_manager()
                .find_by_node_id(node_B.node_identity().node_id())
                .await
                .unwrap()
                .unwrap();
            n.is_banned()
        },
        expect = true,
        max_attempts = 10,
        interval = Duration::from_secs(3)
    );

    node_A
        .dht
        .outbound_requester()
        .send_raw(
            SendMessageParams::new()
                .direct_node_id(node_C.node_identity().node_id().clone())
                .with_dht_header(header_unmodified)
                .finish(),
            bytes,
        )
        .await
        .unwrap();

    // Node C receives the correct message from Node A
    let msg = node_C
        .next_inbound_message(Duration::from_secs(10))
        .await
        .expect("Node C expected an inbound message but it never arrived");
    assert!(msg.decryption_succeeded());
    log::info!("Received message {}", msg.tag);
    let person = msg
        .decryption_result
        .unwrap()
        .decode_part::<Person>(1)
        .unwrap()
        .unwrap();
    assert_eq!(person.name, "John Conway");

    let node_A_id = node_A.node_identity().node_id().clone();
    let node_B_id = node_B.node_identity().node_id().clone();

    node_A.shutdown().await;
    node_B.shutdown().await;
    node_C.shutdown().await;

    // Check the message flow BEFORE deduping
    let received = filter_received(collect_try_recv!(node_C_messaging, timeout = Duration::from_secs(20)));

    let received_from_a = count_messages_received(&received, &[&node_A_id]);
    let received_from_b = count_messages_received(&received, &[&node_B_id]);

    assert_eq!(received_from_a, 1);
    assert_eq!(received_from_b, 1);
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
#[allow(non_snake_case)]
async fn test_dht_repropagate() {
    let mut config = dht_config();
    config.dedup_allowed_message_occurrences = 3;
    let mut node_C = make_node("node_C", PeerFeatures::COMMUNICATION_NODE, config.clone(), []).await;
    let mut node_B = make_node("node_B", PeerFeatures::COMMUNICATION_NODE, config.clone(), [
        node_C.to_peer()
    ])
    .await;
    let mut node_A = make_node("node_A", PeerFeatures::COMMUNICATION_NODE, config, [
        node_B.to_peer(),
        node_C.to_peer(),
    ])
    .await;
    node_A
        .comms
        .peer_manager()
        .add_or_update_peer(node_C.to_peer())
        .await
        .unwrap();
    node_B
        .comms
        .peer_manager()
        .add_or_update_peer(node_C.to_peer())
        .await
        .unwrap();
    node_C
        .comms
        .peer_manager()
        .add_or_update_peer(node_A.to_peer())
        .await
        .unwrap();
    node_C
        .comms
        .peer_manager()
        .add_or_update_peer(node_B.to_peer())
        .await
        .unwrap();
    log::info!(
        "NodeA = {}, NodeB = {}, Node C = {}",
        node_A.node_identity().node_id().short_str(),
        node_B.node_identity().node_id().short_str(),
        node_C.node_identity().node_id().short_str(),
    );

    // Connect the peers that should be connected
    async fn connect_nodes(node1: &mut TestNode, node2: &mut TestNode) {
        node1
            .comms
            .connectivity()
            .dial_peer(node2.node_identity().node_id().clone(), RefKind::Weak)
            .await
            .unwrap();
    }
    // Pre-connect nodes, this helps message passing be more deterministic
    connect_nodes(&mut node_A, &mut node_B).await;
    connect_nodes(&mut node_A, &mut node_C).await;
    connect_nodes(&mut node_B, &mut node_C).await;

    #[derive(Clone, PartialEq, ::prost::Message)]
    struct Person {
        #[prost(string, tag = "1")]
        name: String,
        #[prost(uint32, tag = "2")]
        age: u32,
    }

    let out_msg = OutboundDomainMessage::new(&123, Person {
        name: "Alan Turing".into(),
        age: 41,
    });
    node_A
        .dht
        .outbound_requester()
        .propagate(
            NodeDestination::Unknown,
            OutboundEncryption::ClearText,
            vec![],
            out_msg.clone(),
            String::new(),
        )
        .await
        .unwrap();

    async fn receive_and_repropagate(node: &mut TestNode, out_msg: &OutboundDomainMessage<Person>) {
        let msg = node
            .next_inbound_message(Duration::from_secs(10))
            .await
            .unwrap_or_else(|| panic!("{} expected an inbound message but it never arrived", node.name()));
        log::info!("Received message {}", msg.tag);

        node.dht
            .outbound_requester()
            .send_message(
                SendMessageParams::new()
                    .propagate(NodeDestination::Unknown, vec![])
                    .with_destination(NodeDestination::Unknown)
                    .with_tag(msg.tag)
                    .finish(),
                out_msg.clone(),
            )
            .await
            .unwrap()
            .resolve()
            .await
            .unwrap();
    }

    // This relies on the DHT being set with dedup_allowed_message_occurrences = 3
    receive_and_repropagate(&mut node_B, &out_msg).await;
    receive_and_repropagate(&mut node_C, &out_msg).await;
    receive_and_repropagate(&mut node_A, &out_msg).await;
    receive_and_repropagate(&mut node_B, &out_msg).await;
    receive_and_repropagate(&mut node_C, &out_msg).await;
    receive_and_repropagate(&mut node_A, &out_msg).await;
    receive_and_repropagate(&mut node_B, &out_msg).await;
    receive_and_repropagate(&mut node_C, &out_msg).await;

    node_A.shutdown().await;
    node_B.shutdown().await;
    node_C.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[allow(non_snake_case)]
async fn test_dht_propagate_message_contents_not_malleable_ban() {
    let node_C = make_node("node_C", PeerFeatures::COMMUNICATION_NODE, dht_config(), None).await;
    // Node B knows about Node C
    let mut node_B = make_node(
        "node_B",
        PeerFeatures::COMMUNICATION_NODE,
        dht_config(),
        Some(node_C.to_peer()),
    )
    .await;
    // Node A knows about Node B
    let node_A = make_node(
        "node_A",
        PeerFeatures::COMMUNICATION_NODE,
        dht_config(),
        Some(node_B.to_peer()),
    )
    .await;
    node_A
        .comms
        .peer_manager()
        .add_or_update_peer(node_C.to_peer())
        .await
        .unwrap();
    log::info!(
        "NodeA = {}, NodeB = {}",
        node_A.node_identity().node_id().short_str(),
        node_B.node_identity().node_id().short_str(),
    );

    // Connect the peers that should be connected
    node_A
        .comms
        .connectivity()
        .dial_peer(node_B.node_identity().node_id().clone(), RefKind::Weak)
        .await
        .unwrap();

    #[derive(Clone, PartialEq, ::prost::Message)]
    struct Person {
        #[prost(string, tag = "1")]
        name: String,
        #[prost(uint32, tag = "2")]
        age: u32,
    }

    let out_msg = Person {
        name: "John Conway".into(),
        age: 82,
    };
    node_A
        .dht
        .outbound_requester()
        .send_message_no_header(
            SendMessageParams::new()
                .direct_node_id(node_B.node_identity().node_id().clone())
                .with_destination(node_A.node_identity().public_key().clone().into())
                .with_encryption(OutboundEncryption::ClearText)
                .force_origin()
                .finish(),
            out_msg,
        )
        .await
        .unwrap();

    let msg = node_B.next_inbound_message(Duration::from_secs(10)).await.unwrap();

    let mut envelope = msg.decryption_result.unwrap();
    // Change the message
    envelope.push_part([0x42].to_vec());

    let mut connectivity_events = node_C.comms.connectivity().get_event_subscription();

    // Propagate the changed message (to node C)
    node_B
        .dht
        .outbound_requester()
        .send_message_no_header(
            SendMessageParams::new()
                .propagate(node_B.node_identity().public_key().clone().into(), vec![
                    msg.source_peer.node_id.clone(),
                ])
                .with_dht_header(msg.dht_header)
                .finish(),
            envelope,
        )
        .await
        .unwrap();
    let node_B_node_id = node_B.node_identity().node_id().clone();

    // Node C should ban node B
    let banned_node_id = streams::assert_in_broadcast(
        &mut connectivity_events,
        |r| match r {
            ConnectivityEvent::PeerBanned(node_id) => Some(node_id),
            _ => None,
        },
        Duration::from_secs(10),
    )
    .await;
    assert_eq!(banned_node_id, node_B_node_id);

    node_A.shutdown().await;
    node_B.shutdown().await;
    node_C.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[allow(non_snake_case)]
async fn test_dht_header_not_malleable() {
    let node_C = make_node("node_C", PeerFeatures::COMMUNICATION_NODE, dht_config(), None).await;
    // Node B knows about Node C
    let mut node_B = make_node(
        "node_B",
        PeerFeatures::COMMUNICATION_NODE,
        dht_config(),
        Some(node_C.to_peer()),
    )
    .await;
    // Node A knows about Node B
    let node_A = make_node(
        "node_A",
        PeerFeatures::COMMUNICATION_NODE,
        dht_config(),
        Some(node_B.to_peer()),
    )
    .await;
    node_A
        .comms
        .peer_manager()
        .add_or_update_peer(node_C.to_peer())
        .await
        .unwrap();
    log::info!(
        "NodeA = {}, NodeB = {}",
        node_A.node_identity().node_id().short_str(),
        node_B.node_identity().node_id().short_str(),
    );

    // Connect the peers that should be connected
    node_A
        .comms
        .connectivity()
        .dial_peer(node_B.node_identity().node_id().clone(), RefKind::Weak)
        .await
        .unwrap();

    #[derive(Clone, PartialEq, ::prost::Message)]
    struct Person {
        #[prost(string, tag = "1")]
        name: String,
        #[prost(uint32, tag = "2")]
        age: u32,
    }

    let out_msg = Person {
        name: "John Conway".into(),
        age: 82,
    };
    node_A
        .dht
        .outbound_requester()
        .send_message_no_header(
            SendMessageParams::new()
                .direct_node_id(node_B.node_identity().node_id().clone())
                .with_destination(node_A.node_identity().public_key().clone().into())
                .with_encryption(OutboundEncryption::ClearText)
                .force_origin()
                .finish(),
            out_msg,
        )
        .await
        .unwrap();

    let mut msg = node_B.next_inbound_message(Duration::from_secs(10)).await.unwrap();

    // Modify the header
    msg.dht_header.message_type = DhtMessageType::try_from(21i32).unwrap();

    let envelope = msg.decryption_result.unwrap();
    let mut connectivity_events = node_C.comms.connectivity().get_event_subscription();

    // Propagate the changed message (to node C)
    node_B
        .dht
        .outbound_requester()
        .send_message_no_header(
            SendMessageParams::new()
                .propagate(node_B.node_identity().public_key().clone().into(), vec![
                    msg.source_peer.node_id.clone(),
                ])
                .with_dht_header(msg.dht_header)
                .finish(),
            envelope,
        )
        .await
        .unwrap();
    let node_B_node_id = node_B.node_identity().node_id().clone();

    // Node C should ban node B
    let banned_node_id = streams::assert_in_broadcast(
        &mut connectivity_events,
        |r| match r {
            ConnectivityEvent::PeerBanned(node_id) => Some(node_id),
            _ => None,
        },
        Duration::from_secs(10),
    )
    .await;
    assert_eq!(banned_node_id, node_B_node_id);

    node_A.shutdown().await;
    node_B.shutdown().await;
    node_C.shutdown().await;
}

fn filter_received(events: Vec<MessagingEvent>) -> Vec<MessagingEvent> {
    events
        .into_iter()
        .filter(|e| matches!(e, MessagingEvent::MessageReceived(_, _)))
        .collect()
}

fn count_messages_received(events: &[MessagingEvent], node_ids: &[&NodeId]) -> usize {
    events
        .iter()
        .filter(|event| {
            unpack_enum!(MessagingEvent::MessageReceived(recv_node_id, _tag) = &**event);
            node_ids.contains(&recv_node_id)
        })
        .count()
}

/// How long the topology barriers below are prepared to wait. A loaded machine - CI runs the whole suite at
/// once - takes noticeably longer than an idle one to work through the dial churn described in
/// `wait_for_connectivity_to_settle`, and a barrier that gives up while that is still in progress fails a
/// test that was about to be fine.
const TOPOLOGY_TIMEOUT: Duration = Duration::from_secs(30);
/// How long connectivity has to stay silent before the topology counts as settled.
const CONNECTIVITY_QUIET_PERIOD: Duration = Duration::from_millis(500);
/// How long a link is given to come up on its own before `ensure_connected` dials it itself.
const DIAL_FALLBACK_GRACE: Duration = Duration::from_secs(2);

/// Ensure `node`'s connectivity pool holds an active connection to each of `peers`, dialling only what does
/// not come up on its own.
///
/// Propagation selects its peers from this pool, so a node that has not registered a connection yet will
/// silently propagate to a subset of the topology the test set up. `wait_for_connectivity` is not enough -
/// it only waits for a node to come online, which takes a single peer.
///
/// The dial is a fallback rather than the primary mechanism, because each node's DHT connectivity already
/// dials the peers it was seeded with at start-up. Dialling on top of that races it, and the dialer only
/// de-duplicates dials that are still *in flight*: one issued after the DHT's has completed but before
/// `ConnectivityManager` has pooled it gets dialled again, tie-broken against the live connection, and the
/// loser disconnected. Outbound messaging gives up after `MAX_SEND_RETRIES` and fails whatever it was
/// holding rather than re-queueing it, so that churn is silent message loss - measured at 8-10 dials and
/// 1-3 tie-breaks per run for this test's four links when it dialled every link itself.
///
/// Waiting alone is not enough either: the DHT tries once at start-up and then not again until
/// `update_interval` (2 minutes), so a single failed attempt under load would otherwise hang the test for
/// longer than it is willing to wait. `DIAL_FALLBACK_GRACE` is the compromise - long enough that the
/// start-up dial has normally landed and no second dial is issued at all, short enough to recover from one
/// that did not.
async fn ensure_connected(node: &TestNode, peers: &[&NodeId]) {
    let mut connectivity = node.comms.connectivity();
    let start = Instant::now();
    let deadline = start
        .checked_add(TOPOLOGY_TIMEOUT)
        .expect("TOPOLOGY_TIMEOUT overflows the clock");
    let mut dialled = false;
    loop {
        let active = connectivity.get_active_connections().await.unwrap();
        let missing = peers
            .iter()
            .filter(|peer| !active.iter().any(|conn| conn.peer_node_id() == **peer))
            .copied()
            .collect::<Vec<_>>();
        if missing.is_empty() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "{} never established a connection to {missing:?} (wanted {peers:?})",
            node.name
        );
        if !dialled && start.elapsed() >= DIAL_FALLBACK_GRACE {
            dialled = true;
            for peer in missing {
                // A failure here is not fatal: the loop keeps waiting until `deadline` either way, and the
                // assertion above is the one that should report a link that never comes up.
                let _result = connectivity.dial_peer(peer.clone(), RefKind::Weak).await;
            }
            continue;
        }
        time::sleep(Duration::from_millis(100)).await;
    }
}

/// Wait until none of `nodes` has reported a connectivity change for `CONNECTIVITY_QUIET_PERIOD`.
///
/// Having the right peers in the pool right now does not mean the topology has stopped moving. A node learns
/// about a peer from the connection that peer opened to it, and may then dial it back before its own pool has
/// caught up; the redundant connection is tie-broken against the live one, the loser is disconnected, and
/// anything the outbound handler cannot re-establish within `MAX_SEND_RETRIES` is dropped rather than
/// re-queued. Not seeding that churn in the first place is what the absence of `dial_peer` above is for -
/// this is the backstop for whatever the test did not set up itself.
async fn wait_for_connectivity_to_settle(nodes: &[&TestNode]) {
    // Only the fact that something happened is forwarded, never the event itself: a lagged subscription has
    // by definition just missed a burst of them, and that has to count as activity rather than as silence.
    let (tx, mut rx) = mpsc::channel::<()>(100);
    for node in nodes {
        let mut events = node.comms.connectivity().get_event_subscription();
        let tx = tx.clone();
        // These outlive the call: the point is to observe the *next* quiet period, not to drain a fixed
        // number of events, and the nodes are shut down at the end of the test either way.
        tokio::spawn(async move {
            while let Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) = events.recv().await {
                if tx.send(()).await.is_err() {
                    break;
                }
            }
        });
    }
    drop(tx);

    let deadline = Instant::now()
        .checked_add(TOPOLOGY_TIMEOUT)
        .expect("TOPOLOGY_TIMEOUT overflows the clock");
    // `Ok(None)` means every forwarding task has gone away, which is as quiet as it gets.
    while let Ok(Some(())) = time::timeout(CONNECTIVITY_QUIET_PERIOD, rx.recv()).await {
        if Instant::now() >= deadline {
            // Let the test's own assertions report what the churn actually did rather than failing here.
            break;
        }
    }
}

/// Wait until `count` messages have actually been *received* by a node.
///
/// `MessagingEvent` also carries the protocol-exit variants, so a bare `recv()` can return without a single
/// message having arrived - only `MessageReceived` is counted here.
async fn wait_for_messages_received(events: &mut broadcast::Receiver<MessagingEvent>, count: usize, node: &str) {
    // The senders are kept so that a timeout can say *which* message went missing. Which of the two paths
    // into a node failed is the whole diagnosis, and without this the failure is just a number.
    let mut senders = Vec::with_capacity(count);
    while senders.len() < count {
        let event = time::timeout(Duration::from_secs(20), events.recv())
            .await
            .unwrap_or_else(|_| {
                panic!(
                    "timed out waiting for {count} message(s) at {node}, got {} (from {senders:?})",
                    senders.len()
                )
            })
            .unwrap();
        if let MessagingEvent::MessageReceived(sender, _tag) = event {
            senders.push(sender);
        }
    }
}

async fn wait_for_connectivity(nodes: &[&TestNode]) {
    for node in nodes {
        node.comms
            .connectivity()
            .wait_for_connectivity(Duration::from_secs(10))
            .await
            .unwrap();
    }
}
