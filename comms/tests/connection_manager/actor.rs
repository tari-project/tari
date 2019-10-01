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

use crate::support::{
    factories::{self, TestFactory},
    helpers::database::{clean_up_datastore, init_datastore},
};
use futures::channel::mpsc;
use std::{sync::Arc, time::Duration};
use tari_comms::{
    connection::ZmqContext,
    connection_manager::{create_connection_manager_actor, ConnectionManager, ConnectionManagerError},
    control_service::{messages::RejectReason, ControlService, ControlServiceConfig},
    message::FrameSet,
    peer_manager::{NodeIdentity, Peer, PeerManager},
};
use tari_storage::LMDBWrapper;
use tari_test_utils::random;
use tokio::runtime::Runtime;

#[derive(Clone)]
pub struct CommsTestNode {
    peer_manager: Arc<PeerManager>,
    node_identity: Arc<NodeIdentity>,
    connection_manager: Arc<ConnectionManager>,
    peer: Peer,
}

fn with_alice_and_bob(cb: impl FnOnce(CommsTestNode, CommsTestNode)) {
    let context = ZmqContext::new();

    let alice_identity = Arc::new(factories::node_identity::create().build().unwrap());

    //---------------------------------- Node B Setup --------------------------------------------//

    let bob_control_port_address = factories::net_address::create().build().unwrap();
    let bob_identity = Arc::new(
        factories::node_identity::create()
            .with_control_service_address(bob_control_port_address.clone())
            .build()
            .unwrap(),
    );

    let bob_peer = factories::peer::create()
        .with_net_addresses(vec![bob_control_port_address.clone()])
        .with_public_key(bob_identity.identity.public_key.clone())
        .with_node_id(bob_identity.identity.node_id.clone())
        .build()
        .unwrap();

    // Node B knows no peers
    let (consumer_tx_b, _consumer_rx_b): (mpsc::Sender<FrameSet>, _) = mpsc::channel(10);
    let bob_database_name = random::prefixed_string("connection_manager_actor", 5);
    let datastore = init_datastore(bob_database_name.as_str()).unwrap();
    let database = datastore.get_handle(bob_database_name.as_str()).unwrap();
    let database = LMDBWrapper::new(Arc::new(database));
    let bob_peer_manager = factories::peer_manager::create()
        .with_database(database)
        .build()
        .map(Arc::new)
        .unwrap();
    let bob_connection_manager = Arc::new(
        factories::connection_manager::create()
            .with_context(context.clone())
            .with_node_identity(Arc::clone(&bob_identity.clone()))
            .with_peer_manager(Arc::clone(&bob_peer_manager))
            .with_message_sink_sender(consumer_tx_b)
            .build()
            .unwrap(),
    );

    // Start node B's control service
    let bob_control_service = ControlService::new(context.clone(), bob_identity.clone(), ControlServiceConfig {
        socks_proxy_address: None,
        listener_address: bob_control_port_address,
        requested_connection_timeout: Duration::from_millis(5000),
    })
    .serve(Arc::clone(&bob_connection_manager))
    .unwrap();

    //---------------------------------- Node A setup --------------------------------------------//

    let (consumer_tx_a, _consumer_rx_a) = mpsc::channel(10);
    let alice_database_name = random::prefixed_string("connection_manager_actor", 5);
    let datastore = init_datastore(alice_database_name.as_str()).unwrap();
    let database = datastore.get_handle(alice_database_name.as_str()).unwrap();
    let database = LMDBWrapper::new(Arc::new(database));
    let alice_peer_manager = factories::peer_manager::create()
        .with_peers(vec![bob_peer.clone()])
        .with_database(database)
        .build()
        .map(Arc::new)
        .unwrap();
    let alice_connection_manager = Arc::new(
        factories::connection_manager::create()
            .with_context(context.clone())
            .with_node_identity(Arc::clone(&alice_identity))
            .with_peer_manager(Arc::clone(&alice_peer_manager))
            .with_message_sink_sender(consumer_tx_a)
            .build()
            .unwrap(),
    );

    // Start node A's control service
    let alice_control_port_address = factories::net_address::create().build().unwrap();
    let alice_control_service = ControlService::new(context.clone(), alice_identity.clone(), ControlServiceConfig {
        socks_proxy_address: None,
        listener_address: alice_control_port_address.clone(),
        requested_connection_timeout: Duration::from_millis(5000),
    })
    .serve(Arc::clone(&alice_connection_manager))
    .unwrap();

    let alice_peer = factories::peer::create()
        .with_net_addresses(vec![alice_control_port_address])
        .with_public_key(alice_identity.identity.public_key.clone())
        .with_node_id(alice_identity.identity.node_id.clone())
        .build()
        .unwrap();

    let alice = CommsTestNode {
        peer_manager: alice_peer_manager,
        connection_manager: alice_connection_manager,
        node_identity: alice_identity,
        peer: alice_peer,
    };
    let bob = CommsTestNode {
        peer_manager: bob_peer_manager,
        connection_manager: bob_connection_manager,
        node_identity: bob_identity,
        peer: bob_peer,
    };

    cb(alice.clone(), bob.clone());

    alice_control_service.shutdown().unwrap();
    alice_control_service.timeout_join(Duration::from_millis(1000)).unwrap();

    bob_control_service.shutdown().unwrap();
    bob_control_service.timeout_join(Duration::from_millis(1000)).unwrap();

    match Arc::try_unwrap(alice.connection_manager) {
        Ok(manager) => manager.shutdown().into_iter().map(|r| r.unwrap()).collect::<Vec<()>>(),
        Err(_) => panic!("Unable to unwrap connection manager from Arc"),
    };
    match Arc::try_unwrap(bob.connection_manager) {
        Ok(manager) => manager.shutdown().into_iter().map(|r| r.unwrap()).collect::<Vec<()>>(),
        Err(_) => panic!("Unable to unwrap connection manager from Arc"),
    };

    clean_up_datastore(alice_database_name.as_str());
    clean_up_datastore(bob_database_name.as_str());
}

