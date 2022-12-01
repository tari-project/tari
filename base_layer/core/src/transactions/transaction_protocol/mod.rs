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
//! See [single_receiver::SingleReceiverTransactionProtocol] for more detail.
//!
//! Example use:
//! ```
//! use tari_core::{
//!     test_helpers::create_consensus_constants,
//!     transactions::{
//!         test_helpers::TestParams,
//!         transaction_protocol::single_receiver::SingleReceiverTransactionProtocol,
//!         CryptoFactories,
//!         SenderTransactionProtocol,
//!     },
//! };
//!
//! let alice_secrets = TestParams::new();
//! let bob_secrets = TestParams::new();
//!
//! let builder = SenderTransactionProtocol::builder(1, create_consensus_constants(0));
//! // ... set builder options
//! let mut alice = builder.build(&CryptoFactories::default(), None, u64::MAX).unwrap();
//! let msg = alice.build_single_round_message().unwrap();
//! let mut bob_info = SingleReceiverTransactionProtocol::create(&msg, b.nonce, b.spend_key, &factories, None).unwrap();
//! alice.add_single_recipient_info(bob_info.clone()).unwrap();
//! alice.finalize(&factories, None, u64::MAX).unwrap();
//! ```

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
pub struct RewindData {
    #[derivative(Debug = "ignore")]
    pub rewind_blinding_key: PrivateKey,
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
