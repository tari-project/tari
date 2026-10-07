// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! The wire format between the host and the Ledger application: every APDU payload and every reply, defined once.
//!
//! # Why this exists
//!
//! Every layout used to be written twice. The host hand rolled each payload with `extend_from_slice` in
//! `minotari_ledger_wallet_comms::accessor_methods`, and the device sliced the same bytes back apart at hand computed
//! offsets in its handlers. The two lived in different crates - one of which only compiles for ARM - and nothing
//! noticed when one moved and the other did not. This module is the only definition of both directions now, in the
//! one crate both sides link, so a layout change is made in one place and a mismatch is a compile error rather than
//! a device that quietly reads the wrong field. It is the same arrangement as [`crate::legacy_nonce`] and
//! [`crate::script_offset`]: logic in the shared crate so that it is checked on both sides, and the two copies
//! cannot drift apart.
//!
//! # What this does not do
//!
//! **The codec frames; it does not validate.** A decoder checks that the bytes are the right *length* and hands
//! back the fields; it never asks whether a scalar is canonical, a point is on the curve, or a branch is one the
//! device will serve. Those checks stay at each edge (`from_canonical_bytes`, `KeyType::from_branch_key`, the
//! whitelists in the sibling modules), because *which* check fails first decides which status word the host gets
//! back, and moving them in here would reorder them. Boundary types are therefore raw bytes only - `&[u8; 32]`,
//! `u64`, `u8` - and this crate has no `tari_crypto` dependency.
//!
//! **The wire format is byte frozen.** The layouts here reproduce the hand rolled ones exactly, oddities included
//! (every `u8` branch, network and version is widened to a little endian `u64`, for instance). Improving a layout
//! is a wire format change with its own application version bump, never a codec refactor. The golden vectors in
//! `minotari_ledger_wallet_comms/tests/golden_vectors.rs` were captured before this module existed and must keep
//! passing unchanged.
//!
//! # Shape
//!
//! - **Decoding is zero copy.** A decoded request borrows its 32 and 64 byte fields straight out of the received buffer
//!   as `&[u8; N]`. The device has a very small stack, and decoding must never buffer a payload a second time.
//! - **Encoding writes to a sink.** [`Encode::encode`] takes a [`Writer`] rather than returning a `Vec`, so the device
//!   writes a reply straight into its APDU buffer. The host's [`Encode::to_vec`] convenience sits behind the `alloc`
//!   feature, which the device build does not enable.
//! - **Every request carries its account.** The transport has always prepended the account to the first (or only) chunk
//!   of every instruction, and the device has always read it as the first eight bytes, so it is the first field of
//!   every request type here rather than something bolted on outside the layout.
//!
//! Anything the typed encoders refuse to construct - a malformed APDU the device has to reject - goes through
//! `minotari_ledger_wallet_comms::raw` instead.

mod keys;
mod metadata;
mod nonce;
mod replies;
mod script_offset;
mod signatures;

#[cfg(any(feature = "alloc", test))]
use alloc::vec::Vec;

pub use keys::{
    GetAppNameRequest,
    GetDHSharedSecretRequest,
    GetPublicKeyRequest,
    GetPublicSpendKeyRequest,
    GetVersionRequest,
    GetViewKeyRequest,
};
pub use metadata::{
    GetOneSidedMetadataSignatureRequest,
    OneSidedMetadataSignatureHead,
    OneSidedMetadataSignatureTail,
    ReceiverAddressTooLong,
};
pub use nonce::{
    GenerateEphemeralNonceRequest,
    GetRawSchnorrSignatureLegacyNonceRequest,
    GetRawSchnorrSignatureRequest,
};
pub use replies::{ComAndPubSigReply, EphemeralNonceReply, KeyReply, SchnorrReply, ScriptOffsetReply, TextReply};
pub use script_offset::{
    CHUNK_LAST,
    CHUNK_MORE,
    DerivedScriptKeyChunk,
    PartialScriptKeySumChunk,
    ScriptKeyIndexChunk,
    ScriptOffsetChunk,
    ScriptOffsetChunkBody,
    ScriptOffsetHeaderChunk,
    ScriptOffsetRequest,
};
pub use signatures::{
    GetScriptSchnorrSignatureRequest,
    GetScriptSignatureDerivedRequest,
    GetScriptSignatureManagedRequest,
    ScriptSignatureCommon,
};

