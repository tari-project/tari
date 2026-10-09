// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! `GetOneSidedMetadataSignature`: the one request with a variable length field.
//!
//! # Layout
//!
//! `account(8) | network(8) | txo_version(8) | sender_offset_key_index(8) | value(8) | commitment_mask(32) |
//! address_size(2) | receiver_address(address_size) | message(32) [| sender_offset_branch(8) [| is_change(1)]]`
//!
//! `network`, `txo_version` and `sender_offset_branch` are bytes widened to little endian `u64`s, `address_size` is a
//! little endian `u16`, `is_change` is `0` or `1` and anything else is refused, and anything after `is_change` is
//! ignored.
//!
//! `sender_offset_branch` names the branch the sender offset key is on: `OneSidedSenderOffset` for an ordinary
//! one-sided output, `PreMine` for one whose sender offset `GetScriptOffset` issued in pre-mine mode (the backup
//! pre-mine spend). The device refuses any other branch before its review.
//!
//! It is an optional *trailing* field, appended after `message`, and every field before it keeps the offset it always
//! had. Anything after `message` used to be ignored, which is what lets the format grow this way: hosts only enforce a
//! minimum application version, so an older host talks to a newer application, and its payload - which stops at
//! `message`, or carries fewer than eight bytes after it - decodes exactly as before, with the branch defaulting to
//! `OneSidedSenderOffset` ([`DEFAULT_SENDER_OFFSET_BRANCH`]).
//!
//! `is_change` is a second optional trailing field, after the branch: the host sets it for the change output the
//! wallet's transaction builder made, and the device auto-approves only a flagged output to the wallet's own address.
//! A payload without it - every payload a host from before it sends - is not change, so such a host's change is
//! reviewed rather than refused. Current hosts send it only when it is set, and then send the branch in front of it
//! even when the branch is the default.
//!
//! Current hosts send the branch only when it is *not* that default (or `is_change` is set). An ordinary
//! one-sided send is therefore byte-identical to the old layout, and keeps the old layout's room for a receiver
//! address: the whole payload has to fit one APDU ([`MAX_APDU_DATA_SIZE`]), because the transport writes its length
//! as a single byte. Appending eight bytes to every request would have moved that ceiling for every send to a dual
//! address with a long payment id.
//!
//! # Why the device decodes this in three steps
//!
//! Every other request is one exact length check followed by fields at fixed offsets, so decoding it all up front
//! changes nothing about which check fails first. This one is not. The device has always interleaved its length
//! checks with its validation:
//!
//! 1. at least [`GetOneSidedMetadataSignatureRequest::MIN_SIZE`] bytes, else `WrongApduLength`;
//! 2. `commitment_mask` is canonical, else `KeyDeriveFromCanonical`;
//! 3. `address_size` is a plausible dual address size and the address fits, else `WrongApduLength`;
//! 4. the address has a valid checksum, else `MetadataSignatureFail`;
//! 5. `message` fits after the address, else `WrongApduLength`.
//!
//! A payload that fails more than one of these gets the status word of the *first*, so a decoder that did every
//! length check first would change the answer to some malformed requests - which a refactor must not do. The
//! decoder is therefore staged to match: [`OneSidedMetadataSignatureHead`] for step 1,
//! [`OneSidedMetadataSignatureHead::receiver_address`] for step 3 and [`OneSidedMetadataSignatureTail::message`] for
//! step 5, with the device's own checks in between. The full [`Decode`] impl runs the three back to back, for
//! callers that have no validation to interleave - the host and tests - and is compiled only for them.

use super::{ACCOUNT_SIZE, Decode, DecodeError, Encode, Reader, Request, Writer, write_u64};
use crate::{TARI_DUAL_ADDRESS_MAX_SIZE, TARI_DUAL_ADDRESS_MIN_SIZE, common_types::Instruction};

/// Offset of `address_size`: everything before it is fixed width.
const FIXED_SIZE: usize = ACCOUNT_SIZE + 8 * 4 + 32;
const ADDRESS_SIZE_SIZE: usize = 2;
const MESSAGE_SIZE: usize = 32;
const SENDER_OFFSET_BRANCH_SIZE: usize = 8;
const IS_CHANGE_SIZE: usize = 1;

