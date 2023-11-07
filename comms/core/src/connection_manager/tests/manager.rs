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

use std::time::Duration;

use futures::future;
use tari_shutdown::Shutdown;
use tari_test_utils::{collect_try_recv, unpack_enum};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    runtime::Handle,
    sync::{broadcast, mpsc, oneshot},
};

use crate::{
    backoff::ConstantBackoff,
    connection_manager::{
        ConnectionManager,
        ConnectionManagerError,
        ConnectionManagerEvent,
        ConnectionManagerRequester,
    },
    net_address::{MultiaddressesWithStats, PeerAddressSource},
    peer_manager::{NodeId, Peer, PeerFeatures, PeerFlags, PeerManagerError},
    protocol::{ProtocolEvent, ProtocolId, Protocols},
    test_utils::{
        build_peer_manager,
        count_string_occurrences,
        node_identity::{build_node_identity, ordered_node_identities},
        test_node::{build_connection_manager, TestNodeConfig},
    },
    transports::{MemoryTransport, TcpTransport},
    PeerConnectionError,
};

#[tokio::test]
async fn connect_to_nonexistent_peer() {
    let rt_handle = Handle::current();
    let node_identity = build_node_identity(PeerFeatures::empty());
    let (request_tx, request_rx) = mpsc::channel(1);
    let (event_tx, _) = broadcast::channel(1);
    let mut requester = ConnectionManagerRequester::new(request_tx, event_tx.clone());
    let mut shutdown = Shutdown::new();

    let peer_manager = build_peer_manager();

    let connection_manager = ConnectionManager::new(
        Default::default(),
        MemoryTransport,
        ConstantBackoff::new(Duration::from_secs(1)),
        request_rx,
        node_identity,
        peer_manager,
        event_tx,
        shutdown.to_signal(),
    );

    rt_handle.spawn(connection_manager.run());

    let err = requester.dial_peer(NodeId::default()).await.unwrap_err();
    unpack_enum!(ConnectionManagerError::PeerManagerError(PeerManagerError::PeerNotFoundError) = err);

    shutdown.trigger();
}

#[tokio::test]
#[allow(clippy::similar_names)]
async fn dial_success() {
    static TEST_PROTO: ProtocolId = ProtocolId::from_static(b"/test/valid");
    let shutdown = Shutdown::new();

    let node_identity1 = build_node_identity(PeerFeatures::empty());
    let node_identity2 = build_node_identity(PeerFeatures::empty());

    let (proto_tx1, _) = mpsc::channel(1);
    let (proto_tx2, mut proto_rx2) = mpsc::channel(1);

    // Setup connection manager 1
    let peer_manager1 = build_peer_manager();

    let mut protocols = Protocols::new();
    protocols.add([TEST_PROTO.clone()], &proto_tx1);
    let mut conn_man1 = build_connection_manager(
        {
            let mut config = TestNodeConfig {
                node_identity: node_identity1.clone(),
                ..Default::default()
            };
            config.connection_manager_config.network_info.user_agent = "node1".to_string();
            config
        },
        MemoryTransport,
        peer_manager1.clone(),
        protocols,
        shutdown.to_signal(),
    );

    conn_man1.wait_until_listening().await.unwrap();

    let peer_manager2 = build_peer_manager();
    let mut protocols = Protocols::new();
    protocols.add([TEST_PROTO.clone()], &proto_tx2);
    let mut conn_man2 = build_connection_manager(
        {
            let mut config = TestNodeConfig {
                node_identity: node_identity2.clone(),
                ..Default::default()
            };
            config.connection_manager_config.network_info.user_agent = "node2".to_string();
            config
        },
        MemoryTransport,
        peer_manager2.clone(),
        protocols,
        shutdown.to_signal(),
    );
    let mut subscription2 = conn_man2.get_event_subscription();
    let listener_info = conn_man2.wait_until_listening().await.unwrap();
    let public_address2 = listener_info.bind_address().clone();

    peer_manager1
        .add_peer(Peer::new(
            node_identity2.public_key().clone(),
            node_identity2.node_id().clone(),
            MultiaddressesWithStats::from_addresses_with_source(vec![public_address2], &PeerAddressSource::Config),
            PeerFlags::empty(),
            PeerFeatures::COMMUNICATION_CLIENT,
            Default::default(),
            Default::default(),
        ))
        .await
        .unwrap();

    let mut conn_out = conn_man1.dial_peer(node_identity2.node_id().clone()).await.unwrap();
    assert_eq!(conn_out.peer_node_id(), node_identity2.node_id());
    let peer2 = peer_manager1
        .find_by_node_id(conn_out.peer_node_id())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(peer2.supported_protocols, [&TEST_PROTO]);
    assert_eq!(peer2.user_agent, "node2");

    let event = subscription2.recv().await.unwrap();
    unpack_enum!(ConnectionManagerEvent::PeerConnected(conn_in) = &*event);
    assert_eq!(conn_in.peer_node_id(), node_identity1.node_id());

    let peer1 = peer_manager2
        .find_by_node_id(node_identity1.node_id())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(peer1.supported_protocols(), [&TEST_PROTO]);
    assert_eq!(peer1.user_agent, "node1");

    let err = conn_out
        .open_substream(&ProtocolId::from_static(b"/tari/invalid"))
        .await
        .unwrap_err();
    unpack_enum!(PeerConnectionError::ProtocolError(_err) = err);

    let mut substream_out = conn_out.open_substream(&TEST_PROTO).await.unwrap();
    assert_eq!(substream_out.protocol, TEST_PROTO);

    const MSG: &[u8] = b"Welease Woger!";
    substream_out.stream.write_all(MSG).await.unwrap();

    let protocol_in = proto_rx2.recv().await.unwrap();
    assert_eq!(protocol_in.protocol, &TEST_PROTO);
    unpack_enum!(ProtocolEvent::NewInboundSubstream(node_id, substream_in) = protocol_in.event);
    assert_eq!(&node_id, node_identity1.node_id());

    let mut buf = [0u8; MSG.len()];
    substream_in.read_exact(&mut buf).await.unwrap();
    assert_eq!(buf, MSG);
}

