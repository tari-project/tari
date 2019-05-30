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
    connection::{
        connection::EstablishedConnection,
        error::ConnectionError,
        types::SocketType,
        zmq::{Context, InprocAddress, ZmqEndpoint, ZmqError},
    },
    message::{Frame, MessageEnvelope, MessageEnvelopeHeader, MessageFlags, NodeDestination},
    outbound_message_service::{broadcast_strategy::BroadcastStrategy, outbound_message::OutboundMessage},
    peer_manager::node_identity::NodeIdentity,
    types::{Challenge, MESSAGE_PROTOCOL_VERSION, WIRE_PROTOCOL_VERSION},
};
use derive_error::Error;
use digest::Digest;
use rand::{CryptoRng, Rng};
use rmp_serde;
use serde::Serialize;
use std::{ops::Mul, sync::Arc};
use tari_crypto::{
    keys::{DiffieHellmanSharedSecret, PublicKey, SecretKey},
    signatures::{SchnorrSignature, SchnorrSignatureError},
};
use tari_utilities::{
    chacha20,
    message_format::{MessageFormat, MessageFormatError},
    ByteArray,
    ByteArrayError,
    Hashable,
};

#[derive(Debug, Error)]
pub enum OutboundError {
    /// Problem setting up a socket to an outbound message pool
    SocketError(ZmqError),
    /// Could not connect to the outbound message pool
    SocketConnectionError(zmq::Error),
    /// Problem sending message to outbound message pool
    SendError(ConnectionError),
    /// The secret key was not defined in the node identity
    UndefinedSecretKey,
    /// The message signature could not be serialized to a vector of bytes
    SignatureSerializationError,
    /// The generated shared secret could not be serialized to a vector of bytes
    SharedSecretSerializationError(ByteArrayError),
    /// The message could not be serialized
    MessageSerializationError(MessageFormatError),
    /// Could not successfully sign the message
    SignatureError(SchnorrSignatureError),
}

/// Handler functions use the OutboundMessageService to send messages to peers. The OutboundMessage service will receive
/// messages from handlers, apply a broadcasting strategy, encrypted and serialized the messages into OutboundMessages
/// and write them to the outbound message pool.
pub struct OutboundMessageService<PubKey, SecKey> {
    context: Context,
    outbound_address: InprocAddress,
    node_identity: Arc<NodeIdentity<PubKey, SecKey>>,
}

