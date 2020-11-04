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
    connection_manager::{
        error::ConnectionManagerError,
        manager::ConnectionManagerEvent,
        ConnectionManager,
        ConnectionManagerRequester,
        PeerConnectionError,
    },
    noise::NoiseConfig,
    peer_manager::{NodeId, Peer, PeerFeatures, PeerFlags, PeerManagerError},
    protocol::{ProtocolEvent, ProtocolId, Protocols, IDENTITY_PROTOCOL},
    runtime,
    runtime::task,
    test_utils::{
        count_string_occurrences,
        node_identity::{build_node_identity, ordered_node_identities},
        test_node::{build_connection_manager, build_peer_manager, TestNodeConfig},
    },
    transports::MemoryTransport,
};
use futures::{
    channel::{mpsc, oneshot},
    future,
    AsyncReadExt,
    AsyncWriteExt,
    StreamExt,
};
use std::time::Duration;
use tari_shutdown::Shutdown;
use tari_test_utils::{collect_stream, unpack_enum};
use tokio::{runtime::Handle, sync::broadcast};

#[runtime::test_basic]
async fn connect_to_nonexistent_peer() {
    let rt_handle = Handle::current();
    let node_identity = build_node_identity(PeerFeatures::empty());
    let noise_config = NoiseConfig::new(node_identity.clone());
    let (request_tx, request_rx) = mpsc::channel(1);
    let (event_tx, _) = broadcast::channel(1);
    let mut requester = ConnectionManagerRequester::new(request_tx, event_tx.clone());
    let mut shutdown = Shutdown::new();

    let peer_manager = build_peer_manager();

    let connection_manager = ConnectionManager::new(
        Default::default(),
        MemoryTransport,
        noise_config,
        ConstantBackoff::new(Duration::from_secs(1)),
        request_rx,
        node_identity,
        peer_manager,
        event_tx,
        shutdown.to_signal(),
    );

    rt_handle.spawn(connection_manager.run());

    let err = requester.dial_peer(NodeId::default()).await.unwrap_err();
    unpack_enum!(ConnectionManagerError::PeerManagerError(err) = err);
    unpack_enum!(PeerManagerError::PeerNotFoundError = err);

    shutdown.trigger().unwrap();
}

#[runtime::test_basic]
async fn dial_success() {
    const TEST_PROTO: ProtocolId = ProtocolId::from_static(b"/test/valid");
    let shutdown = Shutdown::new();

    let node_identity1 = build_node_identity(PeerFeatures::empty());
    let node_identity2 = build_node_identity(PeerFeatures::empty());

    let (proto_tx1, _) = mpsc::channel(1);
    let (proto_tx2, mut proto_rx2) = mpsc::channel(1);

    // Setup connection manager 1
    let peer_manager1 = build_peer_manager();

    let mut protocols = Protocols::new();
    protocols.add([TEST_PROTO], proto_tx1);
    let mut conn_man1 = build_connection_manager(
        {
            let mut config = TestNodeConfig {
                node_identity: node_identity1.clone(),
                ..Default::default()
            };
            config.connection_manager_config.user_agent = "node1".to_string();
            config
        },
        peer_manager1.clone(),
        protocols,
        shutdown.to_signal(),
    );

    conn_man1.wait_until_listening().await.unwrap();

    let peer_manager2 = build_peer_manager();
    let mut protocols = Protocols::new();
    protocols.add([TEST_PROTO], proto_tx2);
    let mut conn_man2 = build_connection_manager(
        {
            let mut config = TestNodeConfig {
                node_identity: node_identity2.clone(),
                ..Default::default()
            };
            config.connection_manager_config.user_agent = "node2".to_string();
            config
        },
        peer_manager2.clone(),
        protocols,
        shutdown.to_signal(),
    );
    let mut subscription2 = conn_man2.get_event_subscription();
    let public_address2 = conn_man2.wait_until_listening().await.unwrap();

    peer_manager1
        .add_peer(Peer::new(
            node_identity2.public_key().clone(),
            node_identity2.node_id().clone(),
            vec![public_address2].into(),
            PeerFlags::empty(),
            PeerFeatures::COMMUNICATION_CLIENT,
            &[],
            Default::default(),
        ))
        .await
        .unwrap();

    // Dial at the same time
    let mut conn_out = conn_man1.dial_peer(node_identity2.node_id().clone()).await.unwrap();
    assert_eq!(conn_out.peer_node_id(), node_identity2.node_id());
    let peer2 = peer_manager1.find_by_node_id(conn_out.peer_node_id()).await.unwrap();
    assert_eq!(peer2.supported_protocols, [&IDENTITY_PROTOCOL, &TEST_PROTO]);
    assert_eq!(peer2.user_agent, "node2");

    let event = subscription2.next().await.unwrap().unwrap();
    unpack_enum!(ConnectionManagerEvent::Listening(_addr) = &*event);

    let event = subscription2.next().await.unwrap().unwrap();
    unpack_enum!(ConnectionManagerEvent::PeerConnected(conn_in) = &*event);
    assert_eq!(conn_in.peer_node_id(), node_identity1.node_id());

    let peer1 = peer_manager2.find_by_node_id(node_identity1.node_id()).await.unwrap();
    assert_eq!(peer1.supported_protocols(), [&IDENTITY_PROTOCOL, &TEST_PROTO]);
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

    let protocol_in = proto_rx2.next().await.unwrap();
    assert_eq!(protocol_in.protocol, &TEST_PROTO);
    unpack_enum!(ProtocolEvent::NewInboundSubstream(node_id, substream_in) = protocol_in.event);
    assert_eq!(&node_id, node_identity1.node_id());

    let mut buf = [0u8; MSG.len()];
    substream_in.read_exact(&mut buf).await.unwrap();
    assert_eq!(buf, MSG);
}

