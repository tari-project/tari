// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! The replies the application sends back.
//!
//! Every reply but `GetVersion` and `GetAppName` starts with [`RESPONSE_VERSION`]. The device encodes these; the
//! host decodes them.
//!
//! A reply decoder accepts *at least* its size and ignores anything after it. That is not a relaxation for its own
//! sake: it is what the host's hand written `data.len() < N` checks have always done, and tightening it would be a
//! behaviour change a refactor must not make. The version byte is decoded but not checked - the host checks it only
//! where it always has.

use super::{Decode, DecodeError, Encode, RESPONSE_VERSION, Reader, Writer};

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
}
