// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! The nonce instructions: reserving a device held nonce, signing with it, and the legacy host indexed nonce that
//! [`crate::legacy_nonce`] explains and confines.

use super::{ACCOUNT_SIZE, Decode, DecodeError, Encode, Reader, Request, Writer, write_u64};
use crate::common_types::Instruction;

/// `GenerateEphemeralNonce`: `account(8)`, 8 bytes.
///
/// Nothing but the account. The nonce is a random scalar the device draws, not a derived key, so there is no path
/// for the host to influence and nothing else to send. The same bytes as [`super::GetViewKeyRequest`] by
/// coincidence rather than by rule, which is why it is a type of its own: if it ever took an argument, this is the
/// type that would grow one.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct GenerateEphemeralNonceRequest {
    pub account: u64,
}

impl GenerateEphemeralNonceRequest {
    pub const SIZE: usize = ACCOUNT_SIZE;
}

impl Encode for GenerateEphemeralNonceRequest {
    fn encode(&self, out: &mut impl Writer) {
        write_u64(out, self.account);
    }
}

impl Decode<'_> for GenerateEphemeralNonceRequest {
    fn decode(data: &[u8]) -> Result<Self, DecodeError> {
        let mut reader = Reader::exact(data, Self::SIZE)?;
        Ok(Self { account: reader.u64()? })
    }
}

impl Request for GenerateEphemeralNonceRequest {
    const INSTRUCTION: Instruction = Instruction::GenerateEphemeralNonce;
}

/// `GetRawSchnorrSignature`: `account(8) | index(8) | branch(8) | nonce_handle(8) | challenge(64)`, 96 bytes.
///
/// `nonce_handle` names a nonce `GenerateEphemeralNonce` reserved. It is opaque and single use; the device consumes
/// it whether or not the signature succeeds.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct GetRawSchnorrSignatureRequest<'a> {
    pub account: u64,
    pub index: u64,
    pub branch: u64,
    pub nonce_handle: u64,
    pub challenge: &'a [u8; 64],
}

impl GetRawSchnorrSignatureRequest<'_> {
    pub const SIZE: usize = ACCOUNT_SIZE + 8 * 3 + 64;
}

impl Encode for GetRawSchnorrSignatureRequest<'_> {
    fn encode(&self, out: &mut impl Writer) {
        write_u64(out, self.account);
        write_u64(out, self.index);
        write_u64(out, self.branch);
        write_u64(out, self.nonce_handle);
        out.write(self.challenge);
    }
}

impl<'a> Decode<'a> for GetRawSchnorrSignatureRequest<'a> {
    fn decode(data: &'a [u8]) -> Result<Self, DecodeError> {
        let mut reader = Reader::exact(data, Self::SIZE)?;
        Ok(Self {
            account: reader.u64()?,
            index: reader.u64()?,
            branch: reader.u64()?,
            nonce_handle: reader.u64()?,
            challenge: reader.array()?,
        })
    }
}

impl Request for GetRawSchnorrSignatureRequest<'_> {
    const INSTRUCTION: Instruction = Instruction::GetRawSchnorrSignature;
}

/// `GetRawSchnorrSignatureLegacyNonce`:
/// `account(8) | key_index(8) | key_branch(8) | nonce_index(8) | nonce_branch(8) | challenge(64)`, 104 bytes.
///
/// DEPRECATED along with the instruction; see [`crate::legacy_nonce`] for what it costs and what deletes it. Both
/// branches are carried as the wire's `u64`s, so the device's whitelist - not the framing - is what refuses a
/// branch pre-mine does not use.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct GetRawSchnorrSignatureLegacyNonceRequest<'a> {
    pub account: u64,
    pub key_index: u64,
    pub key_branch: u64,
    pub nonce_index: u64,
    pub nonce_branch: u64,
    pub challenge: &'a [u8; 64],
}

