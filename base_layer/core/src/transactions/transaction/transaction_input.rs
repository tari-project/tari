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
    fmt::{self, Display, Formatter},
};

use blake2::Digest;
use serde::{
    de::{self, Deserializer, MapAccess, SeqAccess, Visitor},
    ser::SerializeStruct,
    Deserialize,
    Serialize,
    Serializer,
};
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
    tari_utilities::{hex::Hex, ByteArray},
};

use crate::{
    consensus::ToConsensusBytes,
    covenants::Covenant,
    transactions::transaction::{
        transaction_output::TransactionOutput,
        OutputFeatures,
        TransactionError,
        UnblindedOutput,
    },
};

/// A transaction input.
///
/// Primarily a reference to an output being spent by the transaction.
#[derive(Debug, Clone)]
pub struct TransactionInput {
    pub version: u8,
    /// Either the hash of TransactionOutput that this Input is spending or its data
    pub spent_output: SpentOutput,
    /// The script input data, if any
    pub input_data: ExecutionStack,
    /// A signature with k_s, signing the script, input data, and mined height
    pub script_signature: ComSignature,
}

/// An input for a transaction that spends an existing output
impl TransactionInput {
    pub fn new(
        spent_output: SpentOutput,
        input_data: ExecutionStack,
        script_signature: ComSignature,
    ) -> TransactionInput {
        TransactionInput {
            version: 0,
            spent_output,
            input_data,
            script_signature,
        }
    }

    /// Create a new Transaction Input with just a reference hash of the spent output
    pub fn new_with_output_hash(
        output_hash: HashOutput,
        input_data: ExecutionStack,
        script_signature: ComSignature,
    ) -> TransactionInput {
        TransactionInput::new(SpentOutput::OutputHash(output_hash), input_data, script_signature)
    }

    /// Create a new Transaction Input with just a reference hash of the spent output
    pub fn new_with_output_data(
        features: OutputFeatures,
        commitment: Commitment,
        script: TariScript,
        input_data: ExecutionStack,
        script_signature: ComSignature,
        sender_offset_public_key: PublicKey,
        covenant: Covenant,
    ) -> TransactionInput {
        TransactionInput::new(
            SpentOutput::OutputData {
                features,
                commitment,
                script,
                sender_offset_public_key,
                covenant,
            },
            input_data,
            script_signature,
        )
    }

    /// Populate the spent output data fields
    pub fn add_output_data(
        &mut self,
        features: OutputFeatures,
        commitment: Commitment,
        script: TariScript,
        sender_offset_public_key: PublicKey,
        covenant: Covenant,
    ) {
        self.spent_output = SpentOutput::OutputData {
            features,
            commitment,
            script,
            sender_offset_public_key,
            covenant,
        };
    }

