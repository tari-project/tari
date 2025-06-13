// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use alloc::{
    string::{String, ToString},
    vec::Vec,
};

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

/// The Tari dual address minimum size (standard dual address)
pub const TARI_DUAL_ADDRESS_MIN_SIZE: usize = 67;
/// The Tari dual address maximum size (with maximum 256-byte payment ID)
pub const TARI_DUAL_ADDRESS_MAX_SIZE: usize = 323; // 67 + 256

/// Convert a serialized Tari dual address to a base58 string
pub fn tari_dual_address_display(address_bytes: &[u8]) -> Result<String, String> {
    if address_bytes.len() < TARI_DUAL_ADDRESS_MIN_SIZE || address_bytes.len() > TARI_DUAL_ADDRESS_MAX_SIZE {
        return Err("Invalid address size".to_string());
    }
    validate_checksum(address_bytes)?;
    let mut base58 = "".to_string();
    base58.push_str(&bs58::encode(&address_bytes[0..1]).into_string());
    base58.push_str(&bs58::encode(&address_bytes[1..2].to_vec()).into_string());
    base58.push_str(&bs58::encode(&address_bytes[2..]).into_string());
    Ok(base58)
}

/// Get the public spend key bytes from a serialized Tari dual address
pub fn get_public_spend_key_bytes_from_tari_dual_address(address_bytes: &[u8]) -> Result<[u8; 32], String> {
    if address_bytes.len() < TARI_DUAL_ADDRESS_MIN_SIZE || address_bytes.len() > TARI_DUAL_ADDRESS_MAX_SIZE {
        return Err("Invalid address size".to_string());
    }
    validate_checksum(address_bytes)?;
    let mut public_spend_key_bytes = [0u8; 32];
    public_spend_key_bytes.copy_from_slice(&address_bytes[34..66]);
    Ok(public_spend_key_bytes)
}

/// Extract payment ID bytes from integrated address, if present
pub fn get_payment_id_bytes_from_tari_dual_address(address_bytes: &[u8]) -> Result<Vec<u8>, String> {
    validate_checksum(address_bytes)?;
    if address_bytes.len() <= TARI_DUAL_ADDRESS_MIN_SIZE {
        return Ok(Vec::new()); // No payment ID
    }

    // Payment ID data is between spend key and checksum
    let payment_id_start = 66;
    let payment_id_end = address_bytes.len() - 1; // Exclude checksum
    Ok(address_bytes[payment_id_start..payment_id_end].to_vec())
}

/// Check if address has payment ID
pub fn address_has_payment_id(address_bytes: &[u8]) -> Result<bool, String> {
    validate_checksum(address_bytes)?;
    Ok(address_bytes.len() > TARI_DUAL_ADDRESS_MIN_SIZE)
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

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    // Helper function to create a test address with checksum
    fn create_test_address(size: usize) -> Vec<u8> {
        let mut address = vec![0u8; size - 1]; // -1 for checksum
                                               // Set some test data
        address[0] = 0x01; // Network/version
        address[1] = 0x02; // Features
                           // Public spend key at positions 34..66
        for i in 34..66 {
            if i < address.len() {
                address[i] = u8::try_from(i - 34).expect("index within u8 range");
            }
        }
        // Add payment ID data if larger than min size
        if size > TARI_DUAL_ADDRESS_MIN_SIZE {
            for i in 66..(size - 1) {
                if i < address.len() {
                    address[i] = 0xAA; // Payment ID data
                }
            }
        }

        // Compute and append checksum
        let checksum = compute_checksum(&address);
        address.push(checksum);
        address
    }

    #[test]
    fn test_standard_dual_address_display() {
        let address = create_test_address(TARI_DUAL_ADDRESS_MIN_SIZE);
        assert_eq!(address.len(), TARI_DUAL_ADDRESS_MIN_SIZE);

        let result = tari_dual_address_display(&address);
        assert!(result.is_ok());
    }

    #[test]
    fn test_integrated_address_display() {
        // Test with small payment ID
        let address_small = create_test_address(TARI_DUAL_ADDRESS_MIN_SIZE + 10);
        let result = tari_dual_address_display(&address_small);
        assert!(result.is_ok());

        // Test with maximum size
        let address_max = create_test_address(TARI_DUAL_ADDRESS_MAX_SIZE);
        let result = tari_dual_address_display(&address_max);
        assert!(result.is_ok());
    }

    #[test]
    fn test_payment_id_extraction() {
        // Test standard address (no payment ID)
        let standard_address = create_test_address(TARI_DUAL_ADDRESS_MIN_SIZE);
        let payment_id = get_payment_id_bytes_from_tari_dual_address(&standard_address).unwrap();
        assert!(payment_id.is_empty());

        // Test integrated address with payment ID
        let integrated_address = create_test_address(TARI_DUAL_ADDRESS_MIN_SIZE + 32);
        let payment_id = get_payment_id_bytes_from_tari_dual_address(&integrated_address).unwrap();
        assert_eq!(payment_id.len(), 32);
        assert!(payment_id.iter().all(|&b| b == 0xAA)); // Test data
    }

    #[test]
    fn test_address_has_payment_id() {
        let standard_address = create_test_address(TARI_DUAL_ADDRESS_MIN_SIZE);
        assert!(!address_has_payment_id(&standard_address).unwrap());

        let integrated_address = create_test_address(TARI_DUAL_ADDRESS_MIN_SIZE + 16);
        assert!(address_has_payment_id(&integrated_address).unwrap());
    }

    #[test]
    fn test_address_size_validation() {
        // Test minimum valid size
        let min_address = create_test_address(TARI_DUAL_ADDRESS_MIN_SIZE);
        assert!(tari_dual_address_display(&min_address).is_ok());

        // Test maximum valid size
        let max_address = create_test_address(TARI_DUAL_ADDRESS_MAX_SIZE);
        assert!(tari_dual_address_display(&max_address).is_ok());
    }

    #[test]
    fn test_invalid_address_sizes() {
        // Test too small
        let too_small = vec![0u8; TARI_DUAL_ADDRESS_MIN_SIZE - 1];
        assert!(tari_dual_address_display(&too_small).is_err());

        // Test too large
        let too_large = vec![0u8; TARI_DUAL_ADDRESS_MAX_SIZE + 1];
        assert!(tari_dual_address_display(&too_large).is_err());
    }

    #[test]
    fn test_public_spend_key_extraction() {
        let address = create_test_address(TARI_DUAL_ADDRESS_MIN_SIZE + 50);
        let spend_key = get_public_spend_key_bytes_from_tari_dual_address(&address).unwrap();

        // Verify the spend key matches our test data
        for (i, &byte) in spend_key.iter().enumerate() {
            assert_eq!(byte, u8::try_from(i).expect("index within u8 range"));
        }
    }
}
