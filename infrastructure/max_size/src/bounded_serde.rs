// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Building blocks for the hand written serde `Deserialize` implementations of the `MaxSize*` types.
//!
//! The derived implementations these replace decoded the whole inner collection first and only then checked its
//! length, so a large payload was fully decoded (and allocated) before being rejected. The helpers here check the
//! bound while decoding, and keep the exact serde shape of the derived implementations: a struct with a single
//! value field (plus, for `MaxSizeVec`, a unit `_marker` field), unknown fields ignored and duplicate fields
//! rejected.

use std::fmt;

use serde::de::{self, DeserializeSeed, Deserializer, SeqAccess, Visitor};

use crate::checked_de::cautious_capacity;

/// A field of one of the `MaxSize*` serde structs.
pub(crate) enum Field {
    /// The field holding the bounded value (`inner`, `string` or `vec`).
    Value,
    /// The `_marker` (`PhantomData`) field of `MaxSizeVec`.
    Marker,
    /// Any other field; ignored, as the derived implementation did.
    Ignore,
}

/// Deserializes a field identifier the way a derived `Deserialize` does: by name, by (byte string) name, or by
/// index.
pub(crate) struct FieldSeed {
    /// The name of the value field; it has index 0.
    pub value: &'static str,
    /// The name of the marker field, if the struct has one; it has index 1.
    pub marker: Option<&'static str>,
}

impl<'de> DeserializeSeed<'de> for FieldSeed {
    type Value = Field;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_identifier(self)
    }
}

impl<'de> Visitor<'de> for FieldSeed {
    type Value = Field;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("field identifier")
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
        Ok(match (v, self.marker) {
            (0, _) => Field::Value,
            (1, Some(_)) => Field::Marker,
            _ => Field::Ignore,
        })
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
        self.visit_bytes(v.as_bytes())
    }

    fn visit_bytes<E: de::Error>(self, v: &[u8]) -> Result<Self::Value, E> {
        if v == self.value.as_bytes() {
            Ok(Field::Value)
        } else if self.marker.is_some_and(|m| v == m.as_bytes()) {
            Ok(Field::Marker)
        } else {
            Ok(Field::Ignore)
        }
    }
}

/// Collects the elements of a sequence into a `Vec`, failing as soon as it is known to hold more than `max`
/// elements.
///
/// A size hint larger than `max` (which a length prefixed format like bincode provides up front) is rejected before
/// any element is read. Otherwise at most `max + 1` elements are decoded. The size hint is attacker controlled, so it
/// is clamped to `max` and to a small fixed allocation before being used for preallocation. `too_long` builds the
/// error from the (lower bound of the) offending length.
pub(crate) fn visit_bounded_seq<'de, A, T, E>(
    mut seq: A,
    max: usize,
    too_long: impl Fn(usize) -> E,
) -> Result<Vec<T>, A::Error>
where
    A: SeqAccess<'de>,
    T: de::Deserialize<'de>,
    E: fmt::Display,
{
    let hint = seq.size_hint();
    if let Some(hint) = hint &&
        hint > max
    {
        return Err(de::Error::custom(too_long(hint)));
    }
    let mut vec = Vec::with_capacity(cautious_capacity::<T>(hint.unwrap_or(0).min(max)));
    while let Some(item) = seq.next_element()? {
        if vec.len() >= max {
            return Err(de::Error::custom(too_long(max.saturating_add(1))));
        }
        vec.push(item);
    }
    Ok(vec)
}
