// Copyright 2019, The Tari Project
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

use super::{MessageError, MessageFlags};
use crate::{
    consts::ENVELOPE_VERSION,
    types::{CommsPublicKey, CommsSecretKey},
    utils::signature,
};
use bytes::Bytes;
use rand::rngs::OsRng;
use std::convert::TryInto;
use tari_crypto::tari_utilities::{message_format::MessageFormat, ByteArray};

// Re-export protos
pub use crate::proto::envelope::*;

/// Represents data that every message contains.
/// As described in [RFC-0172](https://rfc.tari.com/RFC-0172_PeerToPeerMessagingProtocol.html#messaging-structure)
#[derive(Clone, Debug, PartialEq)]
pub struct MessageEnvelopeHeader {
    pub public_key: CommsPublicKey,
    pub signature: Bytes,
    pub flags: MessageFlags,
}

impl Envelope {
    /// Sign a message, construct an Envelope with a Header
    pub fn construct_signed(
        secret_key: &CommsSecretKey,
        public_key: &CommsPublicKey,
        body: Bytes,
        flags: MessageFlags,
    ) -> Result<Self, MessageError>
    {
        // Sign this body
        let header_signature = {
            let sig =
                signature::sign(&mut OsRng, secret_key.clone(), &body).map_err(MessageError::SchnorrSignatureError)?;
            sig.to_binary().map_err(MessageError::MessageFormatError)
        }?;

        Ok(Envelope {
            version: ENVELOPE_VERSION,
            header: Some(EnvelopeHeader {
                public_key: public_key.to_vec(),
                signature: header_signature,
                flags: flags.bits(),
            }),
            body: body.to_vec(),
        })
    }

    /// Verify that the signature provided is valid for the given body
    pub fn verify_signature(&self) -> Result<bool, MessageError> {
        match self
            .header
            .as_ref()
            .map(|header| (header, header.get_comms_public_key()))
        {
            Some((header, Some(public_key))) => signature::verify(&public_key, &header.signature, &self.body),
            _ => Ok(false),
        }
    }

    /// Returns true if the message contains a valid public key in the header, otherwise
    /// false
    pub fn is_valid(&self) -> bool {
        self.get_comms_public_key().is_some()
    }

    /// Returns a valid public key from the header of this envelope, or None if the
    /// public key is invalid
    pub fn get_comms_public_key(&self) -> Option<CommsPublicKey> {
        self.header.as_ref().and_then(|header| header.get_comms_public_key())
    }
}

impl EnvelopeHeader {
    pub fn get_comms_public_key(&self) -> Option<CommsPublicKey> {
        CommsPublicKey::from_bytes(&self.public_key).ok()
    }
}

impl TryInto<MessageEnvelopeHeader> for EnvelopeHeader {
    type Error = MessageError;

    fn try_into(self) -> Result<MessageEnvelopeHeader, Self::Error> {
        Ok(MessageEnvelopeHeader {
            public_key: self
                .get_comms_public_key()
                .ok_or(MessageError::InvalidHeaderPublicKey)?,
            signature: self.signature.into(),
            flags: MessageFlags::from_bits_truncate(self.flags),
        })
    }
}

/// Wraps a number of `prost::Message`s in a EnvelopeBody
#[macro_export]
macro_rules! wrap_in_envelope_body {
    ($($e:expr),+) => {{
        use $crate::message::MessageExt;
        let mut envelope_body = $crate::message::EnvelopeBody::new();
        let mut error = None;
        $(
            match $e.to_encoded_bytes() {
                Ok(bytes) => envelope_body.push_part(bytes),
                Err(err) => {
                    if error.is_none() {
                        error = Some(err);
                    }
                }
            }
        )*

        match error {
            Some(err) => Err(err),
            None => Ok(envelope_body),
        }
    }}
}

impl EnvelopeBody {
    pub fn new() -> Self {
        Self {
            parts: Default::default(),
        }
    }

    pub fn len(&self) -> usize {
        self.parts.len()
    }

    /// Removes and returns the part at the given index. None
    /// is returned if the index is out of bounds
    pub fn take_part(&mut self, index: usize) -> Option<Vec<u8>> {
        Some(index)
            .filter(|i| self.parts.len() > *i)
            // remove panics if out of bounds
            .and_then(|i| Some(self.parts.remove(i)))
    }

    pub fn push_part(&mut self, part: Vec<u8>) {
        self.parts.push(part)
    }

    pub fn into_inner(self) -> Vec<Vec<u8>> {
        self.parts
    }

    /// Decodes a part of the message body and returns the result. If the part index is out of range Ok(None) is
    /// returned
    pub fn decode_part<T>(&self, index: usize) -> Result<Option<T>, MessageError>
    where T: prost::Message + Default {
        match self.parts.get(index) {
            Some(part) => T::decode(part.as_slice()).map(Some).map_err(Into::into),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::message::MessageFlags;
    use rand::rngs::OsRng;
    use tari_crypto::keys::PublicKey;

    #[test]
    fn construct_signed() {
        let (sk, pk) = CommsPublicKey::random_keypair(&mut OsRng);
        let envelope = Envelope::construct_signed(&sk, &pk, Bytes::new(), MessageFlags::all()).unwrap();
        assert_eq!(envelope.get_comms_public_key().unwrap(), pk);
        assert!(envelope.verify_signature().unwrap());
    }

    #[test]
    fn header_try_into() {
        let header = EnvelopeHeader {
            public_key: CommsPublicKey::default().to_vec(),
            flags: MessageFlags::all().bits(),
            signature: vec![1, 2, 3],
        };

        let msg_header: MessageEnvelopeHeader = header.try_into().unwrap();
        assert_eq!(msg_header.public_key, CommsPublicKey::default());
        assert_eq!(msg_header.flags, MessageFlags::all());
        assert_eq!(msg_header.signature, vec![1, 2, 3]);
    }

    #[test]
    fn is_valid() {
        let (sk, pk) = CommsPublicKey::random_keypair(&mut OsRng);
        let mut envelope = Envelope::construct_signed(&sk, &pk, Bytes::new(), MessageFlags::all()).unwrap();
        assert_eq!(envelope.is_valid(), true);
        envelope.header = None;
        assert_eq!(envelope.is_valid(), false);
    }
}
