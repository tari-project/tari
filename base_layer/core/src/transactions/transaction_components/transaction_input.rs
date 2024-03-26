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
    fmt::{Display, Formatter},
};

use blake2::Blake2b;
use borsh::{BorshDeserialize, BorshSerialize};
use digest::consts::{U32, U64};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use tari_common_types::types::{ComAndPubSignature, Commitment, CommitmentFactory, FixedHash, HashOutput, PublicKey};
use tari_crypto::tari_utilities::hex::Hex;
use tari_hashing::TransactionHashDomain;
use tari_script::{ExecutionStack, ScriptContext, StackItem, TariScript};

use super::{TransactionInputVersion, TransactionOutputVersion};
use crate::{
    consensus::DomainSeparatedConsensusHasher,
    covenants::Covenant,
    transactions::{
        tari_amount::MicroMinotari,
        transaction_components,
        transaction_components::{
            transaction_output::TransactionOutput,
            EncryptedData,
            OutputFeatures,
            TransactionError,
        },
    },
};

/// A transaction input.
///
/// Primarily a reference to an output being spent by the transaction.
#[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct TransactionInput {
    pub version: TransactionInputVersion,
    /// Either the hash of TransactionOutput that this Input is spending or its data
    pub spent_output: SpentOutput,
    /// The script input data, if any
    pub input_data: ExecutionStack,
    /// A signature with k_s, signing the script, input data, and mined height
    pub script_signature: ComAndPubSignature,
}

/// An input for a transaction that spends an existing output
impl TransactionInput {
    pub fn new(
        version: TransactionInputVersion,
        spent_output: SpentOutput,
        input_data: ExecutionStack,
        script_signature: ComAndPubSignature,
    ) -> TransactionInput {
        TransactionInput {
            version,
            spent_output,
            input_data,
            script_signature,
        }
    }

    pub fn new_current_version(
        spent_output: SpentOutput,
        input_data: ExecutionStack,
        script_signature: ComAndPubSignature,
    ) -> TransactionInput {
        TransactionInput::new(
            TransactionInputVersion::get_current_version(),
            spent_output,
            input_data,
            script_signature,
        )
    }

    /// Create a new Transaction Input with just a reference hash of the spent output
    pub fn new_with_output_hash(
        output_hash: HashOutput,
        input_data: ExecutionStack,
        script_signature: ComAndPubSignature,
    ) -> TransactionInput {
        TransactionInput::new_current_version(SpentOutput::OutputHash(output_hash), input_data, script_signature)
    }

    /// Create a new Transaction Input with just a reference hash of the spent output
    pub fn new_with_output_data(
        version: TransactionInputVersion,
        features: OutputFeatures,
        commitment: Commitment,
        script: TariScript,
        input_data: ExecutionStack,
        script_signature: ComAndPubSignature,
        sender_offset_public_key: PublicKey,
        covenant: Covenant,
        encrypted_data: EncryptedData,
        metadata_signature: ComAndPubSignature,
        rangeproof_hash: FixedHash,
        minimum_value_promise: MicroMinotari,
    ) -> TransactionInput {
        TransactionInput::new(
            version,
            SpentOutput::OutputData {
                features,
                commitment,
                script,
                sender_offset_public_key,
                covenant,
                version: TransactionOutputVersion::get_current_version(),
                encrypted_data,
                metadata_signature,
                rangeproof_hash,
                minimum_value_promise,
            },
            input_data,
            script_signature,
        )
    }

    /// Populate the spent output data fields
    pub fn add_output_data(
        &mut self,
        version: TransactionOutputVersion,
        features: OutputFeatures,
        commitment: Commitment,
        script: TariScript,
        sender_offset_public_key: PublicKey,
        covenant: Covenant,
        encrypted_data: EncryptedData,
        metadata_signature: ComAndPubSignature,
        rangeproof_hash: FixedHash,
        minimum_value_promise: MicroMinotari,
    ) {
        self.spent_output = SpentOutput::OutputData {
            version,
            features,
            commitment,
            script,
            sender_offset_public_key,
            covenant,
            encrypted_data,
            metadata_signature,
            rangeproof_hash,
            minimum_value_promise,
        };
    }

