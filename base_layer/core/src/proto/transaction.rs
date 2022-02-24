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

use tari_common_types::types::{BlindingFactor, BulletRangeProof, Commitment, PublicKey, BLOCK_HASH_LENGTH};
use tari_crypto::{
    script::{ExecutionStack, TariScript},
    tari_utilities::{ByteArray, ByteArrayError},
};
use tari_utilities::convert::try_convert_all;

use crate::{
    covenants::Covenant,
    proto,
    transactions::{
        aggregated_body::AggregateBody,
        tari_amount::MicroTari,
        transaction_components::{
            AssetOutputFeatures,
            CommitteeDefinitionFeatures,
            KernelFeatures,
            MintNonFungibleFeatures,
            OutputFeatures,
            OutputFeaturesVersion,
            OutputFlags,
            SideChainCheckpointFeatures,
            TemplateParameter,
            Transaction,
            TransactionInput,
            TransactionInputVersion,
            TransactionKernel,
            TransactionKernelVersion,
            TransactionOutput,
            TransactionOutputVersion,
        },
    },
};

//---------------------------------- TransactionKernel --------------------------------------------//

impl TryFrom<proto::types::TransactionKernel> for TransactionKernel {
    type Error = String;

    fn try_from(kernel: proto::types::TransactionKernel) -> Result<Self, Self::Error> {
        let excess = Commitment::from_bytes(
            &kernel
                .excess
                .ok_or_else(|| "Excess not provided in kernel".to_string())?
                .data,
        )
        .map_err(|err| err.to_string())?;

        let excess_sig = kernel
            .excess_sig
            .ok_or_else(|| "excess_sig not provided".to_string())?
            .try_into()
            .map_err(|err: ByteArrayError| err.to_string())?;

        Ok(TransactionKernel::new(
            TransactionKernelVersion::try_from(
                u8::try_from(kernel.version).map_err(|_| "Invalid version: overflowed u8")?,
            )?,
            KernelFeatures::from_bits(kernel.features as u8)
                .ok_or_else(|| "Invalid or unrecognised kernel feature flag".to_string())?,
            MicroTari::from(kernel.fee),
            kernel.lock_height,
            excess,
            excess_sig,
        ))
    }
}

impl From<TransactionKernel> for proto::types::TransactionKernel {
    fn from(kernel: TransactionKernel) -> Self {
        Self {
            features: kernel.features.bits() as u32,
            excess: Some(kernel.excess.into()),
            excess_sig: Some(kernel.excess_sig.into()),
            fee: kernel.fee.into(),
            lock_height: kernel.lock_height,
            version: kernel.version as u32,
        }
    }
}

//---------------------------------- TransactionInput --------------------------------------------//

impl TryFrom<proto::types::TransactionInput> for TransactionInput {
    type Error = String;

