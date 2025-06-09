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

//! # PayRef Calculator for Transactions
//!
//! This module provides efficient PayRef calculation for transactions using stored output hashes.
//! This enables super fast PayRef generation without scanning full transaction outputs.

use tari_common_types::{
    payment_reference::{generate_payment_reference, PaymentReference},
    transaction::{TransactionDirection, TxId},
};

use crate::{
    output_manager_service::{
        payment_reference::{PaymentDetails, PaymentDirection},
        storage::OutputStatus,
    },
    transaction_service::storage::models::CompletedTransaction,
};

/// Calculate PayRefs for a transaction using stored output hashes (FAST)
///
/// This function generates PayRefs only for transactions that have been mined,
/// using the pre-computed output hashes stored with the transaction.
///
/// # Arguments
/// * `tx` - The completed transaction containing stored output hashes
///
/// # Returns
/// A vector of PaymentReferences for the transaction outputs, or empty if not mined
// pub fn calculate_transaction_payrefs(tx: &CompletedTransaction) -> Vec<PaymentReference> {
//     let mut payrefs = Vec::new();
//
//     // Only generate PayRefs if transaction is mined
//     if let Some(block_hash) = tx.mined_in_block.as_ref() {
//         match tx.direction {
//             TransactionDirection::Outbound => {
//                 // For outbound: PayRefs for sent outputs (already excludes change)
//                 for output_hash in &tx.sent_output_hashes {
//                     let payref = generate_payment_reference(block_hash, output_hash);
//                     payrefs.push(payref);
//                 }
//             },
//             TransactionDirection::Inbound => {
//                 // For inbound: PayRefs for received outputs (already excludes change)
//                 for output_hash in &tx.received_output_hashes {
//                     let payref = generate_payment_reference(block_hash, output_hash);
//                     payrefs.push(payref);
//                 }
//             },
//             TransactionDirection::Unknown => {
//                 // Skip unknown direction transactions
//             },
//         }
//     }
//
//     payrefs
// }

/// Get all PayRefs for a specific transaction ID
///
/// # Arguments
/// * `transactions` - List of completed transactions to search
/// * `tx_id` - Transaction ID to find PayRefs for
///
/// # Returns
/// Vector of PaymentReferences for the transaction, empty if not found or not mined
// pub fn get_transaction_payrefs(transactions: &[CompletedTransaction], tx_id: TxId) -> Vec<PaymentReference> {
//     if let Some(tx) = transactions.iter().find(|t| t.tx_id == tx_id) {
//         calculate_transaction_payrefs(tx)
//     } else {
//         Vec::new()
//     }
// }

/// Create payment details from a transaction and PayRef
fn create_payment_details(
    tx: &CompletedTransaction,
    payref: PaymentReference,
    direction: PaymentDirection,
) -> PaymentDetails {
    PaymentDetails {
        payment_reference: payref,
        commitment: Default::default(),
        amount: tx.amount,
        direction,
        status: OutputStatus::Spent, // Since this is a completed transaction, mark as spent
        block_height: tx.mined_height.unwrap_or(0),
        block_hash: tx.mined_in_block.unwrap_or_default(),
        mined_timestamp: tx.mined_timestamp,
        confirmations: tx.confirmations.unwrap_or(0),
        payment_id: Some(tx.payment_id.to_bytes().to_vec()),
    }
}

#[cfg(test)]
mod tests {
    use tari_common_types::{
        transaction::TransactionStatus,
        types::{BlockHash, HashOutput, PrivateKey, Signature},
    };
    use tari_core::transactions::{
        tari_amount::MicroMinotari,
        transaction_components::{encrypted_data::PaymentId, Transaction},
    };

    use super::*;

    fn create_test_transaction() -> CompletedTransaction {
        CompletedTransaction {
            tx_id: TxId::from(1u64),
            source_address: Default::default(),
            destination_address: Default::default(),
            amount: MicroMinotari::from(1000),
            fee: MicroMinotari::from(10),
            transaction: Transaction::new(vec![], vec![], vec![], PrivateKey::default(), PrivateKey::default()),
            status: TransactionStatus::Broadcast,
            timestamp: chrono::Utc::now(),
            cancelled: None,
            direction: TransactionDirection::Outbound,
            send_count: 0,
            last_send_timestamp: None,
            transaction_signature: Signature::default(),
            confirmations: Some(5),
            mined_height: Some(100),
            mined_in_block: Some(BlockHash::from([1u8; 32])),
            mined_timestamp: Some(chrono::Utc::now()),
            payment_id: PaymentId::default(),
            sent_output_hashes: vec![HashOutput::from([2u8; 32])],
            received_output_hashes: vec![],
            change_output_hashes: vec![],
        }
    }

    #[test]
    fn test_calculate_transaction_payrefs_outbound() {
        let tx = create_test_transaction();
        let payrefs = calculate_transaction_payrefs(&tx);

        assert_eq!(payrefs.len(), 1);
        // PayRef should be generated from block hash and output hash
        let expected_payref = generate_payment_reference(&BlockHash::from([1u8; 32]), &HashOutput::from([2u8; 32]));
        assert_eq!(payrefs[0], expected_payref);
    }

    #[test]
    fn test_calculate_transaction_payrefs_not_mined() {
        let mut tx = create_test_transaction();
        tx.mined_in_block = None;

        let payrefs = calculate_transaction_payrefs(&tx);
        assert_eq!(payrefs.len(), 0);
    }

    #[test]
    fn test_find_payment_by_reference() {
        let tx = create_test_transaction();
        let transactions = vec![tx];

        let target_payref = generate_payment_reference(&BlockHash::from([1u8; 32]), &HashOutput::from([2u8; 32]));

        let result = find_payment_by_reference(&transactions, target_payref);
        assert!(result.is_some());

        let payment_details = result.unwrap();
        assert_eq!(payment_details.payment_reference, target_payref);
        assert_eq!(payment_details.amount, MicroMinotari::from(1000));
    }

}
