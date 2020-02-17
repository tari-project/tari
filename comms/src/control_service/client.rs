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
    messages::{MessageHeader, MessageType, PingMessage, PongMessage, RequestConnectionMessage},
    ControlServiceError,
};
use crate::{
    connection::{ConnectionDirection, EstablishedConnection},
    message::{Envelope, EnvelopeBody, MessageExt, MessageFlags},
    peer_manager::{NodeId, NodeIdentity, PeerFeatures},
    types::CommsPublicKey,
    utils::crypt,
};
use bytes::Bytes;
use log::*;
use multiaddr::Multiaddr;
use prost::Message;
use std::{sync::Arc, time::Duration};
use tari_crypto::tari_utilities::ByteArray;

const LOG_TARGET: &str = "comms::control_service::client";

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
        trace!(target: LOG_TARGET, "Sending PING message");
        self.send_msg(MessageType::Ping, PingMessage {})
    }

    /// Send a Ping message and wait until the given timeout for a Pong message.
    pub fn ping_pong(&self, timeout: Duration) -> Result<Option<PongMessage>, ControlServiceError> {
        self.send_ping()?;

        trace!(
            target: LOG_TARGET,
            "Awaiting PONG message for {}ms",
            timeout.as_millis()
        );
        match self.receive_envelope(timeout)? {
            Some(envelope) => {
                let decrypted_body = crypt::decrypt(&self.shared_secret(), &envelope.body)?;
                let body = EnvelopeBody::decode(decrypted_body.as_slice())?;
                let header = body
                    .decode_part::<MessageHeader>(0)?
                    .ok_or_else(|| ControlServiceError::InvalidEnvelopeBody)?;
                match MessageType::from_i32(header.message_type) {
                    Some(MessageType::Pong) => {
                        let msg = body
                            .decode_part(1)?
                            .ok_or_else(|| ControlServiceError::InvalidEnvelopeBody)?;

                        trace!(target: LOG_TARGET, "Received PONG",);
                        Ok(Some(msg))
                    },
                    _ => Err(ControlServiceError::ClientUnexpectedReply),
                }
            },
            None => Ok(None),
        }
    }

    /// Wait until the given timeout for any message _T_.
    pub fn receive_message<T>(&self, timeout: Duration) -> Result<Option<T>, ControlServiceError>
    where T: prost::Message + Default {
        match self.receive_envelope(timeout)? {
            Some(msg) => {
                trace!(target: LOG_TARGET, "Received envelope. Decrypting...");
                let decrypted_bytes = crypt::decrypt(&self.shared_secret(), &msg.body)?;
                let body = EnvelopeBody::decode(decrypted_bytes.as_slice())?;
                trace!(target: LOG_TARGET, "Decoding envelope body of length {}", body.len());
                let maybe_message = body.decode_part(1)?;
                Ok(maybe_message)
            },
            None => Ok(None),
        }
    }

    /// Wait until the given timeout for an [Envelope]. The [Envelope] signature is validated, otherwise
    /// an error is returned.
    ///
    /// [Envelope]: crate::message::Envelope
    pub fn receive_envelope(&self, timeout: Duration) -> Result<Option<Envelope>, ControlServiceError> {
        match connection_try!(self.connection.receive(timeout.as_millis() as u32)) {
            Some(mut frames) => {
                if self.connection.direction() == &ConnectionDirection::Inbound {
                    frames.remove(0);
                }
                let envelope_frame = frames.get(0).ok_or_else(|| ControlServiceError::InvalidEnvelope)?;
                let envelope = Envelope::decode(envelope_frame.as_slice())?;
                if envelope.verify_signature()? {
                    Ok(Some(envelope))
                } else {
                    Err(ControlServiceError::InvalidMessageSignature)
                }
            },
            None => Ok(None),
        }
    }

    /// Send a [RequestConnectionMessage] message.
    ///
    /// [RequestConnectionMessage]: ../messages/struct.RequestConnectionMessage.html
    pub fn send_request_connection(
        &self,
        control_service_address: Multiaddr,
        node_id: NodeId,
        features: PeerFeatures,
    ) -> Result<(), ControlServiceError>
    {
        self.send_msg(MessageType::RequestConnection, RequestConnectionMessage {
            control_service_address: format!("{}", control_service_address),
            node_id: node_id.to_vec(),
            features: features.bits(),
        })
    }

    fn send_msg<T>(&self, message_type: MessageType, msg: T) -> Result<(), ControlServiceError>
    where T: prost::Message {
        let envelope = self.construct_envelope(message_type, msg)?;
        let frame = envelope.to_encoded_bytes()?;

        self.connection
            .send(&[frame])
            .map_err(ControlServiceError::ConnectionError)
    }

    fn construct_envelope<T>(&self, message_type: MessageType, msg: T) -> Result<Envelope, ControlServiceError>
    where T: prost::Message {
        let header = MessageHeader::new(message_type);
        let body_bytes = wrap_in_envelope_body!(header, msg)?.to_encoded_bytes()?;
        let encrypted_bytes = crypt::encrypt(&self.shared_secret(), &body_bytes)?;

        Envelope::construct_signed(
            self.node_identity.secret_key(),
            self.node_identity.public_key(),
            Bytes::from(encrypted_bytes),
            MessageFlags::ENCRYPTED,
        )
        .map_err(ControlServiceError::MessageError)
    }

    fn shared_secret(&self) -> CommsPublicKey {
        crypt::generate_ecdh_secret(self.node_identity.secret_key(), &self.dest_public_key)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::connection::{Connection, ConnectionDirection, InprocAddress, ZmqContext};
    use rand::rngs::OsRng;
    use tari_crypto::keys::PublicKey;

    #[test]
    fn construct_envelope() {
        let addr = InprocAddress::random();
        let context = ZmqContext::new();
        let conn = Connection::new(&context, ConnectionDirection::Outbound)
            .establish(&addr)
            .unwrap();
        let node_identity = Arc::new(NodeIdentity::random_for_test(
            Some("/ip4/127.0.0.1/tcp/9000".parse().unwrap()),
            PeerFeatures::empty(),
        ));
        let (_, public_key) = CommsPublicKey::random_keypair(&mut OsRng);

        let client = ControlServiceClient::new(node_identity.clone(), public_key.clone(), conn);
        let envelope = client.construct_envelope(MessageType::Ping, PingMessage {}).unwrap();

        let header = envelope.header.unwrap();
        assert_eq!(&header.get_comms_public_key().unwrap(), node_identity.public_key());
        assert_eq!(header.flags, MessageFlags::ENCRYPTED.bits());
    }
}
