// Copyright 2019, The Tari Project
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

use std::convert::{TryFrom, TryInto};

use proto::transaction_sender_message::Message as ProtoTxnSenderMessage;
use tari_common_types::types::PublicKey;
use tari_script::TariScript;
use tari_utilities::ByteArray;

use super::{protocol as proto, protocol::transaction_sender_message::Message as ProtoTransactionSenderMessage};
use crate::{
    consensus::{ConsensusDecoding, ToConsensusBytes},
    covenants::Covenant,
    transactions::transaction_protocol::sender::{SingleRoundSenderData, TransactionSenderMessage},
};

impl proto::TransactionSenderMessage {
    pub fn none() -> Self {
        proto::TransactionSenderMessage {
            message: Some(ProtoTxnSenderMessage::None(true)),
        }
    }

    pub fn single(data: proto::SingleRoundSenderData) -> Self {
        proto::TransactionSenderMessage {
            message: Some(ProtoTxnSenderMessage::Single(data)),
        }
    }

    pub fn multiple() -> Self {
        proto::TransactionSenderMessage {
            message: Some(ProtoTxnSenderMessage::Multiple(true)),
        }
    }
}

impl TryFrom<proto::TransactionSenderMessage> for TransactionSenderMessage {
    type Error = String;

    fn try_from(message: proto::TransactionSenderMessage) -> Result<Self, Self::Error> {
        let inner_message = message
            .message
            .ok_or_else(|| "TransactionSenderMessage.message not provided".to_string())?;

        let sender_message = match inner_message {
            ProtoTxnSenderMessage::None(_) => TransactionSenderMessage::None,
            ProtoTxnSenderMessage::Single(data) => TransactionSenderMessage::Single(Box::new(data.try_into()?)),
            ProtoTxnSenderMessage::Multiple(_) => TransactionSenderMessage::Multiple,
        };

        Ok(sender_message)
    }
}

impl From<TransactionSenderMessage> for proto::TransactionSenderMessage {
    fn from(message: TransactionSenderMessage) -> Self {
        let message = match message {
            TransactionSenderMessage::None => ProtoTransactionSenderMessage::None(true),
            TransactionSenderMessage::Single(sender_data) => {
                ProtoTransactionSenderMessage::Single((*sender_data).into())
            },
            TransactionSenderMessage::Multiple => ProtoTransactionSenderMessage::Multiple(true),
        };

        Self { message: Some(message) }
    }
}

//---------------------------------- SingleRoundSenderData --------------------------------------------//

impl TryFrom<proto::SingleRoundSenderData> for SingleRoundSenderData {
    type Error = String;

    fn try_from(data: proto::SingleRoundSenderData) -> Result<Self, Self::Error> {
        let public_excess = PublicKey::from_bytes(&data.public_excess).map_err(|err| err.to_string())?;
        let public_nonce = PublicKey::from_bytes(&data.public_nonce).map_err(|err| err.to_string())?;
        let sender_offset_public_key =
            PublicKey::from_bytes(&data.sender_offset_public_key).map_err(|err| err.to_string())?;
        let metadata = data
            .metadata
            .map(TryInto::try_into)
            .ok_or_else(|| "Transaction metadata not provided".to_string())??;
        let message = data.message;
        let public_commitment_nonce =
            PublicKey::from_bytes(&data.public_commitment_nonce).map_err(|err| err.to_string())?;
        let features = data
            .features
            .map(TryInto::try_into)
            .ok_or_else(|| "Transaction output features not provided".to_string())??;
        let covenant = Covenant::consensus_decode(&mut data.covenant.as_slice()).map_err(|err| err.to_string())?;

        Ok(Self {
            tx_id: data.tx_id.into(),
            amount: data.amount.into(),
            public_excess,
            public_nonce,
            metadata,
            message,
            features,
            script: TariScript::from_bytes(&data.script).map_err(|err| err.to_string())?,
            sender_offset_public_key,
            public_commitment_nonce,
            covenant,
            minimum_value_promise: data.minimum_value_promise.into(),
        })
    }
}

impl From<SingleRoundSenderData> for proto::SingleRoundSenderData {
    fn from(sender_data: SingleRoundSenderData) -> Self {
        Self {
            tx_id: sender_data.tx_id.into(),
            // The amount, in µT, being sent to the recipient
            amount: sender_data.amount.into(),
            // The offset public excess for this transaction
            public_excess: sender_data.public_excess.to_vec(),
            public_nonce: sender_data.public_nonce.to_vec(),
            metadata: Some(sender_data.metadata.into()),
            message: sender_data.message,
            features: Some(sender_data.features.into()),
            script: sender_data.script.to_bytes(),
            sender_offset_public_key: sender_data.sender_offset_public_key.to_vec(),
            public_commitment_nonce: sender_data.public_commitment_nonce.to_vec(),
            covenant: sender_data.covenant.to_consensus_bytes(),
            minimum_value_promise: sender_data.minimum_value_promise.into(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_from_none() {
        let tsm = TransactionSenderMessage::None;
        let ptsm = proto::TransactionSenderMessage::from(tsm);
        assert_eq!(ptsm.message, proto::TransactionSenderMessage::none().message);
    }

    #[test]
    fn test_from_multiple() {
        let tsm = TransactionSenderMessage::Multiple;
        let ptsm = proto::TransactionSenderMessage::from(tsm);
        assert_eq!(ptsm.message, proto::TransactionSenderMessage::multiple().message);
    }
}
