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

use log::*;

use crate::{
    connection::{CurvePublicKey, NetAddress, PeerConnection, PeerConnectionState},
    peer_manager::{NodeId, Peer, PeerManager},
    types::{CommsDataStore, CommsPublicKey},
};

use super::{
    connections::LivePeerConnections,
    establisher::ConnectionEstablisher,
    protocol::PeerConnectionProtocol,
    repository::PeerConnectionEntry,
    ConnectionManagerError,
    PeerConnectionConfig,
    Result,
};

const LOG_TARGET: &'static str = "comms::connection_manager::manager";

pub struct ConnectionManager {
    connections: LivePeerConnections,
    establisher: Arc<ConnectionEstablisher>,
    peer_manager: Arc<PeerManager<CommsPublicKey, CommsDataStore>>,
}

impl ConnectionManager {
    pub fn new(peer_manager: Arc<PeerManager<CommsPublicKey, CommsDataStore>>, config: PeerConnectionConfig) -> Self {
        Self {
            connections: LivePeerConnections::new(),
            establisher: Arc::new(ConnectionEstablisher::new(config, peer_manager.clone())),
            peer_manager,
        }
    }

    pub fn get_peer_manager(&self) -> Arc<PeerManager<CommsPublicKey, CommsDataStore>> {
        self.peer_manager.clone()
    }

    pub fn establish_connection_to_peer(&self, peer: &Peer<CommsPublicKey>) -> Result<Arc<PeerConnection>> {
        self.attempt_peer_connection(peer)
    }

    pub fn establish_connection_to_node_id(&self, node_id: &NodeId) -> Result<Arc<PeerConnection>> {
        match self.peer_manager.find_with_node_id(node_id) {
            Ok(peer) => self.attempt_peer_connection(&peer),
            Err(err) => Err(ConnectionManagerError::PeerManagerError(err)),
        }
    }

    pub fn establish_requested_connection(
        &self,
        peer: &Peer<CommsPublicKey>,
        address: &NetAddress,
        server_public_key: CurvePublicKey,
    ) -> Result<Arc<PeerConnection>>
    {
        let (entry, join_handle) =
            self.establisher
                .establish_outbound_peer_connection(peer, address, server_public_key)?;

        let conn = entry.connection.clone();
        let entry = Arc::new(entry);
        self.connections
            .add_connection(peer.node_id.clone(), entry, join_handle);
        Ok(conn)
    }

    pub fn get_active_connection_count(&self) -> usize {
        self.connections.get_active_connection_count()
    }

    fn attempt_peer_connection(&self, peer: &Peer<CommsPublicKey>) -> Result<Arc<PeerConnection>> {
        let maybe_conn = self.connections.get_connection(&peer.node_id);
        let peer_conn_entry = match maybe_conn {
            Some(conn) => {
                let state = conn.get_state();

                match state {
                    PeerConnectionState::Initial |
                    PeerConnectionState::Disconnected |
                    PeerConnectionState::Shutdown => {
                        warn!(
                            target: LOG_TARGET,
                            "Peer connection state is '{}'. Attempting to reestablish connection to peer.", state
                        );
                        // Ignore not found error when dropping
                        let _ = self.connections.drop_connection(&peer.node_id);
                        self.initiate_peer_connection(peer)?
                    },
                    PeerConnectionState::Failed(err) => {
                        warn!(
                            target: LOG_TARGET,
                            "Peer connection for NodeId={} in failed state. Error({:?}) Attempting to reestablish.",
                            peer.node_id,
                            err
                        );
                        // Ignore not found error when dropping
                        self.connections.drop_connection(&peer.node_id)?;
                        self.initiate_peer_connection(peer)?
                    },
                    // Already have an active connection, just return it
                    PeerConnectionState::Listening(Some(address)) => {
                        debug!(
                            target: LOG_TARGET,
                            "Waiting for NodeId={} to connect at {}...", peer.node_id, address
                        );
                        return Ok(conn);
                    },
                    PeerConnectionState::Listening(None) => {
                        debug!(
                            target: LOG_TARGET,
                            "Listening on non-tcp socket for NodeId={}...", peer.node_id
                        );
                        return Ok(conn);
                    },
                    PeerConnectionState::Connecting => {
                        debug!(target: LOG_TARGET, "Still connecting to {}...", peer.node_id);
                        return Ok(conn);
                    },
                    PeerConnectionState::Connected(Some(address)) => {
                        debug!("Connection already established to {}.", address);
                        return Ok(conn);
                    },
                    PeerConnectionState::Connected(None) => {
                        debug!("Connection already established to non-TCP socket");
                        return Ok(conn);
                    },
                }
            },
            None => {
                debug!(
                    target: LOG_TARGET,
                    "Peer connection does not exist for NodeId={}", peer.node_id
                );
                self.initiate_peer_connection(peer)?
            },
        };

        Ok(peer_conn_entry.connection.clone())
    }

