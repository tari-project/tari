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
    default::Default,
    fmt::{Debug, Formatter},
    sync::OnceLock,
};

use minotari_ledger_wallet_common::common_types::LedgerKeyBranch;
use serde::{Deserialize, Serialize};
use tari_common_types::{
    tari_address::TariAddress,
    transaction::TxId,
    types::{ComAndPubSignature, CompressedCommitment, CompressedPublicKey, FixedHash, RangeProof},
};
use tari_script::{ExecutionStack, Opcode, TariScript, inputs, script};

use super::TransactionOutputVersion;
use crate::{
    MicroMinotari,
    helpers::borsh::SerializedSize,
    key_manager::{SerializedKeyString, TariKeyId, TransactionKeyManagerInterface},
    transaction_components,
    transaction_components::{
        EncryptedData,
        MemoField,
        OutputFeatures,
        OutputType,
        RangeProofType,
        TransactionError,
        TransactionInputVersion,
        covenants::Covenant,
        transaction_input::{SpentOutput, TransactionInput},
        transaction_output::TransactionOutput,
    },
};

/// A wallet output is one where the value and spending key (blinding factor) are known. This can be used to
/// build both inputs and outputs (every input comes from an output)
#[derive(Clone, Serialize, Deserialize)]
pub struct WalletOutput {
    version: TransactionOutputVersion,
    value: MicroMinotari,
    commitment_mask_key_id: TariKeyId,
    features: OutputFeatures,
    script: TariScript,
    covenant: Covenant,
    input_data: ExecutionStack,
    script_key_id: TariKeyId,
    sender_offset_public_key: CompressedPublicKey,
    metadata_signature: ComAndPubSignature,
    script_lock_height: u64,
    encrypted_data: EncryptedData,
    minimum_value_promise: MicroMinotari,
    range_proof: Option<RangeProof>,
    payment_id: MemoField,
    output_hash: FixedHash,
    commitment: CompressedCommitment,
    #[serde(skip)]
    input: OnceLock<TransactionInput>,
    #[serde(skip)]
    output: OnceLock<TransactionOutput>,
}

