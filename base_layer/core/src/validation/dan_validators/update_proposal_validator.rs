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

use super::helpers::{
    fetch_contract_constitution,
    fetch_contract_features,
    get_sidechain_features,
    validate_output_type,
};
use crate::{
    chain_storage::{BlockchainBackend, BlockchainDatabase},
    transactions::transaction_components::{ContractUpdateProposal, OutputType, SideChainFeatures, TransactionOutput},
    validation::{dan_validators::DanLayerValidationError, ValidationError},
};

pub fn validate_update_proposal<B: BlockchainBackend>(
    db: &BlockchainDatabase<B>,
    output: &TransactionOutput,
) -> Result<(), ValidationError> {
    validate_output_type(output, OutputType::ContractConstitutionProposal)?;

    let sidechain_features = get_sidechain_features(output)?;
    let contract_id = sidechain_features.contract_id;

    let proposal_features = get_update_proposal(sidechain_features)?;
    let proposal_id = proposal_features.proposal_id;

    fetch_contract_constitution(db, contract_id)?;

    validate_uniqueness(db, contract_id, proposal_id)?;

    Ok(())
}

fn get_update_proposal(
    sidechain_feature: &SideChainFeatures,
) -> Result<&ContractUpdateProposal, DanLayerValidationError> {
    match sidechain_feature.update_proposal.as_ref() {
        Some(proposal) => Ok(proposal),
        None => Err(DanLayerValidationError::SideChainFeaturesDataNotProvided {
            field_name: "update_proposal",
        }),
    }
}

fn validate_uniqueness<B: BlockchainBackend>(
    db: &BlockchainDatabase<B>,
    contract_id: FixedHash,
    proposal_id: u64,
) -> Result<(), ValidationError> {
    let features = fetch_contract_features(db, contract_id, OutputType::ContractConstitutionProposal)?;
    match features
        .into_iter()
        .filter_map(|feature| feature.update_proposal)
        .find(|proposal| proposal.proposal_id == proposal_id)
    {
        Some(_) => Err(ValidationError::DanLayerError(DanLayerValidationError::DuplicateUtxo {
            contract_id,
            output_type: OutputType::ContractConstitutionProposal,
            details: format!("Proposal ID is {}", proposal_id),
        })),
        None => Ok(()),
    }
}

#[cfg(test)]
mod test {
    use std::convert::TryInto;

    use tari_common_types::types::PublicKey;
    use tari_test_utils::unpack_enum;

    use crate::{
        transactions::transaction_components::OutputType,
        validation::dan_validators::{
            test_helpers::{
                assert_dan_validator_err,
                assert_dan_validator_success,
                create_contract_constitution,
                create_contract_proposal_schema,
                init_test_blockchain,
                publish_constitution,
                publish_definition,
                publish_update_proposal,
                schema_to_transaction,
            },
            DanLayerValidationError,
        },
    };

    #[test]
    fn it_allows_valid_proposals() {
        // initialise a blockchain with enough funds to spend at contract transactions
        let (mut blockchain, utxos) = init_test_blockchain();

        // publish the contract definition into a block
        let contract_id = publish_definition(&mut blockchain, utxos[0].clone());

        // publish the contract constitution into a block
        let validator_node_public_key = PublicKey::default();
        let committee = vec![validator_node_public_key];
        let mut constitution = create_contract_constitution();
        constitution.validator_committee = committee.try_into().unwrap();
        publish_constitution(&mut blockchain, utxos[1].clone(), contract_id, constitution.clone());

        // create a valid proposal transaction
        let proposal_id: u64 = 1;
        let schema = create_contract_proposal_schema(contract_id, utxos[3].clone(), proposal_id, constitution);
        let (tx, _) = schema_to_transaction(&schema);

        assert_dan_validator_success(&blockchain, &tx);
    }

    #[test]
    fn constitution_must_exist() {
        // initialise a blockchain with enough funds to spend at contract transactions
        let (mut blockchain, utxos) = init_test_blockchain();

        // publish the contract definition into a block
        let contract_id = publish_definition(&mut blockchain, utxos[0].clone());

        // skip the contract constitution publication

        // create a contract proposal transaction
        let validator_node_public_key = PublicKey::default();
        let committee = vec![validator_node_public_key];
        let proposal_id: u64 = 1;
        let mut updated_constitution = create_contract_constitution();
        updated_constitution.validator_committee = committee.try_into().unwrap();
        let schema = create_contract_proposal_schema(contract_id, utxos[3].clone(), proposal_id, updated_constitution);
        let (tx, _) = schema_to_transaction(&schema);

        // try to validate the acceptance transaction and check that we get the error
        let err = assert_dan_validator_err(&blockchain, &tx);
        assert!(matches!(
            err,
            DanLayerValidationError::ContractConstitutionNotFound { .. }
        ))
    }

    #[test]
    fn it_rejects_duplicated_proposals() {
        // initialise a blockchain with enough funds to spend at contract transactions
        let (mut blockchain, utxos) = init_test_blockchain();

        // publish the contract definition into a block
        let contract_id = publish_definition(&mut blockchain, utxos[0].clone());

        // publish the contract constitution into a block
        let validator_node_public_key = PublicKey::default();
        let committee = vec![validator_node_public_key];
        let mut constitution = create_contract_constitution();
        constitution.validator_committee = committee.try_into().unwrap();
        publish_constitution(&mut blockchain, utxos[1].clone(), contract_id, constitution.clone());

        // publish a contract update proposal into a block
        let proposal_id: u64 = 1;
        publish_update_proposal(
            &mut blockchain,
            utxos[2].clone(),
            contract_id,
            proposal_id,
            constitution.clone(),
        );

        // create a (duplicated) contract proposal transaction
        let schema = create_contract_proposal_schema(contract_id, utxos[3].clone(), proposal_id, constitution);
        let (tx, _) = schema_to_transaction(&schema);

        // try to validate the duplicated proposal transaction and check that we get the error
        let err = assert_dan_validator_err(&blockchain, &tx);
        let expected_contract_id = contract_id;
        unpack_enum!(
            DanLayerValidationError::DuplicateUtxo {
                output_type,
                contract_id,
                ..
            } = err
        );
        assert_eq!(output_type, OutputType::ContractConstitutionProposal);
        assert_eq!(contract_id, expected_contract_id);
    }
}
