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
        zmq::{Context, InprocAddress},
        Connection,
        Direction,
        SocketEstablishment,
    },
    message::{Frame, MessageEnvelope, MessageEnvelopeHeader, MessageFlags, NodeDestination},
    outbound_message_service::{BroadcastStrategy, OutboundError, OutboundMessage},
    peer_manager::{node_identity::CommsNodeIdentity, peer_manager::PeerManager},
    types::{Challenge, CommsCipher, CommsPublicKey, CommsSecretKey, MESSAGE_PROTOCOL_VERSION, WIRE_PROTOCOL_VERSION},
};

use digest::Digest;
use rand::{CryptoRng, Rng};
use rmp_serde;
use serde::Serialize;
use std::sync::Arc;
use tari_crypto::{
    keys::{DiffieHellmanSharedSecret, SecretKey},
    signatures::SchnorrSignature,
};
use tari_storage::keyvalue_store::DataStore;
use tari_utilities::{ciphers::cipher::Cipher, message_format::MessageFormat, ByteArray};

/// Handler functions use the OutboundMessageService to send messages to peers. The OutboundMessage service will receive
/// messages from handlers, apply a broadcasting strategy, encrypted and serialized the messages into OutboundMessages
/// and write them to the outbound message pool.
pub struct OutboundMessageService<DS> {
    context: Context,
    outbound_address: InprocAddress,
    node_identity: Arc<CommsNodeIdentity>,
    peer_manager: Arc<PeerManager<CommsPublicKey, DS>>,
}

