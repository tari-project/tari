// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! `GetOneSidedMetadataSignature`: the one request with a variable length field.
//!
//! # Layout
//!
//! `account(8) | network(1) | txo_version(1) | sender_offset_key_index(8) | sender_offset_branch(1) | value(8) |
//! commitment_mask(32) | address_size(2) | receiver_address(address_size) | message(32)`
//!
//! `network`, `txo_version` and `sender_offset_branch` are single bytes - they are bytes on both sides, and were
//! widened to `u64`s until application 6.1.1-pre.1 narrowed them, which buys back the room `sender_offset_branch`
//! took: a 149 byte receiver address (an 82 byte payment ID) fits one APDU again. `address_size` is a little endian
//! `u16`, and anything after `message` is ignored.
//!
//! `sender_offset_branch` names the branch the sender offset key is on: `OneSidedSenderOffset` for an ordinary
//! one-sided output, `PreMine` for one whose sender offset `GetScriptOffset` issued in pre-mine mode (the backup
//! pre-mine spend). The device refuses any other branch before its review.
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
const FIXED_SIZE: usize = ACCOUNT_SIZE + 1 + 1 + 8 + 1 + 8 + 32;
const ADDRESS_SIZE_SIZE: usize = 2;
const MESSAGE_SIZE: usize = 32;

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
    pub network: u8,
    pub txo_version: u8,
    pub sender_offset_key_index: u64,
    pub sender_offset_branch: u8,
    pub value: u64,
    pub commitment_mask: &'a [u8; 32],
    /// Private so that it can only be set through [`Self::new`], which is what guarantees the `u16` length prefix
    /// can hold it. The encoder cannot fail, so it must never be handed a length it would have to truncate.
    receiver_address: &'a [u8],
    pub message: &'a [u8; 32],
}

impl<'a> GetOneSidedMetadataSignatureRequest<'a> {
    /// The longest receiver address (with its payment ID) that fits [`Self::MAX_SIZE`].
    pub const MAX_ADDRESS_SIZE: usize = Self::MAX_SIZE - FIXED_SIZE - ADDRESS_SIZE_SIZE - MESSAGE_SIZE;
    /// The most a single APDU can carry: its `Lc` is one byte. The transport builds `Lc` as `len as u8`, so a longer
    /// payload is not refused there - it is silently truncated - and the host has to refuse it before sending.
    pub const MAX_SIZE: usize = 255;
    /// The shortest payload the device reads any further than.
    ///
    /// This is **not** the shortest valid payload. That would be the fixed fields, the length prefix, a minimum size
    /// dual address and the message - 160 bytes; the device has always checked for two short of that (171 of 173
    /// in the layout before 6.1.1-pre.0). It is kept two short because the gap is observable: a 158 or 159 byte
    /// payload gets as far as the `commitment_mask` and address checks, and fails with *their* status word if they
    /// fail, before the missing message is noticed.
    pub const MIN_SIZE: usize = 158;

    /// Build a request, refusing a receiver address its `u16` length prefix cannot describe.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        account: u64,
        network: u8,
        txo_version: u8,
        sender_offset_key_index: u64,
        sender_offset_branch: u8,
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

    /// The encoded length, account included - what goes in the APDU's `Lc`. Compare with [`Self::MAX_SIZE`].
    pub fn encoded_len(&self) -> usize {
        FIXED_SIZE
            .saturating_add(ADDRESS_SIZE_SIZE)
            .saturating_add(self.receiver_address.len())
            .saturating_add(MESSAGE_SIZE)
    }
}

