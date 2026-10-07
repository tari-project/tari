// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! A cheap, type-aware pre-decode check that bounds how much memory decoding a protobuf payload can cost.
//!
//! prost allocates a full Rust struct for every embedded message it decodes. A `repeated` message field costs two
//! bytes per element on the wire (tag and a zero length) but hundreds of bytes once decoded, so a byte cap alone does
//! not bound memory: a 6 MiB request of empty elements decodes into several GB.
//!
//! [DecodeBudget] counts the message instances a payload would make prost allocate, without allocating anything. It
//! follows the message schema: `#[derive(DecodeBudget)]` (from `tari_comms_rpc_macros`) generates, for each prost type,
//! a walker that charges one instance for every length-delimited field whose tag is a message-typed field and descends
//! into it with that field's own walker. `bytes`, `string`, packed scalar and unknown fields are skipped, never
//! entered, so attacker-chosen bytes (ciphertexts, script data) are never mistaken for messages. Every tari protobuf
//! type gets the derive through `tari_common::build::ProtobufCompiler`.
//!
//! Malformed wire data (an invalid wire type, a truncated varint, a length running past the end) simply stops the
//! count: prost will reject the payload when it decodes it, so the walk only has to be right for well-formed input.

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

    /// Charges one map entry, encoded in `contents`, whose value (tag 2) is a message `V`, and walks the value.
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

/// Counts the message instances decoding a protobuf payload would allocate. Implemented with
/// `#[derive(tari_comms_rpc_macros::DecodeBudget)]` for prost types; the defaults (count nothing) suit scalar types.
pub trait DecodeBudget {
    /// Walks the fields of one encoded instance of this message in `buf`, charging `budget` for every embedded message.
    fn count_messages(_buf: &[u8], _budget: &mut Budget) -> Result<(), DecodeBudgetExceeded> {
        Ok(())
    }

    /// For prost oneof enums: charges the length-delimited field `tag` (holding `contents`) if it is one of this
    /// oneof's message variants.
    fn count_oneof_field(_tag: u32, _contents: &[u8], _budget: &mut Budget) -> Result<(), DecodeBudgetExceeded> {
        Ok(())
    }
}

impl DecodeBudget for () {}
impl DecodeBudget for bool {}
impl DecodeBudget for u32 {}
impl DecodeBudget for u64 {}
impl DecodeBudget for i32 {}
impl DecodeBudget for i64 {}
impl DecodeBudget for f32 {}
impl DecodeBudget for f64 {}
impl DecodeBudget for String {}
impl DecodeBudget for Vec<u8> {}
impl DecodeBudget for bytes::Bytes {}

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

/// Calls `f(tag, contents, budget)` for every length-delimited field in `buf`, skipping all other fields. Stops (with
/// `Ok`) at the first malformed field.
pub fn walk_len_fields<F>(buf: &[u8], budget: &mut Budget, mut f: F) -> Result<(), DecodeBudgetExceeded>
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
        let skip = match key & 0x7 {
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
            numbers: vec![10; 100],
        };
        assert_eq!(count::<Outer>(&msg, DEFAULT_MAX_DECODE_ITEMS).unwrap(), 9 + 5 + 6 + 4);

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

    #[test]
    fn it_rejects_a_flat_flood() {
        let flood = Outer {
            items: vec![Inner::default(); 3 * 1024 * 1024],
            ..Default::default()
        };
        let err = count::<Outer>(&flood, DEFAULT_MAX_DECODE_ITEMS).unwrap_err();
        assert_eq!(err, DecodeBudgetExceeded {
            items: DEFAULT_MAX_DECODE_ITEMS + 1,
            max: DEFAULT_MAX_DECODE_ITEMS
        });
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
            // Wire types 3, 4, 6 and 7
            &[0x0b, 0x01],
            &[0x0c, 0x01],
            &[0x0e, 0x01],
            &[0x0f, 0x01],
            // Truncated varints
            &[0x80],
            &[0x08, 0x80],
            // LEN that runs past the end of the buffer
            &[0x0a, 0x7f, 0x01],
            // Truncated fixed-width values
            &[0x09, 0x01],
            &[0x0d, 0x01],
        ];
        for bytes in malformed {
            assert_eq!(check_decode_budget::<Outer>(bytes, 0).unwrap(), 0);
        }
        // Counting stops at the first malformed field
        let mut bytes = Outer {
            items: vec![Inner::default(); 2],
            ..Default::default()
        }
        .encode_to_vec();
        bytes.extend_from_slice(&[0x0f, 0x0a, 0x00]);
        assert_eq!(check_decode_budget::<Outer>(&bytes, 10).unwrap(), 2);
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
