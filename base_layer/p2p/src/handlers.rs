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

use crate::tari_message::{BlockchainMessage, NetMessage, PeerMessage, TariMessageType, ValidatorNodeMessage};
use tari_comms::{
    dispatcher::{DispatchError, DispatchResolver},
    message::{DomainMessageContext, MessageHeader},
    types::DomainMessageDispatcher,
};
use tari_crypto::keys::PublicKey;

// Create a common variable that the worker can change and the test can read to determine that the message was
// correctly dispatched
#[cfg(test)]
static mut TEST_CALLED_FN_TYPE: u8 = 0;

#[derive(Clone)]
pub struct DomainInboundMessageResolver;

impl<PubKey: PublicKey> DispatchResolver<TariMessageType, DomainMessageContext<PubKey>>
    for DomainInboundMessageResolver
{
    /// The dispatch type is determined from the content of the DomainMessageContext, which is used to dispatch the
    /// message to the correct domain handler
    fn resolve(&self, message_context: &DomainMessageContext<PubKey>) -> Result<TariMessageType, DispatchError> {
        let message_header: MessageHeader<TariMessageType> = message_context
            .message
            .to_header()
            .map_err(|err| DispatchError::HandlerError(format!("{}", err)))?;
        Ok(message_header.message_type)
    }
}

/// Specify what handler function should be called for messages with different domain message types
pub fn construct_domain_msg_dispatcher<PubKey: PublicKey>(
) -> DomainMessageDispatcher<PubKey, TariMessageType, DomainInboundMessageResolver> {
    DomainMessageDispatcher::<PubKey, TariMessageType, DomainInboundMessageResolver>::new(
        DomainInboundMessageResolver {},
    )
    .route(NetMessage::Join.into(), handler_net_message_join)
    .route(NetMessage::Discover.into(), handler_net_message_discover)
    .route(PeerMessage::Connect.into(), handler_peer_message_connect)
    .route(BlockchainMessage::NewBlock.into(), handler_blockchain_message_new_block)
    .route(
        ValidatorNodeMessage::Instruction.into(),
        handler_validator_node_message_instruction,
    )
    .catch_all(handler_catch_all)
}

fn handler_net_message_join<PubKey: PublicKey>(
    _message_context: DomainMessageContext<PubKey>,
) -> Result<(), DispatchError> {
    #[cfg(test)]
    {
        unsafe {
            TEST_CALLED_FN_TYPE = NetMessage::Join;
        }
    }

    // TODO: Add logic

    Ok(())
}

fn handler_net_message_discover<PubKey: PublicKey>(
    _message_context: DomainMessageContext<PubKey>,
) -> Result<(), DispatchError> {
    #[cfg(test)]
    {
        unsafe {
            TEST_CALLED_FN_TYPE = NetMessage::Discover;
        }
    }

    // TODO: Add logic

    Ok(())
}

fn handler_peer_message_connect<PubKey: PublicKey>(
    _message_context: DomainMessageContext<PubKey>,
) -> Result<(), DispatchError> {
    #[cfg(test)]
    {
        unsafe {
            TEST_CALLED_FN_TYPE = PeerMessage::Connect;
        }
    }

    // TODO: Add logic

    Ok(())
}

fn handler_blockchain_message_new_block<PubKey: PublicKey>(
    _message_context: DomainMessageContext<PubKey>,
) -> Result<(), DispatchError> {
    #[cfg(test)]
    {
        unsafe {
            TEST_CALLED_FN_TYPE = BlockchainMessage::NewBlock;
        }
    }

    // TODO: Add logic

    Ok(())
}

fn handler_validator_node_message_instruction<PubKey: PublicKey>(
    _message_context: DomainMessageContext<PubKey>,
) -> Result<(), DispatchError> {
    #[cfg(test)]
    {
        unsafe {
            TEST_CALLED_FN_TYPE = ValidatorNodeMessage::Instruction;
        }
    }

    // TODO: Add logic

    Ok(())
}

