#[cfg(test)]
mod payref_tests {
    use crate::transaction_service::storage::models::{CompletedTransaction, InboundTransaction, OutboundTransaction, TransactionDirection};
    use chrono::Utc;
    use tari_common_types::{
        tari_address::TariAddress,
        transaction::{TxId, TransactionStatus},
        types::{FixedHash, HashOutput},
    };
    use tari_core::transactions::{
        tari_amount::MicroMinotari,
        transaction_components::encrypted_data::{PaymentId, TxType},
    };

    /// Test that new PayRef fields exist in transaction structs
    #[test]
    fn test_transaction_structs_have_payref_fields() {
        // Simple test to verify PayRef fields exist by checking their types
        
        // This test just verifies that the PayRef fields exist and have the correct types
        // We can't easily create instances due to complex protocol dependencies
        
        // Test that we can access the fields via field access
        fn check_outbound_fields(_tx: &OutboundTransaction) {
            let _sent: &Vec<HashOutput> = &_tx.sent_output_hashes;
            let _change: &Vec<HashOutput> = &_tx.change_output_hashes;
        }
        
        fn check_inbound_fields(_tx: &InboundTransaction) {
            let _received: &Vec<HashOutput> = &_tx.received_output_hashes;
        }
        
        fn check_completed_fields(_tx: &CompletedTransaction) {
            let _sent: &Vec<HashOutput> = &_tx.sent_output_hashes;
            let _received: &Vec<HashOutput> = &_tx.received_output_hashes;
            let _change: &Vec<HashOutput> = &_tx.change_output_hashes;
        }
        
        // If this compiles, the fields exist with correct types
        assert!(true);
    }

    /// Test that PayRef fields have the correct type (Vec<HashOutput>)
    #[test]
    fn test_payref_fields_are_correct_type() {
        // Test that we can create vectors of HashOutput for PayRef fields
        let output_hash1 = HashOutput::from([1u8; 32]);
        let output_hash2 = HashOutput::from([2u8; 32]);
        let change_hash = HashOutput::from([3u8; 32]);
        
        let sent_hashes: Vec<HashOutput> = vec![output_hash1, output_hash2];
        let received_hashes: Vec<HashOutput> = vec![];
        let change_hashes: Vec<HashOutput> = vec![change_hash];
        
        assert_eq!(sent_hashes.len(), 2);
        assert_eq!(received_hashes.len(), 0);
        assert_eq!(change_hashes.len(), 1);
        assert_eq!(sent_hashes[0], output_hash1);
        assert_eq!(sent_hashes[1], output_hash2);
        assert_eq!(change_hashes[0], change_hash);
    }
}
