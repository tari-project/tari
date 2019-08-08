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

// NOTE: These tests use ports 11123 to 11125
use crate::support::random_string;
use rand::rngs::OsRng;
use std::{convert::TryInto, sync::Arc, thread, time::Duration};
use tari_comms::{
    builder::CommsServices,
    connection::NetAddress,
    connection_manager::PeerConnectionConfig,
    control_service::ControlServiceConfig,
    message::{Frame, Message, MessageEnvelope, MessageFlags, NodeDestination},
    peer_manager::{peer_storage::PeerStorage, NodeIdentity, Peer, PeerManager},
    types::CommsDatabase,
    CommsBuilder,
};
use tari_p2p::{
    dht_service::{DHTService, DHTServiceApi, DiscoverMessage},
    saf_service::{SAFService, SAFServiceApi},
    services::{ServiceExecutor, ServiceRegistry},
    tari_message::TariMessageType,
};
use tari_storage::lmdb_store::LMDBBuilder;
use tari_utilities::message_format::MessageFormat;
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

fn setup_services(
    node_identity: NodeIdentity,
    peer_storage: CommsDatabase,
) -> (
    ServiceExecutor,
    Arc<SAFServiceApi>,
    Arc<DHTServiceApi>,
    Arc<CommsServices<TariMessageType>>,
)
{
    let control_service_address = node_identity.control_service_address().unwrap();
    let saf_service = SAFService::new();
    let saf_api = saf_service.get_api();
    let dht_service = DHTService::new();
    let dht_api = dht_service.get_api();

    let services = ServiceRegistry::new().register(saf_service).register(dht_service);
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

    (ServiceExecutor::execute(&comms, services), saf_api, dht_api, comms)
}

fn pause() {
    thread::sleep(Duration::from_millis(3000));
}

#[test]
#[allow(non_snake_case)]
fn test_saf_store() {
    // Create 3 nodes where only Node B knows A and C, but A wants to talk to Node C. Node A and C are not online at the
    // same time.
    let node_A_identity = new_node_identity("127.0.0.1:11123".parse().unwrap());
    let node_B_identity = new_node_identity("127.0.0.1:11124".parse().unwrap());
    let node_C_identity = new_node_identity("127.0.0.1:11125".parse().unwrap());

    // Setup Node B
    let node_B_tmpdir = TempDir::new(random_string(8).as_str()).unwrap();
    let node_B_database_name = "node_B";
    let (node_B_services, node_B_saf_service_api, _node_B_dht_service_api, _comms_B) = setup_services(
        node_B_identity.clone(),
        create_peer_storage(&node_B_tmpdir, node_B_database_name, vec![
            node_A_identity.clone().into(),
            node_C_identity.clone().into(),
        ]),
    );

    // Node A is sending a discovery message to Node C through Node B, but Node C is offline
    // TODO: The comms stack of Node B should automatically store the forwarded message, this is not yet implemented.
    // This storage behaviour is mimicked by manually storing the message from Node A in Node B.
    let discover_msg: Message = DiscoverMessage {
        node_id: node_A_identity.identity.node_id.clone(),
        net_address: vec![node_A_identity.control_service_address().unwrap()],
    }
    .try_into()
    .unwrap();
    let message_envelope_body: Frame = discover_msg.to_binary().unwrap();
    let message_envelope = MessageEnvelope::construct(
        &node_A_identity,
        node_C_identity.identity.public_key.clone(),
        NodeDestination::Unknown,
        message_envelope_body.clone(),
        MessageFlags::ENCRYPTED,
    )
    .unwrap();
    node_B_saf_service_api.store(message_envelope).unwrap(); // This should happen automatically when node B tries to forward the message

    pause();

    // Node C comes online
    let node_C_tmpdir = TempDir::new(random_string(8).as_str()).unwrap();
    let node_C_database_name = "node_C";
    let (node_C_services, node_C_saf_service_api, _node_C_dht_service_api, _comms_C) = setup_services(
        node_C_identity.clone(),
        create_peer_storage(&node_C_tmpdir, node_C_database_name, vec![node_B_identity
            .clone()
            .into()]),
    );
    // Retrieve messages from Node B
    node_C_saf_service_api.retrieve(None).unwrap();

    pause();
    node_B_services.shutdown().unwrap();
    node_C_services.shutdown().unwrap();

    // Restore PeerStorage of Node C and check that it is aware of Node A
    pause();
    let node_C_peer_manager =
        PeerManager::new(create_peer_storage(&node_C_tmpdir, node_C_database_name, vec![])).unwrap();
    assert!(node_C_peer_manager
        .exists(&node_A_identity.identity.public_key)
        .unwrap());
}
