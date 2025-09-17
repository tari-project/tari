// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use serde::{Deserialize, Serialize};
use tari_common_types::{tari_address::TariAddress, types::FixedHash};

use crate::{
    key_manager::TariKeyId,
    transaction_components::{MemoField, Transaction, WalletOutput},
    MicroMinotari,
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
}

impl OutputPair {
    pub fn new(output: WalletOutput, kernel_nonce: TariKeyId, sender_offset_key_id: Option<TariKeyId>) -> Self {
        Self {
            output,
            kernel_nonce,
            sender_offset_key_id,
        }
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
