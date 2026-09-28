// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Requests that derive a key and hand back its public half (or, for the view key, the key itself), and the two
//! handshake requests `verify_ledger_application` starts with.

use super::{ACCOUNT_SIZE, Decode, DecodeError, Encode, Reader, Request, Writer, write_u64};
use crate::common_types::Instruction;

/// `GetPublicKey`: `account(8) | index(8) | branch(8)`, 24 bytes.
///
/// `branch` is a [`crate::common_types::LedgerKeyBranch`] byte widened to a little endian `u64` - a frozen wire
/// oddity, see the module docs. It is carried as the `u64` the wire holds rather than as the enum, because the
/// device has to be able to see (and refuse) a value that does not fit in a byte at all.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct GetPublicKeyRequest {
    pub account: u64,
    pub index: u64,
    pub branch: u64,
}

impl GetPublicKeyRequest {
    pub const SIZE: usize = ACCOUNT_SIZE + 8 + 8;
}

impl Encode for GetPublicKeyRequest {
    fn encode(&self, out: &mut impl Writer) {
        write_u64(out, self.account);
        write_u64(out, self.index);
        write_u64(out, self.branch);
    }
}

impl Decode<'_> for GetPublicKeyRequest {
    fn decode(data: &[u8]) -> Result<Self, DecodeError> {
        let mut reader = Reader::exact(data, Self::SIZE)?;
        Ok(Self {
            account: reader.u64()?,
            index: reader.u64()?,
            branch: reader.u64()?,
        })
    }
}

impl Request for GetPublicKeyRequest {
    const INSTRUCTION: Instruction = Instruction::GetPublicKey;
}

/// `GetViewKey`: `account(8)`, 8 bytes. The view key's index and key type are fixed on the device.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct GetViewKeyRequest {
    pub account: u64,
}

impl GetViewKeyRequest {
    pub const SIZE: usize = ACCOUNT_SIZE;
}

impl Encode for GetViewKeyRequest {
    fn encode(&self, out: &mut impl Writer) {
        write_u64(out, self.account);
    }
}

impl Decode<'_> for GetViewKeyRequest {
    fn decode(data: &[u8]) -> Result<Self, DecodeError> {
        let mut reader = Reader::exact(data, Self::SIZE)?;
        Ok(Self { account: reader.u64()? })
    }
}

impl Request for GetViewKeyRequest {
    const INSTRUCTION: Instruction = Instruction::GetViewKey;
}

/// `GetVersion`: `account(8) | 0x00`, 9 bytes.
///
/// The device reads nothing from this payload - it answers with its version whatever it is sent - so there is no
/// decoder. The host has always sent a random account and a single zero byte, and still does, so that the bytes on
/// the wire do not change.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct GetVersionRequest {
    pub account: u64,
}

impl Encode for GetVersionRequest {
    fn encode(&self, out: &mut impl Writer) {
        write_u64(out, self.account);
        out.write(&[0]);
    }
}

impl Request for GetVersionRequest {
    const INSTRUCTION: Instruction = Instruction::GetVersion;
}

/// `GetAppName`: `account(8) | 0x00`, 9 bytes. Never decoded, for the same reason as [`GetVersionRequest`].
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct GetAppNameRequest {
    pub account: u64,
}

impl Encode for GetAppNameRequest {
    fn encode(&self, out: &mut impl Writer) {
        write_u64(out, self.account);
        out.write(&[0]);
    }
}

impl Request for GetAppNameRequest {
    const INSTRUCTION: Instruction = Instruction::GetAppName;
}

/// `GetPublicSpendKey`: `account(8)`, 8 bytes. The spend key's index and key type are fixed on the device, and the
/// host can never name them.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct GetPublicSpendKeyRequest {
    pub account: u64,
}

impl GetPublicSpendKeyRequest {
    pub const SIZE: usize = ACCOUNT_SIZE;
}

impl Encode for GetPublicSpendKeyRequest {
    fn encode(&self, out: &mut impl Writer) {
        write_u64(out, self.account);
    }
}

impl Decode<'_> for GetPublicSpendKeyRequest {
    fn decode(data: &[u8]) -> Result<Self, DecodeError> {
        let mut reader = Reader::exact(data, Self::SIZE)?;
        Ok(Self { account: reader.u64()? })
    }
}

impl Request for GetPublicSpendKeyRequest {
    const INSTRUCTION: Instruction = Instruction::GetPublicSpendKey;
}

/// `GetDHSharedSecret`: `account(8) | index(8) | branch(8) | public_key(32)`, 56 bytes.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct GetDHSharedSecretRequest<'a> {
    pub account: u64,
    pub index: u64,
    pub branch: u64,
    pub public_key: &'a [u8; 32],
}

