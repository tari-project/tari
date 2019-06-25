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

use std::sync::Arc;

use super::{TestFactory, TestFactoryError};

use crate::support::factories::peer_manager::PeerManagerFactory;

use crate::support::factories::node_identity::NodeIdentityFactory;
use tari_comms::{
    connection::ZmqContext,
    connection_manager::{ConnectionManager, PeerConnectionConfig},
    peer_manager::{NodeIdentity, PeerManager},
    types::{CommsDataStore, CommsPublicKey},
};

pub fn create() -> ConnectionManagerFactory {
    ConnectionManagerFactory::default()
}

#[derive(Default)]
pub struct ConnectionManagerFactory {
    zmq_context: Option<ZmqContext>,
    peer_connection_config: PeerConnectionConfig,
    peer_manager: Option<Arc<PeerManager<CommsPublicKey, CommsDataStore>>>,
    peer_manager_factory: PeerManagerFactory,
    node_identity_factory: NodeIdentityFactory,
    node_identity: Option<Arc<NodeIdentity>>,
}

impl ConnectionManagerFactory {
    factory_setter!(
        with_peer_connection_config,
        peer_connection_config,
        PeerConnectionConfig
    );

    factory_setter!(
        with_peer_manager,
        peer_manager,
        Option<Arc<PeerManager<CommsPublicKey, CommsDataStore>>>
    );

    factory_setter!(with_peer_manager_factory, peer_manager_factory, PeerManagerFactory);

    factory_setter!(with_context, zmq_context, Option<ZmqContext>);

    factory_setter!(with_node_identity, node_identity, Option<Arc<NodeIdentity>>);

    factory_setter!(with_node_identity_factory, node_identity_factory, NodeIdentityFactory);
}

impl TestFactory for ConnectionManagerFactory {
    type Object = ConnectionManager;

    fn build(self) -> Result<Self::Object, TestFactoryError> {
        let zmq_context = self.zmq_context.unwrap_or(ZmqContext::new());

        let peer_manager = self
            .peer_manager
            .or(Some(Arc::new(self.peer_manager_factory.build()?)))
            .unwrap();

        let node_identity = self
            .node_identity
            .or(Some(Arc::new(self.node_identity_factory.build()?)))
            .unwrap();

        let config = self.peer_connection_config;

        let conn_manager = ConnectionManager::new(zmq_context, node_identity, peer_manager, config);

        Ok(conn_manager)
    }
}
