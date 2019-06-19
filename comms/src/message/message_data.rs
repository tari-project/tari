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
use tari_utilities::message_format::MessageFormat;

/// Messages submitted to the inbound message pool are of type MessageData. This struct contains the received message
/// envelope from a peer, its node identity and the connection id associated with the received message.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct MessageData<PubKey> {
    pub connection_id: Vec<u8>,
    pub source_node_identity: PubKey,
    pub message_envelope: MessageEnvelope,
}

impl<PubKey> MessageData<PubKey>
where PubKey: PublicKey + 'static
{
    /// Construct a new MessageData that consist of the peer connection information and the received message envelope
    /// header and body
    pub fn new(
        connection_id: Vec<u8>,
        source_node_identity: PubKey,
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
    pub fn try_into_frame_set(self) -> Result<FrameSet, MessageError> {
        let mut frame_set = Vec::new();
        frame_set.push(self.connection_id.clone());
        frame_set.push(
            self.source_node_identity
                .to_binary()
                .map_err(|_| MessageError::BinarySerializeError)?,
        );
        frame_set.extend(self.message_envelope.into_frame_set());
        Ok(frame_set)
    }
}

impl<PubKey: PublicKey> TryFrom<FrameSet> for MessageData<PubKey> {
    type Error = MessageError;

    /// Attempt to create a MessageData from a FrameSet
    fn try_from(mut frames: FrameSet) -> Result<Self, Self::Error> {
        if frames.len() < 5 {
            return Err(MessageError::MalformedMultipart);
        };
        let connection_id = frames.remove(0);
        let source_node_identity =
            PubKey::from_binary(&frames.remove(0)).map_err(|_| MessageError::BinaryDeserializeError)?;
        let message_envelope: MessageEnvelope = frames.try_into()?;
        Ok(MessageData {
            message_envelope,
            source_node_identity,
            connection_id,
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::message::Frame;
    use tari_crypto::ristretto::RistrettoPublicKey;

    #[test]
    fn test_try_from_and_try_into() {
        let mut rng = rand::OsRng::new().unwrap();
        let (_, source_node_identity) = RistrettoPublicKey::random_keypair(&mut rng);
        let version_frame: Frame = vec![10];
        let header_frame: Frame = vec![0, 1, 2, 3, 4];
        let body_frame: Frame = vec![5, 6, 7, 8, 9];
        let message_envelope = MessageEnvelope::new(version_frame, header_frame, body_frame);
        let connection_id = vec![10, 11, 12, 13, 14];
        let expected_message_data =
            MessageData::<RistrettoPublicKey>::new(connection_id.clone(), source_node_identity, message_envelope);
        // Convert MessageData to FrameSet
        let message_data_buffer = expected_message_data.clone().try_into_frame_set().unwrap();
        // Create MessageData from FrameSet
        let message_data: MessageData<RistrettoPublicKey> = MessageData::try_from(message_data_buffer).unwrap();
        assert_eq!(expected_message_data, message_data);
    }
}
