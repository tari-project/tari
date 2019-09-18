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

use super::{
    messages::{ControlServiceRequestType, Ping, Pong, RequestPeerConnection},
    ControlServiceError,
};
use crate::{
    connection::{Direction, EstablishedConnection, NetAddress},
    control_service::messages::ControlServiceResponseType,
    message::{Message, MessageEnvelope, MessageFlags, MessageHeader, NodeDestination},
    peer_manager::{NodeId, NodeIdentity},
    types::CommsPublicKey,
};
use std::{convert::TryInto, sync::Arc, time::Duration};
use tari_utilities::message_format::MessageFormat;

/// # ControlServiceClient
///
/// This abstracts communication messages that can be sent and received to/from a [ControlService].
///
/// [ControlService]: ../service/struct.ControlService.html
pub struct ControlServiceClient {
    connection: EstablishedConnection,
    dest_public_key: CommsPublicKey,
    node_identity: Arc<NodeIdentity>,
}

impl ControlServiceClient {
    /// Create a new control service client
    pub fn new(
        node_identity: Arc<NodeIdentity>,
        dest_public_key: CommsPublicKey,
        connection: EstablishedConnection,
    ) -> Self
    {
        Self {
            node_identity,
            connection,
            dest_public_key,
        }
    }

    /// Get a reference to the underlying connection
    pub fn connection(&self) -> &EstablishedConnection {
        &self.connection
    }

    /// Send a Ping message
    pub fn send_ping(&self) -> Result<(), ControlServiceError> {
        self.send_msg(ControlServiceRequestType::Ping, Ping {})
    }

    /// Send a Ping message and wait until the given timeout for a Pong message.
    pub fn ping_pong(&self, timeout: Duration) -> Result<Option<Pong>, ControlServiceError> {
        self.send_msg(ControlServiceRequestType::Ping, Ping {})?;

        match self.receive_raw_message(timeout)? {
            Some(msg) => {
                let header = msg.deserialize_header()?;
                match header.message_type {
                    ControlServiceResponseType::Pong => Ok(Some(msg.deserialize_message()?)),
                    _ => Err(ControlServiceError::ClientUnexpectedReply),
                }
            },
            None => Ok(None),
        }
    }

    /// Wait until the given timeout for any MessageFormat message _T_.
    pub fn receive_message<T>(&self, timeout: Duration) -> Result<Option<T>, ControlServiceError>
    where T: MessageFormat {
        match self.receive_raw_message(timeout)? {
            Some(msg) => {
                let message = msg.deserialize_message()?;
                Ok(Some(message))
            },
            None => Ok(None),
        }
    }

    /// Wait until the given timeout for a raw [Message]. The [Message] signature is validated, otherwise
    /// an error is returned.
    ///
    /// [Message]: ../../message/message/struct.Message.html
    pub fn receive_raw_message(&self, timeout: Duration) -> Result<Option<Message>, ControlServiceError> {
        match connection_try!(self.connection.receive(timeout.as_millis() as u32)) {
            Some(mut frames) => {
                if self.connection.direction() == &Direction::Inbound {
                    frames.drain(0..1);
                }
                let envelope: MessageEnvelope = frames.try_into()?;
                let header = envelope.deserialize_header()?;
                if header.verify_signatures(envelope.body_frame().clone())? {
                    let msg =
                        envelope.deserialize_encrypted_body(&self.node_identity.secret_key, &self.dest_public_key)?;
                    Ok(Some(msg))
                } else {
                    Err(ControlServiceError::InvalidMessageSignature)
                }
            },
            None => Ok(None),
        }
    }

    /// Send a [RequestPeerConnection] message.
    ///
    /// [RequestPeerConnection]: ../messages/struct.RequestPeerConnection.html
    pub fn send_request_connection(
        &self,
        control_service_address: NetAddress,
        node_id: NodeId,
    ) -> Result<(), ControlServiceError>
    {
        let msg = RequestPeerConnection {
            control_service_address,
            node_id,
        };
        self.send_msg(ControlServiceRequestType::RequestPeerConnection, msg)
    }

    fn send_msg<T>(&self, message_type: ControlServiceRequestType, msg: T) -> Result<(), ControlServiceError>
    where T: MessageFormat {
        let envelope = self.construct_envelope(message_type, msg)?;

        self.connection
            .send(envelope.into_frame_set())
            .map_err(ControlServiceError::ConnectionError)
    }

    fn construct_envelope<T>(
        &self,
        message_type: ControlServiceRequestType,
        msg: T,
    ) -> Result<MessageEnvelope, ControlServiceError>
    where
        T: MessageFormat,
    {
        let header = MessageHeader::new(message_type)?;
        let msg = Message::from_message_format(header, msg)?;

        MessageEnvelope::construct(
            &self.node_identity,
            self.dest_public_key.clone(),
            NodeDestination::PublicKey(self.dest_public_key.clone()),
            msg.to_binary()?,
            MessageFlags::ENCRYPTED,
        )
        .map_err(ControlServiceError::MessageError)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::connection::{Connection, Direction, InprocAddress, ZmqContext};
    use rand::rngs::OsRng;
    use tari_crypto::keys::PublicKey;

    #[test]
    fn construct_envelope() {
        let addr = InprocAddress::random();
        let context = ZmqContext::new();
        let conn = Connection::new(&context, Direction::Outbound).establish(&addr).unwrap();
        let node_identity = Arc::new(NodeIdentity::random_for_test(Some("127.0.0.1:9000".parse().unwrap())));
        let (_, public_key) = CommsPublicKey::random_keypair(&mut OsRng::new().unwrap());

        let client = ControlServiceClient::new(node_identity.clone(), public_key.clone(), conn);
        let envelope = client
            .construct_envelope(ControlServiceRequestType::Ping, Ping {})
            .unwrap();

        let header = envelope.deserialize_header().unwrap();
        assert_eq!(header.origin_source, node_identity.identity.public_key);
        assert_eq!(header.peer_source, node_identity.identity.public_key);
        assert_eq!(header.destination, NodeDestination::PublicKey(public_key));
        assert_eq!(header.flags, MessageFlags::ENCRYPTED);
    }
}