use crate::common_types::Instruction;

/// The class byte of every APDU the application accepts. The device configures its transport to refuse any other
/// with `ledger_device_sdk::io::StatusWords::BadCla`, before the application sees the command.
pub const CLA: u8 = 0x80;

/// The version byte the application prefixes every reply with, bar `GetVersion` and `GetAppName`.
///
/// The host checks it where a reply's layout has changed within the lifetime of this protocol - `GetScriptOffset`
/// and `GenerateEphemeralNonce` - so that talking to an application that is too old is a legible error rather than
/// a misparse.
pub const RESPONSE_VERSION: u8 = 2;

/// Size of the account every request starts with.
pub const ACCOUNT_SIZE: usize = 8;

/// A sink for encoded bytes.
///
/// The device implements this over its APDU reply buffer, and the host over a `Vec<u8>`. Writes cannot fail: the
/// largest reply is 161 bytes, well inside the device's reply buffer, the host's sink grows, and a sink that could
/// fail would put an error path into every encoder on a device that has no room for them.
pub trait Writer {
    fn write(&mut self, bytes: &[u8]);
}

#[cfg(any(feature = "alloc", test))]
impl Writer for Vec<u8> {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        self.extend_from_slice(bytes);
    }
}

/// Why a buffer could not be decoded.
///
/// There is only one reason, because the codec only frames. A device handler maps it to
/// `AppSW::WrongApduLength`, exactly as its hand written length check did; a host maps it to the error message its
/// accessor has always produced for a short reply.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// The buffer was not a length this layout can have.
    WrongLength,
}

/// A layout that can be written to the wire.
pub trait Encode {
    /// Write this value's wire bytes to `out`.
    fn encode(&self, out: &mut impl Writer);

    /// This value's wire bytes, for the host.
    #[cfg(any(feature = "alloc", test))]
    fn to_vec(&self) -> Vec<u8> {
        let mut out = Vec::new();
        self.encode(&mut out);
        out
    }
}

/// A layout that can be read back off the wire, borrowing from the buffer it was read from.
///
/// On the device, that buffer is the SDK's APDU buffer, and **a decoded value must not be read after a UI screen**:
/// on Stax and Flex an NBGL screen polls for events while it is up, and an APDU the host sends meanwhile is copied
/// into the same buffer (`ledger_device_sdk` 1.35.0, `io_legacy.rs`, `decode_event`), so the fields would then read
/// the host's new bytes rather than the ones the user reviewed. Copy what is needed after a screen before showing
/// it; the application's `wire::with_screen` makes the borrow checker enforce that.
pub trait Decode<'a>: Sized {
    fn decode(data: &'a [u8]) -> Result<Self, DecodeError>;
}

/// A single exchange instruction's payload, with the APDU header it travels under.
///
/// The header is part of the layout: the device dispatches on `ins` and refuses a `p1`/`p2` it does not expect, so
/// the encoder has to be the thing that decides them.
pub trait Request: Encode {
    /// The instruction this payload is sent as.
    const INSTRUCTION: Instruction;

    /// The APDU `p1` byte. Zero for every single exchange instruction.
    fn p1(&self) -> u8 {
        0
    }

    /// The APDU `p2` byte. Zero for every single exchange instruction.
    fn p2(&self) -> u8 {
        0
    }
}

/// Write a `u64` field: little endian, eight bytes.
#[inline]
fn write_u64(out: &mut impl Writer, value: u64) {
    out.write(&value.to_le_bytes());
}

