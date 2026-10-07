// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! A cheap pre-decode check that bounds how much memory decoding an RPC payload can cost.
//!
//! prost allocates a full Rust struct for every embedded message it decodes. A `repeated` message field costs two
//! bytes per element on the wire (tag and a zero length) but hundreds of bytes once decoded, so a byte cap alone does
//! not bound memory: a 6 MiB request of empty elements decodes into several GB. Every element of a message field is a
//! length-delimited (LEN) item on the wire, so counting LEN items before decoding bounds the number of structs prost
//! will allocate.
//!
//! The scan does not know the message schema. It walks the wire format once and counts every LEN item it finds,
//! descending into the contents of each one in case it is an embedded message. Contents that do not parse as a
//! sequence of protobuf fields (e.g. a hash or a string) simply end that branch: they are counted as one item and never
//! cause a rejection on their own.

use super::RpcError;

/// The default maximum number of LEN items a single RPC payload may carry. Methods that legitimately carry more (e.g.
/// whole block bodies) raise this with `#[rpc(max_items = N)]`.
pub const DEFAULT_MAX_DECODE_ITEMS: usize = 65_536;

/// A payload may carry at most one LEN item per this many bytes. Real messages are made of hashes, keys and signatures,
/// so they come nowhere near this density; a flood of empty or near-empty elements does.
const MIN_BYTES_PER_ITEM: usize = 8;

/// The ratio check never rejects a payload carrying this many items or fewer. Without a floor, a tiny legitimate
/// message (e.g. one short string, or one empty sub-message) would fail the ratio, while this many items can never
/// cost a meaningful amount of memory.
const RATIO_FLOOR_ITEMS: usize = 64;

/// The maximum nesting depth the scan descends to. No RPC message nests anywhere near this deep, so going past it is
/// treated as hostile and rejected.
const MAX_DEPTH: usize = 32;

/// The largest valid protobuf field number (2^29 - 1).
const MAX_FIELD_NUMBER: u64 = (1 << 29) - 1;

/// Checks that decoding `bytes` cannot allocate more than `max_items` embedded messages (and no more than one per
/// [MIN_BYTES_PER_ITEM] bytes of payload). Returns [RpcError::DecodeBudgetExceeded] if it could, or if the payload
/// nests deeper than the scan is willing to follow.
pub fn check_decode_budget(bytes: &[u8], max_items: usize) -> Result<(), RpcError> {
    let ratio_limit = (bytes.len() / MIN_BYTES_PER_ITEM).max(RATIO_FLOOR_ITEMS);
    let mut budget = Budget {
        items: 0,
        limit: max_items.min(ratio_limit),
    };
    match scan(bytes, 0, &mut budget) {
        Ok(()) => Ok(()),
        Err(Exceeded) => Err(RpcError::DecodeBudgetExceeded {
            items: budget.items,
            max: budget.limit,
        }),
    }
}

struct Budget {
    items: usize,
    limit: usize,
}

/// The payload carries too many items or nests too deep.
struct Exceeded;

/// Walks one level of protobuf fields in `buf`, recursing into the contents of every LEN field. Returns `Ok` when the
/// level ends, whether because `buf` was fully consumed or because the rest of it does not parse as protobuf.
fn scan(buf: &[u8], depth: usize, budget: &mut Budget) -> Result<(), Exceeded> {
    let mut pos = 0usize;
    while pos < buf.len() {
        let Some(tag) = read_varint(buf, &mut pos) else {
            return Ok(());
        };
        let field_number = tag >> 3;
        if field_number == 0 || field_number > MAX_FIELD_NUMBER {
            return Ok(());
        }
        let skip = match tag & 0x7 {
            // VARINT
            0 => {
                if read_varint(buf, &mut pos).is_none() {
                    return Ok(());
                }
                0
            },
            // I64
            1 => 8,
            // LEN
            2 => {
                let Some(len) = read_varint(buf, &mut pos).and_then(|len| usize::try_from(len).ok()) else {
                    return Ok(());
                };
                let Some(end) = pos.checked_add(len).filter(|end| *end <= buf.len()) else {
                    return Ok(());
                };
                budget.items = budget.items.saturating_add(1);
                if budget.items > budget.limit {
                    return Err(Exceeded);
                }
                let contents = buf.get(pos..end).unwrap_or_default();
                if !contents.is_empty() {
                    if depth >= MAX_DEPTH {
                        return Err(Exceeded);
                    }
                    scan(contents, depth.saturating_add(1), budget)?;
                }
                len
            },
            // I32
            5 => 4,
            // Groups (3, 4) are not used by prost-generated messages and 6, 7 are not valid wire types
            _ => return Ok(()),
        };
        match pos.checked_add(skip).filter(|end| *end <= buf.len()) {
            Some(end) => pos = end,
            None => return Ok(()),
        }
    }
    Ok(())
}