/// The most data one APDU can carry: `ledger-apdu` serialises the data length as a single byte (`len() as u8`), so a
/// longer payload does not fail - its length silently wraps, and the device answers `WrongApduLength`. See
/// [`GetOneSidedMetadataSignatureRequest::fits_in_one_apdu`].
pub const MAX_APDU_DATA_SIZE: usize = 255;

/// The `sender_offset_branch` of a payload that does not carry one: `OneSidedSenderOffset`, the only branch a host
/// from before the field existed ever meant.
pub const DEFAULT_SENDER_OFFSET_BRANCH: u64 = 0x06;
const _: () =
    assert!(DEFAULT_SENDER_OFFSET_BRANCH == crate::common_types::LedgerKeyBranch::OneSidedSenderOffset as u64);

// `MIN_SIZE` is two bytes short of the shortest payload that can actually be valid - see its docs. Pinned, so that
// neither side of the gap can move without the other being looked at.
const _: () = assert!(
    GetOneSidedMetadataSignatureRequest::MIN_SIZE + 2 ==
        FIXED_SIZE + ADDRESS_SIZE_SIZE + TARI_DUAL_ADDRESS_MIN_SIZE + MESSAGE_SIZE
);

/// The receiver address was too long for its `u16` length prefix.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct ReceiverAddressTooLong;

/// `GetOneSidedMetadataSignature`, as the host sends it. See the module docs for the layout.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct GetOneSidedMetadataSignatureRequest<'a> {
    pub account: u64,
    pub network: u64,
    pub txo_version: u64,
    pub sender_offset_key_index: u64,
    pub sender_offset_branch: u64,
    /// Whether this is the change output the wallet's transaction builder made. See the module docs.
    pub is_change: bool,
    pub value: u64,
    pub commitment_mask: &'a [u8; 32],
    /// Private so that it can only be set through [`Self::new`], which is what guarantees the `u16` length prefix
    /// can hold it. The encoder cannot fail, so it must never be handed a length it would have to truncate.
    receiver_address: &'a [u8],
    pub message: &'a [u8; 32],
}

impl<'a> GetOneSidedMetadataSignatureRequest<'a> {
    /// The shortest payload the device reads any further than.
    ///
    /// This is **not** the shortest valid payload. That would be the fixed fields, the length prefix, a minimum size
    /// dual address and the message - 173 bytes; the device has always checked for 171, two short. It is kept at 171
    /// because the gap is observable: a 171 or 172 byte payload gets as far as the `commitment_mask` and address
    /// checks, and fails with *their* status word if they fail, before the missing message is noticed. The optional
    /// trailing `sender_offset_branch` does not count towards it.
    pub const MIN_SIZE: usize = 171;

    /// Build a request, refusing a receiver address its `u16` length prefix cannot describe.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        account: u64,
        network: u64,
        txo_version: u64,
        sender_offset_key_index: u64,
        sender_offset_branch: u64,
        is_change: bool,
        value: u64,
        commitment_mask: &'a [u8; 32],
        receiver_address: &'a [u8],
        message: &'a [u8; 32],
    ) -> Result<Self, ReceiverAddressTooLong> {
        u16::try_from(receiver_address.len()).map_err(|_| ReceiverAddressTooLong)?;
        Ok(Self {
            account,
            network,
            txo_version,
            sender_offset_key_index,
            sender_offset_branch,
            is_change,
            value,
            commitment_mask,
            receiver_address,
            message,
        })
    }

    /// The serialised receiver address.
    pub fn receiver_address(&self) -> &'a [u8] {
        self.receiver_address
    }

    /// Whether the trailing branch is sent: when it is not the default, or when `is_change` follows it.
    fn sends_branch(&self) -> bool {
        self.sender_offset_branch != DEFAULT_SENDER_OFFSET_BRANCH || self.is_change
    }

    /// The encoded length: the old layout, plus the trailing branch and `is_change` when they are sent.
    pub fn encoded_len(&self) -> usize {
        let branch = if self.sends_branch() {
            SENDER_OFFSET_BRANCH_SIZE
        } else {
            0
        };
        let is_change = if self.is_change { IS_CHANGE_SIZE } else { 0 };
        FIXED_SIZE
            .saturating_add(ADDRESS_SIZE_SIZE)
            .saturating_add(self.receiver_address.len())
            .saturating_add(MESSAGE_SIZE)
            .saturating_add(branch)
            .saturating_add(is_change)
    }

    /// Whether the encoded request fits one APDU ([`MAX_APDU_DATA_SIZE`]). A request that does not would go out with a
    /// wrapped length byte, so the host refuses it instead of sending it.
    pub fn fits_in_one_apdu(&self) -> bool {
        self.encoded_len() <= MAX_APDU_DATA_SIZE
    }
}

