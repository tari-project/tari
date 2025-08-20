// Copyright 2019. The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::fmt;

use serde::{Deserialize, Serialize};
use tari_common_types::{
    transaction::TxId,
    types::{CompressedPublicKey, PrivateKey, Signature},
};

use crate::transactions::{
    transaction_components::TransactionOutput,
    transaction_protocol::{TransactionMetadata, TransactionProtocolError},
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum RecipientState {
    Finalized(Box<RecipientSignedMessage>),
    Failed(TransactionProtocolError),
}

impl fmt::Display for RecipientState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use RecipientState::{Failed, Finalized};
        match self {
            Finalized(signed_message) => write!(
                f,
                "Finalized({:?}, maturity = {})",
                signed_message.output.features.output_type, signed_message.output.features.maturity
            ),
            Failed(err) => write!(f, "Failed({err:?})"),
        }
    }
}

/// This is the message containing the public data that the Receiver will send back to the Sender
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecipientSignedMessage {
    pub tx_id: TxId,
    pub output: TransactionOutput,
    pub public_spend_key: CompressedPublicKey,
    pub partial_signature: Signature,
    pub tx_metadata: TransactionMetadata,
    pub offset: PrivateKey,
}

/// The generalised transaction recipient protocol. A different state transition network is followed depending on
/// whether this is a single recipient or one of many.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ReceiverTransactionProtocol {
    pub state: RecipientState,
}

impl ReceiverTransactionProtocol {
    /// Returns true if the recipient protocol is finalised, and the signature data is ready to be sent to the sender.
    pub fn is_finalized(&self) -> bool {
        matches!(self.state, RecipientState::Finalized(_))
    }

    /// Method to determine if the transaction protocol has failed
    pub fn is_failed(&self) -> bool {
        matches!(&self.state, RecipientState::Failed(_))
    }

    /// Method to return the error behind a failure, if one has occurred
    pub fn failure_reason(&self) -> Option<TransactionProtocolError> {
        match &self.state {
            RecipientState::Failed(e) => Some(e.clone()),
            _ => None,
        }
    }

    /// Retrieve the final signature data to be returned to the sender to complete the transaction.
    pub fn get_signed_data(&self) -> Result<&RecipientSignedMessage, TransactionProtocolError> {
        match &self.state {
            RecipientState::Finalized(data) => Ok(data),
            _ => Err(TransactionProtocolError::InvalidStateError),
        }
    }

    /// Create an empty SenderTransactionProtocol that can be used as a placeholder in data structures that do not
    /// require a well formed version
    pub fn new_placeholder() -> Self {
        ReceiverTransactionProtocol {
            state: RecipientState::Failed(TransactionProtocolError::IncompleteStateError(
                "This is a placeholder protocol".to_string(),
            )),
        }
    }
}
