// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! The replies the application sends back.
//!
//! Every reply but `GetVersion` and `GetAppName` ([`TextReply`]) starts with [`RESPONSE_VERSION`]. The device encodes
//! these; the host decodes them.
//!
//! A reply decoder accepts *at least* its size and ignores anything after it. That is not a relaxation for its own
//! sake: it is what the host's hand written `data.len() < N` checks have always done, and tightening it would be a
//! behaviour change a refactor must not make. The version byte is decoded but not checked - the host checks it only
//! where it always has.

use super::{Decode, DecodeError, Encode, RESPONSE_VERSION, Reader, Writer, write_u64};
use crate::{ephemeral_nonce::EPHEMERAL_NONCE_REPLY_SIZE, script_offset::SCRIPT_OFFSET_REPLY_SIZE};

/// `text(..)`: the `GetVersion` and `GetAppName` replies, the whole reply and nothing else.
///
/// No version byte: these are how the host finds out which application, at which version, it is talking to, so they
/// cannot depend on it already knowing.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct TextReply<'a> {
    pub text: &'a [u8],
}

impl Encode for TextReply<'_> {
    fn encode(&self, out: &mut impl Writer) {
        out.write(self.text);
    }
}

impl<'a> TextReply<'a> {
    /// The whole reply is the text, so unlike every other layout this cannot fail to decode - which is why it is an
    /// inherent function rather than [`Decode`]. An empty reply is the host's to interpret: it means the application
    /// is not running.
    pub fn decode(data: &'a [u8]) -> Self {
        Self { text: data }
    }
}

/// `version(1) | key(32)`, 33 bytes.
///
/// Shared by every instruction that hands back a single 32 byte value: a public key (`GetPublicKey`,
/// `GetPublicSpendKey`, `GetDHSharedSecret`) or the view key's scalar (`GetViewKey`).
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct KeyReply<'a> {
    pub version: u8,
    pub key: &'a [u8; 32],
}

impl<'a> KeyReply<'a> {
    pub const SIZE: usize = 1 + 32;

    /// The reply the current application sends.
    pub fn new(key: &'a [u8; 32]) -> Self {
        Self {
            version: RESPONSE_VERSION,
            key,
        }
    }
}

impl Encode for KeyReply<'_> {
    fn encode(&self, out: &mut impl Writer) {
        out.write(&[self.version]);
        out.write(self.key);
    }
}

impl<'a> Decode<'a> for KeyReply<'a> {
    fn decode(data: &'a [u8]) -> Result<Self, DecodeError> {
        let mut reader = Reader::at_least(data, Self::SIZE)?;
        Ok(Self {
            version: reader.u8()?,
            key: reader.array()?,
        })
    }
}

/// `version(1) | public_nonce(32) | signature(32)`, 65 bytes.
///
/// Shared by all three Schnorr instructions: `GetRawSchnorrSignature`, `GetRawSchnorrSignatureLegacyNonce` and
/// `GetScriptSchnorrSignature`.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct SchnorrReply<'a> {
    pub version: u8,
    pub public_nonce: &'a [u8; 32],
    pub signature: &'a [u8; 32],
}

impl<'a> SchnorrReply<'a> {
    pub const SIZE: usize = 1 + 32 + 32;

    /// The reply the current application sends.
    pub fn new(public_nonce: &'a [u8; 32], signature: &'a [u8; 32]) -> Self {
        Self {
            version: RESPONSE_VERSION,
            public_nonce,
            signature,
        }
    }
}

impl Encode for SchnorrReply<'_> {
    fn encode(&self, out: &mut impl Writer) {
        out.write(&[self.version]);
        out.write(self.public_nonce);
        out.write(self.signature);
    }
}

impl<'a> Decode<'a> for SchnorrReply<'a> {
    fn decode(data: &'a [u8]) -> Result<Self, DecodeError> {
        let mut reader = Reader::at_least(data, Self::SIZE)?;
        Ok(Self {
            version: reader.u8()?,
            public_nonce: reader.array()?,
            signature: reader.array()?,
        })
    }
}

/// `version(1) | ephemeral_commitment(32) | ephemeral_pubkey(32) | u_a(32) | u_x(32) | u_y(32)`, 161 bytes.
///
/// A commitment and public key signature, shared by both script signature instructions and
/// `GetOneSidedMetadataSignature`. The field order is `CommitmentAndPublicKeySignature::to_vec`'s: the ephemeral
/// commitment and key, then the three responses `u_a`, `u_x`, `u_y` - which is not the order `sign` takes its secrets
/// in, so it is spelled out here rather than left to a `to_vec` on either side.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct ComAndPubSigReply<'a> {
    pub version: u8,
    pub ephemeral_commitment: &'a [u8; 32],
    pub ephemeral_pubkey: &'a [u8; 32],
    pub u_a: &'a [u8; 32],
    pub u_x: &'a [u8; 32],
    pub u_y: &'a [u8; 32],
}