    /// Convenience function to create the entire script challenge
    pub fn build_script_signature_challenge(
        version: &TransactionInputVersion,
        ephemeral_commitment: &Commitment,
        ephemeral_pubkey: &PublicKey,
        script: &TariScript,
        input_data: &ExecutionStack,
        script_public_key: &PublicKey,
        commitment: &Commitment,
    ) -> [u8; 64] {
        // We build the message separately to help with hardware wallet support. This reduces the amount of data that
        // needs to be transferred in order to sign the signature.
        let message = TransactionInput::build_script_signature_message(version, script, input_data);
        TransactionInput::finalize_script_signature_challenge(
            version,
            ephemeral_commitment,
            ephemeral_pubkey,
            script_public_key,
            commitment,
            &message,
        )
    }

    /// Convenience function to create the finalize script challenge
    pub fn finalize_script_signature_challenge(
        version: &TransactionInputVersion,
        ephemeral_commitment: &Commitment,
        ephemeral_pubkey: &PublicKey,
        script_public_key: &PublicKey,
        commitment: &Commitment,
        message: &[u8; 32],
    ) -> [u8; 64] {
        match version {
            TransactionInputVersion::V0 | TransactionInputVersion::V1 => {
                DomainSeparatedConsensusHasher::<TransactionHashDomain, Blake2b<U64>>::new("script_challenge")
                    .chain(ephemeral_commitment)
                    .chain(ephemeral_pubkey)
                    .chain(script_public_key)
                    .chain(commitment)
                    .chain(&message)
                    .finalize()
                    .into()
            },
        }
    }

    /// Convenience function to create the entire script signature message for the challenge. This contains all data
    /// outside of the signing keys and nonces.
    pub fn build_script_signature_message(
        version: &TransactionInputVersion,
        script: &TariScript,
        input_data: &ExecutionStack,
    ) -> [u8; 32] {
        match version {
            TransactionInputVersion::V0 | TransactionInputVersion::V1 => {
                DomainSeparatedConsensusHasher::<TransactionHashDomain, Blake2b<U32>>::new("script_message")
                    .chain(version)
                    .chain(script)
                    .chain(input_data)
                    .finalize()
                    .into()
            },
        }
    }

    /// Returns the Commitment of this input. An error is returned if this is a compact input.
    pub fn commitment(&self) -> Result<&Commitment, TransactionError> {
        match self.spent_output {
            SpentOutput::OutputHash(_) => Err(TransactionError::CompactInputMissingData("commitment".to_string())),
            SpentOutput::OutputData { ref commitment, .. } => Ok(commitment),
        }
    }

    /// Returns the OutputFeatures of this input. An error is returned if this is a compact input.
    pub fn features(&self) -> Result<&OutputFeatures, TransactionError> {
        match self.spent_output {
            SpentOutput::OutputHash(_) => Err(TransactionError::CompactInputMissingData("features".to_string())),
            SpentOutput::OutputData { ref features, .. } => Ok(features),
        }
    }

    /// Returns a mutable reference OutputFeatures of this input. An error is returned if this is a compact input.
    /// This is only available for unit tests.
    #[cfg(test)]
    pub fn features_mut(&mut self) -> Result<&mut OutputFeatures, TransactionError> {
        match self.spent_output {
            SpentOutput::OutputHash(_) => Err(TransactionError::CompactInputMissingData("features".to_string())),
            SpentOutput::OutputData { ref mut features, .. } => Ok(features),
        }
    }

    /// Returns a reference to the TariScript of this input. An error is returned if this is a compact input.
    pub fn script(&self) -> Result<&TariScript, TransactionError> {
        match self.spent_output {
            SpentOutput::OutputHash(_) => Err(TransactionError::CompactInputMissingData("script".to_string())),
            SpentOutput::OutputData { ref script, .. } => Ok(script),
        }
    }

