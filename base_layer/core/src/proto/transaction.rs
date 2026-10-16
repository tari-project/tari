// Copyright 2019, The Tari Project
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

//! Impls for transaction proto

use std::{
    convert::{TryFrom, TryInto},
    sync::Arc,
};

use borsh::BorshSerialize;
use tari_common_types::types::{BulletRangeProof, CompressedCommitment, CompressedPublicKey, PrivateKey};
use tari_crypto::tari_utilities::{ByteArray, ByteArrayError};
use tari_script::{ExecutionStack, TariScript};
use tari_transaction_components::{
    MicroMinotari,
    aggregated_body::AggregateBody,
    transaction_components::{
        CoinBaseExtra,
        EncryptedData,
        KernelFeatures,
        OutputFeatures,
        OutputFeaturesVersion,
        OutputType,
        RangeProofType,
        SideChainFeature,
        Transaction,
        TransactionInput,
        TransactionInputVersion,
        TransactionKernel,
        TransactionKernelVersion,
        TransactionOutput,
        TransactionOutputVersion,
        covenants::{Covenant, CovenantDecodeError, decode_borsh_covenant},
    },
};
use tari_utilities::convert::try_convert_all;

use crate::proto;

//---------------------------------- Conversion error --------------------------------------------//

/// The error converting a proto transaction, or one of its inputs, outputs or body, to its domain type. Its message is
/// the same as the string error these conversions returned before it existed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TransactionProtoConversionError {
    /// A full input or output covenant has more tokens than allowed. The message is the covenant decode error.
    #[error("{0}")]
    CovenantTokenLimit(String),
    #[error("{0}")]
    Other(String),
}

impl From<String> for TransactionProtoConversionError {
    fn from(err: String) -> Self {
        Self::Other(err)
    }
}

impl From<&str> for TransactionProtoConversionError {
    fn from(err: &str) -> Self {
        Self::Other(err.to_string())
    }
}

impl From<TransactionProtoConversionError> for String {
    fn from(err: TransactionProtoConversionError) -> Self {
        err.to_string()
    }
}

/// Decodes a borsh encoded covenant field, as `BorshDeserialize` does, reporting the token limit separately
fn decode_covenant_field(field: &[u8]) -> Result<Covenant, TransactionProtoConversionError> {
    let mut buffer = field;
    decode_borsh_covenant(&mut buffer).map_err(|err| match err {
        CovenantDecodeError::ExceededMaxTokens { .. } => {
            TransactionProtoConversionError::CovenantTokenLimit(err.to_string())
        },
        err => TransactionProtoConversionError::Other(err.to_string()),
    })
}

//---------------------------------- TransactionKernel --------------------------------------------//

impl TryFrom<proto::types::TransactionKernel> for TransactionKernel {
    type Error = String;

    fn try_from(kernel: proto::types::TransactionKernel) -> Result<Self, Self::Error> {
        let excess = CompressedCommitment::from_canonical_bytes(
            &kernel
                .excess
                .ok_or_else(|| "Excess not provided in kernel".to_string())?
                .data,
        )
        .map_err(|err| err.to_string())?;

        let excess_sig = kernel
            .excess_sig
            .ok_or_else(|| "excess_sig not provided".to_string())?
            .try_into()?;
        let kernel_features = u8::try_from(kernel.features).map_err(|_| "Kernel features must be a single byte")?;
        let commitment = match kernel.burn_commitment {
            Some(burn_commitment) => {
                Some(CompressedCommitment::from_canonical_bytes(&burn_commitment.data).map_err(|e| e.to_string())?)
            },
            None => None,
        };

        Ok(TransactionKernel::new(
            TransactionKernelVersion::try_from(
                u8::try_from(kernel.version).map_err(|_| "Invalid version: overflowed u8")?,
            )?,
            KernelFeatures::from_bits(kernel_features)
                .ok_or_else(|| "Invalid or unrecognised kernel feature flag".to_string())?,
            MicroMinotari::from(kernel.fee),
            kernel.lock_height,
            excess,
            excess_sig,
            commitment,
        ))
    }
}

