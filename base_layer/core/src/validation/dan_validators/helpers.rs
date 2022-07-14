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

use tari_common_types::types::FixedHash;

use crate::{
    chain_storage::{BlockchainBackend, BlockchainDatabase, UtxoMinedInfo},
    transactions::transaction_components::{
        ContractCheckpoint,
        ContractConstitution,
        ContractUpdateProposal,
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
