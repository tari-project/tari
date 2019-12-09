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

use crate::comms_connector::{InboundDomainConnector, PeerMessage};
use derive_error::Error;
use futures::{channel::mpsc, Sink};
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use std::{error::Error, iter, sync::Arc, time::Duration};
use tari_comms::{
    builder::{CommsBuilderError, CommsError, CommsNode},
    connection::{net_address::ip::SocketAddress, NetAddress},
    connection_manager::PeerConnectionConfig,
    control_service::ControlServiceConfig,
    outbound_message_service::ConstantBackoff,
    peer_manager::{node_identity::NodeIdentityError, NodeIdentity},
    CommsBuilder,
};
use tari_comms_dht as comms_dht;
use tari_comms_dht::{Dht, DhtConfig};
use tari_comms_middleware::{pipeline::ServicePipeline, sink::SinkMiddleware};
use tari_storage::{lmdb_store::LMDBBuilder, LMDBWrapper};
use tokio::runtime::TaskExecutor;
use tower::ServiceBuilder;

#[derive(Debug, Error)]
pub enum CommsInitializationError {
    NodeIdentityError(NodeIdentityError),
    CommsBuilderError(CommsBuilderError),
    CommsServicesError(CommsError),
}

/// Configuration for a comms node
#[derive(Clone)]
pub struct CommsConfig {
    /// Control service config.
    pub control_service: ControlServiceConfig,
    /// An optional SOCKS address.
    pub socks_proxy_address: Option<SocketAddress>,
    /// The address that the inbound peer connection will listen (bind) on. The default is 0.0.0.0:7898
    pub peer_connection_listening_address: NetAddress,
    /// Identity of this node on the network.
    pub node_identity: Arc<NodeIdentity>,
    /// Path to the LMDB data files.
    pub datastore_path: String,
    /// Name to use for the peer database
    pub peer_database_name: String,
    /// The size of the buffer (channel) which holds incoming message requests
    pub inbound_buffer_size: usize,
    /// The size of the buffer (channel) which holds pending outbound message requests
    pub outbound_buffer_size: usize,
    /// Configuration for DHT
    pub dht: DhtConfig,
    /// Length of time to wait for a new outbound connection to be established before timing out
    pub establish_connection_timeout: Duration,
}

// TODO: DRY up these initialization functions

/// Initialize Tari Comms configured for tests
pub fn initialize_local_test_comms<TSink>(
    executor: TaskExecutor,
    node_identity: Arc<NodeIdentity>,
    connector: InboundDomainConnector<TSink>,
    data_path: &str,
) -> Result<(CommsNode, Dht), CommsInitializationError>
where
    TSink: Sink<Arc<PeerMessage>> + Unpin + Clone + Send + Sync + 'static,
    TSink::Error: Error + Send + Sync,
{
    let listener_address = node_identity.control_service_address();
    let peer_database_name = {
        let mut rng = thread_rng();
        iter::repeat(())
            .map(|_| rng.sample(Alphanumeric))
            .take(8)
            .collect::<String>()
    };
    let datastore = LMDBBuilder::new()
        .set_path(data_path)
        .set_environment_size(10)
        .set_max_number_of_databases(1)
        .add_database(&peer_database_name, lmdb_zero::db::CREATE)
        .build()
        .unwrap();
    let peer_database = datastore.get_handle(&peer_database_name).unwrap();
    let peer_database = LMDBWrapper::new(Arc::new(peer_database));

    //---------------------------------- Comms --------------------------------------------//

    // Create inbound and outbound channels
    let (inbound_tx, inbound_rx) = mpsc::channel(10);
    let (outbound_tx, outbound_rx) = mpsc::channel(10);

    let comms = CommsBuilder::new(executor.clone())
        .with_node_identity(node_identity)
        .with_peer_storage(peer_database)
        .with_inbound_sink(inbound_tx)
        .with_outbound_stream(outbound_rx)
        .with_outbound_backoff(ConstantBackoff::new(Duration::from_millis(500)))
        .configure_control_service(ControlServiceConfig {
            listener_address,
            socks_proxy_address: None,
            requested_connection_timeout: Duration::from_millis(2000),
        })
        .configure_peer_connections(PeerConnectionConfig {
            socks_proxy_address: None,
            listening_address: "127.0.0.1:0".parse().expect("cannot fail"),
            peer_connection_establish_timeout: Duration::from_secs(5),
            ..Default::default()
        })
        .build()
        .map_err(CommsInitializationError::CommsBuilderError)?
        .start()
        .map_err(CommsInitializationError::CommsServicesError)?;

    // Create a channel for outbound requests
    let mut dht = comms_dht::DhtBuilder::from_comms(&comms)
        .with_config(DhtConfig {
            discovery_request_timeout: Duration::from_secs(1),
            ..Default::default()
        })
        .finish();

    //---------------------------------- Inbound Pipeline --------------------------------------------//

    // Connect inbound comms messages to the inbound pipeline and run it
    ServicePipeline::new(
        // Messages coming IN from comms to DHT
        inbound_rx,
        // Messages going OUT from DHT to connector (pubsub)
        ServiceBuilder::new()
            .layer(dht.inbound_middleware_layer())
            .service(connector),
    )
    .with_shutdown_signal(comms.shutdown_signal())
    .spawn_with(executor.clone());

    //---------------------------------- Outbound Pipeline --------------------------------------------//

    // Connect outbound message pipeline to comms, and run it
    ServicePipeline::new(
        // Requests coming IN from services to DHT
        dht.take_outbound_receiver().expect("take outbound receiver only once"),
        // Messages going OUT from DHT to comms
        ServiceBuilder::new()
            .layer(dht.outbound_middleware_layer())
            .service(SinkMiddleware::new(outbound_tx)),
    )
    .with_shutdown_signal(comms.shutdown_signal())
    .spawn_with(executor);

    Ok((comms, dht))
}

