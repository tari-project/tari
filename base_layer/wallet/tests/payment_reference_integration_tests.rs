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

//! Integration tests for Payment Reference (PayRef) functionality
//!
//! These tests verify the complete payment verification workflow from
//! wallet payment generation to exchange verification.

use std::time::Duration;

use chrono::Utc;
use minotari_wallet::{
    output_manager_service::{
        payment_reference::{PayRefStatus, PaymentDirection},
        storage::{
            models::{DbWalletOutput, SpendingPriority},
            OutputSource, OutputStatus,
        },
    },
};
use tari_common_types::{
    payment_reference::{generate_payment_reference, parse_payment_reference_hex},
    types::{BlockHash, HashOutput},
};
use tari_core::transactions::{
    tari_amount::MicroMinotari,
    test_helpers::{create_wallet_output_with_data, TestParams},
    transaction_components::{
        encrypted_data::PaymentId,
        OutputFeatures,
    },
};
use tari_script::TariScript;
use tari_utilities::hex::Hex;

/// Test the complete PayRef generation and verification workflow
#[tokio::test]
async fn test_payref_generation_and_verification_workflow() {
    // Create test key manager and parameters
    let key_manager = tari_core::transactions::transaction_key_manager::create_memory_db_key_manager().unwrap();
    let test_params = TestParams::new(&key_manager).await;
    
    // Create test output that simulates a received payment
    let output = create_wallet_output_with_data(
        TariScript::default(),
        OutputFeatures::default(),
        &test_params,
        MicroMinotari::from(1000000), // 1 XTR
        &key_manager,
    ).await.unwrap();
    
    // Simulate the output being mined in a block
    let block_hash = BlockHash::from([1u8; 32]);
    let block_height = 100u64;
    let commitment = output.commitment(&key_manager).await.unwrap();
    
    let db_output = DbWalletOutput {
        commitment: commitment.clone(),
        wallet_output: output.clone(),
        hash: output.hash(&key_manager).await.unwrap(),
        status: OutputStatus::Unspent,
        mined_height: Some(block_height),
        mined_in_block: Some(block_hash),
        mined_timestamp: Some(Utc::now()),
        marked_deleted_at_height: None,
        marked_deleted_in_block: None,
        spending_priority: SpendingPriority::Normal,
        source: OutputSource::OneSided,
        received_in_tx_id: Some(1u64.into()),
        spent_in_tx_id: None,
        payment_id: PaymentId::Empty,
    };
    
    // Test PayRef generation
    let expected_payref = generate_payment_reference(&block_hash, &db_output.hash);
    let generated_payref = db_output.generate_payment_reference().unwrap();
    
    assert_eq!(expected_payref, generated_payref);
    
    // Test PayRef status with sufficient confirmations
    let current_tip_height = block_height + 5; // 6 confirmations
    let status = db_output.get_payment_reference_status(current_tip_height, 5);
    
    match status {
        PayRefStatus::Available(payref, confirmations) => {
            assert_eq!(payref, expected_payref);
            assert_eq!(confirmations, 6);
        },
        _ => panic!("Expected PayRef to be available with sufficient confirmations"),
    }
    
    // Test PayRef status with insufficient confirmations
    let current_tip_height = block_height + 2; // 3 confirmations
    let status = db_output.get_payment_reference_status(current_tip_height, 5);
    
    match status {
        PayRefStatus::Pending(current_confs, blocks_remaining) => {
            assert_eq!(current_confs, 3);
            assert_eq!(blocks_remaining, 2);
        },
        _ => panic!("Expected PayRef to be pending with insufficient confirmations"),
    }
    
    // Test PayRef lookup/verification
    assert!(db_output.matches_payment_reference(&expected_payref));
    
    // Test with wrong PayRef
    let wrong_payref = [0u8; 32];
    assert!(!db_output.matches_payment_reference(&wrong_payref));
    
    // Test payment details generation
    let payment_details = db_output.get_payment_details(current_tip_height + 5, 5).unwrap();
    assert_eq!(payment_details.payment_reference, expected_payref);
    assert_eq!(payment_details.amount, MicroMinotari::from(1000000));
    assert_eq!(payment_details.block_height, block_height);
    assert_eq!(payment_details.block_hash, block_hash);
    assert_eq!(payment_details.direction, PaymentDirection::Received);
    assert_eq!(payment_details.confirmations, 6);
}

