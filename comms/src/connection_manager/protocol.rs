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

use super::{establisher::ConnectionEstablisher, types::PeerConnectionJoinHandle, ConnectionManagerError, Result};
use crate::{
    connection::{types::Linger, CurveEncryption, CurvePublicKey, PeerConnection},
    control_service::messages::ConnectRequestOutcome,
    peer_manager::{NodeIdentity, Peer},
};
use log::*;
use std::sync::Arc;

const LOG_TARGET: &str = "comms::connection_manager::protocol";

pub(super) struct PeerConnectionProtocol<'e, 'ni> {
    node_identity: &'ni Arc<NodeIdentity>,
    establisher: &'e ConnectionEstablisher,
}

impl<'e, 'ni> PeerConnectionProtocol<'e, 'ni> {
    pub fn new(node_identity: &'ni Arc<NodeIdentity>, establisher: &'e ConnectionEstablisher) -> Self {
        Self {
            node_identity,
            establisher,
        }
    }

    /// Send Establish connection message to the peers control port to request a connection
    pub fn negotiate_peer_connection(&self, peer: &Peer) -> Result<(Arc<PeerConnection>, PeerConnectionJoinHandle)> {
        info!(target: LOG_TARGET, "[NodeId={}] Negotiating connection", peer.node_id);
        let control_client = self.establisher.connect_control_service_client(&peer)?;
        // Set the connection linger time to allow enough time to send a message after drop if required
        control_client
            .connection()
            .set_linger(Linger::Timeout(3000))
            .map_err(ConnectionManagerError::ConnectionError)?;
        info!(
            target: LOG_TARGET,
            "[NodeId={}] Established peer control port connection at address {:?}",
            peer.node_id,
            control_client.connection().get_connected_address()
        );

        let (new_inbound_conn, curve_pk, join_handle) = self.open_inbound_peer_connection(&peer)?;

        let address = new_inbound_conn
            .get_address()
            .ok_or(ConnectionManagerError::ConnectionAddressNotEstablished)?;

        debug!(
            target: LOG_TARGET,
            "[NodeId={}] Inbound peer connection established on address {}", peer.node_id, address
        );

        // Create an address which can be connected to externally
        let our_host = self.node_identity.control_service_address.host();
        let external_address = address
            .maybe_port()
            .map(|port| format!("{}:{}", our_host, port))
            .or_else(|| Some(our_host))
            .unwrap()
            .parse()
            .map_err(ConnectionManagerError::NetAddressError)?;

        debug!(
            target: LOG_TARGET,
            "[NodeId={}] Requesting to establish a connection on address {}", peer.node_id, external_address,
        );

        // Construct establish connection message
        control_client
            .send_request_connection(
                self.node_identity.control_service_address.clone(),
                self.node_identity.identity.node_id.clone(),
                external_address,
                curve_pk,
            )
            .map_err(|err| ConnectionManagerError::SendRequestConnectionFailed(format!("{:?}", err)))?;

        debug!(
            target: LOG_TARGET,
            "[NodeId={}] RequestConnection message sent", peer.node_id
        );

        let config = self.establisher.get_config();
        control_client
            .receive_message(config.peer_connection_establish_timeout)
            .map_err(|_| ConnectionManagerError::ConnectionRequestOutcomeRecvFail)?
            .ok_or(ConnectionManagerError::ConnectionRequestOutcomeTimeout)
            .and_then(|msg| match msg {
                ConnectRequestOutcome::Accepted => {
                    info!(
                        target: LOG_TARGET,
                        "[NodeId={}] RequestConnection accepted by destination peer's control port", peer.node_id
                    );
                    Ok((new_inbound_conn, join_handle))
                },
                ConnectRequestOutcome::Rejected => {
                    info!(
                        target: LOG_TARGET,
                        "[NodeId={}] RequestConnection REJECTED by destination peer's control port.", peer.node_id,
                    );
                    Err(ConnectionManagerError::ConnectionRejected)
                },
            })
    }

    fn open_inbound_peer_connection(
        &self,
        peer: &Peer,
    ) -> Result<(Arc<PeerConnection>, CurvePublicKey, PeerConnectionJoinHandle)>
    {
        let (secret_key, public_key) =
            CurveEncryption::generate_keypair().map_err(ConnectionManagerError::CurveEncryptionGenerateError)?;

        let (conn, join_handle) = self
            .establisher
            .establish_inbound_peer_connection(peer.node_id.clone().into(), secret_key)?;

        Ok((conn, public_key, join_handle))
    }
}