/// Initialize Tari Comms
///
/// ## Arguments
///
/// inbound_connector - Service to call
pub fn initialize_comms<TSink>(
    executor: TaskExecutor,
    config: CommsConfig,
    connector: InboundDomainConnector<TSink>,
) -> Result<(CommsNode, Dht), CommsInitializationError>
where
    TSink: Sink<Arc<PeerMessage>> + Unpin + Clone + Send + Sync + 'static,
    TSink::Error: Error + Send + Sync,
{
    let datastore = LMDBBuilder::new()
        .set_path(&config.datastore_path)
        .set_environment_size(10)
        .set_max_number_of_databases(1)
        .add_database(&config.peer_database_name, lmdb_zero::db::CREATE)
        .build()
        .unwrap();
    let peer_database = datastore.get_handle(&config.peer_database_name).unwrap();
    let peer_database = LMDBWrapper::new(Arc::new(peer_database));

    //---------------------------------- Comms --------------------------------------------//

    // Create inbound and outbound channels
    let (inbound_tx, inbound_rx) = mpsc::channel(config.inbound_buffer_size);
    let (outbound_tx, outbound_rx) = mpsc::channel(config.outbound_buffer_size);

    let comms = CommsBuilder::new(executor.clone())
        .with_node_identity(config.node_identity)
        .with_peer_storage(peer_database)
        .with_inbound_sink(inbound_tx)
        .with_outbound_stream(outbound_rx)
        .configure_control_service(config.control_service)
        .configure_peer_connections(PeerConnectionConfig {
            socks_proxy_address: config.socks_proxy_address,
            listening_address: config.peer_connection_listening_address,
            peer_connection_establish_timeout: config.establish_connection_timeout,
            ..Default::default()
        })
        .build()
        .map_err(CommsInitializationError::CommsBuilderError)?
        .start()
        .map_err(CommsInitializationError::CommsServicesError)?;

    // Create a channel for outbound requests
    let mut dht = comms_dht::DhtBuilder::from_comms(&comms)
        .with_config(config.dht.clone())
        .finish();

    //---------------------------------- Inbound Pipeline --------------------------------------------//

    // Connect inbound comms messages to the inbound pipeline and run it
    ServicePipeline::new(
        // Messages coming IN from comms to DHT
        inbound_rx,
        // Messages going OUT from DHT to connector (pubsub)
        ServiceBuilder::new()
            .layer(dht.inbound_middleware_layer())
            .service(connector),
    )
    .with_shutdown_signal(comms.shutdown_signal())
    .spawn_with(executor.clone());

    //---------------------------------- Outbound Pipeline --------------------------------------------//

    // Connect outbound message pipeline to comms, and run it
    ServicePipeline::new(
        // Requests coming IN from services to DHT
        dht.take_outbound_receiver().expect("take outbound receiver only once"),
        // Messages going OUT from DHT to comms
        ServiceBuilder::new()
            .layer(dht.outbound_middleware_layer())
            .service(SinkMiddleware::new(outbound_tx)),
    )
    .with_shutdown_signal(comms.shutdown_signal())
    .spawn_with(executor);

    Ok((comms, dht))
}