#[runtime::test_basic]
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
        peer_manager1.clone(),
        Protocols::new(),
        shutdown.to_signal(),
    );

    let mut subscription1 = conn_man1.get_event_subscription();
    let public_address1 = conn_man1.wait_until_listening().await.unwrap();

    let peer_manager2 = build_peer_manager();
    let mut conn_man2 = build_connection_manager(
        TestNodeConfig {
            node_identity: node_identities[1].clone(),
            ..Default::default()
        },
        peer_manager2.clone(),
        Protocols::new(),
        shutdown.to_signal(),
    );
    let mut subscription2 = conn_man2.get_event_subscription();
    let public_address2 = conn_man2.wait_until_listening().await.unwrap();

    peer_manager1
        .add_peer(Peer::new(
            node_identities[1].public_key().clone(),
            node_identities[1].node_id().clone(),
            vec![public_address2].into(),
            PeerFlags::empty(),
            PeerFeatures::COMMUNICATION_CLIENT,
            &[],
            Default::default(),
        ))
        .await
        .unwrap();

    peer_manager2
        .add_peer(Peer::new(
            node_identities[0].public_key().clone(),
            node_identities[0].node_id().clone(),
            vec![public_address1].into(),
            PeerFlags::empty(),
            PeerFeatures::COMMUNICATION_CLIENT,
            &[],
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

    // Wait for listening and peer connected events
    let event = subscription2.next().await.unwrap().unwrap();
    unpack_enum!(ConnectionManagerEvent::Listening(_addr) = &*event);

    let event = subscription2.next().await.unwrap().unwrap();
    assert!(count_string_occurrences(&[event], &["PeerConnected", "PeerInboundConnectFailed"]) >= 1);

    shutdown.trigger().unwrap();
    drop(conn_man1);
    drop(conn_man2);

    let _events1 = collect_stream!(subscription1, timeout = Duration::from_secs(5))
        .into_iter()
        .map(Result::unwrap)
        .collect::<Vec<_>>();

    let _events2 = collect_stream!(subscription2, timeout = Duration::from_secs(5))
        .into_iter()
        .map(Result::unwrap)
        .collect::<Vec<_>>();

    // TODO: Investigate why two PeerDisconnected events are sometimes received
    // assert!(count_string_occurrences(&events1, &["PeerDisconnected"]) >= 1);
    // assert!(count_string_occurrences(&events2, &["PeerDisconnected"]) >= 1);
}

#[tokio_macros::test_basic]
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
            config.connection_manager_config.user_agent = "node1".to_string();
            config
        },
        peer_manager1.clone(),
        Default::default(),
        shutdown.to_signal(),
    );

    conn_man1.wait_until_listening().await.unwrap();

    let mut subscription1 = conn_man1.get_event_subscription();

    peer_manager1.add_peer(node_identity2.to_peer()).await.unwrap();

    let (ready_tx, ready_rx) = oneshot::channel();
    let dial_result = task::spawn({
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

    shutdown.trigger().unwrap();
    drop(conn_man1);

    let events1 = collect_stream!(subscription1, timeout = Duration::from_secs(5))
        .into_iter()
        .map(Result::unwrap)
        .collect::<Vec<_>>();

    assert_eq!(events1.len(), 1);
    unpack_enum!(ConnectionManagerEvent::PeerConnectFailed(node_id, err) = &*events1[0]);
    assert_eq!(&**node_id, node_identity2.node_id());
    unpack_enum!(ConnectionManagerError::DialCancelled = err);
}
