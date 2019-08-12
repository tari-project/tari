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

// NOTE: These tests use ports 11113 to 11122
use crate::support::random_string;
use rand::rngs::OsRng;
use std::{sync::Arc, thread, time::Duration};
use tari_comms::{
    builder::CommsServices,
    connection::NetAddress,
    connection_manager::PeerConnectionConfig,
    control_service::ControlServiceConfig,
    message::NodeDestination,
    peer_manager::{peer_storage::PeerStorage, NodeIdentity, Peer, PeerManager},
    types::CommsDatabase,
    CommsBuilder,
};
use tari_p2p::{
    dht_service::{DHTService, DHTServiceApi},
    services::{ServiceExecutor, ServiceRegistry},
    tari_message::TariMessageType,
};
use tari_storage::{key_val_store::lmdb_database::LMDBWrapper, lmdb_store::LMDBBuilder};
use tempdir::TempDir;

fn new_node_identity(control_service_address: NetAddress) -> NodeIdentity {
    NodeIdentity::random(&mut OsRng::new().unwrap(), control_service_address).unwrap()
}

fn create_peer_storage(tmpdir: &TempDir, database_name: &str, peers: Vec<Peer>) -> CommsDatabase {
    let datastore = LMDBBuilder::new()
        .set_path(tmpdir.path().to_str().unwrap())
        .set_environment_size(10)
        .set_max_number_of_databases(1)
        .add_database(database_name, lmdb_zero::db::CREATE)
        .build()
        .unwrap();

    let peer_database = datastore.get_handle(database_name).unwrap();
    let peer_database = LMDBWrapper::new(Arc::new(peer_database));
    let mut storage = PeerStorage::new(peer_database).unwrap();
    for peer in peers {
        storage.add_peer(peer).unwrap();
    }

    storage.into_datastore()
}

fn setup_dht_service(
    node_identity: NodeIdentity,
    peer_storage: CommsDatabase,
) -> (ServiceExecutor, Arc<DHTServiceApi>, Arc<CommsServices<TariMessageType>>)
{
    let control_service_address = node_identity.control_service_address().unwrap();
    let dht_service = DHTService::new();
    let dht_api = dht_service.get_api();

    let services = ServiceRegistry::new().register(dht_service);
    let comms = CommsBuilder::new()
        .with_routes(services.build_comms_routes())
        .with_node_identity(node_identity)
        .with_peer_storage(peer_storage)
        .configure_peer_connections(PeerConnectionConfig {
            host: "127.0.0.1".parse().unwrap(),
            ..Default::default()
        })
        .configure_control_service(ControlServiceConfig {
            socks_proxy_address: None,
            listener_address: control_service_address,
            requested_connection_timeout: Duration::from_millis(5000),
        })
        .build()
        .unwrap()
        .start()
        .map(Arc::new)
        .unwrap();

    (ServiceExecutor::execute(&comms, services), dht_api, comms)
}

fn pause() {
    thread::sleep(Duration::from_millis(3000));
}

