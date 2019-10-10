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
    consts::COMMS_RNG,
    message::{error::MessageError, Frame, FrameSet, Message, MessageFlags},
    peer_manager::NodeIdentity,
    types::{CommsPublicKey, CommsSecretKey, MESSAGE_PROTOCOL_VERSION, WIRE_PROTOCOL_VERSION},
    utils::{crypt, signature},
};
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;
use tari_utilities::message_format::MessageFormat;

const FRAMES_PER_MESSAGE: usize = 3;

/// Generate a signature for the peer that confirms the origin_source and body
fn peer_signature(secret_key: CommsSecretKey, body: &Vec<u8>) -> Result<Vec<u8>, MessageError> {
    COMMS_RNG.with(|rng| {
        let peer_signature =
            signature::sign(&mut *rng.borrow_mut(), secret_key, body).map_err(MessageError::SchnorrSignatureError)?;
        peer_signature.to_binary().map_err(MessageError::MessageFormatError)
    })
}

/// Represents data that every message contains.
/// As described in [RFC-0172](https://rfc.tari.com/RFC-0172_PeerToPeerMessagingProtocol.html#messaging-structure)
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct MessageEnvelopeHeader {
    pub version: u8,
    pub message_public_key: CommsPublicKey,
    pub message_signature: Vec<u8>,
    pub flags: MessageFlags,
}

impl MessageEnvelopeHeader {
    /// Verify that the signature provided is valid for the given body
    pub fn verify_signature(&self, body: &Vec<u8>) -> Result<bool, MessageError> {
        signature::verify(&self.message_public_key, self.message_signature.as_slice(), &body)
    }
}

/// Represents a message which is about to go on or has just come off the wire.
/// As described in [RFC-0172](https://rfc.tari.com/RFC-0172_PeerToPeerMessagingProtocol.html#messaging-structure)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MessageEnvelope {
    frames: FrameSet,
}

impl MessageEnvelope {
    /// Create a new MessageEnvelope from four frames
    pub fn new(header: Frame, body: Frame) -> Self {
        MessageEnvelope {
            frames: vec![vec![WIRE_PROTOCOL_VERSION], header, body],
        }
    }

    /// Sign a message, construct a MessageEnvelopeHeader and return the resulting MessageEnvelope
    pub fn construct(
        node_identity: &NodeIdentity,
        dest_public_key: CommsPublicKey,
        mut body: Frame,
        flags: MessageFlags,
    ) -> Result<Self, MessageError>
    {
        if flags.contains(MessageFlags::ENCRYPTED) {
            let shared_key = crypt::generate_ecdh_secret(&node_identity.secret_key, &dest_public_key);
            body = crypt::encrypt(&shared_key, &body)?;
        }

        let peer_signature = peer_signature(node_identity.secret_key.clone(), &body)?;

        let header = MessageEnvelopeHeader {
            version: MESSAGE_PROTOCOL_VERSION,
            message_public_key: node_identity.identity.public_key.clone(),
            message_signature: peer_signature,
            flags,
        };

        Ok(Self::new(
            header.to_binary().map_err(MessageError::MessageFormatError)?,
            body,
        ))
    }

