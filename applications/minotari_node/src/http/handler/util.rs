// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause
use serde::{Deserialize, Deserializer, Serializer};
use tari_utilities::hex;

/// Deserializer for hex string to bytes
pub fn from_hex<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where D: Deserializer<'de> {
    let s: &str = Deserialize::deserialize(deserializer)?;
    hex::from_hex(s).map_err(serde::de::Error::custom)
}
