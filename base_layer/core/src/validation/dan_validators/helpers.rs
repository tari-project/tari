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
    chain_storage::{BlockchainBackend, BlockchainDatabase},
    transactions::transaction_components::{ContractConstitution, OutputType, SideChainFeatures, TransactionOutput},
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
) -> Result<Option<SideChainFeatures>, ValidationError> {
    let outputs = db
        .fetch_contract_outputs_by_contract_id_and_type(contract_id, output_type)
        .map_err(|err| ValidationError::DanLayerError(format!("Could not search outputs: {}", err)))?;
    if outputs.is_empty() {
        return Ok(None);
    }

    let utxo_info = match outputs.first() {
        Some(value) => value,
        None => return Ok(None),
    };

    let transaction_output = match utxo_info.output.as_transaction_output() {
        Some(value) => value,
        None => return Ok(None),
    };

    match transaction_output.features.sidechain_features.as_ref() {
        Some(value) => Ok(Some(value.clone())),
        None => Ok(None),
    }
}

pub fn fetch_contract_constitution<B: BlockchainBackend>(
    db: &BlockchainDatabase<B>,
    contract_id: FixedHash,
) -> Result<ContractConstitution, ValidationError> {
    let features_result = fetch_contract_features(db, contract_id, OutputType::ContractConstitution)?;

    let features = match features_result {
        Some(value) => value,
        None => {
            return Err(ValidationError::DanLayerError(format!(
                "Contract constitution not found for contract_id {}",
                contract_id.to_hex()
            )))
        },
    };

    let constitution = match features.constitution.as_ref() {
        Some(value) => value,
        None => {
            return Err(ValidationError::DanLayerError(
                "Contract constitution data not found in the output features".to_string(),
            ))
        },
    };

    Ok(constitution.clone())
}

pub fn get_sidechain_features(output: &TransactionOutput) -> Result<&SideChainFeatures, ValidationError> {
    match output.features.sidechain_features.as_ref() {
        Some(features) => Ok(features),
        None => Err(ValidationError::DanLayerError(
            "Sidechain features not found".to_string(),
        )),
    }
}