impl From<TransactionKernel> for proto::types::TransactionKernel {
    fn from(kernel: TransactionKernel) -> Self {
        let commitment = kernel.burn_commitment.map(|commitment| commitment.into());
        Self {
            features: u32::from(kernel.features.bits()),
            excess: Some(kernel.excess.into()),
            excess_sig: Some(kernel.excess_sig.into()),
            fee: kernel.fee.into(),
            lock_height: kernel.lock_height,
            version: kernel.version as u32,
            burn_commitment: commitment,
        }
    }
}

//---------------------------------- TransactionInput --------------------------------------------//

impl TryFrom<proto::types::TransactionInput> for TransactionInput {
    type Error = TransactionProtoConversionError;

    fn try_from(input: proto::types::TransactionInput) -> Result<Self, Self::Error> {
        let script_signature = (*input
            .script_signature
            .ok_or_else(|| "script_signature not provided".to_string())?)
        .try_into()
        .map_err(|err: ByteArrayError| err.to_string())?;

        // Check if the received Transaction input is in compact form or not
        if let Some(commitment) = input.commitment {
            let commitment = CompressedCommitment::from_canonical_bytes(&commitment.data).map_err(|e| e.to_string())?;
            let features = input
                .features
                .map(|features| (*features).try_into())
                .ok_or_else(|| "transaction output features not provided".to_string())??;

            let sender_offset_public_key =
                CompressedPublicKey::from_canonical_bytes(input.sender_offset_public_key.as_bytes())
                    .map_err(|err| format!("{err:?}"))?;

            let metadata_signature = (*input
                .metadata_signature
                .ok_or_else(|| "Metadata signature not provided".to_string())?)
            .try_into()
            .map_err(|_| "Metadata signature could not be converted".to_string())?;
            let rangeproof_hash = input
                .rangeproof_hash
                .try_into()
                .map_err(|_| "Invalid rangeproof hash")?;

            Ok(TransactionInput::new_with_output_data(
                TransactionInputVersion::try_from(
                    u8::try_from(input.version).map_err(|_| "Invalid version: overflowed u8")?,
                )?,
                features,
                commitment,
                TariScript::from_bytes(input.script.as_slice()).map_err(|err| format!("{err:?}"))?,
                ExecutionStack::from_bytes(input.input_data.as_slice()).map_err(|err| format!("{err:?}"))?,
                script_signature,
                sender_offset_public_key,
                decode_covenant_field(&input.covenant)?,
                EncryptedData::from_bytes(&input.encrypted_data).map_err(|err| err.to_string())?,
                metadata_signature,
                rangeproof_hash,
                input.minimum_value_promise.into(),
            ))
        } else {
            if input.output_hash.is_empty() {
                return Err("Compact Transaction Input does not contain `output_hash`".into());
            }
            let hash = input.output_hash.try_into().map_err(|_| "Invalid transaction hash")?;
            Ok(TransactionInput::new_with_output_hash(
                hash,
                ExecutionStack::from_bytes(input.input_data.as_slice()).map_err(|err| format!("{err:?}"))?,
                script_signature,
            ))
        }
    }
}

impl TryFrom<TransactionInput> for proto::types::TransactionInput {
    type Error = String;