impl<'a> ComAndPubSigReply<'a> {
    pub const SIZE: usize = 1 + 32 * 5;

    /// The reply the current application sends.
    pub fn new(
        ephemeral_commitment: &'a [u8; 32],
        ephemeral_pubkey: &'a [u8; 32],
        u_a: &'a [u8; 32],
        u_x: &'a [u8; 32],
        u_y: &'a [u8; 32],
    ) -> Self {
        Self {
            version: RESPONSE_VERSION,
            ephemeral_commitment,
            ephemeral_pubkey,
            u_a,
            u_x,
            u_y,
        }
    }
}

impl Encode for ComAndPubSigReply<'_> {
    fn encode(&self, out: &mut impl Writer) {
        out.write(&[self.version]);
        out.write(self.ephemeral_commitment);
        out.write(self.ephemeral_pubkey);
        out.write(self.u_a);
        out.write(self.u_x);
        out.write(self.u_y);
    }
}

impl<'a> Decode<'a> for ComAndPubSigReply<'a> {
    fn decode(data: &'a [u8]) -> Result<Self, DecodeError> {
        let mut reader = Reader::at_least(data, Self::SIZE)?;
        Ok(Self {
            version: reader.u8()?,
            ephemeral_commitment: reader.array()?,
            ephemeral_pubkey: reader.array()?,
            u_a: reader.array()?,
            u_x: reader.array()?,
            u_y: reader.array()?,
        })
    }
}

/// `version(1) | handle(8) | public_nonce(32)`: the `GenerateEphemeralNonce` reply.
///
/// The private nonce never crosses the wire; only the opaque handle that names it and its public form do.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct EphemeralNonceReply<'a> {
    pub version: u8,
    pub handle: u64,
    pub public_nonce: &'a [u8; 32],
}

impl<'a> EphemeralNonceReply<'a> {
    /// [`EPHEMERAL_NONCE_REPLY_SIZE`], which predates this codec and which the scenario suite also checks against.
    pub const SIZE: usize = EPHEMERAL_NONCE_REPLY_SIZE;

    /// The reply the current application sends.
    pub fn new(handle: u64, public_nonce: &'a [u8; 32]) -> Self {
        Self {
            version: RESPONSE_VERSION,
            handle,
            public_nonce,
        }
    }
}

// The fields have to add up to the size the rest of the crate declares.
const _: () = assert!(1 + 8 + 32 == EphemeralNonceReply::SIZE);

impl Encode for EphemeralNonceReply<'_> {
    fn encode(&self, out: &mut impl Writer) {
        out.write(&[self.version]);
        write_u64(out, self.handle);
        out.write(self.public_nonce);
    }
}

impl<'a> Decode<'a> for EphemeralNonceReply<'a> {
    fn decode(data: &'a [u8]) -> Result<Self, DecodeError> {
        let mut reader = Reader::at_least(data, Self::SIZE)?;
        Ok(Self {
            version: reader.u8()?,
            handle: reader.u64()?,
            public_nonce: reader.array()?,
        })
    }
}

/// `version(1) | script_offset(32) | base_index(8)`: the reply to the last `GetScriptOffset` chunk.
///
/// The sender offset keys that blind the offset are `base_index..base_index + sender_offset_count`, walked with
/// [`crate::script_offset::sender_offset_index`] on both sides.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct ScriptOffsetReply<'a> {
    pub version: u8,
    pub script_offset: &'a [u8; 32],
    pub base_index: u64,
}

impl<'a> ScriptOffsetReply<'a> {
    /// [`SCRIPT_OFFSET_REPLY_SIZE`], which predates this codec and which the scenario suite also checks against.
    pub const SIZE: usize = SCRIPT_OFFSET_REPLY_SIZE;

    /// The reply the current application sends.
    pub fn new(script_offset: &'a [u8; 32], base_index: u64) -> Self {
        Self {
            version: RESPONSE_VERSION,
            script_offset,
            base_index,
        }
    }
}

// The fields have to add up to the size the rest of the crate declares.
const _: () = assert!(1 + 32 + 8 == ScriptOffsetReply::SIZE);

impl Encode for ScriptOffsetReply<'_> {
    fn encode(&self, out: &mut impl Writer) {
        out.write(&[self.version]);
        out.write(self.script_offset);
        write_u64(out, self.base_index);
    }
}