impl<PubKey, SecKey> OutboundMessageService<PubKey, SecKey>
where
    PubKey: PublicKey<K = SecKey> + Hashable + DiffieHellmanSharedSecret<K = SecKey, PK = PubKey>,
    SecKey: SecretKey + Mul<PubKey, Output = PubKey> + Mul<Output = SecKey> + Serialize,
{
    /// Constructs a new OutboundMessageService from the context, node_identity and outbound_address
    pub fn new(
        context: Context,
        outbound_address: InprocAddress, /* The outbound_address is an inproc that connects the OutboundMessagePool
                                          * and the OutboundMessageService */
        node_identity: Arc<NodeIdentity<PubKey, SecKey>>,
    ) -> OutboundMessageService<PubKey, SecKey>
    {
        OutboundMessageService {
            context,
            outbound_address,
            node_identity,
        }
    }

    /// Encrypt the message_envelope_body with the generated shared secret if the Encrypted IdentityFlag is set
    fn encrypt_envelope_body(
        &self,
        message_envelope_body: &Frame,
        dest_node_public_key: &PubKey,
    ) -> Result<Frame, OutboundError>
    {
        let node_secret_key = self
            .node_identity
            .secret_key
            .clone()
            .ok_or(OutboundError::UndefinedSecretKey)?;
        let ecdh_shared_secret = PubKey::shared_secret(&node_secret_key, &dest_node_public_key).to_vec();
        let ecdh_shared_secret_bytes: [u8; 32] =
            ByteArray::from_bytes(&ecdh_shared_secret).map_err(|e| OutboundError::SharedSecretSerializationError(e))?;
        Ok(chacha20::encode(message_envelope_body, &ecdh_shared_secret_bytes))
    }

    /// Generate a signature for the MessageEnvelopeHeader from the MessageEnvelopeBody
    fn sign_envelope_body<R: Rng + CryptoRng>(
        &self,
        message_envelope_body: &Frame,
        rng: &mut R,
    ) -> Result<Vec<u8>, OutboundError>
    {
        let challenge = Challenge::new().chain(message_envelope_body.clone()).result().to_vec();
        let nonce = SecKey::random(rng);
        let signature = SchnorrSignature::<PubKey, SecKey>::sign(
            self.node_identity
                .secret_key
                .clone()
                .ok_or(OutboundError::UndefinedSecretKey)?,
            nonce,
            &challenge,
        )
        .map_err(|e| OutboundError::SignatureError(e))?;
        let mut buf: Vec<u8> = Vec::new();
        signature
            .serialize(&mut rmp_serde::Serializer::new(&mut buf))
            .map_err(|_| OutboundError::SignatureSerializationError)?;
        Ok(buf.to_vec())
    }

    /// Handler functions use the send function to transmit a message to a peer or set of peers based on the
    /// BroadcastStrategy
    pub fn send<R: Rng + CryptoRng>(
        &self,
        _broadcast_strategy: BroadcastStrategy, /* TODO: make use of BroadcastStrategy once the PeerManager has been
                                                 * implemented */
        flags: MessageFlags,
        message_envelope_body: &Frame,
        rng: &mut R,
        dest_node_identity: Arc<NodeIdentity<PubKey, SecKey>>, /* TODO: This field is temporary until routing table
                                                                * has been implemented */
    ) -> Result<(), OutboundError>
    {
        // Todo: use the BroadcastStrategy to select appropriate peer(s) from PeerManager and then construct and send a
        // personalised message to each selected peer
        let selected_node_identities = vec![Arc::clone(&dest_node_identity)]; // TODO: Remove with PeerManager call after PeerManager has been implemented

        for dest_node_identity in &selected_node_identities {
            // Constructing a MessageEnvelope
            let message_envelope_body = if flags.contains(MessageFlags::ENCRYPTED) {
                self.encrypt_envelope_body(message_envelope_body, &dest_node_identity.public_key)?
            } else {
                message_envelope_body.clone()
            };
            let signature = self.sign_envelope_body(&message_envelope_body, rng)?;
            let message_envelope_header = MessageEnvelopeHeader {
                version: MESSAGE_PROTOCOL_VERSION,
                source: self.node_identity.public_key.clone(),
                dest: NodeDestination::NodeId(dest_node_identity.node_id.clone()),
                signature,
                flags,
            };
            let message_envelope_header_frame = message_envelope_header
                .to_binary()
                .map_err(OutboundError::MessageSerializationError)?;
            let message_envelope = MessageEnvelope::new(
                vec![WIRE_PROTOCOL_VERSION],
                message_envelope_header_frame,
                message_envelope_body,
            );
            // Construct an OutboundMessage
            let outbound_message =
                OutboundMessage::<MessageEnvelope>::new(dest_node_identity.node_id.clone(), message_envelope);
            let outbound_message_buffer = vec![outbound_message
                .to_binary()
                .map_err(|e| OutboundError::MessageSerializationError(e))?];

            // Send message to outbound message pool
            let outbound_socket = self
                .context
                .socket(SocketType::Request)
                .map_err(|e| OutboundError::SocketError(e))?;
            outbound_socket
                .connect(&self.outbound_address.to_zmq_endpoint())
                .map_err(|e| OutboundError::SocketConnectionError(e))?;
            let outbound_connection: EstablishedConnection = outbound_socket.into();
            outbound_connection
                .send(&outbound_message_buffer)
                .map_err(|e| OutboundError::SendError(e))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use crate::{
        connection::zmq::{Context, InprocAddress, ZmqEndpoint},
        peer_manager::node_id::NodeId,
    };
    use serde::Deserialize;
    use std::{convert::TryFrom, sync::Arc};
    use tari_crypto::ristretto::{RistrettoPublicKey, RistrettoSecretKey};

    #[test]
    fn test_outbound_send() {
        let context = Context::new();
        let mut rng = rand::OsRng::new().unwrap();
        let outbound_address = InprocAddress::random();

        // Create a client that will retrieve messages from the outbound message pool
        let omp_socket = context
            .socket(SocketType::Reply)
            .map_err(|e| OutboundError::SocketError(e))
            .unwrap();
        omp_socket
            .bind(&outbound_address.to_zmq_endpoint())
            .map_err(|e| OutboundError::SocketConnectionError(e))
            .unwrap();

        // Create an identity for the current node and the destination node
        let (sk, pk) = RistrettoPublicKey::random_keypair(&mut rng);
        let node_identity = Arc::new(NodeIdentity::<RistrettoPublicKey, RistrettoSecretKey>::new(
            NodeId::from_key(&pk).unwrap(),
            pk,
            Some(sk),
        ));

        let (sk, pk) = RistrettoPublicKey::random_keypair(&mut rng);
        let dest_node_identity = Arc::new(NodeIdentity::<RistrettoPublicKey, RistrettoSecretKey>::new(
            NodeId::from_key(&pk).unwrap(),
            pk,
            Some(sk),
        ));

        // Setup OutboundMessageService and transmit a message to the destination
        let outbound_message_service = OutboundMessageService::<RistrettoPublicKey, RistrettoSecretKey>::new(
            context,
            outbound_address,
            node_identity.clone(),
        );

        let message_envelope_body: Vec<u8> = vec![0, 1, 2, 3];
        assert!(outbound_message_service
            .send(
                BroadcastStrategy::Direct(dest_node_identity.node_id.clone()),
                MessageFlags::ENCRYPTED,
                &message_envelope_body,
                &mut rng,
                dest_node_identity.clone(), // TODO Remove, this is temporary until the routing table is implemented
            )
            .is_ok());

        let msg_bytes = omp_socket.recv_multipart(0).unwrap();
        let outbound_message = OutboundMessage::<MessageEnvelope>::try_from(msg_bytes).unwrap();
        assert_eq!(outbound_message.destination_node_id, dest_node_identity.node_id);
        assert_eq!(outbound_message.retry_count, 0);
        assert_eq!(outbound_message.last_retry_timestamp, None);
        let message_envelope_header: MessageEnvelopeHeader<RistrettoPublicKey> =
            outbound_message.message_envelope.to_header().unwrap();
        assert_eq!(message_envelope_header.source, node_identity.public_key);
        assert_eq!(
            message_envelope_header.dest,
            NodeDestination::<RistrettoPublicKey>::NodeId(dest_node_identity.node_id.clone())
        );
        // Verify message signature
        let mut de = rmp_serde::Deserializer::new(message_envelope_header.signature.as_slice());
        let signature = SchnorrSignature::<RistrettoPublicKey, RistrettoSecretKey>::deserialize(&mut de).unwrap();
        let challenge = Challenge::new()
            .chain(outbound_message.message_envelope.body_frame())
            .result()
            .to_vec();
        assert!(signature.verify_challenge(&node_identity.public_key, &challenge));
        // Check Encryption
        assert_eq!(message_envelope_header.flags, MessageFlags::ENCRYPTED);
        let ecdh_shared_secret = RistrettoPublicKey::shared_secret(
            &dest_node_identity.secret_key.clone().unwrap(),
            &node_identity.public_key,
        )
        .to_vec();
        let ecdh_shared_secret_bytes: [u8; 32] = ByteArray::from_bytes(&ecdh_shared_secret).unwrap();
        let decoded_message_envelope_body = chacha20::decode(
            outbound_message.message_envelope.body_frame(),
            &ecdh_shared_secret_bytes,
        );
        assert_eq!(message_envelope_body, decoded_message_envelope_body);

        assert!(omp_socket.send("OK".as_bytes(), 0).is_ok());
    }
}
