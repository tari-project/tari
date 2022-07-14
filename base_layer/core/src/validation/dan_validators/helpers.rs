//  Copyright 2022, The Tari Project
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

use tari_common_types::types::{Commitment, FixedHash, PublicKey};

use crate::{
    chain_storage::{BlockchainBackend, BlockchainDatabase, UtxoMinedInfo},
    transactions::transaction_components::{
        CheckpointChallenge,
        CommitteeSignatures,
        ContractAcceptance,
        ContractAmendment,
        ContractCheckpoint,
        ContractConstitution,
        ContractUpdateProposal,
        ContractUpdateProposalAcceptance,
        OutputType,
        SideChainFeatures,
        TransactionOutput,
    },
    validation::{dan_validators::DanLayerValidationError, ValidationError},
};

pub fn validate_output_type(
    output: &TransactionOutput,
    expected_output_type: OutputType,
) -> Result<(), DanLayerValidationError> {
    let output_type = output.features.output_type;
    if output_type != expected_output_type {
        return Err(DanLayerValidationError::UnexpectedOutputType {
            got: output_type,
            expected: expected_output_type,
        });
    }

    Ok(())
}

pub fn get_sidechain_features(output: &TransactionOutput) -> Result<&SideChainFeatures, DanLayerValidationError> {
    match output.features.sidechain_features.as_ref() {
        Some(features) => Ok(features),
        None => Err(DanLayerValidationError::SidechainFeaturesNotProvided),
    }
}

pub fn fetch_contract_features<B: BlockchainBackend>(
    db: &BlockchainDatabase<B>,
    contract_id: FixedHash,
    output_type: OutputType,
) -> Result<Vec<SideChainFeatures>, ValidationError> {
    let features = fetch_contract_utxos(db, contract_id, output_type)?
        .into_iter()
        .filter_map(|utxo| utxo.output.into_unpruned_output())
        .filter_map(|output| output.features.sidechain_features)
        .map(|v| *v)
        .collect();

    Ok(features)
}

pub fn fetch_contract_utxos<B: BlockchainBackend>(
    db: &BlockchainDatabase<B>,
    contract_id: FixedHash,
    output_type: OutputType,
) -> Result<Vec<UtxoMinedInfo>, ValidationError> {
    let utxos = db.fetch_contract_outputs_by_contract_id_and_type(contract_id, output_type)?;
    Ok(utxos)
}

pub fn fetch_contract_constitution<B: BlockchainBackend>(
    db: &BlockchainDatabase<B>,
    contract_id: FixedHash,
) -> Result<ContractConstitution, ValidationError> {
    let mut features = fetch_contract_features(db, contract_id, OutputType::ContractConstitution)?;
    let feature = features
        .pop()
        .ok_or(DanLayerValidationError::ContractConstitutionNotFound { contract_id })?;

    match feature.constitution {
        Some(value) => Ok(value),
        None => Err(ValidationError::DanLayerError(
            DanLayerValidationError::DataInconsistency {
                details: "Contract constitution data not found in the output features".to_string(),
            },
        )),
    }
}

pub fn fetch_contract_update_proposal<B: BlockchainBackend>(
    db: &BlockchainDatabase<B>,
    contract_id: FixedHash,
    proposal_id: u64,
) -> Result<ContractUpdateProposal, ValidationError> {
    let features = fetch_contract_features(db, contract_id, OutputType::ContractConstitutionProposal)?;
    match features
        .into_iter()
        .filter_map(|feature| feature.update_proposal)
        .find(|proposal| proposal.proposal_id == proposal_id)
    {
        Some(proposal) => Ok(proposal),
        None => Err(ValidationError::DanLayerError(
            DanLayerValidationError::ContractUpdateProposalNotFound {
                contract_id,
                proposal_id,
            },
        )),
    }
}

pub fn fetch_current_contract_checkpoint<B: BlockchainBackend>(
    db: &BlockchainDatabase<B>,
    contract_id: FixedHash,
) -> Result<Option<ContractCheckpoint>, ValidationError> {
    let mut features = fetch_contract_features(db, contract_id, OutputType::ContractCheckpoint)?;
    let feature = match features.pop() {
        Some(feat) => feat,
        None => return Ok(None),
    };

    let checkpoint = feature
        .checkpoint
        .ok_or_else(|| DanLayerValidationError::DataInconsistency {
            details: "DB output marked as checkpoint did not contain checkpoint data".to_string(),
        })?;
    Ok(Some(checkpoint))
}

/// Retrieves a contract acceptance object from the sidechain features, returns an error if not present
pub fn get_contract_acceptance(
    sidechain_feature: &SideChainFeatures,
) -> Result<&ContractAcceptance, DanLayerValidationError> {
    match sidechain_feature.acceptance.as_ref() {
        Some(acceptance) => Ok(acceptance),
        None => Err(DanLayerValidationError::ContractAcceptanceNotFound),
    }
}

pub fn get_contract_amendment(
    sidechain_feature: &SideChainFeatures,
) -> Result<&ContractAmendment, DanLayerValidationError> {
    match sidechain_feature.amendment.as_ref() {
        Some(amendment) => Ok(amendment),
        None => Err(DanLayerValidationError::SideChainFeaturesDataNotProvided {
            field_name: "amendment",
        }),
    }
}

