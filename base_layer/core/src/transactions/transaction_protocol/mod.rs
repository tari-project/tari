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
    types::{Challenge, HashOutput, MessageHash, PublicKey},
};
use derive_error::Error;
use digest::Digest;
use serde::{Deserialize, Serialize};
use tari_crypto::{
    range_proof::RangeProofError,
    signatures::SchnorrSignatureError,
    tari_utilities::byte_array::ByteArray,
};

#[derive(Clone, Debug, PartialEq, Error, Deserialize, Serialize)]
pub enum TransactionProtocolError {
    // The current state is not yet completed, cannot transition to next state
    #[error(msg_embedded, no_from, non_std)]
    IncompleteStateError(String),
    #[error(msg_embedded, no_from, non_std)]
    ValidationError(String),
    /// Invalid state transition
    InvalidTransitionError,
    /// Invalid state
    InvalidStateError,
    /// An error occurred while performing a signature
    SigningError(SchnorrSignatureError),
    /// A signature verification failed
    InvalidSignatureError,
    /// An error occurred while building the final transaction
    TransactionBuildError(TransactionError),
    /// The transaction construction broke down due to communication failure
    TimeoutError,
    /// An error was produced while constructing a rangeproof
    RangeProofError(RangeProofError),
    /// This set of parameters is currently not supported
    #[error(msg_embedded, no_from, non_std)]
    UnsupportedError(String),
    /// There has been an error serializing or deserializing this structure
    SerializationError,
}

/// Transaction metadata, including the fee and lock height
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, Serialize)]
pub struct TransactionMetadata {
    /// The absolute fee for the transaction
    pub fee: MicroTari,
    /// The earliest block this transaction can be mined
    pub lock_height: u64,
    /// This is an optional field used by committing to additional tx meta data between the two parties
    pub meta_info: Option<HashOutput>,
    /// This is an optional field and is the hash of the kernel this kernel is linked to.
    /// This field is for example for relative time-locked transactions
    pub linked_kernel: Option<HashOutput>,
}

/// Convenience function that calculates the challenge for the Schnorr signatures
pub fn build_challenge(sum_public_nonces: &PublicKey, metadata: &TransactionMetadata) -> MessageHash {
    Challenge::new()
        .chain(sum_public_nonces.as_bytes())
        .chain(&u64::from(metadata.fee).to_le_bytes())
        .chain(&metadata.lock_height.to_le_bytes())
        .chain(metadata.meta_info.as_ref().unwrap_or(&vec![0]))
        .chain(metadata.linked_kernel.as_ref().unwrap_or(&vec![0]))
        .result()
        .to_vec()
}
