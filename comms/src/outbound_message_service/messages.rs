// Copyright 2019 The Tari Project
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

use crate::{consts::COMMS_RNG, message::MessageFlags, peer_manager::node_id::NodeId};
use rand::Rng;

/// Represents a tag for a message
#[derive(Clone, Debug, Copy, PartialEq, Eq)]
pub struct MessageTag(u64);

impl MessageTag {
    pub fn new() -> Self {
        COMMS_RNG.with(|rng| Self(rng.borrow_mut().gen()))
    }
}

/// The OutboundMessage has a copy of the MessageEnvelope. OutboundMessageService will create the
/// OutboundMessage and forward it to the OutboundMessagePool.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutboundMessage {
    pub tag: MessageTag,
    pub peer_node_id: NodeId,
    pub flags: MessageFlags,
    pub body: Vec<u8>,
}

impl OutboundMessage {
    /// Create a new OutboundMessage from the destination_node_id and message_frames
    pub fn new(peer_node_id: NodeId, flags: MessageFlags, body: Vec<u8>) -> OutboundMessage {
        Self::with_tag(MessageTag::new(), peer_node_id, flags, body)
    }

    /// Create a new OutboundMessage from the destination_node_id and message_frames
    pub fn with_tag(tag: MessageTag, peer_node_id: NodeId, flags: MessageFlags, body: Vec<u8>) -> OutboundMessage {
        OutboundMessage {
            tag,
            peer_node_id,
            flags,
            body,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn with_tag() {
        let node_id = NodeId::new();
        let tag = MessageTag::new();
        let subject = OutboundMessage::with_tag(tag, node_id.clone(), MessageFlags::empty(), vec![1]);
        assert_eq!(tag, subject.tag);
        assert_eq!(subject.body, vec![1]);
        assert_eq!(subject.peer_node_id, node_id);
    }
}
