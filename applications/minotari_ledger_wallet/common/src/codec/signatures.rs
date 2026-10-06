// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Requests for signatures over a host supplied message: both script signature instructions, and the script
//! Schnorr signature `verify_ledger_application` also leans on.

use super::{ACCOUNT_SIZE, Decode, DecodeError, Encode, Reader, Request, Writer, write_u64};
use crate::common_types::Instruction;

/// The prefix both script signature instructions share:
/// `account(8) | network(8) | txi_version(8) | value(32) | commitment_private_key(32) | commitment(32) |
/// message(32)`, 152 bytes.
///
/// `network` and `txi_version` are bytes widened to little endian `u64`s - frozen, see the module docs.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct ScriptSignatureCommon<'a> {
    pub account: u64,
    pub network: u64,
    pub txi_version: u64,
    pub value: &'a [u8; 32],
    pub commitment_private_key: &'a [u8; 32],
    pub commitment: &'a [u8; 32],
    pub message: &'a [u8; 32],
}

#[allow(clippy::inline_always)]
impl<'a> ScriptSignatureCommon<'a> {
    pub const SIZE: usize = ACCOUNT_SIZE + 8 + 8 + 32 * 4;

    fn encode(&self, out: &mut impl Writer) {
        write_u64(out, self.account);
        write_u64(out, self.network);
        write_u64(out, self.txi_version);
        out.write(self.value);
        out.write(self.commitment_private_key);
        out.write(self.commitment);
        out.write(self.message);
    }

    // Forced inline: see `Reader` for why.
    #[inline(always)]
    fn read(reader: &mut Reader<'a>) -> Result<Self, DecodeError> {
        Ok(Self {
            account: reader.u64()?,
            network: reader.u64()?,
            txi_version: reader.u64()?,
            value: reader.array()?,
            commitment_private_key: reader.array()?,
            commitment: reader.array()?,
            message: reader.array()?,
        })
    }
}

/// `GetScriptSignatureManaged`: the common prefix, then `branch(8) | index(8)`, 168 bytes.
///
/// Note the order - branch *then* index - which is the reverse of every other instruction that names a key. It is
/// frozen like everything else here.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct GetScriptSignatureManagedRequest<'a> {
    pub common: ScriptSignatureCommon<'a>,
    pub branch: u64,
    pub index: u64,
}

impl GetScriptSignatureManagedRequest<'_> {
    pub const SIZE: usize = ScriptSignatureCommon::SIZE + 8 + 8;
}

impl Encode for GetScriptSignatureManagedRequest<'_> {
    fn encode(&self, out: &mut impl Writer) {
        self.common.encode(out);
        write_u64(out, self.branch);
        write_u64(out, self.index);
    }
}

impl<'a> Decode<'a> for GetScriptSignatureManagedRequest<'a> {
    fn decode(data: &'a [u8]) -> Result<Self, DecodeError> {
        let mut reader = Reader::exact(data, Self::SIZE)?;
        Ok(Self {
            common: ScriptSignatureCommon::read(&mut reader)?,
            branch: reader.u64()?,
            index: reader.u64()?,
        })
    }
}

impl Request for GetScriptSignatureManagedRequest<'_> {
    const INSTRUCTION: Instruction = Instruction::GetScriptSignatureManaged;
}

/// `GetScriptSignatureDerived`: the common prefix, then `blinding_factor(32)`, 184 bytes. The device folds the
/// blinding factor into `alpha` to get the script key, so the host never names a key by index here.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct GetScriptSignatureDerivedRequest<'a> {
    pub common: ScriptSignatureCommon<'a>,
    pub blinding_factor: &'a [u8; 32],
}

impl GetScriptSignatureDerivedRequest<'_> {
    pub const SIZE: usize = ScriptSignatureCommon::SIZE + 32;
}

impl Encode for GetScriptSignatureDerivedRequest<'_> {
    fn encode(&self, out: &mut impl Writer) {
        self.common.encode(out);
        out.write(self.blinding_factor);
    }
}

impl<'a> Decode<'a> for GetScriptSignatureDerivedRequest<'a> {
    fn decode(data: &'a [u8]) -> Result<Self, DecodeError> {
        let mut reader = Reader::exact(data, Self::SIZE)?;
        Ok(Self {
            common: ScriptSignatureCommon::read(&mut reader)?,
            blinding_factor: reader.array()?,
        })
    }
}

impl Request for GetScriptSignatureDerivedRequest<'_> {
    const INSTRUCTION: Instruction = Instruction::GetScriptSignatureDerived;
}

/// `GetScriptSchnorrSignature`: `account(8) | index(8) | branch(8) | message(32)`, 56 bytes.
///
/// The device signs `message` with a nonce it draws itself, so unlike the raw Schnorr instructions there is no
/// nonce on the wire at all.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct GetScriptSchnorrSignatureRequest<'a> {
    pub account: u64,
    pub index: u64,
    pub branch: u64,
    pub message: &'a [u8; 32],
}

