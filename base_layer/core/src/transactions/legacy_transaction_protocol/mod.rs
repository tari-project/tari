// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

// #![allow(clippy::op_ref)]
use serde::{Deserialize, Serialize};
use tari_crypto::{errors::RangeProofError, signatures::SchnorrSignatureError};
use tari_transaction_components::transaction_components::TransactionError;
use tari_utilities::ByteArrayError;
use thiserror::Error;

use crate::transactions::{tari_amount::*, transaction_components::TransactionError};

pub mod proto;
pub mod recipient;
pub mod sender;
pub use recipient::ReceiverTransactionProtocol;
pub use sender::SenderTransactionProtocol;

use tari_transaction_components::{key_manager::error::KeyManagerServiceError, transaction_components::KernelFeatures};

#[derive(Clone, Debug, PartialEq, Error, Deserialize, Serialize)]
pub enum TransactionProtocolError {
    #[error("The current state is not yet completed, cannot transition to next state: `{0}`")]
    IncompleteStateError(String),
    #[error("Validation error: `{0}`")]
    ValidationError(String),
    #[error("Invalid state transition")]
    InvalidTransitionError,
    #[error("Invalid state")]
    InvalidStateError,
    #[error("An error occurred while performing a signature: `{0}`")]
    SigningError(String),
    #[error("A signature verification failed: {0}")]
    InvalidSignatureError(String),
    #[error("An error occurred while building the final transaction: `{0}`")]
    TransactionBuildError(#[from] TransactionError),
    #[error("The transaction construction broke down due to communication failure")]
    TimeoutError,
    #[error("An error was produced while constructing a rangeproof: `{0}`")]
    RangeProofError(String),
    #[error("This set of parameters is currently not supported: `{0}`")]
    UnsupportedError(String),
    #[error("There has been an error serializing or deserializing this structure: `{0}`")]
    SerializationError(String),
    #[error("Conversion error: `{0}`")]
    ConversionError(String),
    #[error("The script offset private key could not be found")]
    ScriptOffsetPrivateKeyNotFound,
    #[error("The minimum value promise could not be found")]
    MinimumValuePromiseNotFound,
    #[error("Value encryption failed")]
    EncryptionError,
    #[error("Key manager service error: `{0}`")]
    KeyManagerServiceError(String),
    #[error("Address exceeded maximum memo field size: `{0}`")]
    AddressExceededMaximumMemoFieldSize(String),
}

impl From<RangeProofError> for TransactionProtocolError {
    fn from(e: RangeProofError) -> Self {
        TransactionProtocolError::RangeProofError(e.to_string())
    }
}

impl From<SchnorrSignatureError> for TransactionProtocolError {
    fn from(e: SchnorrSignatureError) -> Self {
        TransactionProtocolError::SigningError(e.to_string())
    }
}

impl From<KeyManagerServiceError> for TransactionProtocolError {
    fn from(err: KeyManagerServiceError) -> Self {
        TransactionProtocolError::KeyManagerServiceError(err.to_string())
    }
}

impl From<ByteArrayError> for TransactionProtocolError {
    fn from(err: ByteArrayError) -> Self {
        TransactionProtocolError::SerializationError(err.to_string())
    }
}

/// Transaction metadata, this includes all the fields that needs to be signed on the kernel
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, Serialize)]
pub struct TransactionMetadata {
    /// The absolute fee for the transaction
    pub fee: MicroMinotari,
    /// The earliest block this transaction can be mined
    pub lock_height: u64,
    /// The kernel features
    pub kernel_features: KernelFeatures,
    /// optional burn commitment if present
    pub burn_commitment: Option<CompressedCommitment>,
}

impl TransactionMetadata {
    pub fn new(fee: MicroMinotari, lock_height: u64) -> Self {
        Self {
            fee,
            lock_height,
            kernel_features: KernelFeatures::default(),
            burn_commitment: None,
        }
    }

    pub fn new_with_features(fee: MicroMinotari, lock_height: u64, kernel_features: KernelFeatures) -> Self {
        Self {
            fee,
            lock_height,
            kernel_features,
            burn_commitment: None,
        }
    }
}