/// Test PayRef hex parsing and validation
#[test]
fn test_payref_hex_parsing_and_validation() {
    // Test valid PayRef hex
    let valid_hex = "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
    let parsed = parse_payment_reference_hex(valid_hex).unwrap();
    assert_eq!(parsed.len(), 32);
    
    // Test invalid length
    let invalid_length = "1234567890abcdef";
    assert!(parse_payment_reference_hex(invalid_length).is_err());
    
    // Test invalid hex characters
    let invalid_hex = "gggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggg";
    assert!(parse_payment_reference_hex(invalid_hex).is_err());
    
    // Test mixed case (should work)
    let mixed_case = "1234567890ABCDef1234567890abcdef1234567890ABCDEF1234567890abcdef";
    let parsed_mixed = parse_payment_reference_hex(mixed_case).unwrap();
    assert_eq!(parsed_mixed.len(), 32);
}

/// Test end-to-end exchange verification workflow simulation
#[tokio::test]
async fn test_exchange_verification_workflow_simulation() {
    // This test simulates the complete workflow from user payment to exchange verification
    
    // Step 1: User (sender) creates and sends a payment
    let key_manager = tari_core::transactions::transaction_key_manager::create_memory_db_key_manager().unwrap();
    let test_params = TestParams::new(&key_manager).await;
    let payment_amount = MicroMinotari::from(5000000); // 5 XTR
    
    let sender_output = create_wallet_output_with_data(
        TariScript::default(),
        OutputFeatures::default(),
        &test_params,
        payment_amount,
        &key_manager,
    ).await.unwrap();
    
    // Step 2: Payment gets mined in a block
    let block_hash = BlockHash::from([2u8; 32]);
    let block_height = 200u64;
    let commitment = sender_output.commitment(&key_manager).await.unwrap();
    
    // Step 3: Sender wallet records the sent payment
    let sender_db_output = DbWalletOutput {
        commitment: commitment.clone(),
        wallet_output: sender_output.clone(),
        hash: sender_output.hash(&key_manager).await.unwrap(),
        status: OutputStatus::Spent, // Spent by sender
        mined_height: Some(block_height),
        mined_in_block: Some(block_hash),
        mined_timestamp: Some(Utc::now()),
        marked_deleted_at_height: None,
        marked_deleted_in_block: None,
        spending_priority: SpendingPriority::Normal,
        source: OutputSource::Standard,
        received_in_tx_id: Some(1u64.into()), // Transaction ID
        spent_in_tx_id: None,
        payment_id: PaymentId::Empty,
    };
    
    // Step 4: Exchange (receiver) wallet records the received payment
    let receiver_db_output = DbWalletOutput {
        commitment: commitment.clone(),
        wallet_output: sender_output.clone(), // Same output, received by exchange
        hash: sender_output.hash(&key_manager).await.unwrap(),
        status: OutputStatus::Unspent, // Unspent by receiver
        mined_height: Some(block_height),
        mined_in_block: Some(block_hash),
        mined_timestamp: Some(Utc::now()),
        marked_deleted_at_height: None,
        marked_deleted_in_block: None,
        spending_priority: SpendingPriority::Normal,
        source: OutputSource::OneSided, // Received as one-sided payment
        received_in_tx_id: Some(1u64.into()), // Same transaction ID
        spent_in_tx_id: None,
        payment_id: PaymentId::Empty,
    };
    
    // Step 5: Generate PayRef from sender's perspective
    let current_tip_height = block_height + 10; // Sufficient confirmations
    let sender_payref = sender_db_output.generate_payment_reference().unwrap();
    
    // Verify sender can see PayRef as available
    let sender_status = sender_db_output.get_payment_reference_status(current_tip_height, 5);
    match sender_status {
        PayRefStatus::Available(payref, confirmations) => {
            assert_eq!(payref, sender_payref);
            assert_eq!(confirmations, 11);
        },
        _ => panic!("Sender should see PayRef as available"),
    }
    
    // Step 6: User provides PayRef to exchange support
    let payref_hex = sender_payref.to_hex();
    println!("Customer provides PayRef: {}", payref_hex);
    
    // Step 7: Exchange verifies the PayRef
    let parsed_payref = parse_payment_reference_hex(&payref_hex).unwrap();
    assert_eq!(parsed_payref, sender_payref);
    
    // Exchange checks if they received this payment
    assert!(receiver_db_output.matches_payment_reference(&parsed_payref));
    
    // Exchange gets payment details
    let payment_details = receiver_db_output.get_payment_details(current_tip_height, 10).unwrap();
    assert_eq!(payment_details.payment_reference, sender_payref);
    assert_eq!(payment_details.amount, payment_amount);
    assert_eq!(payment_details.direction, PaymentDirection::Received);
    assert!(payment_details.confirmations >= 10); // Exchange requires more confirmations
    
    println!("Exchange verified payment: {} XTR", payment_details.amount);
    
    // Step 8: Simulate insufficient confirmations scenario
    let low_tip_height = block_height + 2; // Only 3 confirmations
    let low_conf_status = receiver_db_output.get_payment_reference_status(low_tip_height, 10);
    
    match low_conf_status {
        PayRefStatus::Pending(current_confs, blocks_remaining) => {
            assert_eq!(current_confs, 3);
            assert_eq!(blocks_remaining, 7);
            println!("Payment found but needs {} more confirmations", blocks_remaining);
        },
        _ => panic!("Should be pending with insufficient confirmations"),
    }
}