impl GetScriptSchnorrSignatureRequest<'_> {
    pub const SIZE: usize = ACCOUNT_SIZE + 8 + 8 + 32;
}

impl Encode for GetScriptSchnorrSignatureRequest<'_> {
    fn encode(&self, out: &mut impl Writer) {
        write_u64(out, self.account);
        write_u64(out, self.index);
        write_u64(out, self.branch);
        out.write(self.message);
    }
}

impl<'a> Decode<'a> for GetScriptSchnorrSignatureRequest<'a> {
    fn decode(data: &'a [u8]) -> Result<Self, DecodeError> {
        let mut reader = Reader::exact(data, Self::SIZE)?;
        Ok(Self {
            account: reader.u64()?,
            index: reader.u64()?,
            branch: reader.u64()?,
            message: reader.array()?,
        })
    }
}

impl Request for GetScriptSchnorrSignatureRequest<'_> {
    const INSTRUCTION: Instruction = Instruction::GetScriptSchnorrSignature;
}

#[cfg(test)]
mod test {
    // A panic is the desired failure mode in a test.
    #![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]

    use super::*;

    const VALUE: [u8; 32] = [0x31; 32];
    const COMMITMENT_PRIVATE_KEY: [u8; 32] = [0x32; 32];
    const COMMITMENT: [u8; 32] = [0x33; 32];
    const MESSAGE: [u8; 32] = [0x34; 32];
    const BLINDING_FACTOR: [u8; 32] = [0x35; 32];

    fn common() -> ScriptSignatureCommon<'static> {
        ScriptSignatureCommon {
            account: 1,
            network: 0x26,
            txi_version: 1,
            value: &VALUE,
            commitment_private_key: &COMMITMENT_PRIVATE_KEY,
            commitment: &COMMITMENT,
            message: &MESSAGE,
        }
    }

    /// The sizes are the device's own length checks, and the ones the scenario suite's `WrongApduLength` probes
    /// bracket.
    #[test]
    fn the_sizes_are_the_ones_the_device_has_always_checked() {
        assert_eq!(ScriptSignatureCommon::SIZE, 152);
        assert_eq!(GetScriptSignatureManagedRequest::SIZE, 168);
        assert_eq!(GetScriptSignatureDerivedRequest::SIZE, 184);
        assert_eq!(GetScriptSchnorrSignatureRequest::SIZE, 56);
    }

    #[test]
    fn a_managed_script_signature_request_round_trips_with_branch_before_index() {
        let request = GetScriptSignatureManagedRequest {
            common: common(),
            branch: 0x09,
            index: 0x0102,
        };
        let bytes = request.to_vec();
        assert_eq!(bytes.len(), GetScriptSignatureManagedRequest::SIZE);
        assert_eq!(&bytes[24..56], &VALUE);
        assert_eq!(&bytes[56..88], &COMMITMENT_PRIVATE_KEY);
        assert_eq!(&bytes[88..120], &COMMITMENT);
        assert_eq!(&bytes[120..152], &MESSAGE);
        assert_eq!(&bytes[152..160], &0x09u64.to_le_bytes());
        assert_eq!(&bytes[160..168], &0x0102u64.to_le_bytes());
        assert_eq!(GetScriptSignatureManagedRequest::decode(&bytes), Ok(request));
        assert!(GetScriptSignatureManagedRequest::decode(&bytes[..167]).is_err());
    }

    #[test]
    fn a_derived_script_signature_request_round_trips() {
        let request = GetScriptSignatureDerivedRequest {
            common: common(),
            blinding_factor: &BLINDING_FACTOR,
        };
        let mut bytes = request.to_vec();
        assert_eq!(&bytes[152..184], &BLINDING_FACTOR);
        assert_eq!(GetScriptSignatureDerivedRequest::decode(&bytes), Ok(request));
        // A managed request is not a derived one, whichever way the lengths are bent.
        assert!(GetScriptSignatureDerivedRequest::decode(&bytes[..168]).is_err());
        bytes.push(0);
        assert!(GetScriptSignatureDerivedRequest::decode(&bytes).is_err());
    }

    #[test]
    fn a_script_schnorr_signature_request_round_trips() {
        let request = GetScriptSchnorrSignatureRequest {
            account: 1,
            index: 2,
            branch: 6,
            message: &MESSAGE,
        };
        let bytes = request.to_vec();
        assert_eq!(&bytes[24..], &MESSAGE);
        assert_eq!(GetScriptSchnorrSignatureRequest::decode(&bytes), Ok(request));
        assert!(GetScriptSchnorrSignatureRequest::decode(&bytes[..55]).is_err());
    }
}
