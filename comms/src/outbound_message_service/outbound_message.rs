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
    message::{Frame, FrameSet, MessageError},
    peer_manager::node_id::NodeId,
};
use chrono::prelude::*;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::convert::TryFrom;
use tari_utilities::message_format::MessageFormat;

/// The OutboundMessage has a copy of the MessageEnvelope and tracks the number of send attempts, the creation
/// timestamp and the retry timestamp. The OutboundMessageService will create the OutboundMessage and forward it to
/// the outbound message pool. The OutboundMessages can then be retrieved from the pool by the ConnectionManager so they
/// can be sent to the peer destinations.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct OutboundMessage<T> {
    pub destination_node_id: NodeId,
    retry_count: u32,
    creation_timestamp: DateTime<Utc>,
    last_retry_timestamp: Option<DateTime<Utc>>,
    pub message_envelope: T,
}

impl<T> OutboundMessage<T>
where
    T: MessageFormat,
    Self: MessageFormat,
{
    /// Create a new OutboundMessage from the destination_node_id and message_envelope
    pub fn new(destination_node_id: NodeId, message_envelope: T) -> OutboundMessage<T> {
        OutboundMessage {
            destination_node_id,
            retry_count: 0,
            creation_timestamp: Utc::now(),
            last_retry_timestamp: None,
            message_envelope,
        }
    }

    /// Serialize an OutboundMessage into a single frame
    pub fn into_frame(self) -> Result<Frame, MessageError> {
        self.to_binary().map_err(MessageError::MessageFormatError)
    }

    /// Update the retry count and retry timestamp after a transmission attempt
    pub fn mark_transmission_attempt(&mut self) {
        self.retry_count += 1;
        self.last_retry_timestamp = Some(Utc::now());
    }

    pub fn number_of_retries(&self) -> u32 {
        self.retry_count
    }

    pub fn last_retry_timestamp(&self) -> Option<DateTime<Utc>> {
        self.last_retry_timestamp
    }
}

// impl<T: MessageFormat> TryFrom<Frame> for OutboundMessage<T> {
//    type Error = MessageError;
//
//    /// Construct an OutboundMessage from a Frame
//    fn try_from(frame: Frame) -> Result<Self, Self::Error> {
//        Self::from_binary(frame.as_slice())
//            .map_err(MessageError::MessageFormatError)
//    }
//}
//
impl<T: Serialize + DeserializeOwned> TryFrom<FrameSet> for OutboundMessage<T> {
    type Error = MessageError;

    /// Construct an OutboundMessage from a Frame
    fn try_from(frames: FrameSet) -> Result<Self, Self::Error> {
        if frames.len() == 1 {
            OutboundMessage::from_binary(frames[0].as_slice()).map_err(MessageError::MessageFormatError)
        } else {
            Err(MessageError::DeserializeFailed)
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use tari_crypto::{keys::PublicKey, ristretto::RistrettoPublicKey};

    #[test]
    fn test_outbound_message() {
        let mut rng = rand::OsRng::new().unwrap();
        let (_sk, pk) = RistrettoPublicKey::random_keypair(&mut rng);
        let destination_node_id = NodeId::from_key(&pk).unwrap();
        let message_envelope: Frame = vec![0, 1, 2, 3, 4];
        let mut desired_outbound_message = OutboundMessage::new(destination_node_id, message_envelope);
        // Test transmission attempts
        desired_outbound_message.mark_transmission_attempt();
        desired_outbound_message.mark_transmission_attempt();
        assert_eq!(desired_outbound_message.retry_count, 2);
        assert!(desired_outbound_message.last_retry_timestamp.is_some());
        // Test serialization and deserialization
        let msg_frame_result = desired_outbound_message.clone().into_frame();
        assert!(msg_frame_result.is_ok());
        let outbound_message_result = OutboundMessage::from_binary(&msg_frame_result.unwrap());
        assert!(outbound_message_result.is_ok());
        assert_eq!(desired_outbound_message, outbound_message_result.unwrap());
    }
}