    /// Returns a reference to the sender offset public key of this input. An error is returned if this is a compact
    /// input.
    pub fn sender_offset_public_key(&self) -> Result<&PublicKey, TransactionError> {
        match self.spent_output {
            SpentOutput::OutputHash(_) => Err(TransactionError::CompactInputMissingData(
                "sender offset public key".to_string(),
            )),
            SpentOutput::OutputData {
                ref sender_offset_public_key,
                ..
            } => Ok(sender_offset_public_key),
        }
    }

    /// Returns a reference to the covenant of this input. An error is returned if this is a compact input.
    pub fn covenant(&self) -> Result<&Covenant, TransactionError> {
        match self.spent_output {
            SpentOutput::OutputHash(_) => Err(TransactionError::CompactInputMissingData("covenant".to_string())),
            SpentOutput::OutputData { ref covenant, .. } => Ok(covenant),
        }
    }

    /// Returns a reference to the EncryptedData of this input. An error is returned if this is a compact input.
    pub fn encrypted_data(&self) -> Result<&EncryptedData, TransactionError> {
        match self.spent_output {
            SpentOutput::OutputHash(_) => Err(TransactionError::CompactInputMissingData("encrypted data".to_string())),
            SpentOutput::OutputData { ref encrypted_data, .. } => Ok(encrypted_data),
        }
    }

    /// Returns a reference to the metadata signature of this input. An error is returned if this is a compact input.
    pub fn metadata_signature(&self) -> Result<&ComAndPubSignature, TransactionError> {
        match self.spent_output {
            SpentOutput::OutputHash(_) => Err(TransactionError::CompactInputMissingData(
                "metadata signature".to_string(),
            )),
            SpentOutput::OutputData {
                ref metadata_signature, ..
            } => Ok(metadata_signature),
        }
    }

    /// Returns a reference to the rangeproof hash of this input. An error is returned if this is a compact input.
    pub fn rangeproof_hash(&self) -> Result<&FixedHash, TransactionError> {
        match self.spent_output {
            SpentOutput::OutputHash(_) => Err(TransactionError::CompactInputMissingData("rangeproof hash".to_string())),
            SpentOutput::OutputData {
                ref rangeproof_hash, ..
            } => Ok(rangeproof_hash),
        }
    }

    /// Returns a reference to the minimum value promise of this input. An error is returned if this is a compact input.
    pub fn minimum_value_promise(&self) -> Result<&MicroMinotari, TransactionError> {
        match self.spent_output {
            SpentOutput::OutputHash(_) => Err(TransactionError::CompactInputMissingData(
                "minimum value promise".to_string(),
            )),
            SpentOutput::OutputData {
                ref minimum_value_promise,
                ..
            } => Ok(minimum_value_promise),
        }
    }

    /// This will check if the input and the output is the same transactional output by looking at the commitment and
    /// features and script. This will ignore all other output and input fields
    pub fn is_equal_to(&self, output: &TransactionOutput) -> bool {
        self.output_hash() == output.hash()
    }

    /// This will run the script contained in the TransactionInput, returning the resulting
    /// public key if execution succeeds, or otherwise a script error. An error is returned if this is a compact input.
    pub fn run_script(&self, context: Option<ScriptContext>) -> Result<PublicKey, TransactionError> {
        let context = context.unwrap_or_default();

        match self.spent_output {
            SpentOutput::OutputHash(_) => Err(TransactionError::CompactInputMissingData("script".to_string())),
            SpentOutput::OutputData { ref script, .. } => {
                match script.execute_with_context(&self.input_data, &context)? {
                    StackItem::PublicKey(pubkey) => Ok(pubkey),
                    item => Err(TransactionError::ScriptExecutionError(format!(
                        "The script executed successfully but it did not leave a public key on the stack. Remaining \
                         stack item was {:?}",
                        item
                    ))),
                }
            },
        }
    }

