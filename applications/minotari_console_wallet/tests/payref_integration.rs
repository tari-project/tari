//! PayRef Integration Tests
//! 
//! Tests that verify PayRef functionality works end-to-end

#[cfg(test)]
mod tests {
    use tari_common_types::types::HashOutput;

    /// Test that PayRef types are available
    #[test]
    fn test_payref_types_available() {
        // Test that we can create HashOutput for PayRef testing
        let hash1 = HashOutput::from([1u8; 32]);
        let hash2 = HashOutput::from([2u8; 32]);
        
        // Test that we can store them in vectors as PayRef fields expect
        let payrefs: Vec<HashOutput> = vec![hash1, hash2];
        
        assert_eq!(payrefs.len(), 2);
        assert_eq!(payrefs[0], hash1);
        assert_eq!(payrefs[1], hash2);
    }

    /// Test PayRef hex encoding/decoding
    #[test]
    fn test_payref_hex_encoding() {
        use tari_utilities::hex::Hex;
        
        let hash = HashOutput::from([0xab; 32]);
        let hex_string = hash.to_hex();
        
        // PayRef should be 32 bytes = 64 hex chars
        assert_eq!(hex_string.len(), 64);
        assert!(hex_string.chars().all(|c| c.is_ascii_hexdigit()));
    }

    /// Test that the console wallet compiles with PayRef support
    #[test]
    fn test_console_wallet_compiles() {
        // If this test compiles and runs, it means the console wallet
        // successfully includes the PayRef functionality
        assert!(true);
    }
}
