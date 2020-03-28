// Copyright 2020, The Tari Project
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

use crate::{
    backoff::ConstantBackoff,
    builder::CommsBuilder,
    connection_manager::ConnectionManagerEvent,
    memsocket,
    message::{InboundMessage, OutboundMessage},
    multiaddr::{Multiaddr, Protocol},
    peer_manager::{Peer, PeerFeatures},
    pipeline,
    pipeline::SinkService,
    protocol::{messaging::MessagingEvent, ProtocolEvent, Protocols},
    runtime,
    test_utils::node_identity::build_node_identity,
    transports::MemoryTransport,
    types::CommsSubstream,
    CommsNode,
};
use bytes::Bytes;
use futures::{channel::mpsc, AsyncReadExt, AsyncWriteExt, SinkExt, StreamExt};
use std::{collections::HashSet, convert::identity, hash::Hash, sync::Arc, time::Duration};
use tari_storage::HashmapDatabase;
use tari_test_utils::{collect_stream, unpack_enum};

async fn spawn_node(
    protocols: Protocols<CommsSubstream>,
) -> (CommsNode, mpsc::Receiver<InboundMessage>, mpsc::Sender<OutboundMessage>) {
    let addr = format!("/memory/{}", memsocket::acquire_next_memsocket_port())
        .parse::<Multiaddr>()
        .unwrap();
    let node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    node_identity.set_public_address(addr.clone()).unwrap();

    let (inbound_tx, inbound_rx) = mpsc::channel(10);
    let (outbound_tx, outbound_rx) = mpsc::channel(10);

    let comms_node = CommsBuilder::new()
        // These calls are just to get rid of unused function warnings. 
        // <IrrelevantCalls>
        .with_executor(runtime::current_executor())
        .with_dial_backoff(ConstantBackoff::new(Duration::from_millis(500)))
        .on_shutdown(|| {})
        // </IrrelevantCalls>
        .with_listener_address(addr)
        .with_transport(MemoryTransport)
        .with_peer_storage(HashmapDatabase::new())

        .with_node_identity(node_identity)
        .with_protocols(protocols)
        .build()
        .unwrap();

    let comms_node = comms_node
        .with_messaging_pipeline(
            pipeline::Builder::new()
                // Outbound messages will be forwarded "as is" to outbound messaging
                .with_outbound_pipeline(outbound_rx, identity)
                .max_concurrent_inbound_tasks(1)
                // Inbound messages will be forwarded "as is" to inbound_tx
                .with_inbound_pipeline(SinkService::new(inbound_tx))
                .finish(),
        )
        .spawn()
        .await
        .unwrap();

    unpack_enum!(Protocol::Memory(_port) = comms_node.listening_address().iter().next().unwrap());

    // This call is to get rid of unused function warnings
    comms_node.peer_manager();

    (comms_node, inbound_rx, outbound_tx)
}