    fn try_from(input: proto::types::TransactionInput) -> Result<Self, Self::Error> {
        let script_signature = input
            .script_signature
            .ok_or_else(|| "script_signature not provided".to_string())?
            .try_into()
            .map_err(|err: ByteArrayError| err.to_string())?;

        // Check if the received Transaction input is in compact form or not
        if let Some(commitment) = input.commitment {
            let commitment = Commitment::from_bytes(&commitment.data).map_err(|e| e.to_string())?;
            let features = input
                .features
                .map(TryInto::try_into)
                .ok_or_else(|| "transaction output features not provided".to_string())??;

            let sender_offset_public_key =
                PublicKey::from_bytes(input.sender_offset_public_key.as_bytes()).map_err(|err| format!("{:?}", err))?;

            Ok(TransactionInput::new_with_output_data(
                TransactionInputVersion::try_from(
                    u8::try_from(input.version).map_err(|_| "Invalid version: overflowed u8")?,
                )?,
                features,
                commitment,
                TariScript::from_bytes(input.script.as_slice()).map_err(|err| format!("{:?}", err))?,
                ExecutionStack::from_bytes(input.input_data.as_slice()).map_err(|err| format!("{:?}", err))?,
                script_signature,
                sender_offset_public_key,
                Covenant::from_bytes(&input.covenant).map_err(|err| err.to_string())?,
            ))
        } else {
            if input.output_hash.is_empty() {
                return Err("Compact Transaction Input does not contain `output_hash`".to_string());
            }
            Ok(TransactionInput::new_with_output_hash(
                input.output_hash,
                ExecutionStack::from_bytes(input.input_data.as_slice()).map_err(|err| format!("{:?}", err))?,
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
                input_data: input.input_data.as_bytes(),
                script_signature: Some(input.script_signature.into()),
                output_hash,
                ..Default::default()
            })
        } else {
            Ok(Self {
                features: Some(
                    input
                        .features()
                        .map_err(|_| "Non-compact Transaction input should contain features".to_string())?
                        .clone()
                        .into(),
                ),
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
                    .as_bytes(),
                input_data: input.input_data.as_bytes(),
                script_signature: Some(input.script_signature.clone().into()),
                sender_offset_public_key: input
                    .sender_offset_public_key()
                    .map_err(|_| "Non-compact Transaction input should contain sender_offset_public_key".to_string())?
                    .as_bytes()
                    .to_vec(),
                output_hash: Vec::new(),
                covenant: input
                    .covenant()
                    .map_err(|_| "Non-compact Transaction input should contain covenant".to_string())?
                    .to_bytes(),
                version: input.version as u32,
            })
        }
    }
}

//---------------------------------- TransactionOutput --------------------------------------------//

impl TryFrom<proto::types::TransactionOutput> for TransactionOutput {
    type Error = String;

    fn try_from(output: proto::types::TransactionOutput) -> Result<Self, Self::Error> {
        let features = output
            .features
            .map(TryInto::try_into)
            .ok_or_else(|| "Transaction output features not provided".to_string())??;

        let commitment = output
            .commitment
            .map(|commit| Commitment::from_bytes(&commit.data))
            .ok_or_else(|| "Transaction output commitment not provided".to_string())?
            .map_err(|err| err.to_string())?;

        let sender_offset_public_key =
            PublicKey::from_bytes(output.sender_offset_public_key.as_bytes()).map_err(|err| format!("{:?}", err))?;

        let script = TariScript::from_bytes(&output.script).map_err(|err| err.to_string())?;

        let metadata_signature = output
            .metadata_signature
            .ok_or_else(|| "Metadata signature not provided".to_string())?
            .try_into()
            .map_err(|_| "Metadata signature could not be converted".to_string())?;

        let covenant = Covenant::from_bytes(&output.covenant).map_err(|err| err.to_string())?;

        Ok(Self::new(
            TransactionOutputVersion::try_from(
                u8::try_from(output.version).map_err(|_| "Invalid version: overflowed u8")?,
            )?,
            features,
            commitment,
            BulletRangeProof(output.range_proof),
            script,
            sender_offset_public_key,
            metadata_signature,
            covenant,
        ))
    }
}

impl From<TransactionOutput> for proto::types::TransactionOutput {
    fn from(output: TransactionOutput) -> Self {
        Self {
            features: Some(output.features.into()),
            commitment: Some(output.commitment.into()),
            range_proof: output.proof.to_vec(),
            script: output.script.as_bytes(),
            sender_offset_public_key: output.sender_offset_public_key.as_bytes().to_vec(),
            metadata_signature: Some(output.metadata_signature.into()),
            covenant: output.covenant.to_bytes(),
            version: output.version as u32,
        }
    }
}

//---------------------------------- OutputFeatures --------------------------------------------//

impl TryFrom<proto::types::OutputFeatures> for OutputFeatures {
    type Error = String;