    /// Validates the script signature. An error is returned if the script signature is invalid or this is a compact
    /// input.
    pub fn validate_script_signature(
        &self,
        script_public_key: &PublicKey,
        factory: &CommitmentFactory,
    ) -> Result<(), TransactionError> {
        match self.spent_output {
            SpentOutput::OutputHash(_) => Err(TransactionError::CompactInputMissingData(
                "script signature".to_string(),
            )),
            SpentOutput::OutputData {
                ref script,
                ref commitment,
                ..
            } => {
                let challenge = TransactionInput::build_script_signature_challenge(
                    &self.version,
                    self.script_signature.ephemeral_commitment(),
                    self.script_signature.ephemeral_pubkey(),
                    script,
                    &self.input_data,
                    script_public_key,
                    commitment,
                );
                if self.script_signature.verify_challenge(
                    commitment,
                    script_public_key,
                    &challenge,
                    factory,
                    &mut OsRng,
                ) {
                    Ok(())
                } else {
                    Err(TransactionError::InvalidSignatureError(
                        "Verifying script signature".to_string(),
                    ))
                }
            },
        }
    }

    /// This will run the script and verify the script signature. If its valid, it will return the resulting public key
    /// from the script. An error is always returned if this is a compact input.
    pub fn run_and_verify_script(
        &self,
        factory: &CommitmentFactory,
        context: Option<ScriptContext>,
    ) -> Result<PublicKey, TransactionError> {
        let key = self.run_script(context)?;
        self.validate_script_signature(&key, factory)?;
        Ok(key)
    }

    /// Returns true if this input is mature at the given height, otherwise false
    /// An error is returned if this is a compact input.
    pub fn is_mature_at(&self, block_height: u64) -> Result<bool, TransactionError> {
        match self.spent_output {
            SpentOutput::OutputHash(_) => Err(TransactionError::CompactInputMissingData("features".to_string())),
            SpentOutput::OutputData { ref features, .. } => Ok(features.maturity <= block_height),
        }
    }

    /// Returns the hash of the output data contained in this input.
    /// This hash matches the hash of a transaction output that this input spends.
    pub fn output_hash(&self) -> FixedHash {
        match &self.spent_output {
            SpentOutput::OutputHash(ref h) => *h,
            SpentOutput::OutputData {
                version,
                commitment,
                script,
                features,
                covenant,
                encrypted_data,
                sender_offset_public_key,
                metadata_signature,
                rangeproof_hash,
                minimum_value_promise,
                ..
            } => transaction_components::hash_output(
                *version,
                features,
                commitment,
                rangeproof_hash,
                script,
                sender_offset_public_key,
                metadata_signature,
                covenant,
                encrypted_data,
                *minimum_value_promise,
            ),
        }
    }

    pub fn smt_hash(&self, mined_height: u64) -> FixedHash {
        let utxo_hash = self.output_hash();
        let smt_hash = DomainSeparatedConsensusHasher::<TransactionHashDomain, Blake2b<U32>>::new("smt_hash")
            .chain(&utxo_hash)
            .chain(&mined_height);

        match self.version {
            TransactionInputVersion::V0 | TransactionInputVersion::V1 => smt_hash.finalize().into(),
        }
    }

    /// Returns true if this is a compact input, otherwise false.
    pub fn is_compact(&self) -> bool {
        matches!(self.spent_output, SpentOutput::OutputHash(_))
    }

    /// Implement the canonical hashing function for TransactionInput for use in ordering
    pub fn canonical_hash(&self) -> FixedHash {
        let writer = DomainSeparatedConsensusHasher::<TransactionHashDomain, Blake2b<U32>>::new("transaction_input")
            .chain(&self.version)
            .chain(&self.script_signature)
            .chain(&self.input_data)
            .chain(&self.output_hash());

        writer.finalize().into()
    }