#[tokio_macros::test_basic]
async fn peer_to_peer_custom_protocols() {
    const TEST_PROTOCOL: Bytes = Bytes::from_static(b"/tari/test");
    const ANOTHER_TEST_PROTOCOL: Bytes = Bytes::from_static(b"/tari/test-again");
    const TEST_MSG: &[u8] = b"Hello Tari";
    const ANOTHER_TEST_MSG: &[u8] = b"Comms is running smoothly";

    // Setup test protocols
    let (test_sender, _test_protocol_rx1) = mpsc::channel(10);
    let (another_test_sender, mut another_test_protocol_rx1) = mpsc::channel(10);
    let protocols1 = Protocols::new()
        .add(&[TEST_PROTOCOL], test_sender)
        .add(&[ANOTHER_TEST_PROTOCOL], another_test_sender);
    let (test_sender, mut test_protocol_rx2) = mpsc::channel(10);
    let (another_test_sender, _another_test_protocol_rx2) = mpsc::channel(10);
    let protocols2 = Protocols::new()
        .add(&[TEST_PROTOCOL], test_sender)
        .add(&[ANOTHER_TEST_PROTOCOL], another_test_sender);

    let (comms_node1, _, _) = spawn_node(protocols1).await;
    let (comms_node2, _, _) = spawn_node(protocols2).await;

    let node_identity1 = comms_node1.node_identity();
    let node_identity2 = comms_node2.node_identity();
    comms_node1
        .peer_manager()
        .add_peer(Peer::new(
            node_identity2.public_key().clone(),
            node_identity2.node_id().clone(),
            node_identity2.public_address().clone().into(),
            Default::default(),
            Default::default(),
            &[TEST_PROTOCOL, ANOTHER_TEST_PROTOCOL],
        ))
        .await
        .unwrap();

    let mut conn_man_events1 = comms_node1.subscribe_connection_manager_events();
    let mut conn_man_requester1 = comms_node1.connection_manager();
    let mut conn_man_events2 = comms_node2.subscribe_connection_manager_events();

    let mut conn1 = conn_man_requester1
        .dial_peer(node_identity2.node_id().clone())
        .await
        .unwrap();

    // Check that both nodes get the PeerConnected event. We subscribe after the nodes are initialized
    // so we miss those events.
    let next_event = conn_man_events2.next().await.unwrap().unwrap();
    unpack_enum!(ConnectionManagerEvent::PeerConnected(conn2) = Arc::try_unwrap(next_event).unwrap());
    let next_event = conn_man_events1.next().await.unwrap().unwrap();
    unpack_enum!(ConnectionManagerEvent::PeerConnected(_conn) = &*next_event);

    // Let's speak both our test protocols
    let mut negotiated_substream1 = conn1.open_substream(&TEST_PROTOCOL).await.unwrap();
    assert_eq!(negotiated_substream1.protocol, TEST_PROTOCOL);
    negotiated_substream1.stream.write_all(TEST_MSG).await.unwrap();

    let mut negotiated_substream2 = conn2.open_substream(&ANOTHER_TEST_PROTOCOL).await.unwrap();
    assert_eq!(negotiated_substream2.protocol, ANOTHER_TEST_PROTOCOL);
    negotiated_substream2.stream.write_all(ANOTHER_TEST_MSG).await.unwrap();

    // Read TEST_PROTOCOL message to node 2 from node 1
    let negotiation = test_protocol_rx2.next().await.unwrap();
    assert_eq!(negotiation.protocol, TEST_PROTOCOL);
    unpack_enum!(ProtocolEvent::NewInboundSubstream(node_id, substream) = negotiation.event);
    assert_eq!(&*node_id, node_identity1.node_id());
    let mut buf = [0u8; TEST_MSG.len()];
    substream.read_exact(&mut buf).await.unwrap();
    assert_eq!(buf, TEST_MSG);

    // Read ANOTHER_TEST_PROTOCOL message to node 1 from node 2
    let negotiation = another_test_protocol_rx1.next().await.unwrap();
    assert_eq!(negotiation.protocol, ANOTHER_TEST_PROTOCOL);
    unpack_enum!(ProtocolEvent::NewInboundSubstream(node_id, substream) = negotiation.event);
    assert_eq!(&*node_id, node_identity2.node_id());
    let mut buf = [0u8; ANOTHER_TEST_MSG.len()];
    substream.read_exact(&mut buf).await.unwrap();
    assert_eq!(buf, ANOTHER_TEST_MSG);

    comms_node1.shutdown().await;
    comms_node2.shutdown().await;
}

#[tokio_macros::test_basic]
async fn peer_to_peer_messaging() {
    const NUM_MSGS: usize = 100;

    let (comms_node1, inbound_rx1, mut outbound_tx1) = spawn_node(Protocols::new()).await;
    let (comms_node2, inbound_rx2, mut outbound_tx2) = spawn_node(Protocols::new()).await;

    let messaging_events1 = comms_node1.subscribe_messaging_events();
    let messaging_events2 = comms_node2.subscribe_messaging_events();

    let node_identity1 = comms_node1.node_identity();
    let node_identity2 = comms_node2.node_identity();
    comms_node1
        .peer_manager()
        .add_peer(Peer::new(
            node_identity2.public_key().clone(),
            node_identity2.node_id().clone(),
            node_identity2.public_address().clone().into(),
            Default::default(),
            Default::default(),
            &[],
        ))
        .await
        .unwrap();

    // Send NUM_MSGS messages from node 1 to node 2
    for i in 0..NUM_MSGS {
        let outbound_msg = OutboundMessage::new(
            node_identity2.node_id().clone(),
            Default::default(),
            format!("#{:0>3} - comms messaging is so hot right now!", i).into(),
        );
        outbound_tx1.send(outbound_msg).await.unwrap();
    }

    let messages1_to_2 = collect_stream!(inbound_rx2, take = NUM_MSGS, timeout = Duration::from_secs(10));
    let events = collect_stream!(messaging_events1, take = NUM_MSGS, timeout = Duration::from_secs(10));
    events.into_iter().map(Result::unwrap).for_each(|m| {
        unpack_enum!(MessagingEvent::MessageSent(_t) = &*m);
    });

    let events = collect_stream!(messaging_events2, take = NUM_MSGS, timeout = Duration::from_secs(10));
    events.into_iter().map(Result::unwrap).for_each(|m| {
        unpack_enum!(MessagingEvent::MessageReceived(_n, _t) = &*m);
    });

    // Send NUM_MSGS messages from node 2 to node 1
    for i in 0..NUM_MSGS {
        let outbound_msg = OutboundMessage::new(
            node_identity1.node_id().clone(),
            Default::default(),
            format!("#{:0>3} - comms messaging is so hot right now!", i).into(),
        );
        outbound_tx2.send(outbound_msg).await.unwrap();
    }

    let messages2_to_1 = collect_stream!(inbound_rx1, take = NUM_MSGS, timeout = Duration::from_secs(10));

    // Check that we got all the messages
    let check_messages = |msgs: Vec<InboundMessage>| {
        for (i, msg) in msgs.iter().enumerate() {
            let expected_msg_prefix = format!("#{:0>3}", i);
            // 0..4 zero padded prefix bytes e.g. #003, #023, #100
            assert_eq!(&msg.body[0..4], expected_msg_prefix.as_bytes());
        }
    };
    assert_eq!(messages1_to_2.len(), NUM_MSGS);
    check_messages(messages1_to_2);
    assert_eq!(messages2_to_1.len(), NUM_MSGS);
    check_messages(messages2_to_1);

    comms_node1.shutdown().await;
    comms_node2.shutdown().await;
}