fn handler_catch_all<PubKey: PublicKey>(_message_context: DomainMessageContext<PubKey>) -> Result<(), DispatchError> {
    #[cfg(test)]
    {
        unsafe {
            TEST_CALLED_FN_TYPE = 0;
        }
    }

    // TODO: Add logic

    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::tari_message::TariMessageType;
    use std::{convert::TryInto, sync::Arc, thread, time::Duration};
    use tari_comms::{
        connection::{
            types::SocketType,
            zmq::{InprocAddress, ZmqEndpoint},
            EstablishedConnection,
            NetAddress,
            ZmqContext,
        },
        inbound_message_service::{
            comms_msg_handlers::construct_comms_msg_dispatcher,
            inbound_message_service::InboundMessageService,
        },
        message::{Message, MessageData, MessageEnvelope, MessageFlags, NodeDestination},
        outbound_message_service::outbound_message_service::OutboundMessageService,
        peer_manager::{node_id::NodeId, peer_manager::PeerManager, CommsNodeIdentity, NodeIdentity, PeerNodeIdentity},
        types::{CommsDataStore, CommsPublicKey},
    };
    use tari_crypto::{
        keys::PublicKey,
        ristretto::{RistrettoPublicKey, RistrettoSecretKey},
    };
    use tari_utilities::{byte_array::ByteArray, message_format::MessageFormat};

    fn pause() {
        thread::sleep(Duration::from_millis(5));
    }

    fn init_node_identity() {
        let secret_key = RistrettoSecretKey::from_bytes(&[
            1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ])
        .unwrap();
        let public_key = RistrettoPublicKey::from_secret_key(&secret_key);
        let node_id = NodeId::from_key(&public_key).unwrap();
        NodeIdentity::<RistrettoPublicKey>::set_global(CommsNodeIdentity {
            identity: PeerNodeIdentity::new(node_id, public_key),
            secret_key,
            control_service_address: "127.0.0.1:9090".parse::<NetAddress>().unwrap(),
        });
    }

    #[test]
    fn test_handlers() {
        // Setup Comms system
        init_node_identity();
        let node_identity = CommsNodeIdentity::global().unwrap();
        let context = ZmqContext::new();
        let inbound_msg_pool_address = InprocAddress::random();
        // Create a conn_client that can submit messages to the InboundMessageService
        let client_socket = context.socket(SocketType::Request).unwrap();
        client_socket
            .connect(&inbound_msg_pool_address.to_zmq_endpoint())
            .unwrap();
        let conn_client: EstablishedConnection = client_socket.try_into().unwrap();
        // Setup Dispatchers, InboundMessageService and OutboundMessageService
        let domain_dispatcher = Arc::new(construct_domain_msg_dispatcher::<RistrettoPublicKey>());
        let comms_dispatcher = Arc::new(construct_comms_msg_dispatcher::<
            RistrettoPublicKey,
            TariMessageType,
            DomainInboundMessageResolver,
        >());
        let peer_manager = Arc::new(PeerManager::<CommsPublicKey, CommsDataStore>::new(None).unwrap());
        let outbound_message_service = Arc::new(
            OutboundMessageService::new(context.clone(), InprocAddress::random(), peer_manager.clone()).unwrap(),
        );
        let inbound_message_service = InboundMessageService::new(
            context,
            inbound_msg_pool_address,
            comms_dispatcher,
            domain_dispatcher,
            outbound_message_service,
            peer_manager,
        )
        .unwrap();
        inbound_message_service.start();

        // Create and send unencrypted message
        let message_type = TariMessageType::new(NetMessage::Discover);
        let message_header = MessageHeader {
            message_type: message_type.clone(),
        };
        let message_body = "Test Message Body1".as_bytes().to_vec();
        let message_envelope_body = Message::from_message_format(message_header, message_body).unwrap();
        let dest_public_key = node_identity.identity.public_key.clone(); // Send to self
        let dest_node_id = node_identity.identity.node_id.clone(); // Send to self
        let message_envelope = MessageEnvelope::construct(
            node_identity.clone(),
            dest_public_key.clone(),
            NodeDestination::Unknown,
            &message_envelope_body.to_binary().unwrap(),
            MessageFlags::NONE,
        )
        .unwrap();
        let connection_id = vec![0, 1, 2, 3, 4];
        let message_data = MessageData::<RistrettoPublicKey>::new(connection_id.clone(), None, message_envelope);
        // Submit Message to the InboundMessageService
        pause();
        conn_client.send(&message_data.into_frame_set()).unwrap();
        conn_client.receive(2000).unwrap();
        pause();
        unsafe {
            assert_eq!(TEST_CALLED_FN_TYPE, message_type.value());
        }

        // Create and send encrypted message
        let message_type = TariMessageType::new(BlockchainMessage::NewBlock);
        let message_header = MessageHeader {
            message_type: message_type.clone(),
        };
        let message_body = "Test Message Body2".as_bytes().to_vec();
        let message_envelope_body = Message::from_message_format(message_header, message_body).unwrap();
        let message_envelope = MessageEnvelope::construct(
            node_identity,
            dest_public_key,
            NodeDestination::NodeId(dest_node_id),
            &message_envelope_body.to_binary().unwrap(),
            MessageFlags::ENCRYPTED,
        )
        .unwrap();
        let message_data = MessageData::<RistrettoPublicKey>::new(connection_id, None, message_envelope);
        // Submit Message to the InboundMessageService
        pause();
        conn_client.send(&message_data.into_frame_set()).unwrap();
        conn_client.receive(2000).unwrap();
        pause();
        unsafe {
            assert_eq!(TEST_CALLED_FN_TYPE, message_type.value());
        }
    }
}
