// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Transaction Protocol Manager facilitates the process of constructing a Mimblewimble transaction between two parties.
//!
//! The Transaction Protocol Manager implements a protocol to construct a Mimwblewimble transaction between two parties
//! , a Sender and a Receiver. In this transaction the Sender is paying the Receiver from their inputs and also paying
//! to as many change outputs as they like. The Receiver will receive a single output from this transaction.
//! The module consists of three main components:
//! - A Builder for the initial Sender state data
//! - A SenderTransactionProtocolManager which manages the Sender's state machine
//! - A ReceiverTransactionProtocolManager which manages the Receiver's state machine.
//!
//! The two state machines run in parallel and will be managed by each respective party. Each state machine has methods
//! to construct and accept the public data messages that needs to be transmitted between the parties. The diagram below
//! illustrates the progression of the two state machines and shows where the public data messages are constructed and
//! accepted in each state machine
//!
//! The sequence diagram for the single receiver protocol is:
//!
//! <div class="mermaid">
//!   sequenceDiagram
//!   participant Sender
//!   participant Receiver
//! #
//!   activate Sender
//!     Sender-->>Sender: initialize transaction
//!   deactivate Sender
//! #
//!   activate Sender
//!   Sender-->>+Receiver: partial tx info
//!   Receiver-->>Receiver: validate tx info
//!   Receiver-->>Receiver: create new output and sign
//!   Receiver-->>-Sender: signed partial transaction
//!   deactivate Sender
//! #
//!   activate Sender
//!     Sender-->>Sender: validate and sign
//!   deactivate Sender
//! #
//!   alt tx is valid
//!   Sender-->>Network: Broadcast transaction
//!   else tx is invalid
//!   Sender--XSender: Failed
//!   end
//! </div>
//!
//! If there are multiple recipients, the protocol is more involved and requires three rounds of communication:
//!
//! <div class="mermaid">
//!   sequenceDiagram
//!   participant Sender
//!   participant Receivers
//! #
//!   activate Sender
//!   Sender-->>Sender: initialize
//!   deactivate Sender
//! #
//!   activate Sender
//!   Sender-->>+Receivers: [tx_id, amount_i]
//!   note left of Sender: CollectingPubKeys
//!   note right of Receivers: Initialization
//!   Receivers-->>-Sender: [tx_id, Pi, Ri]
//!   deactivate Sender
//! #
//!   alt invalid
//!   Sender--XSender: failed
//!   end
//! #
//!   activate Sender
//!   Sender-->>+Receivers: [tx_id, ΣR, ΣP]
//!   note left of Sender: CollectingSignatures
//!   note right of Receivers: Signing
//!   Receivers-->>Receivers: create output and sign
//!   Receivers-->>-Sender: [tx_id, Output_i, s_i]
//!   deactivate Sender
//! #
//!   note left of Sender: Finalizing
//!   alt is_valid()
//!   Sender-->>Sender: Finalized
//!   else invalid
//!   Sender--XSender: Failed
//!   end
//! </div>

// #![allow(clippy::op_ref)]

use derivative::Derivative;
use serde::{Deserialize, Serialize};
use tari_common_types::types::PrivateKey;
use tari_crypto::{errors::RangeProofError, hash::blake2::Blake256, signatures::SchnorrSignatureError};
use thiserror::Error;

use crate::transactions::{tari_amount::*, transaction_components::TransactionError};

pub mod proto;
pub mod recipient;
pub mod sender;
pub mod single_receiver;
pub mod transaction_initializer;
use tari_common_types::types::Commitment;
use tari_crypto::{hash_domain, hashing::DomainSeparatedHasher};
use tari_key_manager::key_manager_service::KeyManagerServiceError;

use crate::transactions::transaction_components::KernelFeatures;

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
    SigningError(#[from] SchnorrSignatureError),
    #[error("A signature verification failed: {0}")]
    InvalidSignatureError(String),
    #[error("An error occurred while building the final transaction: `{0}`")]
    TransactionBuildError(#[from] TransactionError),
    #[error("The transaction construction broke down due to communication failure")]
    TimeoutError,
    #[error("An error was produced while constructing a rangeproof: `{0}`")]
    RangeProofError(#[from] RangeProofError),
    #[error("This set of parameters is currently not supported: `{0}`")]
    UnsupportedError(String),
    #[error("There has been an error serializing or deserializing this structure")]
    SerializationError,
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
}

impl From<KeyManagerServiceError> for TransactionProtocolError {
    fn from(err: KeyManagerServiceError) -> Self {
        TransactionProtocolError::KeyManagerServiceError(err.to_string())
    }
}

/// Transaction metadata, this includes all the fields that needs to be signed on the kernel
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, Serialize)]
pub struct TransactionMetadata {
    /// The absolute fee for the transaction
    pub fee: MicroTari,
    /// The earliest block this transaction can be mined
    pub lock_height: u64,
    /// The kernel features
    pub kernel_features: KernelFeatures,
    /// optional burn commitment if present
    pub burn_commitment: Option<Commitment>,
}

impl TransactionMetadata {
    pub fn new(fee: MicroTari, lock_height: u64) -> Self {
        Self {
            fee,
            lock_height,
            kernel_features: KernelFeatures::default(),
            burn_commitment: None,
        }
    }

    pub fn new_with_features(fee: MicroTari, lock_height: u64, kernel_features: KernelFeatures) -> Self {
        Self {
            fee,
            lock_height,
            kernel_features,
            burn_commitment: None,
        }
    }
}

#[derive(Derivative, Clone)]
#[derivative(Debug)]
pub struct RecoveryData {
    pub encryption_key: PrivateKey,
}

// hash domain
hash_domain!(
    CalculateTxIdTransactionProtocolHashDomain,
    "com.tari.tari-project.base_layer.core.transactions.transaction_protocol.calculate_tx_id",
    1
);

pub type CalculateTxIdTransactionProtocolHasherBlake256 =
    DomainSeparatedHasher<Blake256, CalculateTxIdTransactionProtocolHashDomain>;
