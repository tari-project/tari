// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! `GetOneSidedMetadataSignature`: a chunked instruction carrying the output's raw common fields.
//!
//! # Layout
//!
//! Each chunk is its own APDU, with the chunk number in `p1` and "more chunks follow" in `p2`
//! ([`super::CHUNK_MORE`] / [`super::CHUNK_LAST`]), as for `GetScriptOffset`:
//!
//! - chunk 0, always with `p2 = CHUNK_MORE`: the head, [`MetadataSignatureHeadChunk`] - `account(8) | network(8) |
//!   sender_offset_key_index(8) | sender_offset_branch(8) | value(8) | commitment_mask(32) | preimage_size(2) |
//!   address_size(2) | receiver_address(address_size)`;
//! - chunks 1, 2, ...: the metadata signature preimage, `preimage_size` bytes in all, cut into consecutive pieces of at
//!   most [`PREIMAGE_CHUNK_SIZE`] bytes, the last with `p2 = CHUNK_LAST`.
//!
//! `network` and `sender_offset_branch` are bytes widened to little endian `u64`s, and `preimage_size` and
//! `address_size` are little endian `u16`s.
//!
//! The preimage is what the device hashes into the metadata signature's `common` message: the borsh encodings of the
//! output's version, features, covenant, encrypted data and minimum value promise, back to back. What it may contain,
//! and how the device reads it, is [`crate::metadata_output`]. It is at most
//! [`crate::metadata_output::MAX_METADATA_PREIMAGE_SIZE`] bytes, which is why it cannot travel in one APDU.
//!
//! The device answers every chunk but the last with an empty `Ok`, and the last with the signature.
//!
//! # A new instruction byte
//!
//! This instruction used to carry the `common` message as an opaque 32 byte hash, in a single APDU, under `0x11`. The
//! new layout is not one the old one can be mistaken for, so it travels under a new instruction byte, `0x15`, and
//! `0x11` is retired: a host from before it is refused with `InsNotSupported` rather than read in the new layout.

use super::{ACCOUNT_SIZE, CHUNK_LAST, CHUNK_MORE, Decode, DecodeError, Encode, Reader, Request, Writer, write_u64};
use crate::{TARI_DUAL_ADDRESS_MAX_SIZE, TARI_DUAL_ADDRESS_MIN_SIZE, common_types::Instruction};

/// The size of the head's fields before the receiver address.
const HEAD_FIXED_SIZE: usize = ACCOUNT_SIZE + 8 * 4 + 32 + 2 + 2;

/// The most data one APDU can carry: `ledger-apdu` serialises the data length as a single byte (`len() as u8`), so a
/// longer payload does not fail - its length silently wraps, and the device answers `WrongApduLength`.
pub const MAX_APDU_DATA_SIZE: usize = 255;

/// The size of every preimage chunk but the last, which carries what is left.
pub const PREIMAGE_CHUNK_SIZE: usize = 250;

/// The receiver address was too long for its `u16` length prefix.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct ReceiverAddressTooLong;

/// Chunk 0 of `GetOneSidedMetadataSignature`. See the module docs for the layout.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct MetadataSignatureHeadChunk<'a> {
    pub account: u64,
    pub network: u64,
    pub sender_offset_key_index: u64,
    pub sender_offset_branch: u64,
    pub value: u64,
    pub commitment_mask: &'a [u8; 32],
    /// The size of the preimage the chunks after this one carry.
    pub preimage_size: u16,
    /// Private so that it can only be set through [`Self::new`], which is what guarantees the `u16` length prefix
    /// can hold it. The encoder cannot fail, so it must never be handed a length it would have to truncate.
    receiver_address: &'a [u8],
}

