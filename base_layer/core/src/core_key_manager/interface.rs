// Copyright 2023 The Tari Project
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

use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;
use tari_common_types::types::{ComAndPubSignature, Commitment, PrivateKey, PublicKey, RangeProof, Signature};
use tari_key_manager::key_manager_service::{KeyManagerInterface, KeyManagerServiceError};
use tari_utilities::hex::Hex;

use crate::transactions::transaction_components::{
    EncryptedData,
    TransactionError,
    TransactionInputVersion,
    TransactionKernelVersion,
    TransactionOutputVersion,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum KeyId {
    Default { branch: String, index: u64 },
    Imported { key: PublicKey },
}

impl fmt::Display for KeyId {
    // This trait requires `fmt` with this exact signature.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            KeyId::Default { branch: b, index: i } => write!(f, "default.{}.{}", b, i),
            KeyId::Imported { key: public_key } => write!(f, "imported.{}", public_key.to_hex()),
        }
    }
}
impl Default for KeyId {
    fn default() -> Self {
        KeyId::Default {
            branch: "".to_string(),
            index: 0,
        }
    }
}

impl FromStr for KeyId {
    type Err = String;

    fn from_str(id: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = id.split('.').collect();
        match parts.first() {
            None => Err("Out of bounds".to_string()),
            Some(val) => match *val {
                "default" => {
                    if parts.len() != 3 {
                        return Err("Wrong format".to_string());
                    }
                    let index = parts[2]
                        .parse()
                        .map_err(|_| "Index for default, invalid u64".to_string())?;
                    Ok(KeyId::Default {
                        branch: parts[1].into(),
                        index,
                    })
                },
                "imported" => {
                    if parts.len() != 2 {
                        return Err("Wrong format".to_string());
                    }
                    let key = PublicKey::from_hex(parts[1]).map_err(|_| "Invalid public key".to_string())?;
                    Ok(KeyId::Imported { key })
                },
                _ => Err("Wrong format".to_string()),
            },
        }
    }
}

#[derive(Clone, Copy, EnumIter)]
pub enum CoreKeyManagerBranch {
    DataEncryption,
    Coinbase,
    CommitmentMask,
}

impl CoreKeyManagerBranch {
    /// Warning: Changing these strings will affect the backwards compatibility of the wallet with older databases or
    /// recovery.
    pub fn get_branch_key(self) -> String {
        match self {
            CoreKeyManagerBranch::DataEncryption => "data encryption".to_string(),
            CoreKeyManagerBranch::Coinbase => "coinbase".to_string(),
            CoreKeyManagerBranch::CommitmentMask => "commitment mask".to_string(),
        }
    }
}

#[async_trait::async_trait]
pub trait BaseLayerKeyManagerInterface: KeyManagerInterface<PublicKey> {
    /// Gets the pedersen commitment for the specified index
    async fn get_commitment(
        &self,
        spend_key_id: &KeyId,
        value: &PrivateKey,
    ) -> Result<Commitment, KeyManagerServiceError>;

    async fn construct_range_proof(
        &self,
        spend_key_id: &KeyId,
        value: u64,
        min_value: u64,
    ) -> Result<RangeProof, TransactionError>;

    async fn get_script_signature(
        &self,
        script_key_id: &KeyId,
        spend_key_id: &KeyId,
        value: &PrivateKey,
        tx_version: &TransactionInputVersion,
        script_message: &[u8; 32],
    ) -> Result<ComAndPubSignature, TransactionError>;

    async fn get_partial_kernel_signature(
        &self,
        spend_key_id: &KeyId,
        total_nonce: &PublicKey,
        total_excess: &PublicKey,
        kernel_version: &TransactionKernelVersion,
        kernel_message: &[u8; 32],
    ) -> Result<Signature, TransactionError>;

    async fn get_kernel_signature_nonce(&self, spend_key_id: &KeyId) -> Result<PublicKey, TransactionError>;

    async fn encrypt_data_for_recovery(
        &self,
        spend_key_id: &KeyId,
        value: u64,
    ) -> Result<EncryptedData, TransactionError>;

    async fn try_commitment_key_recovery(
        &self,
        commitment: &Commitment,
        data: &EncryptedData,
    ) -> Result<(KeyId, u64), TransactionError>;

    async fn get_sender_offset_public_key(&self, script_key_id: &KeyId) -> Result<PublicKey, TransactionError>;

    async fn get_metadata_signature_ephemeral_commitment(
        &self,
        spend_key_id: &KeyId,
    ) -> Result<Commitment, TransactionError>;

    async fn get_metadata_signature_ephemeral_public_key(
        &self,
        spend_key_id: &KeyId,
    ) -> Result<PublicKey, TransactionError>;

    async fn get_receiver_partial_metadata_signature(
        &self,
        spend_key_id: &KeyId,
        value: &PrivateKey,
        sender_offset_public_key: &PublicKey,
        ephemeral_pubkey: &PublicKey,
        tx_version: &TransactionOutputVersion,
        metadata_signature_message: &[u8; 32],
    ) -> Result<ComAndPubSignature, TransactionError>;

    async fn get_sender_partial_metadata_signature(
        &self,
        script_key_id: &KeyId,
        commitment: &Commitment,
        ephemeral_commitment: &Commitment,
        tx_version: &TransactionOutputVersion,
        metadata_signature_message: &[u8; 32],
    ) -> Result<ComAndPubSignature, TransactionError>;
}
