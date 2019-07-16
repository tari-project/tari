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

// NOTE: This test uses ports 11113 and 11114
use crate::support::random_string;
use rand::rngs::OsRng;
use std::{sync::Arc, time::Duration};
use tari_comms::{
    connection::NetAddress,
    connection_manager::PeerConnectionConfig,
    control_service::ControlServiceConfig,
    peer_manager::{peer_storage::PeerStorage, NodeIdentity, Peer},
    types::CommsDatabase,
    CommsBuilder,
};
use tari_p2p::{
    dht_service::{DHTService, DHTServiceApi},
    services::{ServiceExecutor, ServiceRegistry},
    tari_message::{NetMessage, TariMessageType},
};
use tari_storage::lmdb_store::LMDBBuilder;
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
    let mut storage = PeerStorage::new(peer_database).unwrap();
    for peer in peers {
        storage.add_peer(peer).unwrap();
    }

    storage.into_datastore()
}

fn setup_dht_service(
    node_identity: NodeIdentity,
    peer_storage: CommsDatabase,
) -> (ServiceExecutor, Arc<DHTServiceApi>)
{
    let dht_service = DHTService::new();
    let dht_api = dht_service.get_api();

    let services = ServiceRegistry::new().register(dht_service);
    let comms = CommsBuilder::new()
        .with_routes(services.build_comms_routes())
        .with_node_identity(node_identity.clone())
        .with_peer_storage(peer_storage)
        .configure_peer_connections(PeerConnectionConfig {
            host: "127.0.0.1".parse().unwrap(),
            ..Default::default()
        })
        .configure_control_service(ControlServiceConfig {
            socks_proxy_address: None,
            listener_address: node_identity.control_service_address.clone(),
            accept_message_type: TariMessageType::new(NetMessage::Accept),
            requested_outbound_connection_timeout: Duration::from_millis(5000),
        })
        .build()
        .unwrap()
        .start()
        .unwrap();

    (ServiceExecutor::execute(Arc::new(comms), services), dht_api)
}

#[test]
#[allow(non_snake_case)]
fn test_dht_service() {
    let _ = simple_logger::init();

    let node_A_identity = new_node_identity("127.0.0.1:11113".parse().unwrap());
    let node_B_identity = new_node_identity("127.0.0.1:11114".parse().unwrap());

    // Setup Node A
    let node_A_tmpdir = TempDir::new(random_string(8).as_str()).unwrap();
    let (node_A_services, node_A_dht_service_api) = setup_dht_service(
        node_A_identity.clone(),
        create_peer_storage(&node_A_tmpdir, "node_A", vec![node_B_identity.clone().into()]),
    );
    // Setup Node B
    let node_B_tmpdir = TempDir::new(random_string(8).as_str()).unwrap();
    let (node_B_services, node_B_dht_service_api) = setup_dht_service(
        node_B_identity.clone(),
        create_peer_storage(&node_B_tmpdir, "node_B", vec![node_A_identity.clone().into()]),
    );

    assert!(node_A_dht_service_api.send_join().is_ok());
    assert!(node_B_dht_service_api
        .send_discover(node_A_identity.identity.public_key)
        .is_ok());

    node_A_services.shutdown().unwrap();
    node_B_services.shutdown().unwrap();
}
