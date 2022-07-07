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

use tari_common_types::types::{BlindingFactor, BulletRangeProof, Commitment, FixedHash, PublicKey};
use tari_crypto::tari_utilities::{ByteArray, ByteArrayError};
use tari_script::{ExecutionStack, TariScript};
use tari_utilities::convert::try_convert_all;

use crate::{
    covenants::Covenant,
    proto,
    transactions::{
        aggregated_body::AggregateBody,
        tari_amount::MicroTari,
        transaction_components::{
            bytes_into_fixed_string,
            AssetOutputFeatures,
            CheckpointParameters,
            CommitteeDefinitionFeatures,
            CommitteeMembers,
            CommitteeSignatures,
            ConstitutionChangeFlags,
            ConstitutionChangeRules,
            ContractAcceptance,
            ContractAcceptanceRequirements,
            ContractAmendment,
            ContractCheckpoint,
            ContractConstitution,
            ContractDefinition,
            ContractSpecification,
            ContractUpdateProposal,
            ContractUpdateProposalAcceptance,
            EncryptedValue,
            FunctionRef,
            KernelFeatures,
            MintNonFungibleFeatures,
            OutputFeatures,
            OutputFeaturesVersion,
            OutputType,
            PublicFunction,
            RequirementsForConstitutionChange,
            SideChainCheckpointFeatures,
            SideChainConsensus,
            SideChainFeatures,
            SignerSignature,
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
            .try_into()?;
        let kernel_features = u8::try_from(kernel.features).map_err(|_| "Kernel features must be a single byte")?;

        Ok(TransactionKernel::new(
            TransactionKernelVersion::try_from(
                u8::try_from(kernel.version).map_err(|_| "Invalid version: overflowed u8")?,
            )?,
            KernelFeatures::from_bits(kernel_features)
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
            features: u32::from(kernel.features.bits()),
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
                EncryptedValue::from_bytes(&input.encrypted_value).map_err(|err| err.to_string())?,
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
                // Output hash is only used in compact form
                output_hash: Vec::new(),
                covenant: input
                    .covenant()
                    .map_err(|_| "Non-compact Transaction input should contain covenant".to_string())?
                    .to_bytes(),
                version: input.version as u32,
                encrypted_value: input
                    .encrypted_value()
                    .map_err(|_| "Non-compact Transaction input should contain encrypted value".to_string())?
                    .to_vec(),
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

        let encrypted_value = EncryptedValue::from_bytes(&output.encrypted_value).map_err(|err| err.to_string())?;

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
            encrypted_value,
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
            encrypted_value: output.encrypted_value.to_vec(),
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
        let sidechain_features = features
            .sidechain_features
            .map(SideChainFeatures::try_from)
            .transpose()?;

        let flags = features
            .flags
            .try_into()
            .map_err(|_| "Invalid output type: overflowed")?;

        Ok(OutputFeatures::new(
            OutputFeaturesVersion::try_from(
                u8::try_from(features.version).map_err(|_| "Invalid version: overflowed u8")?,
            )?,
            OutputType::from_byte(flags).ok_or_else(|| "Invalid or unrecognised output type".to_string())?,
            features.maturity,
            u8::try_from(features.recovery_byte).map_err(|_| "Invalid recovery byte: overflowed u8")?,
            features.metadata,
            unique_id,
            sidechain_features,
            parent_public_key,
            None,
            None,
            None,
            None,
        ))
    }
}

impl From<OutputFeatures> for proto::types::OutputFeatures {
    fn from(features: OutputFeatures) -> Self {
        Self {
            flags: u32::from(features.output_type.as_byte()),
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
            recovery_byte: u32::from(features.recovery_byte),
            sidechain_features: features.sidechain_features.map(Into::into),
        }
    }
}

//---------------------------------- SideChainFeatures --------------------------------------------//
impl From<SideChainFeatures> for proto::types::SideChainFeatures {
    fn from(value: SideChainFeatures) -> Self {
        Self {
            contract_id: value.contract_id.to_vec(),
            definition: value.definition.map(Into::into),
            constitution: value.constitution.map(Into::into),
            acceptance: value.acceptance.map(Into::into),
            update_proposal: value.update_proposal.map(Into::into),
            update_proposal_acceptance: value.update_proposal_acceptance.map(Into::into),
            amendment: value.amendment.map(Into::into),
            checkpoint: value.checkpoint.map(Into::into),
        }
    }
}

impl TryFrom<proto::types::SideChainFeatures> for SideChainFeatures {
    type Error = String;

    fn try_from(features: proto::types::SideChainFeatures) -> Result<Self, Self::Error> {
        let contract_id = features.contract_id.try_into().map_err(|_| "Invalid contract_id")?;
        let definition = features.definition.map(ContractDefinition::try_from).transpose()?;
        let constitution = features.constitution.map(ContractConstitution::try_from).transpose()?;
        let acceptance = features.acceptance.map(ContractAcceptance::try_from).transpose()?;
        let update_proposal = features
            .update_proposal
            .map(ContractUpdateProposal::try_from)
            .transpose()?;
        let update_proposal_acceptance = features
            .update_proposal_acceptance
            .map(ContractUpdateProposalAcceptance::try_from)
            .transpose()?;
        let amendment = features.amendment.map(ContractAmendment::try_from).transpose()?;
        let checkpoint = features.checkpoint.map(ContractCheckpoint::try_from).transpose()?;

        Ok(Self {
            contract_id,
            definition,
            constitution,
            acceptance,
            update_proposal,
            update_proposal_acceptance,
            amendment,
            checkpoint,
        })
    }
}

//---------------------------------- ContractConstitution --------------------------------------------//
impl From<ContractConstitution> for proto::types::ContractConstitution {
    fn from(value: ContractConstitution) -> Self {
        Self {
            validator_committee: Some(value.validator_committee.into()),
            acceptance_requirements: Some(value.acceptance_requirements.into()),
            consensus: value.consensus.into(),
            checkpoint_params: Some(value.checkpoint_params.into()),
            constitution_change_rules: Some(value.constitution_change_rules.into()),
            initial_reward: value.initial_reward.into(),
        }
    }
}

impl TryFrom<proto::types::ContractConstitution> for ContractConstitution {
    type Error = String;

    fn try_from(value: proto::types::ContractConstitution) -> Result<Self, Self::Error> {
        use num_traits::FromPrimitive;
        let validator_committee = value
            .validator_committee
            .map(TryInto::try_into)
            .ok_or("validator_committee not provided")??;
        let acceptance_requirements = value
            .acceptance_requirements
            .map(TryInto::try_into)
            .ok_or("acceptance_requirements not provided")??;
        let consensus = SideChainConsensus::from_i32(value.consensus).ok_or("Invalid SideChainConsensus")?;
        let checkpoint_params = value
            .checkpoint_params
            .map(TryInto::try_into)
            .ok_or("checkpoint_params not provided")??;
        let constitution_change_rules = value
            .constitution_change_rules
            .map(TryInto::try_into)
            .ok_or("constitution_change_rules not provided")??;
        let initial_reward = value.initial_reward.into();

        Ok(Self {
            validator_committee,
            acceptance_requirements,
            consensus,
            checkpoint_params,
            constitution_change_rules,
            initial_reward,
        })
    }
}

//---------------------------------- ContractCheckpoint --------------------------------------------//
impl From<ContractCheckpoint> for proto::types::ContractCheckpoint {
    fn from(value: ContractCheckpoint) -> Self {
        Self {
            checkpoint_number: value.checkpoint_number,
            merkle_root: value.merkle_root.to_vec(),
            signatures: Some(value.signatures.into()),
        }
    }
}

impl TryFrom<proto::types::ContractCheckpoint> for ContractCheckpoint {
    type Error = String;

    fn try_from(value: proto::types::ContractCheckpoint) -> Result<Self, Self::Error> {
        let merkle_root = value.merkle_root.try_into().map_err(|_| "Invalid merkle root")?;
        let signatures = value.signatures.map(TryInto::try_into).transpose()?.unwrap_or_default();
        Ok(Self {
            checkpoint_number: value.checkpoint_number,
            merkle_root,
            signatures,
        })
    }
}

//---------------------------------- ContractAcceptanceRequirements --------------------------------------------//
impl From<ContractAcceptanceRequirements> for proto::types::ContractAcceptanceRequirements {
    fn from(value: ContractAcceptanceRequirements) -> Self {
        Self {
            acceptance_period_expiry: value.acceptance_period_expiry,
            minimum_quorum_required: value.minimum_quorum_required,
        }
    }
}

impl TryFrom<proto::types::ContractAcceptanceRequirements> for ContractAcceptanceRequirements {
    type Error = String;

    fn try_from(value: proto::types::ContractAcceptanceRequirements) -> Result<Self, Self::Error> {
        Ok(Self {
            acceptance_period_expiry: value.acceptance_period_expiry,
            minimum_quorum_required: value.minimum_quorum_required,
        })
    }
}

//---------------------------------- ContractAcceptance --------------------------------------------//

impl From<ContractAcceptance> for proto::types::ContractAcceptance {
    fn from(value: ContractAcceptance) -> Self {
        Self {
            validator_node_public_key: value.validator_node_public_key.as_bytes().to_vec(),
            signature: Some(value.signature.into()),
        }
    }
}

impl TryFrom<proto::types::ContractAcceptance> for ContractAcceptance {
    type Error = String;

    fn try_from(value: proto::types::ContractAcceptance) -> Result<Self, Self::Error> {
        let validator_node_public_key =
            PublicKey::from_bytes(value.validator_node_public_key.as_bytes()).map_err(|err| format!("{:?}", err))?;
        let signature = value
            .signature
            .ok_or_else(|| "signature not provided".to_string())?
            .try_into()?;

        Ok(Self {
            validator_node_public_key,
            signature,
        })
    }
}

//---------------------------------- ContractUpdateProposal --------------------------------------------//

impl From<ContractUpdateProposal> for proto::types::ContractUpdateProposal {
    fn from(value: ContractUpdateProposal) -> Self {
        Self {
            proposal_id: value.proposal_id,
            signature: Some(value.signature.into()),
            updated_constitution: Some(value.updated_constitution.into()),
        }
    }
}

impl TryFrom<proto::types::ContractUpdateProposal> for ContractUpdateProposal {
    type Error = String;

    fn try_from(value: proto::types::ContractUpdateProposal) -> Result<Self, Self::Error> {
        let signature = value
            .signature
            .ok_or_else(|| "signature not provided".to_string())?
            .try_into()?;

        let updated_constitution = value
            .updated_constitution
            .ok_or_else(|| "updated_constiution not provided".to_string())?
            .try_into()?;

        Ok(Self {
            proposal_id: value.proposal_id,
            signature,
            updated_constitution,
        })
    }
}

//---------------------------------- ContractUpdateProposalAcceptance --------------------------------------------//

impl From<ContractUpdateProposalAcceptance> for proto::types::ContractUpdateProposalAcceptance {
    fn from(value: ContractUpdateProposalAcceptance) -> Self {
        Self {
            proposal_id: value.proposal_id,
            validator_node_public_key: value.validator_node_public_key.as_bytes().to_vec(),
            signature: Some(value.signature.into()),
        }
    }
}

impl TryFrom<proto::types::ContractUpdateProposalAcceptance> for ContractUpdateProposalAcceptance {
    type Error = String;

    fn try_from(value: proto::types::ContractUpdateProposalAcceptance) -> Result<Self, Self::Error> {
        let validator_node_public_key =
            PublicKey::from_bytes(value.validator_node_public_key.as_bytes()).map_err(|err| format!("{:?}", err))?;
        let signature = value
            .signature
            .ok_or_else(|| "signature not provided".to_string())?
            .try_into()?;

        Ok(Self {
            proposal_id: value.proposal_id,
            validator_node_public_key,
            signature,
        })
    }
}

//---------------------------------- ContractAmendment --------------------------------------------//

impl From<ContractAmendment> for proto::types::ContractAmendment {
    fn from(value: ContractAmendment) -> Self {
        Self {
            proposal_id: value.proposal_id,
            validator_committee: Some(value.validator_committee.into()),
            validator_signatures: Some(value.validator_signatures.into()),
            updated_constitution: Some(value.updated_constitution.into()),
            activation_window: value.activation_window,
        }
    }
}

impl TryFrom<proto::types::ContractAmendment> for ContractAmendment {
    type Error = String;

    fn try_from(value: proto::types::ContractAmendment) -> Result<Self, Self::Error> {
        let validator_committee = value
            .validator_committee
            .map(TryInto::try_into)
            .ok_or("validator_committee not provided")??;

        let validator_signatures = value
            .validator_signatures
            .map(TryInto::try_into)
            .ok_or("validator_signatures not provided")??;

        let updated_constitution = value
            .updated_constitution
            .ok_or_else(|| "updated_constiution not provided".to_string())?
            .try_into()?;

        Ok(Self {
            proposal_id: value.proposal_id,
            validator_committee,
            validator_signatures,
            updated_constitution,
            activation_window: value.activation_window,
        })
    }
}

//---------------------------------- SideChainConsensus --------------------------------------------//
impl From<SideChainConsensus> for proto::types::SideChainConsensus {
    fn from(value: SideChainConsensus) -> Self {
        #[allow(clippy::enum_glob_use)]
        use proto::types::SideChainConsensus::*;
        match value {
            SideChainConsensus::Bft => Bft,
            SideChainConsensus::ProofOfWork => ProofOfWork,
            SideChainConsensus::MerkleRoot => MerkleRoot,
        }
    }
}

impl TryFrom<proto::types::SideChainConsensus> for SideChainConsensus {
    type Error = String;

    fn try_from(value: proto::types::SideChainConsensus) -> Result<Self, Self::Error> {
        #[allow(clippy::enum_glob_use)]
        use proto::types::SideChainConsensus::*;
        match value {
            Unspecified => Err("Side chain consensus not specified or invalid".to_string()),
            Bft => Ok(SideChainConsensus::Bft),
            ProofOfWork => Ok(SideChainConsensus::ProofOfWork),
            MerkleRoot => Ok(SideChainConsensus::MerkleRoot),
        }
    }
}

//---------------------------------- CheckpointParameters --------------------------------------------//
impl From<CheckpointParameters> for proto::types::CheckpointParameters {
    fn from(value: CheckpointParameters) -> Self {
        Self {
            minimum_quorum_required: value.minimum_quorum_required,
            abandoned_interval: value.abandoned_interval,
            quarantine_interval: value.quarantine_interval,
        }
    }
}

impl TryFrom<proto::types::CheckpointParameters> for CheckpointParameters {
    type Error = String;

    fn try_from(value: proto::types::CheckpointParameters) -> Result<Self, Self::Error> {
        Ok(Self {
            minimum_quorum_required: value.minimum_quorum_required,
            abandoned_interval: value.abandoned_interval,
            quarantine_interval: value.quarantine_interval,
        })
    }
}

//---------------------------------- ConstitutionChangeRules --------------------------------------------//
impl From<ConstitutionChangeRules> for proto::types::ConstitutionChangeRules {
    fn from(value: ConstitutionChangeRules) -> Self {
        Self {
            change_flags: value.change_flags.bits().into(),
            requirements_for_constitution_change: value.requirements_for_constitution_change.map(Into::into),
        }
    }
}

impl TryFrom<proto::types::ConstitutionChangeRules> for ConstitutionChangeRules {
    type Error = String;

    fn try_from(value: proto::types::ConstitutionChangeRules) -> Result<Self, Self::Error> {
        Ok(Self {
            change_flags: u8::try_from(value.change_flags)
                .ok()
                .and_then(ConstitutionChangeFlags::from_bits)
                .ok_or("Invalid change_flags")?,
            requirements_for_constitution_change: value
                .requirements_for_constitution_change
                .map(RequirementsForConstitutionChange::try_from)
                .transpose()?,
        })
    }
}

//---------------------------------- RequirementsForConstitutionChange --------------------------------------------//
impl From<RequirementsForConstitutionChange> for proto::types::RequirementsForConstitutionChange {
    fn from(value: RequirementsForConstitutionChange) -> Self {
        Self {
            minimum_constitution_committee_signatures: value.minimum_constitution_committee_signatures,
            constitution_committee: value.constitution_committee.map(Into::into),
            backup_keys: value.backup_keys.map(Into::into),
        }
    }
}

impl TryFrom<proto::types::RequirementsForConstitutionChange> for RequirementsForConstitutionChange {
    type Error = String;

    fn try_from(value: proto::types::RequirementsForConstitutionChange) -> Result<Self, Self::Error> {
        Ok(Self {
            minimum_constitution_committee_signatures: value.minimum_constitution_committee_signatures,
            constitution_committee: value
                .constitution_committee
                .map(CommitteeMembers::try_from)
                .transpose()?,
            backup_keys: value.backup_keys.map(CommitteeMembers::try_from).transpose()?,
        })
    }
}

//---------------------------------- CommitteeMembers --------------------------------------------//
impl From<CommitteeMembers> for proto::types::CommitteeMembers {
    fn from(value: CommitteeMembers) -> Self {
        Self {
            members: value.members().iter().map(|pk| pk.to_vec()).collect(),
        }
    }
}

impl TryFrom<proto::types::CommitteeMembers> for CommitteeMembers {
    type Error = String;

    fn try_from(value: proto::types::CommitteeMembers) -> Result<Self, Self::Error> {
        if value.members.len() > CommitteeMembers::MAX_MEMBERS {
            return Err(format!(
                "Too many committee members: expected {} but got {}",
                CommitteeMembers::MAX_MEMBERS,
                value.members.len()
            ));
        }

        let members = value
            .members
            .iter()
            .enumerate()
            .map(|(i, c)| {
                PublicKey::from_bytes(c)
                    .map_err(|err| format!("committee member #{} was not a valid public key: {}", i + 1, err))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let members = CommitteeMembers::try_from(members).map_err(|e| e.to_string())?;
        Ok(members)
    }
}

//---------------------------------- CommitteeSignatures --------------------------------------------//
impl From<CommitteeSignatures> for proto::types::CommitteeSignatures {
    fn from(value: CommitteeSignatures) -> Self {
        Self {
            signatures: value.signatures().iter().map(Into::into).collect(),
        }
    }
}

impl TryFrom<proto::types::CommitteeSignatures> for CommitteeSignatures {
    type Error = String;

    fn try_from(value: proto::types::CommitteeSignatures) -> Result<Self, Self::Error> {
        if value.signatures.len() > CommitteeSignatures::MAX_SIGNATURES {
            return Err(format!(
                "Too many committee signatures: expected {} but got {}",
                CommitteeSignatures::MAX_SIGNATURES,
                value.signatures.len()
            ));
        }

        let signatures = value
            .signatures
            .into_iter()
            .enumerate()
            .map(|(i, s)| {
                SignerSignature::try_from(s)
                    .map_err(|err| format!("committee signature #{} was not a valid signature: {}", i + 1, err))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let signatures = CommitteeSignatures::try_from(signatures).map_err(|e| e.to_string())?;
        Ok(signatures)
    }
}

// TODO: deprecated

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
        if value.merkle_root.len() != FixedHash::byte_size() {
            return Err(format!(
                "Invalid side chain checkpoint merkle length {}",
                value.merkle_root.len()
            ));
        }
        let merkle_root = FixedHash::try_from(value.merkle_root).map_err(|e| e.to_string())?;
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

//---------------------------------- ContractDefinition --------------------------------------------//

impl TryFrom<proto::types::ContractDefinition> for ContractDefinition {
    type Error = String;

    fn try_from(value: proto::types::ContractDefinition) -> Result<Self, Self::Error> {
        let contract_name = bytes_into_fixed_string(value.contract_name);

        let contract_issuer =
            PublicKey::from_bytes(value.contract_issuer.as_bytes()).map_err(|err| format!("{:?}", err))?;

        let contract_spec = value
            .contract_spec
            .map(ContractSpecification::try_from)
            .ok_or_else(|| "contract_spec is missing".to_string())??;

        Ok(Self {
            contract_name,
            contract_issuer,
            contract_spec,
        })
    }
}

impl From<ContractDefinition> for proto::types::ContractDefinition {
    fn from(value: ContractDefinition) -> Self {
        let contract_name = value.contract_name.as_bytes().to_vec();
        let contract_issuer = value.contract_issuer.as_bytes().to_vec();

        Self {
            contract_name,
            contract_issuer,
            contract_spec: Some(value.contract_spec.into()),
        }
    }
}

impl TryFrom<proto::types::ContractSpecification> for ContractSpecification {
    type Error = String;

    fn try_from(value: proto::types::ContractSpecification) -> Result<Self, Self::Error> {
        let runtime = bytes_into_fixed_string(value.runtime);
        let public_functions = value
            .public_functions
            .into_iter()
            .map(PublicFunction::try_from)
            .collect::<Result<_, _>>()?;

        Ok(Self {
            runtime,
            public_functions,
        })
    }
}

impl From<ContractSpecification> for proto::types::ContractSpecification {
    fn from(value: ContractSpecification) -> Self {
        let public_functions = value.public_functions.into_iter().map(|f| f.into()).collect();
        Self {
            runtime: value.runtime.as_bytes().to_vec(),
            public_functions,
        }
    }
}

impl TryFrom<proto::types::PublicFunction> for PublicFunction {
    type Error = String;

    fn try_from(value: proto::types::PublicFunction) -> Result<Self, Self::Error> {
        let function = value
            .function
            .map(FunctionRef::try_from)
            .ok_or_else(|| "function is missing".to_string())??;

        Ok(Self {
            name: bytes_into_fixed_string(value.name),
            function,
        })
    }
}

impl From<PublicFunction> for proto::types::PublicFunction {
    fn from(value: PublicFunction) -> Self {
        Self {
            name: value.name.as_bytes().to_vec(),
            function: Some(value.function.into()),
        }
    }
}

impl TryFrom<proto::types::FunctionRef> for FunctionRef {
    type Error = String;

    fn try_from(value: proto::types::FunctionRef) -> Result<Self, Self::Error> {
        let template_id = FixedHash::try_from(value.template_id).map_err(|err| format!("{:?}", err))?;
        let function_id = u16::try_from(value.function_id).map_err(|_| "Invalid function_id: overflowed u16")?;

        Ok(Self {
            template_id,
            function_id,
        })
    }
}

impl From<FunctionRef> for proto::types::FunctionRef {
    fn from(value: FunctionRef) -> Self {
        let template_id = value.template_id.as_bytes().to_vec();

        Self {
            template_id,
            function_id: value.function_id.into(),
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
