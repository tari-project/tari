// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! A cheap, type-aware pre-decode check that bounds how much memory decoding a protobuf payload can cost.
//!
//! prost allocates a full Rust struct for every embedded message it decodes. A `repeated` message field costs two
//! bytes per element on the wire (tag and a zero length) but hundreds of bytes once decoded, so a byte cap alone does
//! not bound memory: a 6 MiB request of empty elements decodes into several GB.
//!
//! [DecodeBudget] counts the allocations a payload would make prost perform, without allocating anything. It follows
//! the message schema: `#[derive(DecodeBudget)]` (from `tari_comms_rpc_macros`) generates, for each prost type, a
//! walker that charges one item for every length-delimited field whose tag is a message-typed field and descends into
//! it with that field's own walker. Each element of a `repeated bytes`/`repeated string` field is also charged one item
//! (it is a separate `Vec`/`String` once decoded) and a packed repeated scalar field is charged its byte length (an
//! upper bound on its element count). The contents of `bytes`, `string`, packed and unknown fields are never entered,
//! so attacker-chosen bytes (ciphertexts, script data) are never mistaken for messages. Every tari protobuf type gets
//! the derive through `tari_common::build::ProtobufCompiler`.
//!
//! Unknown groups are skipped the way prost skips them, so they cannot hide what follows. Malformed wire data (an
//! invalid wire type, a truncated varint, a length running past the end, an unterminated group) simply stops the count:
//! prost rejects the payload when it decodes it, so the walk only has to be right for well-formed input.

/// The default maximum number of message instances a single RPC payload may carry. Methods that legitimately carry
/// more (e.g. whole block bodies) raise this with `#[rpc(max_items = N)]`.
pub const DEFAULT_MAX_DECODE_ITEMS: usize = 16_384;

/// The deepest message nesting the walk follows. Walks follow the schema, so this is only reached by a recursive
/// message type; going past it is treated as over budget.
const MAX_DEPTH: usize = 128;

/// The payload would make prost allocate more message instances than allowed, or nests deeper than [MAX_DEPTH].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodeBudgetExceeded {
    /// The instances counted when the walk stopped
    pub items: usize,
    /// The budget
    pub max: usize,
}

/// The running count of message instances for one payload.
#[derive(Debug)]
pub struct Budget {
    items: usize,
    max: usize,
    depth: usize,
}

impl Budget {
    /// A budget that allows at most `max` message instances.
    pub fn new(max: usize) -> Self {
        Self {
            items: 0,
            max,
            depth: 0,
        }
    }

    /// The message instances counted so far
    pub fn items(&self) -> usize {
        self.items
    }

    fn exceeded(&self) -> DecodeBudgetExceeded {
        DecodeBudgetExceeded {
            items: self.items,
            max: self.max,
        }
    }

    /// Charges `count` items that are not walked, e.g. the elements of a `repeated bytes` field or (as an upper bound)
    /// of a packed repeated scalar field.
    pub fn charge_items(&mut self, count: usize) -> Result<(), DecodeBudgetExceeded> {
        self.items = self.items.saturating_add(count);
        if self.items > self.max {
            return Err(self.exceeded());
        }
        Ok(())
    }

    /// Charges one instance of message `T`, encoded in `contents`, and walks it.
    pub fn charge_message<T: DecodeBudget + ?Sized>(&mut self, contents: &[u8]) -> Result<(), DecodeBudgetExceeded> {
        self.items = self.items.saturating_add(1);
        if self.items > self.max || self.depth >= MAX_DEPTH {
            return Err(self.exceeded());
        }
        self.depth = self.depth.saturating_add(1);
        let result = T::count_messages(contents, self);
        self.depth = self.depth.saturating_sub(1);
        result
    }

