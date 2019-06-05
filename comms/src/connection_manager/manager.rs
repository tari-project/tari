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
    connection::{zmq::CurvePublicKey, Context, NetAddress, PeerConnection, PeerConnectionState},
    peer_manager::Peer,
    types::CommsPublicKey,
};

use super::{
    protocol::PeerConnectionProtocol,
    ConnectionManagerError,
    LivePeerConnections,
    PeerConnectionConfig,
    Result,
};

const LOG_TARGET: &'static str = "comms::connection_manager::manager";

pub struct ConnectionManager {
    connections: Arc<LivePeerConnections>,
}

impl ConnectionManager {
    pub fn new(context: &Context, config: PeerConnectionConfig) -> Self {
        Self {
            connections: Arc::new(LivePeerConnections::new(context.clone(), config)),
        }
    }

    /// Gets a peer connection, establishing one if
    pub fn new_connection_to_peer(
        &self,
        peer: &mut Peer<CommsPublicKey>,
        peer_curve_pk: CurvePublicKey,
        net_address: NetAddress,
    ) -> Result<Arc<PeerConnection>>
    {
        let node_id = Arc::new(peer.node_id.clone());
        debug!("Attempting to connect to peer [{}]", node_id);
        let maybe_conn = self.connections.get_connection(&node_id);
        match maybe_conn {
            Some(conn) => self.ensure_peer_connection(peer, conn, peer_curve_pk, net_address),
            None => self.establish_peer_connection(peer, peer_curve_pk, net_address),
        }
    }

    /// Gets an active connectionf or the peer
    pub fn get_connection(&self, peer: &mut Peer<CommsPublicKey>) -> Result<Arc<PeerConnection>> {
        let node_id = Arc::new(peer.node_id.clone());
        self.connections
            .get_active_connection(&node_id)
            .ok_or(ConnectionManagerError::PeerConnectionNotFound)
    }

    fn ensure_peer_connection(
        &self,
        peer: &mut Peer<CommsPublicKey>,
        existing_conn: Arc<PeerConnection>,
        peer_curve_pk: CurvePublicKey,
        address: NetAddress,
    ) -> Result<Arc<PeerConnection>>
    {
        let state = existing_conn
            .get_state()
            .map_err(ConnectionManagerError::ConnectionError)?;
        match state {
            PeerConnectionState::Initial | PeerConnectionState::Disconnected | PeerConnectionState::Shutdown => {
                warn!(
                    target: LOG_TARGET,
                    "Peer connection state is '{}'. Attempting to reestablish connection to peer.", state
                );
                self.connections.drop_connection(&peer.node_id)?;
                self.establish_peer_connection(peer, peer_curve_pk, address)
            },
            PeerConnectionState::Failed(err) => {
                warn!(
                    target: LOG_TARGET,
                    "Peer connection for NodeId={} in failed state. Error({:?}) Attempting to reestablish.",
                    peer.node_id,
                    err
                );
                self.connections.drop_connection(&peer.node_id)?;
                self.establish_peer_connection(peer, peer_curve_pk, address)
            },
            PeerConnectionState::Connecting => {
                debug!(target: LOG_TARGET, "Still connecting to {}...", peer.node_id);
                Ok(existing_conn)
            },
            PeerConnectionState::Connected(Some(address)) => {
                debug!("Connection already established to  {}.", address);
                Ok(existing_conn)
            },
            PeerConnectionState::Connected(None) => {
                debug!("Connection already established to  non-TCP socket");
                Ok(existing_conn)
            },
        }
    }

    fn establish_peer_connection(
        &self,
        peer: &mut Peer<CommsPublicKey>,
        peer_curve_pk: CurvePublicKey,
        address: NetAddress,
    ) -> Result<Arc<PeerConnection>>
    {
        debug!("Establishing new connection to {}", peer.node_id);
        let protocol = PeerConnectionProtocol::new(peer);
        protocol
            .establish_outbound(self.connections.clone(), peer_curve_pk, address)
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
