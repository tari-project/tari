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

use tari_common_types::types::{PrivateKey, PublicKey};
use tari_p2p::proto::transaction_protocol as proto;
use tari_utilities::ByteArray;

use crate::transactions::transaction_protocol::recipient::RecipientSignedMessage;

impl TryFrom<proto::RecipientSignedMessage> for RecipientSignedMessage {
    type Error = String;

    fn try_from(message: proto::RecipientSignedMessage) -> Result<Self, Self::Error> {
        let output = message
            .output
            .map(TryInto::try_into)
            .ok_or_else(|| "Transaction output not provided".to_string())??;

        let public_spend_key = PublicKey::from_canonical_bytes(&message.public_spend_key)
            .map_err(|err| format!("public_spend_key: {}", err))?;

        let partial_signature = message
            .partial_signature
            .map(TryInto::try_into)
            .ok_or_else(|| "Transaction partial signature not provided".to_string())??;
        let metadata = message
            .metadata
            .map(TryInto::try_into)
            .ok_or_else(|| "Transaction metadata not provided".to_string())??;

        let offset = PrivateKey::from_canonical_bytes(&message.offset).map_err(|err| format!("offset: {}", err))?;

        Ok(Self {
            tx_id: message.tx_id.into(),
            output,
            public_spend_key,
            partial_signature,
            tx_metadata: metadata,
            offset,
        })
    }
}

impl TryFrom<RecipientSignedMessage> for proto::RecipientSignedMessage {
    type Error = String;

    fn try_from(message: RecipientSignedMessage) -> Result<Self, Self::Error> {
        Ok(Self {
            tx_id: message.tx_id.into(),
            output: Some(message.output.try_into()?),
            public_spend_key: message.public_spend_key.to_vec(),
            partial_signature: Some(message.partial_signature.into()),
            metadata: Some(message.tx_metadata.into()),
            offset: message.offset.to_vec(),
        })
    }
}
