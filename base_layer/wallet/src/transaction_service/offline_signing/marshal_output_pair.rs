// Copyright 2025. The Tari Project
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
use serde::{Deserialize, Serialize};
use tari_core::transactions::{
    transaction_key_manager::{error::KeyManagerServiceError, TariKeyId, TransactionKeyManagerInterface},
    transaction_protocol::sender::OutputPair,
};
use tari_utilities::hex::{from_hex, Hex};

use crate::transaction_service::error::TransactionServiceError;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct MarshalOutputPair {
    pub output_pair: OutputPair,
    pub encrypted_kernel_nonce: String,
    pub encrypted_sender_offset_key: Option<String>,
    pub encrypted_output_commitment_mask: String,
}

impl MarshalOutputPair {
    pub async fn marshal<KM: TransactionKeyManagerInterface>(
        key_manager: &KM,
        output_pair: OutputPair,
    ) -> Result<Self, TransactionServiceError> {
        let encrypted_kernel_nonce = MarshalOutputPair::encrypt_key(key_manager, &output_pair.kernel_nonce).await?;
        let encrypted_sender_offset_key = match &output_pair.sender_offset_key_id {
            Some(key) => Some(MarshalOutputPair::encrypt_key(key_manager, key).await?),
            None => None,
        };
        let encrypted_output_commitment_mask =
            MarshalOutputPair::encrypt_key(key_manager, &output_pair.output.spending_key_id).await?;

        Ok(MarshalOutputPair {
            output_pair,
            encrypted_kernel_nonce,
            encrypted_sender_offset_key,
            encrypted_output_commitment_mask,
        })
    }

    pub async fn unmarshal<KM: TransactionKeyManagerInterface>(
        &mut self,
        key_manager: &KM,
    ) -> Result<(), TransactionServiceError> {
        self.output_pair.kernel_nonce =
            MarshalOutputPair::import_encrypted_key(key_manager, &self.encrypted_kernel_nonce).await?;
        if let Some(sender_offset_key_id) = &self.encrypted_sender_offset_key {
            self.output_pair.sender_offset_key_id =
                Some(MarshalOutputPair::import_encrypted_key(key_manager, sender_offset_key_id).await?);
        }
        self.output_pair.output.spending_key_id =
            MarshalOutputPair::import_encrypted_key(key_manager, &self.encrypted_output_commitment_mask).await?;
        Ok(())
    }

    async fn encrypt_key<KM: TransactionKeyManagerInterface>(
        key_manager: &KM,
        key_id: &TariKeyId,
    ) -> Result<String, KeyManagerServiceError> {
        let encrypted = key_manager.encrypted_key(key_id, None).await?;
        Ok(encrypted.to_hex())
    }

    async fn import_encrypted_key<KM: TransactionKeyManagerInterface>(
        key_manager: &KM,
        encrypted: &str,
    ) -> Result<TariKeyId, KeyManagerServiceError> {
        let encrypted_bytes =
            from_hex(encrypted).map_err(|err| KeyManagerServiceError::DecryptionFailed(err.to_string()))?;
        let key_id = key_manager.import_encrypted_key(encrypted_bytes, None).await?;
        Ok(key_id)
    }
}