    /// Charges one map entry, encoded in `contents`, whose value (tag 2) is a message `V`, and walks the value. Entries
    /// of maps with scalar, bytes or string values are charged with `charge_items(1)` instead.
    pub fn charge_map_entry<V: DecodeBudget + ?Sized>(&mut self, contents: &[u8]) -> Result<(), DecodeBudgetExceeded> {
        self.items = self.items.saturating_add(1);
        if self.items > self.max {
            return Err(self.exceeded());
        }
        walk_len_fields(contents, self, |tag, value, budget| {
            if tag == 2 {
                budget.charge_message::<V>(value)
            } else {
                Ok(())
            }
        })
    }
}

/// Counts the allocations decoding a protobuf payload would make prost perform.
///
/// Do not implement this by hand for message types; derive it with `#[derive(tari_comms_rpc_macros::DecodeBudget)]`
/// (every tari protobuf type already has it). A hand-written `count_messages` that does not walk every message-typed
/// field silently switches the guard off for that type. `count_messages` has no default for the same reason: an empty
/// `impl DecodeBudget for X {}` does not compile.
pub trait DecodeBudget {
    /// Walks the fields of one encoded instance of this message in `buf`, charging `budget` for every embedded message.
    fn count_messages(buf: &[u8], budget: &mut Budget) -> Result<(), DecodeBudgetExceeded>;

    /// For prost oneof enums: charges the length-delimited field `tag` (holding `contents`) if it is one of this
    /// oneof's message variants. Not used for any other type.
    fn count_oneof_field(_tag: u32, _contents: &[u8], _budget: &mut Budget) -> Result<(), DecodeBudgetExceeded> {
        Ok(())
    }
}

/// Implements [DecodeBudget] for payload types that contain no embedded messages
macro_rules! impl_no_embedded_messages {
    ($($ty:ty),* $(,)?) => {
        $(
            impl DecodeBudget for $ty {
                fn count_messages(_buf: &[u8], _budget: &mut Budget) -> Result<(), DecodeBudgetExceeded> {
                    Ok(())
                }
            }
        )*
    };
}

impl_no_embedded_messages!((), bool, u32, u64, i32, i64, f32, f64, String, Vec<u8>, bytes::Bytes);

/// Checks that decoding `buf` as a `T` allocates at most `max_items` embedded message instances. Returns the number
/// counted.
pub fn check_decode_budget<T: DecodeBudget + ?Sized>(
    buf: &[u8],
    max_items: usize,
) -> Result<usize, DecodeBudgetExceeded> {
    let mut budget = Budget::new(max_items);
    T::count_messages(buf, &mut budget)?;
    Ok(budget.items())
}

/// Decodes `buf` as a `T` after checking (with [check_decode_budget]) that it carries at most `max_items` embedded
/// message instances. An over-budget payload is reported as a `prost::DecodeError`, so callers handle it exactly like
/// any other undecodable payload.
pub fn decode_with_max_items<T>(buf: &[u8], max_items: usize) -> Result<T, prost::DecodeError>
where T: prost::Message + Default + DecodeBudget {
    check_decode_budget::<T>(buf, max_items).map_err(|err| {
        prost::DecodeError::new(format!(
            "message exceeds the decode budget ({} embedded items, at most {} allowed)",
            err.items, err.max
        ))
    })?;
    T::decode(buf)
}

/// Calls `f(tag, contents, budget)` for every length-delimited field in `buf`, skipping all other fields (including
/// groups, as prost does). Stops (with `Ok`) at the first malformed field.
pub fn walk_len_fields<F>(buf: &[u8], budget: &mut Budget, f: F) -> Result<(), DecodeBudgetExceeded>
where F: FnMut(u32, &[u8], &mut Budget) -> Result<(), DecodeBudgetExceeded> {
    walk_fields(buf, budget, &[], f)
}

