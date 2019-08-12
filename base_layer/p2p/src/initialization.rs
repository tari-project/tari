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
use std::{net::IpAddr, sync::Arc};
use tari_comms::{
    builder::{CommsBuilderError, CommsRoutes, CommsServices, CommsServicesError},
    connection::{net_address::ip::SocketAddress, NetAddress},
    connection_manager::PeerConnectionConfig,
    control_service::ControlServiceConfig,
    peer_manager::{node_identity::NodeIdentityError, NodeIdentity},
    types::{CommsPublicKey, CommsSecretKey},
    CommsBuilder,
};
use tari_storage::{key_val_store::lmdb_database::LMDBWrapper, lmdb_store::LMDBBuilder};

#[derive(Debug, Error)]
pub enum CommsInitializationError {
    NodeIdentityError(NodeIdentityError),
    CommsBuilderError(CommsBuilderError),
    CommsServicesError(CommsServicesError),
}

#[derive(Clone)]
pub struct CommsConfig {
    pub control_service: ControlServiceConfig,
    pub socks_proxy_address: Option<SocketAddress>,
    pub host: IpAddr,
    pub public_key: CommsPublicKey,
    pub secret_key: CommsSecretKey,
    pub public_address: NetAddress,
    pub datastore_path: String,
    pub peer_database_name: String,
}

pub fn initialize_comms(
    config: CommsConfig,
    comms_routes: CommsRoutes<TariMessageType>,
) -> Result<Arc<CommsServices<TariMessageType>>, CommsInitializationError>
{
    let node_identity = NodeIdentity::new(config.secret_key, config.public_key, config.public_address)
        .map_err(CommsInitializationError::NodeIdentityError)?;

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

    let builder = CommsBuilder::new()
        .with_routes(comms_routes.clone())
        .with_node_identity(node_identity)
        .with_peer_storage(peer_database)
        .configure_control_service(config.control_service)
        .configure_peer_connections(PeerConnectionConfig {
            socks_proxy_address: config.socks_proxy_address,
            host: config.host,
            ..Default::default()
        });

    let comms = builder
        .build()
        .map_err(CommsInitializationError::CommsBuilderError)?
        .start()
        .map(Arc::new)
        .map_err(CommsInitializationError::CommsServicesError)?;

    Ok(comms)
}
