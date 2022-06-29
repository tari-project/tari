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

use tari_common_types::types::{FixedHash, PublicKey};
use tari_utilities::hex::Hex;

use super::helpers::{
    fetch_contract_features,
    fetch_contract_update_proposal,
    get_sidechain_features,
    validate_output_type,
};
use crate::{
    chain_storage::{BlockchainBackend, BlockchainDatabase},
    transactions::transaction_components::{
        ContractUpdateProposal,
        ContractUpdateProposalAcceptance,
        OutputType,
        SideChainFeatures,
        TransactionOutput,
    },
    validation::ValidationError,
};

pub fn validate_update_proposal_acceptance<B: BlockchainBackend>(
    db: &BlockchainDatabase<B>,
    output: &TransactionOutput,
) -> Result<(), ValidationError> {
    validate_output_type(output, OutputType::ContractConstitutionChangeAcceptance)?;

    let sidechain_features = get_sidechain_features(output)?;
    let contract_id = sidechain_features.contract_id;

    let acceptance_features = get_contract_update_proposal_acceptance(sidechain_features)?;
    let proposal_id = acceptance_features.proposal_id;
    let validator_node_public_key = &acceptance_features.validator_node_public_key;

    let proposal = fetch_contract_update_proposal(db, contract_id, proposal_id)?;

    validate_duplication(db, contract_id, proposal_id, validator_node_public_key)?;
    validate_public_key(proposal, validator_node_public_key)?;

    // TODO: check that the signature of the transaction is valid
    // TODO: check that the acceptance is inside the acceptance window of the proposal
    // TODO: check that the stake of the transaction is at least the minimum specified in the constitution

    Ok(())
}

/// Retrieves a contract update proposal acceptance object from the sidechain features, returns an error if not present
fn get_contract_update_proposal_acceptance(
    sidechain_feature: &SideChainFeatures,
) -> Result<&ContractUpdateProposalAcceptance, ValidationError> {
    match sidechain_feature.update_proposal_acceptance.as_ref() {
        Some(acceptance) => Ok(acceptance),
        None => Err(ValidationError::DanLayerError(
            "Contract update proposal acceptance features not found".to_string(),
        )),
    }
}

/// Checks that the validator node has not already published the acceptance for the contract
fn validate_duplication<B: BlockchainBackend>(
    db: &BlockchainDatabase<B>,
    contract_id: FixedHash,
    proposal_id: u64,
    validator_node_public_key: &PublicKey,
) -> Result<(), ValidationError> {
    let features = fetch_contract_features(db, contract_id, OutputType::ContractConstitutionChangeAcceptance)?;
    match features
        .into_iter()
        .filter_map(|feature| feature.update_proposal_acceptance)
        .find(|feature| {
            feature.validator_node_public_key == *validator_node_public_key && feature.proposal_id == proposal_id
        }) {
        Some(_) => {
            let msg = format!(
                "Duplicated contract update proposal acceptance for contract_id ({:?}), proposal_id ({}) and \
                 validator_node_public_key ({:?})",
                contract_id.to_hex(),
                proposal_id,
                validator_node_public_key,
            );
            Err(ValidationError::DanLayerError(msg))
        },
        None => Ok(()),
    }
}

