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
    connection::FrameSet,
    message::{MessageEnvelope, MessageError},
};
use serde_derive::{Deserialize, Serialize};
use std::convert::{TryFrom, TryInto};
use tari_crypto::keys::PublicKey;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct MessageContext<PubKey> {
    pub connection_id: Vec<u8>,
    pub node_identity: Option<PubKey>,
    pub message_envelope: MessageEnvelope,
}

impl<PubKey: PublicKey + 'static> MessageContext<PubKey> {
    /// Construct a new MessageContext that consist of the peer connection information and the received message header
    /// and body
    pub fn new(
        connection_id: Vec<u8>,
        node_identity: Option<PubKey>,
        message_envelope: MessageEnvelope,
    ) -> MessageContext<PubKey>
    {
        MessageContext {
            connection_id,
            node_identity,
            message_envelope,
        }
    }

    /// Convert the MessageContext into a FrameSet
    pub fn into_frame_set(self) -> FrameSet {
        let mut frame_set = Vec::new();
        frame_set.push(self.connection_id.clone());
        frame_set.extend(self.message_envelope.into_frame_set());
        frame_set
    }
}

impl<PubKey: PublicKey> TryFrom<FrameSet> for MessageContext<PubKey> {
    type Error = MessageError;

    /// Attempt to create a MessageContext from a FrameSet
    fn try_from(mut frames: FrameSet) -> Result<Self, Self::Error> {
        let connection_id = if frames.len() > 0 {
            // `remove` panics if the index is out of bounds, so we have to check
            frames.remove(0)
        } else {
            return Err(MessageError::MalformedMultipart);
        };

        let message_envelope: MessageEnvelope = frames.try_into()?;

        Ok(MessageContext {
            message_envelope,
            node_identity: None,
            connection_id,
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::connection::Frame;
    use std::convert::TryInto;
    use tari_crypto::ristretto::RistrettoPublicKey;

    #[test]
    fn test_try_from_and_to_frame_set() {
        let connection_id: Frame = vec![0, 1, 2, 3, 4];
        let header_frame: Frame = vec![11, 12, 13, 14, 15];
        let body_frame: Frame = vec![11, 12, 13, 14, 15];
        let version_frame: Frame = vec![10];

        // Frames received off the "wire"
        let frames = vec![
            connection_id.clone(),
            version_frame.clone(),
            header_frame.clone(),
            body_frame.clone(),
        ];

        // Convert to MessageContext
        let message_context: MessageContext<RistrettoPublicKey> = frames.try_into().unwrap();

        let message_envelope = MessageEnvelope::new(version_frame, header_frame, body_frame);

        let expected_message_context = MessageContext::<RistrettoPublicKey>::new(connection_id, None, message_envelope);

        assert_eq!(expected_message_context, message_context);

        // Convert MessageContext to FrameSet
        let message_context_buffer = expected_message_context.clone().into_frame_set();
        // Create MessageContext from FrameSet
        let message_context: Result<MessageContext<RistrettoPublicKey>, MessageError> =
            MessageContext::try_from(message_context_buffer);
        assert_eq!(expected_message_context, message_context.unwrap());
    }
}