impl<'a> Decode<'a> for ScriptOffsetReply<'a> {
    fn decode(data: &'a [u8]) -> Result<Self, DecodeError> {
        let mut reader = Reader::at_least(data, Self::SIZE)?;
        Ok(Self {
            version: reader.u8()?,
            script_offset: reader.array()?,
            base_index: reader.u64()?,
        })
    }
}

#[cfg(test)]
mod test {
    // A panic is the desired failure mode in a test.
    #![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]

    use super::*;

    #[test]
    fn a_key_reply_is_the_version_then_the_key() {
        let key = [0xab; 32];
        let bytes = KeyReply::new(&key).to_vec();
        assert_eq!(bytes.len(), KeyReply::SIZE);
        assert_eq!(bytes[0], RESPONSE_VERSION);
        assert_eq!(&bytes[1..], &key);
        assert_eq!(KeyReply::decode(&bytes), Ok(KeyReply::new(&key)));
    }

    /// Trailing bytes are ignored and a short reply is refused, exactly as the host's length checks always did.
    #[test]
    fn a_key_reply_is_decoded_from_a_prefix() {
        let mut bytes = KeyReply::new(&[1; 32]).to_vec();
        bytes.push(0xff);
        assert_eq!(KeyReply::decode(&bytes).map(|reply| *reply.key), Ok([1; 32]));
        assert!(KeyReply::decode(&bytes[..KeyReply::SIZE - 1]).is_err());
    }

    #[test]
    fn a_schnorr_reply_is_the_version_then_the_nonce_then_the_signature() {
        let bytes = SchnorrReply::new(&[0x11; 32], &[0x22; 32]).to_vec();
        assert_eq!(bytes.len(), SchnorrReply::SIZE);
        assert_eq!(bytes[0], RESPONSE_VERSION);
        assert_eq!(&bytes[1..33], &[0x11; 32]);
        assert_eq!(&bytes[33..65], &[0x22; 32]);
        assert_eq!(
            SchnorrReply::decode(&bytes),
            Ok(SchnorrReply::new(&[0x11; 32], &[0x22; 32]))
        );
        assert!(SchnorrReply::decode(&bytes[..64]).is_err());
    }

    /// Five distinct fillers, so that two fields read from the same offset show up here rather than as a signature
    /// that mysteriously fails to verify.
    #[test]
    fn a_com_and_pub_sig_reply_keeps_its_five_fields_in_to_vec_order() {
        let reply = ComAndPubSigReply::new(&[0x31; 32], &[0x32; 32], &[0x33; 32], &[0x34; 32], &[0x35; 32]);
        let bytes = reply.to_vec();
        assert_eq!(bytes.len(), ComAndPubSigReply::SIZE);
        for (i, filler) in [0x31u8, 0x32, 0x33, 0x34, 0x35].into_iter().enumerate() {
            let start = 1 + 32 * i;
            assert_eq!(&bytes[start..start + 32], &[filler; 32]);
        }
        assert_eq!(ComAndPubSigReply::decode(&bytes), Ok(reply));
        assert!(ComAndPubSigReply::decode(&bytes[..160]).is_err());
    }

    #[test]
    fn an_ephemeral_nonce_reply_is_the_version_then_the_handle_then_the_nonce() {
        let reply = EphemeralNonceReply::new(0x0807_0605_0403_0201, &[0x44; 32]);
        let bytes = reply.to_vec();
        assert_eq!(bytes.len(), EPHEMERAL_NONCE_REPLY_SIZE);
        assert_eq!(bytes[0], RESPONSE_VERSION);
        assert_eq!(&bytes[1..9], &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(&bytes[9..], &[0x44; 32]);
        assert_eq!(EphemeralNonceReply::decode(&bytes), Ok(reply));
        assert!(EphemeralNonceReply::decode(&bytes[..40]).is_err());
    }

    #[test]
    fn a_script_offset_reply_is_the_version_then_the_offset_then_the_base_index() {
        let reply = ScriptOffsetReply::new(&[0x55; 32], 0x0807_0605_0403_0201);
        let bytes = reply.to_vec();
        assert_eq!(bytes.len(), SCRIPT_OFFSET_REPLY_SIZE);
        assert_eq!(bytes[0], RESPONSE_VERSION);
        assert_eq!(&bytes[1..33], &[0x55; 32]);
        assert_eq!(&bytes[33..], &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(ScriptOffsetReply::decode(&bytes), Ok(reply));
        assert!(ScriptOffsetReply::decode(&bytes[..40]).is_err());
    }
}
