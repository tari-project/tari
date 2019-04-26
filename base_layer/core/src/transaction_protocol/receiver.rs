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

use crate::{
    transaction::{OutputFeatures, TransactionOutput},
    transaction_protocol::{
        sender::{SenderMessage, SingleRoundSenderData},
        single_receiver::SingleReceiverTransactionProtocol,
        TransactionProtocolError,
    },
    types::{PublicKey, SecretKey, Signature},
};
use std::collections::HashMap;
use tari_crypto::challenge::MessageHash;

pub enum RecipientState {
    Finalized(RecipientSignedTransactionData),
    Failed(TransactionProtocolError),
}

/// An enum describing the types of information that a recipient can send back to the receiver
#[derive(Debug, Clone)]
pub(super) enum RecipientInfo {
    None,
    Single(Option<Box<RecipientSignedTransactionData>>),
    Multiple(HashMap<u64, MultiRecipientInfo>),
}

#[derive(Debug, Clone)]
pub(super) struct MultiRecipientInfo {
    pub commitment: MessageHash,
    pub data: RecipientSignedTransactionData,
}

/// This is the message containing the public data that the Receiver will send back to the Sender
#[derive(Clone, Debug)]
pub struct RecipientSignedTransactionData {
    pub tx_id: u64,
    pub output: TransactionOutput,
    pub public_spend_key: PublicKey,
    pub partial_signature: Signature,
}

// impl PartialEq for RecipientSignedTransactionData {
//    fn eq(&self, other: &Self) -> bool {
//        self.tx_id == other.tx_id
//            && self.partial_signature.get_signature() == other.partial_signature.get_signature()
//            && self.partial_signature.get_public_nonce() == other.partial_signature.get_public_nonce()
//    }
//}

pub struct ReceiverTransactionProtocol {
    state: RecipientState,
}

impl ReceiverTransactionProtocol {
    pub fn new(
        info: SenderMessage,
        nonce: SecretKey,
        spending_key: SecretKey,
        features: OutputFeatures,
    ) -> ReceiverTransactionProtocol
    {
        let state = match info {
            SenderMessage::None => RecipientState::Failed(TransactionProtocolError::InvalidStateError),
            SenderMessage::Single(v) => {
                ReceiverTransactionProtocol::run_single_recipient_protocol(nonce, spending_key, features, &v)
            },
            SenderMessage::Multiple => Self::run_multi_recipient_protocol(),
        };
        ReceiverTransactionProtocol { state }
    }

    pub fn is_finalized(&self) -> bool {
        match self.state {
            RecipientState::Finalized(_) => true,
            _ => false,
        }
    }

    pub fn get_signed_data(&self) -> Result<&RecipientSignedTransactionData, TransactionProtocolError> {
        match &self.state {
            RecipientState::Finalized(data) => Ok(&data),
            _ => Err(TransactionProtocolError::InvalidStateError),
        }
    }

    fn run_single_recipient_protocol(
        nonce: SecretKey,
        key: SecretKey,
        features: OutputFeatures,
        data: &SingleRoundSenderData,
    ) -> RecipientState
    {
        let signer = SingleReceiverTransactionProtocol::new(data, nonce, key, features);
        match signer {
            Ok(signed_data) => RecipientState::Finalized(signed_data),
            Err(e) => RecipientState::Failed(e),
        }
    }

    fn run_multi_recipient_protocol() -> RecipientState {
        RecipientState::Failed(TransactionProtocolError::UnsupportedError(
            "Multiple recipients aren't supported yet".into(),
        ))
    }
}