    fn try_from(input: TransactionInput) -> Result<Self, Self::Error> {
        if input.is_compact() {
            let output_hash = input.output_hash();
            Ok(Self {
                input_data: input.input_data.to_bytes(),
                script_signature: Some(Box::new(input.script_signature.into())),
                output_hash: output_hash.to_vec(),
                ..Default::default()
            })
        } else {
            let mut covenant = Vec::new();
            BorshSerialize::serialize(
                &input
                    .covenant()
                    .map_err(|_| "Non-compact Transaction input should contain covenant".to_string())?,
                &mut covenant,
            )
            .map_err(|err| err.to_string())?;
            Ok(Self {
                features: Some(Box::new(
                    input
                        .features()
                        .map_err(|_| "Non-compact Transaction input should contain features".to_string())?
                        .clone()
                        .into(),
                )),
                commitment: Some(
                    input
                        .commitment()
                        .map_err(|_| "Non-compact Transaction input should contain commitment".to_string())?
                        .clone()
                        .into(),
                ),
                script: input
                    .script()
                    .map_err(|_| "Non-compact Transaction input should contain script".to_string())?
                    .to_bytes(),
                input_data: input.input_data.to_bytes(),
                script_signature: Some(Box::new(input.script_signature.clone().into())),
                sender_offset_public_key: input
                    .sender_offset_public_key()
                    .map_err(|_| "Non-compact Transaction input should contain sender_offset_public_key".to_string())?
                    .as_bytes()
                    .to_vec(),
                // Output hash is only used in compact form
                output_hash: Vec::new(),
                covenant,
                version: input.version as u32,
                encrypted_data: input
                    .encrypted_data()
                    .map_err(|_| "Non-compact Transaction input should contain encrypted value".to_string())?
                    .to_byte_vec(),
                metadata_signature: Some(Box::new(
                    input
                        .metadata_signature()
                        .map_err(|_| "Non-compact Transaction input should contain a metadata_signature".to_string())?
                        .clone()
                        .into(),
                )),
                rangeproof_hash: input
                    .rangeproof_hash()
                    .map_err(|_| "Non-compact Transaction input should contain a rangeproof hash".to_string())?
                    .to_vec(),
                minimum_value_promise: input
                    .minimum_value_promise()
                    .map_err(|_| "Non-compact Transaction input should contain the minimum value promise".to_string())?
                    .as_u64(),
            })
        }
    }
}

//---------------------------------- TransactionOutput --------------------------------------------//

impl TryFrom<proto::types::TransactionOutput> for TransactionOutput {
    type Error = TransactionProtoConversionError;

    fn try_from(output: proto::types::TransactionOutput) -> Result<Self, Self::Error> {
        let features = output
            .features
            .map(|features| (*features).try_into())
            .ok_or_else(|| "Transaction output features not provided".to_string())??;

        let commitment = output
            .commitment
            .map(|commit| CompressedCommitment::from_canonical_bytes(&commit.data))
            .ok_or_else(|| "Transaction output commitment not provided".to_string())?
            .map_err(|err| err.to_string())?;

        let sender_offset_public_key =
            CompressedPublicKey::from_canonical_bytes(output.sender_offset_public_key.as_bytes())
                .map_err(|err| format!("{err:?}"))?;

        let range_proof = if let Some(proof) = output.range_proof {
            Some(BulletRangeProof::from_canonical_bytes(&proof.proof_bytes).map_err(|err| err.to_string())?)
        } else {
            None
        };

        let script = TariScript::from_bytes(&output.script).map_err(|err| err.to_string())?;

        let metadata_signature = (*output
            .metadata_signature
            .ok_or_else(|| "Metadata signature not provided".to_string())?)
        .try_into()
        .map_err(|_| "Metadata signature could not be converted".to_string())?;

        let covenant = decode_covenant_field(&output.covenant)?;

        let encrypted_data = EncryptedData::from_bytes(&output.encrypted_data).map_err(|err| err.to_string())?;

        let minimum_value_promise = output.minimum_value_promise.into();

        Ok(Self::new(
            TransactionOutputVersion::try_from(
                u8::try_from(output.version).map_err(|_| "Invalid version: overflowed u8")?,
            )?,
            features,
            commitment,
            range_proof,
            script,
            sender_offset_public_key,
            metadata_signature,
            covenant,
            encrypted_data,
            minimum_value_promise,
        ))
    }
}

impl TryFrom<TransactionOutput> for proto::types::TransactionOutput {
    type Error = String;

