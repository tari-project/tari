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

use blake2::Digest;
use serde::{Deserialize, Serialize};
use tari_common_types::types::{
    Challenge,
    ComSignature,
    Commitment,
    CommitmentFactory,
    HashDigest,
    HashOutput,
    PublicKey,
};
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    script::{ExecutionStack, ScriptContext, StackItem, TariScript},
    tari_utilities::{hex::Hex, ByteArray, Hashable},
};

use crate::transactions::transaction::{
    transaction_output::TransactionOutput,
    OutputFeatures,
    TransactionError,
    UnblindedOutput,
};

/// A transaction input.
///
/// Primarily a reference to an output being spent by the transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionInput {
    /// Either the hash of TransactionOutput that this Input is spending or its data
    pub spent_output: SpentOutput,
    /// The script input data, if any
    pub input_data: ExecutionStack,
    /// A signature with k_s, signing the script, input data, and mined height
    pub script_signature: ComSignature,
}

/// An input for a transaction that spends an existing output
impl TransactionInput {
    /// Create a new Transaction Input with just a reference hash of the spent output
    pub fn new_with_output_hash(
        output_hash: HashOutput,
        input_data: ExecutionStack,
        script_signature: ComSignature,
    ) -> TransactionInput {
        TransactionInput {
            spent_output: SpentOutput::OutputHash(output_hash),
            input_data,
            script_signature,
        }
    }

    /// Create a new Transaction Input with just a reference hash of the spent output
    pub fn new_with_output_data(
        features: OutputFeatures,
        commitment: Commitment,
        script: TariScript,
        input_data: ExecutionStack,
        script_signature: ComSignature,
        sender_offset_public_key: PublicKey,
    ) -> TransactionInput {
        TransactionInput {
            spent_output: SpentOutput::OutputData {
                features,
                commitment,
                script,
                sender_offset_public_key,
            },
            input_data,
            script_signature,
        }
    }

    /// Populate the spent output data fields
    pub fn add_output_data(
        &mut self,
        features: OutputFeatures,
        commitment: Commitment,
        script: TariScript,
        sender_offset_public_key: PublicKey,
    ) {
        self.spent_output = SpentOutput::OutputData {
            features,
            commitment,
            script,
            sender_offset_public_key,
        };
    }

    pub fn build_script_challenge(
        nonce_commitment: &Commitment,
        script: &TariScript,
        input_data: &ExecutionStack,
        script_public_key: &PublicKey,
        commitment: &Commitment,
    ) -> Vec<u8> {
        Challenge::new()
            .chain(nonce_commitment.as_bytes())
            .chain(script.as_bytes().as_slice())
            .chain(input_data.as_bytes().as_slice())
            .chain(script_public_key.as_bytes())
            .chain(commitment.as_bytes())
            .finalize()
            .to_vec()
    }

    pub fn commitment(&self) -> Result<&Commitment, TransactionError> {
        match self.spent_output {
            SpentOutput::OutputHash(_) => Err(TransactionError::MissingTransactionInputData),
            SpentOutput::OutputData { ref commitment, .. } => Ok(commitment),
        }
    }

    pub fn features(&self) -> Result<&OutputFeatures, TransactionError> {
        match self.spent_output {
            SpentOutput::OutputHash(_) => Err(TransactionError::MissingTransactionInputData),
            SpentOutput::OutputData { ref features, .. } => Ok(features),
        }
    }

    pub fn script(&self) -> Result<&TariScript, TransactionError> {
        match self.spent_output {
            SpentOutput::OutputHash(_) => Err(TransactionError::MissingTransactionInputData),
            SpentOutput::OutputData { ref script, .. } => Ok(script),
        }
    }

    pub fn sender_offset_public_key(&self) -> Result<&PublicKey, TransactionError> {
        match self.spent_output {
            SpentOutput::OutputHash(_) => Err(TransactionError::MissingTransactionInputData),
            SpentOutput::OutputData {
                ref sender_offset_public_key,
                ..
            } => Ok(sender_offset_public_key),
        }
    }

    /// Checks if the given un-blinded input instance corresponds to this blinded Transaction Input
    pub fn opened_by(&self, input: &UnblindedOutput, factory: &CommitmentFactory) -> Result<bool, TransactionError> {
        match self.spent_output {
            SpentOutput::OutputHash(_) => Err(TransactionError::MissingTransactionInputData),
            SpentOutput::OutputData { ref commitment, .. } => {
                Ok(factory.open(&input.spending_key, &input.value.into(), commitment))
            },
        }
    }

    /// This will check if the input and the output is the same transactional output by looking at the commitment and
    /// features and script. This will ignore all other output and input fields
    pub fn is_equal_to(&self, output: &TransactionOutput) -> bool {
        self.output_hash() == output.hash()
    }

