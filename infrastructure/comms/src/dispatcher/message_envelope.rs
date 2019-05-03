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

use crate::connection::{message::RawDataMessage, net_address::*};

use crate::peer_manager::node_id::*;
use bitflags::*;
use derive_error::Error;
use serde::{Deserialize, Serialize};
use serde_derive::{Deserialize, Serialize};
use std::convert::TryFrom;
use tari_crypto::keys::PublicKey;


#[derive(Debug, Error)]
pub enum MessageEnvelopeError {
    // An error occurred serialising an object into binary
    BinarySerializeError(rmp_serde::encode::Error),
    // An error occurred deserialising binary data into an object
    BinaryDeserializeError(rmp_serde::decode::Error),
}

bitflags! {
    #[derive(Deserialize, Serialize)]
    struct IdentityFlags: u8 {
        const ENCRYPTED = 0b00000001;
    }
}

pub enum NodeDestination<PubKey>
where PubKey: PublicKey
{
    Unknown,
    PublicKey(PubKey),
    NodeId(NodeId),
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct MessageIdentity<PubKey>
where PubKey: PublicKey
{
    version: u8,
    source: PubKey,
    dest: NodeDestination<Pubkey>,
    signature: Vec<u8>,
    flags: IdentityFlags,
}

impl<'a, PubKey> TryFrom<Vec<u8>> for MessageIdentity<PubKey>
where PubKey: PublicKey + Deserialize<'a>
{
    type Error = MessageEnvelopeError;

    fn try_from(message_identity: Vec<u8>) -> Result<MessageIdentity<PubKey>, MessageEnvelopeError>
    where PubKey: PublicKey {
        let mut de = rmp_serde::Deserializer::new(message_identity.as_slice());
        Deserialize::deserialize(&mut de).map_err(|e| MessageEnvelopeError::BinaryDeserializeError(e))
    }
}

impl<PubKey> TryFrom<MessageIdentity<PubKey>> for Vec<u8>
where PubKey: PublicKey + Serialize
{
    type Error = MessageEnvelopeError;

    fn try_from(message_identity: MessageIdentity<PubKey>) -> Result<Vec<u8>, MessageEnvelopeError> {
        let mut buf = Vec::new();
        message_identity
            .serialize(&mut rmp_serde::Serializer::new(&mut buf))
            .map_err(|e| MessageEnvelopeError::BinarySerializeError(e))?;
        Ok(buf.to_vec())
    }
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct MessageVersion {
    version: u16,
}

impl TryFrom<Vec<u8>> for MessageVersion {
    type Error = MessageEnvelopeError;

    fn try_from(message_version: Vec<u8>) -> Result<MessageVersion, MessageEnvelopeError> {
        let mut de = rmp_serde::Deserializer::new(message_version.as_slice());
        Deserialize::deserialize(&mut de).map_err(|e| MessageEnvelopeError::BinaryDeserializeError(e))
    }
}

impl TryFrom<MessageVersion> for Vec<u8> {
    type Error = MessageEnvelopeError;

    fn try_from(message_version: MessageVersion) -> Result<Vec<u8>, MessageEnvelopeError> {
        let mut buf = Vec::new();
        message_version
            .serialize(&mut rmp_serde::Serializer::new(&mut buf))
            .map_err(|e| MessageEnvelopeError::BinarySerializeError(e))?;
        Ok(buf.to_vec())
    }
}

pub struct MessageHeader {
    data: Vec<u8>,
}

pub struct MessageBody {
    data: Vec<u8>,
}

/// This struct represents the raw data message deserialized
pub struct MessageEnvelope<PubKey>
where PubKey: PublicKey
{
    data_message_header: MessageIdentity<PubKey>,
    version: MessageVersion,
    internal_header: MessageHeader,
    internal_body: MessageBody,
}

impl<'a, PubKey> TryFrom<RawDataMessage> for MessageEnvelope<PubKey>
where PubKey: PublicKey + Deserialize<'a>
{
    type Error = MessageEnvelopeError;

    fn try_from(raw_message: RawDataMessage) -> Result<MessageEnvelope<PubKey>, MessageEnvelopeError> {
        let mut raw_frames = raw_message.get_frames();
        Ok(MessageEnvelope {
            internal_body: MessageBody {
                data: raw_frames.remove(3),
            },
            internal_header: MessageHeader {
                data: raw_frames.remove(2),
            },
            version: MessageVersion::try_from(raw_frames.remove(1))?,
            data_message_header: MessageIdentity::try_from(raw_frames.remove(0))?,
        })
    }
}

impl<PubKey> TryFrom<MessageEnvelope<PubKey>> for RawDataMessage
where PubKey: PublicKey + Serialize
{
    type Error = MessageEnvelopeError;

    fn try_from(message: MessageEnvelope<PubKey>) -> Result<RawDataMessage, MessageEnvelopeError> {
        let identity = <Vec<u8>>::try_from(message.identity)?;
        let version = <Vec<u8>>::try_from(message.version)?;
        let header = message.internal_header.data;
        let body = message.internal_body.data;
        Ok(RawDataMessage::new(identity, version, header, body))
    }
}