    pub(super) fn build_script_challenge(
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

    pub fn features_mut(&mut self) -> Result<&mut OutputFeatures, TransactionError> {
        match self.spent_output {
            SpentOutput::OutputHash(_) => Err(TransactionError::MissingTransactionInputData),
            SpentOutput::OutputData { ref mut features, .. } => Ok(features),
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

    pub fn covenant(&self) -> Result<&Covenant, TransactionError> {
        match self.spent_output {
            SpentOutput::OutputHash(_) => Err(TransactionError::MissingTransactionInputData),
            SpentOutput::OutputData { ref covenant, .. } => Ok(covenant),
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
        match output.try_hash() {
            Ok(hash) => self.output_hash() == Ok(hash),
            Err(_e) => false,
        }
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
    pub fn output_hash(&self) -> Result<Vec<u8>, String> {
        match self.version {
            0 => match self.spent_output {
                SpentOutput::OutputHash(ref h) => Ok(h.clone()),
                SpentOutput::OutputData {
                    ref commitment,
                    ref script,
                    ref features,
                    ref covenant,
                    ..
                } => Ok(HashDigest::new()
                    .chain(features.to_consensus_bytes())
                    .chain(commitment.as_bytes())
                    .chain(script.as_bytes())
                    .chain(covenant.to_consensus_bytes())
                    .finalize()
                    .to_vec()),
            },
            _ => Err("Newer version!".to_string()),
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
                ref covenant,
            } => Ok(HashDigest::new()
                .chain(features.to_consensus_bytes())
                .chain(commitment.to_consensus_bytes())
                .chain(script.as_bytes())
                .chain(sender_offset_public_key.to_consensus_bytes())
                .chain(self.script_signature.u().to_consensus_bytes())
                .chain(self.script_signature.v().to_consensus_bytes())
                .chain(self.script_signature.public_nonce().to_consensus_bytes())
                .chain(self.input_data.as_bytes())
                .chain(covenant.to_consensus_bytes())
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
    pub fn to_compact(&self) -> Result<Self, String> {
        Ok(Self::new(
            match &self.spent_output {
                SpentOutput::OutputHash(h) => SpentOutput::OutputHash(h.clone()),
                SpentOutput::OutputData { .. } => SpentOutput::OutputHash(self.output_hash()?),
            },
            self.input_data.clone(),
            self.script_signature.clone(),
        ))
    }
}

impl Serialize for TransactionInput {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
        let mut state = serializer.serialize_struct("TransactionInput", 4)?;
        state.serialize_field("version", &self.version)?;
        state.serialize_field("spent_output", &self.spent_output)?;
        state.serialize_field("input_data", &self.input_data)?;
        state.serialize_field("script_signature", &self.script_signature)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for TransactionInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: Deserializer<'de> {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "snake_case")]
        enum Field {
            Version,
            SpentOutput,
            InputData,
            ScriptSignature,
        }

        struct TransactionInputVisitor;

        impl<'de> Visitor<'de> for TransactionInputVisitor {
            type Value = TransactionInput;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("struct TransactionInput")
            }

            fn visit_seq<V>(self, mut seq: V) -> Result<TransactionInput, V::Error>
            where V: SeqAccess<'de> {
                let version: u8 = seq.next_element()?.ok_or_else(|| de::Error::invalid_length(0, &self))?;
                match version {
                    0 => {
                        let spent_output = seq.next_element()?.ok_or_else(|| de::Error::invalid_length(1, &self))?;
                        let input_data = seq.next_element()?.ok_or_else(|| de::Error::invalid_length(2, &self))?;
                        let script_signature =
                            seq.next_element()?.ok_or_else(|| de::Error::invalid_length(3, &self))?;
                        Ok(TransactionInput::new(spent_output, input_data, script_signature))
                    },
                    _ => Err(de::Error::invalid_value(
                        de::Unexpected::Str("new version needs implementing!"),
                        &self,
                    )),
                }
            }

            fn visit_map<V>(self, mut map: V) -> Result<TransactionInput, V::Error>
            where V: MapAccess<'de> {
                let mut version = None;
                let mut spent_output = None;
                let mut input_data = None;
                let mut script_signature = None;
                while let Some(key) = map.next_key()? {
                    match key {
                        Field::Version => {
                            if version.is_some() {
                                return Err(de::Error::duplicate_field("version"));
                            }
                            version = Some(map.next_value()?);
                        },
                        Field::SpentOutput => {
                            if spent_output.is_some() {
                                return Err(de::Error::duplicate_field("spent_output"));
                            }
                            spent_output = Some(map.next_value()?);
                        },
                        Field::InputData => {
                            if input_data.is_some() {
                                return Err(de::Error::duplicate_field("input_data"));
                            }
                            input_data = Some(map.next_value()?);
                        },
                        Field::ScriptSignature => {
                            if script_signature.is_some() {
                                return Err(de::Error::duplicate_field("script_signature"));
                            }
                            script_signature = Some(map.next_value()?);
                        },
                    }
                }
                let version: u8 = version.ok_or_else(|| de::Error::missing_field("version"))?;
                match version {
                    0 => {
                        let spent_output = spent_output.ok_or_else(|| de::Error::missing_field("spent_output"))?;
                        let input_data = input_data.ok_or_else(|| de::Error::missing_field("input_data"))?;
                        let script_signature =
                            script_signature.ok_or_else(|| de::Error::missing_field("script_signature"))?;
                        Ok(TransactionInput::new(spent_output, input_data, script_signature))
                    },
                    _ => Err(de::Error::invalid_value(
                        de::Unexpected::Str("new version needs implementing!"),
                        &self,
                    )),
                }
            }
        }

        const FIELDS: &'static [&'static str] = &["version", "spent_output", "input_data", "script_signature"];
        deserializer.deserialize_struct("TransactionInput", FIELDS, TransactionInputVisitor)
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
                ..
            } => write!(
                fmt,
                "{} [{:?}], Script: ({}), Offset_Pubkey: ({})",
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
        /// The transaction covenant
        covenant: Covenant,
    },
}