    fn initiate_peer_connection(&self, peer: &Peer<CommsPublicKey>) -> Result<Arc<PeerConnectionEntry>> {
        let protocol = PeerConnectionProtocol::new(&self.establisher)?;

        protocol
            .negotiate_peer_connection(peer)
            .and_then(|(new_inbound_conn_entry, join_handle)| {
                debug!(
                    target: LOG_TARGET,
                    "[{}] Waiting for peer connection acceptance from remote peer ", new_inbound_conn_entry.address
                );
                let config = self.establisher.get_config();
                // Wait for a message from the peer before continuing
                new_inbound_conn_entry
                    .connection
                    .wait_connected_or_failure(&config.peer_connection_establish_timeout)
                    .or_else(|err| {
                        error!(
                            target: LOG_TARGET,
                            "Peer did not accept the connection within {} [NodeId={}] : {:?}",
                            peer.node_id,
                            err,
                            config.peer_connection_establish_timeout
                        );
                        Err(ConnectionManagerError::ConnectionError(err))
                    })?;

                self.connections
                    .add_connection(peer.node_id.clone(), new_inbound_conn_entry.clone(), join_handle);
                Ok(new_inbound_conn_entry)
            })
            .or_else(|err| {
                warn!(
                    target: LOG_TARGET,
                    "Failed to establish peer connection to NodeId={}", peer.node_id
                );
                error!(target: LOG_TARGET, "Error (NodeId={}): {}", peer.node_id, err);
                Err(err)
            })
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use crate::{
        connection::{types::Linger, Context, InprocAddress, NetAddress},
        control_service::{handlers, ControlService, ControlServiceConfig, ControlServiceMessageType},
        dispatcher::Dispatcher,
        peer_manager::CommsNodeIdentity,
        test_support::{
            factories::{self, Factory},
            helpers::ConnectionMessageCounter,
        },
    };
    use std::time::Duration;

    fn make_peer_connection_config(context: &Context, consumer_address: InprocAddress) -> PeerConnectionConfig {
        PeerConnectionConfig {
            context: context.clone(),
            control_service_establish_timeout: Duration::from_millis(2000),
            peer_connection_establish_timeout: Duration::from_secs(5),
            max_message_size: 1024,
            host: "127.0.0.1".parse().unwrap(),
            max_connect_retries: 3,
            consumer_address,
            socks_proxy_address: None,
        }
    }

    fn make_peer_manager(peers: Vec<Peer<CommsPublicKey>>) -> Arc<PeerManager<CommsPublicKey, CommsDataStore>> {
        Arc::new(factories::peer_manager::create().with_peers(peers).build().unwrap())
    }

    #[test]
    #[allow(non_snake_case)]
    fn establish_peer_connection_by_peer() {
        let context = Context::new();

        let dispatcher = Dispatcher::new(handlers::ControlServiceResolver::new()).route(
            ControlServiceMessageType::EstablishConnection,
            handlers::establish_connection,
        );

        let node_identity = CommsNodeIdentity::global().unwrap();

        //---------------------------------- Node B Setup --------------------------------------------//

        let node_B_consumer_address = InprocAddress::random();
        let node_B_control_port_address: NetAddress = factories::net_address::create().build().unwrap();

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
        let node_B_connection_manager = Arc::new(ConnectionManager::new(
            node_B_peer_manager,
            make_peer_connection_config(&context, node_B_consumer_address.clone()),
        ));

        // Start node B's control service
        let node_B_control_service = ControlService::new(&context)
            .configure(ControlServiceConfig {
                socks_proxy_address: None,
                listener_address: node_B_control_port_address,
            })
            .serve(dispatcher, node_B_connection_manager)
            .unwrap();

        //---------------------------------- Node A setup --------------------------------------------//

        let node_A_consumer_address = InprocAddress::random();

        // Add node B to node A's peer manager
        let node_A_peer_manager = make_peer_manager(vec![node_B_peer.clone()]);
        let node_A_connection_manager = Arc::new(ConnectionManager::new(
            node_A_peer_manager,
            make_peer_connection_config(&context, node_A_consumer_address),
        ));

        //------------------------------ Negotiate connection to node B -----------------------------------//

        let to_node_B_conn = node_A_connection_manager
            .establish_connection_to_peer(&node_B_peer)
            .unwrap();

        to_node_B_conn.set_linger(Linger::Indefinitely).unwrap();

        assert_eq!(node_A_connection_manager.connections.get_active_connection_count(), 1);

        assert!(to_node_B_conn.is_active());

        to_node_B_conn.send(vec!["HELLO".as_bytes().to_vec()]).unwrap();
        to_node_B_conn.send(vec!["TARI".as_bytes().to_vec()]).unwrap();

        node_B_control_service.shutdown().unwrap();
        node_B_control_service.handle.join().unwrap().unwrap();

        assert_eq!(node_A_connection_manager.get_active_connection_count(), 1);
        node_B_msg_counter.assert_count(2, 1000);
    }
}
