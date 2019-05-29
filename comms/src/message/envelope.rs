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
    connection::{Frame, FrameSet},
    message::{error::MessageError, MessageFlags, NodeDestination},
};

use crate::message::Message;
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;
use tari_crypto::keys::PublicKey;
use tari_utilities::message_format::MessageFormat;

const FRAMES_PER_MESSAGE: usize = 3;

/// Represents data that every message contains.
/// As described in [RFC-0172](https://rfc.tari.com/RFC-0172_PeerToPeerMessagingProtocol.html#messaging-structure)
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct MessageEnvelopeHeader<P> {
    pub version: u8,
    pub source: P,
    pub dest: NodeDestination<P>,
    pub signature: Vec<u8>,
    pub flags: MessageFlags,
}

/// Represents a message which is about to go on or has just come off the wire.
/// As described in [RFC-0172](https://rfc.tari.com/RFC-0172_PeerToPeerMessagingProtocol.html#messaging-structure)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MessageEnvelope {
    frames: FrameSet,
}

impl MessageEnvelope {
    /// Create a new MessageEnvelope from four frames
    pub fn new(version: Frame, header: Frame, body: Frame) -> Self {
        MessageEnvelope {
            frames: vec![version, header, body],
        }
    }

    /// Returns the frame that is expected to be version frame
    pub fn version_frame(&self) -> &Frame {
        &self.frames[0]
    }

    /// Returns the frame that is expected to be header frame
    pub fn header_frame(&self) -> &Frame {
        &self.frames[1]
    }

    /// Returns the [MessageEnvelopeHeader] deserialized from the header frame
    pub fn to_header<P: PublicKey>(&self) -> Result<MessageEnvelopeHeader<P>, MessageError>
    where MessageEnvelopeHeader<P>: MessageFormat {
        MessageEnvelopeHeader::<P>::from_binary(self.header_frame()).map_err(Into::into)
    }

    /// Returns the frame that is expected to be body frame
    pub fn body_frame(&self) -> &Frame {
        &self.frames[2]
    }

    /// Returns the Message deserialized from the body frame
    pub fn message_body(&self) -> Result<Message, MessageError> {
        Message::from_binary(self.body_frame()).map_err(Into::into)
    }

    /// This struct is consumed and the contained FrameSet is returned.
    pub fn into_frame_set(self) -> FrameSet {
        self.frames
    }
}

impl TryFrom<FrameSet> for MessageEnvelope {
    type Error = MessageError;

    /// Returns a MessageEnvelope from a FrameSet
    fn try_from(frames: FrameSet) -> Result<Self, Self::Error> {
        if frames.len() != FRAMES_PER_MESSAGE {
            return Err(MessageError::MalformedMultipart);
        }

        Ok(MessageEnvelope { frames })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::message::{MessageEnvelopeHeader, MessageFlags, NodeDestination};
    use rand;
    use rmp_serde;
    use serde::{Deserialize, Serialize};
    use tari_crypto::{
        keys::SecretKey,
        ristretto::{RistrettoPublicKey, RistrettoSecretKey},
    };

    use std::convert::TryInto;

    #[test]
    fn try_from_valid() {
        let example = vec![vec![1u8], vec![2u8], vec![3u8]];

        let raw_message: Result<MessageEnvelope, MessageError> = example.try_into();

        assert!(raw_message.is_ok());
        let envelope = raw_message.unwrap();
        assert_eq!(envelope.version_frame(), &[1u8]);
        assert_eq!(envelope.header_frame(), &[2u8]);
        assert_eq!(envelope.body_frame(), &[3u8]);
    }

    #[test]
    fn try_from_invalid() {
        let example = vec![vec![1u8], vec![2u8]];

        let raw_message: Result<MessageEnvelope, MessageError> = example.try_into();

        assert!(raw_message.is_err());
        let error = raw_message.err().unwrap();
        match error {
            MessageError::MalformedMultipart => {},
            _ => panic!("Unexpected MessageError {:?}", error),
        }
    }

    #[test]
    fn header() {
        let (_sk, pk) = RistrettoPublicKey::random_keypair(&mut rand::OsRng::new().unwrap());
        let header = MessageEnvelopeHeader {
            version: 0,
            source: pk,
            dest: NodeDestination::Unknown,
            signature: vec![0],
            flags: MessageFlags::ENCRYPTED,
        };

        let envelope = MessageEnvelope::new(vec![0u8], header.to_binary().unwrap(), vec![0u8]);

        assert_eq!(header, envelope.to_header().unwrap());
    }

    #[test]
    fn test_ser_des() {
        let version = 0;
        let mut rng = rand::OsRng::new().unwrap();
        let k = RistrettoSecretKey::random(&mut rng);
        let p = RistrettoPublicKey::from_secret_key(&k);
        let source = p;
        let dest: NodeDestination<RistrettoPublicKey> = NodeDestination::Unknown;
        let signature = vec![0];
        let flags = MessageFlags::ENCRYPTED;
        let header: MessageEnvelopeHeader<RistrettoPublicKey> = MessageEnvelopeHeader {
            version,
            source,
            dest,
            signature,
            flags,
        };

        let mut buf = Vec::new();
        header.serialize(&mut rmp_serde::Serializer::new(&mut buf)).unwrap();
        let serialized = buf.to_vec();
        let mut de = rmp_serde::Deserializer::new(serialized.as_slice());
        let deserialized: MessageEnvelopeHeader<RistrettoPublicKey> = Deserialize::deserialize(&mut de).unwrap();
        assert_eq!(deserialized, header);
    }
}
