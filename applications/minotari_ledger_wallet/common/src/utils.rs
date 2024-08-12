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

/// The Tari dual address size
pub const TARI_DUAL_ADDRESS_SIZE: usize = 67;

/// Convert a serialized Tari dual address to a base58 string
pub fn tari_dual_address_display(address_bytes: &[u8; TARI_DUAL_ADDRESS_SIZE]) -> Result<String, String> {
    validate_checksum(address_bytes.as_ref())?;
    let mut base58 = "".to_string();
    base58.push_str(&bs58::encode(&address_bytes[0..1]).into_string());
    base58.push_str(&bs58::encode(&address_bytes[1..2].to_vec()).into_string());
    base58.push_str(&bs58::encode(&address_bytes[2..]).into_string());
    Ok(base58)
}

/// Get the public spend key bytes from a serialized Tari dual address
pub fn get_public_spend_key_from_tari_dual_address(
    address_bytes: &[u8; TARI_DUAL_ADDRESS_SIZE],
) -> Result<[u8; 32], String> {
    validate_checksum(address_bytes.as_ref())?;
    let mut public_spend_key = [0u8; 32];
    public_spend_key.copy_from_slice(&address_bytes[34..66]);
    Ok(public_spend_key)
}

// Determine whether a byte slice ends with a valid checksum
// If it is valid, returns the underlying data slice (without the checksum)
fn validate_checksum(data: &[u8]) -> Result<&[u8], String> {
    // Empty data is not allowed, nor data only consisting of a checksum
    if data.len() < 2 {
        return Err("ChecksumError::InputDataTooShort".to_string());
    }

    // It's sufficient to check the entire slice against a zero checksum
    match compute_checksum(data) {
        0u8 => Ok(&data[..data.len() - 1]),
        _ => Err("ChecksumError::InvalidChecksum".to_string()),
    }
}

// Compute the DammSum checksum for a byte slice
fn compute_checksum(data: &[u8]) -> u8 {
    // Perform the Damm algorithm
    let mask = mask();
    let mut result = 0u8;

    for digit in data {
        result ^= *digit; // add
        let overflow = (result & (1 << 7)) != 0;
        result <<= 1; // double
        if overflow {
            // reduce
            result ^= mask;
        }
    }

    result
}

// Set up the mask, fixed for a dictionary size of `2^8 == 256`
// This can fail on invalid coefficients, which will cause a panic
// To ensure this doesn't happen in production, it is directly tested
fn mask() -> u8 {
    const COEFFICIENTS: [u8; 3] = [4, 3, 1];
    let mut mask = 1u8;

    for bit in COEFFICIENTS {
        let shift = 1u8.checked_shl(u32::from(bit)).unwrap();
        mask = mask.checked_add(shift).unwrap();
    }

    mask
}
