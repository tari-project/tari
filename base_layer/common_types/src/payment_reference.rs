// Copyright 2024. The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

//! # Payment Reference (PayRef) Utilities
//!
//! This module provides shared utilities for Payment Reference (PayRef) generation and validation
//! across the Tari ecosystem. PayRefs are globally unique identifiers for individual transaction
//! outputs that enable payment verification without compromising privacy.
//!
//! ## PayRef Generation
//!
//! PayRefs are generated using the formula:
//! ```text
//! PayRef = Blake2b_256(block_hash || output_hash)
//! ```
//!
//! This approach ensures:
//! - Global uniqueness (block hashes are unique across blockchain history)
//! - Verifiability (any party can compute and verify PayRefs)
//! - Privacy preservation (no additional information leakage)
//! - Stability (becomes permanent after sufficient confirmations)

use blake2::Blake2b;
use digest::consts::U32;
use serde::{Deserialize, Serialize};
use tari_crypto::hashing::DomainSeparatedHasher;
use tari_hashing::PaymentReferenceHashDomain;
use tari_utilities::hex::{Hex, HexError};

use crate::types::{BlockHash, FixedHash, HashOutput};
/// A Payment Reference (PayRef) - a 32-byte globally unique identifier for transaction outputs
pub type PaymentReference = FixedHash;

/// Errors that can occur during PayRef operations
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub enum PayRefError {
    /// Invalid PayRef format (must be 64-character hex string)
    #[error("Invalid PayRef format: expected 64 hexadecimal characters, got {0}")]
    InvalidFormat(String),
    /// PayRef hex string contains invalid characters
    #[error("Invalid PayRef hex: contains non-hexadecimal characters")]
    InvalidHex,
    /// PayRef decodes to wrong length (must be exactly 32 bytes)
    #[error("Invalid PayRef length: expected 32 bytes, got {0}")]
    InvalidLength(usize),
    /// Missing required data for PayRef generation
    #[error("Missing required data: {0}")]
    MissingData(String),
    /// PayRef not found in database or index
    #[error("PayRef not found")]
    NotFound,
    /// Database error during PayRef operations
    #[error("Database error: {0}")]
    DatabaseError(String),
    #[error("Hex conversion error: {0}")]
    HexError(String),
}

impl From<HexError> for PayRefError {
    fn from(err: HexError) -> Self {
        PayRefError::HexError(err.to_string())
    }
}

/// Generate a Payment Reference from block hash and output hash
///
/// This is the canonical PayRef generation function used throughout the Tari ecosystem.
/// It uses domain-separated Blake2b hashing to ensure security and prevent hash collisions
/// with other parts of the system.
///
/// # Arguments
/// * `block_hash` - Hash of the block containing the output
/// * `output_hash` - Hash of the transaction output
///
/// # Returns
/// A 32-byte Payment Reference that is globally unique and verifiable
///
/// # Example
/// ```rust
/// use tari_common_types::{
///     payment_reference::generate_payment_reference,
///     types::{BlockHash, HashOutput},
/// };
///
/// let block_hash = BlockHash::from([1u8; 32]);
/// let output_hash = HashOutput::from([2u8; 32]);
/// let payref = generate_payment_reference(&block_hash, &output_hash);
/// println!("PayRef: {}", hex::encode(payref));
/// ```
pub fn generate_payment_reference(block_hash: &BlockHash, output_hash: &HashOutput) -> PaymentReference {
    let mut hasher =
        DomainSeparatedHasher::<Blake2b<U32>, PaymentReferenceHashDomain>::new_with_label("payment_reference");
    hasher.update(block_hash.as_slice());
    hasher.update(output_hash.as_slice());
    let mut output = [0u8; 32];
    hasher.finalize_into_reset(digest::generic_array::GenericArray::from_mut_slice(&mut output));
    FixedHash::from(output)
}