/// A cursor over a received buffer that hands out borrowed fixed width fields.
///
/// `split_first_chunk` rather than indexing, so that a decoder has no bounds check to panic on: a device that
/// panicked on a malformed APDU would exit the application rather than answer with a status word.
///
/// # Why `#[inline(always)]`
///
/// The device builds at `opt-level = 'z'`, where LLVM prefers a call to an inlined body. That is the wrong call
/// here. Once a decoder is inlined into its handler, the length check it starts with lets LLVM fold every per-field
/// check that follows, and the fields become plain loads at constant offsets - which is what the hand written
/// slicing it replaced compiled to. Left out of line, each field keeps its own check and its own error path, and
/// the decoded struct has to be written out through memory. Measured on the nanosplus build, forcing these inline is
/// several hundred bytes smaller, and the device application has a size budget the codec must not grow.
struct Reader<'a> {
    rest: &'a [u8],
}

#[allow(clippy::inline_always)]
impl<'a> Reader<'a> {
    /// A reader over a buffer that must be exactly `size` bytes - the device's `data.len() != N` check.
    #[inline(always)]
    fn exact(data: &'a [u8], size: usize) -> Result<Self, DecodeError> {
        if data.len() != size {
            return Err(DecodeError::WrongLength);
        }
        Ok(Self { rest: data })
    }

    /// A reader over a buffer that must be at least `size` bytes, trailing bytes ignored.
    ///
    /// For two things only: the host's decoding of *replies*, which has always been `data.len() < N` with anything
    /// after ignored, and the variable length metadata request's head and message, whose historical checks are
    /// minimums. A new fixed size request must use [`Self::exact`]: the device has always refused a request of the
    /// wrong length outright, and accepting trailing bytes would be a behaviour change a codec must not make.
    #[inline(always)]
    fn at_least(data: &'a [u8], size: usize) -> Result<Self, DecodeError> {
        if data.len() < size {
            return Err(DecodeError::WrongLength);
        }
        Ok(Self { rest: data })
    }

    #[inline(always)]
    fn array<const N: usize>(&mut self) -> Result<&'a [u8; N], DecodeError> {
        let (field, rest) = self.rest.split_first_chunk::<N>().ok_or(DecodeError::WrongLength)?;
        self.rest = rest;
        Ok(field)
    }

    #[inline(always)]
    fn u64(&mut self) -> Result<u64, DecodeError> {
        self.array::<8>().map(|bytes| u64::from_le_bytes(*bytes))
    }

    #[inline(always)]
    fn u8(&mut self) -> Result<u8, DecodeError> {
        self.array::<1>().map(|&[byte]| byte)
    }
}

#[cfg(test)]
mod test {
    // A panic is the desired failure mode in a test.
    #![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]

    use super::*;

    /// An instruction's registered encoder: encodes a sample request, and reports which instruction the encoder is
    /// typed for.
    type Encoder = fn() -> (Instruction, Vec<u8>);

    fn registered<R: Request>(request: &R) -> (Instruction, Vec<u8>) {
        (R::INSTRUCTION, request.to_vec())
    }

    const KEY: [u8; 32] = [0x11; 32];
    const CHALLENGE: [u8; 64] = [0x22; 64];

