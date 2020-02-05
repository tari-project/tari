// Copyright 2020, The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use crate::{
    message::{Envelope, InboundMessage},
    peer_manager::{AsyncPeerManager, NodeId, Peer, PeerManagerError},
    protocol::messaging::error::InboundMessagingError,
};
use bytes::Bytes;
use log::*;
use prost::Message;
use std::convert::TryInto;

const LOG_TARGET: &str = "comms::protocol::messaging::inbound";

pub struct InboundMessaging {
    /// Peer manager used to verify peer sending the message
    peer_manager: AsyncPeerManager,
}

impl InboundMessaging {
    pub fn new(peer_manager: AsyncPeerManager) -> Self {
        Self { peer_manager }
    }

    /// Process a single received message from its raw serialized form i.e. a FrameSet
    pub async fn process_message(
        &mut self,
        source_node_id: &NodeId,
        msg: &mut Bytes,
    ) -> Result<InboundMessage, InboundMessagingError>
    {
        let envelope = Envelope::decode(msg)?;

        if !envelope.is_valid() {
            return Err(InboundMessagingError::InvalidEnvelope);
        }

        trace!(
            target: LOG_TARGET,
            "Received message envelope version {} from NodeId={}",
            envelope.version,
            source_node_id
        );

        if !envelope.verify_signature()? {
            return Err(InboundMessagingError::InvalidMessageSignature);
        }

        let peer = self.find_known_peer(source_node_id).await?;

        let public_key = envelope.get_comms_public_key().expect("already checked");

        if peer.public_key != public_key {
            return Err(InboundMessagingError::PeerPublicKeyMismatch);
        }

        // -- Message is authenticated --
        let Envelope { header, body, .. } = envelope;
        let header = header.expect("already checked").try_into().expect("already checked");

        let inbound_message = InboundMessage::new(peer, header, body.into());

        Ok(inbound_message)
    }

    /// Check whether the the source of the message is known to our Peer Manager, if it is return the peer but otherwise
    /// we discard the message as it should be in our Peer Manager
    async fn find_known_peer(&self, source_node_id: &NodeId) -> Result<Peer, InboundMessagingError> {
        match self.peer_manager.find_by_node_id(source_node_id).await {
            Ok(peer) => Ok(peer),
            Err(PeerManagerError::PeerNotFoundError) => {
                warn!(
                    target: LOG_TARGET,
                    "Received unknown node id from peer connection. Discarding message from NodeId '{}'",
                    source_node_id
                );
                Err(InboundMessagingError::CannotFindSourcePeer)
            },
            Err(PeerManagerError::BannedPeer) => {
                warn!(
                    target: LOG_TARGET,
                    "Received banned node id from peer connection. Discarding message from NodeId '{}'", source_node_id
                );
                Err(InboundMessagingError::CannotFindSourcePeer)
            },
            Err(err) => {
                warn!(
                    target: LOG_TARGET,
                    "Peer manager failed to look up source node id because '{}'", err
                );
                Err(InboundMessagingError::PeerManagerError(err))
            },
        }
    }
}