/// Reads a protobuf base-128 varint at `pos`, advancing `pos` past it. Returns `None` if the buffer ends before the
/// varint does or the varint is longer than 10 bytes.
fn read_varint(buf: &[u8], pos: &mut usize) -> Option<u64> {
    let mut value = 0u64;
    for i in 0..10u32 {
        let byte = *buf.get(*pos)?;
        *pos = pos.checked_add(1)?;
        value |= u64::from(byte & 0x7f).checked_shl(i.checked_mul(7)?)?;
        if byte & 0x80 == 0 {
            return Some(value);
        }
    }
    None
}

#[cfg(test)]
mod test {
    use super::*;

    /// Encodes a LEN field with field number 1 holding `contents`.
    fn len_field(contents: &[u8]) -> Vec<u8> {
        let mut buf = vec![0x0a];
        prost::encoding::encode_varint(contents.len() as u64, &mut buf);
        buf.extend_from_slice(contents);
        buf
    }

    fn assert_exceeded(bytes: &[u8], max_items: usize) -> (usize, usize) {
        match check_decode_budget(bytes, max_items) {
            Err(RpcError::DecodeBudgetExceeded { items, max }) => (items, max),
            res => panic!("expected DecodeBudgetExceeded, got {res:?}"),
        }
    }

    #[test]
    fn it_rejects_a_flat_flood_of_empty_items() {
        // A 6 MiB request of empty `repeated` elements: two bytes on the wire each
        let flood = [0x0a, 0x00].repeat(3 * 1024 * 1024);
        let (items, max) = assert_exceeded(&flood, DEFAULT_MAX_DECODE_ITEMS);
        assert_eq!(max, DEFAULT_MAX_DECODE_ITEMS);
        assert_eq!(items, DEFAULT_MAX_DECODE_ITEMS + 1);

        // The ratio alone rejects it even with an unlimited item cap
        let (_, max) = assert_exceeded(&flood, usize::MAX);
        assert_eq!(max, flood.len() / MIN_BYTES_PER_ITEM);
    }

    #[test]
    fn it_rejects_a_nested_flood() {
        // The flood sits several messages deep, behind a few wrappers
        let mut payload = [0x0a, 0x00].repeat(100_000);
        for _ in 0..4 {
            payload = len_field(&payload);
        }
        assert_exceeded(&payload, DEFAULT_MAX_DECODE_ITEMS);

        // Many wrappers each holding a smaller flood add up
        let wrapper = len_field(&[0x0a, 0x00].repeat(1_000));
        let payload = wrapper.repeat(100);
        let (items, _) = assert_exceeded(&payload, DEFAULT_MAX_DECODE_ITEMS);
        assert!(items > payload.len() / MIN_BYTES_PER_ITEM);
    }

    #[test]
    fn it_accepts_dense_but_legitimate_payloads() {
        // Hash-sized fields are 34 bytes per item on the wire
        let payload = len_field(&[0xaa; 32]).repeat(DEFAULT_MAX_DECODE_ITEMS);
        check_decode_budget(&payload, DEFAULT_MAX_DECODE_ITEMS).unwrap();

        // Tiny messages are under the ratio floor
        check_decode_budget(&len_field(b"Jo"), DEFAULT_MAX_DECODE_ITEMS).unwrap();
        check_decode_budget(&len_field(&[]), DEFAULT_MAX_DECODE_ITEMS).unwrap();
        check_decode_budget(&[], DEFAULT_MAX_DECODE_ITEMS).unwrap();
    }

