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
use tari_common_types::types::{ComAndPubSignature, CompressedPublicKey, PrivateKey, RangeProof};
use tari_script::{ExecutionStack, TariScript};

use super::{RangeProofType, TransactionOutputVersion};
use crate::{
    key_manager::{SecretTransactionKeyManagerInterface, TransactionKeyManagerInterface},
    transaction_components::{
        covenants::Covenant,
        EncryptedData,
        MemoField,
        OutputFeatures,
        TransactionError,
        WalletOutput,
    },
    MicroMinotari,
};

/// An unblinded output is one where the value and spending key (blinding factor) are known. This can be used to
/// build both inputs and outputs (every input comes from an output). This is only used for import and export where
/// serialization is important.
#[derive(Clone, Serialize, Deserialize)]
pub struct UnblindedOutput {
    pub version: TransactionOutputVersion,
    pub value: MicroMinotari,
    pub commitment_mask_key: PrivateKey,
    pub features: OutputFeatures,
    pub script: TariScript,
    pub covenant: Covenant,
    pub input_data: ExecutionStack,
    pub script_private_key: PrivateKey,
    pub sender_offset_public_key: CompressedPublicKey,
    pub metadata_signature: ComAndPubSignature,
    pub script_lock_height: u64,
    pub encrypted_data: EncryptedData,
    pub minimum_value_promise: MicroMinotari,
    pub range_proof: Option<RangeProof>,
}

impl UnblindedOutput {
    /// Creates a new un-blinded output
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        version: TransactionOutputVersion,
        value: MicroMinotari,
        commitment_mask_key: PrivateKey,
        features: OutputFeatures,
        script: TariScript,
        input_data: ExecutionStack,
        script_private_key: PrivateKey,
        sender_offset_public_key: CompressedPublicKey,
        metadata_signature: ComAndPubSignature,
        script_lock_height: u64,
        covenant: Covenant,
        encrypted_data: EncryptedData,
        minimum_value_promise: MicroMinotari,
        range_proof: Option<RangeProof>,
    ) -> Self {
        let range_proof = if features.range_proof_type == RangeProofType::BulletProofPlus {
            range_proof
        } else {
            None
        };
        Self {
            version,
            value,
            commitment_mask_key,
            features,
            script,
            input_data,
            script_private_key,
            sender_offset_public_key,
            metadata_signature,
            script_lock_height,
            covenant,
            encrypted_data,
            minimum_value_promise,
            range_proof,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_current_version(
        value: MicroMinotari,
        commitment_mask_key: PrivateKey,
        features: OutputFeatures,
        script: TariScript,
        input_data: ExecutionStack,
        script_private_key: PrivateKey,
        sender_offset_public_key: CompressedPublicKey,
        metadata_signature: ComAndPubSignature,
        script_lock_height: u64,
        covenant: Covenant,
        encrypted_data: EncryptedData,
        minimum_value_promise: MicroMinotari,
        range_proof: Option<RangeProof>,
    ) -> Self {
        Self::new(
            TransactionOutputVersion::get_current_version(),
            value,
            commitment_mask_key,
            features,
            script,
            input_data,
            script_private_key,
            sender_offset_public_key,
            metadata_signature,
            script_lock_height,
            covenant,
            encrypted_data,
            minimum_value_promise,
            range_proof,
        )
    }

    pub fn to_wallet_output<KM: TransactionKeyManagerInterface>(
        self,
        key_manager: &KM,
        payment_id: MemoField,
    ) -> Result<WalletOutput, TransactionError> {
        let commitment_mask_key_id = key_manager.import_key(self.commitment_mask_key, None)?;
        let spending_key = key_manager.get_spend_key();
        let script_key_id = key_manager
            .import_key(self.script_private_key, Some(spending_key.key_id))?;
        let wallet_output = WalletOutput::new_with_rangeproof(
            self.version,
            self.value,
            commitment_mask_key_id,
            self.features,
            self.script,
            self.input_data,
            script_key_id,
            self.sender_offset_public_key,
            self.metadata_signature,
            self.script_lock_height,
            self.covenant,
            self.encrypted_data,
            self.minimum_value_promise,
            self.range_proof,
            payment_id,
            key_manager,
        )?;
        Ok(wallet_output)
    }

    pub fn from_wallet_output<KM: SecretTransactionKeyManagerInterface>(
        output: WalletOutput,
        key_manager: &KM,
    ) -> Result<Self, TransactionError> {
        let commitment_mask_key = key_manager.get_private_key(output.commitment_mask_key_id())?;
        let script_private_key = key_manager.get_private_key(output.script_key_id())?;
        let range_proof = if output.features().range_proof_type == RangeProofType::BulletProofPlus {
            output.range_proof().clone()
        } else {
            None
        };
        let unblinded_output = UnblindedOutput {
            version: output.version(),
            value: output.value(),
            commitment_mask_key,
            features: output.features().clone(),
            script: output.script().clone(),
            covenant: output.covenant().clone(),
            input_data: output.input_data().clone(),
            script_private_key,
            sender_offset_public_key: output.sender_offset_public_key().clone(),
            metadata_signature: output.metadata_signature().clone(),
            script_lock_height: output.script_lock_height(),
            encrypted_data: output.encrypted_data().clone(),
            minimum_value_promise: output.minimum_value_promise(),
            range_proof,
        };
        Ok(unblinded_output)
    }
}

// These implementations are used for order these outputs for UTXO selection which will be done by comparing the values
impl Eq for UnblindedOutput {}

impl PartialEq for UnblindedOutput {
    fn eq(&self, other: &UnblindedOutput) -> bool {
        self.value == other.value
    }
}

impl PartialOrd<UnblindedOutput> for UnblindedOutput {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for UnblindedOutput {
    fn cmp(&self, other: &Self) -> Ordering {
        self.value.cmp(&other.value)
    }
}

impl Debug for UnblindedOutput {
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
            .field("encrypted_data", &self.encrypted_data)
            .field("minimum_value_promise", &self.minimum_value_promise)
            .finish()
    }
}
