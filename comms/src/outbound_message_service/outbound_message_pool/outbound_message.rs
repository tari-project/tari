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

use crate::{message::FrameSet, peer_manager::node_id::NodeId};
use serde::{Deserialize, Serialize};

/// The OutboundMessage has a copy of the MessageEnvelope. OutboundMessageService will create the
/// OutboundMessage and forward it to the OutboundMessagePool.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct OutboundMessage {
    destination_node_id: NodeId,
    message_frames: FrameSet,
}

impl OutboundMessage {
    /// Create a new OutboundMessage from the destination_node_id and message_frames
    pub fn new(destination: NodeId, message_frames: FrameSet) -> OutboundMessage {
        OutboundMessage {
            destination_node_id: destination,
            message_frames,
        }
    }

    /// Get a reference to the destination NodeID
    pub fn destination_node_id(&self) -> &NodeId {
        &self.destination_node_id
    }

    /// Get a reference to the message frames
    pub fn message_frames(&self) -> &FrameSet {
        &self.message_frames
    }

    /// Consume this wrapper and return ownership of the frames
    pub fn into_frames(self) -> FrameSet {
        self.message_frames
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn new() {
        let node_id = NodeId::new();
        let subject = OutboundMessage::new(node_id.clone(), vec![vec![1]]);
        assert_eq!(subject.message_frames[0].len(), 1);
        assert_eq!(subject.destination_node_id, node_id);
    }

    #[test]
    fn getters() {
        let node_id = NodeId::new();
        let frames = vec![vec![1]];
        let subject = OutboundMessage::new(node_id.clone(), frames.clone());

        assert_eq!(subject.destination_node_id(), &node_id);
        assert_eq!(subject.message_frames(), &frames);
        assert_eq!(subject.into_frames(), frames);
    }
}
