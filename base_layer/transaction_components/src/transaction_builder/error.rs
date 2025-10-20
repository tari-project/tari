// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use tari_common_types::tari_address::TariAddressError;
use tari_script::ScriptError;
use tari_utilities::ByteArrayError;
use thiserror::Error;

use crate::{legacy_key_manager::error::KeyManagerServiceError, transaction_components::TransactionError, MicroMinotari};

#[derive(Debug, Error)]
pub enum TransactionBuilderError {
    #[error("Key manager error: `{0}`")]
    KeyManagerError(#[from] KeyManagerServiceError),
    #[error("Tari Address error: `{0}`")]
    TariAddressError(#[from] TariAddressError),
    #[error("No fee set for transaction")]
    FeeNotSet,
    #[error("No outputs for transaction")]
    NoRecipients,
    #[error("Invalid address, address does not contain a view key")]
    InvalidAddressNoViewKey,
    #[error("No inputs provided for transaction")]
    NoInputs,
    #[error("Transaction exceeds maximum inputs limit of {0}")]
    ExceedsMaxInputs(usize),
    #[error("Transaction exceeds maximum outputs limit of {0}")]
    ExceedsMaxOutputs(usize),
    #[error("Transaction amount overflows u64")]
    TransactionAmountOverflow,
    #[error("Spending ({sent}) more than available ({available})")]
    SpendingMoreThanAvailable {
        available: MicroMinotari,
        sent: MicroMinotari,
    },
    #[error("Fee ({fee}) is greater than the amount sent ({sent})")]
    FeeGreaterThanAmount { fee: MicroMinotari, sent: MicroMinotari },
    #[error("Invalid serialized size: {0}")]
    InvalidSerializedSize(String),
    #[error("{0}")]
    InvalidMemo(String),
    #[error("Invalid script: {0}")]
    InvalidScript(#[from] ScriptError),
    #[error("Transaction error: {0}")]
    TransactionError(#[from] TransactionError),
    #[error("ByteArrayError error: {0}")]
    ByteArrayError(String),
    #[error("Sender offset key ID is missing")]
    SenderOffsetKeyIdMissing,
    #[error("Only a single burned output is allowed in a transaction")]
    MultipleBurnCommitments,
}

impl From<ByteArrayError> for TransactionBuilderError {
    fn from(e: ByteArrayError) -> Self {
        TransactionBuilderError::ByteArrayError(e.to_string())
    }
}