    fn try_from(features: proto::types::OutputFeatures) -> Result<Self, Self::Error> {
        let unique_id = if features.unique_id.is_empty() {
            None
        } else {
            Some(features.unique_id.clone())
        };
        let parent_public_key = if features.parent_public_key.is_empty() {
            None
        } else {
            Some(PublicKey::from_bytes(features.parent_public_key.as_bytes()).map_err(|err| format!("{:?}", err))?)
        };

        Ok(OutputFeatures::new(
            OutputFeaturesVersion::try_from(
                u8::try_from(features.version).map_err(|_| "Invalid version: overflowed u8")?,
            )?,
            OutputFlags::from_bits(features.flags as u8)
                .ok_or_else(|| "Invalid or unrecognised output flags".to_string())?,
            features.maturity,
            features.metadata,
            unique_id,
            parent_public_key,
            match features.asset {
                Some(a) => Some(a.try_into()?),
                None => None,
            },
            match features.mint_non_fungible {
                Some(m) => Some(m.try_into()?),
                None => None,
            },
            features.sidechain_checkpoint.map(|s| s.try_into()).transpose()?,
            features.committee_definition.map(|c| c.try_into()).transpose()?,
        ))
    }
}

impl From<OutputFeatures> for proto::types::OutputFeatures {
    fn from(features: OutputFeatures) -> Self {
        Self {
            flags: features.flags.bits() as u32,
            maturity: features.maturity,
            metadata: features.metadata,
            unique_id: features.unique_id.unwrap_or_default(),
            parent_public_key: features
                .parent_public_key
                .map(|a| a.as_bytes().to_vec())
                .unwrap_or_default(),
            asset: features.asset.map(|a| a.into()),
            mint_non_fungible: features.mint_non_fungible.map(|m| m.into()),
            sidechain_checkpoint: features.sidechain_checkpoint.map(|s| s.into()),
            version: features.version as u32,
            committee_definition: features.committee_definition.map(|c| c.into()),
        }
    }
}

impl TryFrom<proto::types::AssetOutputFeatures> for AssetOutputFeatures {
    type Error = String;

    fn try_from(features: proto::types::AssetOutputFeatures) -> Result<Self, Self::Error> {
        let public_key = PublicKey::from_bytes(features.public_key.as_bytes()).map_err(|err| format!("{:?}", err))?;

        Ok(Self {
            public_key,
            template_ids_implemented: features.template_ids_implemented,
            template_parameters: features.template_parameters.into_iter().map(|s| s.into()).collect(),
        })
    }
}

impl From<AssetOutputFeatures> for proto::types::AssetOutputFeatures {
    fn from(features: AssetOutputFeatures) -> Self {
        Self {
            public_key: features.public_key.as_bytes().to_vec(),
            template_ids_implemented: features.template_ids_implemented,
            template_parameters: features.template_parameters.into_iter().map(|tp| tp.into()).collect(),
        }
    }
}

impl From<proto::types::TemplateParameter> for TemplateParameter {
    fn from(source: proto::types::TemplateParameter) -> Self {
        Self {
            template_id: source.template_id,
            template_data_version: source.template_data_version,
            template_data: source.template_data,
        }
    }
}

impl From<TemplateParameter> for proto::types::TemplateParameter {
    fn from(source: TemplateParameter) -> Self {
        Self {
            template_id: source.template_id,
            template_data_version: source.template_data_version,
            template_data: source.template_data,
        }
    }
}

impl TryFrom<proto::types::MintNonFungibleFeatures> for MintNonFungibleFeatures {
    type Error = String;

    fn try_from(value: proto::types::MintNonFungibleFeatures) -> Result<Self, Self::Error> {
        let asset_public_key =
            PublicKey::from_bytes(value.asset_public_key.as_bytes()).map_err(|err| format!("{:?}", err))?;

        let asset_owner_commitment = value
            .asset_owner_commitment
            .map(|c| Commitment::from_bytes(&c.data))
            .ok_or_else(|| "asset_owner_commitment is missing".to_string())?
            .map_err(|err| err.to_string())?;
        Ok(Self {
            asset_public_key,
            asset_owner_commitment,
        })
    }
}

