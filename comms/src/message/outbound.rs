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

use crate::{message::MessageTag, peer_manager::NodeId, protocol::messaging::SendFailReason};
use bytes::Bytes;
use futures::channel::oneshot;
use std::{
    fmt,
    fmt::{Error, Formatter},
};

pub type MessagingReplyResult = Result<(), SendFailReason>;
pub type MessagingReplyRx = oneshot::Receiver<MessagingReplyResult>;

/// Contains details required to build a message envelope and send a message to a peer. OutboundMessage will not copy
/// the body bytes when cloned and is 'cheap to clone(tm)'.
#[derive(Debug)]
pub struct OutboundMessage {
    pub tag: MessageTag,
    pub peer_node_id: NodeId,
    pub body: Bytes,
    pub reply: MessagingReplyTx,
}

impl OutboundMessage {
    pub fn new(peer_node_id: NodeId, body: Bytes) -> Self {
        Self {
            tag: MessageTag::new(),
            peer_node_id,
            body,
            reply: MessagingReplyTx::none(),
        }
    }

    pub fn with_reply(peer_node_id: NodeId, body: Bytes, reply: MessagingReplyTx) -> Self {
        Self {
            tag: MessageTag::new(),
            peer_node_id,
            body,
            reply,
        }
    }

    #[inline]
    pub fn reply_success(&mut self) {
        self.reply.reply_success();
    }

    pub fn reply_fail(&mut self, reason: SendFailReason) {
        self.reply.reply_fail(reason);
    }

    pub fn take_reply(&mut self) -> Option<MessagingReplyTx> {
        self.reply.take()
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

/// Wrapper struct for a oneshot reply sender. When this struct is dropped, an automatic fail is sent on the oneshot if
/// a response has not already been sent.
#[derive(Debug)]
pub struct MessagingReplyTx(Option<oneshot::Sender<MessagingReplyResult>>);

impl MessagingReplyTx {
    pub fn into_inner(mut self) -> Option<oneshot::Sender<MessagingReplyResult>> {
        self.0.take()
    }

    pub fn none() -> Self {
        Self(None)
    }

    pub fn reply_success(&mut self) {
        if let Some(reply_tx) = self.0.take() {
            let _ = reply_tx.send(Ok(()));
        }
    }

    pub fn reply_fail(&mut self, reason: SendFailReason) {
        if let Some(reply_tx) = self.0.take() {
            let _ = reply_tx.send(Err(reason));
        }
    }

    pub fn take(&mut self) -> Option<Self> {
        self.0.take().map(Into::into)
    }
}

impl From<oneshot::Sender<MessagingReplyResult>> for MessagingReplyTx {
    fn from(inner: oneshot::Sender<MessagingReplyResult>) -> Self {
        Self(Some(inner))
    }
}
impl From<Option<oneshot::Sender<MessagingReplyResult>>> for MessagingReplyTx {
    fn from(inner: Option<oneshot::Sender<MessagingReplyResult>>) -> Self {
        Self(inner)
    }
}

impl Drop for MessagingReplyTx {
    fn drop(&mut self) {
        // If this is dropped and the reply tx has not been used already, send an error reply
        if let Some(reply_tx) = self.0.take() {
            let _ = reply_tx.send(Err(SendFailReason::Dropped));
        }
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
        let subject = OutboundMessage {
            tag,
            peer_node_id: node_id.clone(),
            reply: MessagingReplyTx::none(),
            body: TEST_MSG.clone(),
        };
        assert_eq!(tag, subject.tag);
        assert_eq!(subject.body, TEST_MSG);
        assert_eq!(subject.peer_node_id, node_id);
    }
}
