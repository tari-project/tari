// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use alloc::{
    borrow::ToOwned,
    string::{String, ToString},
    vec::Vec,
};

pub const PUSH_PUBKEY_IDENTIFIER: &str = "217e";

/// Convert a u16 to a string
pub fn u16_to_string(number: u16) -> String {
    let mut buffer = [0u8; 6]; // Maximum length for a 16-bit integer (including null terminator)
    let mut pos = 0;

    if number == 0 {
        buffer[pos] = b'0';
        pos += 1;
    } else {
        let mut num = number;

        let mut digits = [0u8; 6];
        let mut num_digits = 0;

        while num > 0 {
            digits[num_digits] = b'0' + (num % 10) as u8;
            num /= 10;
            num_digits += 1;
        }

        while num_digits > 0 {
            num_digits -= 1;
            buffer[pos] = digits[num_digits];
            pos += 1;
        }
    }

    String::from_utf8_lossy(&buffer[..pos]).to_string()
}

/// Convert a hex string to serialized bytes made up as an identifier concatenated with data
pub fn hex_to_bytes_serialized(identifier: &str, data: &str) -> Result<Vec<u8>, String> {
    if identifier.len() % 2 != 0 {
        return Err("Invalid identifier".to_string());
    }
    if data.len() % 2 != 0 {
        return Err("Invalid payload".to_string());
    }

    let hex = identifier.to_owned() + data;
    let hex = hex.as_str();

    let mut serialized = Vec::new();
    for i in 0..hex.len() / 2 {
        let byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).map_err(|_e| "Invalid hex string".to_string())?;
        serialized.push(byte);
    }
    Ok(serialized)
}