#[tokio_macros::test_basic]
async fn peer_to_peer_messaging_simultaneous() {
    const NUM_MSGS: usize = 10;

    let (comms_node1, inbound_rx1, mut outbound_tx1) = spawn_node(Protocols::new()).await;
    let (comms_node2, inbound_rx2, mut outbound_tx2) = spawn_node(Protocols::new()).await;

    let o1 = outbound_tx1.clone();
    let o2 = outbound_tx2.clone();

    let node_identity1 = comms_node1.node_identity().clone();
    let node_identity2 = comms_node2.node_identity().clone();
    comms_node1
        .peer_manager()
        .add_peer(Peer::new(
            node_identity2.public_key().clone(),
            node_identity2.node_id().clone(),
            node_identity2.public_address().clone().into(),
            Default::default(),
            Default::default(),
            &[],
        ))
        .await
        .unwrap();
    comms_node2
        .peer_manager()
        .add_peer(Peer::new(
            node_identity1.public_key().clone(),
            node_identity1.node_id().clone(),
            node_identity1.public_address().clone().into(),
            Default::default(),
            Default::default(),
            &[],
        ))
        .await
        .unwrap();

    // Simultaneously send messages between the two nodes
    let rt_handle = runtime::current_executor();
    let handle1 = rt_handle.spawn(async move {
        for i in 0..NUM_MSGS {
            let outbound_msg = OutboundMessage::new(
                node_identity2.node_id().clone(),
                Default::default(),
                format!("#{:0>3} - comms messaging is so hot right now!", i).into(),
            );
            outbound_tx1.send(outbound_msg).await.unwrap();
        }
    });

    let handle2 = rt_handle.spawn(async move {
        for i in 0..NUM_MSGS {
            let outbound_msg = OutboundMessage::new(
                node_identity1.node_id().clone(),
                Default::default(),
                format!("#{:0>3} - comms messaging is so hot right now!", i).into(),
            );
            outbound_tx2.send(outbound_msg).await.unwrap();
        }
    });

    handle1.await.unwrap();
    handle2.await.unwrap();

    // Tasks are finished, let's see if all the messages made it though
    let messages1_to_2 = collect_stream!(inbound_rx2, take = NUM_MSGS, timeout = Duration::from_secs(10));
    let messages2_to_1 = collect_stream!(inbound_rx1, take = NUM_MSGS, timeout = Duration::from_secs(10));

    assert!(has_unique_elements(messages1_to_2.into_iter().map(|m| m.body)));
    assert!(has_unique_elements(messages2_to_1.into_iter().map(|m| m.body)));

    drop(o1);
    drop(o2);

    comms_node1.shutdown().await;
    comms_node2.shutdown().await;
}

fn has_unique_elements<T>(iter: T) -> bool
where
    T: IntoIterator,
    T::Item: Eq + Hash,
{
    let mut uniq = HashSet::new();
    iter.into_iter().all(move |x| uniq.insert(x))
}