impl Encode for GetOneSidedMetadataSignatureRequest<'_> {
    fn encode(&self, out: &mut impl Writer) {
        write_u64(out, self.account);
        write_u64(out, self.network);
        write_u64(out, self.txo_version);
        write_u64(out, self.sender_offset_key_index);
        write_u64(out, self.value);
        out.write(self.commitment_mask);
        // `new` has already refused anything longer, so this never saturates.
        let address_size = u16::try_from(self.receiver_address.len()).unwrap_or(u16::MAX);
        out.write(&address_size.to_le_bytes());
        out.write(self.receiver_address);
        out.write(self.message);
        // Appended, so that every field above keeps its offset, and only when it is needed, so that an ordinary send
        // is the old layout byte for byte; see the module docs.
        if self.sends_branch() {
            write_u64(out, self.sender_offset_branch);
        }
        if self.is_change {
            out.write(&[1]);
        }
    }
}

/// The one-shot decode, for the host and for tests only.
///
/// It runs every length check - the head, the address, the message - before anything else can look at the fields,
/// which is exactly the ordering the device must *not* use: the device interleaves them with the commitment mask and
/// address checksum checks, and a malformed request gets the status word of whichever check fires first (see the
/// module docs). Gated off the device build (which does not enable `alloc`), so that a handler cannot reach for this
/// and silently change which status word a malformed request gets. The device decodes in stages instead.
#[cfg(any(feature = "alloc", test))]
impl<'a> Decode<'a> for GetOneSidedMetadataSignatureRequest<'a> {
    fn decode(data: &'a [u8]) -> Result<Self, DecodeError> {
        let head = OneSidedMetadataSignatureHead::decode(data)?;
        let tail = head.receiver_address()?;
        Ok(Self {
            account: head.account,
            network: head.network,
            txo_version: head.txo_version,
            sender_offset_key_index: head.sender_offset_key_index,
            sender_offset_branch: tail.sender_offset_branch()?,
            is_change: tail.is_change()?,
            value: head.value,
            commitment_mask: head.commitment_mask,
            receiver_address: tail.receiver_address,
            message: tail.message()?,
        })
    }
}

impl Request for GetOneSidedMetadataSignatureRequest<'_> {
    const INSTRUCTION: Instruction = Instruction::GetOneSidedMetadataSignature;
}

/// Step 1 of decoding `GetOneSidedMetadataSignature`: the fixed width fields. See the module docs.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct OneSidedMetadataSignatureHead<'a> {
    pub account: u64,
    pub network: u64,
    pub txo_version: u64,
    pub sender_offset_key_index: u64,
    pub value: u64,
    pub commitment_mask: &'a [u8; 32],
    /// Everything from `address_size` on.
    rest: &'a [u8],
}

impl<'a> Decode<'a> for OneSidedMetadataSignatureHead<'a> {
    fn decode(data: &'a [u8]) -> Result<Self, DecodeError> {
        let mut reader = Reader::at_least(data, GetOneSidedMetadataSignatureRequest::MIN_SIZE)?;
        Ok(Self {
            account: reader.u64()?,
            network: reader.u64()?,
            txo_version: reader.u64()?,
            sender_offset_key_index: reader.u64()?,
            value: reader.u64()?,
            commitment_mask: reader.array()?,
            rest: reader.rest,
        })
    }
}

