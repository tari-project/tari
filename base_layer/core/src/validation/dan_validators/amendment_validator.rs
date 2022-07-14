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
    fetch_contract_features,
    fetch_contract_update_proposal,
    get_sidechain_features,
    validate_output_type,
};
use crate::{
    chain_storage::{BlockchainBackend, BlockchainDatabase},
    transactions::transaction_components::{
        ContractAmendment,
        ContractUpdateProposal,
        OutputType,
        SideChainFeatures,
        TransactionOutput,
    },
    validation::{dan_validators::DanLayerValidationError, ValidationError},
};

pub fn validate_amendment<B: BlockchainBackend>(
    db: &BlockchainDatabase<B>,
    output: &TransactionOutput,
) -> Result<(), ValidationError> {
    validate_output_type(output, OutputType::ContractAmendment)?;

    let sidechain_features = get_sidechain_features(output)?;
    let contract_id = sidechain_features.contract_id;

    let amendment = get_contract_amendment(sidechain_features)?;
    let proposal_id = amendment.proposal_id;
    let proposal = fetch_contract_update_proposal(db, contract_id, proposal_id)?;

    validate_uniqueness(db, contract_id, proposal_id)?;
    validate_updated_constiution(amendment, &proposal)?;

    Ok(())
}

fn get_contract_amendment(
    sidechain_feature: &SideChainFeatures,
) -> Result<&ContractAmendment, DanLayerValidationError> {
    match sidechain_feature.amendment.as_ref() {
        Some(amendment) => Ok(amendment),
        None => Err(DanLayerValidationError::SideChainFeaturesDataNotProvided {
            field_name: "amendment",
        }),
    }
}

fn validate_uniqueness<B: BlockchainBackend>(
    db: &BlockchainDatabase<B>,
    contract_id: FixedHash,
    proposal_id: u64,
) -> Result<(), ValidationError> {
    let features = fetch_contract_features(db, contract_id, OutputType::ContractAmendment)?;
    match features
        .into_iter()
        .filter_map(|feature| feature.amendment)
        .find(|amendment| amendment.proposal_id == proposal_id)
    {
        Some(_) => Err(ValidationError::DanLayerError(DanLayerValidationError::DuplicateUtxo {
            contract_id,
            output_type: OutputType::ContractAmendment,
            details: format!("proposal_id = {}", proposal_id),
        })),
        None => Ok(()),
    }
}

