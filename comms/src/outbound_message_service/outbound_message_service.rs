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
    connection::{
        types::Linger,
        zmq::{InprocAddress, ZmqContext},
        Connection,
        Direction,
        SocketEstablishment,
    },
    message::{Frame, Message, MessageEnvelope, MessageError, MessageFlags, NodeDestination},
    peer_manager::{peer_manager::PeerManager, NodeIdentity},
};
use std::{convert::TryInto, sync::Arc};
use tari_utilities::message_format::MessageFormat;

/// Handler functions use the OutboundMessageService to send messages to peers. The OutboundMessage service will receive
/// messages from handlers, apply a broadcasting strategy, encrypted and serialized the messages into OutboundMessages
/// and write them to the outbound message pool.
pub struct OutboundMessageService {
    context: ZmqContext,
    outbound_address: InprocAddress,
    node_identity: Arc<NodeIdentity>,
    peer_manager: Arc<PeerManager>,
}

impl OutboundMessageService {
    /// Constructs a new OutboundMessageService from the context, node_identity and outbound_address
    pub fn new(
        context: ZmqContext,
        node_identity: Arc<NodeIdentity>,
        outbound_address: InprocAddress, /* The outbound_address is an inproc that connects the OutboundMessagePool
                                          * and the OutboundMessageService */
        peer_manager: Arc<PeerManager>,
    ) -> Result<OutboundMessageService, OutboundError>
    {
        Ok(OutboundMessageService {
            context,
            outbound_address,
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
        // Send message to outbound message pool
        let outbound_connection = Connection::new(&self.context, Direction::Outbound)
            .set_name("OMS to OMP")
            .set_linger(Linger::Timeout(5000))
            .set_socket_establishment(SocketEstablishment::Connect)
            .establish(&self.outbound_address)
            .map_err(|e| OutboundError::ConnectionError(e))?;

        // Use the BroadcastStrategy to select appropriate peer(s) from PeerManager and then construct and send a
        // personalised message to each selected peer
        let mut selected_node_identities = self.peer_manager.get_broadcast_identities(broadcast_strategy)?;

        // Constructing a MessageEnvelope for each recipient
        for dest_node_identity in selected_node_identities.drain(..) {
            let message_envelope = MessageEnvelope::construct(
                &self.node_identity,
                dest_node_identity.public_key.clone(),
                NodeDestination::NodeId(dest_node_identity.node_id.clone()),
                message_envelope_body.clone(),
                flags,
            )
            .map_err(|e| OutboundError::MessageSerializationError(e))?;

            let msg = OutboundMessage::new(dest_node_identity.node_id, message_envelope.into_frame_set());
            let msg_buffer = msg.to_binary().map_err(OutboundError::MessageFormatError)?;

            outbound_connection
                .send(&[msg_buffer])
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
        types::CommsPublicKey,
    };
    use log::*;
    use std::path::PathBuf;
    use tari_crypto::{keys::PublicKey, ristretto::RistrettoPublicKey};
    use tari_storage::lmdb_store::{LMDBBuilder, LMDBError, LMDBStore};

    pub fn init() {
        let _ = simple_logger::init();
    }

    fn get_path(name: &str) -> String {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("tests/data");
        path.push(name);
        path.to_str().unwrap().to_string()
    }

    fn init_datastore(name: &str) -> Result<LMDBStore, LMDBError> {
        let path = get_path(name);
        let _ = std::fs::create_dir(&path).unwrap_or_default();
        LMDBBuilder::new()
            .set_path(&path)
            .set_environment_size(10)
            .set_max_number_of_databases(2)
            .add_database(name, lmdb_zero::db::CREATE)
            .build()
    }

    fn clean_up_datastore(name: &str) {
        std::fs::remove_dir_all(get_path(name)).unwrap();
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
        let database_name = "oms_test_outbound_send"; // Note: every test should have unique database
        let datastore = init_datastore(database_name).unwrap();
        let peer_database = datastore.get_handle(database_name).unwrap();
        let peer_manager = Arc::new(PeerManager::new(peer_database).unwrap());
        peer_manager.add_peer(dest_peer.clone()).unwrap();

        let outbound_message_service =
            OutboundMessageService::new(context, node_identity.clone(), outbound_address, peer_manager).unwrap();

        // Construct and send OutboundMessage
        let message_header = "Test Message Header".as_bytes().to_vec();
        let message_body = "Test Message Body".as_bytes().to_vec();
        let message_envelope_body = Message::from_message_format(message_header, message_body).unwrap();
        outbound_message_service
            .send_raw(
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
        let outbound_message = OutboundMessage::from_binary(&msg_bytes[0]).unwrap();
        assert_eq!(outbound_message.destination_node_id(), &dest_peer.node_id);
        assert_eq!(outbound_message.num_attempts(), 0);
        assert_eq!(outbound_message.is_scheduled(), true);
        let message_envelope: MessageEnvelope = outbound_message.message_frames().clone().try_into().unwrap();
        let message_envelope_header = message_envelope.to_header::<CommsPublicKey>().unwrap();
        assert_eq!(message_envelope_header.source, node_identity.identity.public_key);
        assert_eq!(
            message_envelope_header.dest,
            NodeDestination::NodeId(dest_peer.node_id.clone())
        );
        assert!(message_envelope.verify_signature().unwrap());
        assert_eq!(message_envelope_header.flags, MessageFlags::ENCRYPTED);
        let decoded_message_envelope_body = message_envelope
            .decrypted_message_body(&dest_sk, &node_identity.identity.public_key)
            .unwrap();
        assert_eq!(message_envelope_body, decoded_message_envelope_body);

        clean_up_datastore(database_name);
    }
}
