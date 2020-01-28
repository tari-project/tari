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

use futures::channel::mpsc;
use rand::rngs::OsRng;
use std::{sync::Arc, time::Duration};
use tari_comms::{
    builder::CommsNode,
    connection_manager::PeerConnectionConfig,
    control_service::ControlServiceConfig,
    multiaddr::Multiaddr,
    peer_manager::{peer_storage::PeerStorage, NodeIdentity, Peer, PeerFeatures},
    types::CommsDatabase,
    CommsBuilder,
};
use tari_comms_dht::{envelope::NodeDestination, inbound::DecryptedDhtMessage, Dht, DhtBuilder};
use tari_comms_middleware::{pipeline::ServicePipeline, sink::SinkMiddleware};
use tari_storage::{lmdb_store::LMDBBuilder, LMDBWrapper};
use tari_test_utils::{async_assert_eventually, paths::create_temporary_data_path, random, runtime};
use tower::ServiceBuilder;

fn new_node_identity(control_service_address: Multiaddr) -> NodeIdentity {
    NodeIdentity::random(&mut OsRng, control_service_address, PeerFeatures::COMMUNICATION_NODE).unwrap()
}

fn create_peer_storage(peers: Vec<Peer>) -> CommsDatabase {
    let database_name = random::string(8);
    let datastore = LMDBBuilder::new()
        .set_path(create_temporary_data_path().to_str().unwrap())
        .set_environment_size(10)
        .set_max_number_of_databases(1)
        .add_database(&database_name, lmdb_zero::db::CREATE)
        .build()
        .unwrap();

    let peer_database = datastore.get_handle(&database_name).unwrap();
    let peer_database = LMDBWrapper::new(Arc::new(peer_database));
    let mut storage = PeerStorage::new_indexed(peer_database).unwrap();
    for peer in peers {
        storage.add_peer(peer).unwrap();
    }

    storage.into()
}

fn setup_comms_dht(
    executor: tokio::runtime::Handle,
    node_identity: NodeIdentity,
    storage: CommsDatabase,
    inbound_sink: mpsc::Sender<DecryptedDhtMessage>,
) -> (CommsNode, Dht)
{
    // Create inbound and outbound channels
    let (inbound_tx, inbound_rx) = mpsc::channel(10);
    let (outbound_tx, outbound_rx) = mpsc::channel(10);

    let comms = CommsBuilder::new(executor.clone())
        .with_peer_storage(storage)
        .with_inbound_sink(inbound_tx)
        .with_outbound_stream(outbound_rx)
        .configure_control_service(ControlServiceConfig {
            listening_address: node_identity.public_address(),
            ..Default::default()
        })
        .configure_peer_connections(PeerConnectionConfig {
            listening_address: "/ip4/127.0.0.1/tcp/0".parse().unwrap(),
            ..Default::default()
        })
        .with_node_identity(Arc::new(node_identity))
        .build()
        .unwrap()
        .start()
        .unwrap();

    // Create a channel for outbound requests
    let mut dht = DhtBuilder::from_comms(&comms)
        .local_test()
        .with_discovery_timeout(Duration::from_secs(60))
        .finish();

    //---------------------------------- Inbound Pipeline --------------------------------------------//

    // Connect inbound comms messages to the inbound pipeline and run it
    let inbound_pipeline = ServicePipeline::new(
        // Messages coming IN from comms to DHT
        inbound_rx,
        // Messages going OUT from DHT to connector (pubsub)
        ServiceBuilder::new()
            .layer(dht.inbound_middleware_layer())
            .service(SinkMiddleware::new(inbound_sink)),
    );
    inbound_pipeline.spawn_with(executor.clone());

    //---------------------------------- Outbound Pipeline --------------------------------------------//

    //    // Connect outbound message pipeline to comms, and run it
    let outbound_pipeline = ServicePipeline::new(
        // Requests coming IN from services to DHT
        dht.take_outbound_receiver().expect("take outbound receiver only once"),
        // Messages going OUT from DHT to comms
        ServiceBuilder::new()
            .layer(dht.outbound_middleware_layer())
            .service(SinkMiddleware::new(outbound_tx)),
    );
    outbound_pipeline.spawn_with(executor);

    (comms, dht)
}

