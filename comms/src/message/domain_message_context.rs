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
    message::message::Message,
    outbound_message_service::outbound_message_service::OutboundMessageService,
    peer_manager::peer_manager::PeerManager,
    types::{CommsDataStore, CommsPublicKey},
};
use std::sync::Arc;
use tari_crypto::keys::PublicKey;

/// The DomainMessageContext is the container that will be dispatched to the domain handlers. It contains the received
/// message after the comms level envelope has been removed. It also has handles to the PeerManager and
/// OutboundMessageService.
#[derive(Clone)]
pub struct DomainMessageContext<PubKey> {
    pub source_node_identity: Option<PubKey>,
    pub message: Message,
    pub outbound_message_service: Arc<OutboundMessageService>,
    pub peer_manager: Arc<PeerManager<CommsPublicKey, CommsDataStore>>,
}

impl<PubKey> DomainMessageContext<PubKey>
where PubKey: PublicKey
{
    /// Construct a new DomainMessageContext that consist of the peer connection information and the received message
    /// header and body
    pub fn new(
        source_node_identity: Option<PubKey>,
        message: Message,
        outbound_message_service: Arc<OutboundMessageService>,
        peer_manager: Arc<PeerManager<CommsPublicKey, CommsDataStore>>,
    ) -> DomainMessageContext<PubKey>
    {
        DomainMessageContext {
            source_node_identity,
            message,
            outbound_message_service,
            peer_manager,
        }
    }
}