    fn sample_script_signature_common() -> ScriptSignatureCommon<'static> {
        ScriptSignatureCommon {
            account: 1,
            network: 2,
            txi_version: 3,
            value: &KEY,
            commitment_private_key: &KEY,
            commitment: &KEY,
            message: &KEY,
        }
    }

    /// Every instruction's encoder.
    ///
    /// An exhaustive `match` on purpose, in the style of `test_instruction_conversion`: a new `Instruction` variant
    /// without an entry here is a compile error, not a row that is silently skipped. And because each arm reports
    /// the instruction its encoder is typed for, an arm pointing at the wrong encoder fails the test below.
    fn encoder(instruction: Instruction) -> Encoder {
        match instruction {
            Instruction::GetVersion => || registered(&GetVersionRequest { account: 1 }),
            Instruction::GetAppName => || registered(&GetAppNameRequest { account: 1 }),
            Instruction::GetPublicSpendKey => || registered(&GetPublicSpendKeyRequest { account: 1 }),
            Instruction::GetPublicKey => || {
                registered(&GetPublicKeyRequest {
                    account: 1,
                    index: 2,
                    branch: 3,
                })
            },
            Instruction::GetScriptSignatureDerived => || {
                registered(&GetScriptSignatureDerivedRequest {
                    common: sample_script_signature_common(),
                    blinding_factor: &KEY,
                })
            },
            Instruction::GetScriptOffset => || {
                let request = ScriptOffsetRequest {
                    account: 1,
                    sender_offset_count: 2,
                    partial_script_key_sum: &KEY,
                    script_key_indexes: &[(3, 4)],
                    derived_script_keys: &[&KEY],
                };
                let header = request.chunks().next().expect("an exchange always has a header chunk");
                registered(&header)
            },
            Instruction::GetViewKey => || registered(&GetViewKeyRequest { account: 1 }),
            Instruction::GetDHSharedSecret => || {
                registered(&GetDHSharedSecretRequest {
                    account: 1,
                    index: 2,
                    branch: 3,
                    public_key: &KEY,
                })
            },
            Instruction::GetRawSchnorrSignature => || {
                registered(&GetRawSchnorrSignatureRequest {
                    account: 1,
                    index: 2,
                    branch: 3,
                    nonce_handle: 4,
                    challenge: &CHALLENGE,
                })
            },
            Instruction::GetScriptSchnorrSignature => || {
                registered(&GetScriptSchnorrSignatureRequest {
                    account: 1,
                    index: 2,
                    branch: 3,
                    message: &KEY,
                })
            },
            Instruction::GetOneSidedMetadataSignature => || {
                registered(
                    &GetOneSidedMetadataSignatureRequest::new(1, 2, 3, 4, 6, 5, &KEY, &[0x22; 67], &KEY)
                        .expect("a 67 byte address fits its length prefix"),
                )
            },
            Instruction::GetScriptSignatureManaged => || {
                registered(&GetScriptSignatureManagedRequest {
                    common: sample_script_signature_common(),
                    branch: 3,
                    index: 4,
                })
            },
            Instruction::GenerateEphemeralNonce => || registered(&GenerateEphemeralNonceRequest { account: 1 }),
            Instruction::GetRawSchnorrSignatureLegacyNonce => || {
                registered(&GetRawSchnorrSignatureLegacyNonceRequest {
                    account: 1,
                    key_index: 2,
                    key_branch: 3,
                    nonce_index: 4,
                    nonce_branch: 5,
                    challenge: &CHALLENGE,
                })
            },
        }
    }

    /// Every instruction the protocol has, found by walking the whole byte range through `from_byte` - itself
    /// pinned exhaustively by `test_instruction_conversion` - so that the list cannot fall behind the enum.
    fn every_instruction() -> Vec<Instruction> {
        (0..=u8::MAX).filter_map(Instruction::from_byte).collect()
    }

    #[test]
    fn every_instruction_has_a_registered_encoder() {
        let instructions = every_instruction();
        assert_eq!(instructions.len(), 14, "a new instruction needs a registered encoder");

        for instruction in instructions {
            let (encoded_as, bytes) = encoder(instruction)();
            assert_eq!(
                encoded_as, instruction,
                "{instruction:?} is registered with the encoder for {encoded_as:?}"
            );
            assert!(
                bytes.len() >= ACCOUNT_SIZE,
                "{instruction:?} encodes {} bytes, which cannot hold the account",
                bytes.len()
            );
        }
    }

    #[test]
    fn a_reader_hands_out_fields_in_order_and_refuses_to_run_past_the_end() {
        let data = [1, 0, 0, 0, 0, 0, 0, 0, 0xaa, 0xbb];
        let mut reader = Reader::exact(&data, 10).unwrap();
        assert_eq!(reader.u64(), Ok(1));
        assert_eq!(reader.u8(), Ok(0xaa));
        assert_eq!(reader.array::<1>(), Ok(&[0xbb]));
        assert_eq!(reader.u8(), Err(DecodeError::WrongLength));

        assert!(Reader::exact(&data, 9).is_err());
        assert!(Reader::exact(&data, 11).is_err());
        assert!(Reader::at_least(&data, 10).is_ok());
        assert!(Reader::at_least(&data, 9).is_ok());
        assert!(Reader::at_least(&data, 11).is_err());
    }
}