#[test]
#[allow(non_snake_case)]
fn test_dht_join_propagation() {
    // Create 3 nodes where only Node B knows A and C, but A and C want to talk to each other
    let node_A_identity = new_node_identity("127.0.0.1:11113".parse().unwrap());
    let node_B_identity = new_node_identity("127.0.0.1:11114".parse().unwrap());
    let node_C_identity = new_node_identity("127.0.0.1:11115".parse().unwrap());

    // Setup Node A
    let node_A_tmpdir = TempDir::new(random_string(8).as_str()).unwrap();
    let node_A_database_name = "node_A";
    let (node_A_services, node_A_dht_service_api, _comms_A) = setup_dht_service(
        node_A_identity.clone(),
        create_peer_storage(&node_A_tmpdir, node_A_database_name, vec![node_B_identity
            .clone()
            .into()]),
    );
    // Setup Node B
    let node_B_tmpdir = TempDir::new(random_string(8).as_str()).unwrap();
    let node_B_database_name = "node_B";
    let (node_B_services, _node_B_dht_service_api, _comms_B) = setup_dht_service(
        node_B_identity.clone(),
        create_peer_storage(&node_B_tmpdir, node_B_database_name, vec![
            node_A_identity.clone().into(),
            node_C_identity.clone().into(),
        ]),
    );
    // Setup Node C
    let node_C_tmpdir = TempDir::new(random_string(8).as_str()).unwrap();
    let node_C_database_name = "node_C";
    let (node_C_services, _node_C_dht_service_api, _comms_C) = setup_dht_service(
        node_C_identity.clone(),
        create_peer_storage(&node_C_tmpdir, node_C_database_name, vec![node_B_identity
            .clone()
            .into()]),
    );

    // Send a join request from Node A, through B to C. As all Nodes are in the same network region, once Node C
    // receives the join request from Node A, it will send a direct join request back to A.
    pause();
    assert!(node_A_dht_service_api.send_join().is_ok());

    pause();
    node_A_services.shutdown().unwrap();
    node_B_services.shutdown().unwrap();
    node_C_services.shutdown().unwrap();

    // Restore PeerStorage of Node A and Node C and check that they are aware of each other
    pause();
    let node_A_peer_manager =
        PeerManager::new(create_peer_storage(&node_A_tmpdir, node_A_database_name, vec![])).unwrap();
    let node_C_peer_manager =
        PeerManager::new(create_peer_storage(&node_C_tmpdir, node_C_database_name, vec![])).unwrap();
    assert!(node_C_peer_manager
        .exists(&node_A_identity.identity.public_key)
        .unwrap());
    assert!(node_A_peer_manager
        .exists(&node_C_identity.identity.public_key)
        .unwrap());
}

#[test]
#[allow(non_snake_case)]
fn test_dht_discover_propagation() {
    // Create 3 nodes where only Node B knows A and C, but A and C want to talk to each other
    let node_A_identity = new_node_identity("127.0.0.1:11116".parse().unwrap());
    let node_B_identity = new_node_identity("127.0.0.1:11117".parse().unwrap());
    let node_C_identity = new_node_identity("127.0.0.1:11118".parse().unwrap());
    let node_D_identity = new_node_identity("127.0.0.1:11119".parse().unwrap());

    // Setup Node A
    let node_A_tmpdir = TempDir::new(random_string(8).as_str()).unwrap();
    let node_A_database_name = "node_A";
    let (node_A_services, node_A_dht_service_api, _comms_A) = setup_dht_service(
        node_A_identity.clone(),
        create_peer_storage(&node_A_tmpdir, node_A_database_name, vec![node_B_identity
            .clone()
            .into()]),
    );
    // Setup Node B
    let node_B_tmpdir = TempDir::new(random_string(8).as_str()).unwrap();
    let node_B_database_name = "node_B";
    let (node_B_services, _node_B_dht_service_api, _comms_B) = setup_dht_service(
        node_B_identity.clone(),
        create_peer_storage(&node_B_tmpdir, node_B_database_name, vec![
            node_A_identity.clone().into(),
            node_C_identity.clone().into(),
        ]),
    );
    // Setup Node C
    let node_C_tmpdir = TempDir::new(random_string(8).as_str()).unwrap();
    let node_C_database_name = "node_C";
    let (node_C_services, _node_C_dht_service_api, _comms_C) = setup_dht_service(
        node_C_identity.clone(),
        create_peer_storage(&node_C_tmpdir, node_C_database_name, vec![
            node_B_identity.clone().into(),
            node_D_identity.clone().into(),
        ]),
    );
    // Setup Node D
    let node_D_tmpdir = TempDir::new(random_string(8).as_str()).unwrap();
    let node_D_database_name = "node_D";
    let (node_D_services, _node_D_dht_service_api, _comms_D) = setup_dht_service(
        node_D_identity.clone(),
        create_peer_storage(&node_D_tmpdir, node_D_database_name, vec![node_C_identity
            .clone()
            .into()]),
    );

    // Send a discover request from Node A, through B and C, to D. Once Node D
    // receives the discover request from Node A, it will send a direct join request back to A.
    pause();
    assert!(node_A_dht_service_api
        .send_discover(
            node_D_identity.identity.public_key.clone(),
            None,
            NodeDestination::Unknown
        )
        .is_ok());

    pause();
    node_A_services.shutdown().unwrap();
    node_B_services.shutdown().unwrap();
    node_C_services.shutdown().unwrap();
    node_D_services.shutdown().unwrap();

    // Restore PeerStorage of Node A and Node D and check that they are aware of each other
    pause();
    let node_A_peer_manager =
        PeerManager::new(create_peer_storage(&node_A_tmpdir, node_A_database_name, vec![])).unwrap();
    let node_D_peer_manager =
        PeerManager::new(create_peer_storage(&node_D_tmpdir, node_D_database_name, vec![])).unwrap();
    assert!(node_A_peer_manager
        .exists(&node_D_identity.identity.public_key)
        .unwrap());
    assert!(node_D_peer_manager
        .exists(&node_A_identity.identity.public_key)
        .unwrap());
}

