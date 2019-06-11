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

use super::Message;
use crate::{
    message::{error::MessageError, Frame, FrameSet, MessageFlags, NodeDestination},
    peer_manager::CommsNodeIdentity,
    types::{CommsPublicKey, MESSAGE_PROTOCOL_VERSION, WIRE_PROTOCOL_VERSION},
    utils::crypto,
};

use crate::types::{CommsCipher, CommsSecretKey};
use rand::OsRng;
use serde::{Deserialize, Serialize};
use std::{convert::TryFrom, sync::Arc};
use tari_crypto::keys::DiffieHellmanSharedSecret;
use tari_utilities::{ciphers::cipher::Cipher, message_format::MessageFormat, ByteArray};

const FRAMES_PER_MESSAGE: usize = 3;

/// Represents data that every message contains.
/// As described in [RFC-0172](https://rfc.tari.com/RFC-0172_PeerToPeerMessagingProtocol.html#messaging-structure)
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct MessageEnvelopeHeader {
    pub version: u8,
    pub source: CommsPublicKey,
    pub dest: NodeDestination<CommsPublicKey>,
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

    /// Sign a message, construct a MessageEnvelopeHeader and return the resulting MessageEnvelope
    pub fn construct(
        node_identity: Arc<CommsNodeIdentity>,
        dest_public_key: CommsPublicKey,
        dest: NodeDestination<CommsPublicKey>,
        body: &Frame,
        flags: MessageFlags,
    ) -> Result<Self, MessageError>
    {
        let body = if flags.contains(MessageFlags::ENCRYPTED) {
            encrypt_envelope_body(&node_identity.secret_key, &dest_public_key, &body)?
        } else {
            body.clone()
        };

        let signature = crypto::sign(&mut OsRng::new().unwrap(), node_identity.secret_key.clone(), &body)
            .map_err(MessageError::SchnorrSignatureError)?;
        let signature = signature.to_binary().map_err(MessageError::MessageFormatError)?;

        let header = MessageEnvelopeHeader {
            version: MESSAGE_PROTOCOL_VERSION,
            source: node_identity.identity.public_key.clone(),
            dest,
            signature,
            flags,
        };

        Ok(Self::new(
            vec![WIRE_PROTOCOL_VERSION],
            header.to_binary().map_err(MessageError::MessageFormatError)?,
            body,
        ))
    }

    /// Verify that the signature provided in the message header is valid for the specified source and body of the
    /// message envelope
    pub fn verify_signature(&self) -> Result<bool, MessageError> {
        let message_envelope_header: MessageEnvelopeHeader = self.to_header()?;
        crypto::verify(
            message_envelope_header.source,
            message_envelope_header.signature,
            self.body_frame(),
        )
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
    pub fn to_header(&self) -> Result<MessageEnvelopeHeader, MessageError>
    where MessageEnvelopeHeader: MessageFormat {
        MessageEnvelopeHeader::from_binary(self.header_frame()).map_err(Into::into)
    }

    /// Returns the frame that is expected to be body frame
    pub fn body_frame(&self) -> &Frame {
        &self.frames[2]
    }

    /// Returns the Message deserialized from the body frame
    pub fn message_body(&self) -> Result<Message, MessageError> {
        Message::from_binary(self.body_frame()).map_err(Into::into)
    }

    /// Returns the decrypted and deserialized Message from the body frame
    pub fn decrypted_message_body(
        &self,
        dest_secret_key: &CommsSecretKey,
        source_public_key: &CommsPublicKey,
    ) -> Result<Message, MessageError>
    {
        let decrypted_frame = decrypted_envelope_body(&dest_secret_key, &source_public_key, self.body_frame())?;
        Message::from_binary(&decrypted_frame).map_err(Into::into)
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

/// Encrypt the message_envelope_body with the generated shared secret
fn encrypt_envelope_body(
    source_secret_key: &CommsSecretKey,
    dest_public_key: &CommsPublicKey,
    message_body: &Frame,
) -> Result<Frame, MessageError>
{
    let ecdh_shared_secret = CommsPublicKey::shared_secret(&source_secret_key, &dest_public_key).to_vec();
    CommsCipher::seal_with_integral_nonce(message_body, &ecdh_shared_secret).map_err(|e| MessageError::CipherError(e))
}

/// Decrypt the message_envelope_body with the generated shared secret
fn decrypted_envelope_body(
    dest_secret_key: &CommsSecretKey,
    source_public_key: &CommsPublicKey,
    encrypted_message_body: &Frame,
) -> Result<Frame, MessageError>
{
    let ecdh_shared_secret = CommsPublicKey::shared_secret(&dest_secret_key, &source_public_key).to_vec();
    CommsCipher::open_with_integral_nonce(encrypted_message_body, &ecdh_shared_secret)
        .map_err(|e| MessageError::CipherError(e))
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::message::{MessageEnvelopeHeader, MessageFlags, NodeDestination};
    use rand;
    use rmp_serde;
    use serde::{Deserialize, Serialize};
    use tari_crypto::{
        keys::{PublicKey, SecretKey},
        ristretto::{RistrettoPublicKey, RistrettoSecretKey},
    };

    use std::convert::TryInto;
    use tari_utilities::hex::to_hex;

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
        let header = MessageEnvelopeHeader {
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
        let deserialized: MessageEnvelopeHeader = Deserialize::deserialize(&mut de).unwrap();
        assert_eq!(deserialized, header);
    }

    #[test]
    fn construct() {
        let node_identity = CommsNodeIdentity::global().unwrap();
        let dest_secret_key = node_identity.secret_key.clone();
        let dest_public_key = node_identity.identity.public_key.clone(); // Send to self
        let message_header = "Test Message Header".as_bytes().to_vec();
        let message_body = "Test Message Body".as_bytes().to_vec();
        let message_envelope_body = Message::from_message_format(message_header, message_body.clone()).unwrap();
        let message_envelope_body_frame = message_envelope_body.to_binary().unwrap();
        let envelope = MessageEnvelope::construct(
            node_identity.clone(),
            dest_public_key.clone(),
            NodeDestination::Unknown,
            &message_envelope_body_frame,
            MessageFlags::NONE,
        )
        .unwrap();
        assert_eq!("00", to_hex(envelope.version_frame()));
        let header = MessageEnvelopeHeader::from_binary(envelope.header_frame()).unwrap();
        assert_eq!(dest_public_key, header.source);
        assert_eq!(MessageFlags::NONE, header.flags);
        assert_eq!(NodeDestination::Unknown, header.dest);
        assert_eq!(71, header.signature.len());
        assert_eq!(message_envelope_body_frame, *envelope.body_frame());
        assert!(envelope.verify_signature().unwrap());

        // Check Encrypted MessageEnvelope construction
        let envelope = MessageEnvelope::construct(
            node_identity.clone(),
            dest_public_key.clone(),
            NodeDestination::Unknown,
            &message_envelope_body_frame,
            MessageFlags::ENCRYPTED,
        )
        .unwrap();
        assert!(envelope.verify_signature().unwrap());
        assert_ne!(message_envelope_body_frame, *envelope.body_frame());
        assert_eq!(
            envelope
                .decrypted_message_body(&dest_secret_key, &node_identity.identity.public_key)
                .unwrap(),
            message_envelope_body
        );
    }
}
