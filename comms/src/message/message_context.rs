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
    dispatcher::DispatchableKey,
    inbound_message_service::inbound_message_publisher::InboundMessagePublisher,
    message::{InboundMessage, MessageEnvelope},
    outbound_message_service::outbound_message_service::OutboundMessageService,
    peer_manager::{peer_manager::PeerManager, NodeIdentity, Peer},
};
use serde::{de::DeserializeOwned, Serialize};
use std::{
    fmt::Debug,
    sync::{Arc, RwLock},
};

#[derive(Clone)]
pub struct MessageContext<MType>
where MType: Send + Sync + Debug
{
    pub forwardable: bool,
    pub message_envelope: MessageEnvelope,
    pub peer: Peer,
    pub outbound_message_service: Arc<OutboundMessageService>,
    pub peer_manager: Arc<PeerManager>,
    pub inbound_message_publisher: Arc<RwLock<InboundMessagePublisher<MType, InboundMessage>>>,
    pub node_identity: Arc<NodeIdentity>,
}

impl<MType> MessageContext<MType>
where
    MType: DispatchableKey,
    MType: Serialize + DeserializeOwned,
    MType: Debug,
{
    /// Construct a new MessageContext that consist of the peer connection information and the received message header
    /// and body
    pub fn new(
        node_identity: Arc<NodeIdentity>,
        peer: Peer,
        forwardable: bool,
        message_envelope: MessageEnvelope,
        outbound_message_service: Arc<OutboundMessageService>,
        peer_manager: Arc<PeerManager>,
        inbound_message_publisher: Arc<RwLock<InboundMessagePublisher<MType, InboundMessage>>>,
    ) -> Self
    {
        MessageContext {
            forwardable,
            message_envelope,
            peer,
            node_identity,
            outbound_message_service,
            peer_manager,
            inbound_message_publisher,
        }
    }
}