#[test]
#[allow(non_snake_case)]
fn test_dht_join_on_service_start() {
    // Create 2 nodes where Node A has a old control_service_address for Node B
    let node_A_identity = new_node_identity("127.0.0.1:11120".parse().unwrap());
    let outdated_node_B_identity = new_node_identity("127.0.0.1:11121".parse().unwrap());
    let node_B_identity = outdated_node_B_identity.clone(); // new_node_identity("127.0.0.1:11122".parse().unwrap());
    node_B_identity
        .set_control_service_address("127.0.0.1:11121".parse().unwrap())
        .unwrap();

    // Setup Node A
    let node_A_tmpdir = TempDir::new(random_string(8).as_str()).unwrap();
    let node_A_database_name = "node_A";
    let (node_A_services, _node_A_dht_service_api, comms_A) = setup_dht_service(
        node_A_identity.clone(),
        create_peer_storage(&node_A_tmpdir, node_A_database_name, vec![outdated_node_B_identity
            .clone()
            .into()]),
    );
    // Wait for Node A to startup
    pause();

    // Setup Node B
    let node_B_tmpdir = TempDir::new(random_string(8).as_str()).unwrap();
    let node_B_database_name = "node_B";
    let (node_B_services, _node_B_dht_service_api, _comms_B) = setup_dht_service(
        node_B_identity.clone(),
        create_peer_storage(&node_B_tmpdir, node_B_database_name, vec![node_A_identity
            .clone()
            .into()]),
    );
    // Node A and B are aware of each other on startup but Node A has outdated information for Node B
    // As Node B comes online it will send a join request to all its neighbouring peers

    pause();
    // The NetAddress of Node A has changed, the DHT Service will detect the change and inform the neighbouring peers
    // using a join request
    let node_A_updated_net_address: NetAddress = "127.0.0.1:11122".parse().unwrap();
    comms_A
        .node_identity()
        .set_control_service_address(node_A_updated_net_address.clone())
        .unwrap();

    pause();
    node_A_services.shutdown().unwrap();
    node_B_services.shutdown().unwrap();

    // Restore PeerStorage of Node A and B
    pause();
    let node_A_peer_manager =
        PeerManager::new(create_peer_storage(&node_A_tmpdir, node_A_database_name, vec![])).unwrap();
    let node_B_peer_manager =
        PeerManager::new(create_peer_storage(&node_B_tmpdir, node_B_database_name, vec![])).unwrap();
    // Check that Node A is aware of the updated NetAddress of Node B
    let mut peer = node_A_peer_manager
        .find_with_public_key(&node_B_identity.identity.public_key)
        .unwrap();
    assert!(peer
        .addresses
        .find_address_mut(&node_B_identity.control_service_address().unwrap())
        .is_ok());
    // Check that Node B is aware of the updated NetAddress of Node A
    let mut peer = node_B_peer_manager
        .find_with_public_key(&node_A_identity.identity.public_key)
        .unwrap();
    assert!(peer.addresses.find_address_mut(&node_A_updated_net_address).is_ok());
}
