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
        next::{ConnectionManager, ConnectionManagerRequester},
    },
    noise::NoiseConfig,
    peer_manager::{NodeId, Peer, PeerFeatures, PeerFlags, PeerManagerError},
    protocol::ProtocolNotifier,
    test_utils::{
        node_identity::{build_node_identity, ordered_node_identities},
        test_node::{build_connection_manager, build_peer_manager, TestNodeConfig},
    },
    transports::MemoryTransport,
};
use futures::{channel::mpsc, future};
use std::{sync::Arc, time::Duration};
use tari_shutdown::Shutdown;
use tari_test_utils::{collect_stream, unpack_enum};
use tokio::{runtime::Handle, sync::broadcast};
use tokio_macros as r#async;

#[r#async::test_basic]
async fn connect_to_nonexistent_peer() {
    let rt_handle = Handle::current();
    let node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let noise_config = NoiseConfig::new(node_identity.clone());
    let (request_tx, request_rx) = mpsc::channel(1);
    let (event_tx, _) = broadcast::channel(1);
    let mut requester = ConnectionManagerRequester::new(request_tx, event_tx.clone());
    let mut shutdown = Shutdown::new();

    let peer_manager = build_peer_manager();

    let connection_manager = ConnectionManager::new(
        Default::default(),
        rt_handle.clone(),
        MemoryTransport,
        noise_config,
        Arc::new(ConstantBackoff::new(Duration::from_secs(1))),
        request_rx,
        node_identity,
        peer_manager.into(),
        ProtocolNotifier::new(),
        event_tx,
        shutdown.to_signal(),
    );

    rt_handle.spawn(connection_manager.run());

    let result = requester.dial_peer(NodeId::default()).await;
    unpack_enum!(Result::Err(err) = result);
    match err {
        ConnectionManagerError::PeerManagerError(PeerManagerError::PeerNotFoundError) => {},
        _ => panic!(
            "Unexpected error. Expected \
             `ConnectionManagerError::PeerManagerError(PeerManagerError::PeerNotFoundError)`"
        ),
    }

    shutdown.trigger().unwrap();
}

#[r#async::test_basic]
async fn simultaneous_dial_events() {
    let rt_handle = Handle::current();
    let mut shutdown = Shutdown::new();

    let node_identities = ordered_node_identities(2);

    // Setup connection manager 1
    let peer_manager1 = build_peer_manager();
    let mut conn_man1 = build_connection_manager(
        rt_handle.clone(),
        TestNodeConfig {
            node_identity: node_identities[0].clone(),
            ..Default::default()
        },
        peer_manager1.clone(),
        shutdown.to_signal(),
    );

    let subscription1 = conn_man1.subscribe_events();
    let public_address1 = conn_man1.wait_until_listening().await.unwrap();

    let peer_manager2 = build_peer_manager();
    let mut conn_man2 = build_connection_manager(
        rt_handle,
        TestNodeConfig {
            node_identity: node_identities[1].clone(),
            ..Default::default()
        },
        peer_manager2.clone(),
        shutdown.to_signal(),
    );
    let subscription2 = conn_man2.subscribe_events();
    let public_address2 = conn_man2.wait_until_listening().await.unwrap();

    peer_manager1
        .add_peer(Peer::new(
            node_identities[1].public_key().clone(),
            node_identities[1].node_id().clone(),
            vec![public_address2].into(),
            PeerFlags::empty(),
            PeerFeatures::COMMUNICATION_CLIENT,
        ))
        .unwrap();

    peer_manager2
        .add_peer(Peer::new(
            node_identities[0].public_key().clone(),
            node_identities[0].node_id().clone(),
            vec![public_address1].into(),
            PeerFlags::empty(),
            PeerFeatures::COMMUNICATION_CLIENT,
        ))
        .unwrap();

    // Dial at the same time
    let (result1, result2) = future::join(
        conn_man1.dial_peer(node_identities[1].node_id().clone()),
        conn_man2.dial_peer(node_identities[0].node_id().clone()),
    )
    .await;

    result1.unwrap();
    result2.unwrap();

    shutdown.trigger().unwrap();
    drop(conn_man1);
    drop(conn_man2);

    let events1 = collect_stream!(subscription1, timeout = Duration::from_secs(5))
        .into_iter()
        .map(Result::unwrap)
        .collect::<Vec<_>>();

    let events2 = collect_stream!(subscription2, timeout = Duration::from_secs(5))
        .into_iter()
        .map(Result::unwrap)
        .collect::<Vec<_>>();

    let count_disconnected_events = |events: Vec<Arc<ConnectionManagerEvent>>| {
        events
            .iter()
            .filter(|event| match &***event {
                ConnectionManagerEvent::PeerDisconnected(_) => true,
                _ => false,
            })
            .count()
    };

    // Check for only one disconnect event
    assert_eq!(count_disconnected_events(events1), 1);
    assert_eq!(count_disconnected_events(events2), 1);
}