/// Like [walk_len_fields], for a message with map fields under `map_tags`. prost decodes a map field without checking
/// its wire type: whatever the wire type, it reads a length and a map entry. So a field under a map tag is handed to
/// `f` as length-delimited whatever its wire type says, which keeps the walk in step with prost.
pub fn walk_fields<F>(buf: &[u8], budget: &mut Budget, map_tags: &[u32], mut f: F) -> Result<(), DecodeBudgetExceeded>
where F: FnMut(u32, &[u8], &mut Budget) -> Result<(), DecodeBudgetExceeded> {
    let mut pos = 0usize;
    while pos < buf.len() {
        let Some(key) = read_varint(buf, &mut pos) else {
            return Ok(());
        };
        let Ok(tag) = u32::try_from(key >> 3) else {
            return Ok(());
        };
        if tag == 0 {
            return Ok(());
        }
        // Wire types 6 and 7 fail prost's key decode, before it looks at the tag
        let wire_type = key & 0x7;
        if wire_type > 5 {
            return Ok(());
        }
        let wire_type = if map_tags.contains(&tag) { 2 } else { wire_type };
        let skip = match wire_type {
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
                let Some(contents) = pos.checked_add(len).and_then(|end| buf.get(pos..end)) else {
                    return Ok(());
                };
                f(tag, contents, budget)?;
                len
            },
            // StartGroup: prost skips a group under an unknown tag and carries on decoding what follows, so the walk
            // must too. Nothing inside a skipped group is allocated, so nothing in it is charged.
            3 => {
                if !skip_group(buf, &mut pos, tag, 0) {
                    return Ok(());
                }
                0
            },
            // I32
            5 => 4,
            // A stray EndGroup (4) is an error to prost, and 6, 7 are not valid wire types
            _ => return Ok(()),
        };
        match pos.checked_add(skip).filter(|end| *end <= buf.len()) {
            Some(end) => pos = end,
            None => return Ok(()),
        }
    }
    Ok(())
}

