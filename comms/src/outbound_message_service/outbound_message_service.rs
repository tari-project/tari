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

use super::{outbound_message_pool::OutboundMessage, BroadcastStrategy, OutboundError};
use crate::{
    message::{Frame, Message, MessageEnvelope, MessageError, MessageFlags, NodeDestination},
    peer_manager::{peer_manager::PeerManager, NodeIdentity},
};
use crossbeam_channel::Sender;
use std::{convert::TryInto, sync::Arc};
use tari_utilities::message_format::MessageFormat;

/// Handler functions use the OutboundMessageService to send messages to peers. The OutboundMessage service will receive
/// messages from handlers, apply a broadcasting strategy, encrypted and serialized the messages into OutboundMessages
/// and write them to the outbound message pool.
pub struct OutboundMessageService {
    message_sender: Sender<OutboundMessage>,
    node_identity: Arc<NodeIdentity>,
    peer_manager: Arc<PeerManager>,
}

impl OutboundMessageService {
    /// Constructs a new OutboundMessageService
    pub fn new(
        node_identity: Arc<NodeIdentity>,
        message_sender: Sender<OutboundMessage>,
        peer_manager: Arc<PeerManager>,
    ) -> Result<OutboundMessageService, OutboundError>
    {
        Ok(OutboundMessageService {
            message_sender,
            node_identity,
            peer_manager,
        })
    }

    /// Sends a domain-level message using the given BroadcastStrategy.
    ///
    /// *Arguments*
    ///
    /// - `broadcast_strategy`: [BroadcastStrategy]
    /// - `flags`: MessageFlags - See [message module docs].
    /// - `message`: T - The message to send.
    ///
    /// [BroadcastStrategy]: ../broadcast_strategy/enum.BroadcastStrategy.html
    /// [message module docs]: ../../message/index.html
    pub fn send_message<T>(
        &self,
        broadcast_strategy: BroadcastStrategy,
        flags: MessageFlags,
        message: T,
    ) -> Result<(), OutboundError>
    where
        T: TryInto<Message, Error = MessageError>,
        T: MessageFormat,
    {
        let msg = message.try_into().map_err(OutboundError::MessageSerializationError)?;

        let message_envelope_body = msg.to_binary().map_err(OutboundError::MessageFormatError)?;
        self.send_raw(broadcast_strategy, flags, message_envelope_body)
    }

    /// Handler functions use the send function to transmit a message to a peer or set of peers based on the
    /// BroadcastStrategy
    pub fn send_raw(
        &self,
        broadcast_strategy: BroadcastStrategy,
        flags: MessageFlags,
        message_envelope_body: Frame,
    ) -> Result<(), OutboundError>
    {
        // Use the BroadcastStrategy to select appropriate peer(s) from PeerManager and then construct and send a
        // personalised message to each selected peer
        let selected_node_identities = self.peer_manager.get_broadcast_identities(broadcast_strategy)?;

        // Constructing a MessageEnvelope for each recipient
        for dest_node_identity in selected_node_identities.into_iter() {
            let message_envelope = MessageEnvelope::construct(
                &self.node_identity,
                dest_node_identity.public_key.clone(),
                NodeDestination::NodeId(dest_node_identity.node_id.clone()),
                message_envelope_body.clone(),
                flags,
            )
            .map_err(OutboundError::MessageSerializationError)?;

            let msg = OutboundMessage::new(dest_node_identity.node_id, message_envelope.into_frame_set());
            self.message_sender
                .send(msg)
                .map_err(|_| OutboundError::SyncSenderError)?;
        }
        Ok(())
    }