impl Encode for GetOneSidedMetadataSignatureRequest<'_> {
    fn encode(&self, out: &mut impl Writer) {
        write_u64(out, self.account);
        out.write(&[self.network, self.txo_version]);
        write_u64(out, self.sender_offset_key_index);
        out.write(&[self.sender_offset_branch]);
        write_u64(out, self.value);
        out.write(self.commitment_mask);
        // `new` has already refused anything longer, so this never saturates.
        let address_size = u16::try_from(self.receiver_address.len()).unwrap_or(u16::MAX);
        out.write(&address_size.to_le_bytes());
        out.write(self.receiver_address);
        out.write(self.message);
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
            sender_offset_branch: head.sender_offset_branch,
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
    pub network: u8,
    pub txo_version: u8,
    pub sender_offset_key_index: u64,
    pub sender_offset_branch: u8,
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
            network: reader.u8()?,
            txo_version: reader.u8()?,
            sender_offset_key_index: reader.u64()?,
            sender_offset_branch: reader.u8()?,
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
    /// Step 5: the message after the address. Anything after the message is ignored, as it always has been.
    pub fn message(&self) -> Result<&'a [u8; 32], DecodeError> {
        let mut reader = Reader::at_least(self.rest, MESSAGE_SIZE)?;
        reader.array()
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
        GetOneSidedMetadataSignatureRequest::new(1, 0x26, 1, 7, 0x06, 1_000_000, &MASK, address, &MESSAGE).unwrap()
    }

    #[test]
    fn it_round_trips_with_the_address_size_in_front_of_the_address() {
        let address = vec![0xaa; TARI_DUAL_ADDRESS_MIN_SIZE + 3];
        let bytes = request(&address).to_vec();
        assert_eq!(bytes.len(), FIXED_SIZE + 2 + address.len() + 32);
        assert_eq!(&bytes[0..8], &1u64.to_le_bytes());
        assert_eq!(bytes[8], 0x26, "network, one byte");
        assert_eq!(bytes[9], 1, "txo_version, one byte");
        assert_eq!(&bytes[10..18], &7u64.to_le_bytes());
        assert_eq!(bytes[18], 0x06, "the sender offset branch, one byte, follows its index");
        assert_eq!(&bytes[19..27], &1_000_000u64.to_le_bytes());
        assert_eq!(&bytes[27..59], &MASK);
        assert_eq!(&bytes[59..61], &70u16.to_le_bytes());
        assert_eq!(&bytes[61..131], address.as_slice());
        assert_eq!(&bytes[131..], &MESSAGE);
        assert_eq!(
            GetOneSidedMetadataSignatureRequest::decode(&bytes),
            Ok(request(&address))
        );
    }

    #[test]
    fn the_encoded_length_is_what_is_encoded() {
        for size in [TARI_DUAL_ADDRESS_MIN_SIZE, 149, 162, 163, TARI_DUAL_ADDRESS_MAX_SIZE] {
            let address = vec![0xaa; size];
            let request = request(&address);
            assert_eq!(request.encoded_len(), request.to_vec().len(), "address of {size} bytes");
        }
        // 162 bytes of address is the most a single APDU can carry.
        assert_eq!(GetOneSidedMetadataSignatureRequest::MAX_ADDRESS_SIZE, 162);
        assert_eq!(
            request(&[0xaa; 162]).encoded_len(),
            GetOneSidedMetadataSignatureRequest::MAX_SIZE
        );
    }

    /// A 149 byte address - 67 bytes of dual address and an 82 byte payment ID - fit one APDU before 6.1.1-pre.0
    /// added `sender_offset_branch`, and fits again now that the byte-sized fields are bytes on the wire.
    #[test]
    fn a_149_byte_address_fits_one_apdu() {
        assert!(request(&[0xaa; 149]).encoded_len() <= GetOneSidedMetadataSignatureRequest::MAX_SIZE);
    }

    #[test]
    fn the_host_cannot_build_an_address_its_length_prefix_cannot_hold() {
        let address = vec![0; usize::from(u16::MAX) + 1];
        assert_eq!(
            GetOneSidedMetadataSignatureRequest::new(0, 0, 0, 0, 0, 0, &MASK, &address, &MESSAGE),
            Err(ReceiverAddressTooLong)
        );
        let address = vec![0; usize::from(u16::MAX)];
        assert!(GetOneSidedMetadataSignatureRequest::new(0, 0, 0, 0, 0, 0, &MASK, &address, &MESSAGE).is_ok());
    }

    /// The staging is the point of this module: each stage fails only on its own length check, so a payload that
    /// is short in a later field still reaches the device's checks on an earlier one.
    #[test]
    fn a_payload_short_in_its_message_still_yields_its_head_and_address() {
        let address = vec![0xaa; TARI_DUAL_ADDRESS_MIN_SIZE];
        let bytes = request(&address).to_vec();
        // 158 bytes: two short of the message, but not short of the minimum the device checks first.
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
            bytes[59..61].copy_from_slice(&size.to_le_bytes());
            let head = OneSidedMetadataSignatureHead::decode(&bytes).unwrap();
            assert_eq!(head.receiver_address(), Err(DecodeError::WrongLength), "size {size}");
        }
    }

    #[test]
    fn an_address_that_runs_past_the_end_is_refused() {
        let mut bytes: Vec<u8> = request(&[0xaa; TARI_DUAL_ADDRESS_MIN_SIZE]).to_vec();
        let size = u16::try_from(TARI_DUAL_ADDRESS_MAX_SIZE).unwrap();
        bytes[59..61].copy_from_slice(&size.to_le_bytes());
        let head = OneSidedMetadataSignatureHead::decode(&bytes).unwrap();
        assert_eq!(head.receiver_address(), Err(DecodeError::WrongLength));
    }

    /// Trailing bytes after the message have always been ignored.
    #[test]
    fn bytes_after_the_message_are_ignored() {
        let address = vec![0xaa; TARI_DUAL_ADDRESS_MIN_SIZE];
        let mut bytes = request(&address).to_vec();
        bytes.extend_from_slice(&[0xff; 5]);
        assert_eq!(
            GetOneSidedMetadataSignatureRequest::decode(&bytes),
            Ok(request(&address))
        );
    }
}
