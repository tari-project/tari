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
    message::{DomainMessageContext, MessageContext},
    types::MessageDispatcher,
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
    fn resolve(&self, _msg: &MessageContext<PubKey, DispKey, DispRes>) -> Result<CommsDispatchType, DispatchError> {
        // TODO:
        // if signature is correct {
        //     if message_context.message_envelope_header.flags.contains(IdentityFlags::ENCRYPTED)  {
        //         if msg can be decrypted {
        //             if msg meant for curr node {
        //                 DispatchType::Handle
        //             }
        //             else {
        //                 CommsDispatchType::Forward
        //             }
        //         }
        //         else {
        //             CommsDispatchType::Forward
        //         }
        //     }
        //     else {
        //         CommsDispatchType::Handle
        //     }
        // }
        // else {
        //     CommsDispatchType::Discard
        // }

        Ok(CommsDispatchType::Handle)
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
    // Change the MessageContext into the DomainMessageContext and submit to the domain dispatcher
    let message = message_context
        .message_data
        .message_envelope
        .message_body()
        .map_err(|e| DispatchError::HandlerError(format!("{}", e)))?;
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
