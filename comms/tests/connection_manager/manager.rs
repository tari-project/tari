//  Copyright 2019 The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use crate::support::{
    factories::{self, TestFactory},
    helpers::ConnectionMessageCounter,
};
use std::{sync::Arc, thread, time::Duration};
use tari_comms::{
    connection::{types::Linger, InprocAddress, ZmqContext},
    connection_manager::{ConnectionManager, PeerConnectionConfig},
    control_service::{ControlService, ControlServiceConfig},
    peer_manager::{Peer, PeerManager},
    types::{CommsDataStore, CommsPublicKey},
};

fn make_peer_connection_config(consumer_address: InprocAddress) -> PeerConnectionConfig {
    PeerConnectionConfig {
        control_service_establish_timeout: Duration::from_millis(2000),
        peer_connection_establish_timeout: Duration::from_secs(5),
        max_message_size: 1024,
        host: "127.0.0.1".parse().unwrap(),
        max_connect_retries: 3,
        message_sink_address: consumer_address,
        socks_proxy_address: None,
    }
}

fn make_peer_manager(peers: Vec<Peer<CommsPublicKey>>) -> Arc<PeerManager<CommsPublicKey, CommsDataStore>> {
    Arc::new(factories::peer_manager::create().with_peers(peers).build().unwrap())
}

#[test]
#[allow(non_snake_case)]
fn establish_peer_connection_by_peer() {
    let _ = simple_logger::init();
    let context = ZmqContext::new();

    let node_identity = Arc::new(factories::node_identity::create::<CommsPublicKey>().build().unwrap());

    //---------------------------------- Node B Setup --------------------------------------------//

    let node_B_consumer_address = InprocAddress::random();
    let node_B_control_port_address = factories::net_address::create().build().unwrap();

    let node_B_msg_counter = ConnectionMessageCounter::new(&context);
    node_B_msg_counter.start(node_B_consumer_address.clone());

    let node_B_peer = factories::peer::create()
        .with_net_addresses(vec![node_B_control_port_address.clone()])
        // Set node B's secret key to be the same as node A's so that we can generate the same shared secret
        // TODO: we'll need a way to generate separate node identities for two nodes
        .with_public_key(node_identity.identity.public_key.clone())
        .build()
        .unwrap();

    // Node B knows no peers
    let node_B_peer_manager = make_peer_manager(vec![]);
    let node_B_connection_manager = Arc::new(
        factories::connection_manager::create()
            .with_context(context.clone())
            .with_node_identity(node_identity.clone())
            .with_peer_manager(node_B_peer_manager)
            .with_peer_connection_config(make_peer_connection_config(node_B_consumer_address.clone()))
            .build()
            .unwrap(),
    );

    // Start node B's control service
    let node_B_control_service = ControlService::new(context.clone(), node_identity.clone(), ControlServiceConfig {
        socks_proxy_address: None,
        listener_address: node_B_control_port_address,
        accept_message_type: 123,
    })
    .serve(node_B_connection_manager)
    .unwrap();

    //---------------------------------- Node A setup --------------------------------------------//

    let node_A_consumer_address = InprocAddress::random();

    // Add node B to node A's peer manager
    let node_A_peer_manager = make_peer_manager(vec![node_B_peer.clone()]);
    let node_A_connection_manager = Arc::new(ConnectionManager::new(
        context.clone(),
        node_identity.clone(),
        node_A_peer_manager,
        make_peer_connection_config(node_A_consumer_address),
    ));

    //------------------------------ Negotiate connection to node B -----------------------------------//

    let node_B_peer_copy = node_B_peer.clone();
    let node_A_connection_manager_cloned = node_A_connection_manager.clone();
    let handle1 = thread::spawn(move || -> Result<(), String> {
        let to_node_B_conn = node_A_connection_manager_cloned
            .establish_connection_to_peer(&node_B_peer)
            .map_err(|err| format!("{:?}", err))?;
        to_node_B_conn.set_linger(Linger::Indefinitely).unwrap();
        to_node_B_conn
            .send(vec!["THREAD1".as_bytes().to_vec()])
            .map_err(|err| format!("{:?}", err))?;
        Ok(())
    });

    let node_A_connection_manager_cloned = node_A_connection_manager.clone();
    let handle2 = thread::spawn(move || -> Result<(), String> {
        let to_node_B_conn = node_A_connection_manager_cloned
            .establish_connection_to_peer(&node_B_peer_copy)
            .map_err(|err| format!("{:?}", err))?;
        to_node_B_conn.set_linger(Linger::Indefinitely).unwrap();
        to_node_B_conn
            .send(vec!["THREAD2".as_bytes().to_vec()])
            .map_err(|err| format!("{:?}", err))?;
        Ok(())
    });

    handle1.join().unwrap().unwrap();
    handle2.join().unwrap().unwrap();

    node_B_control_service.shutdown().unwrap();
    node_B_control_service.handle.join().unwrap().unwrap();

    assert_eq!(node_A_connection_manager.get_active_connection_count(), 1);
    node_B_msg_counter.assert_count(2, 1000);

    match Arc::try_unwrap(node_A_connection_manager) {
        Ok(manager) => manager.shutdown().into_iter().map(|r| r.unwrap()).collect::<Vec<()>>(),
        Err(_) => panic!("Unable to unwrap connection manager from Arc"),
    };
}
