// Copyright 2018 The Tari Project
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
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE
//
// Portions of this file were originally copyrighted (c) 2018 The Grin Developers, issued under the Apache License,
// Version 2.0, available at http://www.apache.org/licenses/LICENSE-2.0.

use std::{
    cmp::Ordering,
    fmt::{Debug, Formatter},
};

use serde::{Deserialize, Serialize};
use tari_common_types::types::{ComAndPubSignature, Commitment, FixedHash, PublicKey, RangeProof};
use tari_script::{ExecutionStack, TariScript};

use super::TransactionOutputVersion;
use crate::{
    borsh::SerializedSize,
    covenants::Covenant,
    transactions::{
        key_manager::{TariKeyId, TransactionKeyManagerInterface},
        tari_amount::MicroMinotari,
        transaction_components,
        transaction_components::{
            transaction_input::{SpentOutput, TransactionInput},
            transaction_output::TransactionOutput,
            EncryptedData,
            OutputFeatures,
            OutputType,
            RangeProofType,
            TransactionError,
            TransactionInputVersion,
        },
    },
};

/// A wallet output is one where the value and spending key (blinding factor) are known. This can be used to
/// build both inputs and outputs (every input comes from an output)
#[derive(Clone, Serialize, Deserialize)]
pub struct WalletOutput {
    pub version: TransactionOutputVersion,
    pub value: MicroMinotari,
    pub spending_key_id: TariKeyId,
    pub features: OutputFeatures,
    pub script: TariScript,
    pub covenant: Covenant,
    pub input_data: ExecutionStack,
    pub script_key_id: TariKeyId,
    pub sender_offset_public_key: PublicKey,
    pub metadata_signature: ComAndPubSignature,
    pub script_lock_height: u64,
    pub encrypted_data: EncryptedData,
    pub minimum_value_promise: MicroMinotari,
    pub rangeproof: Option<RangeProof>,
}

impl WalletOutput {
    /// Creates a new wallet output

