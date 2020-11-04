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

pub mod proto;
pub mod recipient;
pub mod sender;
pub mod single_receiver;
pub mod transaction_initializer;

use crate::transactions::{
    tari_amount::*,
    transaction::TransactionError,
    types::{Challenge, MessageHash, PublicKey},
};
use digest::Digest;
use serde::{Deserialize, Serialize};
use tari_crypto::{
    range_proof::RangeProofError,
    signatures::SchnorrSignatureError,
    tari_utilities::byte_array::ByteArray,
};
use thiserror::Error;

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
    #[error("A signature verification failed")]
    InvalidSignatureError,
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
}

/// Transaction metadata, including the fee and lock height
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, Serialize)]
pub struct TransactionMetadata {
    /// The absolute fee for the transaction
    pub fee: MicroTari,
    /// The earliest block this transaction can be mined
    pub lock_height: u64,
}

/// Convenience function that calculates the challenge for the Schnorr signatures
pub fn build_challenge(sum_public_nonces: &PublicKey, metadata: &TransactionMetadata) -> MessageHash {
    Challenge::new()
        .chain(sum_public_nonces.as_bytes())
        .chain(&u64::from(metadata.fee).to_le_bytes())
        .chain(&metadata.lock_height.to_le_bytes())
        .result()
        .to_vec()
}