impl<'a> OneSidedMetadataSignatureHead<'a> {
    /// Step 3: the receiver address.
    ///
    /// Refuses an `address_size` outside the sizes a dual address can have, and an address that runs past the end
    /// of the payload. Whether the address is *valid* - its checksum - is the device's step 4, not this.
    pub fn receiver_address(&self) -> Result<OneSidedMetadataSignatureTail<'a>, DecodeError> {
        let mut reader = Reader { rest: self.rest };
        let address_size = usize::from(u16::from_le_bytes(*reader.array::<ADDRESS_SIZE_SIZE>()?));
        if !(TARI_DUAL_ADDRESS_MIN_SIZE..=TARI_DUAL_ADDRESS_MAX_SIZE).contains(&address_size) {
            return Err(DecodeError::WrongLength);
        }
        let (receiver_address, rest) = reader
            .rest
            .split_at_checked(address_size)
            .ok_or(DecodeError::WrongLength)?;
        Ok(OneSidedMetadataSignatureTail { receiver_address, rest })
    }
}

/// Step 3's result: the receiver address, and what follows it.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct OneSidedMetadataSignatureTail<'a> {
    pub receiver_address: &'a [u8],
    rest: &'a [u8],
}

impl<'a> OneSidedMetadataSignatureTail<'a> {
    /// Step 5: the message after the address.
    pub fn message(&self) -> Result<&'a [u8; 32], DecodeError> {
        let mut reader = Reader::at_least(self.rest, MESSAGE_SIZE)?;
        reader.array()
    }

    /// The optional `sender_offset_branch` after the message, or [`DEFAULT_SENDER_OFFSET_BRANCH`] when fewer than
    /// eight bytes follow it - which is every payload a host from before the field existed sends. Bytes after it are
    /// ignored. Only a payload short of the message itself is an error, the same one [`Self::message`] gives.
    pub fn sender_offset_branch(&self) -> Result<u64, DecodeError> {
        let mut reader = Reader::at_least(self.rest, MESSAGE_SIZE)?;
        reader.array::<MESSAGE_SIZE>()?;
        if reader.rest.len() < SENDER_OFFSET_BRANCH_SIZE {
            return Ok(DEFAULT_SENDER_OFFSET_BRANCH);
        }
        reader.u64()
    }

    /// The optional `is_change` after the branch, or `false` when the payload stops before it - which is every
    /// payload a host from before the field existed sends. `0` and `1` are the only values; anything else is refused.
    /// Bytes after it are ignored.
    pub fn is_change(&self) -> Result<bool, DecodeError> {
        let mut reader = Reader::at_least(self.rest, MESSAGE_SIZE)?;
        reader.array::<MESSAGE_SIZE>()?;
        if reader.rest.len() < SENDER_OFFSET_BRANCH_SIZE.saturating_add(IS_CHANGE_SIZE) {
            return Ok(false);
        }
        reader.u64()?;
        match reader.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(DecodeError::WrongLength),
        }
    }
}

#[cfg(test)]
mod test {
    // A panic is the desired failure mode in a test.
    #![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]

    use alloc::{vec, vec::Vec};

    use super::*;

    const MASK: [u8; 32] = [0x41; 32];
    const MESSAGE: [u8; 32] = [0x42; 32];