impl GetRawSchnorrSignatureLegacyNonceRequest<'_> {
    pub const SIZE: usize = ACCOUNT_SIZE + 8 * 4 + 64;
}

impl Encode for GetRawSchnorrSignatureLegacyNonceRequest<'_> {
    fn encode(&self, out: &mut impl Writer) {
        write_u64(out, self.account);
        write_u64(out, self.key_index);
        write_u64(out, self.key_branch);
        write_u64(out, self.nonce_index);
        write_u64(out, self.nonce_branch);
        out.write(self.challenge);
    }
}

impl<'a> Decode<'a> for GetRawSchnorrSignatureLegacyNonceRequest<'a> {
    fn decode(data: &'a [u8]) -> Result<Self, DecodeError> {
        let mut reader = Reader::exact(data, Self::SIZE)?;
        Ok(Self {
            account: reader.u64()?,
            key_index: reader.u64()?,
            key_branch: reader.u64()?,
            nonce_index: reader.u64()?,
            nonce_branch: reader.u64()?,
            challenge: reader.array()?,
        })
    }
}

impl Request for GetRawSchnorrSignatureLegacyNonceRequest<'_> {
    const INSTRUCTION: Instruction = Instruction::GetRawSchnorrSignatureLegacyNonce;
}

#[cfg(test)]
mod test {
    // A panic is the desired failure mode in a test.
    #![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]

    use super::*;

    const CHALLENGE: [u8; 64] = [0x51; 64];

    /// The sizes are the device's own length checks.
    #[test]
    fn the_sizes_are_the_ones_the_device_has_always_checked() {
        assert_eq!(GenerateEphemeralNonceRequest::SIZE, 8);
        assert_eq!(GetRawSchnorrSignatureRequest::SIZE, 96);
        assert_eq!(GetRawSchnorrSignatureLegacyNonceRequest::SIZE, 104);
    }

    #[test]
    fn generate_ephemeral_nonce_is_the_account_alone() {
        let request = GenerateEphemeralNonceRequest { account: 9 };
        assert_eq!(request.to_vec(), 9u64.to_le_bytes().to_vec());
        assert_eq!(GenerateEphemeralNonceRequest::decode(&request.to_vec()), Ok(request));
        assert!(GenerateEphemeralNonceRequest::decode(&[0; 9]).is_err());
    }

    #[test]
    fn a_raw_schnorr_request_carries_the_handle_before_the_challenge() {
        let request = GetRawSchnorrSignatureRequest {
            account: 1,
            index: 2,
            branch: 8,
            nonce_handle: 0x0102_0304,
            challenge: &CHALLENGE,
        };
        let bytes = request.to_vec();
        assert_eq!(&bytes[24..32], &0x0102_0304u64.to_le_bytes());
        assert_eq!(&bytes[32..], &CHALLENGE);
        assert_eq!(GetRawSchnorrSignatureRequest::decode(&bytes), Ok(request));
        assert!(GetRawSchnorrSignatureRequest::decode(&bytes[..95]).is_err());
    }

    #[test]
    fn a_legacy_nonce_request_carries_both_branches_as_the_wire_has_them() {
        let request = GetRawSchnorrSignatureLegacyNonceRequest {
            account: 1,
            key_index: 2,
            key_branch: 9,
            nonce_index: 3,
            // Not a branch at all: refusing it is the device's whitelist, not the framing.
            nonce_branch: u64::MAX,
            challenge: &CHALLENGE,
        };
        let bytes = request.to_vec();
        assert_eq!(&bytes[16..24], &9u64.to_le_bytes());
        assert_eq!(&bytes[32..40], &u64::MAX.to_le_bytes());
        assert_eq!(&bytes[40..], &CHALLENGE);
        assert_eq!(GetRawSchnorrSignatureLegacyNonceRequest::decode(&bytes), Ok(request));
        assert!(GetRawSchnorrSignatureLegacyNonceRequest::decode(&bytes[..103]).is_err());
    }
}