#[tokio::test]
#[allow(clippy::similar_names)]
async fn dial_success_aux_tcp_listener() {
    static TEST_PROTO: ProtocolId = ProtocolId::from_static(b"/test/valid");
    let shutdown = Shutdown::new();

    let node_identity1 = build_node_identity(PeerFeatures::empty());
    let node_identity2 = build_node_identity(PeerFeatures::empty());

    let (proto_tx1, mut proto_rx1) = mpsc::channel(1);
    let (proto_tx2, _) = mpsc::channel(1);

    // Setup connection manager 1
    let peer_manager1 = build_peer_manager();

    let mut protocols = Protocols::new();
    protocols.add([TEST_PROTO.clone()], &proto_tx1);
    let mut conn_man1 = build_connection_manager(
        {
            let mut config = TestNodeConfig {
                node_identity: node_identity1.clone(),
                ..Default::default()
            };
            config.connection_manager_config.auxiliary_tcp_listener_address =
                Some("/ip4/127.0.0.1/tcp/0".parse().unwrap());
            config.connection_manager_config.network_info.user_agent = "node1".to_string();
            config
        },
        MemoryTransport,
        peer_manager1.clone(),
        protocols,
        shutdown.to_signal(),
    );
    // This is required for the test to pass. Because we do not have a Connectivity actor to receive the event and hold
    // onto the PeerConnection handle, the connection would drop in this test.
    let _event_sub1 = conn_man1.get_event_subscription();

    let tcp_listener_addr = conn_man1
        .wait_until_listening()
        .await
        .unwrap()
        .auxiliary_bind_address()
        .unwrap()
        .clone();

    let peer_manager2 = build_peer_manager();
    peer_manager2
        .add_peer(Peer::new(
            node_identity1.public_key().clone(),
            node_identity1.node_id().clone(),
            MultiaddressesWithStats::from_addresses_with_source(vec![tcp_listener_addr], &PeerAddressSource::Config),
            PeerFlags::empty(),
            PeerFeatures::COMMUNICATION_CLIENT,
            Default::default(),
            Default::default(),
        ))
        .await
        .unwrap();
    let mut protocols = Protocols::new();
    protocols.add([TEST_PROTO.clone()], &proto_tx2);
    let mut conn_man2 = build_connection_manager(
        {
            let mut config = TestNodeConfig {
                node_identity: node_identity2.clone(),
                ..Default::default()
            };
            config.connection_manager_config.listener_address = "/ip4/127.0.0.1/tcp/0".parse().unwrap();
            config.connection_manager_config.network_info.user_agent = "node2".to_string();
            config
        },
        // Node 2 needs to use the tcp transport to connect to node1's tcp socket
        TcpTransport::new(),
        peer_manager2.clone(),
        protocols,
        shutdown.to_signal(),
    );
    conn_man2.wait_until_listening().await.unwrap();

    let mut connection = conn_man2.dial_peer(node_identity1.node_id().clone()).await.unwrap();
    assert_eq!(connection.peer_node_id(), node_identity1.node_id());

    let mut substream_out = connection.open_substream(&TEST_PROTO).await.unwrap();

    const MSG: &[u8] = b"Welease Woger!";
    substream_out.stream.write_all(MSG).await.unwrap();

    let protocol_in = proto_rx1.recv().await.unwrap();
    assert_eq!(protocol_in.protocol, &TEST_PROTO);
    unpack_enum!(ProtocolEvent::NewInboundSubstream(node_id, substream_in) = protocol_in.event);
    assert_eq!(&node_id, node_identity2.node_id());

    let mut buf = [0u8; MSG.len()];
    substream_in.read_exact(&mut buf).await.unwrap();
    assert_eq!(buf, MSG);
}