    fn request(address: &[u8]) -> GetOneSidedMetadataSignatureRequest<'_> {
        request_on(address, 0x09)
    }

    fn request_on(address: &[u8], sender_offset_branch: u64) -> GetOneSidedMetadataSignatureRequest<'_> {
        change_request_on(address, sender_offset_branch, false)
    }

    fn change_request_on(
        address: &[u8],
        sender_offset_branch: u64,
        is_change: bool,
    ) -> GetOneSidedMetadataSignatureRequest<'_> {
        GetOneSidedMetadataSignatureRequest::new(
            1,
            0x26,
            1,
            7,
            sender_offset_branch,
            is_change,
            1_000_000,
            &MASK,
            address,
            &MESSAGE,
        )
        .unwrap()
    }

    #[test]
    fn it_round_trips_with_the_address_size_in_front_of_the_address() {
        let address = vec![0xaa; TARI_DUAL_ADDRESS_MIN_SIZE + 3];
        let bytes = request(&address).to_vec();
        assert_eq!(bytes.len(), FIXED_SIZE + 2 + address.len() + 32 + 8);
        assert_eq!(bytes.len(), request(&address).encoded_len());
        // Every field keeps the offset it had before `sender_offset_branch` existed...
        assert_eq!(&bytes[24..32], &7u64.to_le_bytes());
        assert_eq!(&bytes[32..40], &1_000_000u64.to_le_bytes());
        assert_eq!(&bytes[40..72], &MASK);
        assert_eq!(&bytes[72..74], &70u16.to_le_bytes());
        assert_eq!(&bytes[74..144], address.as_slice());
        assert_eq!(&bytes[144..176], &MESSAGE);
        // ...and the branch is appended after the message.
        assert_eq!(&bytes[176..], &0x09u64.to_le_bytes());
        assert_eq!(
            GetOneSidedMetadataSignatureRequest::decode(&bytes),
            Ok(request(&address))
        );
    }

    #[test]
    fn the_host_cannot_build_an_address_its_length_prefix_cannot_hold() {
        let address = vec![0; usize::from(u16::MAX) + 1];
        assert_eq!(
            GetOneSidedMetadataSignatureRequest::new(0, 0, 0, 0, 0, false, 0, &MASK, &address, &MESSAGE),
            Err(ReceiverAddressTooLong)
        );
        let address = vec![0; usize::from(u16::MAX)];
        assert!(GetOneSidedMetadataSignatureRequest::new(0, 0, 0, 0, 0, false, 0, &MASK, &address, &MESSAGE).is_ok());
    }

    /// The staging is the point of this module: each stage fails only on its own length check, so a payload that
    /// is short in a later field still reaches the device's checks on an earlier one.
    #[test]
    fn a_payload_short_in_its_message_still_yields_its_head_and_address() {
        let address = vec![0xaa; TARI_DUAL_ADDRESS_MIN_SIZE];
        let bytes = request(&address).to_vec();
        // 171 bytes: two short of the message, but not short of the minimum the device checks first.
        let short = &bytes[..GetOneSidedMetadataSignatureRequest::MIN_SIZE];
        let head = OneSidedMetadataSignatureHead::decode(short).unwrap();
        assert_eq!(head.commitment_mask, &MASK);
        let tail = head.receiver_address().unwrap();
        assert_eq!(tail.receiver_address, address.as_slice());
        assert_eq!(tail.message(), Err(DecodeError::WrongLength));

        assert!(
            OneSidedMetadataSignatureHead::decode(&bytes[..GetOneSidedMetadataSignatureRequest::MIN_SIZE - 1]).is_err()
        );
    }

    #[test]
    fn an_address_size_outside_the_dual_address_range_is_refused_before_it_is_read() {
        let mut bytes = request(&[0xaa; TARI_DUAL_ADDRESS_MIN_SIZE]).to_vec();
        for size in [
            0,
            TARI_DUAL_ADDRESS_MIN_SIZE - 1,
            TARI_DUAL_ADDRESS_MAX_SIZE + 1,
            usize::from(u16::MAX),
        ] {
            let size = u16::try_from(size).unwrap();
            bytes[72..74].copy_from_slice(&size.to_le_bytes());
            let head = OneSidedMetadataSignatureHead::decode(&bytes).unwrap();
            assert_eq!(head.receiver_address(), Err(DecodeError::WrongLength), "size {size}");
        }
    }

    #[test]
    fn an_address_that_runs_past_the_end_is_refused() {
        let mut bytes: Vec<u8> = request(&[0xaa; TARI_DUAL_ADDRESS_MIN_SIZE]).to_vec();
        let size = u16::try_from(TARI_DUAL_ADDRESS_MAX_SIZE).unwrap();
        bytes[72..74].copy_from_slice(&size.to_le_bytes());
        let head = OneSidedMetadataSignatureHead::decode(&bytes).unwrap();
        assert_eq!(head.receiver_address(), Err(DecodeError::WrongLength));
    }

    /// A change request carries the branch, even the default one, and then `is_change`; both decoders read it, and
    /// bytes after it are ignored.
    #[test]
    fn a_change_request_round_trips_and_bytes_after_it_are_ignored() {
        let address = vec![0xaa; TARI_DUAL_ADDRESS_MIN_SIZE];
        let change = change_request_on(&address, DEFAULT_SENDER_OFFSET_BRANCH, true);
        let mut bytes = change.to_vec();
        assert_eq!(bytes.len(), FIXED_SIZE + 2 + address.len() + 32 + 8 + 1);
        assert_eq!(bytes.len(), change.encoded_len());
        assert_eq!(
            &bytes[bytes.len() - 9..bytes.len() - 1],
            &DEFAULT_SENDER_OFFSET_BRANCH.to_le_bytes()
        );
        assert_eq!(bytes[bytes.len() - 1], 1);
        assert_eq!(GetOneSidedMetadataSignatureRequest::decode(&bytes), Ok(change));

        bytes.extend_from_slice(&[0xff; 5]);
        assert_eq!(GetOneSidedMetadataSignatureRequest::decode(&bytes), Ok(change));
        let tail = OneSidedMetadataSignatureHead::decode(&bytes)
            .unwrap()
            .receiver_address()
            .unwrap();
        assert_eq!(tail.is_change(), Ok(true));
    }

    /// `is_change` is `0` or `1`, and nothing else; an explicit `0` is not change.
    #[test]
    fn is_change_is_strict() {
        let address = vec![0xaa; TARI_DUAL_ADDRESS_MIN_SIZE];
        let mut bytes = request(&address).to_vec();
        bytes.push(0);
        assert_eq!(
            GetOneSidedMetadataSignatureRequest::decode(&bytes),
            Ok(request(&address))
        );
        for value in [2, 0x80, 0xff] {
            let last = bytes.len() - 1;
            bytes[last] = value;
            assert_eq!(
                GetOneSidedMetadataSignatureRequest::decode(&bytes),
                Err(DecodeError::WrongLength),
                "is_change {value}"
            );
        }
    }

    /// A payload that stops at the branch - every current non-change request with a branch, and every request from a
    /// host from before `is_change` existed - is not change.
    #[test]
    fn a_payload_without_is_change_is_not_change() {
        let address = vec![0xaa; TARI_DUAL_ADDRESS_MIN_SIZE];
        let bytes = request(&address).to_vec();
        assert_eq!(
            GetOneSidedMetadataSignatureRequest::decode(&bytes).map(|r| r.is_change),
            Ok(false)
        );
        let old_layout = request_on(&address, DEFAULT_SENDER_OFFSET_BRANCH).to_vec();
        let tail = OneSidedMetadataSignatureHead::decode(&old_layout)
            .unwrap()
            .receiver_address()
            .unwrap();
        assert_eq!(tail.is_change(), Ok(false));
    }

    /// The default branch is not sent: an ordinary send is the old layout byte for byte, so it keeps the old layout's
    /// room for a receiver address.
    #[test]
    fn the_default_branch_encodes_as_the_old_layout() {
        let address = vec![0xaa; TARI_DUAL_ADDRESS_MIN_SIZE + 3];
        let with_branch = request(&address).to_vec();
        let ordinary = request_on(&address, DEFAULT_SENDER_OFFSET_BRANCH);
        let bytes = ordinary.to_vec();
        assert_eq!(bytes.len(), FIXED_SIZE + 2 + address.len() + 32);
        assert_eq!(bytes.len(), ordinary.encoded_len());
        assert_eq!(bytes.as_slice(), &with_branch[..with_branch.len() - 8]);
        assert_eq!(GetOneSidedMetadataSignatureRequest::decode(&bytes), Ok(ordinary));
    }

    /// A host from before `sender_offset_branch` existed stops at the message. Its payload must decode exactly as it
    /// always did - same fields from the same offsets - with the branch defaulting to `OneSidedSenderOffset`, so an
    /// older host's ordinary one-sided send still works against this application.
    #[test]
    fn an_old_layout_payload_decodes_with_the_default_branch() {
        let address = vec![0xaa; TARI_DUAL_ADDRESS_MIN_SIZE + 3];
        let mut old_layout = request(&address).to_vec();
        old_layout.truncate(old_layout.len() - 8);
        assert_eq!(old_layout.len(), FIXED_SIZE + 2 + address.len() + 32);

        let expected = request_on(&address, DEFAULT_SENDER_OFFSET_BRANCH);
        assert_eq!(GetOneSidedMetadataSignatureRequest::decode(&old_layout), Ok(expected));

        // Fewer than eight trailing bytes - which an old host was free to send, and which were ignored - still mean
        // "no branch".
        for extra in 1..8 {
            let mut bytes = old_layout.clone();
            bytes.extend_from_slice(&vec![0xff; extra]);
            assert_eq!(
                GetOneSidedMetadataSignatureRequest::decode(&bytes),
                Ok(expected),
                "{extra} trailing bytes"
            );
        }

        // The staged decode the device uses agrees.
        let head = OneSidedMetadataSignatureHead::decode(&old_layout).unwrap();
        assert_eq!(head.value, 1_000_000);
        assert_eq!(
            head.receiver_address().unwrap().sender_offset_branch(),
            Ok(DEFAULT_SENDER_OFFSET_BRANCH)
        );
    }

    /// The largest address the old layout fits in one APDU - `255 - 106` = 149 bytes - still does with the default
    /// branch, and decodes. With the `PreMine` branch it is eight bytes too long and is reported as such.
    #[test]
    fn the_largest_old_layout_address_still_fits_one_apdu() {
        let largest = MAX_APDU_DATA_SIZE - (FIXED_SIZE + 2 + 32);
        assert_eq!(largest, 149);
        let address = vec![0xaa; largest];
        let ordinary = request_on(&address, DEFAULT_SENDER_OFFSET_BRANCH);
        assert!(ordinary.fits_in_one_apdu());
        let bytes = ordinary.to_vec();
        assert_eq!(bytes.len(), MAX_APDU_DATA_SIZE);
        assert_eq!(GetOneSidedMetadataSignatureRequest::decode(&bytes), Ok(ordinary));

        assert!(!request_on(&address, 0x09).fits_in_one_apdu());
        assert!(request_on(&address[..largest - 8], 0x09).fits_in_one_apdu());
        let one_more = vec![0xaa; largest + 1];
        assert!(!request_on(&one_more, DEFAULT_SENDER_OFFSET_BRANCH).fits_in_one_apdu());
    }

    /// A current host's payload carries the branch, and both decoders read it.
    #[test]
    fn a_new_layout_payload_decodes_its_branch() {
        let address = vec![0xaa; TARI_DUAL_ADDRESS_MIN_SIZE];
        for branch in [0x09, 0x07, 0x08, 0xff] {
            let bytes = request_on(&address, branch).to_vec();
            assert_eq!(
                GetOneSidedMetadataSignatureRequest::decode(&bytes).map(|r| r.sender_offset_branch),
                Ok(branch)
            );
            let tail = OneSidedMetadataSignatureHead::decode(&bytes)
                .unwrap()
                .receiver_address()
                .unwrap();
            assert_eq!(tail.sender_offset_branch(), Ok(branch));
        }
    }

    /// A payload short of the message is still refused by the branch read, with the message's own error.
    #[test]
    fn the_branch_read_needs_the_message() {
        let address = vec![0xaa; TARI_DUAL_ADDRESS_MIN_SIZE];
        let bytes = request(&address).to_vec();
        let tail = OneSidedMetadataSignatureHead::decode(&bytes[..GetOneSidedMetadataSignatureRequest::MIN_SIZE])
            .unwrap()
            .receiver_address()
            .unwrap();
        assert_eq!(tail.sender_offset_branch(), Err(DecodeError::WrongLength));
    }
}