    /// Sets the input maturity. Only available in unit tests.
    /// An error is returned if this is a compact input.
    #[cfg(test)]
    pub fn set_maturity(&mut self, maturity: u64) -> Result<(), TransactionError> {
        if let SpentOutput::OutputData { ref mut features, .. } = self.spent_output {
            features.maturity = maturity;
            Ok(())
        } else {
            Err(TransactionError::CompactInputMissingData("features".to_string()))
        }
    }

    /// Sets the input's Tari script. Only useful in tests.
    /// An error is returned if this is a compact input.
    #[cfg(test)]
    pub fn set_script(&mut self, new_script: TariScript) -> Result<(), TransactionError> {
        if let SpentOutput::OutputData { ref mut script, .. } = self.spent_output {
            *script = new_script;
            Ok(())
        } else {
            Err(TransactionError::CompactInputMissingData("script".to_string()))
        }
    }

    /// Return a copy of this TransactionInput in its compact form.
    pub fn to_compact(&self) -> Self {
        Self::new(
            self.version,
            match &self.spent_output {
                SpentOutput::OutputHash(h) => SpentOutput::OutputHash(*h),
                SpentOutput::OutputData { .. } => SpentOutput::OutputHash(self.output_hash()),
            },
            self.input_data.clone(),
            self.script_signature.clone(),
        )
    }
}

impl Display for TransactionInput {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self.spent_output {
            SpentOutput::OutputHash(ref h) => write!(fmt, "Input spending Output hash: {}", h),
            SpentOutput::OutputData {
                ref commitment,
                ref script,
                ref features,
                ref sender_offset_public_key,
                ..
            } => write!(
                fmt,
                "({}, {}) [{:?}], Script: ({}), Input_data : ({}), Offset_Pubkey: ({}), Input Hash: {}",
                commitment.to_hex(),
                self.output_hash(),
                features,
                script,
                self.input_data.to_hex(),
                sender_offset_public_key.to_hex(),
                self.canonical_hash(),
            ),
        }
    }
}

impl PartialEq<Self> for TransactionInput {
    fn eq(&self, other: &Self) -> bool {
        self.output_hash() == other.output_hash() &&
            self.script_signature == other.script_signature &&
            self.input_data == other.input_data
    }
}

impl Eq for TransactionInput {}

impl PartialOrd for TransactionInput {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TransactionInput {
    fn cmp(&self, other: &Self) -> Ordering {
        self.output_hash().cmp(&other.output_hash())
    }
}

impl Default for TransactionInput {
    fn default() -> Self {
        let output = SpentOutput::create_from_output(TransactionOutput::default());

        TransactionInput::new_current_version(output, ExecutionStack::default(), Default::default())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[allow(clippy::large_enum_variant)]
pub enum SpentOutput {
    OutputHash(HashOutput),
    OutputData {
        version: TransactionOutputVersion,
        features: OutputFeatures,
        commitment: Commitment,
        script: TariScript,
        sender_offset_public_key: PublicKey,
        /// The transaction covenant
        covenant: Covenant,
        encrypted_data: EncryptedData,
        metadata_signature: ComAndPubSignature,
        rangeproof_hash: FixedHash,
        minimum_value_promise: MicroMinotari,
    },
}

impl SpentOutput {
    pub fn get_type(&self) -> u8 {
        match self {
            SpentOutput::OutputHash(_) => 0,
            SpentOutput::OutputData { .. } => 1,
        }
    }

    pub fn create_from_output(output: TransactionOutput) -> SpentOutput {
        let rp_hash = match output.proof {
            Some(proof) => proof.hash(),
            None => FixedHash::zero(),
        };
        SpentOutput::OutputData {
            version: output.version,
            features: output.features,
            commitment: output.commitment,
            script: output.script,
            sender_offset_public_key: output.sender_offset_public_key,
            covenant: output.covenant,
            encrypted_data: output.encrypted_data,
            metadata_signature: output.metadata_signature,
            rangeproof_hash: rp_hash,
            minimum_value_promise: output.minimum_value_promise,
        }
    }
}