pub fn get_checkpoint(sidechain_features: &SideChainFeatures) -> Result<&ContractCheckpoint, DanLayerValidationError> {
    match sidechain_features.checkpoint.as_ref() {
        Some(checkpoint) => Ok(checkpoint),
        None => Err(DanLayerValidationError::MissingContractData {
            contract_id: sidechain_features.contract_id,
            output_type: OutputType::ContractCheckpoint,
        }),
    }
}

/// Retrieves a contract update proposal acceptance object from the sidechain features, returns an error if not present
pub fn get_contract_update_proposal_acceptance(
    sidechain_feature: &SideChainFeatures,
) -> Result<&ContractUpdateProposalAcceptance, DanLayerValidationError> {
    match sidechain_feature.update_proposal_acceptance.as_ref() {
        Some(acceptance) => Ok(acceptance),
        None => Err(DanLayerValidationError::SideChainFeaturesDataNotProvided {
            field_name: "update_proposal_acceptance",
        }),
    }
}

pub fn get_update_proposal(
    sidechain_feature: &SideChainFeatures,
) -> Result<&ContractUpdateProposal, DanLayerValidationError> {
    match sidechain_feature.update_proposal.as_ref() {
        Some(proposal) => Ok(proposal),
        None => Err(DanLayerValidationError::SideChainFeaturesDataNotProvided {
            field_name: "update_proposal",
        }),
    }
}

pub fn get_commitee_members(signatures: &CommitteeSignatures) -> Vec<&PublicKey> {
    signatures.into_iter().map(|s| s.signer()).collect::<Vec<&PublicKey>>()
}

pub fn fetch_constitution_height<B: BlockchainBackend>(
    db: &BlockchainDatabase<B>,
    contract_id: FixedHash,
) -> Result<u64, ValidationError> {
    let utxos = fetch_contract_utxos(db, contract_id, OutputType::ContractConstitution)?;
    // Only one constitution should be stored for a particular contract_id
    match utxos.first() {
        Some(utxo) => Ok(utxo.mined_height),
        None => Err(ValidationError::DanLayerError(
            DanLayerValidationError::ContractConstitutionNotFound { contract_id },
        )),
    }
}

pub fn fetch_constitution_commitment<B: BlockchainBackend>(
    db: &BlockchainDatabase<B>,
    contract_id: FixedHash,
) -> Result<Commitment, ValidationError> {
    let outputs: Vec<TransactionOutput> = fetch_contract_utxos(db, contract_id, OutputType::ContractConstitution)?
        .into_iter()
        .filter_map(|utxo| utxo.output.into_unpruned_output())
        .collect();

    // Only one constitution should be stored for a particular contract_id
    if outputs.is_empty() {
        return Err(ValidationError::DanLayerError(
            DanLayerValidationError::ContractConstitutionNotFound { contract_id },
        ));
    }

    Ok(outputs[0].commitment().clone())
}

pub fn fetch_proposal_height<B: BlockchainBackend>(
    db: &BlockchainDatabase<B>,
    contract_id: FixedHash,
    proposal_id: u64,
) -> Result<u64, ValidationError> {
    let utxos = fetch_contract_utxos(db, contract_id, OutputType::ContractConstitutionProposal)?;
    let proposal_utxo = utxos.into_iter().find(|utxo| {
        let output = match utxo.output.as_transaction_output() {
            Some(value) => value,
            None => return false,
        };
        output.features.contains_sidechain_proposal(&contract_id, proposal_id)
    });

    match proposal_utxo {
        Some(utxo) => Ok(utxo.mined_height),
        None => Err(ValidationError::DanLayerError(
            DanLayerValidationError::ContractUpdateProposalNotFound {
                contract_id,
                proposal_id,
            },
        )),
    }
}

pub fn fetch_proposal_commitment<B: BlockchainBackend>(
    db: &BlockchainDatabase<B>,
    contract_id: FixedHash,
    proposal_id: u64,
) -> Result<Commitment, ValidationError> {
    let outputs: Vec<TransactionOutput> =
        fetch_contract_utxos(db, contract_id, OutputType::ContractConstitutionProposal)?
            .into_iter()
            .filter_map(|utxo| utxo.output.into_unpruned_output())
            .filter(|output| output.features.contains_sidechain_proposal(&contract_id, proposal_id))
            .collect();

    // Only one constitution should be stored for a particular contract_id
    if outputs.is_empty() {
        return Err(ValidationError::DanLayerError(
            DanLayerValidationError::ContractConstitutionNotFound { contract_id },
        ));
    }

    Ok(outputs[0].commitment().clone())
}

pub fn create_checkpoint_challenge(checkpoint: &ContractCheckpoint, contract_id: &FixedHash) -> CheckpointChallenge {
    // TODO: update when shared commitment consensus among VNs is implemented
    let commitment = Commitment::default();
    CheckpointChallenge::new(
        contract_id,
        &commitment,
        &checkpoint.merkle_root,
        checkpoint.checkpoint_number,
    )
}