    /// Forwards a received message_envelope to other peers using the given BroadcastStrategy.
    ///
    /// *Arguments*
    ///
    /// - `broadcast_strategy`: [BroadcastStrategy]
    /// - `message_envelope`: MessageEnvelope - The message to forward.
    ///
    /// [BroadcastStrategy]: ../broadcast_strategy/enum.BroadcastStrategy.html
    pub fn forward_message(
        &self,
        broadcast_strategy: BroadcastStrategy,
        message_envelope: MessageEnvelope,
    ) -> Result<(), OutboundError>
    {
        // Use the BroadcastStrategy to select appropriate peer(s) from PeerManager and then forward the
        // received message to each selected peer
        let selected_node_identities = self.peer_manager.get_broadcast_identities(broadcast_strategy)?;

        // Modify MessageEnvelope for forwarding
        let message_envelope = MessageEnvelope::forward_construct(&self.node_identity, message_envelope)
            .map_err(OutboundError::MessageSerializationError)?;
        let message_envelope_frames = message_envelope.into_frame_set();

        // Constructing an OutboundMessage for each recipient
        for dest_node_identity in selected_node_identities.into_iter() {
            let msg = OutboundMessage::new(dest_node_identity.node_id, message_envelope_frames.clone());

            self.message_sender
                .send(msg)
                .map_err(|_| OutboundError::SyncSenderError)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use crate::{
        connection::net_address::NetAddress,
        message::Message,
        peer_manager::{
            node_id::NodeId,
            peer::{Peer, PeerFlags},
        },
    };
    use bitflags::_core::time::Duration;
    use crossbeam_channel as channel;
    use tari_crypto::{keys::PublicKey, ristretto::RistrettoPublicKey};
    use tari_storage::HMapDatabase;

    fn make_test_message_frame() -> Frame {
        let message_header = "Test Message Header".as_bytes().to_vec();
        let message_body = "Test Message Body".as_bytes().to_vec();
        let message_envelope_body = Message::from_message_format(message_header, message_body).unwrap();
        message_envelope_body.to_binary().unwrap()
    }

    #[test]
    fn test_outbound_send() {
        let mut rng = rand::OsRng::new().unwrap();
        let node_identity = Arc::new(NodeIdentity::random_for_test(None));

        let (dest_sk, pk) = RistrettoPublicKey::random_keypair(&mut rng);
        let node_id = NodeId::from_key(&pk).unwrap();
        let net_addresses = "127.0.0.1:55445".parse::<NetAddress>().unwrap().into();
        let dest_peer = Peer::new(pk, node_id, net_addresses, PeerFlags::default());

        // Setup OutboundMessageService and transmit a message to the destination
        let peer_manager = Arc::new(PeerManager::new(HMapDatabase::new()).unwrap());
        peer_manager.add_peer(dest_peer.clone()).unwrap();

        let (message_sender, message_receiver) = channel::unbounded();
        let outbound_message_service =
            OutboundMessageService::new(node_identity.clone(), message_sender, peer_manager).unwrap();

        // Construct and send OutboundMessage
        let message_header = "Test Message Header".as_bytes().to_vec();
        let message_body = "Test Message Body".as_bytes().to_vec();
        let message = Message::from_message_format(message_header, message_body).unwrap();
        outbound_message_service
            .send_raw(
                BroadcastStrategy::DirectNodeId(dest_peer.node_id.clone()),
                MessageFlags::ENCRYPTED,
                message.to_binary().unwrap(),
            )
            .unwrap();

        let outbound_message = message_receiver.recv_timeout(Duration::from_millis(100)).unwrap();
        assert_eq!(outbound_message.destination_node_id(), &dest_peer.node_id);
        let message_envelope: MessageEnvelope = outbound_message.message_frames().clone().try_into().unwrap();
        let message_envelope_header = message_envelope.deserialize_header().unwrap();
        assert_eq!(message_envelope_header.origin_source, node_identity.identity.public_key);
        assert_eq!(message_envelope_header.peer_source, node_identity.identity.public_key);
        assert_eq!(
            message_envelope_header.dest,
            NodeDestination::NodeId(dest_peer.node_id.clone())
        );
        assert!(message_envelope_header
            .verify_signatures(message_envelope.body_frame().clone())
            .unwrap());
        assert_eq!(message_envelope_header.flags, MessageFlags::ENCRYPTED);
        let decrypted_message = message_envelope
            .deserialize_encrypted_body(&dest_sk, &node_identity.identity.public_key)
            .unwrap();
        assert_eq!(message, decrypted_message);
    }

    #[test]
    fn test_outbound_forward() {
        let (message_sender, message_receiver) = channel::unbounded();
        let origin_node_identity = Arc::new(NodeIdentity::random_for_test(None));
        let peer_node_identity = Arc::new(NodeIdentity::random_for_test(None));
        let dest_node_identity = Arc::new(NodeIdentity::random_for_test(None));

        let net_addresses = "127.0.0.1:55445".parse::<NetAddress>().unwrap().into();
        let dest_peer = Peer::new(
            dest_node_identity.identity.public_key.clone(),
            dest_node_identity.identity.node_id.clone(),
            net_addresses,
            PeerFlags::default(),
        );

        // Setup OutboundMessageService and transmit a message to the destination
        let peer_manager = Arc::new(PeerManager::new(HMapDatabase::new()).unwrap());
        peer_manager.add_peer(dest_peer.clone()).unwrap();

        let outbound_message_service =
            OutboundMessageService::new(peer_node_identity.clone(), message_sender, peer_manager).unwrap();

        // Origin constructs MessageEnvelope
        let desire_message_body = make_test_message_frame();
        let origin_envelope = MessageEnvelope::construct(
            &origin_node_identity,
            dest_node_identity.identity.public_key.clone(),
            NodeDestination::Unknown,
            desire_message_body.clone(),
            MessageFlags::ENCRYPTED,
        )
        .unwrap();

        // Peer receives MessageEnvelope from Origin, modifies and forwards it
        let peer_envelope = MessageEnvelope::forward_construct(&peer_node_identity, origin_envelope).unwrap();

        outbound_message_service
            .forward_message(
                BroadcastStrategy::DirectNodeId(dest_node_identity.identity.node_id.clone()),
                peer_envelope,
            )
            .unwrap();

        let outbound_message = message_receiver.recv_timeout(Duration::from_millis(100)).unwrap();
        assert_eq!(outbound_message.destination_node_id(), &dest_peer.node_id);
        let message_envelope: MessageEnvelope = outbound_message.message_frames().clone().try_into().unwrap();
        let message_envelope_header = message_envelope.deserialize_header().unwrap();
        assert_eq!(
            message_envelope_header.origin_source,
            origin_node_identity.identity.public_key
        );
        assert_eq!(
            message_envelope_header.peer_source,
            peer_node_identity.identity.public_key
        );
        assert_eq!(message_envelope_header.dest, NodeDestination::Unknown);
        assert!(message_envelope_header
            .verify_signatures(message_envelope.body_frame().clone())
            .unwrap());
        assert_eq!(message_envelope_header.flags, MessageFlags::ENCRYPTED);
        assert_eq!(
            desire_message_body,
            message_envelope
                .decrypted_body_frame(
                    &dest_node_identity.secret_key,
                    &origin_node_identity.identity.public_key
                )
                .unwrap()
        );
    }
}
