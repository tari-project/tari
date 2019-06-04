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

use tari_crypto::keys::DiffieHellmanSharedSecret;

use tari_utilities::{chacha20, message_format::MessageFormat, ByteArray};

use crate::{
    connection::{
        connection::EstablishedConnection,
        Connection,
        CurveEncryption,
        CurvePublicKey,
        Direction,
        Linger,
        PeerConnection,
    },
    control_service::ControlServiceMessageType,
    message::{
        p2p::EstablishConnection,
        Frame,
        Message,
        MessageEnvelope,
        MessageFlags,
        MessageHeader,
        NodeDestination,
    },
    peer_manager::{node_identity::CommsNodeIdentity, Peer},
    types::CommsPublicKey,
};

use super::{ConnectionDirection, ConnectionManagerError, LivePeerConnections, Result};

pub struct PeerConnectionProtocol<'p> {
    peer: &'p mut Peer<CommsPublicKey>,
}

impl<'p> PeerConnectionProtocol<'p> {
    pub fn new(peer: &'p mut Peer<CommsPublicKey>) -> Self {
        Self { peer }
    }

    pub fn establish(
        &self,
        connections: Arc<LivePeerConnections>,
        server_public_key: CurvePublicKey,
    ) -> Result<Arc<PeerConnection>>
    {
        // Send establish connection to peer's control service
        let control_port_connection = self.establish_control_service_connection(&connections, server_public_key)?;

        let (inbound_peer_conn, establish_message) = self.open_inbound_peer_connection(&connections)?;

        self.send_establish_message(control_port_connection, establish_message)?;

        Ok(inbound_peer_conn)
    }

    fn send_establish_message<'c>(&self, control_conn: EstablishedConnection, msg: EstablishConnection) -> Result<()> {
        let node_identity = CommsNodeIdentity::global().ok_or(ConnectionManagerError::GlobalNodeIdentityNotSet)?;

        let message_header = MessageHeader {
            message_type: ControlServiceMessageType::EstablishConnection,
        };
        let msg = Message::from_message_format(message_header, msg).map_err(ConnectionManagerError::MessageError)?;
        let body = msg.to_binary().map_err(ConnectionManagerError::MessageFormatError)?;

        // Encrypt body
        let encrypted_body = self.encrypt_body(&node_identity, body)?;

        let envelope = MessageEnvelope::construct(
            node_identity,
            NodeDestination::NodeId(self.peer.node_id.clone()),
            encrypted_body,
            MessageFlags::ENCRYPTED,
        )
        .map_err(ConnectionManagerError::MessageError)?;

        control_conn
            .send(envelope.into_frame_set())
            .map_err(ConnectionManagerError::ConnectionError)?;

        Ok(())
    }

    fn encrypt_body(&self, identity: &Arc<CommsNodeIdentity>, body: Frame) -> Result<Frame> {
        let ecdh_shared_secret = CommsPublicKey::shared_secret(&identity.secret_key, &self.peer.public_key).to_vec();
        let ecdh_shared_secret_bytes: [u8; 32] = ByteArray::from_bytes(&ecdh_shared_secret)
            .map_err(ConnectionManagerError::SharedSecretSerializationError)?;
        Ok(chacha20::encode(&body, &ecdh_shared_secret_bytes))
    }

    fn open_inbound_peer_connection(
        &self,
        connections: &Arc<LivePeerConnections>,
    ) -> Result<(Arc<PeerConnection>, EstablishConnection)>
    {
        let (secret_key, public_key) =
            CurveEncryption::generate_keypair().map_err(ConnectionManagerError::CurveEncryptionGenerateError)?;

        let address = connections.establish_connection(ConnectionDirection::Inbound {
            node_id: self.peer.node_id.clone(),
            secret_key,
        })?;

        let node_identity = CommsNodeIdentity::global().ok_or(ConnectionManagerError::GlobalNodeIdentityNotSet)?;

        let connection = connections
            .get_connection(&self.peer.node_id)
            .ok_or(ConnectionManagerError::PeerConnectionNotFound)?;

        Ok((connection, EstablishConnection {
            address,
            control_service_address: node_identity.control_service_address.clone(),
            public_key: node_identity.identity.public_key.clone(),
            node_id: node_identity.identity.node_id.clone(),
            server_key: public_key,
        }))
    }

    fn establish_control_service_connection(
        &self,
        connections: &Arc<LivePeerConnections>,
        server_public_key: CurvePublicKey,
    ) -> Result<EstablishedConnection>
    {
        let context = connections.borrow_context();
        let config = &connections.config;

        // TODO: Set net address stats and try all net addresses before giving up
        let address = self.peer.addresses.addresses[0].clone().as_net_address();

        let (sk, pk) = CurveEncryption::generate_keypair().map_err(ConnectionManagerError::ConnectionError)?;

        let conn = Connection::new(context, Direction::Outbound)
            .set_linger(Linger::Timeout(100))
            .set_socks_proxy_addr(config.socks_proxy_address.clone())
            .set_max_message_size(Some(config.max_message_size))
            .set_curve_encryption(CurveEncryption::Client {
                server_public_key,
                public_key: pk,
                secret_key: sk,
            })
            .set_receive_hwm(0)
            .establish(&address)
            .map_err(ConnectionManagerError::ConnectionError)?;

        Ok(conn)
    }
}
