//  Copyright 2021, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{
    any::Any,
    fmt::{Display, Formatter},
    io,
    iter::FromIterator,
};

use digest::Digest;
use integer_encoding::VarIntWriter;
use tari_common_types::types::HashDigest;

use crate::{
    consensus::ToConsensusBytes,
    covenants::{
        byte_codes,
        decoder::{CovenantDecodeError, CovenantReadExt},
        encoder::CovenentWriteExt,
        error::CovenantError,
    },
    transactions::transaction_components::{TransactionInput, TransactionOutput},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum OutputField {
    Commitment = byte_codes::FIELD_COMMITMENT,
    Script = byte_codes::FIELD_SCRIPT,
    SenderOffsetPublicKey = byte_codes::FIELD_SENDER_OFFSET_PUBLIC_KEY,
    Covenant = byte_codes::FIELD_COVENANT,
    Features = byte_codes::FIELD_FEATURES,
    FeaturesFlags = byte_codes::FIELD_FEATURES_FLAGS,
    FeaturesMaturity = byte_codes::FIELD_FEATURES_MATURITY,
    FeaturesUniqueId = byte_codes::FIELD_FEATURES_UNIQUE_ID,
    FeaturesParentPublicKey = byte_codes::FIELD_FEATURES_PARENT_PUBLIC_KEY,
    FeaturesMetadata = byte_codes::FIELD_FEATURES_METADATA,
}

impl OutputField {
    pub fn from_byte(byte: u8) -> Result<Self, CovenantDecodeError> {
        use byte_codes::*;
        #[allow(clippy::enum_glob_use)]
        use OutputField::*;
        match byte {
            FIELD_COMMITMENT => Ok(Commitment),
            FIELD_SCRIPT => Ok(Script),
            FIELD_SENDER_OFFSET_PUBLIC_KEY => Ok(SenderOffsetPublicKey),
            FIELD_COVENANT => Ok(Covenant),
            FIELD_FEATURES => Ok(Features),
            FIELD_FEATURES_FLAGS => Ok(FeaturesFlags),
            FIELD_FEATURES_MATURITY => Ok(FeaturesMaturity),
            FIELD_FEATURES_UNIQUE_ID => Ok(FeaturesUniqueId),
            FIELD_FEATURES_PARENT_PUBLIC_KEY => Ok(FeaturesParentPublicKey),
            FIELD_FEATURES_METADATA => Ok(FeaturesMetadata),

            _ => Err(CovenantDecodeError::UnknownByteCode { code: byte }),
        }
    }

    pub fn as_byte(self) -> u8 {
        self as u8
    }

    pub fn get_field_value_ref<'a, T: 'static>(self, output: &'a TransactionOutput) -> Option<&'a T> {
        #[allow(clippy::enum_glob_use)]
        use OutputField::*;
        let val = match self {
            Commitment => &output.commitment as &dyn Any,
            Script => &output.script as &dyn Any,
            SenderOffsetPublicKey => &output.sender_offset_public_key as &dyn Any,
            Covenant => &output.covenant as &dyn Any,
            Features => &output.features as &dyn Any,
            FeaturesFlags => &output.features.flags as &dyn Any,
            FeaturesMaturity => &output.features.maturity as &dyn Any,
            FeaturesUniqueId => &output.features.unique_id as &dyn Any,
            FeaturesParentPublicKey => &output.features.parent_public_key as &dyn Any,
            FeaturesMetadata => &output.features.metadata as &dyn Any,
        };
        val.downcast_ref::<T>()
    }

    pub fn get_field_value_bytes(self, output: &TransactionOutput) -> Vec<u8> {
        #[allow(clippy::enum_glob_use)]
        use OutputField::*;
        match self {
            Commitment => output.commitment.to_consensus_bytes(),
            Script => output.script.to_consensus_bytes(),
            SenderOffsetPublicKey => output.sender_offset_public_key.to_consensus_bytes(),
            Covenant => output.covenant.to_consensus_bytes(),
            Features => output.features.to_consensus_bytes(),
            FeaturesFlags => output.features.flags.to_consensus_bytes(),
            FeaturesMaturity => output.features.maturity.to_consensus_bytes(),
            FeaturesUniqueId => output.features.unique_id.to_consensus_bytes(),
            FeaturesParentPublicKey => output.features.parent_public_key.to_consensus_bytes(),
            FeaturesMetadata => output.features.metadata.to_consensus_bytes(),
        }
    }

    pub fn is_eq_input(self, input: &TransactionInput, output: &TransactionOutput) -> bool {
        #[allow(clippy::enum_glob_use)]
        use OutputField::*;
        match self {
            Commitment => input
                .commitment()
                .map(|commitment| *commitment == output.commitment)
                .unwrap_or(false),
            Script => input.script().map(|script| *script == output.script).unwrap_or(false),
            SenderOffsetPublicKey => input
                .sender_offset_public_key()
                .map(|sender_offset_public_key| *sender_offset_public_key == output.sender_offset_public_key)
                .unwrap_or(false),
            Covenant => input
                .covenant()
                .map(|covenant| *covenant == output.covenant)
                .unwrap_or(false),
            Features => input
                .features()
                .map(|features| *features == output.features)
                .unwrap_or(false),
            FeaturesFlags => input
                .features()
                .map(|features| features.flags == output.features.flags)
                .unwrap_or(false),
            FeaturesMaturity => input
                .features()
                .map(|features| features.maturity == output.features.maturity)
                .unwrap_or(false),
            FeaturesUniqueId => input
                .features()
                .map(|features| features.unique_id == output.features.unique_id)
                .unwrap_or(false),
            FeaturesParentPublicKey => input
                .features()
                .map(|features| features.parent_public_key == output.features.parent_public_key)
                .unwrap_or(false),
            FeaturesMetadata => input
                .features()
                .map(|features| features.metadata == output.features.metadata)
                .unwrap_or(false),
        }
    }

    pub fn is_eq<T: PartialEq + 'static>(self, output: &TransactionOutput, val: &T) -> Result<bool, CovenantError> {
        use OutputField::{FeaturesParentPublicKey, FeaturesUniqueId};
        match self {
            // Handle edge cases
            FeaturesParentPublicKey | FeaturesUniqueId => match self.get_field_value_ref::<Option<T>>(output) {
                Some(Some(field_val)) => Ok(field_val == val),
                Some(None) => Ok(false),
                None => Err(CovenantError::InvalidArgument {
                    filter: "is_eq",
                    details: format!("Invalid type for field {}", self),
                }),
            },
            _ => match self.get_field_value_ref::<T>(output) {
                Some(field_val) => Ok(field_val == val),
                None => Err(CovenantError::InvalidArgument {
                    filter: "is_eq",
                    details: format!("Invalid type for field {}", self),
                }),
            },
        }
    }

    //---------------------------------- Macro helpers --------------------------------------------//
    #[allow(dead_code)]
    pub fn commitment() -> Self {
        OutputField::Commitment
    }

    #[allow(dead_code)]
    pub fn script() -> Self {
        OutputField::Script
    }

    #[allow(dead_code)]
    pub fn sender_offset_public_key() -> Self {
        OutputField::SenderOffsetPublicKey
    }

    #[allow(dead_code)]
    pub fn covenant() -> Self {
        OutputField::Covenant
    }

    #[allow(dead_code)]
    pub fn features() -> Self {
        OutputField::Features
    }

    #[allow(dead_code)]
    pub fn features_flags() -> Self {
        OutputField::FeaturesFlags
    }

    #[allow(dead_code)]
    pub fn features_maturity() -> Self {
        OutputField::FeaturesMaturity
    }

    #[allow(dead_code)]
    pub fn features_unique_id() -> Self {
        OutputField::FeaturesUniqueId
    }

    #[allow(dead_code)]
    pub fn features_parent_public_key() -> Self {
        OutputField::FeaturesParentPublicKey
    }

    #[allow(dead_code)]
    pub fn features_metadata() -> Self {
        OutputField::FeaturesMetadata
    }
}

impl Display for OutputField {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        #[allow(clippy::enum_glob_use)]
        use OutputField::*;
        match self {
            Commitment => write!(f, "field::commitment"),
            SenderOffsetPublicKey => write!(f, "field::sender_offset_public_key"),
            Script => write!(f, "field::script"),
            Covenant => write!(f, "field::covenant"),
            Features => write!(f, "field::features"),
            FeaturesFlags => write!(f, "field::features_flags"),
            FeaturesUniqueId => write!(f, "field::features_unique_id"),
            FeaturesMetadata => write!(f, "field::features_metadata"),
            FeaturesParentPublicKey => write!(f, "field::features_parent_public_key"),
            FeaturesMaturity => write!(f, "field::features_maturity"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OutputFields {
    fields: Vec<OutputField>,
}

impl OutputFields {
    /// The number of unique fields available. This always matches the number of variants in `OutputField`.
    pub const NUM_FIELDS: usize = 10;

    pub fn new() -> Self {
        Default::default()
    }

    pub fn push(&mut self, field: OutputField) {
        self.fields.push(field);
    }

    pub fn read_from<R: io::Read>(reader: &mut R) -> Result<Self, CovenantDecodeError> {
        // Each field is a byte
        let buf = reader.read_variable_length_bytes(Self::NUM_FIELDS)?;
        buf.iter().map(|byte| OutputField::from_byte(*byte)).collect()
    }

    pub fn write_to<W: io::Write>(&self, writer: &mut W) -> Result<usize, io::Error> {
        let len = self.fields.len();
        if len > Self::NUM_FIELDS {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "tried to write more than maximum number of fields",
            ));
        }
        let mut written = writer.write_varint(len)?;
        for byte in self.iter().map(|f| f.as_byte()) {
            written += writer.write_u8_fixed(byte)?;
        }
        Ok(written)
    }

    pub fn iter(&self) -> impl Iterator<Item = &OutputField> + '_ {
        self.fields.iter()
    }

    pub fn len(&self) -> usize {
        self.fields.len()
    }

    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    pub fn construct_challenge_from(&self, output: &TransactionOutput) -> HashDigest {
        let mut challenge = HashDigest::new();
        for field in &self.fields {
            challenge.update(field.get_field_value_bytes(output));
        }
        challenge
    }

    pub fn fields(&self) -> &[OutputField] {
        &self.fields
    }
}

impl From<Vec<OutputField>> for OutputFields {
    fn from(fields: Vec<OutputField>) -> Self {
        OutputFields { fields }
    }
}
impl FromIterator<OutputField> for OutputFields {
    fn from_iter<T: IntoIterator<Item = OutputField>>(iter: T) -> Self {
        Self {
            fields: iter.into_iter().collect(),
        }
    }
}

#[cfg(test)]
mod test {
    use tari_common_types::types::PublicKey;
    use tari_script::script;

    use super::*;
    use crate::{
        covenant,
        covenants::test::{create_input, create_outputs},
        transactions::{
            test_helpers::UtxoTestParams,
            transaction_components::{OutputFeatures, OutputFlags},
        },
    };

    mod output_field {
        use super::*;

        mod is_eq {
            use tari_common_types::types::Commitment;

            use super::*;

            #[test]
            fn it_returns_true_if_eq() {
                let output = create_outputs(1, UtxoTestParams {
                    features: OutputFeatures {
                        parent_public_key: Some(Default::default()),
                        unique_id: Some(b"1234".to_vec()),
                        ..Default::default()
                    },
                    script: script![Drop Nop],
                    ..Default::default()
                })
                .remove(0);

                assert!(OutputField::Commitment.is_eq(&output, &output.commitment).unwrap());
                assert!(OutputField::Features.is_eq(&output, &output.features).unwrap());
                assert!(OutputField::Script.is_eq(&output, &output.script).unwrap());
                assert!(OutputField::Covenant.is_eq(&output, &output.covenant).unwrap());
                assert!(OutputField::FeaturesMaturity
                    .is_eq(&output, &output.features.maturity)
                    .unwrap());
                assert!(OutputField::FeaturesFlags
                    .is_eq(&output, &output.features.flags)
                    .unwrap());
                assert!(OutputField::FeaturesParentPublicKey
                    .is_eq(&output, output.features.parent_public_key.as_ref().unwrap())
                    .unwrap());
                assert!(OutputField::FeaturesMetadata
                    .is_eq(&output, &output.features.metadata)
                    .unwrap());
                assert!(OutputField::FeaturesUniqueId
                    .is_eq(&output, output.features.unique_id.as_ref().unwrap())
                    .unwrap());
                assert!(OutputField::SenderOffsetPublicKey
                    .is_eq(&output, &output.sender_offset_public_key)
                    .unwrap());
            }

            #[test]
            fn it_returns_false_if_not_eq() {
                let output = create_outputs(1, UtxoTestParams {
                    features: OutputFeatures {
                        parent_public_key: Some(Default::default()),
                        unique_id: Some(b"1234".to_vec()),
                        ..Default::default()
                    },
                    script: script![Drop Nop],
                    ..Default::default()
                })
                .remove(0);

                assert!(!OutputField::Commitment.is_eq(&output, &Commitment::default()).unwrap());
                assert!(!OutputField::Features
                    .is_eq(&output, &OutputFeatures::default())
                    .unwrap());
                assert!(!OutputField::Script.is_eq(&output, &script![Nop Drop]).unwrap());
                assert!(!OutputField::Covenant
                    .is_eq(&output, &covenant!(and(identity(), identity())))
                    .unwrap());
                assert!(!OutputField::FeaturesMaturity.is_eq(&output, &123u64).unwrap());
                assert!(!OutputField::FeaturesFlags
                    .is_eq(&output, &OutputFlags::COINBASE_OUTPUT)
                    .unwrap());
                assert!(!OutputField::FeaturesParentPublicKey
                    .is_eq(&output, &PublicKey::default())
                    .unwrap());
                assert!(!OutputField::FeaturesMetadata.is_eq(&output, &[123u8]).unwrap());
                assert!(!OutputField::FeaturesUniqueId.is_eq(&output, &[123u8]).unwrap());
                assert!(!OutputField::SenderOffsetPublicKey
                    .is_eq(&output, &PublicKey::default())
                    .unwrap());
            }
        }

        mod is_eq_input {
            use super::*;
            use crate::transactions::transaction_components::SpentOutput;

            #[test]
            fn it_returns_true_if_eq_input() {
                let output = create_outputs(1, UtxoTestParams {
                    features: OutputFeatures {
                        maturity: 42,
                        ..Default::default()
                    },
                    script: script![Drop Nop],
                    ..Default::default()
                })
                .remove(0);
                let mut input = create_input();
                if let SpentOutput::OutputData {
                    features,
                    commitment,
                    script,
                    sender_offset_public_key,
                    covenant,
                    ..
                } = &mut input.spent_output
                {
                    *features = output.features.clone();
                    *commitment = output.commitment.clone();
                    *script = output.script.clone();
                    *sender_offset_public_key = output.sender_offset_public_key.clone();
                    *covenant = output.covenant.clone();
                }

                assert!(OutputField::Commitment.is_eq_input(&input, &output));
                assert!(OutputField::Features.is_eq_input(&input, &output));
                assert!(OutputField::Script.is_eq_input(&input, &output));
                assert!(OutputField::Covenant.is_eq_input(&input, &output));
                assert!(OutputField::FeaturesMaturity.is_eq_input(&input, &output));
                assert!(OutputField::FeaturesFlags.is_eq_input(&input, &output));
                assert!(OutputField::FeaturesParentPublicKey.is_eq_input(&input, &output));
                assert!(OutputField::FeaturesMetadata.is_eq_input(&input, &output));
                assert!(OutputField::FeaturesUniqueId.is_eq_input(&input, &output));
                assert!(OutputField::SenderOffsetPublicKey.is_eq_input(&input, &output));
            }
        }

        #[test]
        fn display() {
            let output_fields = [
                OutputField::Commitment,
                OutputField::Features,
                OutputField::FeaturesFlags,
                OutputField::FeaturesUniqueId,
                OutputField::FeaturesMetadata,
                OutputField::FeaturesMaturity,
                OutputField::FeaturesParentPublicKey,
                OutputField::SenderOffsetPublicKey,
                OutputField::Script,
                OutputField::Covenant,
            ];
            output_fields.iter().for_each(|f| {
                assert!(f.to_string().starts_with("field::"));
            })
        }
    }

    mod output_fields {
        use super::*;

        mod construct_challenge_from {
            use super::*;
            use crate::consensus::ConsensusEncoding;

            #[test]
            fn it_constructs_challenge_using_consensus_encoding() {
                let features = OutputFeatures {
                    maturity: 42,
                    flags: OutputFlags::COINBASE_OUTPUT,
                    ..Default::default()
                };
                let output = create_outputs(1, UtxoTestParams {
                    features,
                    script: script![Drop Nop],
                    ..Default::default()
                })
                .remove(0);

                let mut fields = OutputFields::new();
                fields.push(OutputField::Features);
                fields.push(OutputField::Commitment);
                fields.push(OutputField::Script);
                let hash = fields.construct_challenge_from(&output).finalize();

                let mut challenge = Vec::new();
                output.features.consensus_encode(&mut challenge).unwrap();
                output.commitment.consensus_encode(&mut challenge).unwrap();
                output.script.consensus_encode(&mut challenge).unwrap();
                let expected_hash = HashDigest::new().chain(&challenge).finalize();
                assert_eq!(hash, expected_hash);
            }
        }

        mod get_field_value_ref {
            use super::*;

            #[test]
            fn it_retrieves_the_value_as_ref() {
                let mut features = OutputFeatures {
                    maturity: 42,
                    ..Default::default()
                };
                let output = create_outputs(1, UtxoTestParams {
                    features: features.clone(),
                    ..Default::default()
                })
                .pop()
                .unwrap();
                features.set_recovery_byte(output.features.recovery_byte);
                let r = OutputField::Features.get_field_value_ref::<OutputFeatures>(&output);
                assert_eq!(*r.unwrap(), features);
            }
        }
    }
}
