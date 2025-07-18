// Copyright 2020. The Tari Project
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

use std::{
    convert::TryFrom,
    fmt::{Display, Error, Formatter},
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tari_common_types::{
    payment_reference::{generate_payment_reference, PaymentReference},
    tari_address::TariAddress,
    transaction::{TransactionConversionError, TransactionDirection, TransactionStatus, TxId},
    types::{BlockHash, CompressedCommitment, FixedHash, PrivateKey, Signature},
};
use tari_core::{
    consensus::ConsensusConstants,
    transactions::{
        fee::Fee,
        tari_amount::MicroMinotari,
        transaction_components::{payment_id::PaymentId, Transaction},
        ReceiverTransactionProtocol,
        SenderTransactionProtocol,
    },
};

use crate::transaction_service::error::TransactionStorageError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InboundTransaction {
    pub tx_id: TxId,
    pub source_address: TariAddress,
    pub amount: MicroMinotari,
    pub receiver_protocol: ReceiverTransactionProtocol,
    pub status: TransactionStatus,
    pub payment_id: PaymentId,
    pub timestamp: DateTime<Utc>,
    pub cancelled: bool,
    pub direct_send_success: bool,
    pub send_count: u32,
    pub last_send_timestamp: Option<DateTime<Utc>>,
    /// Hashes of outputs received from others (excluding change)
    pub received_output_hashes: Vec<FixedHash>,
}

impl InboundTransaction {
    pub fn new(
        tx_id: TxId,
        source_address: TariAddress,
        amount: MicroMinotari,
        receiver_protocol: ReceiverTransactionProtocol,
        status: TransactionStatus,
        payment_id: PaymentId,
        timestamp: DateTime<Utc>,
    ) -> Self {
        Self {
            tx_id,
            source_address,
            amount,
            receiver_protocol,
            status,
            payment_id,
            timestamp,
            cancelled: false,
            direct_send_success: false,
            send_count: 0,
            last_send_timestamp: None,
            received_output_hashes: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OutboundTransaction {
    pub tx_id: TxId,
    pub destination_address: TariAddress,
    pub amount: MicroMinotari,
    pub fee: MicroMinotari,
    pub sender_protocol: SenderTransactionProtocol,
    pub status: TransactionStatus,
    pub payment_id: PaymentId,
    pub timestamp: DateTime<Utc>,
    pub cancelled: bool,
    pub direct_send_success: bool,
    pub send_count: u32,
    pub last_send_timestamp: Option<DateTime<Utc>>,
    /// Hashes of outputs being sent to others (excluding change)
    pub sent_output_hashes: Vec<FixedHash>,
}

impl OutboundTransaction {
    pub fn new(
        tx_id: TxId,
        destination_address: TariAddress,
        amount: MicroMinotari,
        fee: MicroMinotari,
        sender_protocol: SenderTransactionProtocol,
        status: TransactionStatus,
        payment_id: PaymentId,
        timestamp: DateTime<Utc>,
        direct_send_success: bool,
    ) -> Self {
        Self {
            tx_id,
            destination_address,
            amount,
            fee,
            sender_protocol,
            status,
            payment_id,
            timestamp,
            cancelled: false,
            direct_send_success,
            send_count: 0,
            last_send_timestamp: None,
            sent_output_hashes: Vec::new(),
        }
    }

    pub fn new_with_output_hashes(
        tx_id: TxId,
        destination_address: TariAddress,
        amount: MicroMinotari,
        fee: MicroMinotari,
        sender_protocol: SenderTransactionProtocol,
        status: TransactionStatus,
        payment_id: PaymentId,
        timestamp: DateTime<Utc>,
        direct_send_success: bool,
        sent_output_hashes: Vec<FixedHash>,
    ) -> Self {
        Self {
            tx_id,
            destination_address,
            amount,
            fee,
            sender_protocol,
            status,
            payment_id,
            timestamp,
            cancelled: false,
            direct_send_success,
            send_count: 0,
            last_send_timestamp: None,
            sent_output_hashes,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompletedTransaction {
    pub tx_id: TxId,
    pub source_address: TariAddress,
    pub destination_address: TariAddress,
    pub amount: MicroMinotari,
    pub fee: MicroMinotari,
    pub transaction: Transaction,
    pub status: TransactionStatus,
    pub timestamp: DateTime<Utc>,
    pub cancelled: Option<TxCancellationReason>,
    pub direction: TransactionDirection,
    pub send_count: u32,
    pub last_send_timestamp: Option<DateTime<Utc>>,
    pub transaction_signature: Signature,
    pub mined_height: Option<u64>,
    pub mined_in_block: Option<BlockHash>,
    pub mined_timestamp: Option<DateTime<Utc>>,
    pub payment_id: PaymentId,
    /// Hashes of outputs being sent to others (excluding change)
    pub sent_output_hashes: Vec<FixedHash>,
    /// Hashes of outputs received from others (excluding change)
    pub received_output_hashes: Vec<FixedHash>,
    /// Hashes of change outputs (for reference)
    pub change_output_hashes: Vec<FixedHash>,
}

impl CompletedTransaction {
    pub fn new(
        tx_id: TxId,
        source_address: TariAddress,
        destination_address: TariAddress,
        amount: MicroMinotari,
        fee: MicroMinotari,
        transaction: Transaction,
        status: TransactionStatus,
        timestamp: DateTime<Utc>,
        direction: TransactionDirection,
        mined_height: Option<u64>,
        mined_timestamp: Option<DateTime<Utc>>,
        payment_id: PaymentId,
    ) -> Result<Self, TransactionStorageError> {
        if status == TransactionStatus::Coinbase {
            return Err(TransactionStorageError::CoinbaseNotSupported);
        }
        let transaction_signature = if let Some(excess_sig) = transaction.first_kernel_excess_sig() {
            excess_sig.clone()
        } else {
            Signature::default()
        };
        Ok(Self {
            tx_id,
            source_address,
            destination_address,
            amount,
            fee,
            transaction,
            status,
            timestamp,
            cancelled: None,
            direction,
            send_count: 0,
            last_send_timestamp: None,
            transaction_signature,
            mined_height,
            mined_in_block: None,
            mined_timestamp,
            payment_id,
            sent_output_hashes: Vec::new(),
            received_output_hashes: Vec::new(),
            change_output_hashes: Vec::new(),
        })
    }

    /// Helper function to calculate fee_per_gram from total fee and original transaction. The resulting fee_per_gram is
    /// rounded up to ensure we don't underpay.
    ///
    /// # Parameters
    /// - `total_fee`: The target total fee to pay for the transaction
    /// - `consensus_manager`: The consensus manager to use for calculating the transaction weight
    /// - `num_inputs`: Number of transaction inputs for the new transaction
    /// - `num_outputs`: Number of transaction outputs for the new transaction
    ///
    /// # Returns
    /// A tuple of (weight_in_grams, fee_per_gram) where:
    /// - `weight_in_grams`: The calculated transaction weight in grams
    /// - `fee_per_gram`: The fee per gram calculated from total_fee / weight_in_grams
    ///
    /// # Notes
    /// Calculates actual features and scripts sizes from the original transaction outputs,
    /// then converts total fee to fee_per_gram using floating point division to avoid truncation.
    ///
    /// # Errors
    ///
    /// Returns `TransactionStorageError::FailedToCalculateTransactionFee` if:
    /// - `total_fee` is zero
    /// - `num_inputs` is zero
    /// - `num_outputs` is zero
    /// - The transaction has no outputs (`self.transaction.body.outputs().len()` is zero)
    /// - Failed to calculate features and scripts size from the original transaction
    pub fn calculate_fee_per_gram_from_total_fee(
        &self,
        total_fee: MicroMinotari,
        consensus_constants: &ConsensusConstants,
        num_inputs: usize,
        num_outputs: usize,
    ) -> Result<(u64, MicroMinotari), TransactionStorageError> {
        // Check for zero values that would cause division by zero or invalid calculations
        if total_fee == MicroMinotari::zero() {
            return Err(TransactionStorageError::FailedToCalculateTransactionFee(
                "Total fee cannot be zero".to_string(),
            ));
        }

        if num_inputs == 0 {
            return Err(TransactionStorageError::FailedToCalculateTransactionFee(
                "Number of inputs cannot be zero".to_string(),
            ));
        }

        if num_outputs == 0 {
            return Err(TransactionStorageError::FailedToCalculateTransactionFee(
                "Number of outputs cannot be zero".to_string(),
            ));
        }

        let original_outputs = self.transaction.body.outputs();
        if original_outputs.is_empty() {
            return Err(TransactionStorageError::FailedToCalculateTransactionFee(
                "Transaction must have at least one output".to_string(),
            ));
        }

        let fee_calculator = Fee::new(*consensus_constants.transaction_weight_params());

        // Calculate average features and scripts size from actual transaction outputs
        let total_features_and_scripts_size: Result<usize, std::io::Error> = original_outputs
            .iter()
            .map(|output| output.get_features_and_scripts_size())
            .sum();

        let total_features_and_scripts_size = total_features_and_scripts_size.map_err(|e| {
            TransactionStorageError::FailedToCalculateTransactionFee(format!(
                "Failed to calculate features and scripts size from original transaction: {}",
                e
            ))
        })?;

        // Calculate average size per output from original transaction
        let avg_features_and_scripts_size_per_output = total_features_and_scripts_size / original_outputs.len();

        // Apply rounding and multiply by number of outputs for new transaction
        let features_and_scripts_size = fee_calculator
            .weighting()
            .round_up_features_and_scripts_size(avg_features_and_scripts_size_per_output) *
            num_outputs;

        // Use the Fee struct's weighting calculation to get transaction weight in grams
        let weight_in_grams = fee_calculator.weighting().calculate(
            1, // num_kernels
            num_inputs,
            num_outputs,
            features_and_scripts_size,
        );

        let fee_per_gram = if weight_in_grams > 0 {
            // Use ceiling division to ensure we never underestimate the fee
            let fee_per_gram_u64 = total_fee.0.div_ceil(weight_in_grams);
            // Ensure minimum of 1 (though ceiling division should handle this for positive values)
            MicroMinotari::from(fee_per_gram_u64.max(1))
        } else {
            MicroMinotari::from(1)
        };

        Ok((weight_in_grams, fee_per_gram))
    }

    /// Extract input commitments from a CompletedTransaction
    /// This is useful when you need to get the input commitments from a broadcast transaction
    /// Returns only the commitments that have full data (not compact/hash-only inputs)
    pub fn get_input_commitments_from_completed_transaction(
        &self,
    ) -> Result<Vec<CompressedCommitment>, TransactionStorageError> {
        let commitments: Vec<CompressedCommitment> = self
            .transaction
            .body
            .inputs()
            .iter()
            .filter_map(|input| match input.commitment() {
                Ok(commitment) => Some(commitment.clone()),
                Err(_) => {
                    // Skip compact inputs that don't have commitment data
                    None
                },
            })
            .collect();
        Ok(commitments)
    }

    /// Create a CompletedTransaction with specified output hashes for PayRef functionality
    pub fn new_with_output_hashes(
        tx_id: TxId,
        source_address: TariAddress,
        destination_address: TariAddress,
        amount: MicroMinotari,
        fee: MicroMinotari,
        transaction: Transaction,
        status: TransactionStatus,
        timestamp: DateTime<Utc>,
        direction: TransactionDirection,
        mined_height: Option<u64>,
        mined_timestamp: Option<DateTime<Utc>>,
        payment_id: PaymentId,
        sent_output_hashes: Vec<FixedHash>,
        received_output_hashes: Vec<FixedHash>,
        change_output_hashes: Vec<FixedHash>,
    ) -> Result<Self, TransactionStorageError> {
        if status == TransactionStatus::Coinbase {
            return Err(TransactionStorageError::CoinbaseNotSupported);
        }
        let transaction_signature = if let Some(excess_sig) = transaction.first_kernel_excess_sig() {
            excess_sig.clone()
        } else {
            Signature::default()
        };
        Ok(Self {
            tx_id,
            source_address,
            destination_address,
            amount,
            fee,
            transaction,
            status,
            timestamp,
            cancelled: None,
            direction,
            send_count: 0,
            last_send_timestamp: None,
            transaction_signature,
            mined_height,
            mined_in_block: None,
            mined_timestamp,
            payment_id,
            sent_output_hashes,
            received_output_hashes,
            change_output_hashes,
        })
    }

    pub fn calculate_received_payment_references(&self) -> Vec<PaymentReference> {
        if let Some(block_hash) = self.mined_in_block.as_ref() {
            return self
                .received_output_hashes
                .iter()
                .map(|hash| generate_payment_reference(block_hash, hash))
                .collect();
        }
        vec![]
    }

    pub fn calculate_sent_payment_references(&self) -> Vec<PaymentReference> {
        if let Some(block_hash) = self.mined_in_block.as_ref() {
            return self
                .sent_output_hashes
                .iter()
                .map(|hash| generate_payment_reference(block_hash, hash))
                .collect();
        }
        vec![]
    }

    pub fn calculate_change_payment_references(&self) -> Vec<PaymentReference> {
        if let Some(block_hash) = self.mined_in_block.as_ref() {
            return self
                .change_output_hashes
                .iter()
                .map(|hash| generate_payment_reference(block_hash, hash))
                .collect();
        }
        vec![]
    }

    pub fn from_outbound(tx: OutboundTransaction, change_output_hashes: Vec<FixedHash>) -> Self {
        let transaction = if tx.sender_protocol.is_finalized() {
            match tx.sender_protocol.get_transaction() {
                Ok(tx) => tx.clone(),
                Err(_) => Transaction::new(vec![], vec![], vec![], PrivateKey::default(), PrivateKey::default()),
            }
        } else {
            Transaction::new(vec![], vec![], vec![], PrivateKey::default(), PrivateKey::default())
        };
        let transaction_signature = if let Some(excess_sig) = transaction.first_kernel_excess_sig() {
            excess_sig.clone()
        } else {
            Signature::default()
        };
        Self {
            tx_id: tx.tx_id,
            source_address: Default::default(),
            destination_address: tx.destination_address,
            amount: tx.amount,
            fee: tx.fee,
            status: tx.status,
            timestamp: tx.timestamp,
            cancelled: if tx.cancelled {
                Some(TxCancellationReason::UserCancelled)
            } else {
                None
            },
            transaction,
            direction: TransactionDirection::Outbound,
            send_count: 0,
            last_send_timestamp: None,
            transaction_signature,
            mined_height: None,
            mined_in_block: None,
            mined_timestamp: None,
            payment_id: tx.payment_id,
            sent_output_hashes: tx.sent_output_hashes,
            received_output_hashes: Vec::new(),
            change_output_hashes,
        }
    }
}

impl From<CompletedTransaction> for InboundTransaction {
    fn from(ct: CompletedTransaction) -> Self {
        Self {
            tx_id: ct.tx_id,
            source_address: ct.source_address,
            amount: ct.amount,
            receiver_protocol: ReceiverTransactionProtocol::new_placeholder(),
            status: ct.status,
            payment_id: ct.payment_id,
            timestamp: ct.timestamp,
            cancelled: ct.cancelled.is_some(),
            direct_send_success: false,
            send_count: 0,
            last_send_timestamp: None,
            received_output_hashes: ct.received_output_hashes,
        }
    }
}

impl From<CompletedTransaction> for OutboundTransaction {
    fn from(ct: CompletedTransaction) -> Self {
        Self {
            tx_id: ct.tx_id,
            destination_address: ct.destination_address,
            amount: ct.amount,
            fee: ct.fee,
            sender_protocol: SenderTransactionProtocol::new_placeholder(),
            status: ct.status,
            payment_id: ct.payment_id,
            timestamp: ct.timestamp,
            cancelled: ct.cancelled.is_some(),
            direct_send_success: false,
            send_count: 0,
            last_send_timestamp: None,
            sent_output_hashes: ct.sent_output_hashes,
        }
    }
}

impl From<InboundTransaction> for CompletedTransaction {
    fn from(tx: InboundTransaction) -> Self {
        Self {
            tx_id: tx.tx_id,
            source_address: tx.source_address,
            destination_address: Default::default(),
            amount: tx.amount,
            fee: MicroMinotari::from(0),
            status: tx.status,
            timestamp: tx.timestamp,
            cancelled: if tx.cancelled {
                Some(TxCancellationReason::UserCancelled)
            } else {
                None
            },
            transaction: Transaction::new(vec![], vec![], vec![], PrivateKey::default(), PrivateKey::default()),
            direction: TransactionDirection::Inbound,
            send_count: 0,
            last_send_timestamp: None,
            transaction_signature: Signature::default(),
            mined_height: None,
            mined_in_block: None,
            mined_timestamp: None,
            payment_id: tx.payment_id,
            sent_output_hashes: Vec::new(),
            received_output_hashes: tx.received_output_hashes,
            change_output_hashes: Vec::new(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum WalletTransaction {
    PendingInbound(InboundTransaction),
    PendingOutbound(OutboundTransaction),
    Completed(CompletedTransaction),
}

impl WalletTransaction {
    pub fn source_address(&self) -> Option<TariAddress> {
        match self {
            WalletTransaction::PendingInbound(tx) => Some(tx.source_address.clone()),
            WalletTransaction::PendingOutbound(_) => None,
            WalletTransaction::Completed(tx) => Some(tx.source_address.clone()),
        }
    }
}

impl From<WalletTransaction> for CompletedTransaction {
    fn from(tx: WalletTransaction) -> Self {
        match tx {
            WalletTransaction::PendingInbound(tx) => CompletedTransaction::from(tx),
            WalletTransaction::PendingOutbound(tx) => CompletedTransaction::from_outbound(tx, Vec::new()),
            WalletTransaction::Completed(tx) => tx,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TxCancellationReason {
    Unknown,            // 0
    UserCancelled,      // 1
    Timeout,            // 2
    DoubleSpend,        // 3
    Orphan,             // 4
    TimeLocked,         // 5
    InvalidTransaction, // 6
    Oversized,          // 7
}

impl TryFrom<u32> for TxCancellationReason {
    type Error = TransactionConversionError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(TxCancellationReason::Unknown),
            1 => Ok(TxCancellationReason::UserCancelled),
            2 => Ok(TxCancellationReason::Timeout),
            3 => Ok(TxCancellationReason::DoubleSpend),
            4 => Ok(TxCancellationReason::Orphan),
            5 => Ok(TxCancellationReason::TimeLocked),
            6 => Ok(TxCancellationReason::InvalidTransaction),
            7 => Ok(TxCancellationReason::Oversized),
            code => Err(TransactionConversionError { code: code as i32 }),
        }
    }
}

impl Display for TxCancellationReason {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), Error> {
        #[allow(clippy::enum_glob_use)]
        use TxCancellationReason::*;
        let response = match self {
            Unknown => "Unknown",
            UserCancelled => "User Cancelled",
            Timeout => "Timeout",
            DoubleSpend => "Double Spend",
            Orphan => "Orphan",
            TimeLocked => "TimeLocked",
            InvalidTransaction => "Invalid Transaction",
            Oversized => "Oversized",
        };
        fmt.write_str(response)
    }
}

#[cfg(test)]
mod test {
    use chrono::Utc;
    use tari_common::configuration::Network;
    use tari_common_types::{
        tari_address::TariAddress,
        transaction::{TransactionDirection, TransactionStatus, TxId},
        types::{PrivateKey, RangeProof, Signature},
    };
    use tari_core::{
        consensus::ConsensusManager,
        covenants::Covenant,
        transactions::{
            tari_amount::MicroMinotari,
            transaction_components::{
                payment_id::PaymentId,
                EncryptedData,
                OutputFeatures,
                Transaction,
                TransactionOutput,
            },
        },
    };
    use tari_script::TariScript;

    use super::*;

    fn create_test_completed_transaction(num_outputs: usize) -> CompletedTransaction {
        // Create minimal test outputs with dummy data
        let mut outputs = Vec::new();
        for _i in 0..num_outputs {
            let output = TransactionOutput::new_current_version(
                OutputFeatures::default(),
                Default::default(),          // Use default commitment for testing
                Some(RangeProof::default()), // Use default range proof for testing
                TariScript::default(),
                Default::default(), // sender_offset_public_key
                Default::default(), // metadata_signature
                Covenant::default(),
                EncryptedData::default(),
                MicroMinotari::from(1000),
            );
            outputs.push(output);
        }

        // Create a minimal transaction with dummy data
        let transaction = Transaction::new(
            vec![], // inputs
            outputs,
            vec![], // kernels
            PrivateKey::default(),
            PrivateKey::default(),
        );

        CompletedTransaction {
            tx_id: TxId::from(1u64),
            source_address: TariAddress::default(),
            destination_address: TariAddress::default(),
            amount: MicroMinotari::from(1000),
            fee: MicroMinotari::from(100),
            transaction,
            status: TransactionStatus::Completed,
            timestamp: Utc::now(),
            cancelled: None,
            direction: TransactionDirection::Outbound,
            send_count: 0,
            last_send_timestamp: None,
            transaction_signature: Signature::default(),
            mined_height: None,
            mined_in_block: None,
            mined_timestamp: None,
            payment_id: PaymentId::default(),
            sent_output_hashes: vec![],
            received_output_hashes: vec![],
            change_output_hashes: vec![],
        }
    }

    #[test]
    fn test_calculate_fee_per_gram_basic_cases() {
        let consensus_manager = ConsensusManager::builder(Network::LocalNet).build().unwrap();
        let completed_tx = create_test_completed_transaction(2);

        // Test case 1: Exact division (400 / 200 = 2)
        let total_fee = MicroMinotari::from(400);
        let tip_height = 100; // Ordinary number, doesn't matter in this case
        let result = completed_tx
            .calculate_fee_per_gram_from_total_fee(total_fee, consensus_manager.consensus_constants(tip_height), 1, 2)
            .unwrap();
        let (weight, fee_per_gram) = result;

        // Verify the calculated fee meets or exceeds the requested fee
        let calculated_fee = fee_per_gram.0 * weight;
        assert!(
            calculated_fee >= total_fee.0,
            "Calculated fee {} should be >= requested fee {}",
            calculated_fee,
            total_fee.0
        );
    }

    #[test]
    fn test_calculate_fee_per_gram_rounding_up() {
        let consensus_manager = ConsensusManager::builder(Network::LocalNet).build().unwrap();
        let completed_tx = create_test_completed_transaction(2);

        // Test case 2: Should round up (134 / 200 = 0.67, should become 1)
        let total_fee = MicroMinotari::from(134);
        let tip_height = 100; // Ordinary number, doesn't matter in this case
        let result = completed_tx
            .calculate_fee_per_gram_from_total_fee(total_fee, consensus_manager.consensus_constants(tip_height), 1, 2)
            .unwrap();
        let (weight, fee_per_gram) = result;

        // fee_per_gram should be at least 1
        assert!(
            fee_per_gram.0 >= 1,
            "fee_per_gram should be at least 1, got {}",
            fee_per_gram.0
        );

        // Verify the calculated fee meets or exceeds the requested fee
        let calculated_fee = fee_per_gram.0 * weight;
        assert!(
            calculated_fee >= total_fee.0,
            "Calculated fee {} should be >= requested fee {} (weight: {}, fee_per_gram: {})",
            calculated_fee,
            total_fee.0,
            weight,
            fee_per_gram.0
        );
    }

    #[test]
    fn test_calculate_fee_per_gram_small_amounts() {
        let consensus_manager = ConsensusManager::builder(Network::LocalNet).build().unwrap();
        let completed_tx = create_test_completed_transaction(1);

        // Test case 3: Very small fee
        let total_fee = MicroMinotari::from(1);
        let tip_height = 100; // Ordinary number, doesn't matter in this case
        let result = completed_tx
            .calculate_fee_per_gram_from_total_fee(total_fee, consensus_manager.consensus_constants(tip_height), 1, 1)
            .unwrap();
        let (weight, fee_per_gram) = result;

        // fee_per_gram should be at least 1
        assert!(fee_per_gram.0 >= 1, "fee_per_gram should be at least 1");

        // Verify the calculated fee meets or exceeds the requested fee
        let calculated_fee = fee_per_gram.0 * weight;
        assert!(
            calculated_fee >= total_fee.0,
            "Calculated fee {} should be >= requested fee {}",
            calculated_fee,
            total_fee.0
        );
    }

    #[test]
    fn test_calculate_fee_per_gram_large_amounts() {
        let consensus_manager = ConsensusManager::builder(Network::LocalNet).build().unwrap();
        let completed_tx = create_test_completed_transaction(3);

        // Test case 4: Large fee
        let total_fee = MicroMinotari::from(1_000_000);
        let tip_height = 100; // Ordinary number, doesn't matter in this case
        let result = completed_tx
            .calculate_fee_per_gram_from_total_fee(total_fee, consensus_manager.consensus_constants(tip_height), 2, 3)
            .unwrap();
        let (weight, fee_per_gram) = result;

        // Verify the calculated fee meets or exceeds the requested fee
        let calculated_fee = fee_per_gram.0 * weight;
        assert!(
            calculated_fee >= total_fee.0,
            "Calculated fee {} should be >= requested fee {}",
            calculated_fee,
            total_fee.0
        );

        // For large amounts, we shouldn't have excessive overpayment
        let overpayment = calculated_fee - total_fee.0;
        assert!(
            overpayment < weight,
            "Overpayment {} should be less than weight {} for efficiency",
            overpayment,
            weight
        );
    }

    #[test]
    fn test_calculate_fee_per_gram_edge_cases() {
        let consensus_manager = ConsensusManager::builder(Network::LocalNet).build().unwrap();
        let completed_tx = create_test_completed_transaction(1);

        // Test case 5: Fractional result that needs rounding
        let total_fee = MicroMinotari::from(999);
        let tip_height = 100; // Ordinary number, doesn't matter in this case
        let result = completed_tx
            .calculate_fee_per_gram_from_total_fee(total_fee, consensus_manager.consensus_constants(tip_height), 1, 1)
            .unwrap();
        let (weight, fee_per_gram) = result;

        // Verify the calculated fee meets or exceeds the requested fee
        let calculated_fee = fee_per_gram.0 * weight;
        assert!(
            calculated_fee >= total_fee.0,
            "Calculated fee {} should be >= requested fee {}",
            calculated_fee,
            total_fee.0
        );
    }

    #[test]
    fn test_calculate_fee_per_gram_error_cases() {
        let consensus_manager = ConsensusManager::builder(Network::LocalNet).build().unwrap();
        let completed_tx = create_test_completed_transaction(1);

        // Test case 6: Zero fee should fail
        let total_fee = MicroMinotari::zero();
        let tip_height = 100; // Ordinary number, doesn't matter in this case
        let result = completed_tx.calculate_fee_per_gram_from_total_fee(
            total_fee,
            consensus_manager.consensus_constants(tip_height),
            1,
            1,
        );
        assert!(result.is_err(), "Zero fee should result in error");

        // Test case 7: Zero inputs should fail
        let total_fee = MicroMinotari::from(100);
        let result = completed_tx.calculate_fee_per_gram_from_total_fee(
            total_fee,
            consensus_manager.consensus_constants(tip_height),
            0,
            1,
        );
        assert!(result.is_err(), "Zero inputs should result in error");

        // Test case 8: Zero outputs should fail
        let result = completed_tx.calculate_fee_per_gram_from_total_fee(
            total_fee,
            consensus_manager.consensus_constants(tip_height),
            1,
            0,
        );
        assert!(result.is_err(), "Zero outputs should result in error");
    }

    #[test]
    fn test_calculate_fee_per_gram_consistency() {
        let consensus_manager = ConsensusManager::builder(Network::LocalNet).build().unwrap();
        let completed_tx = create_test_completed_transaction(2);

        // Test case 9: Multiple calls with same parameters should return same result
        let total_fee = MicroMinotari::from(500);
        let tip_height = 100; // Ordinary number, doesn't matter in this case
        let result1 = completed_tx
            .calculate_fee_per_gram_from_total_fee(total_fee, consensus_manager.consensus_constants(tip_height), 1, 2)
            .unwrap();
        let result2 = completed_tx
            .calculate_fee_per_gram_from_total_fee(total_fee, consensus_manager.consensus_constants(tip_height), 1, 2)
            .unwrap();

        assert_eq!(result1, result2, "Multiple calls should return consistent results");
    }

    #[test]
    fn test_calculate_fee_per_gram_no_overpayment() {
        let consensus_manager = ConsensusManager::builder(Network::LocalNet).build().unwrap();
        let completed_tx = create_test_completed_transaction(1);
        let tip_height = 100; // Ordinary number, doesn't matter in this case

        // Test case 10: Verify we don't overpay by more than necessary
        for fee_amount in [1, 10, 50, 100, 250, 500, 1000, 10000] {
            let total_fee = MicroMinotari::from(fee_amount);
            let result = completed_tx
                .calculate_fee_per_gram_from_total_fee(
                    total_fee,
                    consensus_manager.consensus_constants(tip_height),
                    1,
                    1,
                )
                .unwrap();
            let (weight, fee_per_gram) = result;

            let calculated_fee = fee_per_gram.0 * weight;

            // Should meet the minimum requirement
            assert!(
                calculated_fee >= total_fee.0,
                "Calculated fee {} should be >= requested fee {} for amount {}",
                calculated_fee,
                total_fee.0,
                fee_amount
            );

            // Should not overpay by more than the weight (which represents the granularity)
            let overpayment = calculated_fee - total_fee.0;
            assert!(
                overpayment < weight,
                "Overpayment {} should be less than weight {} for fee amount {}",
                overpayment,
                weight,
                fee_amount
            );
        }
    }

    #[test]
    fn test_calculate_fee_per_gram_user_example_134_200() {
        let consensus_manager = ConsensusManager::builder(Network::LocalNet).build().unwrap();
        let completed_tx = create_test_completed_transaction(1);

        // Test the specific user example: 134 / 200 should round up to at least 1
        // and ensure final fee >= 134
        let total_fee = MicroMinotari::from(134);
        let tip_height = 100; // Ordinary number, doesn't matter in this case
        let result = completed_tx
            .calculate_fee_per_gram_from_total_fee(total_fee, consensus_manager.consensus_constants(tip_height), 1, 1)
            .unwrap();
        let (weight, fee_per_gram) = result;

        // fee_per_gram should be at least 1
        assert!(
            fee_per_gram.0 >= 1,
            "fee_per_gram should be at least 1 for the 134/200 case, got {}",
            fee_per_gram.0
        );

        // The calculated total fee should meet or exceed the requested 134
        let calculated_fee = fee_per_gram.0 * weight;
        assert!(
            calculated_fee >= 134,
            "Calculated fee {} should be >= requested fee 134 (weight: {}, fee_per_gram: {})",
            calculated_fee,
            weight,
            fee_per_gram.0
        );

        // With ceiling division, we should get exactly the right amount or slightly more
        // For 134/weight, ceiling division should give us the minimum fee_per_gram needed
        let expected_min_fee_per_gram = 134_u64.div_ceil(weight);
        assert!(
            fee_per_gram.0 >= expected_min_fee_per_gram,
            "fee_per_gram {} should be at least the ceiling division result {}",
            fee_per_gram.0,
            expected_min_fee_per_gram
        );
    }
}
