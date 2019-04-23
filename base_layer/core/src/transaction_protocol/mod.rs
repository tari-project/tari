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
//!#
//!   activate Sender
//!   Sender-->>Sender: initialize
//!   deactivate Sender
//!#
//!   activate Sender
//!   Sender-->>+Receivers: [tx_id, amount_i]
//!   note left of Sender: CollectingPubKeys
//!   note right of Receivers: Initialization
//!   Receivers-->>-Sender: [tx_id, Pi, Ri]
//!   deactivate Sender
//!#
//!   alt invalid
//!   Sender--XSender: failed
//!   end
//!#
//!   activate Sender
//!   Sender-->>+Receivers: [tx_id, ΣR, ΣP]
//!   note left of Sender: CollectingSignatures
//!   note right of Receivers: Signing
//!   Receivers-->>Receivers: create output and sign
//!   Receivers-->>-Sender: [tx_id, Output_i, s_i]
//!   deactivate Sender
//!#
//!   note left of Sender: Finalizing
//!   alt is_valid()
//!   Sender-->>Sender: Finalized
//!   else invalid
//!   Sender--XSender: Failed
//!   end
//! </div>

pub mod receiver;
pub mod sender;
pub mod single_receiver;
pub mod transaction_initializer;
#[cfg(test)]
pub mod test_common;

use crate::{
    transaction::TransactionError,
    types::{PublicKey, SignatureHash},
};
use derive_error::Error;
use tari_crypto::{challenge::Challenge, signatures::SchnorrSignatureError};
use tari_utilities::byte_array::ByteArray;

#[derive(Clone, Debug, PartialEq, Error)]
pub enum TransactionProtocolError {
    // The current state is not yet completed, cannot transition to next state
    #[error(msg_embedded, no_from, non_std)]
    IncompleteStateError(String),
    #[error(msg_embedded, no_from, non_std)]
    ValidationError(String),
    // Invalid state transition
    InvalidTransitionError,
    // Invalid state
    InvalidStateError,
    // An error occurred while performing a signature
    SigningError(SchnorrSignatureError),
    // A signature verification failed
    InvalidSignatureError,
    // An error occurred while building the final transaction
    TransactionBuildError(TransactionError),
    // The transaction construction broke down due to communication failure
    TimeoutError,
    // This set of parameters is currently not supported
    #[error(msg_embedded, no_from, non_std)]
    UnsupportedError(String),
}

/// Transaction metadata, including the fee and lock height
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TransactionMetadata {
    /// The absolute fee for the transaction
    fee: u64,
    /// The earliest block this transaction can be mined
    lock_height: u64,
}

/// Convenience function that calculates the challenge for the Schnorr signatures
pub fn build_challenge(
    sum_public_nonces: &PublicKey,
    sum_public_keys: &PublicKey,
    metadata: &TransactionMetadata,
) -> Challenge<SignatureHash>
{
    Challenge::<SignatureHash>::new()
        .concat(sum_public_nonces.as_bytes())
        .concat(sum_public_keys.as_bytes())
        .concat(&metadata.fee.to_le_bytes())
        .concat(&metadata.lock_height.to_le_bytes())
}