impl<'a> MetadataSignatureHeadChunk<'a> {
    /// Build a head, refusing a receiver address its `u16` length prefix cannot describe.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        account: u64,
        network: u64,
        sender_offset_key_index: u64,
        sender_offset_branch: u64,
        value: u64,
        commitment_mask: &'a [u8; 32],
        preimage_size: u16,
        receiver_address: &'a [u8],
    ) -> Result<Self, ReceiverAddressTooLong> {
        u16::try_from(receiver_address.len()).map_err(|_| ReceiverAddressTooLong)?;
        Ok(Self {
            account,
            network,
            sender_offset_key_index,
            sender_offset_branch,
            value,
            commitment_mask,
            preimage_size,
            receiver_address,
        })
    }

    /// The serialised receiver address.
    pub fn receiver_address(&self) -> &'a [u8] {
        self.receiver_address
    }

    /// The encoded length.
    pub fn encoded_len(&self) -> usize {
        HEAD_FIXED_SIZE.saturating_add(self.receiver_address.len())
    }

    /// Whether the encoded head fits one APDU ([`MAX_APDU_DATA_SIZE`]). A head that does not would go out with a
    /// wrapped length byte, so the host refuses it instead of sending it.
    pub fn fits_in_one_apdu(&self) -> bool {
        self.encoded_len() <= MAX_APDU_DATA_SIZE
    }
}

impl Encode for MetadataSignatureHeadChunk<'_> {
    fn encode(&self, out: &mut impl Writer) {
        write_u64(out, self.account);
        write_u64(out, self.network);
        write_u64(out, self.sender_offset_key_index);
        write_u64(out, self.sender_offset_branch);
        write_u64(out, self.value);
        out.write(self.commitment_mask);
        out.write(&self.preimage_size.to_le_bytes());
        // `new` has already refused anything longer, so this never saturates.
        let address_size = u16::try_from(self.receiver_address.len()).unwrap_or(u16::MAX);
        out.write(&address_size.to_le_bytes());
        out.write(self.receiver_address);
    }
}

impl<'a> Decode<'a> for MetadataSignatureHeadChunk<'a> {
    /// Exact: the fixed fields, then an address of a dual address's size that runs to the end of the chunk.
    fn decode(data: &'a [u8]) -> Result<Self, DecodeError> {
        let mut reader = Reader::at_least(data, HEAD_FIXED_SIZE)?;
        let account = reader.u64()?;
        let network = reader.u64()?;
        let sender_offset_key_index = reader.u64()?;
        let sender_offset_branch = reader.u64()?;
        let value = reader.u64()?;
        let commitment_mask = reader.array()?;
        let preimage_size = u16::from_le_bytes(*reader.array::<2>()?);
        let address_size = usize::from(u16::from_le_bytes(*reader.array::<2>()?));
        if !(TARI_DUAL_ADDRESS_MIN_SIZE..=TARI_DUAL_ADDRESS_MAX_SIZE).contains(&address_size) ||
            reader.rest.len() != address_size
        {
            return Err(DecodeError::WrongLength);
        }
        Ok(Self {
            account,
            network,
            sender_offset_key_index,
            sender_offset_branch,
            value,
            commitment_mask,
            preimage_size,
            receiver_address: reader.rest,
        })
    }
}

/// What one chunk carries.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum MetadataSignatureChunkBody<'a> {
    Head(MetadataSignatureHeadChunk<'a>),
    Preimage(&'a [u8]),
}

/// One `GetOneSidedMetadataSignature` APDU: a body, and the chunk number and continuation flag it travels under.
///
/// Built by [`MetadataSignatureRequest::chunks`], which only ever produces a well formed sequence. A malformed one is
/// sent through `minotari_ledger_wallet_comms::raw` instead.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct MetadataSignatureChunk<'a> {
    pub chunk_number: u8,
    pub more: bool,
    pub body: MetadataSignatureChunkBody<'a>,
}

impl Encode for MetadataSignatureChunk<'_> {
    fn encode(&self, out: &mut impl Writer) {
        match &self.body {
            MetadataSignatureChunkBody::Head(head) => head.encode(out),
            MetadataSignatureChunkBody::Preimage(piece) => out.write(piece),
        }
    }
}