    /// Modify and sign a forwarded MessageEnvelope
    pub fn forward_construct(
        secret_key: CommsSecretKey,
        public_key: CommsPublicKey,
        message_envelope: MessageEnvelope,
    ) -> Result<Self, MessageError>
    {
        let mut message_envelope_header = message_envelope.deserialize_header()?;
        message_envelope_header.message_public_key = public_key;
        message_envelope_header.message_signature = peer_signature(secret_key, &message_envelope.body_frame())?;

        Ok(Self::new(
            message_envelope_header
                .to_binary()
                .map_err(MessageError::MessageFormatError)?,
            message_envelope.body_frame().clone(),
        ))
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
    pub fn deserialize_header(&self) -> Result<MessageEnvelopeHeader, MessageError> {
        MessageEnvelopeHeader::from_binary(self.header_frame()).map_err(Into::into)
    }

    /// Returns the frame that is expected to be body frame
    pub fn body_frame(&self) -> &Frame {
        &self.frames[2]
    }

    /// Consumes the envelope and returns the body frame
    pub fn into_body_frame(mut self) -> Frame {
        self.frames.remove(2)
    }

    /// Returns the version of this envelope
    pub fn version(&self) -> u8 {
        let frame = self.version_frame();
        // If version not specified, it's assumed to be 0
        if frame.len() == 0 {
            return 0;
        }
        frame[0]
    }

    /// Returns a decrypted version of the body frame
    pub fn decrypted_body_frame(
        &self,
        secret_key: &CommsSecretKey,
        source_public_key: &CommsPublicKey,
    ) -> Result<Frame, MessageError>
    {
        let ecdh_shared_secret = crypt::generate_ecdh_secret(secret_key, source_public_key);
        let decrypted_frame =
            crypt::decrypt(&ecdh_shared_secret, self.body_frame()).map_err(MessageError::CipherError)?;
        Ok(decrypted_frame)
    }

    /// Returns the Message deserialized from the body frame
    //    pub fn deserialize_body(&self) -> Result<Message, MessageError> {
    //        Message::from_binary(self.body_frame()).map_err(Into::into)
    //    }

    /// Returns the decrypted and deserialized Message from the body frame
    pub fn deserialize_encrypted_body(
        &self,
        secret_key: &CommsSecretKey,
        source_public_key: &CommsPublicKey,
    ) -> Result<Message, MessageError>
    {
        Message::from_binary(&self.decrypted_body_frame(secret_key, source_public_key)?).map_err(Into::into)
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

//#[cfg(test)]
// mod test {
//    use super::*;
//    use crate::message::{MessageEnvelopeHeader, MessageFlags, NodeDestination};
//    use rand;
//    use std::{convert::TryInto, sync::Arc};
//    use tari_crypto::{keys::PublicKey, ristretto::RistrettoPublicKey};
//    use tari_utilities::hex::to_hex;
//
//    #[test]
//    fn try_from_valid() {
//        let example = vec![vec![1u8], vec![2u8], vec![3u8]];
//
//        let raw_message: Result<MessageEnvelope, MessageError> = example.try_into();
//
//        assert!(raw_message.is_ok());
//        let envelope = raw_message.unwrap();
//        assert_eq!(envelope.version_frame(), &[1u8]);
//        assert_eq!(envelope.header_frame(), &[2u8]);
//        assert_eq!(envelope.body_frame(), &[3u8]);
//    }
//
//    #[test]
//    fn try_from_invalid() {
//        let example = vec![vec![1u8], vec![2u8]];
//
//        let raw_message: Result<MessageEnvelope, MessageError> = example.try_into();
//
//        assert!(raw_message.is_err());
//        let error = raw_message.err().unwrap();
//        match error {
//            MessageError::MalformedMultipart => {},
//            _ => panic!("Unexpected MessageError {:?}", error),
//        }
//    }
//
//    #[test]
//    fn header() {
//        let (_sk, pk) = RistrettoPublicKey::random_keypair(&mut rand::OsRng::new().unwrap());
//        let header = MessageEnvelopeHeader {
//            version: 0,
//            peer_pubkey: pk,
//            destination: NodeDestination::Unknown,
//            peer_signature: vec![0],
//            flags: MessageFlags::ENCRYPTED,
//        };
//
//        let envelope = MessageEnvelope::new(vec![0u8], header.to_binary().unwrap(), vec![0u8]);
//
//        assert_eq!(header, envelope.deserialize_header().unwrap());
//    }
//
//    fn make_test_message_frame() -> Frame {
//        let message_header = "Test Message Header".as_bytes().to_vec();
//        let message_body = "Test Message Body".as_bytes().to_vec();
//        let message_envelope_body = Message::from_message_format(message_header, message_body).unwrap();
//        message_envelope_body.to_binary().unwrap()
//    }
//
//    #[test]
//    fn construct() {
//        let node_identity = Arc::new(NodeIdentity::random_for_test(None));
//        let dest_public_key = &node_identity.identity.public_key;
//
//        let message_envelope_body_frame = make_test_message_frame();
//
//        let envelope = MessageEnvelope::construct(
//            &node_identity,
//            dest_public_key.clone(),
//            NodeDestination::Unknown,
//            message_envelope_body_frame.clone(),
//            MessageFlags::NONE,
//        )
//        .unwrap();
//        assert_eq!("00", to_hex(envelope.version_frame()));
//        let header = MessageEnvelopeHeader::from_binary(envelope.header_frame()).unwrap();
//        assert_eq!(MessageFlags::NONE, header.flags);
//        assert_eq!(NodeDestination::Unknown, header.destination);
//        assert_eq!(&message_envelope_body_frame, envelope.body_frame());
//    }
//
//    #[test]
//    fn forward_construct() {
//        let origin_node_identity = Arc::new(NodeIdentity::random_for_test(None));
//        let peer_node_identity = Arc::new(NodeIdentity::random_for_test(None));
//        let dest_node_identity = Arc::new(NodeIdentity::random_for_test(None));
//
//        // Original MessageEnvelope
//        let message_envelope_body_frame = make_test_message_frame();
//        let origin_envelope = MessageEnvelope::construct(
//            &origin_node_identity,
//            dest_node_identity.identity.public_key.clone(),
//            NodeDestination::Unknown,
//            message_envelope_body_frame.clone(),
//            MessageFlags::ENCRYPTED,
//        )
//        .unwrap();
//
//        // Forwarded MessageEnvelope
//        let peer_envelope = MessageEnvelope::forward_construct(
//            origin_node_identity.secret_key.clone(),
//            peer_node_identity.identity.public_key.clone(),
//            origin_envelope,
//        )
//        .unwrap();
//        let peer_header = MessageEnvelopeHeader::from_binary(peer_envelope.header_frame()).unwrap();
//
//        assert_eq!(peer_header.peer_pubkey, peer_node_identity.identity.public_key);
//        assert_eq!(
//            peer_envelope
//                .decrypted_body_frame(
//                    &dest_node_identity.secret_key,
//                    &origin_node_identity.identity.public_key
//                )
//                .unwrap(),
//            message_envelope_body_frame
//        );
//        assert!(peer_header
//            .verify_signatures(peer_envelope.body_frame().clone())
//            .unwrap());
//    }
//
//    #[test]
//    fn construct_encrypted() {
//        let node_identity = Arc::new(NodeIdentity::random_for_test(None));
//        let dest_public_key = &node_identity.identity.public_key;
//
//        let message_envelope_body_frame = make_test_message_frame();
//        let envelope = MessageEnvelope::construct(
//            &node_identity,
//            dest_public_key.clone(),
//            NodeDestination::Unknown,
//            message_envelope_body_frame.clone(),
//            MessageFlags::ENCRYPTED,
//        )
//        .unwrap();
//
//        assert_ne!(&message_envelope_body_frame, envelope.body_frame());
//    }
//
//    #[test]
//    fn envelope_decrypt_message_body() {
//        let node_identity = Arc::new(NodeIdentity::random_for_test(None));
//        let dest_secret_key = node_identity.secret_key.clone();
//        let dest_public_key = &node_identity.identity.public_key;
//
//        let message_envelope_body_frame = make_test_message_frame();
//        let envelope = MessageEnvelope::construct(
//            &node_identity,
//            dest_public_key.clone(),
//            NodeDestination::Unknown,
//            message_envelope_body_frame.clone(),
//            MessageFlags::ENCRYPTED,
//        )
//        .unwrap();
//
//        assert_eq!(
//            envelope
//                .deserialize_encrypted_body(&dest_secret_key, &node_identity.identity.public_key)
//                .unwrap(),
//            Message::from_binary(&message_envelope_body_frame).unwrap()
//        );
//    }
//
//    #[test]
//    fn message_header_verify_signature() {
//        let node_identity = Arc::new(NodeIdentity::random_for_test(None));
//        let dest_public_key = &node_identity.identity.public_key;
//
//        let message_envelope_body_frame = make_test_message_frame();
//        let envelope = MessageEnvelope::construct(
//            &node_identity,
//            dest_public_key.clone(),
//            NodeDestination::Unknown,
//            message_envelope_body_frame.clone(),
//            MessageFlags::NONE,
//        )
//        .unwrap();
//
//        let header = envelope.deserialize_header().unwrap();
//        let mut body = envelope.body_frame().clone();
//        assert!(header.verify_signatures(body.clone()).unwrap());
//
//        body.push(0);
//        assert!(!header.verify_signatures(body.clone()).unwrap());
//    }
//}
