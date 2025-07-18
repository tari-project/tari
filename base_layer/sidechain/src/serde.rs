// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::borrow::Cow;

use serde::{Deserialize, Deserializer, Serializer};
/// Encodes a byte type as a hex string if the serializer is human-readable, Otherwise efficiently encodes it as
/// bytes.
/// This is different from the standard serde byte encoding, which encodes a sequence of u8s (e.g. [1, 2, 3]) as a JSON
/// array.
pub mod hex_or_bytes {
    use super::*;

    pub fn serialize<S: Serializer, T: AsRef<[u8]>>(v: &T, s: S) -> Result<S::Ok, S::Error> {
        if s.is_human_readable() {
            let st = hex::encode(v.as_ref());
            s.serialize_str(&st)
        } else {
            s.serialize_bytes(v.as_ref())
        }
    }

    /// Use a serde deserializer to serialize the hex string of the given object.
    pub fn deserialize<'de, D, T>(d: D) -> Result<T, D::Error>
    where
        D: Deserializer<'de>,
        T: for<'a> TryFrom<&'a [u8]>,
    {
        let value = if d.is_human_readable() {
            let hex = <Cow<'_, str> as Deserialize>::deserialize(d)?;
            let bytes = hex::decode(&*hex).map_err(serde::de::Error::custom)?;
            T::try_from(&bytes).map_err(|_| serde::de::Error::custom("Failed to convert bytes to T"))?
        } else {
            let bytes = <Cow<'_, [u8]> as Deserialize>::deserialize(d)?;
            T::try_from(&bytes).map_err(|_| serde::de::Error::custom("Failed to convert bytes to T"))?
        };

        Ok(value)
    }
}
