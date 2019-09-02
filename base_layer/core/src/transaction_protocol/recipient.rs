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
        sender::{SingleRoundSenderData as SD, TransactionSenderMessage},
        single_receiver::SingleReceiverTransactionProtocol,
        TransactionProtocolError,
    },
    types::{CommitmentFactory, MessageHash, PrivateKey, PublicKey, RangeProofService, Signature},
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, convert::TryInto};
use tari_comms::message::{Message, MessageError};
use tari_p2p::tari_message::{BlockchainMessage, TariMessageType};

#[derive(Clone, Debug)]
pub enum RecipientState {
    Finalized(Box<RecipientSignedMessage>),
    Failed(TransactionProtocolError),
}

/// An enum describing the types of information that a recipient can send back to the receiver
#[derive(Debug, Clone)]
pub(super) enum RecipientInfo {
    None,
    Single(Option<Box<RecipientSignedMessage>>),
    Multiple(HashMap<u64, MultiRecipientInfo>),
}

#[derive(Debug, Clone)]
pub(super) struct MultiRecipientInfo {
    pub commitment: MessageHash,
    pub data: RecipientSignedMessage,
}

/// This is the message containing the public data that the Receiver will send back to the Sender
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RecipientSignedMessage {
    pub tx_id: u64,
    pub output: TransactionOutput,
    pub public_spend_key: PublicKey,
    pub partial_signature: Signature,
}

/// Convert `RecipientSignedMessage` into a Tari Message that can be sent via the tari comms stack
impl TryInto<Message> for RecipientSignedMessage {
    type Error = MessageError;

    fn try_into(self) -> Result<Message, Self::Error> {
        Ok((TariMessageType::new(BlockchainMessage::TransactionReply), self).try_into()?)
    }
}

/// The generalised transaction recipient protocol. A different state transition network is followed depending on
/// whether this is a single recipient or one of many.
#[derive(Clone, Debug)]
pub struct ReceiverTransactionProtocol {
    pub state: RecipientState,
}

/// Initiate a new recipient protocol state.
///
/// It takes as input the transaction message from the sender (which will indicate how many rounds the transaction
/// protocol will undergo, the recipient's nonce and spend key, as well as the output features for this recipient's
/// transaction output.
///
/// The function returns the protocol in the relevant state. If this is a single-round protocol, the state will
/// already be finalised, and the return message will be accessible from the `get_signed_data` method.
impl ReceiverTransactionProtocol {
    pub fn new(
        info: TransactionSenderMessage,
        nonce: PrivateKey,
        spending_key: PrivateKey,
        features: OutputFeatures,
        prover: &RangeProofService,
        factory: &CommitmentFactory,
    ) -> ReceiverTransactionProtocol
    {
        let state = match info {
            TransactionSenderMessage::None => RecipientState::Failed(TransactionProtocolError::InvalidStateError),
            TransactionSenderMessage::Single(v) => {
                ReceiverTransactionProtocol::single_round(nonce, spending_key, features, &v, prover, factory)
            },
            TransactionSenderMessage::Multiple => Self::multi_round(),
        };
        ReceiverTransactionProtocol { state }
    }

    /// Returns true if the recipient protocol is finalised, and the signature data is ready to be sent to the sender.
    pub fn is_finalized(&self) -> bool {
        match self.state {
            RecipientState::Finalized(_) => true,
            _ => false,
        }
    }

    /// Method to determine if the transaction protocol has failed
    pub fn is_failed(&self) -> bool {
        match &self.state {
            RecipientState::Failed(_) => true,
            _ => false,
        }
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

    /// Run the single-round recipient protocol, which can immediately construct an output and sign the data
    fn single_round(
        nonce: PrivateKey,
        key: PrivateKey,
        features: OutputFeatures,
        data: &SD,
        prover: &RangeProofService,
        factory: &CommitmentFactory,
    ) -> RecipientState
    {
        let signer = SingleReceiverTransactionProtocol::create(data, nonce, key, features, prover, factory);
        match signer {
            Ok(signed_data) => RecipientState::Finalized(Box::new(signed_data)),
            Err(e) => RecipientState::Failed(e),
        }
    }

    fn multi_round() -> RecipientState {
        RecipientState::Failed(TransactionProtocolError::UnsupportedError(
            "Multiple recipients aren't supported yet".into(),
        ))
    }
}

#[cfg(test)]
mod test {
    use crate::{
        tari_amount::*,
        transaction::OutputFeatures,
        transaction_protocol::{
            build_challenge,
            sender::{SingleRoundSenderData, TransactionSenderMessage},
            test_common::TestParams,
            TransactionMetadata,
        },
        types::{PublicKey, Signature, COMMITMENT_FACTORY, PROVER},
        ReceiverTransactionProtocol,
    };
    use rand::OsRng;
    use tari_crypto::{commitment::HomomorphicCommitmentFactory, keys::PublicKey as PK};

    #[test]
    fn single_round_recipient() {
        let mut rng = OsRng::new().unwrap();
        let p = TestParams::new(&mut rng);
        let m = TransactionMetadata {
            fee: MicroTari(125),
            lock_height: 0,
        };
        let msg = SingleRoundSenderData {
            tx_id: 15,
            amount: MicroTari(500),
            public_excess: PublicKey::from_secret_key(&p.spend_key), // any random key will do
            public_nonce: PublicKey::from_secret_key(&p.change_key), // any random key will do
            metadata: m.clone(),
        };
        let sender_info = TransactionSenderMessage::Single(Box::new(msg.clone()));
        let pubkey = PublicKey::from_secret_key(&p.spend_key);
        let receiver = ReceiverTransactionProtocol::new(
            sender_info,
            p.nonce.clone(),
            p.spend_key.clone(),
            OutputFeatures::default(),
            &PROVER,
            &COMMITMENT_FACTORY,
        );
        assert!(receiver.is_finalized());
        let data = receiver.get_signed_data().unwrap();
        assert_eq!(data.tx_id, 15);
        assert_eq!(data.public_spend_key, pubkey);
        assert!(COMMITMENT_FACTORY.open_value(&p.spend_key, 500, &data.output.commitment));
        assert!(data.output.verify_range_proof(&PROVER).unwrap());
        let r_sum = &msg.public_nonce + &p.public_nonce;
        let e = build_challenge(&r_sum, &m);
        let s = Signature::sign(p.spend_key.clone(), p.nonce.clone(), &e).unwrap();
        assert_eq!(data.partial_signature, s);
    }
}
