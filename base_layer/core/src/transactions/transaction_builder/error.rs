// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use crate::transactions::transaction_key_manager::error::KeyManagerServiceError;

#[derive(Clone, Debug, PartialEq, Error, Deserialize, Serialize)]
pub enum TransactionBuilderError {
    #[error("Key manager error: `{0}`")]
    KeyManagerError(#[from] KeyManagerServiceError),
    #[error("No fee set for transaction")]
    FeeNotSet,
    #[error("No outputs for transaction")]
    NoRecipients,
    #[error("No inputs provided for transaction")]
    NoInputs,
    #[error("Transaction exceeds maximum inputs limit of {0}")]
    ExceedsMaxInputs(usize),
    #[error("Transaction exceeds maximum outputs limit of {0}")]
    ExceedsMaxOutputs(usize),
    TransactionAmountOverflow,
    SpendingMoreThanAvailable {
        available: MicroMinotari,
        sent: MicroMinotari,
    },
    FeeGreaterThanAmount {
        available: MicroMinotari,
        sent: MicroMinotari,
    },
}