impl Request for MetadataSignatureChunk<'_> {
    const INSTRUCTION: Instruction = Instruction::GetOneSidedMetadataSignature;

    fn p1(&self) -> u8 {
        self.chunk_number
    }

    fn p2(&self) -> u8 {
        if self.more { CHUNK_MORE } else { CHUNK_LAST }
    }
}

/// A whole `GetOneSidedMetadataSignature` exchange, as the host sends it. `head.preimage_size` must be
/// `preimage.len()`; [`Self::new`] sets it.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct MetadataSignatureRequest<'a> {
    pub head: MetadataSignatureHeadChunk<'a>,
    pub preimage: &'a [u8],
}

/// The preimage was empty, or too long for its `u16` size field.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct PreimageSizeOutOfRange;

impl<'a> MetadataSignatureRequest<'a> {
    /// A request for `preimage` under `head`, with the head's `preimage_size` set to match.
    pub fn new(mut head: MetadataSignatureHeadChunk<'a>, preimage: &'a [u8]) -> Result<Self, PreimageSizeOutOfRange> {
        if preimage.is_empty() {
            return Err(PreimageSizeOutOfRange);
        }
        head.preimage_size = u16::try_from(preimage.len()).map_err(|_| PreimageSizeOutOfRange)?;
        Ok(Self { head, preimage })
    }

    /// The chunks of this exchange, in order: the head, then the preimage in pieces of [`PREIMAGE_CHUNK_SIZE`], with
    /// `more` set on all but the last.
    pub fn chunks(&self) -> impl Iterator<Item = MetadataSignatureChunk<'a>> + use<'a> {
        let head = MetadataSignatureChunkBody::Head(self.head);
        let pieces = self
            .preimage
            .chunks(PREIMAGE_CHUNK_SIZE)
            .map(MetadataSignatureChunkBody::Preimage);
        let total = self.preimage.len().div_ceil(PREIMAGE_CHUNK_SIZE).saturating_add(1);
        core::iter::once(head)
            .chain(pieces)
            .enumerate()
            .map(move |(i, body)| MetadataSignatureChunk {
                chunk_number: u8::try_from(i).unwrap_or(u8::MAX),
                more: i.saturating_add(1) != total,
                body,
            })
    }
}

#[cfg(test)]
mod test {
    // A panic is the desired failure mode in a test.
    #![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]

    use alloc::{vec, vec::Vec};

    use super::*;

    const MASK: [u8; 32] = [0x41; 32];

