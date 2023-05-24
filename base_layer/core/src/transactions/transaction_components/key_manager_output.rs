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
use tari_common_types::types::{ComAndPubSignature, FixedHash, PublicKey};
use tari_script::{ExecutionStack, TariScript};

use super::TransactionOutputVersion;
use crate::{
    borsh::SerializedSize,
    core_key_manager::{BaseLayerKeyManagerInterface, KeyId},
    covenants::Covenant,
    transactions::{
        tari_amount::MicroTari,
        transaction_components,
        transaction_components::{
            transaction_input::{SpentOutput, TransactionInput},
            transaction_output::TransactionOutput,
            EncryptedValue,
            OutputFeatures,
            TransactionError,
            TransactionInputVersion,
        },
    },
};

/// An unblinded output is one where the value and spending key (blinding factor) are known. This can be used to
/// build both inputs and outputs (every input comes from an output)
// TODO: Try to get rid of 'Serialize' and 'Deserialize' traits here; see related comment at 'struct RawTransactionInfo'
// #LOGGED
#[derive(Clone, Serialize, Deserialize)]
pub struct KeyManagerOutput {
    pub version: TransactionOutputVersion,
    pub value: MicroTari,
    pub spending_key: KeyId, // rename to id
    pub features: OutputFeatures,
    pub script: TariScript,
    pub covenant: Covenant,
    pub input_data: ExecutionStack,
    pub script_private_key: KeyId, //rename to id
    pub sender_offset_public_key: PublicKey,
    pub metadata_signature: ComAndPubSignature,
    pub script_lock_height: u64,
    pub encrypted_value: EncryptedValue,
    pub minimum_value_promise: MicroTari,
}