#[test]
fn establish_connection_simple() {
    with_alice_and_bob(|alice, bob| {
        let rt = Runtime::new().unwrap();
        let (mut requester, service) = create_connection_manager_actor(1, alice.connection_manager);

        rt.spawn(service.start());

        let conn = rt
            .block_on(requester.dial_node(bob.node_identity.identity.node_id.clone()))
            .unwrap();
        assert!(conn.is_active());
    })
}

#[test]
fn establish_connection_simultaneous_connect() {
    with_alice_and_bob(|alice, bob| {
        let rt = Runtime::new().unwrap();
        //        let mut pool = ThreadPool::new().unwrap();
        let (requester_alice, service) = create_connection_manager_actor(1, Arc::clone(&alice.connection_manager));
        rt.spawn(service.start());

        let (requester_bob, service) = create_connection_manager_actor(1, Arc::clone(&bob.connection_manager));
        bob.peer_manager.add_peer(alice.peer.clone()).unwrap();
        rt.spawn(service.start());

        let alice_node_id = alice.node_identity.identity.node_id.clone();
        let bob_node_id = bob.node_identity.identity.node_id.clone();

        let mut attempt_count = 0;
        loop {
            let mut requester_alice_inner = requester_alice.clone();
            let mut requester_bob_inner = requester_bob.clone();
            let (alice_result, bob_result) = rt.block_on(async {
                futures::join!(
                    requester_alice_inner.dial_node(bob_node_id.clone()),
                    requester_bob_inner.dial_node(alice_node_id.clone())
                )
            });

            match (alice_result, bob_result) {
                // Alice rejected Bob's connection attempt
                (Ok(conn), Err(ConnectionManagerError::ConnectionRejected(reason))) => {
                    assert_eq!(reason, RejectReason::CollisionDetected);
                    assert!(conn.is_active());
                    break;
                },
                // Bob rejected Alice's connection attempt
                (Err(ConnectionManagerError::ConnectionRejected(reason)), Ok(conn)) => {
                    assert_eq!(reason, RejectReason::CollisionDetected);
                    assert!(conn.is_active());
                    break;
                },
                (Ok(_), Ok(_)) if attempt_count < 10 => {
                    alice.connection_manager.disconnect_peer(&bob.peer).unwrap();
                    bob.connection_manager.disconnect_peer(&alice.peer).unwrap();
                    attempt_count += 1;
                },
                _ => panic!("Unable to trigger simultaneous connection conflict after 5 attempts"),
            }
        }
    })
}
