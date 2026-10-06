// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! `GetScriptOffset`: the one chunked instruction.
//!
//! # Layout
//!
//! Each chunk is its own APDU, with the chunk number in `p1` and "more chunks follow" in `p2`:
//!
//! - chunk 0: `account(8) | sender_offset_count(8) | script_index_count(8) | derived_script_key_count(8)` - the header,
//!   [`ScriptOffsetHeaderChunk`];
//! - chunk 1: `partial_script_key_sum(32)` - [`PartialScriptKeySumChunk`];
//! - the next `script_index_count` chunks: `branch(8) | index(8)` - [`ScriptKeyIndexChunk`];
//! - the next `derived_script_key_count` chunks: `blinding_factor(32)` - [`DerivedScriptKeyChunk`].
//!
//! Only chunk 0 carries the account. The reply to the last chunk is [`super::ScriptOffsetReply`]; every earlier chunk
//! is answered with an empty `Ok`.
//!
//! # What stays out of here
//!
//! The rules that make a script offset safe to hand back - that the header declares at least one sender offset key
//! and at least one device derived script key, which chunk numbers fall in which section, and that the reply is
//! only emitted once something the device derived has actually been folded in - live in [`crate::script_offset`],
//! and the accumulation itself lives in the device's `ScriptOffsetCtx`, whose reset semantics are what keep a
//! rejected exchange from being resumed. This module only frames the chunks. In particular, **nothing here decides
//! which chunk type a chunk number is**: the host can send any chunk number it likes, and the device classifies it
//! with [`crate::script_offset::script_key_section`] before it picks a decoder.
//!
//! Each chunk type is decoded on its own, with an exact length check, because that is what the device has always
//! done: a chunk that is the wrong length for its section is `WrongApduLength`, decided before anything in it is
//! looked at.

use super::{ACCOUNT_SIZE, Decode, DecodeError, Encode, Reader, Request, Writer, write_u64};
use crate::{common_types::Instruction, script_offset::SCRIPT_OFFSET_HEADER_SIZE};

/// The value of `p2` on every chunk but the last.
pub const CHUNK_MORE: u8 = 0x01;
/// The value of `p2` on the last chunk.
pub const CHUNK_LAST: u8 = 0x00;

/// Chunk 0: `account(8) | sender_offset_count(8) | script_index_count(8) | derived_script_key_count(8)`, 32 bytes.
///
/// The counts as the host declared them, unchecked. [`crate::script_offset::parse_script_offset_header`] is what
/// turns this into a `ScriptOffsetHeader` the device may act on, and it refuses anything that would leave the reply
/// unblinded.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct ScriptOffsetHeaderChunk {
    pub account: u64,
    pub sender_offset_count: u64,
    pub script_index_count: u64,
    pub derived_script_key_count: u64,
}

impl ScriptOffsetHeaderChunk {
    /// [`SCRIPT_OFFSET_HEADER_SIZE`], which predates this codec.
    pub const SIZE: usize = SCRIPT_OFFSET_HEADER_SIZE;
}

const _: () = assert!(ACCOUNT_SIZE + 8 * 3 == ScriptOffsetHeaderChunk::SIZE);

impl Encode for ScriptOffsetHeaderChunk {
    fn encode(&self, out: &mut impl Writer) {
        write_u64(out, self.account);
        write_u64(out, self.sender_offset_count);
        write_u64(out, self.script_index_count);
        write_u64(out, self.derived_script_key_count);
    }
}

impl Decode<'_> for ScriptOffsetHeaderChunk {
    fn decode(data: &[u8]) -> Result<Self, DecodeError> {
        let mut reader = Reader::exact(data, Self::SIZE)?;
        Ok(Self {
            account: reader.u64()?,
            sender_offset_count: reader.u64()?,
            script_index_count: reader.u64()?,
            derived_script_key_count: reader.u64()?,
        })
    }
}

