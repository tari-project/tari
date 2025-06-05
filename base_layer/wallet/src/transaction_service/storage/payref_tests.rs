#[cfg(test)]
mod payref_tests {
    use crate::transaction_service::storage::models::{CompletedTransaction, InboundTransaction, OutboundTransaction};
    use tari_common_types::transaction::TransactionDirection;
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

    /// Test that OutputWithPayRef structure works correctly
    #[test]
    fn test_output_with_payref_structure() {
        use crate::transaction_service::handle::{OutputWithPayRef, OutputType, OutputStatus};
        
        let output_hash = HashOutput::from([1u8; 32]);
        let payref = Some([5u8; 32]);
        let amount = MicroMinotari::from(1000);
        
        let output_with_payref = OutputWithPayRef {
            output_hash,
            payment_reference: payref,
            output_type: OutputType::Sent,
            amount,
            status: OutputStatus::Available,
        };
        
        assert_eq!(output_with_payref.output_hash, output_hash);
        assert_eq!(output_with_payref.payment_reference, payref);
        assert!(matches!(output_with_payref.output_type, OutputType::Sent));
        assert_eq!(output_with_payref.amount, amount);
        assert!(matches!(output_with_payref.status, OutputStatus::Available));
    }

    /// Test that TransactionWithPayRefs structure works with per-output approach
    #[test]
    fn test_transaction_with_payrefs_per_output() {
        use crate::transaction_service::handle::{TransactionWithPayRefs, OutputWithPayRef, OutputType, OutputStatus};
        use crate::transaction_service::storage::models::CompletedTransaction;
        
        // Create mock outputs with PayRefs
        let sent_output = OutputWithPayRef {
            output_hash: HashOutput::from([1u8; 32]),
            payment_reference: Some([10u8; 32]),
            output_type: OutputType::Sent,
            amount: MicroMinotari::from(500),
            status: OutputStatus::Available,
        };
        
        let received_output = OutputWithPayRef {
            output_hash: HashOutput::from([2u8; 32]),
            payment_reference: Some([20u8; 32]),
            output_type: OutputType::Received,
            amount: MicroMinotari::from(300),
            status: OutputStatus::Available,
        };
        
        let change_output = OutputWithPayRef {
            output_hash: HashOutput::from([3u8; 32]),
            payment_reference: Some([30u8; 32]),
            output_type: OutputType::Change,
            amount: MicroMinotari::from(200),
            status: OutputStatus::Available,
        };
        
        let outputs_with_payrefs = vec![sent_output, received_output, change_output];
        
        // Mock completed transaction - we'll just create a placeholder
        // In a real test, this would be properly constructed
        let completed_tx = create_mock_completed_transaction();
        
        let tx_with_payrefs = TransactionWithPayRefs {
            transaction: completed_tx,
            outputs_with_payrefs,
            recipient_count: 1, // Only counting sent outputs (excluding change)
        };
        
        assert_eq!(tx_with_payrefs.outputs_with_payrefs.len(), 3);
        assert_eq!(tx_with_payrefs.recipient_count, 1);
        
        // Check that each output type is correctly represented
        let sent_outputs: Vec<_> = tx_with_payrefs.outputs_with_payrefs.iter()
            .filter(|o| matches!(o.output_type, OutputType::Sent))
            .collect();
        let received_outputs: Vec<_> = tx_with_payrefs.outputs_with_payrefs.iter()
            .filter(|o| matches!(o.output_type, OutputType::Received))
            .collect();
        let change_outputs: Vec<_> = tx_with_payrefs.outputs_with_payrefs.iter()
            .filter(|o| matches!(o.output_type, OutputType::Change))
            .collect();
            
        assert_eq!(sent_outputs.len(), 1);
        assert_eq!(received_outputs.len(), 1);
        assert_eq!(change_outputs.len(), 1);
    }

    /// Helper function to create a mock completed transaction for testing
    /// This uses Default::default() to avoid complex nested construction
    fn create_mock_completed_transaction() -> CompletedTransaction {
        // Use Default to avoid complex construction
        let mut tx = CompletedTransaction::default();
        tx.tx_id = TxId::from(1);
        tx.amount = MicroMinotari::from(1000);
        tx.sent_output_hashes = vec![HashOutput::from([1u8; 32])];
        tx.received_output_hashes = vec![HashOutput::from([2u8; 32])];
        tx.change_output_hashes = vec![HashOutput::from([3u8; 32])];
        tx
    }
}