/// Parse a Payment Reference from a hexadecimal string
///
/// # Arguments
/// * `hex_str` - 64-character hexadecimal string representation of the PayRef
///
/// # Returns
/// * `Ok(PaymentReference)` - Successfully parsed 32-byte PayRef
/// * `Err(PayRefError)` - Invalid format, invalid hex, or wrong length
///
/// # Example
/// ```rust
/// use tari_common_types::payment_reference::parse_payment_reference_hex;
///
/// let payref_hex = "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
/// match parse_payment_reference_hex(payref_hex) {
///     Ok(payref) => println!("Parsed PayRef: {:?}", payref),
///     Err(e) => eprintln!("Error: {}", e),
/// }
/// ```
pub fn parse_payment_reference_hex(hex_str: &str) -> Result<PaymentReference, PayRefError> {
    let hash = FixedHash::from_hex(hex_str)?;
    Ok(hash)
}

/// Format a Payment Reference for display according to the specified format
///
/// # Arguments
/// * `payref` - 32-byte Payment Reference
/// * `format` - Display format specification
///
/// # Returns
/// Formatted string representation of the PayRef
pub fn format_payment_reference(payref: &PaymentReference, format: &PayRefDisplayFormat) -> String {
    let hex = payref.to_hex();
    match format {
        PayRefDisplayFormat::Full => hex,
        PayRefDisplayFormat::Shortened => {
            if hex.len() >= 16 {
                format!("{}...{}", &hex[0..8], &hex[hex.len() - 8..])
            } else {
                hex
            }
        },
        PayRefDisplayFormat::Custom {
            prefix_chars,
            suffix_chars,
        } => {
            let prefix = *prefix_chars as usize;
            let suffix = *suffix_chars as usize;
            if hex.len() >= (prefix + suffix) {
                format!("{}...{}", &hex[0..prefix], &hex[hex.len() - suffix..])
            } else {
                hex
            }
        },
    }
}

/// Display format options for Payment References
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PayRefDisplayFormat {
    /// Show full 64-character hex string
    Full,
    /// Show shortened format (8...8)
    Shortened,
    /// Custom format with specified prefix and suffix character counts
    Custom { prefix_chars: u8, suffix_chars: u8 },
}

impl Default for PayRefDisplayFormat {
    fn default() -> Self {
        Self::Shortened
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: Full integration tests require the specific commitment/block hash types
    // These basic tests verify the utility functions work correctly

    #[test]
    fn test_parse_payment_reference_hex_valid() {
        let hex_str = "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
        let result = parse_payment_reference_hex(hex_str);
        assert!(result.is_ok());

        let payref = result.unwrap();
        assert_eq!(payref.len(), 32);
    }

    #[test]
    fn test_format_payment_reference() {
        let payref = FixedHash::from([
            0x12, 0x34, 0x56, 0x78, 0x90, 0xab, 0xcd, 0xef, 0x12, 0x34, 0x56, 0x78, 0x90, 0xab, 0xcd, 0xef, 0x12, 0x34,
            0x56, 0x78, 0x90, 0xab, 0xcd, 0xef, 0x12, 0x34, 0x56, 0x78, 0x90, 0xab, 0xcd, 0xef,
        ]);

        // Test full format
        let full = format_payment_reference(&payref, &PayRefDisplayFormat::Full);
        assert_eq!(full, "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef");

        // Test shortened format
        let shortened = format_payment_reference(&payref, &PayRefDisplayFormat::Shortened);
        assert_eq!(shortened, "12345678...90abcdef");

        // Test custom format
        let custom = format_payment_reference(&payref, &PayRefDisplayFormat::Custom {
            prefix_chars: 4,
            suffix_chars: 4,
        });
        assert_eq!(custom, "1234...cdef");
    }

    #[test]
    fn test_round_trip_hex_conversion() {
        let original_hex = "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
        let payref = parse_payment_reference_hex(original_hex).unwrap();
        let converted_hex = payref.to_hex();
        assert_eq!(original_hex, converted_hex);
    }
}
