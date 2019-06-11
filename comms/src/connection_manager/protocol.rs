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

use log::*;

use std::sync::Arc;

use tari_utilities::message_format::MessageFormat;

use crate::{
    connection::{connection::EstablishedConnection, CurveEncryption, CurvePublicKey, PeerConnection},
    control_service::ControlServiceMessageType,
    message::{p2p::EstablishConnection, Message, MessageEnvelope, MessageFlags, MessageHeader, NodeDestination},
    peer_manager::{node_identity::CommsNodeIdentity, Peer},
    types::CommsPublicKey,
};

use super::{establisher::ConnectionEstablisher, types::PeerConnectionJoinHandle, ConnectionManagerError, Result};
use std::time::Duration;

const LOG_TARGET: &'static str = "comms::connection_manager::protocol";

pub(crate) struct PeerConnectionProtocol<'e> {
    node_identity: Arc<CommsNodeIdentity>,
    establisher: &'e ConnectionEstablisher,
}

impl<'e> PeerConnectionProtocol<'e> {
    pub fn new(establisher: &'e ConnectionEstablisher) -> Result<Self> {
        CommsNodeIdentity::global()
            .map(|node_identity| Self {
                node_identity,
                establisher,
            })
            .ok_or(ConnectionManagerError::GlobalNodeIdentityNotSet)
    }

    /// Send Establish connection message to the peers control port to request a connection
    pub fn negotiate_peer_connection(
        &self,
        peer: &Peer<CommsPublicKey>,
    ) -> Result<(Arc<PeerConnection>, PeerConnectionJoinHandle)>
    {
        info!(target: LOG_TARGET, "[NodeId={}] Negotiating connection", peer.node_id);
        let control_port_conn = self.establisher.establish_control_service_connection(&peer)?;

        debug!(
            target: LOG_TARGET,
            "[NodeId={}] Control port connection established", peer.node_id
        );
        let (new_inbound_conn, curve_pk, join_handle) = self.open_inbound_peer_connection(&peer)?;

        debug!(
            target: LOG_TARGET,
            "[NodeId={}] Inbound peer connection established", peer.node_id
        );

        let address = new_inbound_conn
            .get_address()
            .ok_or(ConnectionManagerError::ConnectionAddressNotEstablished)?;

        // Construct establish connection message
        let msg = EstablishConnection {
            address,
            control_service_address: self.node_identity.control_service_address.clone(),
            public_key: self.node_identity.identity.public_key.clone(),
            node_id: self.node_identity.identity.node_id.clone(),
            server_key: curve_pk,
        };

        self.send_establish_message(&peer, control_port_conn, msg)?;
        debug!(
            target: LOG_TARGET,
            "[NodeId={}] EstablishConnection message sent", peer.node_id
        );

        Ok((new_inbound_conn, join_handle))
    }

    fn send_establish_message(
        &self,
        peer: &Peer<CommsPublicKey>,
        control_conn: EstablishedConnection,
        msg: EstablishConnection,
    ) -> Result<()>
    {
        let message_header = MessageHeader {
            message_type: ControlServiceMessageType::EstablishConnection,
        };
        let msg = Message::from_message_format(message_header, msg).map_err(ConnectionManagerError::MessageError)?;
        let body = msg.to_binary().map_err(ConnectionManagerError::MessageFormatError)?;

        let envelope = MessageEnvelope::construct(
            self.node_identity.clone(),
            peer.public_key.clone(),
            NodeDestination::NodeId(peer.node_id.clone()),
            &body,
            MessageFlags::ENCRYPTED,
        )
        .map_err(ConnectionManagerError::MessageError)?;

        control_conn
            .send(envelope.into_frame_set())
            .map_err(ConnectionManagerError::ConnectionError)?;

        Ok(())
    }

    fn open_inbound_peer_connection(
        &self,
        peer: &Peer<CommsPublicKey>,
    ) -> Result<(Arc<PeerConnection>, CurvePublicKey, PeerConnectionJoinHandle)>
    {
        let (secret_key, public_key) =
            CurveEncryption::generate_keypair().map_err(ConnectionManagerError::CurveEncryptionGenerateError)?;

        let (conn, join_handle) = self
            .establisher
            .establish_inbound_peer_connection(peer.node_id.clone().into(), secret_key)?;
        conn.wait_listening_or_failure(&Duration::from_millis(200))
            .map_err(ConnectionManagerError::ConnectionError)?;

        Ok((conn, public_key, join_handle))
    }
}
