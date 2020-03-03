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
    message::{MessageFlags, MessageTag},
    peer_manager::NodeId,
};
use bytes::Bytes;
use std::{
    fmt,
    fmt::{Error, Formatter},
};

/// Contains details required to build a message envelope and send a message to a peer. OutboundMessage will not copy
/// the body bytes when cloned and is 'cheap to clone(tm)'.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutboundMessage {
    pub tag: MessageTag,
    pub peer_node_id: NodeId,
    pub flags: MessageFlags,
    pub body: Bytes,
}

impl OutboundMessage {
    /// Create a new OutboundMessage
    pub fn new(peer_node_id: NodeId, flags: MessageFlags, body: Bytes) -> OutboundMessage {
        Self::with_tag(MessageTag::new(), peer_node_id, flags, body)
    }

    /// Create a new OutboundMessage with the specified MessageTag
    pub fn with_tag(tag: MessageTag, peer_node_id: NodeId, flags: MessageFlags, body: Bytes) -> OutboundMessage {
        OutboundMessage {
            tag,
            peer_node_id,
            flags,
            body,
        }
    }
}

impl fmt::Display for OutboundMessage {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(
            f,
            "OutboundMessage (tag: {}, {} bytes) for peer '{}'",
            self.tag,
            self.body.len(),
            self.peer_node_id.short_str()
        )
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn with_tag() {
        static TEST_MSG: Bytes = Bytes::from_static(b"The ghost brigades");
        let node_id = NodeId::new();
        let tag = MessageTag::new();
        let subject = OutboundMessage::with_tag(tag, node_id.clone(), MessageFlags::empty(), TEST_MSG.clone());
        assert_eq!(tag, subject.tag);
        assert_eq!(subject.body, TEST_MSG);
        assert_eq!(subject.peer_node_id, node_id);
    }
}
