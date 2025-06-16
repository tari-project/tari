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
    pub encrypted_kernel_nonce: Option<String>,
    pub encrypted_sender_offset_key_id: Option<String>,
    pub encrypted_output_spending_key_id: Option<String>,
    pub encrypted_output_script_key_id: Option<String>,
}

impl MarshalOutputPair {
    pub async fn marshal<KM: TransactionKeyManagerInterface>(
        key_manager: &KM,
        output_pair: OutputPair,
    ) -> Result<Self, TransactionServiceError> {
        let encrypted_kernel_nonce =
            MarshalOutputPair::encrypt_key_failsafe(key_manager, &output_pair.kernel_nonce).await;
        let encrypted_sender_offset_key_id = match &output_pair.sender_offset_key_id {
            Some(key) => MarshalOutputPair::encrypt_key_failsafe(key_manager, key).await,
            None => None,
        };
        let encrypted_output_spending_key_id =
            MarshalOutputPair::encrypt_key_failsafe(key_manager, &output_pair.output.spending_key_id).await;
        let encrypted_output_script_key_id =
            MarshalOutputPair::encrypt_key_failsafe(key_manager, &output_pair.output.script_key_id).await;

        Ok(MarshalOutputPair {
            output_pair,
            encrypted_kernel_nonce,
            encrypted_sender_offset_key_id,
            encrypted_output_spending_key_id,
            encrypted_output_script_key_id,
        })
    }

    pub async fn unmarshal<KM: TransactionKeyManagerInterface>(
        &mut self,
        key_manager: &KM,
    ) -> Result<(), TransactionServiceError> {
        if let Some(kernel_nonce) = &self.encrypted_kernel_nonce {
            self.output_pair.kernel_nonce = MarshalOutputPair::import_encrypted_key(key_manager, kernel_nonce).await?;
        }
        if let Some(sender_offset_key_id) = &self.encrypted_sender_offset_key_id {
            self.output_pair.sender_offset_key_id =
                Some(MarshalOutputPair::import_encrypted_key(key_manager, sender_offset_key_id).await?);
        }
        if let Some(spending_key_id) = &self.encrypted_output_spending_key_id {
            self.output_pair.output.spending_key_id =
                MarshalOutputPair::import_encrypted_key(key_manager, spending_key_id).await?;
        }
        if let Some(script_key_id) = &self.encrypted_output_script_key_id {
            self.output_pair.output.script_key_id =
                MarshalOutputPair::import_encrypted_key(key_manager, script_key_id).await?;
        }
        Ok(())
    }

    async fn encrypt_key_failsafe<KM: TransactionKeyManagerInterface>(
        key_manager: &KM,
        key_id: &TariKeyId,
    ) -> Option<String> {
        match key_manager.encrypted_key(key_id, None).await {
            Ok(key_id) => Some(key_id.to_hex()),
            Err(_) => None,
        }
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
