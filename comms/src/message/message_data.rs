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

use crate::message::{FrameSet, MessageEnvelope, MessageError};
use serde_derive::{Deserialize, Serialize};
use std::convert::{TryFrom, TryInto};
use tari_crypto::keys::PublicKey;

/// Messages submitted to the inbound message pool are of type MessageData. This struct contains the received message
/// envelope from a peer, its node identity and the connection id associated with the received message.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct MessageData<PubKey> {
    pub connection_id: Vec<u8>,
    pub source_node_identity: Option<PubKey>,
    pub message_envelope: MessageEnvelope,
}

impl<PubKey> MessageData<PubKey>
where PubKey: PublicKey + 'static
{
    /// Construct a new MessageData that consist of the peer connection information and the received message envelope
    /// header and body
    pub fn new(
        connection_id: Vec<u8>,
        source_node_identity: Option<PubKey>,
        message_envelope: MessageEnvelope,
    ) -> MessageData<PubKey>
    {
        MessageData {
            connection_id,
            source_node_identity,
            message_envelope,
        }
    }

    /// Convert the MessageData into a FrameSet
    pub fn into_frame_set(self) -> FrameSet {
        let mut frame_set = Vec::new();
        frame_set.push(self.connection_id.clone());
        frame_set.extend(self.message_envelope.into_frame_set());
        frame_set
    }
}

impl<PubKey: PublicKey> TryFrom<FrameSet> for MessageData<PubKey> {
    type Error = MessageError;

    /// Attempt to create a MessageData from a FrameSet
    fn try_from(mut frames: FrameSet) -> Result<Self, Self::Error> {
        let connection_id = if frames.len() > 0 {
            // `remove` panics if the index is out of bounds, so we have to check
            frames.remove(0)
        } else {
            return Err(MessageError::MalformedMultipart);
        };

        let message_envelope: MessageEnvelope = frames.try_into()?;

        Ok(MessageData {
            message_envelope,
            source_node_identity: None,
            connection_id,
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::message::Frame;
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

        // Convert to MessageData
        let message_data: MessageData<RistrettoPublicKey> = frames.try_into().unwrap();

        let message_envelope = MessageEnvelope::new(version_frame, header_frame, body_frame);

        let expected_message_data = MessageData::<RistrettoPublicKey>::new(connection_id, None, message_envelope);

        assert_eq!(expected_message_data, message_data);

        // Convert MessageData to FrameSet
        let message_data_buffer = expected_message_data.clone().into_frame_set();
        // Create MessageData from FrameSet
        let message_data: Result<MessageData<RistrettoPublicKey>, MessageError> =
            MessageData::try_from(message_data_buffer);
        assert_eq!(expected_message_data, message_data.unwrap());
    }
}