    fn try_from(output: TransactionOutput) -> Result<Self, Self::Error> {
        let mut covenant = Vec::new();
        BorshSerialize::serialize(&output.covenant, &mut covenant).map_err(|err| err.to_string())?;
        let range_proof = output.proof.map(|proof| proto::types::RangeProof {
            proof_bytes: proof.to_vec(),
        });
        Ok(Self {
            features: Some(Box::new(output.features.into())),
            commitment: Some(output.commitment.into()),
            range_proof,
            script: output.script.to_bytes(),
            sender_offset_public_key: output.sender_offset_public_key.as_bytes().to_vec(),
            metadata_signature: Some(Box::new(output.metadata_signature.into())),
            covenant,
            version: output.version as u32,
            encrypted_data: output.encrypted_data.to_byte_vec(),
            minimum_value_promise: output.minimum_value_promise.into(),
        })
    }
}

//---------------------------------- OutputFeatures --------------------------------------------//

impl TryFrom<proto::types::OutputFeatures> for OutputFeatures {
    type Error = String;

    fn try_from(features: proto::types::OutputFeatures) -> Result<Self, Self::Error> {
        let sidechain_feature = features
            .sidechain_feature
            .map(|feature| SideChainFeature::try_from(*feature))
            .transpose()?;

        let output_type = features
            .output_type
            .try_into()
            .map_err(|_| "Invalid output type: overflowed")?;

        let range_proof_type = features
            .range_proof_type
            .try_into()
            .map_err(|_| "Invalid range proof type: overflowed")?;

        Ok(OutputFeatures::new(
            OutputFeaturesVersion::try_from(
                u8::try_from(features.version).map_err(|_| "Invalid version: overflowed u8")?,
            )?,
            OutputType::from_byte(output_type).ok_or_else(|| "Invalid or unrecognised output type".to_string())?,
            features.maturity,
            CoinBaseExtra::try_from(features.coinbase_extra).map_err(|e| e.to_string())?,
            sidechain_feature,
            RangeProofType::from_byte(range_proof_type)
                .ok_or_else(|| "Invalid or unrecognised range proof type".to_string())?,
        ))
    }
}

impl From<OutputFeatures> for proto::types::OutputFeatures {
    fn from(features: OutputFeatures) -> Self {
        Self {
            output_type: u32::from(features.output_type.as_byte()),
            maturity: features.maturity,
            coinbase_extra: features.coinbase_extra.to_vec(),
            version: features.version as u32,
            sidechain_feature: features.sidechain_feature.map(|feature| Box::new(feature.into())),
            range_proof_type: u32::from(features.range_proof_type.as_byte()),
        }
    }
}

//---------------------------------- AggregateBody --------------------------------------------//

impl TryFrom<proto::types::AggregateBody> for AggregateBody {
    type Error = TransactionProtoConversionError;

    fn try_from(body: proto::types::AggregateBody) -> Result<Self, Self::Error> {
        let inputs = try_convert_all(body.inputs)?;
        let outputs = try_convert_all(body.outputs)?;
        let kernels = try_convert_all(body.kernels)?;
        let body = AggregateBody::new_unsorted(inputs, outputs, kernels);
        Ok(body)
    }
}

impl TryFrom<AggregateBody> for proto::types::AggregateBody {
    type Error = String;

    fn try_from(body: AggregateBody) -> Result<Self, Self::Error> {
        let (i, o, k) = body.dissolve();
        Ok(Self {
            inputs: i
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<proto::types::TransactionInput>, _>>()?,
            outputs: o
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<_>, String>>()?,
            kernels: k.into_iter().map(Into::into).collect(),
        })
    }
}

//----------------------------------- Transaction ---------------------------------------------//

impl TryFrom<proto::types::Transaction> for Transaction {
    type Error = TransactionProtoConversionError;

