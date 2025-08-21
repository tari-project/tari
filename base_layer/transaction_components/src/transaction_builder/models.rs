// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use tari_common_types::{tari_address::TariAddress, types::FixedHash};

use crate::{
    key_manager::{TariKeyId, TransactionKeyManagerInterface},
    tari_amount::MicroMinotari,
    transaction_components::{
        memo_field::MemoField,
        Transaction,
        TransactionError,
        TransactionInput,
        TransactionOutput,
        WalletOutput,
    },
};

#[derive(Clone, Debug, PartialEq)]
pub struct RecipientDetails {
    pub output: OutputPair,
    pub recipient_address: TariAddress,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OutputPair {
    pub output: WalletOutput,
    pub kernel_nonce: TariKeyId,
    pub sender_offset_key_id: Option<TariKeyId>,
    #[serde(skip)]
    tx_input: OnceLock<TransactionInput>,
    #[serde(skip)]
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
        if let Some(input) = self.tx_input.get() {
            return Ok(input.clone());
        }
        let input = self.output.to_transaction_input(key_manager).await?;
        let _unused = self.tx_input.set(input.clone());
        Ok(input)
    }

    pub async fn tx_output<KM: TransactionKeyManagerInterface>(
        &self,
        key_manager: &KM,
    ) -> Result<TransactionOutput, TransactionError> {
        if let Some(output) = self.tx_output.get() {
            return Ok(output.clone());
        }
        let output = self.output.to_transaction_output(key_manager).await?;
        let _unused = self.tx_output.set(output.clone());
        Ok(output)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FinalizedTransaction {
    pub source_address: TariAddress,
    pub destination_addresses: Vec<TariAddress>,
    pub amount: MicroMinotari,
    pub fee: MicroMinotari,
    pub transaction: Transaction,
    pub payment_id: MemoField,
    pub change: Option<WalletOutput>,
    pub sent_outputs: Vec<OutputPair>,
    /// Hashes of outputs being sent to others (excluding change)
    pub sent_output_hashes: Vec<FixedHash>,
    /// Hashes of outputs received from others (excluding change)
    pub received_output_hashes: Vec<FixedHash>,
    /// Hashes of change outputs (for reference)
    pub change_output_hashes: Vec<FixedHash>,
}