impl From<MintNonFungibleFeatures> for proto::types::MintNonFungibleFeatures {
    fn from(value: MintNonFungibleFeatures) -> Self {
        Self {
            asset_public_key: value.asset_public_key.as_bytes().to_vec(),
            asset_owner_commitment: Some(value.asset_owner_commitment.into()),
        }
    }
}

impl TryFrom<proto::types::SideChainCheckpointFeatures> for SideChainCheckpointFeatures {
    type Error = String;

    fn try_from(value: proto::types::SideChainCheckpointFeatures) -> Result<Self, Self::Error> {
        if value.merkle_root.len() != BLOCK_HASH_LENGTH {
            return Err(format!(
                "Invalid side chain checkpoint merkle length {}",
                value.merkle_root.len()
            ));
        }
        let mut merkle_root = [0u8; BLOCK_HASH_LENGTH];
        merkle_root.copy_from_slice(&value.merkle_root[0..BLOCK_HASH_LENGTH]);
        let committee = value
            .committee
            .into_iter()
            .map(|c| PublicKey::from_bytes(&c).map_err(|err| format!("{:?}", err)))
            .collect::<Result<_, _>>()?;
        Ok(Self { merkle_root, committee })
    }
}

impl From<SideChainCheckpointFeatures> for proto::types::SideChainCheckpointFeatures {
    fn from(value: SideChainCheckpointFeatures) -> Self {
        Self {
            merkle_root: value.merkle_root.as_bytes().to_vec(),
            committee: value.committee.into_iter().map(|c| c.as_bytes().to_vec()).collect(),
        }
    }
}

impl TryFrom<proto::types::CommitteeDefinitionFeatures> for CommitteeDefinitionFeatures {
    type Error = String;

    fn try_from(value: proto::types::CommitteeDefinitionFeatures) -> Result<Self, Self::Error> {
        let committee = value
            .committee
            .into_iter()
            .map(|c| PublicKey::from_bytes(&c).map_err(|err| format!("{:?}", err)))
            .collect::<Result<_, _>>()?;
        let effective_sidechain_height = value.effective_sidechain_height;

        Ok(Self {
            committee,
            effective_sidechain_height,
        })
    }
}

impl From<CommitteeDefinitionFeatures> for proto::types::CommitteeDefinitionFeatures {
    fn from(value: CommitteeDefinitionFeatures) -> Self {
        Self {
            committee: value.committee.into_iter().map(|c| c.as_bytes().to_vec()).collect(),
            effective_sidechain_height: value.effective_sidechain_height,
        }
    }
}

//---------------------------------- AggregateBody --------------------------------------------//

impl TryFrom<proto::types::AggregateBody> for AggregateBody {
    type Error = String;

    fn try_from(body: proto::types::AggregateBody) -> Result<Self, Self::Error> {
        let inputs = try_convert_all(body.inputs)?;
        let outputs = try_convert_all(body.outputs)?;
        let kernels = try_convert_all(body.kernels)?;
        let body = AggregateBody::new(inputs, outputs, kernels);
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
            outputs: o.into_iter().map(Into::into).collect(),
            kernels: k.into_iter().map(Into::into).collect(),
        })
    }
}

//----------------------------------- Transaction ---------------------------------------------//

impl TryFrom<proto::types::Transaction> for Transaction {
    type Error = String;

    fn try_from(tx: proto::types::Transaction) -> Result<Self, Self::Error> {
        let offset = tx
            .offset
            .map(|offset| BlindingFactor::from_bytes(&offset.data))
            .ok_or_else(|| "Blinding factor offset not provided".to_string())?
            .map_err(|err| err.to_string())?;
        let body = tx
            .body
            .map(TryInto::try_into)
            .ok_or_else(|| "Body not provided".to_string())??;
        let script_offset = tx
            .script_offset
            .map(|script_offset| BlindingFactor::from_bytes(&script_offset.data))
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
