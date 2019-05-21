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

use crate::inbound_message_service::{
    comms_msg_handlers::determine_comms_msg_dispatch_type,
    message_dispatcher::Dispatchable,
};
use serde_derive::{Deserialize, Serialize};
use std::convert::TryFrom;
use tari_comms::connection::message::{FrameSet, MessageEnvelopeHeader, MessageError};
use tari_crypto::keys::PublicKey;

/// The number of frames required to construct a MessageContext
const FRAMES_PER_MESSAGE: usize = 5;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct MessageContext<PubKey> {
    pub connection_id: Vec<u8>,
    pub source: Vec<u8>,
    pub version: Vec<u8>,
    pub node_identity: Option<PubKey>,
    pub message_envelope_header: MessageEnvelopeHeader<PubKey>,
    pub message_envelope_body: Vec<u8>,
}

impl<PubKey: PublicKey + 'static> MessageContext<PubKey> {
    /// Construct a new MessageContext that consist of the peer connection information and the received message header
    /// and body
    pub fn new(
        connection_id: Vec<u8>,
        source: Vec<u8>,
        version: Vec<u8>,
        node_identity: Option<PubKey>,
        message_envelope_header: MessageEnvelopeHeader<PubKey>,
        message_envelope_body: Vec<u8>,
    ) -> MessageContext<PubKey>
    {
        MessageContext {
            connection_id,
            source,
            version,
            node_identity,
            message_envelope_header,
            message_envelope_body,
        }
    }

    /// Serialize the MessageContext into a FrameSet
    pub fn to_frame_set(&self) -> Result<FrameSet, MessageError> {
        let mut frame_set: Vec<Vec<u8>> = Vec::new();
        frame_set.push(self.connection_id.clone());
        frame_set.push(self.source.clone());
        frame_set.push(self.version.clone());
        // node identity should be excluded
        frame_set.push(self.message_envelope_header.to_frame()?);
        frame_set.push(self.message_envelope_body.clone());
        Ok(frame_set)
    }
}

impl<PubKey: PublicKey> TryFrom<FrameSet> for MessageContext<PubKey> {
    type Error = MessageError;

    /// Attempt to create a MessageContext from a FrameSet
    fn try_from(mut frames: FrameSet) -> Result<Self, Self::Error> {
        if frames.len() == FRAMES_PER_MESSAGE {
            Ok(MessageContext {
                message_envelope_body: frames.remove(4),
                message_envelope_header: MessageEnvelopeHeader::try_from(frames.remove(3))?,
                version: frames.remove(2),
                node_identity: None,
                source: frames.remove(1),
                connection_id: frames.remove(0),
            })
        } else {
            Err(MessageError::MalformedMultipart)
        }
    }
}

impl<PubKey: PublicKey> Dispatchable for MessageContext<PubKey> {
    fn dispatch_type(&self) -> u32 {
        determine_comms_msg_dispatch_type(&self)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use tari_comms::connection::message::{IdentityFlags, MessageEnvelopeHeader, NodeDestination};
    use tari_crypto::{
        keys::{PublicKey, SecretKey},
        ristretto::{RistrettoPublicKey, RistrettoSecretKey},
    };

    #[test]
    fn test_try_from_and_to_frame_set() {
        // Create a new Message Context
        let mut rng = rand::OsRng::new().unwrap();
        let connection_id: Vec<u8> = vec![0, 1, 2, 3, 4];
        let source: Vec<u8> = vec![5, 6, 7, 8, 9];
        let version: Vec<u8> = vec![10];
        let dest: NodeDestination<RistrettoPublicKey> = NodeDestination::Unknown;
        let message_envelope_header: MessageEnvelopeHeader<RistrettoPublicKey> = MessageEnvelopeHeader {
            version: 0,
            source: RistrettoPublicKey::from_secret_key(&RistrettoSecretKey::random(&mut rng)),
            dest,
            signature: vec![0],
            flags: IdentityFlags::ENCRYPTED,
        };
        let message_envelope_body: Vec<u8> = vec![11, 12, 13, 14, 15];
        let desired_message_context = MessageContext::<RistrettoPublicKey>::new(
            connection_id,
            source,
            version,
            None,
            message_envelope_header,
            message_envelope_body,
        );
        // Convert MessageContext to FrameSet
        let message_context_buffer = desired_message_context.to_frame_set().unwrap();
        // Create MessageContext from FrameSet
        let message_context: Result<MessageContext<RistrettoPublicKey>, MessageError> =
            MessageContext::try_from(message_context_buffer);
        assert_eq!(desired_message_context, message_context.unwrap());
    }
}