#[tokio::test]
async fn simultaneous_dial_events() {
    let mut shutdown = Shutdown::new();

    let node_identities = ordered_node_identities(2, Default::default());

    // Setup connection manager 1
    let peer_manager1 = build_peer_manager();
    let mut conn_man1 = build_connection_manager(
        TestNodeConfig {
            node_identity: node_identities[0].clone(),
            ..Default::default()
        },
        MemoryTransport,
        peer_manager1.clone(),
        Protocols::new(),
        shutdown.to_signal(),
    );

    let mut subscription1 = conn_man1.get_event_subscription();
    let listener_info = conn_man1.wait_until_listening().await.unwrap();
    let public_address1 = listener_info.bind_address().clone();

    let peer_manager2 = build_peer_manager();
    let mut conn_man2 = build_connection_manager(
        TestNodeConfig {
            node_identity: node_identities[1].clone(),
            ..Default::default()
        },
        MemoryTransport,
        peer_manager2.clone(),
        Protocols::new(),
        shutdown.to_signal(),
    );
    let mut subscription2 = conn_man2.get_event_subscription();
    let listener_info = conn_man2.wait_until_listening().await.unwrap();
    let public_address2 = listener_info.bind_address().clone();

    peer_manager1
        .add_peer(Peer::new(
            node_identities[1].public_key().clone(),
            node_identities[1].node_id().clone(),
            MultiaddressesWithStats::from_addresses_with_source(vec![public_address2], &PeerAddressSource::Config),
            PeerFlags::empty(),
            PeerFeatures::COMMUNICATION_CLIENT,
            Default::default(),
            Default::default(),
        ))
        .await
        .unwrap();

    peer_manager2
        .add_peer(Peer::new(
            node_identities[0].public_key().clone(),
            node_identities[0].node_id().clone(),
            MultiaddressesWithStats::from_addresses_with_source(vec![public_address1], &PeerAddressSource::Config),
            PeerFlags::empty(),
            PeerFeatures::COMMUNICATION_CLIENT,
            Default::default(),
            Default::default(),
        ))
        .await
        .unwrap();

    // Dial at the same time
    let (result1, result2) = future::join(
        conn_man1.dial_peer(node_identities[1].node_id().clone()),
        conn_man2.dial_peer(node_identities[0].node_id().clone()),
    )
    .await;

    // Either dial could fail (due to being cancelled/rejected by tie breaking) but never both
    match (result1, result2) {
        (Ok(_), Ok(_)) => {},
        (Err(_), Ok(_)) => {},
        (Ok(_), Err(_)) => {},
        _ => panic!("unexpected simultaneous dial result"),
    }

    let event = subscription2.recv().await.unwrap();
    assert!(count_string_occurrences(&[event], &["PeerConnected", "PeerInboundConnectFailed"]) >= 1);

    shutdown.trigger();
    drop(conn_man1);
    drop(conn_man2);

    let _events1 = collect_try_recv!(subscription1, timeout = Duration::from_secs(5));
    let _events2 = collect_try_recv!(subscription2, timeout = Duration::from_secs(5));
}

#[tokio::test]
async fn dial_cancelled() {
    let mut shutdown = Shutdown::new();

    let node_identity1 = build_node_identity(PeerFeatures::empty());
    let node_identity2 = build_node_identity(PeerFeatures::empty());

    // Setup connection manager 1
    let peer_manager1 = build_peer_manager();

    let mut conn_man1 = build_connection_manager(
        {
            let mut config = TestNodeConfig {
                node_identity: node_identity1.clone(),
                dial_backoff_duration: Duration::from_secs(100),
                ..Default::default()
            };
            config.connection_manager_config.network_info.user_agent = "node1".to_string();
            // To ensure that dial takes a long time so that we can test cancelling it
            config.connection_manager_config.max_dial_attempts = 100;
            config
        },
        MemoryTransport,
        peer_manager1.clone(),
        Default::default(),
        shutdown.to_signal(),
    );

    conn_man1.wait_until_listening().await.unwrap();

    let mut subscription1 = conn_man1.get_event_subscription();

    peer_manager1.add_peer(node_identity2.to_peer()).await.unwrap();

    let (ready_tx, ready_rx) = oneshot::channel();
    let dial_result = tokio::spawn({
        let mut cm = conn_man1.clone();
        let node_id = node_identity2.node_id().clone();
        async move {
            ready_tx.send(()).unwrap();
            cm.dial_peer(node_id).await
        }
    });

    ready_rx.await.unwrap();
    conn_man1.cancel_dial(node_identity2.node_id().clone()).await.unwrap();
    let err = dial_result.await.unwrap().unwrap_err();
    unpack_enum!(ConnectionManagerError::DialCancelled = err);

    shutdown.trigger();
    drop(conn_man1);

    let events1 = collect_try_recv!(subscription1, timeout = Duration::from_secs(5));

    assert_eq!(events1.len(), 1);
    unpack_enum!(ConnectionManagerEvent::PeerConnectFailed(node_id, err) = &*events1[0]);
    assert_eq!(node_id, node_identity2.node_id());
    unpack_enum!(ConnectionManagerError::DialCancelled = err);
}