fn validate_updated_constiution(
    amendment: &ContractAmendment,
    proposal: &ContractUpdateProposal,
) -> Result<(), DanLayerValidationError> {
    if amendment.updated_constitution != proposal.updated_constitution {
        return Err(DanLayerValidationError::UpdatedConstitutionAmendmentMismatch);
    }

    Ok(())
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
                create_block,
                create_contract_amendment_schema,
                create_contract_constitution,
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
    fn it_allows_valid_amendments() {
        // initialise a blockchain with enough funds to spend at contract transactions
        let (mut blockchain, change) = init_test_blockchain();

        // publish the contract definition into a block
        let contract_id = publish_definition(&mut blockchain, change[0].clone());

        // publish the contract constitution into a block
        let validator_node_public_key = PublicKey::default();
        let committee = vec![validator_node_public_key];
        let mut constitution = create_contract_constitution();
        constitution.validator_committee = committee.try_into().unwrap();
        publish_constitution(&mut blockchain, change[1].clone(), contract_id, constitution.clone());

        // publish a contract update proposal into a block
        let proposal_id: u64 = 1;
        publish_update_proposal(
            &mut blockchain,
            change[2].clone(),
            contract_id,
            proposal_id,
            constitution.clone(),
        );

        // create a valid amendment transaction
        let proposal_id = 1;
        let schema = create_contract_amendment_schema(contract_id, change[3].clone(), proposal_id, constitution);
        let (tx, _) = schema_to_transaction(&schema);
        assert_dan_validator_success(&blockchain, &tx);
    }

    #[test]
    fn proposal_must_exist() {
        // initialise a blockchain with enough funds to spend at contract transactions
        let (mut blockchain, change) = init_test_blockchain();

        // publish the contract definition into a block
        let contract_id = publish_definition(&mut blockchain, change[0].clone());

        // publish the contract constitution into a block
        let validator_node_public_key = PublicKey::default();
        let committee = vec![validator_node_public_key];
        let mut constitution = create_contract_constitution();
        constitution.validator_committee = committee.try_into().unwrap();
        publish_constitution(&mut blockchain, change[1].clone(), contract_id, constitution.clone());

        // skip the publication of the contract update proposal

        // create an amendment transaction
        let proposal_id = 1;
        let schema = create_contract_amendment_schema(contract_id, change[1].clone(), proposal_id, constitution);
        let (tx, _) = schema_to_transaction(&schema);

        // try to validate the acceptance transaction and check that we get the error
        let err = assert_dan_validator_err(&blockchain, &tx);
        assert!(matches!(
            err,
            DanLayerValidationError::ContractUpdateProposalNotFound { .. }
        ))
    }

    #[test]
    fn it_rejects_duplicated_amendments() {
        // initialise a blockchain with enough funds to spend at contract transactions
        let (mut blockchain, change) = init_test_blockchain();

        // publish the contract definition into a block
        let contract_id = publish_definition(&mut blockchain, change[0].clone());

        // publish the contract constitution into a block
        let validator_node_public_key = PublicKey::default();
        let committee = vec![validator_node_public_key];
        let mut constitution = create_contract_constitution();
        constitution.validator_committee = committee.try_into().unwrap();
        publish_constitution(&mut blockchain, change[1].clone(), contract_id, constitution.clone());

        // publish a contract update proposal into a block
        let proposal_id: u64 = 1;
        publish_update_proposal(
            &mut blockchain,
            change[2].clone(),
            contract_id,
            proposal_id,
            constitution.clone(),
        );

        // publish the contract amendment into a block
        let schema =
            create_contract_amendment_schema(contract_id, change[3].clone(), proposal_id, constitution.clone());
        create_block(&mut blockchain, "amendment", schema);

        // create a (duplicated) contract amendment transaction
        let schema = create_contract_amendment_schema(contract_id, change[4].clone(), proposal_id, constitution);
        let (tx, _) = schema_to_transaction(&schema);

        // try to validate the duplicated amendment transaction and check that we get the error
        let err = assert_dan_validator_err(&blockchain, &tx);
        let expected_contract_id = contract_id;
        unpack_enum!(
            DanLayerValidationError::DuplicateUtxo {
                output_type,
                contract_id,
                ..
            } = err
        );
        assert_eq!(output_type, OutputType::ContractAmendment);
        assert_eq!(contract_id, expected_contract_id);
    }

    #[test]
    fn it_rejects_altered_updated_constitution() {
        // initialise a blockchain with enough funds to spend at contract transactions
        let (mut blockchain, change) = init_test_blockchain();

        // publish the contract definition into a block
        let contract_id = publish_definition(&mut blockchain, change[0].clone());

        // publish the contract constitution into a block
        let validator_node_public_key = PublicKey::default();
        let committee = vec![validator_node_public_key];
        let mut constitution = create_contract_constitution();
        constitution.validator_committee = committee.try_into().unwrap();
        publish_constitution(&mut blockchain, change[1].clone(), contract_id, constitution.clone());

        // publish a contract update proposal into a block
        let proposal_id: u64 = 1;
        publish_update_proposal(
            &mut blockchain,
            change[2].clone(),
            contract_id,
            proposal_id,
            constitution,
        );

        // create an amendment with an altered committee (compared to the proposal)
        let mut altered_constitution = create_contract_constitution();
        altered_constitution.validator_committee = vec![].try_into().unwrap();
        let schema =
            create_contract_amendment_schema(contract_id, change[4].clone(), proposal_id, altered_constitution);
        let (tx, _) = schema_to_transaction(&schema);

        // try to validate the amendment transaction and check that we get the error
        let err = assert_dan_validator_err(&blockchain, &tx);
        assert!(matches!(
            err,
            DanLayerValidationError::UpdatedConstitutionAmendmentMismatch { .. }
        ))
    }
}
