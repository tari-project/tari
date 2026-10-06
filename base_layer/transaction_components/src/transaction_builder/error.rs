// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use tari_common_types::tari_address::TariAddressError;
use tari_script::ScriptError;
use tari_utilities::ByteArrayError;
use thiserror::Error;

use crate::{MicroMinotari, key_manager::error::KeyManagerError, transaction_components::TransactionError};

#[derive(Debug, Error)]
pub enum TransactionBuilderError {
    #[error("Key manager error: `{0}`")]
    KeyManagerError(#[from] KeyManagerError),
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
    #[error("Inputs cannot be added after the sender offset keys have been reserved")]
    InputsAfterOutputs,
    #[error("`reserve_sender_offset_keys` may only be called once per transaction")]
    SenderOffsetKeysAlreadyReserved,
    #[error(
        "`reserve_sender_offset_keys` was never called, so the input script keys were never folded into the script \
         offset and the transaction would not validate"
    )]
    SenderOffsetKeysNotReserved,
    #[error("An output needed a reserved sender offset key but the pool was empty")]
    SenderOffsetKeyPoolExhausted,
    #[error(
        "{remaining} reserved sender offset key(s) were never placed on an output; every reserved key is subtracted \
         from the script offset, so the transaction would not validate"
    )]
    SenderOffsetKeyPoolNotDrained { remaining: usize },
    #[error("Recipient specs must all be declared before `reserve_sender_offset_keys` is called")]
    RecipientSpecAfterReserve,
    #[error(
        "{added} output(s) were attached after the sender offset keys were reserved but only {declared} were \
         declared; the fee and the change decision the reservation committed to never accounted for them"
    )]
    UndeclaredOutputAfterReserve { declared: usize, added: usize },
    #[error(
        "The outputs attached after the sender offset keys were reserved are worth {actual_value} across \
         {actual_weight} weighted byte(s), but the reservation was told to expect {declared_value} across \
         {declared_weight}. The fee and the change decision it committed to were computed for outputs this \
         transaction does not carry."
    )]
    PendingOutputMismatch {
        declared_value: MicroMinotari,
        actual_value: MicroMinotari,
        declared_weight: usize,
        actual_weight: usize,
    },
    #[error(
        "This transaction needs {requested} sender offset keys but the ledger device derives at most {max} in one \
         exchange, so it is limited to {max} outputs including change"
    )]
    TooManyOutputsForDevice { requested: usize, max: usize },
    #[error("Only a single burned output is allowed in a transaction")]
    MultipleBurnCommitments,
    #[error("Transaction builder error: {0}")]
    Other(String),
}

impl From<ByteArrayError> for TransactionBuilderError {
    fn from(e: ByteArrayError) -> Self {
        TransactionBuilderError::ByteArrayError(e.to_string())
    }
}