/// Skips the rest of a group opened with `group_tag`, following prost's `skip_field`: every field up to the EndGroup
/// with the same tag, including nested groups. Returns false if the group is malformed (unterminated, mismatched,
/// nested deeper than [MAX_DEPTH]), all of which prost rejects.
fn skip_group(buf: &[u8], pos: &mut usize, group_tag: u32, depth: usize) -> bool {
    if depth >= MAX_DEPTH {
        return false;
    }
    loop {
        let Some(key) = read_varint(buf, pos) else {
            return false;
        };
        let Ok(tag) = u32::try_from(key >> 3) else {
            return false;
        };
        if tag == 0 {
            return false;
        }
        let skip = match key & 0x7 {
            0 => {
                if read_varint(buf, pos).is_none() {
                    return false;
                }
                0
            },
            1 => 8,
            2 => match read_varint(buf, pos).and_then(|len| usize::try_from(len).ok()) {
                Some(len) => len,
                None => return false,
            },
            3 => {
                if !skip_group(buf, pos, tag, depth.saturating_add(1)) {
                    return false;
                }
                0
            },
            4 => return tag == group_tag,
            5 => 4,
            _ => return false,
        };
        match pos.checked_add(skip).filter(|end| *end <= buf.len()) {
            Some(end) => *pos = end,
            None => return false,
        }
    }
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
    use std::collections::HashMap;

    use prost::Message;
    use tari_comms_rpc_macros::DecodeBudget;

    use super::*;

    #[derive(Clone, PartialEq, Message, DecodeBudget)]
    struct Leaf {
        #[prost(uint32, tag = "1")]
        value: u32,
    }

    #[derive(Clone, PartialEq, Message, DecodeBudget)]
    struct Inner {
        #[prost(bytes = "vec", tag = "1")]
        data: Vec<u8>,
        #[prost(message, repeated, tag = "2")]
        leaves: Vec<Leaf>,
    }

    #[derive(Clone, PartialEq, prost::Oneof, DecodeBudget)]
    enum Choice {
        #[prost(message, tag = "3")]
        Message(Inner),
        #[prost(bytes, tag = "4")]
        Bytes(Vec<u8>),
        #[prost(message, tag = "7")]
        Boxed(Box<Leaf>),
    }

    #[derive(Clone, PartialEq, Message, DecodeBudget)]
    struct Outer {
        #[prost(message, repeated, tag = "1")]
        items: Vec<Inner>,
        #[prost(message, optional, boxed, tag = "2")]
        single: Option<Box<Inner>>,
        #[prost(oneof = "Choice", tags = "3, 4, 7")]
        choice: Option<Choice>,
        #[prost(map = "string, message", tag = "5")]
        by_name: HashMap<String, Leaf>,
        #[prost(bytes = "vec", tag = "6")]
        blob: Vec<u8>,
        #[prost(string, tag = "8")]
        text: String,
        #[prost(uint64, repeated, tag = "9")]
        numbers: Vec<u64>,
        #[prost(bytes = "vec", repeated, tag = "10")]
        hashes: Vec<Vec<u8>>,
        #[prost(string, repeated, tag = "11")]
        names: Vec<String>,
        #[prost(map = "int32, bytes", tag = "12")]
        blobs: HashMap<i32, Vec<u8>>,
    }

    /// A recursive message type
    #[derive(Clone, PartialEq, Message, DecodeBudget)]
    struct Node {
        #[prost(message, optional, boxed, tag = "1")]
        child: Option<Box<Node>>,
    }

    fn count<T: DecodeBudget>(msg: &impl Message, max: usize) -> Result<usize, DecodeBudgetExceeded> {
        check_decode_budget::<T>(&msg.encode_to_vec(), max)
    }

    /// Bytes that look like a deep stack of nested messages
    fn fake_messages(len: usize) -> Vec<u8> {
        [0x0a, 0x00].repeat(len / 2)
    }

    fn leaf() -> Leaf {
        Leaf { value: 1 }
    }

    fn inner(leaves: usize) -> Inner {
        Inner {
            data: fake_messages(64),
            leaves: vec![leaf(); leaves],
        }
    }

    #[test]
    fn it_counts_every_message_instance() {
        let msg = Outer {
            // 3 + 3 * 2 leaves
            items: vec![inner(2); 3],
            // 1 + 4 leaves
            single: Some(Box::new(inner(4))),
            // 1 + 5 leaves
            choice: Some(Choice::Message(inner(5))),
            // 2 entries + 2 values
            by_name: [("a".to_string(), leaf()), ("b".to_string(), leaf())]
                .into_iter()
                .collect(),
            blob: fake_messages(1_000),
            text: "\n\0\n\0".to_string(),
            // Packed: one byte each, charged by byte length
            numbers: vec![10; 100],
            // One each, contents not entered
            hashes: vec![fake_messages(64); 3],
            names: vec![String::from_utf8(fake_messages(64)).unwrap(); 2],
            // One per entry, values not entered
            blobs: [(1, fake_messages(64)), (2, fake_messages(64))].into_iter().collect(),
        };
        assert_eq!(
            count::<Outer>(&msg, DEFAULT_MAX_DECODE_ITEMS).unwrap(),
            9 + 5 + 6 + 4 + 100 + 3 + 2 + 2
        );

        let boxed_variant = Outer {
            choice: Some(Choice::Boxed(Box::new(leaf()))),
            ..Default::default()
        };
        assert_eq!(count::<Outer>(&boxed_variant, DEFAULT_MAX_DECODE_ITEMS).unwrap(), 1);
    }

    #[test]
    fn it_never_enters_bytes_or_strings() {
        let msg = Outer {
            blob: fake_messages(1_000_000),
            choice: Some(Choice::Bytes(fake_messages(1_000_000))),
            text: String::from_utf8(fake_messages(1_000_000)).unwrap(),
            single: Some(Box::new(Inner {
                data: fake_messages(1_000_000),
                leaves: vec![],
            })),
            ..Default::default()
        };
        assert_eq!(count::<Outer>(&msg, 1).unwrap(), 1);
    }

    /// The wire bytes of field `tag` holding `contents`. Flood tests build their payloads from these rather than from
    /// decoded structs, which would cost hundreds of MB just to encode.
    fn len_field(tag: u32, contents: &[u8]) -> Vec<u8> {
        let mut buf = Vec::with_capacity(contents.len().saturating_add(16));
        prost::encoding::encode_key(tag, prost::encoding::WireType::LengthDelimited, &mut buf);
        prost::encoding::encode_varint(contents.len() as u64, &mut buf);
        buf.extend_from_slice(contents);
        buf
    }

    /// `count` empty elements of the repeated field `tag`: two bytes each on the wire
    fn empty_elements(tag: u32, count: usize) -> Vec<u8> {
        len_field(tag, &[]).repeat(count)
    }

    /// The RPC request size cap. This module is compiled without the `rpc` feature too, so it keeps its own copy of
    /// `protocol::rpc::RPC_MAX_REQUEST_SIZE`, tied to the real one when that exists.
    const MAX_REQUEST_SIZE: usize = 6 * 1024 * 1024;
    #[cfg(feature = "rpc")]
    const _: () = assert!(MAX_REQUEST_SIZE == crate::protocol::rpc::RPC_MAX_REQUEST_SIZE);

    /// Elements that fill a request up to the size cap, leaving room for the enclosing headers
    const FLOOD_ELEMENTS: usize = MAX_REQUEST_SIZE / 2 - 8;

    #[test]
    fn it_rejects_a_flat_flood() {
        let flood = empty_elements(1, FLOOD_ELEMENTS);
        assert!(flood.len() <= MAX_REQUEST_SIZE);
        let err = check_decode_budget::<Outer>(&flood, DEFAULT_MAX_DECODE_ITEMS).unwrap_err();
        assert_eq!(err, DecodeBudgetExceeded {
            items: DEFAULT_MAX_DECODE_ITEMS + 1,
            max: DEFAULT_MAX_DECODE_ITEMS
        });
    }

    #[test]
    fn it_rejects_a_flood_of_bytes_or_string_elements() {
        // Two bytes each on the wire, a whole `Vec`/`String` each once decoded
        check_decode_budget::<Outer>(&empty_elements(10, FLOOD_ELEMENTS), DEFAULT_MAX_DECODE_ITEMS).unwrap_err();
        check_decode_budget::<Outer>(&empty_elements(11, FLOOD_ELEMENTS), DEFAULT_MAX_DECODE_ITEMS).unwrap_err();
    }

    /// prost reads a map field as a length and an entry whatever wire type its key says, so the walk must too
    #[test]
    fn a_map_field_is_charged_whatever_its_wire_type() {
        use prost::encoding::{WireType, encode_key, encode_varint};

        // A `by_name` (tag 5) entry: key "a", value a `Leaf`
        let mut entry = len_field(1, b"a");
        entry.extend_from_slice(&len_field(2, &leaf().encode_to_vec()));
        // Followed by 10 empty `items`, which the walk must not lose track of
        let tail = empty_elements(1, 10);

        for wire_type in [
            WireType::Varint,
            WireType::SixtyFourBit,
            WireType::LengthDelimited,
            WireType::StartGroup,
            WireType::EndGroup,
            WireType::ThirtyTwoBit,
        ] {
            let mut bytes = Vec::new();
            encode_key(5, wire_type, &mut bytes);
            encode_varint(entry.len() as u64, &mut bytes);
            bytes.extend_from_slice(&entry);
            bytes.extend_from_slice(&tail);

            let decoded = Outer::decode(bytes.as_slice()).unwrap();
            assert_eq!(decoded.by_name.len(), 1, "{wire_type:?}");
            assert_eq!(decoded.items.len(), 10, "{wire_type:?}");
            // Entry, its value and the 10 items
            assert_eq!(
                check_decode_budget::<Outer>(&bytes, DEFAULT_MAX_DECODE_ITEMS).unwrap(),
                12,
                "{wire_type:?}"
            );
        }

        // A flood of `by_name` entries sent as varints is still a flood
        let mut flood = Vec::new();
        for _ in 0..100_000 {
            encode_key(5, WireType::Varint, &mut flood);
            encode_varint(entry.len() as u64, &mut flood);
            flood.extend_from_slice(&entry);
        }
        check_decode_budget::<Outer>(&flood, DEFAULT_MAX_DECODE_ITEMS).unwrap_err();
    }

    #[test]
    fn it_rejects_a_flood_of_scalar_map_entries() {
        // `blobs` (tag 12) entries with distinct keys and empty values: a few bytes each on the wire, a map node each
        // once decoded
        let mut flood = Vec::new();
        for key in 0..100_000u64 {
            let mut entry = Vec::new();
            prost::encoding::encode_key(1, prost::encoding::WireType::Varint, &mut entry);
            prost::encoding::encode_varint(key, &mut entry);
            flood.extend_from_slice(&len_field(12, &entry));
        }
        check_decode_budget::<Outer>(&flood, DEFAULT_MAX_DECODE_ITEMS).unwrap_err();

        let msg = Outer {
            blobs: (0..100).map(|key| (key, vec![])).collect(),
            ..Default::default()
        };
        assert_eq!(count::<Outer>(&msg, DEFAULT_MAX_DECODE_ITEMS).unwrap(), 100);
    }

    #[test]
    fn it_rejects_a_flood_of_packed_scalars() {
        // One byte each on the wire, eight once decoded
        let flood = len_field(9, &vec![1u8; 2 * FLOOD_ELEMENTS]);
        check_decode_budget::<Outer>(&flood, DEFAULT_MAX_DECODE_ITEMS).unwrap_err();
        // The byte length bounds the element count from above
        let msg = Outer {
            numbers: vec![u64::MAX; 100],
            ..Default::default()
        };
        assert_eq!(count::<Outer>(&msg, DEFAULT_MAX_DECODE_ITEMS).unwrap(), 1_000);
    }

    /// An unknown group, which prost skips before carrying on
    fn group_prefix() -> Vec<u8> {
        let mut buf = Vec::new();
        prost::encoding::encode_key(1000, prost::encoding::WireType::StartGroup, &mut buf);
        prost::encoding::encode_key(1, prost::encoding::WireType::Varint, &mut buf);
        prost::encoding::encode_varint(7, &mut buf);
        prost::encoding::encode_key(1000, prost::encoding::WireType::EndGroup, &mut buf);
        buf
    }

    fn with_group_prefix(msg: &impl Message) -> Vec<u8> {
        let mut buf = group_prefix();
        buf.extend_from_slice(&msg.encode_to_vec());
        buf
    }

    #[test]
    fn a_group_does_not_hide_a_flood() {
        let mut flood = group_prefix();
        flood.extend_from_slice(&empty_elements(1, FLOOD_ELEMENTS));
        check_decode_budget::<Outer>(&flood, DEFAULT_MAX_DECODE_ITEMS).unwrap_err();

        // Inside a nested message: `single` (tag 2) holding a group then a flood of `leaves` (tag 2)
        let mut inner = group_prefix();
        inner.extend_from_slice(&empty_elements(2, FLOOD_ELEMENTS));
        check_decode_budget::<Outer>(&len_field(2, &inner), DEFAULT_MAX_DECODE_ITEMS).unwrap_err();

        // prost really does decode past the group, so the count must too
        let small = Outer {
            items: vec![Inner::default(); 10],
            ..Default::default()
        };
        let bytes = with_group_prefix(&small);
        assert_eq!(Outer::decode(bytes.as_slice()).unwrap(), small);
        assert_eq!(
            check_decode_budget::<Outer>(&bytes, DEFAULT_MAX_DECODE_ITEMS).unwrap(),
            10
        );
    }

    #[test]
    fn it_rejects_a_nested_flood() {
        let flood = Outer {
            single: Some(Box::new(inner(100_000))),
            ..Default::default()
        };
        count::<Outer>(&flood, DEFAULT_MAX_DECODE_ITEMS).unwrap_err();
        let flood = Outer {
            choice: Some(Choice::Message(inner(100_000))),
            ..Default::default()
        };
        count::<Outer>(&flood, DEFAULT_MAX_DECODE_ITEMS).unwrap_err();
    }

    #[test]
    fn it_rejects_one_over_the_cap() {
        let msg = Outer {
            items: vec![Inner::default(); 100],
            ..Default::default()
        };
        assert_eq!(count::<Outer>(&msg, 100).unwrap(), 100);
        let err = count::<Outer>(&msg, 99).unwrap_err();
        assert_eq!(err, DecodeBudgetExceeded { items: 100, max: 99 });
    }

    #[test]
    fn malformed_wire_stops_the_count_without_rejecting() {
        let malformed: &[&[u8]] = &[
            // Field number 0
            &[0x00, 0x01],
            // Wire types 6 and 7
            &[0x0e, 0x01],
            &[0x0f, 0x01],
            // A stray EndGroup (tag 15)
            &[0x7c],
            // An unterminated group, and one closed with the wrong tag (tag 15, closed as 14)
            &[0x7b, 0x08, 0x01],
            &[0x7b, 0x74],
            // Truncated varints
            &[0x80],
            &[0x08, 0x80],
            // LEN that runs past the end of the buffer
            &[0x0a, 0x7f, 0x01],
            // Truncated fixed-width values (tag 15)
            &[0x79, 0x01],
            &[0x7d, 0x01],
        ];
        for bytes in malformed {
            assert_eq!(check_decode_budget::<Outer>(bytes, 0).unwrap(), 0);
            // Wherever the walk stops early, prost rejects the payload, so nothing after that point is decoded
            assert!(Outer::decode(*bytes).is_err(), "prost accepted {bytes:02x?}");
            let mut after_valid_field = Outer {
                items: vec![Inner::default()],
                ..Default::default()
            }
            .encode_to_vec();
            after_valid_field.extend_from_slice(bytes);
            assert!(Outer::decode(after_valid_field.as_slice()).is_err());
        }
        // Counting stops at the first malformed field
        let mut bytes = Outer {
            items: vec![Inner::default(); 2],
            ..Default::default()
        }
        .encode_to_vec();
        bytes.extend_from_slice(&[0x0f, 0x0a, 0x00]);
        assert_eq!(check_decode_budget::<Outer>(&bytes, 10).unwrap(), 2);
        assert!(Outer::decode(bytes.as_slice()).is_err());
    }

    #[test]
    fn it_bounds_the_depth_of_recursive_types() {
        let mut node = Node::default();
        for _ in 0..MAX_DEPTH {
            node = Node {
                child: Some(Box::new(node)),
            };
        }
        assert_eq!(count::<Node>(&node, usize::MAX).unwrap(), MAX_DEPTH);

        let node = Node {
            child: Some(Box::new(node)),
        };
        count::<Node>(&node, usize::MAX).unwrap_err();
    }

    #[test]
    fn scalar_payloads_count_nothing() {
        assert_eq!(count::<u64>(&u64::MAX, 0).unwrap(), 0);
        assert_eq!(count::<String>(&"\n\0\n\0".to_string(), 0).unwrap(), 0);
        assert_eq!(count::<Vec<u8>>(&fake_messages(1_000), 0).unwrap(), 0);
        assert_eq!(count::<()>(&(), 0).unwrap(), 0);
    }
}