/// Chunk 1: `partial_script_key_sum(32)`, the sum of the script private keys the host already knows.
///
/// One opaque scalar the host computed itself, which is why it never counts towards blinding the reply - see
/// [`crate::script_offset::check_offset_is_blinded`].
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct PartialScriptKeySumChunk<'a> {
    pub partial_script_key_sum: &'a [u8; 32],
}

impl PartialScriptKeySumChunk<'_> {
    pub const SIZE: usize = 32;
}

impl Encode for PartialScriptKeySumChunk<'_> {
    fn encode(&self, out: &mut impl Writer) {
        out.write(self.partial_script_key_sum);
    }
}

impl<'a> Decode<'a> for PartialScriptKeySumChunk<'a> {
    fn decode(data: &'a [u8]) -> Result<Self, DecodeError> {
        let mut reader = Reader::exact(data, Self::SIZE)?;
        Ok(Self {
            partial_script_key_sum: reader.array()?,
        })
    }
}

/// An indexed script key chunk: `branch(8) | index(8)`, 16 bytes - a pre-mine script key the device derives by
/// index. Branch *then* index, as in `GetScriptSignatureManaged`.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct ScriptKeyIndexChunk {
    pub branch: u64,
    pub index: u64,
}

impl ScriptKeyIndexChunk {
    pub const SIZE: usize = 16;
}

impl Encode for ScriptKeyIndexChunk {
    fn encode(&self, out: &mut impl Writer) {
        write_u64(out, self.branch);
        write_u64(out, self.index);
    }
}

impl Decode<'_> for ScriptKeyIndexChunk {
    fn decode(data: &[u8]) -> Result<Self, DecodeError> {
        let mut reader = Reader::exact(data, Self::SIZE)?;
        Ok(Self {
            branch: reader.u64()?,
            index: reader.u64()?,
        })
    }
}

/// A derived script key chunk: `blinding_factor(32)`, which the device folds into `alpha` to get the script key.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct DerivedScriptKeyChunk<'a> {
    pub blinding_factor: &'a [u8; 32],
}

impl DerivedScriptKeyChunk<'_> {
    pub const SIZE: usize = 32;
}

impl Encode for DerivedScriptKeyChunk<'_> {
    fn encode(&self, out: &mut impl Writer) {
        out.write(self.blinding_factor);
    }
}

impl<'a> Decode<'a> for DerivedScriptKeyChunk<'a> {
    fn decode(data: &'a [u8]) -> Result<Self, DecodeError> {
        let mut reader = Reader::exact(data, Self::SIZE)?;
        Ok(Self {
            blinding_factor: reader.array()?,
        })
    }
}

/// What one chunk carries.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ScriptOffsetChunkBody<'a> {
    Header(ScriptOffsetHeaderChunk),
    PartialScriptKeySum(PartialScriptKeySumChunk<'a>),
    ScriptKeyIndex(ScriptKeyIndexChunk),
    DerivedScriptKey(DerivedScriptKeyChunk<'a>),
}

/// One `GetScriptOffset` APDU: a body, and the chunk number and continuation flag it travels under.
///
/// Built by [`ScriptOffsetRequest::chunks`], which only ever produces a well formed sequence. A malformed one - out
/// of order, resumed after a rejection, a body in the wrong section - is exactly what the device has to refuse, and
/// is sent through `minotari_ledger_wallet_comms::raw` instead.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct ScriptOffsetChunk<'a> {
    pub chunk_number: u8,
    pub more: bool,
    pub body: ScriptOffsetChunkBody<'a>,
}

impl Encode for ScriptOffsetChunk<'_> {
    fn encode(&self, out: &mut impl Writer) {
        match &self.body {
            ScriptOffsetChunkBody::Header(chunk) => chunk.encode(out),
            ScriptOffsetChunkBody::PartialScriptKeySum(chunk) => chunk.encode(out),
            ScriptOffsetChunkBody::ScriptKeyIndex(chunk) => chunk.encode(out),
            ScriptOffsetChunkBody::DerivedScriptKey(chunk) => chunk.encode(out),
        }
    }
}