    fn try_from(tx: proto::types::Transaction) -> Result<Self, Self::Error> {
        let offset = tx
            .offset
            .map(|offset| PrivateKey::from_canonical_bytes(&offset.data))
            .ok_or_else(|| "Blinding factor offset not provided".to_string())?
            .map_err(|err| err.to_string())?;
        let body = tx
            .body
            .map(TryInto::try_into)
            .ok_or_else(|| "Body not provided".to_string())??;
        let script_offset = tx
            .script_offset
            .map(|script_offset| PrivateKey::from_canonical_bytes(&script_offset.data))
            .ok_or_else(|| "Script offset not provided".to_string())?
            .map_err(|err| err.to_string())?;

        Ok(Self {
            offset,
            body,
            script_offset,
        })
    }
}

impl TryFrom<Transaction> for proto::types::Transaction {
    type Error = String;

    fn try_from(tx: Transaction) -> Result<Self, Self::Error> {
        Ok(Self {
            offset: Some(tx.offset.into()),
            body: Some(tx.body.try_into()?),
            script_offset: Some(tx.script_offset.into()),
        })
    }
}

impl TryFrom<Arc<Transaction>> for proto::types::Transaction {
    type Error = String;

    fn try_from(tx: Arc<Transaction>) -> Result<Self, Self::Error> {
        match Arc::try_unwrap(tx) {
            Ok(tx) => Ok(tx.try_into()?),
            Err(tx) => Ok(Self {
                offset: Some(tx.offset.clone().into()),
                body: Some(tx.body.clone().try_into()?),
                script_offset: Some(tx.script_offset.clone().into()),
            }),
        }
    }
}

#[cfg(test)]
mod test {
    use borsh::BorshDeserialize;

    use super::*;
    use crate::mempool::test_utils::{covenant_over_token_limit, transaction_proto_with_covenant};

    fn borsh_field(covenant: &[u8]) -> Vec<u8> {
        let mut field = Vec::new();
        prost::encoding::encode_varint(covenant.len() as u64, &mut field);
        field.extend_from_slice(covenant);
        field
    }

    #[test]
    fn an_output_covenant_over_the_token_limit_is_reported_as_such() {
        let tx = transaction_proto_with_covenant(&covenant_over_token_limit());
        let err = Transaction::try_from(tx).unwrap_err();
        assert!(
            matches!(&err, TransactionProtoConversionError::CovenantTokenLimit(_)),
            "{err:?}"
        );
        // The message is the decode error, as before
        assert_eq!(err.to_string(), "Covenant exceeded the maximum of 128 tokens");

        for ok in [vec![], vec![0x20; 128]] {
            Transaction::try_from(transaction_proto_with_covenant(&ok)).unwrap();
        }

        let other = [
            // Unknown byte code
            vec![0xff],
            // Over MAX_COVENANT_BYTES
            vec![0x20; 4097],
            // Non-minimal nested length prefix
            vec![0x05, 0x81, 0x00, 0x20],
        ];
        for covenant in other {
            let borsh_err = Covenant::deserialize(&mut borsh_field(&covenant).as_slice()).unwrap_err();
            let err = Transaction::try_from(transaction_proto_with_covenant(&covenant)).unwrap_err();
            assert_eq!(err, TransactionProtoConversionError::Other(borsh_err.to_string()));
        }
    }

    #[test]
    fn a_full_input_covenant_over_the_token_limit_is_reported_as_such() {
        let mut tx = transaction_proto_with_covenant(&[]);
        let input = tx.body.as_mut().unwrap().inputs.first_mut().unwrap();
        assert!(input.commitment.is_some());
        input.covenant = borsh_field(&covenant_over_token_limit());
        let err = Transaction::try_from(tx).unwrap_err();
        assert!(
            matches!(err, TransactionProtoConversionError::CovenantTokenLimit(_)),
            "{err:?}"
        );
    }

    #[test]
    fn a_compact_input_covenant_is_not_decoded() {
        let mut tx = transaction_proto_with_covenant(&[]);
        let input = tx.body.as_mut().unwrap().inputs.first_mut().unwrap();
        input.commitment = None;
        input.output_hash = vec![1; 32];
        input.covenant = borsh_field(&covenant_over_token_limit());
        let tx = Transaction::try_from(tx).unwrap();
        assert!(tx.body.inputs().first().unwrap().is_compact());
    }
}
