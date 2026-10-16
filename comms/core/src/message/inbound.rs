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

use std::sync::Arc;

use bytes::Bytes;
use tokio::sync::OwnedSemaphorePermit;

use super::MessageTag;
use crate::peer_manager::NodeId;

/// Authenticated inbound message
#[derive(Clone, Debug)]
pub struct InboundMessage {
    pub tag: MessageTag,
    /// The connected peer which sent this message
    pub source_peer: NodeId,
    /// The raw message envelope
    pub body: Bytes,
    /// One of the in-flight slots of the messaging session that received this message. The session reads no further
    /// frames while all of its slots are held. The slot is held until the consumer has finished processing the message
    /// and the last clone of it is dropped - not when the message is taken off the inbound channel. The DHT
    /// deserializer destructures the message with `..`, so the slot stays in the partly moved message until the whole
    /// DHT pipeline call for it has finished, which caps each session at a few concurrent pipeline tasks.
    inflight_permit: Option<Arc<OwnedSemaphorePermit>>,
}

impl InboundMessage {
    /// Construct a new InboundMessage
    pub fn new(source_peer: NodeId, body: Bytes) -> Self {
        Self {
            tag: MessageTag::new(),
            source_peer,
            body,
            inflight_permit: None,
        }
    }

    /// Attaches an in-flight slot of the receiving messaging session, which is held until this message (and every
    /// clone of it) is dropped, i.e. until the consumer has finished processing it
    pub(crate) fn with_inflight_permit(mut self, permit: OwnedSemaphorePermit) -> Self {
        self.inflight_permit = Some(Arc::new(permit));
        self
    }
}