impl Request for ScriptOffsetChunk<'_> {
    const INSTRUCTION: Instruction = Instruction::GetScriptOffset;

    fn p1(&self) -> u8 {
        self.chunk_number
    }

    fn p2(&self) -> u8 {
        if self.more { CHUNK_MORE } else { CHUNK_LAST }
    }
}

/// A whole `GetScriptOffset` exchange, as the host sends it.
///
/// Holds borrowed wire fields only - `derived_script_keys` is a slice of *references* - so that assembling the
/// chunks never copies a blinding factor or the partial sum into a buffer of its own.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct ScriptOffsetRequest<'a> {
    pub account: u64,
    pub sender_offset_count: u64,
    pub partial_script_key_sum: &'a [u8; 32],
    /// `(branch, index)` pairs, each branch a byte widened to the wire's `u64`.
    pub script_key_indexes: &'a [(u64, u64)],
    pub derived_script_keys: &'a [&'a [u8; 32]],
}

impl<'a> ScriptOffsetRequest<'a> {
    /// The chunks of this exchange, in order: the header, the partial sum, the indexed script keys, then the derived
    /// script keys, with `more` set on all but the last.
    ///
    /// A chunk number that does not fit in `p1` is sent as `0`. That is what the hand rolled assembly this replaced
    /// did (`u8::try_from(i).unwrap_or(0)`), and the wire format is frozen; it is unreachable in practice, because
    /// the device refuses any chunk number above its `MAX_PAYLOADS` of 250 long before `u8` runs out.
    pub fn chunks(&self) -> impl Iterator<Item = ScriptOffsetChunk<'a>> + use<'a> {
        let header = ScriptOffsetChunkBody::Header(ScriptOffsetHeaderChunk {
            account: self.account,
            sender_offset_count: self.sender_offset_count,
            script_index_count: u64::try_from(self.script_key_indexes.len()).unwrap_or(u64::MAX),
            derived_script_key_count: u64::try_from(self.derived_script_keys.len()).unwrap_or(u64::MAX),
        });
        let partial_sum = ScriptOffsetChunkBody::PartialScriptKeySum(PartialScriptKeySumChunk {
            partial_script_key_sum: self.partial_script_key_sum,
        });
        let indexes = self
            .script_key_indexes
            .iter()
            .map(|&(branch, index)| ScriptOffsetChunkBody::ScriptKeyIndex(ScriptKeyIndexChunk { branch, index }));
        let derived = self
            .derived_script_keys
            .iter()
            .map(|&blinding_factor| ScriptOffsetChunkBody::DerivedScriptKey(DerivedScriptKeyChunk { blinding_factor }));

        let total = self
            .script_key_indexes
            .len()
            .saturating_add(self.derived_script_keys.len())
            .saturating_add(2);
        [header, partial_sum]
            .into_iter()
            .chain(indexes)
            .chain(derived)
            .enumerate()
            .map(move |(i, body)| ScriptOffsetChunk {
                chunk_number: u8::try_from(i).unwrap_or(0),
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

    const PARTIAL: [u8; 32] = [0x71; 32];
    const BLINDING_A: [u8; 32] = [0x72; 32];
    const BLINDING_B: [u8; 32] = [0x73; 32];

    #[test]
    fn a_full_exchange_is_header_sum_indexes_then_derived_keys_in_numbered_order() {
        let indexes = [(9, 100), (9, 101)];
        let derived = [&BLINDING_A, &BLINDING_B];
        let request = ScriptOffsetRequest {
            account: 7,
            sender_offset_count: 3,
            partial_script_key_sum: &PARTIAL,
            script_key_indexes: &indexes,
            derived_script_keys: &derived,
        };
        let chunks: Vec<_> = request.chunks().collect();
        assert_eq!(chunks.len(), 6);
        for (i, chunk) in chunks.iter().enumerate() {
            assert_eq!(usize::from(chunk.chunk_number), i);
            assert_eq!(chunk.more, i != 5, "chunk {i}");
            assert_eq!(chunk.p1(), chunk.chunk_number);
            assert_eq!(chunk.p2(), if i == 5 { CHUNK_LAST } else { CHUNK_MORE });
        }
        assert_eq!(
            chunks[0].body,
            ScriptOffsetChunkBody::Header(ScriptOffsetHeaderChunk {
                account: 7,
                sender_offset_count: 3,
                script_index_count: 2,
                derived_script_key_count: 2,
            })
        );
        // The account rides on chunk 0 only.
        assert_eq!(&chunks[0].to_vec()[..8], &7u64.to_le_bytes());
        assert_eq!(chunks[1].to_vec(), PARTIAL.to_vec());
        let mut index_chunk = 9u64.to_le_bytes().to_vec();
        index_chunk.extend_from_slice(&101u64.to_le_bytes());
        assert_eq!(chunks[3].to_vec(), index_chunk);
        assert_eq!(chunks[5].to_vec(), BLINDING_B.to_vec());
    }

    #[test]
    fn the_smallest_exchange_ends_on_its_only_payload_chunk() {
        let derived = [&BLINDING_A];
        let request = ScriptOffsetRequest {
            account: 1,
            sender_offset_count: 1,
            partial_script_key_sum: &PARTIAL,
            script_key_indexes: &[],
            derived_script_keys: &derived,
        };
        let flags: Vec<_> = request.chunks().map(|chunk| (chunk.chunk_number, chunk.more)).collect();
        assert_eq!(flags, vec![(0, true), (1, true), (2, false)]);
    }

    /// Frozen from the hand rolled assembly: a chunk number past `u8::MAX` goes out as zero.
    #[test]
    fn a_chunk_number_that_does_not_fit_in_p1_is_sent_as_zero() {
        let indexes = vec![(9u64, 0u64); 300];
        let request = ScriptOffsetRequest {
            account: 1,
            sender_offset_count: 1,
            partial_script_key_sum: &PARTIAL,
            script_key_indexes: &indexes,
            derived_script_keys: &[],
        };
        let numbers: Vec<_> = request.chunks().map(|chunk| chunk.chunk_number).collect();
        assert_eq!(numbers[255], 255);
        assert_eq!(numbers[256], 0);
        assert_eq!(numbers.len(), 302);
    }

    /// Every chunk type decodes at its exact length only, as the device's per-section length checks always did.
    #[test]
    fn each_chunk_type_round_trips_at_its_exact_length_only() {
        let header = ScriptOffsetHeaderChunk {
            account: 1,
            sender_offset_count: 2,
            script_index_count: 3,
            derived_script_key_count: 4,
        };
        let bytes = header.to_vec();
        assert_eq!(bytes.len(), 32);
        assert_eq!(ScriptOffsetHeaderChunk::decode(&bytes), Ok(header));
        assert!(ScriptOffsetHeaderChunk::decode(&bytes[..31]).is_err());

        let sum = PartialScriptKeySumChunk {
            partial_script_key_sum: &PARTIAL,
        };
        assert_eq!(PartialScriptKeySumChunk::decode(&sum.to_vec()), Ok(sum));
        assert!(PartialScriptKeySumChunk::decode(&[0; 33]).is_err());

        let index = ScriptKeyIndexChunk { branch: 9, index: 5 };
        assert_eq!(ScriptKeyIndexChunk::decode(&index.to_vec()), Ok(index));
        assert!(ScriptKeyIndexChunk::decode(&[0; 17]).is_err());
        // A derived key chunk is not an index chunk, whichever section the device thinks it is in.
        assert!(ScriptKeyIndexChunk::decode(&BLINDING_A).is_err());

        let derived = DerivedScriptKeyChunk {
            blinding_factor: &BLINDING_A,
        };
        assert_eq!(DerivedScriptKeyChunk::decode(&derived.to_vec()), Ok(derived));
        assert!(DerivedScriptKeyChunk::decode(&index.to_vec()).is_err());
    }
}