    fn head(address: &[u8]) -> MetadataSignatureHeadChunk<'_> {
        MetadataSignatureHeadChunk::new(1, 0x26, 7, 0x06, 1_000_000, &MASK, 0, address).unwrap()
    }

    #[test]
    fn the_head_round_trips_with_the_address_last() {
        let address = vec![0xaa; TARI_DUAL_ADDRESS_MIN_SIZE + 3];
        let mut head = head(&address);
        head.preimage_size = 300;
        let bytes = head.to_vec();
        assert_eq!(bytes.len(), HEAD_FIXED_SIZE + address.len());
        assert_eq!(bytes.len(), head.encoded_len());
        assert_eq!(&bytes[..8], &1u64.to_le_bytes());
        assert_eq!(&bytes[8..16], &0x26u64.to_le_bytes());
        assert_eq!(&bytes[16..24], &7u64.to_le_bytes());
        assert_eq!(&bytes[24..32], &0x06u64.to_le_bytes());
        assert_eq!(&bytes[32..40], &1_000_000u64.to_le_bytes());
        assert_eq!(&bytes[40..72], &MASK);
        assert_eq!(&bytes[72..74], &300u16.to_le_bytes());
        assert_eq!(&bytes[74..76], &70u16.to_le_bytes());
        assert_eq!(&bytes[76..], address.as_slice());
        assert_eq!(MetadataSignatureHeadChunk::decode(&bytes), Ok(head));
    }

    #[test]
    fn a_head_with_a_wrong_address_size_is_refused() {
        let address = vec![0xaa; TARI_DUAL_ADDRESS_MIN_SIZE];
        let bytes = head(&address).to_vec();
        // Short of, or past, the declared address.
        assert!(MetadataSignatureHeadChunk::decode(&bytes[..bytes.len() - 1]).is_err());
        let mut long = bytes.clone();
        long.push(0);
        assert!(MetadataSignatureHeadChunk::decode(&long).is_err());
        // A size no dual address can have.
        for size in [0, TARI_DUAL_ADDRESS_MIN_SIZE - 1, TARI_DUAL_ADDRESS_MAX_SIZE + 1] {
            let mut bad = bytes.clone();
            bad[74..76].copy_from_slice(&u16::try_from(size).unwrap().to_le_bytes());
            assert!(MetadataSignatureHeadChunk::decode(&bad).is_err(), "size {size}");
        }
        assert!(MetadataSignatureHeadChunk::decode(&bytes[..HEAD_FIXED_SIZE - 1]).is_err());
    }

    #[test]
    fn the_host_cannot_build_an_address_its_length_prefix_cannot_hold() {
        let address = vec![0; usize::from(u16::MAX) + 1];
        assert_eq!(
            MetadataSignatureHeadChunk::new(0, 0, 0, 0, 0, &MASK, 0, &address),
            Err(ReceiverAddressTooLong)
        );
    }

    /// The head, then the preimage in 250 byte pieces, numbered from 0, `more` on all but the last, and the head's
    /// size set to the preimage's.
    #[test]
    fn a_request_is_the_head_then_the_preimage_in_numbered_pieces() {
        let address = vec![0xaa; TARI_DUAL_ADDRESS_MIN_SIZE];
        for size in [1, 249, 250, 251, 500, 1211] {
            let preimage: Vec<u8> = (0..size).map(|i| u8::try_from(i % 251).unwrap()).collect();
            let request = MetadataSignatureRequest::new(head(&address), &preimage).unwrap();
            assert_eq!(usize::from(request.head.preimage_size), size);
            let chunks: Vec<_> = request.chunks().collect();
            assert_eq!(chunks.len(), 1 + size.div_ceil(PREIMAGE_CHUNK_SIZE));
            let mut reassembled = Vec::new();
            for (i, chunk) in chunks.iter().enumerate() {
                assert_eq!(usize::from(chunk.chunk_number), i);
                assert_eq!(chunk.more, i + 1 != chunks.len());
                assert_eq!(chunk.p2(), if chunk.more { CHUNK_MORE } else { CHUNK_LAST });
                let bytes = chunk.to_vec();
                assert!(bytes.len() <= MAX_APDU_DATA_SIZE);
                match chunk.body {
                    MetadataSignatureChunkBody::Head(head) => {
                        assert_eq!(i, 0);
                        assert_eq!(MetadataSignatureHeadChunk::decode(&bytes), Ok(head));
                    },
                    MetadataSignatureChunkBody::Preimage(piece) => {
                        assert!(i > 0);
                        assert_eq!(bytes, piece);
                        reassembled.extend_from_slice(piece);
                    },
                }
            }
            assert_eq!(reassembled, preimage);
        }
        assert_eq!(
            MetadataSignatureRequest::new(head(&address), &[]),
            Err(PreimageSizeOutOfRange)
        );
    }

    /// The largest address that fits the head in one APDU.
    #[test]
    fn the_largest_address_that_fits_one_apdu() {
        let largest = MAX_APDU_DATA_SIZE - HEAD_FIXED_SIZE;
        assert_eq!(largest, 179);
        assert!(head(&vec![0xaa; largest]).fits_in_one_apdu());
        assert!(!head(&vec![0xaa; largest + 1]).fits_in_one_apdu());
    }
}