impl<DS> OutboundMessageService<DS>
where DS: DataStore
{
    /// Constructs a new OutboundMessageService from the context, node_identity and outbound_address
    pub fn new(
        context: Context,
        outbound_address: InprocAddress, /* The outbound_address is an inproc that connects the OutboundMessagePool
                                          * and the OutboundMessageService */
        peer_manager: Arc<PeerManager<CommsPublicKey, DS>>,
    ) -> Result<OutboundMessageService<DS>, OutboundError>
    {
        let node_identity: Arc<CommsNodeIdentity> =
            CommsNodeIdentity::global().ok_or(OutboundError::UndefinedSecretKey)?;

        Ok(OutboundMessageService {
            context,
            outbound_address,
            node_identity,
            peer_manager,
        })
    }

    /// Encrypt the message_envelope_body with the generated shared secret if the Encrypted IdentityFlag is set
    fn encrypt_envelope_body(
        &self,
        message_envelope_body: &Frame,
        dest_node_public_key: &CommsPublicKey,
    ) -> Result<Frame, OutboundError>
    {
        let ecdh_shared_secret =
            CommsPublicKey::shared_secret(&self.node_identity.secret_key, &dest_node_public_key).to_vec();
        let ecdh_shared_secret_bytes: [u8; 32] =
            ByteArray::from_bytes(&ecdh_shared_secret).map_err(|e| OutboundError::SharedSecretSerializationError(e))?;
        Ok(CommsCipher::seal_with_integral_nonce(
            message_envelope_body,
            &ecdh_shared_secret_bytes.to_vec(),
        )?)
    }

    /// Generate a signature for the MessageEnvelopeHeader from the MessageEnvelopeBody
    fn sign_envelope_body<R: Rng + CryptoRng>(
        &self,
        message_envelope_body: &Frame,
        rng: &mut R,
    ) -> Result<Vec<u8>, OutboundError>
    {
        let challenge = Challenge::new().chain(message_envelope_body.clone()).result().to_vec();
        let nonce = CommsSecretKey::random(rng);
        let signature = SchnorrSignature::<CommsPublicKey, CommsSecretKey>::sign(
            self.node_identity.secret_key.clone(),
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
        broadcast_strategy: BroadcastStrategy,
        flags: MessageFlags,
        message_envelope_body: &Frame,
        rng: &mut R,
    ) -> Result<(), OutboundError>
    {
        let node_identity = CommsNodeIdentity::global().ok_or(OutboundError::UndefinedSecretKey)?;
        // Use the BroadcastStrategy to select appropriate peer(s) from PeerManager and then construct and send a
        // personalised message to each selected peer
        let selected_node_identities = self.peer_manager.get_broadcast_identities(broadcast_strategy)?;
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
                source: node_identity.identity.public_key.clone(),
                dest: NodeDestination::NodeId(dest_node_identity.node_id.clone()),
                signature,
                flags,
            };
            let message_envelope_header_frame = message_envelope_header
                .to_binary()
                .map_err(|e| OutboundError::MessageFormatError(e))?;
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
                .map_err(|e| OutboundError::MessageFormatError(e))?];
            // Send message to outbound message pool
            let outbound_connection = Connection::new(&self.context, Direction::Outbound)
                .set_socket_establishment(SocketEstablishment::Connect)
                .establish(&self.outbound_address)
                .map_err(|e| OutboundError::ConnectionError(e))?;
            outbound_connection
                .send(&outbound_message_buffer)
                .map_err(|e| OutboundError::ConnectionError(e))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use crate::{
        connection::net_address::{net_addresses::NetAddresses, NetAddress},
        message::FrameSet,
        peer_manager::{
            node_id::NodeId,
            peer::{Peer, PeerFlags},
        },
    };
    use serde::Deserialize;
    use std::convert::TryFrom;
    use tari_crypto::{
        keys::PublicKey,
        ristretto::{RistrettoPublicKey, RistrettoSecretKey},
    };
    use tari_storage::lmdb::LMDBStore;

    #[test]
    fn test_outbound_send() {
        let context = Context::new();
        let mut rng = rand::OsRng::new().unwrap();
        let outbound_address = InprocAddress::random();

        // Create a Outbound Message Pool connection that will receive messages from the outbound message service
        let message_queue_connection = Connection::new(&context, Direction::Inbound)
            .set_socket_establishment(SocketEstablishment::Bind)
            .establish(&outbound_address)
            .unwrap();

        let node_identity = CommsNodeIdentity::global().unwrap();

        let (dest_sk, pk) = RistrettoPublicKey::random_keypair(&mut rng);
        let node_id = NodeId::from_key(&pk).unwrap();
        let net_addresses = NetAddresses::from("1.2.3.4:8000".parse::<NetAddress>().unwrap());
        let dest_peer: Peer<RistrettoPublicKey> =
            Peer::<RistrettoPublicKey>::new(pk, node_id, net_addresses, PeerFlags::default());

        // Setup OutboundMessageService and transmit a message to the destination
        let peer_manager = Arc::new(PeerManager::<CommsPublicKey, LMDBStore>::new(None).unwrap());
        assert!(peer_manager.add_peer(dest_peer.clone()).is_ok());

        let outbound_message_service = OutboundMessageService::new(context, outbound_address, peer_manager).unwrap();

        let message_envelope_body: Vec<u8> = vec![0, 1, 2, 3];
        assert!(outbound_message_service
            .send(
                BroadcastStrategy::Direct(dest_peer.node_id.clone()),
                MessageFlags::ENCRYPTED,
                &message_envelope_body,
                &mut rng,
            )
            .is_ok());

        let msg_bytes: FrameSet = message_queue_connection.receive(100).unwrap().drain(1..).collect();
        let outbound_message = OutboundMessage::<MessageEnvelope>::try_from(msg_bytes).unwrap();
        assert_eq!(outbound_message.destination_node_id, dest_peer.node_id);
        assert_eq!(outbound_message.number_of_retries(), 0);
        assert_eq!(outbound_message.last_retry_timestamp(), None);
        let message_envelope_header: MessageEnvelopeHeader<RistrettoPublicKey> =
            outbound_message.message_envelope.to_header().unwrap();
        assert_eq!(message_envelope_header.source, node_identity.identity.public_key);
        assert_eq!(
            message_envelope_header.dest,
            NodeDestination::<RistrettoPublicKey>::NodeId(dest_peer.node_id.clone())
        );
        // Verify message signature
        let mut de = rmp_serde::Deserializer::new(message_envelope_header.signature.as_slice());
        let signature = SchnorrSignature::<RistrettoPublicKey, RistrettoSecretKey>::deserialize(&mut de).unwrap();
        let challenge = Challenge::new()
            .chain(outbound_message.message_envelope.body_frame())
            .result()
            .to_vec();
        assert!(signature.verify_challenge(&node_identity.identity.public_key, &challenge));
        // Check Encryption
        assert_eq!(message_envelope_header.flags, MessageFlags::ENCRYPTED);
        let ecdh_shared_secret =
            RistrettoPublicKey::shared_secret(&dest_sk, &node_identity.identity.public_key).to_vec();
        let ecdh_shared_secret_bytes: [u8; 32] = ByteArray::from_bytes(&ecdh_shared_secret).unwrap();
        let decoded_message_envelope_body: Vec<u8> = CommsCipher::open_with_integral_nonce(
            outbound_message.message_envelope.body_frame(),
            &ecdh_shared_secret_bytes.to_vec(),
        )
        .unwrap();
        assert_eq!(message_envelope_body, decoded_message_envelope_body);
    }
}