impl WalletOutput {
    /// Creates a new wallet output
    #[allow(clippy::too_many_arguments)]
    pub fn new<KM: TransactionKeyManagerInterface>(
        version: TransactionOutputVersion,
        value: MicroMinotari,
        commitment_mask_key_id: TariKeyId,
        features: OutputFeatures,
        script: TariScript,
        input_data: ExecutionStack,
        script_key_id: TariKeyId,
        sender_offset_public_key: CompressedPublicKey,
        metadata_signature: ComAndPubSignature,
        script_lock_height: u64,
        covenant: Covenant,
        encrypted_data: EncryptedData,
        minimum_value_promise: MicroMinotari,
        payment_id: MemoField,
        key_manager: &KM,
    ) -> Result<Self, TransactionError> {
        let range_proof = if features.range_proof_type == RangeProofType::BulletProofPlus {
            Some(key_manager.construct_range_proof(
                &commitment_mask_key_id,
                value.into(),
                minimum_value_promise.into(),
            )?)
        } else {
            None
        };
        let commitment = key_manager.get_commitment(&commitment_mask_key_id, &value.into())?;
        let output_hash = FixedHash::zero();
        let mut output = Self {
            version,
            value,
            commitment_mask_key_id,
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
            range_proof,
            payment_id,
            commitment,
            output_hash,
            input: OnceLock::new(),
            output: OnceLock::new(),
        };
        output.recalculate_hash();
        Ok(output)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_rangeproof<KM: TransactionKeyManagerInterface>(
        version: TransactionOutputVersion,
        value: MicroMinotari,
        commitment_mask_key_id: TariKeyId,
        features: OutputFeatures,
        script: TariScript,
        input_data: ExecutionStack,
        script_key_id: TariKeyId,
        sender_offset_public_key: CompressedPublicKey,
        metadata_signature: ComAndPubSignature,
        script_lock_height: u64,
        covenant: Covenant,
        encrypted_data: EncryptedData,
        minimum_value_promise: MicroMinotari,
        rangeproof: Option<RangeProof>,
        payment_id: MemoField,
        key_manager: &KM,
    ) -> Result<Self, TransactionError> {
        let commitment = key_manager.get_commitment(&commitment_mask_key_id, &value.into())?;
        let rp_hash = match &rangeproof {
            Some(rp) => rp.hash(),
            None => FixedHash::zero(),
        };
        let output_hash = transaction_components::hash_output(
            version,
            &features,
            &commitment,
            &rp_hash,
            &script,
            &sender_offset_public_key,
            &metadata_signature,
            &covenant,
            &encrypted_data,
            minimum_value_promise,
        );
        Ok(Self {
            version,
            value,
            commitment_mask_key_id,
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
            range_proof: rangeproof,
            payment_id,
            commitment,
            output_hash,
            input: OnceLock::new(),
            output: OnceLock::new(),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_from_parts(
        version: TransactionOutputVersion,
        value: MicroMinotari,
        commitment_mask_key_id: TariKeyId,
        features: OutputFeatures,
        script: TariScript,
        input_data: ExecutionStack,
        script_key_id: TariKeyId,
        sender_offset_public_key: CompressedPublicKey,
        metadata_signature: ComAndPubSignature,
        script_lock_height: u64,
        covenant: Covenant,
        encrypted_data: EncryptedData,
        minimum_value_promise: MicroMinotari,
        rangeproof: Option<RangeProof>,
        payment_id: MemoField,
        output_hash: FixedHash,
        commitment: CompressedCommitment,
    ) -> Self {
        Self {
            version,
            value,
            commitment_mask_key_id,
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
            range_proof: rangeproof,
            payment_id,
            commitment,
            output_hash,
            input: OnceLock::new(),
            output: OnceLock::new(),
        }
    }

    /// This will create a new wallet output and try and calculate the required script key and input stack to spend this
    /// output, will return None if it cannot calculate the script key or input stack
    pub fn new_imported<KM: TransactionKeyManagerInterface>(
        value: MicroMinotari,
        commitment_mask_key_id: TariKeyId,
        memo: MemoField,
        output: TransactionOutput,
        key_manager: &KM,
    ) -> Result<Self, TransactionError> {
        let (input_data, script_key_id) =
            WalletOutput::calculate_script_private_key_id(&output.script, &commitment_mask_key_id, key_manager)?
                .ok_or(TransactionError::KeyManagerError(
                    "Could not find a valid script key for the script".to_string(),
                ))?;
        let wallet_output = WalletOutput::new_from_transaction_output(
            value,
            commitment_mask_key_id,
            memo,
            output,
            input_data,
            script_key_id,
        );
        Ok(wallet_output)
    }

    pub fn new_from_transaction_output(
        value: MicroMinotari,
        commitment_mask_key_id: TariKeyId,
        memo: MemoField,
        output: TransactionOutput,
        input_data: ExecutionStack,
        script_key_id: TariKeyId,
    ) -> Self {
        let output_hash = output.hash();
        let locked = OnceLock::new();
        let _unused = locked.set(output.clone());
        WalletOutput {
            version: output.version,
            value,
            commitment_mask_key_id,
            features: output.features,
            script: output.script,
            input_data,
            script_key_id,
            sender_offset_public_key: output.sender_offset_public_key,
            metadata_signature: output.metadata_signature,
            script_lock_height: 0,
            covenant: output.covenant,
            encrypted_data: output.encrypted_data,
            minimum_value_promise: output.minimum_value_promise,
            range_proof: output.proof,
            payment_id: memo,
            commitment: output.commitment,
            output_hash,
            input: OnceLock::new(),
            output: locked,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_current_version<KM: TransactionKeyManagerInterface>(
        value: MicroMinotari,
        commitment_mask_key_id: TariKeyId,
        features: OutputFeatures,
        script: TariScript,
        input_data: ExecutionStack,
        script_key_id: TariKeyId,
        sender_offset_public_key: CompressedPublicKey,
        metadata_signature: ComAndPubSignature,
        script_lock_height: u64,
        covenant: Covenant,
        encrypted_data: EncryptedData,
        minimum_value_promise: MicroMinotari,
        payment_id: MemoField,
        key_manager: &KM,
    ) -> Result<Self, TransactionError> {
        Self::new(
            TransactionOutputVersion::get_current_version(),
            value,
            commitment_mask_key_id,
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
            payment_id,
            key_manager,
        )
    }

    fn calculate_script_private_key_id<KM: TransactionKeyManagerInterface>(
        script: &TariScript,
        commitment_mask_key_id: &TariKeyId,
        key_manager: &KM,
    ) -> Result<Option<(ExecutionStack, TariKeyId)>, TransactionError> {
        if *script == script!(Nop)? {
            // This is a nop, so we can just create a new key for the input stack.
            let key = key_manager.get_random_key(None, None)?;
            return Ok(Some((inputs!(key.pub_key.clone()), key.key_id)));
        }
        // this is push public key script, so lets see if we know the public key
        if let [Opcode::PushPubKey(public_key)] = script.as_slice() {
            // first check non stealth direct to spend key outputs
            let spend_key = key_manager.get_spend_key();
            if spend_key.pub_key == **public_key {
                return Ok(Some((ExecutionStack::default(), spend_key.key_id)));
            }

            // next lets check the commitment mask derived keys
            let result =
                key_manager.find_script_key_id_from_commitment_mask_key_id(commitment_mask_key_id, Some(public_key))?;
            if let Some(script_key_id) = result {
                return Ok(Some((ExecutionStack::default(), script_key_id)));
            }
            // now lets try stealth
            let script_spending_key =
                key_manager.stealth_address_script_spending_key(commitment_mask_key_id, &spend_key.pub_key)?;

            if script_spending_key == **public_key {
                let script_key = TariKeyId::Derived {
                    key: SerializedKeyString::from(commitment_mask_key_id.to_string()),
                };
                return Ok(Some((ExecutionStack::default(), script_key)));
            }
        }

        // no match

        Ok(None)
    }

    /// Commits an KeyManagerOutput into a Transaction input
    pub fn to_transaction_input<KM: TransactionKeyManagerInterface>(
        &self,
        key_manager: &KM,
    ) -> Result<TransactionInput, TransactionError> {
        if let Some(input) = self.input.get() {
            return Ok(input.clone());
        }
        let rangeproof_hash = match &self.range_proof {
            Some(rp) => rp.hash(),
            None => FixedHash::zero(),
        };
        let version = TransactionInputVersion::get_current_version();
        let script_message = TransactionInput::build_script_signature_message(version, &self.script, &self.input_data);
        let value = self.value.into();
        let script_signature = key_manager.get_script_signature(
            &self.script_key_id,
            &self.commitment_mask_key_id,
            &value,
            version,
            &script_message,
        )?;
        let input = TransactionInput::new_current_version(
            SpentOutput::OutputData {
                features: self.features.clone(),
                commitment: self.commitment.clone(),
                script: self.script.clone(),
                sender_offset_public_key: self.sender_offset_public_key.clone(),
                covenant: self.covenant.clone(),
                version: self.version,
                encrypted_data: self.encrypted_data.clone(),
                metadata_signature: self.metadata_signature.clone(),
                rangeproof_hash,
                minimum_value_promise: self.minimum_value_promise,
            },
            self.input_data.clone(),
            script_signature,
        );
        let _unused = self.input.set(input.clone());
        Ok(input)
    }

    /// Calculate the deterministic TxId for this output, given a optional unique repeatable key
    pub fn calculate_tx_id(&self, unique_key: &[u8]) -> TxId {
        TxId::new_deterministic(unique_key, &self.output_hash)
    }

    /// It creates a transaction input given an updated multi-party script public keys and nonces. The inputs
    /// `script_signature_public_nonces` and `script_public_key_shares` exclude the caller's data.
    pub fn to_transaction_input_with_multi_party_script_signature<KM: TransactionKeyManagerInterface>(
        &self,
        aggregated_script_signature_public_nonces: &CompressedPublicKey,
        aggregated_script_public_key_shares: &CompressedPublicKey,
        key_manager: &KM,
    ) -> Result<(TransactionInput, CompressedPublicKey), TransactionError> {
        let value = self.value.into();
        let version = TransactionInputVersion::get_current_version();
        let commitment = key_manager.get_commitment(&self.commitment_mask_key_id, &value)?;

        let message = TransactionInput::build_script_signature_message(version, &self.script, &self.input_data);
        let ephemeral_public_key_self =
            key_manager.get_random_key(None, Some(LedgerKeyBranch::MetadataEphemeralNonce))?;
        let script_public_key_self = key_manager.get_public_key_at_key_id(&self.script_key_id)?;
        let script_public_key = CompressedPublicKey::new_from_pk(
            aggregated_script_public_key_shares.to_public_key()? + script_public_key_self.to_public_key()?,
        );

        let total_ephemeral_public_key = CompressedPublicKey::new_from_pk(
            aggregated_script_signature_public_nonces.to_public_key()? +
                &ephemeral_public_key_self.pub_key.to_public_key()?,
        );
        let commitment_partial_script_signature = key_manager.get_partial_script_signature(
            &self.commitment_mask_key_id,
            &value,
            version,
            &total_ephemeral_public_key,
            &script_public_key,
            &message,
        )?;
        let challenge = TransactionInput::finalize_script_signature_challenge(
            version,
            commitment_partial_script_signature.ephemeral_commitment(),
            &total_ephemeral_public_key,
            &script_public_key,
            &commitment,
            &message,
        );
        let script_key_partial_script_signature = key_manager.sign_with_nonce_and_challenge(
            &self.script_key_id,
            &ephemeral_public_key_self.key_id,
            &challenge,
        )?;
        let script_signature = ComAndPubSignature::new_from_capk_signature(
            &commitment_partial_script_signature.to_capk_signature()? +
                &script_key_partial_script_signature.to_schnorr_signature()?,
        );

        let input = TransactionInput::new_current_version(
            SpentOutput::OutputData {
                features: self.features.clone(),
                commitment,
                script: self.script.clone(),
                sender_offset_public_key: self.sender_offset_public_key.clone(),
                covenant: self.covenant.clone(),
                encrypted_data: self.encrypted_data.clone(),
                metadata_signature: self.metadata_signature.clone(),
                version: self.version,
                minimum_value_promise: self.minimum_value_promise,
                rangeproof_hash: match &self.range_proof {
                    Some(rp) => rp.hash(),
                    None => FixedHash::zero(),
                },
            },
            self.input_data.clone(),
            script_signature,
        );

        Ok((input, script_public_key))
    }

    /// Commits an WalletOutput into a TransactionInput that only contains the hash of the spent output data
    pub fn to_compact_transaction_input<KM: TransactionKeyManagerInterface>(
        &self,
        key_manager: &KM,
    ) -> Result<TransactionInput, TransactionError> {
        let input = self.to_transaction_input(key_manager)?;

        Ok(TransactionInput::new(
            input.version,
            SpentOutput::OutputHash(input.output_hash()),
            input.input_data,
            input.script_signature,
        ))
    }

    pub fn to_transaction_output(&self) -> Result<TransactionOutput, TransactionError> {
        if self.features.range_proof_type == RangeProofType::RevealedValue && self.minimum_value_promise != self.value {
            return Err(TransactionError::RangeProofError(format!(
                "Invalid revealed value: Expected {}, received {}",
                self.value, self.minimum_value_promise
            )));
        }
        if let Some(output) = self.output.get() {
            return Ok(output.clone());
        }
        let output = TransactionOutput::new(
            self.version,
            self.features.clone(),
            self.commitment.clone(),
            self.range_proof.clone(),
            self.script.clone(),
            self.sender_offset_public_key.clone(),
            self.metadata_signature.clone(),
            self.covenant.clone(),
            self.encrypted_data.clone(),
            self.minimum_value_promise,
        );
        let _unused = self.output.set(output.clone());
        Ok(output)
    }

    pub fn features_and_scripts_byte_size(&self) -> std::io::Result<usize> {
        Ok(self.features.get_serialized_size()? +
            self.script.get_serialized_size()? +
            self.covenant.get_serialized_size()?)
    }

    /// Is this a burned output kernel?
    pub fn is_burned(&self) -> bool {
        matches!(self.features.output_type, OutputType::Burn)
    }

    /// helper function to determine if this is a coinbase or not
    pub fn is_coinbase(&self) -> bool {
        matches!(self.features.output_type, OutputType::Coinbase)
    }

    pub fn change_encrypted_data<KM: TransactionKeyManagerInterface>(
        &mut self,
        encrypted_data: EncryptedData,
        sender_offset: &TariKeyId,
        payment_id: MemoField,
        key_manager: &KM,
    ) -> Result<(), TransactionError> {
        self.input = OnceLock::new();
        self.output = OnceLock::new();
        self.encrypted_data = encrypted_data;
        self.payment_id = payment_id;
        // now we have to update the metadata signature as this has changed
        let metadata_message = TransactionOutput::metadata_signature_message_from_parts(
            self.version,
            &self.script,
            &self.features,
            &self.covenant,
            &self.encrypted_data,
            &self.minimum_value_promise,
        );

        let metadata_sig = key_manager.get_metadata_signature(
            &self.commitment_mask_key_id,
            &self.value.into(),
            sender_offset,
            self.version,
            &metadata_message,
            self.features.range_proof_type,
        )?;
        self.metadata_signature = metadata_sig;
        self.recalculate_hash();
        Ok(())
    }

    pub fn change_encrypted_data_with_verified_signature<KM: TransactionKeyManagerInterface>(
        &mut self,
        encrypted_data: EncryptedData,
        sender_offset: &TariKeyId,
        payment_id: MemoField,
        recipient_address: &TariAddress,
        key_manager: &KM,
    ) -> Result<(), TransactionError> {
        self.input = OnceLock::new();
        self.output = OnceLock::new();
        self.encrypted_data = encrypted_data;
        self.payment_id = payment_id;
        // now we have to update the metadata signature as this has changed
        let metadata_message_common = TransactionOutput::metadata_signature_message_common_from_parts(
            &self.version,
            &self.features,
            &self.covenant,
            &self.encrypted_data,
            &self.minimum_value_promise,
        );

        let metadata_sig = key_manager.get_metadata_signature_user_verified(
            &self.commitment_mask_key_id,
            self.value,
            sender_offset,
            self.version,
            &metadata_message_common,
            self.features.range_proof_type,
            &self.script,
            recipient_address,
        )?;
        self.metadata_signature = metadata_sig;
        self.recalculate_hash();
        Ok(())
    }

    fn recalculate_hash(&mut self) {
        let rp_hash = match &self.range_proof {
            Some(rp) => rp.hash(),
            None => FixedHash::zero(),
        };
        let output_hash = transaction_components::hash_output(
            self.version,
            &self.features,
            &self.commitment,
            &rp_hash,
            &self.script,
            &self.sender_offset_public_key,
            &self.metadata_signature,
            &self.covenant,
            &self.encrypted_data,
            self.minimum_value_promise,
        );
        self.output_hash = output_hash;
    }

    pub fn version(&self) -> TransactionOutputVersion {
        self.version
    }

    pub fn value(&self) -> MicroMinotari {
        self.value
    }

    pub fn set_value<KM: TransactionKeyManagerInterface>(
        &mut self,
        value: MicroMinotari,
        key_manager: &KM,
    ) -> Result<(), TransactionError> {
        self.input = OnceLock::new();
        self.output = OnceLock::new();
        self.value = value;
        let commitment = key_manager.get_commitment(&self.commitment_mask_key_id, &self.value.into())?;
        self.commitment = commitment;
        let range_proof = if self.features.range_proof_type == RangeProofType::BulletProofPlus {
            Some(key_manager.construct_range_proof(
                &self.commitment_mask_key_id,
                self.value.into(),
                self.minimum_value_promise.into(),
            )?)
        } else {
            None
        };
        self.range_proof = range_proof;
        self.recalculate_hash();
        Ok(())
    }

    pub fn commitment_mask_key_id(&self) -> &TariKeyId {
        &self.commitment_mask_key_id
    }

    pub fn set_commitment_mask_key_id<KM: TransactionKeyManagerInterface>(
        &mut self,
        key_id: TariKeyId,
        key_manager: &KM,
    ) -> Result<(), TransactionError> {
        self.input = OnceLock::new();
        self.output = OnceLock::new();
        self.commitment_mask_key_id = key_id;
        let commitment = key_manager.get_commitment(&self.commitment_mask_key_id, &self.value.into())?;
        self.commitment = commitment;
        self.recalculate_hash();
        Ok(())
    }

    pub fn features(&self) -> &OutputFeatures {
        &self.features
    }

    pub fn set_features(&mut self, features: OutputFeatures) {
        self.input = OnceLock::new();
        self.output = OnceLock::new();
        self.features = features;
        self.recalculate_hash();
    }

    pub fn script(&self) -> &TariScript {
        &self.script
    }

    pub fn set_script(&mut self, script: TariScript) {
        self.input = OnceLock::new();
        self.output = OnceLock::new();
        self.script = script;
        self.recalculate_hash();
    }

    pub fn covenant(&self) -> &Covenant {
        &self.covenant
    }

    pub fn set_covenant(&mut self, covenant: Covenant) {
        self.input = OnceLock::new();
        self.output = OnceLock::new();
        self.covenant = covenant;
        self.recalculate_hash();
    }

    pub fn input_data(&self) -> &ExecutionStack {
        &self.input_data
    }

    pub fn set_input_data(&mut self, input_data: ExecutionStack) {
        self.input = OnceLock::new();
        self.input_data = input_data;
    }

    pub fn script_key_id(&self) -> &TariKeyId {
        &self.script_key_id
    }

    pub fn set_script_key_id(&mut self, key_id: TariKeyId) {
        self.input = OnceLock::new();
        self.output = OnceLock::new();
        self.script_key_id = key_id;
    }

    pub fn sender_offset_public_key(&self) -> &CompressedPublicKey {
        &self.sender_offset_public_key
    }

    pub fn set_sender_offset_public_key(&mut self, pk: CompressedPublicKey) {
        self.input = OnceLock::new();
        self.output = OnceLock::new();
        self.sender_offset_public_key = pk;
        self.recalculate_hash();
    }

    pub fn metadata_signature(&self) -> &ComAndPubSignature {
        &self.metadata_signature
    }

    pub fn set_metadata_signature(&mut self, sig: ComAndPubSignature) {
        self.input = OnceLock::new();
        self.output = OnceLock::new();
        self.metadata_signature = sig;
        self.recalculate_hash();
    }

    pub fn script_lock_height(&self) -> u64 {
        self.script_lock_height
    }

    pub fn set_script_lock_height(&mut self, height: u64) {
        self.script_lock_height = height;
    }

    pub fn encrypted_data(&self) -> &EncryptedData {
        &self.encrypted_data
    }

    pub fn minimum_value_promise(&self) -> MicroMinotari {
        self.minimum_value_promise
    }

    pub fn set_minimum_value_promise(&mut self, value: MicroMinotari) {
        self.input = OnceLock::new();
        self.output = OnceLock::new();
        self.minimum_value_promise = value;
        self.recalculate_hash();
    }

    pub fn range_proof(&self) -> &Option<RangeProof> {
        &self.range_proof
    }

    pub fn set_range_proof(&mut self, proof: Option<RangeProof>) {
        self.input = OnceLock::new();
        self.output = OnceLock::new();
        self.range_proof = proof;
        self.recalculate_hash();
    }

    pub fn payment_id(&self) -> &MemoField {
        &self.payment_id
    }

    pub fn output_hash(&self) -> FixedHash {
        self.output_hash
    }

    pub fn commitment(&self) -> &CompressedCommitment {
        &self.commitment
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
            .field("commitment_mask_key_id", &self.commitment_mask_key_id)
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
