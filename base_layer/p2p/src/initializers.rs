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

use crate::{
    ping_pong::PingPongService,
    services::{ServiceExecutor, ServiceRegistry},
    tari_message::TariMessageType,
};
use derive_error::Error;
use rand::rngs::OsRng;
use std::{net::IpAddr, sync::Arc};
use tari_comms::{
    builder::{CommsBuilderError, CommsServicesError},
    connection::net_address::ip::SocketAddress,
    connection_manager::PeerConnectionConfig,
    control_service::ControlServiceConfig,
    peer_manager::{node_identity::NodeIdentityError, NodeIdentity},
    CommsBuilder,
};

#[derive(Debug, Error)]
pub enum InitializationError {
    /// Failed to create RNG
    RngError,
    NodeIdentityError(NodeIdentityError),
    CommsBuilderError(CommsBuilderError),
    CommsServicesError(CommsServicesError),
}

pub struct CommsConfig {
    control_service: ControlServiceConfig<TariMessageType>,
    socks_proxy_address: Option<SocketAddress>,
    host: IpAddr,
}

pub struct WalletConfig {
    comms: CommsConfig,
}

pub fn initialize_wallet(config: WalletConfig) -> Result<ServiceExecutor, InitializationError> {
    let mut rng = OsRng::new().map_err(|_| InitializationError::RngError)?;

    let registry = ServiceRegistry::new().register(PingPongService::new());

    let node_identity = NodeIdentity::random(&mut rng, config.comms.control_service.listener_address.clone())
        .map_err(InitializationError::NodeIdentityError)?;

    let comm_routes = registry.build_comms_routes();

    let comms = CommsBuilder::new()
        .with_routes(comm_routes.clone())
        .with_node_identity(node_identity)
        .configure_control_service(config.comms.control_service)
        .configure_peer_connections(PeerConnectionConfig {
            socks_proxy_address: config.comms.socks_proxy_address,
            host: config.comms.host,
            ..Default::default()
        })
        .build()
        .map_err(InitializationError::CommsBuilderError)?
        .start()
        .map(Arc::new)
        .map_err(InitializationError::CommsServicesError)?;

    Ok(ServiceExecutor::execute(comms, registry))
}
