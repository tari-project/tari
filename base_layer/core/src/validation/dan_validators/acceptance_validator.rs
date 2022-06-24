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

use tari_common_types::types::PublicKey;

use super::helpers::{get_contract_constitution, get_sidechain_features, validate_output_type};
use crate::{
    chain_storage::{BlockchainBackend, BlockchainDatabase},
    transactions::transaction_components::{
        ContractAcceptance,
        ContractConstitution,
        OutputType,
        SideChainFeatures,
        TransactionOutput,
    },
    validation::ValidationError,
};

/// This validator checks that the provided output corresponds to a valid Contract Acceptance in the DAN layer
pub fn validate_acceptance<B: BlockchainBackend>(
    db: &BlockchainDatabase<B>,
    output: &TransactionOutput,
) -> Result<(), ValidationError> {
    validate_output_type(output, OutputType::ContractValidatorAcceptance)?;

    let sidechain_features = get_sidechain_features(output)?;
    let contract_id = sidechain_features.contract_id;

    let acceptance_features = get_contract_acceptance(sidechain_features)?;
    let validator_node_public_key = &acceptance_features.validator_node_public_key;

    let constitution = get_contract_constitution(db, contract_id)?;

    validate_public_key(constitution, validator_node_public_key)?;

    // TODO: check that the signature of the transaction is valid
    // TODO: check that the acceptance is inside the accpentance window of the constiution
    // TODO: check that the stake of the transaction is at least the minimum specified in the constitution
    // TODO: check for duplicated acceptances

    Ok(())
}

/// Retrieves a contract acceptance object from the sidechain features, returns an error if not present
fn get_contract_acceptance(sidechain_feature: &SideChainFeatures) -> Result<&ContractAcceptance, ValidationError> {
    match sidechain_feature.acceptance.as_ref() {
        Some(acceptance) => Ok(acceptance),
        None => Err(ValidationError::DanLayerError(
            "Invalid contract acceptance: acceptance features not found".to_string(),
        )),
    }
}

/// Checks that the validator public key is present as part of the proposed committee in the constitution
fn validate_public_key(
    constitution: ContractConstitution,
    validator_node_public_key: &PublicKey,
) -> Result<(), ValidationError> {
    let is_validator_in_committee = constitution
        .validator_committee
        .members()
        .contains(validator_node_public_key);
    if !is_validator_in_committee {
        let msg = format!(
            "Invalid contract acceptance: validator node public key is not in committee ({:?})",
            validator_node_public_key
        );
        return Err(ValidationError::DanLayerError(msg));
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use tari_common_types::types::PublicKey;
    use tari_utilities::hex::Hex;

    use crate::validation::{
        dan_validators::{
            test_helpers::{
                create_block,
                create_contract_acceptance_schema,
                create_contract_constitution_schema,
                init_test_blockchain,
                publish_definition,
                schema_to_transaction,
            },
            TxDanLayerValidator,
        },
        MempoolTransactionValidation,
        ValidationError,
    };

    #[test]
    fn it_rejects_contract_acceptances_of_non_committee_members() {
        let (mut blockchain, change) = init_test_blockchain();
        let contract_id = publish_definition(&mut blockchain, change[0].clone());

        // publish the contract constitution into a block
        // we deliberately use a committee with only a defult public key to be able to trigger the committee error later
        let committee = vec![PublicKey::default()];
        let schema = create_contract_constitution_schema(contract_id, change[1].clone(), committee);
        create_block(&mut blockchain, "constitution", schema);

        // create a contract acceptance transaction
        // we use a public key that is not included in the constitution committee, to trigger the error
        let validator_node_public_key =
            PublicKey::from_hex("70350e09c474809209824c6e6888707b7dd09959aa227343b5106382b856f73a").unwrap();
        let schema = create_contract_acceptance_schema(contract_id, change[2].clone(), validator_node_public_key);
        let (txs, _) = schema_to_transaction(&[schema]);

        // try to validate the acceptance transaction and check that we get the committee error
        let validator = TxDanLayerValidator::new(blockchain.db().clone());
        let err = validator.validate(txs.first().unwrap()).unwrap_err();
        match err {
            ValidationError::DanLayerError(message) => {
                assert!(message.contains("Invalid contract acceptance: validator node public key is not in committee"))
            },
            _ => panic!("Expected a consensus error"),
        }
    }
}