#[test]
#[allow(non_snake_case)]
fn dht_join_propagation() {
    env_logger::init();
    runtime::test_async(|rt| {
        // Create 3 nodes where only Node B knows A and C, but A and C want to talk to each other
        let node_A_identity = new_node_identity("/ip4/127.0.0.1/tcp/11113".parse::<Multiaddr>().unwrap());
        let node_B_identity = new_node_identity("/ip4/127.0.0.1/tcp/11114".parse::<Multiaddr>().unwrap());
        let node_C_identity = new_node_identity("/ip4/127.0.0.1/tcp/11115".parse::<Multiaddr>().unwrap());

        // Node A knows about Node B
        let (tx, ims_rx_A) = mpsc::channel(1);
        let (node_A_comms, node_A_dht) = setup_comms_dht(
            rt.handle().clone(),
            node_A_identity.clone(),
            create_peer_storage(vec![node_B_identity.clone().into()]),
            tx,
        );
        // Node B knows about Node A and C
        let (tx, ims_rx_B) = mpsc::channel(1);
        let (node_B_comms, node_B_dht) = setup_comms_dht(
            rt.handle().clone(),
            node_B_identity.clone(),
            create_peer_storage(vec![node_A_identity.clone().into(), node_C_identity.clone().into()]),
            tx,
        );
        // Node C knows about Node B
        let (tx, ims_rx_C) = mpsc::channel(1);
        let (node_C_comms, node_C_dht) = setup_comms_dht(
            rt.handle().clone(),
            node_C_identity.clone(),
            create_peer_storage(vec![node_B_identity.clone().into()]),
            tx,
        );

        rt.spawn(async move {
            // Send a join request from Node A, through B to C. As all Nodes are in the same network region, once
            // Node C receives the join request from Node A, it will send a direct join request back
            // to A.
            node_A_dht.dht_requester().send_join().await.unwrap();

            let node_A_peer_manager = node_A_comms.peer_manager();
            let node_A_node_identity = node_A_comms.node_identity();
            let node_C_peer_manager = node_C_comms.peer_manager();
            let node_C_node_identity = node_C_comms.node_identity();

            // Check that Node A knows about Node C and vice versa
            async_assert_eventually!(
                node_A_peer_manager.exists(node_C_node_identity.public_key()),
                expect = true,
                max_attempts = 10,
                interval = Duration::from_millis(1000)
            );
            async_assert_eventually!(
                node_C_peer_manager.exists(node_A_node_identity.public_key()),
                expect = true,
                max_attempts = 10,
                interval = Duration::from_millis(500)
            );

            let node_C_peer = node_A_peer_manager
                .find_by_public_key(node_C_node_identity.public_key())
                .unwrap();
            assert_eq!(&node_C_peer.features, node_C_node_identity.features());

            // Make sure these variables only drop after the test is done
            drop(ims_rx_A);
            drop(ims_rx_B);
            drop(ims_rx_C);

            drop(node_A_dht);
            drop(node_B_dht);
            drop(node_C_dht);
            node_A_comms.shutdown().unwrap();
            node_B_comms.shutdown().unwrap();
            node_C_comms.shutdown().unwrap();
        });
    });
}

#[test]
#[allow(non_snake_case)]
fn dht_discover_propagation() {
    // Create 4 nodes where A knows B, B knows A and C, C knows B and D, and D knows C
    let node_A_identity = new_node_identity("/ip4/127.0.0.1/tcp/11116".parse().unwrap());
    let node_B_identity = new_node_identity("/ip4/127.0.0.1/tcp/11117".parse().unwrap());
    let node_C_identity = new_node_identity("/ip4/127.0.0.1/tcp/11118".parse().unwrap());
    let node_D_identity = new_node_identity("/ip4/127.0.0.1/tcp/11119".parse().unwrap());

    runtime::test_async(|rt| {
        // Node A knows about Node B
        let (tx, ims_rx_A) = mpsc::channel(1);
        let (node_A_comms, node_A_dht) = setup_comms_dht(
            rt.handle().clone(),
            node_A_identity.clone(),
            create_peer_storage(vec![node_B_identity.clone().into()]),
            tx,
        );
        // Node B knows about Node C
        let (tx, ims_rx_B) = mpsc::channel(1);
        let (node_B_comms, node_B_dht) = setup_comms_dht(
            rt.handle().clone(),
            node_B_identity.clone(),
            create_peer_storage(vec![node_C_identity.clone().into()]),
            tx,
        );
        // Node C knows about Node D
        let (tx, ims_rx_C) = mpsc::channel(1);
        let (node_C_comms, node_C_dht) = setup_comms_dht(
            rt.handle().clone(),
            node_C_identity.clone(),
            create_peer_storage(vec![node_D_identity.clone().into()]),
            tx,
        );
        // Node C knows no one
        let (tx, ims_rx_D) = mpsc::channel(1);
        let (node_D_comms, node_D_dht) = setup_comms_dht(
            rt.handle().clone(),
            node_D_identity.clone(),
            create_peer_storage(vec![]),
            tx,
        );

        rt.spawn(async move {
            // Send a discover request from Node A, through B and C, to D. Once Node D
            // receives the discover request from Node A, it should send a  discovery response
            // request back to A at which time this call will resolve (or timeout).
            node_A_dht
                .discovery_service_requester()
                .discover_peer(node_D_identity.public_key().clone(), None, NodeDestination::Unknown)
                .await
                .unwrap();

            let node_A_peer_manager = node_A_comms.peer_manager();
            let node_A_node_identity = node_A_comms.node_identity();
            let node_B_peer_manager = node_B_comms.peer_manager();
            let node_B_node_identity = node_B_comms.node_identity();
            let node_C_peer_manager = node_C_comms.peer_manager();
            let node_C_node_identity = node_C_comms.node_identity();
            let node_D_peer_manager = node_D_comms.peer_manager();
            let node_D_node_identity = node_D_comms.node_identity();

            // Check that all the nodes know about each other in the chain and the discovery worked
            assert!(node_A_peer_manager.exists(node_D_node_identity.public_key()));
            assert!(node_B_peer_manager.exists(node_A_node_identity.public_key()));
            assert!(node_C_peer_manager.exists(node_B_node_identity.public_key()));
            assert!(node_D_peer_manager.exists(node_C_node_identity.public_key()));
            assert!(node_D_peer_manager.exists(node_A_node_identity.public_key()));

            // Make sure these variables only drop after the test is done
            drop(ims_rx_A);
            drop(ims_rx_B);
            drop(ims_rx_C);
            drop(ims_rx_D);

            drop(node_A_dht);
            drop(node_B_dht);
            drop(node_C_dht);
            drop(node_D_dht);

            node_A_comms.shutdown().unwrap();
            node_B_comms.shutdown().unwrap();
            node_C_comms.shutdown().unwrap();
            node_D_comms.shutdown().unwrap();
        });
    });
}
