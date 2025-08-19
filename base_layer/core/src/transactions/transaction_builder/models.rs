// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use tari_common_types::{tari_address::TariAddress, types::FixedHash};

use crate::transactions::{
    tari_amount::MicroMinotari,
    transaction_components::{
        memo_field::MemoField,
        Transaction,
        TransactionError,
        TransactionInput,
        TransactionOutput,
        WalletOutput,
    },
    transaction_key_manager::{TariKeyId, TransactionKeyManagerInterface},
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RecipientDetails {
    pub output: OutputPair,
    pub recipient_address: TariAddress,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct OutputPair {
    pub output: WalletOutput,
    pub kernel_nonce: TariKeyId,
    pub sender_offset_key_id: TariKeyId,
    tx_input: OnceLock<TransactionInput>,
    tx_output: OnceLock<TransactionOutput>,
}

impl OutputPair {
    pub fn new(output: WalletOutput, kernel_nonce: TariKeyId, sender_offset_key_id: Option<TariKeyId>) -> Self {
        Self {
            output,
            kernel_nonce,
            sender_offset_key_id,
            tx_input: OnceLock::new(),
            tx_output: OnceLock::new(),
        }
    }

    pub async fn tx_input<KM: TransactionKeyManagerInterface>(
        &self,
        key_manager: &KM,
    ) -> Result<TransactionInput, TransactionError> {
        match self.tx_input.get() {
            Some(input) => Ok(input.clone()),
            None => Ok(self.output.to_transaction_input(key_manager).await?),
        }
    }

    pub async fn tx_output<KM: TransactionKeyManagerInterface>(
        &self,
        key_manager: &KM,
    ) -> Result<TransactionOutput, TransactionError> {
        match self.tx_output.get() {
            Some(output) => Ok(output.clone()),
            None => Ok(self.output.to_transaction_output(key_manager).await?),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct FinalizedTransaction {
    pub source_address: TariAddress,
    pub destination_addresses: Vec<TariAddress>,
    pub amount: MicroMinotari,
    pub fee: MicroMinotari,
    pub transaction: Transaction,
    pub payment_id: MemoField,
    /// Hashes of outputs being sent to others (excluding change)
    pub sent_output_hashes: Vec<FixedHash>,
    /// Hashes of outputs received from others (excluding change)
    pub received_output_hashes: Vec<FixedHash>,
    /// Hashes of change outputs (for reference)
    pub change_output_hashes: Vec<FixedHash>,
}