    /// This will run the script contained in the TransactionInput, returning either a script error or the resulting
    /// public key.
    pub fn run_script(&self, context: Option<ScriptContext>) -> Result<PublicKey, TransactionError> {
        let context = context.unwrap_or_default();

        match self.spent_output {
            SpentOutput::OutputHash(_) => Err(TransactionError::MissingTransactionInputData),
            SpentOutput::OutputData { ref script, .. } => {
                match script.execute_with_context(&self.input_data, &context)? {
                    StackItem::PublicKey(pubkey) => Ok(pubkey),
                    _ => Err(TransactionError::ScriptExecutionError(
                        "The script executed successfully but it did not leave a public key on the stack".to_string(),
                    )),
                }
            },
        }
    }

    pub fn validate_script_signature(
        &self,
        public_script_key: &PublicKey,
        factory: &CommitmentFactory,
    ) -> Result<(), TransactionError> {
        match self.spent_output {
            SpentOutput::OutputHash(_) => Err(TransactionError::MissingTransactionInputData),
            SpentOutput::OutputData {
                ref script,
                ref commitment,
                ..
            } => {
                let challenge = TransactionInput::build_script_challenge(
                    self.script_signature.public_nonce(),
                    script,
                    &self.input_data,
                    public_script_key,
                    commitment,
                );
                if self
                    .script_signature
                    .verify_challenge(&(commitment + public_script_key), &challenge, factory)
                {
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
    /// from the script.
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
    pub fn is_mature_at(&self, block_height: u64) -> Result<bool, TransactionError> {
        match self.spent_output {
            SpentOutput::OutputHash(_) => Err(TransactionError::MissingTransactionInputData),
            SpentOutput::OutputData { ref features, .. } => Ok(features.maturity <= block_height),
        }
    }

    /// Returns the hash of the output data contained in this input.
    /// This hash matches the hash of a transaction output that this input spends.
    pub fn output_hash(&self) -> Vec<u8> {
        match self.spent_output {
            SpentOutput::OutputHash(ref h) => h.clone(),
            SpentOutput::OutputData {
                ref commitment,
                ref script,
                ref features,
                ..
            } => HashDigest::new()
                .chain(features.to_v1_bytes())
                .chain(commitment.as_bytes())
                .chain(script.as_bytes())
                .finalize()
                .to_vec(),
        }
    }

    pub fn is_compact(&self) -> bool {
        matches!(self.spent_output, SpentOutput::OutputHash(_))
    }

    /// Implement the canonical hashing function for TransactionInput for use in ordering
    pub fn canonical_hash(&self) -> Result<Vec<u8>, TransactionError> {
        match self.spent_output {
            SpentOutput::OutputHash(_) => Err(TransactionError::MissingTransactionInputData),
            SpentOutput::OutputData {
                ref features,
                ref commitment,
                ref script,
                ref sender_offset_public_key,
            } => Ok(HashDigest::new()
                .chain(features.to_v1_bytes())
                .chain(commitment.as_bytes())
                .chain(script.as_bytes())
                .chain(sender_offset_public_key.as_bytes())
                .chain(self.script_signature.u().as_bytes())
                .chain(self.script_signature.v().as_bytes())
                .chain(self.script_signature.public_nonce().as_bytes())
                .chain(self.input_data.as_bytes())
                .finalize()
                .to_vec()),
        }
    }

    pub fn set_maturity(&mut self, maturity: u64) -> Result<(), TransactionError> {
        if let SpentOutput::OutputData { ref mut features, .. } = self.spent_output {
            features.maturity = maturity;
            Ok(())
        } else {
            Err(TransactionError::MissingTransactionInputData)
        }
    }

    /// Return a clone of this Input into its compact form
    pub fn to_compact(&self) -> Self {
        Self {
            spent_output: match &self.spent_output {
                SpentOutput::OutputHash(h) => SpentOutput::OutputHash(h.clone()),
                SpentOutput::OutputData { .. } => SpentOutput::OutputHash(self.output_hash()),
            },
            input_data: self.input_data.clone(),
            script_signature: self.script_signature.clone(),
        }
    }
}

impl Display for TransactionInput {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self.spent_output {
            SpentOutput::OutputHash(ref h) => write!(fmt, "Input spending Output hash: {}", h.to_hex()),
            SpentOutput::OutputData {
                ref commitment,
                ref script,
                ref features,
                ref sender_offset_public_key,
            } => write!(
                fmt,
                "{} [{:?}], Script hash: ({}), Offset_Pubkey: ({})",
                commitment.to_hex(),
                features,
                script,
                sender_offset_public_key.to_hex()
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
        self.output_hash().partial_cmp(&other.output_hash())
    }
}

impl Ord for TransactionInput {
    fn cmp(&self, other: &Self) -> Ordering {
        self.output_hash().cmp(&other.output_hash())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant)]
pub enum SpentOutput {
    OutputHash(HashOutput),
    OutputData {
        features: OutputFeatures,
        commitment: Commitment,
        script: TariScript,
        sender_offset_public_key: PublicKey,
    },
}