impl GetDHSharedSecretRequest<'_> {
    pub const SIZE: usize = ACCOUNT_SIZE + 8 + 8 + 32;
}

impl Encode for GetDHSharedSecretRequest<'_> {
    fn encode(&self, out: &mut impl Writer) {
        write_u64(out, self.account);
        write_u64(out, self.index);
        write_u64(out, self.branch);
        out.write(self.public_key);
    }
}

impl<'a> Decode<'a> for GetDHSharedSecretRequest<'a> {
    fn decode(data: &'a [u8]) -> Result<Self, DecodeError> {
        let mut reader = Reader::exact(data, Self::SIZE)?;
        Ok(Self {
            account: reader.u64()?,
            index: reader.u64()?,
            branch: reader.u64()?,
            public_key: reader.array()?,
        })
    }
}

impl Request for GetDHSharedSecretRequest<'_> {
    const INSTRUCTION: Instruction = Instruction::GetDHSharedSecret;
}

#[cfg(test)]
mod test {
    // A panic is the desired failure mode in a test.
    #![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]

    use alloc::vec;

    use super::*;

    #[test]
    fn get_public_key_round_trips_at_its_exact_length() {
        let request = GetPublicKeyRequest {
            account: 0x0807_0605_0403_0201,
            index: 0x1112_1314_1516_1718,
            branch: 0x08,
        };
        let bytes = request.to_vec();
        assert_eq!(bytes, vec![
            1, 2, 3, 4, 5, 6, 7, 8, 0x18, 0x17, 0x16, 0x15, 0x14, 0x13, 0x12, 0x11, 0x08, 0, 0, 0, 0, 0, 0, 0
        ]);
        assert_eq!(GetPublicKeyRequest::decode(&bytes), Ok(request));
    }

    /// The device refuses anything but the exact length, one byte either way included, before it reads a field.
    #[test]
    fn get_public_key_refuses_any_other_length() {
        let bytes = [0u8; GetPublicKeyRequest::SIZE + 1];
        assert!(GetPublicKeyRequest::decode(&bytes).is_err());
        assert!(GetPublicKeyRequest::decode(&bytes[..GetPublicKeyRequest::SIZE - 1]).is_err());
        assert!(GetPublicKeyRequest::decode(&[]).is_err());
    }

    /// A branch that does not fit in a byte still decodes: refusing it is the device's `BadBranchKey`, not a
    /// framing error, and must stay that way so the status word does not change.
    #[test]
    fn get_public_key_leaves_the_branch_to_the_device() {
        let request = GetPublicKeyRequest {
            account: 0,
            index: 0,
            branch: u64::MAX,
        };
        assert_eq!(GetPublicKeyRequest::decode(&request.to_vec()), Ok(request));
    }

    #[test]
    fn get_view_key_is_the_account_alone() {
        let request = GetViewKeyRequest { account: 7 };
        assert_eq!(request.to_vec(), 7u64.to_le_bytes().to_vec());
        assert_eq!(GetViewKeyRequest::decode(&request.to_vec()), Ok(request));
        assert!(GetViewKeyRequest::decode(&[0u8; 9]).is_err());
        assert!(GetViewKeyRequest::decode(&[0u8; 7]).is_err());
    }

    /// The handshake requests carry a trailing zero byte the device never reads. It is part of the frozen format.
    #[test]
    fn the_handshake_requests_are_the_account_and_a_zero_byte() {
        let mut expected = 5u64.to_le_bytes().to_vec();
        expected.push(0);
        assert_eq!(GetVersionRequest { account: 5 }.to_vec(), expected);
        assert_eq!(GetAppNameRequest { account: 5 }.to_vec(), expected);
    }

    #[test]
    fn get_public_spend_key_is_the_account_alone() {
        let request = GetPublicSpendKeyRequest { account: 3 };
        assert_eq!(request.to_vec(), 3u64.to_le_bytes().to_vec());
        assert_eq!(GetPublicSpendKeyRequest::decode(&request.to_vec()), Ok(request));
        assert!(GetPublicSpendKeyRequest::decode(&[0u8; 9]).is_err());
    }

    #[test]
    fn get_dh_shared_secret_round_trips_with_the_point_last() {
        let point = [0x61; 32];
        let request = GetDHSharedSecretRequest {
            account: 1,
            index: 2,
            branch: 6,
            public_key: &point,
        };
        let bytes = request.to_vec();
        assert_eq!(bytes.len(), 56);
        assert_eq!(&bytes[24..], &point);
        assert_eq!(GetDHSharedSecretRequest::decode(&bytes), Ok(request));
        assert!(GetDHSharedSecretRequest::decode(&bytes[..55]).is_err());
    }
}
