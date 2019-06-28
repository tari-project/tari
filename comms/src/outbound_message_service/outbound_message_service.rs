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
        zmq::{InprocAddress, ZmqContext},
        Connection,
        Direction,
        SocketEstablishment,
    },
    message::{Frame, MessageEnvelope, MessageFlags, NodeDestination},
    outbound_message_service::{BroadcastStrategy, OutboundError, OutboundMessage},
    peer_manager::{peer_manager::PeerManager, NodeIdentity},
    types::CommsDataStore,
};
use std::sync::Arc;
use tari_utilities::message_format::MessageFormat;

/// Handler functions use the OutboundMessageService to send messages to peers. The OutboundMessage service will receive
/// messages from handlers, apply a broadcasting strategy, encrypted and serialized the messages into OutboundMessages
/// and write them to the outbound message pool.
pub struct OutboundMessageService {
    context: ZmqContext,
    outbound_address: InprocAddress,
    node_identity: Arc<NodeIdentity>,
    peer_manager: Arc<PeerManager<CommsDataStore>>,
}

impl OutboundMessageService {
    /// Constructs a new OutboundMessageService from the context, node_identity and outbound_address
    pub fn new(
        context: ZmqContext,
        node_identity: Arc<NodeIdentity>,
        outbound_address: InprocAddress, /* The outbound_address is an inproc that connects the OutboundMessagePool
                                          * and the OutboundMessageService */
        peer_manager: Arc<PeerManager<CommsDataStore>>,
    ) -> Result<OutboundMessageService, OutboundError>
    {
        Ok(OutboundMessageService {
            context,
            outbound_address,
            node_identity,
            peer_manager,
        })
    }

    /// Handler functions use the send function to transmit a message to a peer or set of peers based on the
    /// BroadcastStrategy
    pub fn send(
        &self,
        broadcast_strategy: BroadcastStrategy,
        flags: MessageFlags,
        message_envelope_body: Frame,
    ) -> Result<(), OutboundError>
    {
        // Send message to outbound message pool
        let outbound_connection = Connection::new(&self.context, Direction::Outbound)
            .set_name("OMS to OMP")
            .set_socket_establishment(SocketEstablishment::Connect)
            .establish(&self.outbound_address)
            .map_err(|e| OutboundError::ConnectionError(e))?;

        let node_identity = &self.node_identity;
        // Use the BroadcastStrategy to select appropriate peer(s) from PeerManager and then construct and send a
        // personalised message to each selected peer
        let selected_node_identities = self.peer_manager.get_broadcast_identities(broadcast_strategy)?;
        for dest_node_identity in &selected_node_identities {
            // Constructing a MessageEnvelope
            let message_envelope = MessageEnvelope::construct(
                &node_identity,
                dest_node_identity.public_key.clone(),
                NodeDestination::NodeId(dest_node_identity.node_id.clone()),
                message_envelope_body.clone(),
                flags,
            )
            .map_err(|e| OutboundError::MessageSerializationError(e))?;

            // Construct an OutboundMessage
            let outbound_message =
                OutboundMessage::<MessageEnvelope>::new(dest_node_identity.node_id.clone(), message_envelope);

            let outbound_message_buffer = vec![outbound_message
                .to_binary()
                .map_err(|e| OutboundError::MessageFormatError(e))?];
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
        connection::net_address::NetAddress,
        message::{FrameSet, Message},
        peer_manager::{
            node_id::NodeId,
            peer::{Peer, PeerFlags},
        },
    };
    use log::*;
    use std::convert::TryFrom;
    use tari_crypto::{keys::PublicKey, ristretto::RistrettoPublicKey};
    use tari_storage::lmdb::LMDBStore;

    pub fn init() {
        let _ = simple_logger::init();
    }

    #[test]
    fn test_outbound_send() {
        init();
        let context = ZmqContext::new();
        let mut rng = rand::OsRng::new().unwrap();
        let outbound_address = InprocAddress::random();

        // Create a Outbound Message Pool connection that will receive messages from the outbound message service
        let message_queue_connection = Connection::new(&context, Direction::Inbound)
            .set_socket_establishment(SocketEstablishment::Bind)
            .establish(&outbound_address)
            .unwrap();

        let node_identity = Arc::new(NodeIdentity::random_for_test(None));

        let (dest_sk, pk) = RistrettoPublicKey::random_keypair(&mut rng);
        let node_id = NodeId::from_key(&pk).unwrap();
        let net_addresses = "127.0.0.1:55445".parse::<NetAddress>().unwrap().into();
        let dest_peer = Peer::new(pk, node_id, net_addresses, PeerFlags::default());

        // Setup OutboundMessageService and transmit a message to the destination
        let peer_manager = Arc::new(PeerManager::<LMDBStore>::new(None).unwrap());
        peer_manager.add_peer(dest_peer.clone()).unwrap();

        let outbound_message_service =
            OutboundMessageService::new(context, node_identity.clone(), outbound_address, peer_manager).unwrap();

        // Construct and send OutboundMessage
        let message_header = "Test Message Header".as_bytes().to_vec();
        let message_body = "Test Message Body".as_bytes().to_vec();
        let message_envelope_body = Message::from_message_format(message_header, message_body).unwrap();
        outbound_message_service
            .send(
                BroadcastStrategy::DirectNodeId(dest_peer.node_id.clone()),
                MessageFlags::ENCRYPTED,
                message_envelope_body.to_binary().unwrap(),
            )
            .unwrap();

        let msg_bytes: FrameSet = message_queue_connection.receive(100).unwrap().drain(1..).collect();
        debug!(
            target: "comms::outbound_message_service::outbound_message_service",
            "Received message bytes: {:?}", msg_bytes
        );
        let outbound_message = OutboundMessage::<MessageEnvelope>::try_from(msg_bytes).unwrap();
        assert_eq!(outbound_message.destination_node_id, dest_peer.node_id);
        assert_eq!(outbound_message.number_of_retries(), 0);
        assert_eq!(outbound_message.last_retry_timestamp(), None);
        let message_envelope_header = outbound_message.message_envelope.to_header().unwrap();
        assert_eq!(message_envelope_header.source, node_identity.identity.public_key);
        assert_eq!(
            message_envelope_header.dest,
            NodeDestination::<RistrettoPublicKey>::NodeId(dest_peer.node_id.clone())
        );
        assert!(outbound_message.message_envelope.verify_signature().unwrap());
        assert_eq!(message_envelope_header.flags, MessageFlags::ENCRYPTED);
        let decoded_message_envelope_body = outbound_message
            .message_envelope
            .decrypted_message_body(&dest_sk, &node_identity.identity.public_key)
            .unwrap();
        assert_eq!(message_envelope_body, decoded_message_envelope_body);
    }
}