/// Checks that the validator public key is present as part of the proposed committee in the constitution
fn validate_public_key(
    proposal: ContractUpdateProposal,
    validator_node_public_key: &PublicKey,
) -> Result<(), ValidationError> {
    let is_validator_in_committee = proposal
        .updated_constitution
        .validator_committee
        .members()
        .contains(validator_node_public_key);
    if !is_validator_in_committee {
        let msg = format!(
            "Validator node public key is not in committee ({:?})",
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

    use crate::validation::dan_validators::test_helpers::{
        assert_dan_validator_fail,
        assert_dan_validator_success,
        create_block,
        create_contract_constitution_schema,
        create_contract_update_proposal_acceptance_schema,
        init_test_blockchain,
        publish_constitution,
        publish_definition,
        publish_update_proposal,
        schema_to_transaction,
    };

    #[test]
    fn it_allows_valid_acceptances() {
        // initialise a blockchain with enough funds to spend at contract transactions
        let (mut blockchain, change) = init_test_blockchain();

        // publish the contract definition into a block
        let contract_id = publish_definition(&mut blockchain, change[0].clone());

        // publish the contract constitution into a block
        let validator_node_public_key = PublicKey::default();
        let committee = vec![validator_node_public_key.clone()];
        publish_constitution(&mut blockchain, change[1].clone(), contract_id, committee.clone());

        // publish the contract update proposal into a block
        let proposal_id: u64 = 1;
        publish_update_proposal(&mut blockchain, change[2].clone(), contract_id, proposal_id, committee);

        // create a valid contract acceptance transaction
        let proposal_id = 1;
        let schema = create_contract_update_proposal_acceptance_schema(
            contract_id,
            change[4].clone(),
            proposal_id,
            validator_node_public_key,
        );
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
        let committee = vec![validator_node_public_key.clone()];
        publish_constitution(&mut blockchain, change[1].clone(), contract_id, committee);

        // skip the publication of the contract update proposal

        // create a contract update proposal acceptance transaction
        let proposal_id = 1;
        let schema = create_contract_update_proposal_acceptance_schema(
            contract_id,
            change[1].clone(),
            proposal_id,
            validator_node_public_key,
        );
        let (tx, _) = schema_to_transaction(&schema);

        // try to validate the acceptance transaction and check that we get the error
        assert_dan_validator_fail(&blockchain, &tx, "Contract update proposal not found");
    }

    #[test]
    fn it_rejects_duplicated_acceptances() {
        // initialise a blockchain with enough funds to spend at contract transactions
        let (mut blockchain, change) = init_test_blockchain();

        // publish the contract definition into a block
        let contract_id = publish_definition(&mut blockchain, change[0].clone());

        // publish the contract constitution into a block
        let validator_node_public_key = PublicKey::default();
        let committee = vec![validator_node_public_key.clone()];
        publish_constitution(&mut blockchain, change[1].clone(), contract_id, committee.clone());

        // publish the contract update proposal into a block
        let proposal_id: u64 = 1;
        publish_update_proposal(&mut blockchain, change[2].clone(), contract_id, proposal_id, committee);

        // publish the contract update proposal acceptance into a block
        let proposal_id = 1;
        let schema = create_contract_update_proposal_acceptance_schema(
            contract_id,
            change[3].clone(),
            proposal_id,
            validator_node_public_key.clone(),
        );
        create_block(&mut blockchain, "proposal-acceptance", schema);

        // create a (duplicated) contract acceptance transaction
        let proposal_id = 1;
        let schema = create_contract_update_proposal_acceptance_schema(
            contract_id,
            change[4].clone(),
            proposal_id,
            validator_node_public_key,
        );
        let (tx, _) = schema_to_transaction(&schema);

        // try to validate the (duplicated) proposal acceptance transaction and check that we get the error
        assert_dan_validator_fail(&blockchain, &tx, "Duplicated contract update proposal acceptance");
    }

    #[test]
    fn it_rejects_acceptances_of_non_committee_members() {
        // initialise a blockchain with enough funds to spend at contract transactions
        let (mut blockchain, change) = init_test_blockchain();

        // publish the contract definition into a block
        let contract_id = publish_definition(&mut blockchain, change[0].clone());

        // publish the contract constitution into a block
        let schema = create_contract_constitution_schema(contract_id, change[1].clone(), vec![]);
        create_block(&mut blockchain, "constitution", schema);

        // publish the contract update proposal into a block
        // we deliberately use a committee with only a defult public key to be able to trigger the committee error later
        let proposal_id: u64 = 1;
        let committee = vec![PublicKey::default()];
        publish_update_proposal(&mut blockchain, change[2].clone(), contract_id, proposal_id, committee);

        // publish the contract update proposal acceptance into a block
        // we use a public key that is not included in the proposal committee, to trigger the error
        let validator_node_public_key =
            PublicKey::from_hex("70350e09c474809209824c6e6888707b7dd09959aa227343b5106382b856f73a").unwrap();
        let proposal_id = 1;
        let schema = create_contract_update_proposal_acceptance_schema(
            contract_id,
            change[3].clone(),
            proposal_id,
            validator_node_public_key,
        );
        let (tx, _) = schema_to_transaction(&schema);

        // try to validate the proposal acceptance transaction and check that we get the committee error
        assert_dan_validator_fail(&blockchain, &tx, "Validator node public key is not in committee");
    }
}