    #[test]
    fn it_rejects_one_item_over_the_ratio() {
        // Each item is exactly 8 bytes: tag, length and 6 bytes that do not parse as fields (field number 0)
        let item = len_field(&[0u8; 6]);
        assert_eq!(item.len(), MIN_BYTES_PER_ITEM);
        let at_ratio = item.repeat(1_000);
        check_decode_budget(&at_ratio, DEFAULT_MAX_DECODE_ITEMS).unwrap();

        // One more (empty) item, but not enough bytes to pay for it
        let mut over_ratio = at_ratio;
        over_ratio.extend_from_slice(&len_field(&[]));
        let (items, max) = assert_exceeded(&over_ratio, DEFAULT_MAX_DECODE_ITEMS);
        assert_eq!(max, 1_000);
        assert_eq!(items, 1_001);
    }

    #[test]
    fn it_rejects_one_item_over_the_cap() {
        let at_cap = len_field(&[0xaa; 32]).repeat(100);
        check_decode_budget(&at_cap, 100).unwrap();

        let over_cap = len_field(&[0xaa; 32]).repeat(101);
        let (items, max) = assert_exceeded(&over_cap, 100);
        assert_eq!(max, 100);
        assert_eq!(items, 101);
    }

    #[test]
    fn malformed_contents_never_reject_on_their_own() {
        let malformed: &[&[u8]] = &[
            // Field number 0
            &[0x00, 0x01, 0x02],
            // Wire types 3, 4, 6 and 7
            &[0x0b, 0x01],
            &[0x0c, 0x01],
            &[0x0e, 0x01],
            &[0x0f, 0x01],
            // Truncated varint tag and value
            &[0x80],
            &[0x08, 0x80],
            // Over-long varint
            &[0x08, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01],
            // LEN that runs past the end of the buffer
            &[0x0a, 0x7f, 0x01],
            &[0x0a, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01],
            // Truncated fixed-width values
            &[0x09, 0x01, 0x02],
            &[0x0d, 0x01],
            // A valid field followed by garbage
            &[0x08, 0x01, 0xff],
        ];
        for contents in malformed {
            // As a payload of its own, and as the contents of a LEN field
            check_decode_budget(contents, DEFAULT_MAX_DECODE_ITEMS).unwrap();
            check_decode_budget(&len_field(contents), DEFAULT_MAX_DECODE_ITEMS).unwrap();
        }

        // Arbitrary bytes, e.g. hashes, keys and strings
        let mut state = 0x2545_f491_4f6c_dd1du64;
        let mut bytes = vec![0u8; 64 * 1024];
        for b in &mut bytes {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            *b = state.to_le_bytes()[0];
        }
        check_decode_budget(&bytes, DEFAULT_MAX_DECODE_ITEMS).unwrap();
        for chunk in bytes.chunks(32) {
            check_decode_budget(&len_field(chunk), DEFAULT_MAX_DECODE_ITEMS).unwrap();
        }
    }

    fn nested(levels: usize) -> Vec<u8> {
        // A varint field at the bottom, so that the innermost LEN item has contents to descend into
        let mut payload = vec![0x08, 0x01];
        for _ in 0..levels {
            payload = len_field(&payload);
        }
        payload
    }

    #[test]
    fn it_rejects_nesting_deeper_than_the_depth_cap() {
        check_decode_budget(&nested(MAX_DEPTH), DEFAULT_MAX_DECODE_ITEMS).unwrap();
        assert_exceeded(&nested(MAX_DEPTH + 1), DEFAULT_MAX_DECODE_ITEMS);
        assert_exceeded(&nested(1_000), DEFAULT_MAX_DECODE_ITEMS);
    }
}