/// Test PayRef collision resistance
#[test]
fn test_payref_collision_resistance() {
    // Test that different block hash + commitment combinations produce different PayRefs
    let block_hash1 = BlockHash::from([1u8; 32]);
    let block_hash2 = BlockHash::from([2u8; 32]);
    
    // Create dummy output hashes
    let output_hash1 = HashOutput::from([1u8; 32]);
    let output_hash2 = HashOutput::from([2u8; 32]);
    
    let payref1 = generate_payment_reference(&block_hash1, &output_hash1);
    let payref2 = generate_payment_reference(&block_hash2, &output_hash1);
    let payref3 = generate_payment_reference(&block_hash1, &output_hash2);
    let payref4 = generate_payment_reference(&block_hash2, &output_hash2);
    
    // All PayRefs should be different
    assert_ne!(payref1, payref2);
    assert_ne!(payref1, payref3);
    assert_ne!(payref1, payref4);
    assert_ne!(payref2, payref3);
    assert_ne!(payref2, payref4);
    assert_ne!(payref3, payref4);
    
    // Same inputs should produce same output (deterministic)
    let payref1_again = generate_payment_reference(&block_hash1, &output_hash1);
    assert_eq!(payref1, payref1_again);
}

/// Test PayRef stability across blockchain reorganizations
#[tokio::test]
async fn test_payref_stability_and_reorg_handling() {
    // This test simulates how PayRefs behave during blockchain reorganizations
    
    let key_manager = tari_core::transactions::transaction_key_manager::create_memory_db_key_manager().unwrap();
    let test_params = TestParams::new(&key_manager).await;
    let output = create_wallet_output_with_data(
        TariScript::default(),
        OutputFeatures::default(),
        &test_params,
        MicroMinotari::from(1000000),
        &key_manager,
    ).await.unwrap();
    
    // Original block
    let original_block_hash = BlockHash::from([1u8; 32]);
    let block_height = 100u64;
    let commitment = output.commitment(&key_manager).await.unwrap();
    
    let mut db_output = DbWalletOutput {
        commitment: commitment.clone(),
        wallet_output: output.clone(),
        hash: output.hash(&key_manager).await.unwrap(),
        status: OutputStatus::Unspent,
        mined_height: Some(block_height),
        mined_in_block: Some(original_block_hash),
        mined_timestamp: Some(Utc::now()),
        marked_deleted_at_height: None,
        marked_deleted_in_block: None,
        spending_priority: SpendingPriority::Normal,
        source: OutputSource::OneSided,
        received_in_tx_id: Some(1u64.into()),
        spent_in_tx_id: None,
        payment_id: PaymentId::Empty,
    };
    
    // Generate original PayRef
    let original_payref = db_output.generate_payment_reference().unwrap();
    
    // Test with sufficient confirmations
    let stable_tip_height = block_height + 10; // Well beyond reorg risk
    let stable_status = db_output.get_payment_reference_status(stable_tip_height, 5);
    
    match stable_status {
        PayRefStatus::Available(payref, confirmations) => {
            assert_eq!(payref, original_payref);
            assert_eq!(confirmations, 11);
        },
        _ => panic!("PayRef should be stable with many confirmations"),
    }
    
    // Simulate blockchain reorganization (PayRef would change if block hash changes)
    let reorg_block_hash = BlockHash::from([2u8; 32]); // Different block hash
    db_output.mined_in_block = Some(reorg_block_hash);
    
    let reorg_payref = db_output.generate_payment_reference().unwrap();
    assert_ne!(original_payref, reorg_payref); // PayRef changes with different block
    
    // This demonstrates why confirmation requirements are important for PayRef stability
    // In practice, wallets should wait for sufficient confirmations before showing PayRefs
}

