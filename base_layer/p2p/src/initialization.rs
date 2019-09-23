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

use crate::tari_message::TariMessageType;
use derive_error::Error;
use futures::Sink;
use std::{error::Error, net::IpAddr, sync::Arc};
use tari_comms::{
    builder::{CommsBuilderError, CommsError, CommsNode, CommsServices},
    connection::net_address::ip::SocketAddress,
    connection_manager::PeerConnectionConfig,
    control_service::ControlServiceConfig,
    peer_manager::{node_identity::NodeIdentityError, NodeIdentity},
    CommsBuilder,
};
use tari_comms_middleware::{
    encryption::DecryptionLayer,
    forward::ForwardLayer,
    inbound_connector::InboundDomainConnector,
    message::PeerMessage,
};
use tari_storage::{lmdb_store::LMDBBuilder, LMDBWrapper};
use tokio::runtime::TaskExecutor;
use tower::ServiceBuilder;

#[derive(Debug, Error)]
pub enum CommsInitializationError {
    NodeIdentityError(NodeIdentityError),
    CommsBuilderError(CommsBuilderError),
    CommsServicesError(CommsError),
}

#[derive(Clone)]
pub struct CommsConfig {
    pub control_service: ControlServiceConfig,
    pub socks_proxy_address: Option<SocketAddress>,
    pub host: IpAddr,
    pub node_identity: Arc<NodeIdentity>,
    /// Path to the LMDB file
    pub datastore_path: String,
    /// Name to use for the peer database
    pub peer_database_name: String,
}

pub fn initialize_comms<TSink>(
    executor: TaskExecutor,
    config: CommsConfig,
    connector: InboundDomainConnector<TariMessageType, TSink>,
) -> Result<CommsNode, CommsInitializationError>
where
    TSink: Sink<Arc<PeerMessage<TariMessageType>>> + Clone + Unpin + Send + 'static,
    TSink::Error: Error + Send,
{
    let _ = std::fs::create_dir(&config.datastore_path).unwrap_or_default();
    let datastore = LMDBBuilder::new()
        .set_path(&config.datastore_path)
        .set_environment_size(10)
        .set_max_number_of_databases(1)
        .add_database(&config.peer_database_name, lmdb_zero::db::CREATE)
        .build()
        .unwrap();
    let peer_database = datastore.get_handle(&config.peer_database_name).unwrap();
    let peer_database = LMDBWrapper::new(Arc::new(peer_database));

    let inbound_middleware = |comms: CommsServices| {
        ServiceBuilder::new()
            .layer(DecryptionLayer::new(Arc::clone(&comms.node_identity)))
            .layer(ForwardLayer::new(
                comms.peer_manager,
                comms.node_identity,
                comms.outbound_service_requester,
            ))
            .service(connector)
    };

    let comms = CommsBuilder::new(executor)
        .with_node_identity(config.node_identity)
        .with_peer_storage(peer_database)
        .with_inbound_middleware(inbound_middleware)
        .configure_control_service(config.control_service)
        .configure_peer_connections(PeerConnectionConfig {
            socks_proxy_address: config.socks_proxy_address,
            host: config.host,
            ..Default::default()
        })
        .build()
        .map_err(CommsInitializationError::CommsBuilderError)?
        .start()
        .map_err(CommsInitializationError::CommsServicesError)?;

    Ok(comms)
}