    #[allow(clippy::too_many_arguments)]
    pub async fn new<KM: TransactionKeyManagerInterface>(
        version: TransactionOutputVersion,
        value: MicroMinotari,
        spending_key_id: TariKeyId,
        features: OutputFeatures,
        script: TariScript,
        input_data: ExecutionStack,
        script_key_id: TariKeyId,
        sender_offset_public_key: PublicKey,
        metadata_signature: ComAndPubSignature,
        script_lock_height: u64,
        covenant: Covenant,
        encrypted_data: EncryptedData,
        minimum_value_promise: MicroMinotari,
        key_manager: &KM,
    ) -> Result<Self, TransactionError> {
        let rangeproof = if features.range_proof_type == RangeProofType::BulletProofPlus {
            Some(
                key_manager
                    .construct_range_proof(&spending_key_id, value.into(), minimum_value_promise.into())
                    .await?,
            )
        } else {
            None
        };
        Ok(Self {
            version,
            value,
            spending_key_id,
            features,
            script,
            input_data,
            script_key_id,
            sender_offset_public_key,
            metadata_signature,
            script_lock_height,
            covenant,
            encrypted_data,
            minimum_value_promise,
            rangeproof,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_rangeproof(
        version: TransactionOutputVersion,
        value: MicroMinotari,
        spending_key_id: TariKeyId,
        features: OutputFeatures,
        script: TariScript,
        input_data: ExecutionStack,
        script_key_id: TariKeyId,
        sender_offset_public_key: PublicKey,
        metadata_signature: ComAndPubSignature,
        script_lock_height: u64,
        covenant: Covenant,
        encrypted_data: EncryptedData,
        minimum_value_promise: MicroMinotari,
        rangeproof: Option<RangeProof>,
    ) -> Self {
        Self {
            version,
            value,
            spending_key_id,
            features,
            script,
            input_data,
            script_key_id,
            sender_offset_public_key,
            metadata_signature,
            script_lock_height,
            covenant,
            encrypted_data,
            minimum_value_promise,
            rangeproof,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn new_current_version<KM: TransactionKeyManagerInterface>(
        value: MicroMinotari,
        spending_key_id: TariKeyId,
        features: OutputFeatures,
        script: TariScript,
        input_data: ExecutionStack,
        script_key_id: TariKeyId,
        sender_offset_public_key: PublicKey,
        metadata_signature: ComAndPubSignature,
        script_lock_height: u64,
        covenant: Covenant,
        encrypted_data: EncryptedData,
        minimum_value_promise: MicroMinotari,
        key_manager: &KM,
    ) -> Result<Self, TransactionError> {
        Self::new(
            TransactionOutputVersion::get_current_version(),
            value,
            spending_key_id,
            features,
            script,
            input_data,
            script_key_id,
            sender_offset_public_key,
            metadata_signature,
            script_lock_height,
            covenant,
            encrypted_data,
            minimum_value_promise,
            key_manager,
        )
        .await
    }

    /// Commits an KeyManagerOutput into a Transaction input
    pub async fn to_transaction_input<KM: TransactionKeyManagerInterface>(
        &self,
        key_manager: &KM,
    ) -> Result<TransactionInput, TransactionError> {
        let value = self.value.into();
        let commitment = key_manager.get_commitment(&self.spending_key_id, &value).await?;
        let rangeproof_hash = match &self.rangeproof {
            Some(rp) => rp.hash(),
            None => FixedHash::zero(),
        };
        let version = TransactionInputVersion::get_current_version();
        let script_message = TransactionInput::build_script_signature_message(&version, &self.script, &self.input_data);
        let script_signature = key_manager
            .get_script_signature(
                &self.script_key_id,
                &self.spending_key_id,
                &value,
                &version,
                &script_message,
            )
            .await?;

        Ok(TransactionInput::new_current_version(
            SpentOutput::OutputData {
                features: self.features.clone(),
                commitment,
                script: self.script.clone(),
                sender_offset_public_key: self.sender_offset_public_key.clone(),
                covenant: self.covenant.clone(),
                version: self.version,
                encrypted_data: self.encrypted_data,
                metadata_signature: self.metadata_signature.clone(),
                rangeproof_hash,
                minimum_value_promise: self.minimum_value_promise,
            },
            self.input_data.clone(),
            script_signature,
        ))
    }

    /// Commits an WalletOutput into a TransactionInput that only contains the hash of the spent output data
    pub async fn to_compact_transaction_input<KM: TransactionKeyManagerInterface>(
        &self,
        key_manager: &KM,
    ) -> Result<TransactionInput, TransactionError> {
        let input = self.to_transaction_input(key_manager).await?;

        Ok(TransactionInput::new(
            input.version,
            SpentOutput::OutputHash(input.output_hash()),
            input.input_data,
            input.script_signature,
        ))
    }

    pub async fn to_transaction_output<KM: TransactionKeyManagerInterface>(
        &self,
        key_manager: &KM,
    ) -> Result<TransactionOutput, TransactionError> {
        if self.features.range_proof_type == RangeProofType::RevealedValue && self.minimum_value_promise != self.value {
            return Err(TransactionError::RangeProofError(format!(
                "Invalid revealed value: Expected {}, received {}",
                self.value, self.minimum_value_promise
            )));
        }
        let value = self.value.into();
        let commitment = key_manager.get_commitment(&self.spending_key_id, &value).await?;

        let output = TransactionOutput::new(
            self.version,
            self.features.clone(),
            commitment,
            self.rangeproof.clone(),
            self.script.clone(),
            self.sender_offset_public_key.clone(),
            self.metadata_signature.clone(),
            self.covenant.clone(),
            self.encrypted_data,
            self.minimum_value_promise,
        );

        Ok(output)
    }

    pub fn features_and_scripts_byte_size(&self) -> std::io::Result<usize> {
        Ok(self.features.get_serialized_size()? +
            self.script.get_serialized_size()? +
            self.covenant.get_serialized_size()?)
    }

    // Note: The Hashable trait is not used here due to the dependency on `CryptoFactories`, and `commitment` is not
    // Note: added to the struct to ensure consistency between `commitment`, `spending_key` and `value`.
    pub async fn hash<KM: TransactionKeyManagerInterface>(
        &self,
        key_manager: &KM,
    ) -> Result<FixedHash, TransactionError> {
        let output = self.to_transaction_output(key_manager).await?;
        let rp_hash = match output.proof {
            Some(rp) => rp.hash(),
            None => FixedHash::zero(),
        };
        Ok(transaction_components::hash_output(
            self.version,
            &self.features,
            &output.commitment,
            &rp_hash,
            &self.script,
            &self.sender_offset_public_key,
            &self.metadata_signature,
            &self.covenant,
            &self.encrypted_data,
            self.minimum_value_promise,
        ))
    }

    pub async fn commitment<KM: TransactionKeyManagerInterface>(
        &self,
        key_manager: &KM,
    ) -> Result<Commitment, TransactionError> {
        Ok(key_manager
            .get_commitment(&self.spending_key_id, &self.value.into())
            .await?)
    }

    /// Is this a burned output kernel?
    pub fn is_burned(&self) -> bool {
        matches!(self.features.output_type, OutputType::Burn)
    }
}

// These implementations are used for order these outputs for UTXO selection which is done by comparing the values
impl Eq for WalletOutput {}

impl PartialEq for WalletOutput {
    fn eq(&self, other: &WalletOutput) -> bool {
        self.value == other.value
    }
}

impl PartialOrd<WalletOutput> for WalletOutput {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for WalletOutput {
    fn cmp(&self, other: &Self) -> Ordering {
        self.value.cmp(&other.value)
    }
}

impl Debug for WalletOutput {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyManagerOutput")
            .field("version", &self.version)
            .field("value", &self.value)
            .field("spending_key_id", &self.spending_key_id)
            .field("features", &self.features)
            .field("script", &self.script)
            .field("covenant", &self.covenant)
            .field("input_data", &self.input_data)
            .field("script_private_key_id", &self.script_key_id)
            .field("sender_offset_public_key", &self.sender_offset_public_key)
            .field("metadata_signature", &self.metadata_signature)
            .field("script_lock_height", &self.script_lock_height)
            .field("encrypted_data", &self.encrypted_data)
            .field("minimum_value_promise", &self.minimum_value_promise)
            .finish()
    }
}