/// Test PayRef performance with large numbers of outputs
#[tokio::test]
async fn test_payref_performance_with_many_outputs() {
    let num_outputs = 100; // Reduced for test efficiency
    let mut generated_payrefs = Vec::new();
    let mut all_outputs = Vec::new();
    
    let start_time = std::time::Instant::now();
    
    // Create shared key manager for efficiency
    let key_manager = tari_core::transactions::transaction_key_manager::create_memory_db_key_manager().unwrap();
    let test_params = TestParams::new(&key_manager).await;
    
    // Generate many outputs with PayRefs
    for i in 0..num_outputs {
        let output = create_wallet_output_with_data(
            TariScript::default(),
            OutputFeatures::default(),
            &test_params,
            MicroMinotari::from(1000000 + i), // Unique amounts
            &key_manager,
        ).await.unwrap();
        
        let block_hash = BlockHash::from([(i % 256) as u8; 32]); // Varied block hashes
        let block_height = 100 + i;
        
        let output_commitment = output.commitment(&key_manager).await.unwrap();
        let db_output = DbWalletOutput {
            commitment: output_commitment.clone(),
            wallet_output: output.clone(),
            hash: output.hash(&key_manager).await.unwrap(),
            status: OutputStatus::Unspent,
            mined_height: Some(block_height),
            mined_in_block: Some(block_hash),
            mined_timestamp: Some(Utc::now()),
            marked_deleted_at_height: None,
            marked_deleted_in_block: None,
            spending_priority: SpendingPriority::Normal,
            source: OutputSource::OneSided,
            received_in_tx_id: Some((i + 1).into()),
            spent_in_tx_id: None,
            payment_id: PaymentId::Empty,
        };
        
        if let Some(payref) = db_output.generate_payment_reference() {
            generated_payrefs.push(payref);
        }
        all_outputs.push(db_output);
    }
    
    let generation_time = start_time.elapsed();
    println!("Generated {} PayRefs in {:?}", num_outputs, generation_time);
    
    // Test PayRef lookup performance
    let lookup_start = std::time::Instant::now();
    let test_payref = generated_payrefs[num_outputs as usize / 2]; // Middle PayRef
    
    let mut found = false;
    for output in &all_outputs {
        if output.matches_payment_reference(&test_payref) {
            found = true;
            break;
        }
    }
    
    let lookup_time = lookup_start.elapsed();
    println!("PayRef lookup took {:?}", lookup_time);
    
    assert!(found, "Should find the test PayRef");
    assert_eq!(generated_payrefs.len(), num_outputs as usize);
    
    // Verify all PayRefs are unique
    let mut unique_payrefs = std::collections::HashSet::new();
    for payref in &generated_payrefs {
        assert!(unique_payrefs.insert(*payref), "Found duplicate PayRef");
    }
    
    // Performance should be reasonable even with many outputs
    assert!(generation_time < Duration::from_secs(5), "PayRef generation took too long");
    assert!(lookup_time < Duration::from_millis(100), "PayRef lookup took too long");
}