impl KeyManagerOutput {
    /// Creates a new un-blinded output

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        version: TransactionOutputVersion,
        value: MicroTari,
        spending_key: KeyId,
        features: OutputFeatures,
        script: TariScript,
        input_data: ExecutionStack,
        script_private_key: KeyId,
        sender_offset_public_key: PublicKey,
        metadata_signature: ComAndPubSignature,
        script_lock_height: u64,
        covenant: Covenant,
        encrypted_value: EncryptedValue,
        minimum_value_promise: MicroTari,
    ) -> Self {
        Self {
            version,
            value,
            spending_key,
            features,
            script,
            input_data,
            script_private_key,
            sender_offset_public_key,
            metadata_signature,
            script_lock_height,
            covenant,
            encrypted_value,
            minimum_value_promise,
        }
    }

    pub fn new_current_version(
        value: MicroTari,
        spending_key: KeyId,
        features: OutputFeatures,
        script: TariScript,
        input_data: ExecutionStack,
        script_private_key: KeyId,
        sender_offset_public_key: PublicKey,
        metadata_signature: ComAndPubSignature,
        script_lock_height: u64,
        covenant: Covenant,
        encrypted_value: EncryptedValue,
        minimum_value_promise: MicroTari,
    ) -> Self {
        Self::new(
            TransactionOutputVersion::get_current_version(),
            value,
            spending_key,
            features,
            script,
            input_data,
            script_private_key,
            sender_offset_public_key,
            metadata_signature,
            script_lock_height,
            covenant,
            encrypted_value,
            minimum_value_promise,
        )
    }

    /// Commits an UnblindedOutput into a Transaction input
    pub async fn as_transaction_input<KM: BaseLayerKeyManagerInterface>(
        &self,
        key_manager: &KM,
    ) -> Result<TransactionInput, TransactionError> {
        let value = self.value.into();
        let commitment = key_manager.get_commitment(&self.spending_key, &value).await?;
        let version = TransactionInputVersion::get_current_version();
        let script_message = TransactionInput::build_script_signature_message(
            version,
            &self.script,
            &self.input_data,
        );
        let script_signature = key_manager
            .get_script_signature(&self.script_private_key, &self.spending_key, &value, version, &script_message)
            .await?;

        // .map_err(|_| TransactionError::InvalidSignatureError("Generating script signature".to_string()))?;

        Ok(TransactionInput::new_current_version(
            SpentOutput::OutputData {
                features: self.features.clone(),
                commitment,
                script: self.script.clone(),
                sender_offset_public_key: self.sender_offset_public_key.clone(),
                covenant: self.covenant.clone(),
                version: self.version,
                encrypted_value: self.encrypted_value.clone(),
                minimum_value_promise: self.minimum_value_promise,
            },
            self.input_data.clone(),
            script_signature,
        ))
    }

    /// Commits an UnblindedOutput into a TransactionInput that only contains the hash of the spent output data
    pub async fn as_compact_transaction_input<KM: BaseLayerKeyManagerInterface>(
        &self,
        key_manager: &KM,
    ) -> Result<TransactionInput, TransactionError> {
        let input = self.as_transaction_input(key_manager).await?;

        Ok(TransactionInput::new(
            input.version,
            SpentOutput::OutputHash(input.output_hash()),
            input.input_data,
            input.script_signature,
        ))
    }

    pub async fn as_transaction_output<KM: BaseLayerKeyManagerInterface>(
        &self,
        key_manager: &KM,
    ) -> Result<TransactionOutput, TransactionError> {
        let value = self.value.into();
        let commitment = key_manager.get_commitment(&self.spending_key, &value).await?;
        let range_proof = key_manager
            .construct_range_proof(&self.spending_key, self.value.into(), self.minimum_value_promise.into())
            .await?;

        let output = TransactionOutput::new(
            self.version,
            self.features.clone(),
            commitment,
            range_proof,
            self.script.clone(),
            self.sender_offset_public_key.clone(),
            self.metadata_signature.clone(),
            self.covenant.clone(),
            self.encrypted_value.clone(),
            self.minimum_value_promise,
        );

        Ok(output)
    }

    pub fn features_and_scripts_byte_size(&self) -> usize {
        self.features.get_serialized_size() + self.script.get_serialized_size() + self.covenant.get_serialized_size()
    }

    // Note: The Hashable trait is not used here due to the dependency on `CryptoFactories`, and `commitment` is not
    // Note: added to the struct to ensure consistency between `commitment`, `spending_key` and `value`.
    pub async fn hash<KM: BaseLayerKeyManagerInterface>(
        &self,
        key_manager: &KM,
    ) -> Result<FixedHash, TransactionError> {
        let value = self.value.into();
        let commitment = key_manager.get_commitment(&self.spending_key, &value).await?;
        Ok(transaction_components::hash_output(
            self.version,
            &self.features,
            &commitment,
            &self.script,
            &self.covenant,
            &self.encrypted_value,
            &self.sender_offset_public_key,
            self.minimum_value_promise,
        ))
    }
}

// These implementations are used for order these outputs for UTXO selection which will be done by comparing the values
impl Eq for KeyManagerOutput {}

impl PartialEq for KeyManagerOutput {
    fn eq(&self, other: &KeyManagerOutput) -> bool {
        self.value == other.value
    }
}

impl PartialOrd<KeyManagerOutput> for KeyManagerOutput {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.value.partial_cmp(&other.value)
    }
}

impl Ord for KeyManagerOutput {
    fn cmp(&self, other: &Self) -> Ordering {
        self.value.cmp(&other.value)
    }
}

impl Debug for KeyManagerOutput {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UnblindedOutput")
            .field("version", &self.version)
            .field("value", &self.value)
            .field("spending_key", &"<secret>")
            .field("features", &self.features)
            .field("script", &self.script)
            .field("covenant", &self.covenant)
            .field("input_data", &self.input_data)
            .field("script_private_key", &"<secret>")
            .field("sender_offset_public_key", &self.sender_offset_public_key)
            .field("metadata_signature", &self.metadata_signature)
            .field("script_lock_height", &self.script_lock_height)
            .field("minimum_value_promise", &self.minimum_value_promise)
            .finish()
    }
}
