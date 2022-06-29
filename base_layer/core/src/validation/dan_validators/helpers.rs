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
use tari_utilities::hex::Hex;

use crate::{
    chain_storage::{BlockchainBackend, BlockchainDatabase, UtxoMinedInfo},
    transactions::transaction_components::{
        ContractConstitution,
        ContractUpdateProposal,
        OutputType,
        SideChainFeatures,
        TransactionOutput,
    },
    validation::ValidationError,
};

pub fn validate_output_type(
    output: &TransactionOutput,
    expected_output_type: OutputType,
) -> Result<(), ValidationError> {
    let output_type = output.features.output_type;
    if output_type != expected_output_type {
        let msg = format!(
            "Invalid output type: expected {:?} but got {:?}",
            expected_output_type, output_type
        );
        return Err(ValidationError::DanLayerError(msg));
    }

    Ok(())
}

pub fn fetch_contract_features<B: BlockchainBackend>(
    db: &BlockchainDatabase<B>,
    contract_id: FixedHash,
    output_type: OutputType,
) -> Result<Vec<SideChainFeatures>, ValidationError> {
    let features = fetch_contract_utxos(db, contract_id, output_type)?
        .iter()
        .filter_map(|utxo| utxo.output.as_transaction_output())
        .filter_map(|output| output.features.sidechain_features.as_ref())
        .cloned()
        .collect();

    Ok(features)
}

pub fn fetch_height<B: BlockchainBackend>(
    db: &BlockchainDatabase<B>,
    contract_id: FixedHash,
    output_type: OutputType,
) -> Result<u64, ValidationError> {
    let utxos = fetch_contract_utxos(db, contract_id, output_type)?;
    match utxos.first() {
        Some(utxo) => Ok(utxo.mined_height),
        None => {
            let msg = format!(
                "Could not find UTXO for contract_id ({}) and type ({})",
                contract_id.to_hex(),
                output_type
            );
            Err(ValidationError::DanLayerError(msg))
        },
    }
}

pub fn fetch_contract_utxos<B: BlockchainBackend>(
    db: &BlockchainDatabase<B>,
    contract_id: FixedHash,
    output_type: OutputType,
) -> Result<Vec<UtxoMinedInfo>, ValidationError> {
    let utxos = db
        .fetch_contract_outputs_by_contract_id_and_type(contract_id, output_type)
        .map_err(|err| ValidationError::DanLayerError(format!("Could not search outputs: {}", err)))?;

    Ok(utxos)
}

pub fn fetch_contract_constitution<B: BlockchainBackend>(
    db: &BlockchainDatabase<B>,
    contract_id: FixedHash,
) -> Result<ContractConstitution, ValidationError> {
    let features = fetch_contract_features(db, contract_id, OutputType::ContractConstitution)?;
    if features.is_empty() {
        return Err(ValidationError::DanLayerError(format!(
            "Contract constitution not found for contract_id {}",
            contract_id.to_hex()
        )));
    }

    let feature = &features[0];

    let constitution = match feature.constitution.as_ref() {
        Some(value) => value,
        None => {
            return Err(ValidationError::DanLayerError(
                "Contract constitution data not found in the output features".to_string(),
            ))
        },
    };

    Ok(constitution.clone())
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
        None => Err(ValidationError::DanLayerError(format!(
            "Contract update proposal not found for contract_id {} and proposal_id {}",
            contract_id.to_hex(),
            proposal_id
        ))),
    }
}

pub fn get_sidechain_features(output: &TransactionOutput) -> Result<&SideChainFeatures, ValidationError> {
    match output.features.sidechain_features.as_ref() {
        Some(features) => Ok(features),
        None => Err(ValidationError::DanLayerError(
            "Sidechain features not found".to_string(),
        )),
    }
}
