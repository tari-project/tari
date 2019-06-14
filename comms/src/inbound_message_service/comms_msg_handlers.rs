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
    dispatcher::{DispatchError, DispatchResolver, DispatchableKey},
    message::{DomainMessageContext, Message, MessageContext, MessageEnvelopeHeader, MessageFlags, NodeDestination},
    peer_manager::node_identity::CommsNodeIdentity,
    types::{CommsPublicKey, MessageDispatcher},
};
use tari_crypto::keys::PublicKey;

/// The comms_msg_dispatcher will determine the type of message and forward it to the the correct handler
#[derive(Eq, PartialEq, Hash, Clone, Debug)]
pub enum CommsDispatchType {
    // Messages of this type must be handled
    Handle,
    // Messages of this type must be forwarded to peers
    Forward,
    // Messages of this type can be ignored and discarded
    Discard,
}

/// Specify what handler function should be called for messages with different comms level dispatch types
pub fn construct_comms_msg_dispatcher<PubKey, DispKey, DispRes>(
) -> MessageDispatcher<MessageContext<PubKey, DispKey, DispRes>>
where
    PubKey: PublicKey,
    DispKey: DispatchableKey,
    DispRes: DispatchResolver<DispKey, DomainMessageContext<PubKey>>,
{
    MessageDispatcher::new(InboundMessageServiceResolver {})
        .route(CommsDispatchType::Handle, handler_handle)
        .route(CommsDispatchType::Forward, handler_forward)
        .route(CommsDispatchType::Discard, handler_discard)
}

#[derive(Clone)]
pub struct InboundMessageServiceResolver;

impl<PubKey, DispKey, DispRes> DispatchResolver<CommsDispatchType, MessageContext<PubKey, DispKey, DispRes>>
    for InboundMessageServiceResolver
where
    PubKey: PublicKey,
    DispKey: DispatchableKey,
    DispRes: DispatchResolver<DispKey, DomainMessageContext<PubKey>>,
{
    /// The dispatch type is determined from the content of the MessageContext, which is used to dispatch the message to
    /// the correct handler
    fn resolve(
        &self,
        message_context: &MessageContext<PubKey, DispKey, DispRes>,
    ) -> Result<CommsDispatchType, DispatchError>
    {
        // Verify source node message signature
        if !message_context
            .message_data
            .message_envelope
            .verify_signature()
            .map_err(|e| DispatchError::HandlerError(format!("{}", e)))?
        {
            return Ok(CommsDispatchType::Discard);
        }
        // Check destination of message
        let message_envelope_header: MessageEnvelopeHeader = message_context
            .message_data
            .message_envelope
            .to_header()
            .map_err(|e| DispatchError::HandlerError(format!("{}", e)))?;
        let node_identity =
            CommsNodeIdentity::global().ok_or(DispatchError::HandlerError("identity issue".to_string()))?;
        match message_envelope_header.dest {
            NodeDestination::<CommsPublicKey>::Unknown => Ok(CommsDispatchType::Handle),
            NodeDestination::<CommsPublicKey>::PublicKey(dest_public_key) => {
                if node_identity.identity.public_key == dest_public_key {
                    Ok(CommsDispatchType::Handle)
                } else {
                    Ok(CommsDispatchType::Forward)
                }
            },
            NodeDestination::<CommsPublicKey>::NodeId(dest_node_id) => {
                if node_identity.identity.node_id == dest_node_id {
                    Ok(CommsDispatchType::Handle)
                } else {
                    Ok(CommsDispatchType::Forward)
                }
            },
        }
    }
}

fn handler_handle<PubKey, DispKey, DispRes>(
    message_context: MessageContext<PubKey, DispKey, DispRes>,
) -> Result<(), DispatchError>
where
    PubKey: PublicKey,
    DispKey: DispatchableKey,
    DispRes: DispatchResolver<DispKey, DomainMessageContext<PubKey>>,
{
    // Check encryption and retrieved Message
    let message_envelope_header: MessageEnvelopeHeader = message_context
        .message_data
        .message_envelope
        .to_header()
        .map_err(|e| DispatchError::HandlerError(format!("{}", e)))?;
    let node_identity =
        CommsNodeIdentity::global().ok_or(DispatchError::HandlerError("Node Identity not set".to_string()))?;
    let message: Message;
    if message_envelope_header.flags.contains(MessageFlags::ENCRYPTED) {
        match message_context
            .message_data
            .message_envelope
            .decrypted_message_body(&node_identity.secret_key, &message_envelope_header.source)
        {
            Ok(decrypted_message_body) => {
                message = decrypted_message_body;
            },
            Err(_) => {
                if message_envelope_header.dest == NodeDestination::<CommsPublicKey>::Unknown {
                    // Message might have been for this node if it could have been decrypted
                    return handler_forward(message_context);
                } else {
                    // Message was for this node but could not be decrypted
                    return handler_discard(message_context);
                }
            },
        }
    } else {
        message = message_context
            .message_data
            .message_envelope
            .message_body()
            .map_err(|e| DispatchError::HandlerError(format!("{}", e)))?;
    };
    // Construct DomainMessageContext and dispatch using domain dispatcher
    let domain_message_context = DomainMessageContext::new(
        message_context.message_data.source_node_identity,
        message,
        message_context.outbound_message_service,
        message_context.peer_manager,
    );
    message_context.domain_dispatcher.dispatch(domain_message_context)
}

fn handler_forward<PubKey, DispKey, DispRes>(
    _message_context: MessageContext<PubKey, DispKey, DispRes>,
) -> Result<(), DispatchError>
where
    PubKey: PublicKey,
    DispKey: DispatchableKey,
    DispRes: DispatchResolver<DispKey, DomainMessageContext<PubKey>>,
{
    // TODO: Add logic for message forwarding

    Ok(())
}

fn handler_discard<PubKey, DispKey, DispRes>(
    _message_context: MessageContext<PubKey, DispKey, DispRes>,
) -> Result<(), DispatchError>
where
    PubKey: PublicKey,
    DispKey: DispatchableKey,
    DispRes: DispatchResolver<DispKey, DomainMessageContext<PubKey>>,
{
    // TODO: Add logic for discarding a message

    Ok(())
}
