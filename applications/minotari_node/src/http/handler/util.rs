// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause
use serde::{Deserialize, Deserializer};
use tari_utilities::hex;

/// Deserializer for hex string to bytes
pub fn from_hex<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where D: Deserializer<'de> {
    let s: &str = Deserialize::deserialize(deserializer)?;
    hex::from_hex(s).map_err(serde::de::Error::custom)
}

pub fn from_hex_comma_separated<'de, D>(deserializer: D) -> Result<Vec<Vec<u8>>, D::Error>
where D: Deserializer<'de> {
    let s: &str = Deserialize::deserialize(deserializer)?;
    let s = s.trim();
    if s.is_empty() {
        return Ok(vec![]);
    }
    let mut res = Vec::new();
    for s in s.split(',') {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            continue; // Skip empty segments
        }
        if !trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(serde::de::Error::custom("Invalid hex string"));
        }
        let bytes = hex::from_hex(trimmed).map_err(serde::de::Error::custom)?;
        res.push(bytes);
    }
    Ok(res)
}
