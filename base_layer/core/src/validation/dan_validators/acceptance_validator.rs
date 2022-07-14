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

use tari_common_types::types::{FixedHash, PublicKey, Signature};
use tari_utilities::hex::Hex;

use super::helpers::{
    fetch_constitution_commitment,
    fetch_constitution_height,
    fetch_contract_constitution,
    fetch_contract_features,
    get_contract_acceptance,
    get_sidechain_features,
    validate_output_type,
};
use crate::{
    chain_storage::{BlockchainBackend, BlockchainDatabase},
    transactions::transaction_components::{
        ContractAcceptanceChallenge,
        ContractConstitution,
        OutputType,
        SignerSignature,
        TransactionOutput,
    },
    validation::{dan_validators::DanLayerValidationError, ValidationError},
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
    let signature = &acceptance_features.signature;

    let constitution = fetch_contract_constitution(db, contract_id)?;

    validate_uniqueness(db, contract_id, validator_node_public_key)?;
    validate_public_key(&constitution, validator_node_public_key)?;
    validate_acceptance_window(db, contract_id, &constitution)?;
    validate_signature(db, signature, contract_id, validator_node_public_key)?;

    // TODO: check that the stake of the transaction is at least the minimum specified in the constitution

    Ok(())
}

/// Checks that the validator node has not already published the acceptance for the contract
fn validate_uniqueness<B: BlockchainBackend>(
    db: &BlockchainDatabase<B>,
    contract_id: FixedHash,
    validator_node_public_key: &PublicKey,
) -> Result<(), ValidationError> {
    let features = fetch_contract_features(db, contract_id, OutputType::ContractValidatorAcceptance)?;
    match features
        .into_iter()
        .filter_map(|feature| feature.acceptance)
        .find(|feature| feature.validator_node_public_key == *validator_node_public_key)
    {
        Some(_) => Err(ValidationError::DanLayerError(DanLayerValidationError::DuplicateUtxo {
            contract_id,
            output_type: OutputType::ContractValidatorAcceptance,
            details: format!(
                "Validator ({}) sent duplicate acceptance UTXO",
                validator_node_public_key.to_hex(),
            ),
        })),
        None => Ok(()),
    }
}

/// Checks that the validator public key is present as part of the proposed committee in the constitution
fn validate_public_key(
    constitution: &ContractConstitution,
    validator_node_public_key: &PublicKey,
) -> Result<(), DanLayerValidationError> {
    let is_validator_in_committee = constitution.validator_committee.contains(validator_node_public_key);
    if !is_validator_in_committee {
        return Err(DanLayerValidationError::ValidatorNotInCommittee {
            public_key: validator_node_public_key.to_hex(),
        });
    }

    Ok(())
}

fn validate_acceptance_window<B: BlockchainBackend>(
    db: &BlockchainDatabase<B>,
    contract_id: FixedHash,
    constitution: &ContractConstitution,
) -> Result<(), ValidationError> {
    let constitution_height = fetch_constitution_height(db, contract_id)?;
    let max_allowed_absolute_height =
        constitution_height + constitution.acceptance_requirements.acceptance_period_expiry;
    let current_height = db.get_height()?;

    let window_has_expired = current_height > max_allowed_absolute_height;
    if window_has_expired {
        return Err(ValidationError::DanLayerError(
            DanLayerValidationError::AcceptanceWindowHasExpired { contract_id },
        ));
    }

    Ok(())
}

pub fn validate_signature<B: BlockchainBackend>(
    db: &BlockchainDatabase<B>,
    signature: &Signature,
    contract_id: FixedHash,
    validator_node_public_key: &PublicKey,
) -> Result<(), ValidationError> {
    let commitment = fetch_constitution_commitment(db, contract_id)?;
    let challenge = ContractAcceptanceChallenge::new(&commitment, &contract_id);

    let is_valid_signature = SignerSignature::verify(signature, validator_node_public_key, challenge);
    if !is_valid_signature {
        return Err(ValidationError::DanLayerError(
            DanLayerValidationError::InvalidSignature,
        ));
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use std::convert::TryInto;

    use tari_common_types::types::{Commitment, PublicKey};

    use super::fetch_constitution_commitment;
    use crate::{
        txn_schema,
        validation::dan_validators::{
            test_helpers::{
                assert_dan_validator_err,
                assert_dan_validator_success,
                create_acceptance_signature,
                create_block,
                create_contract_acceptance_schema,
                create_contract_acceptance_schema_with_signature,
                create_contract_constitution,
                create_random_key_pair,
                init_test_blockchain,
                publish_constitution,
                publish_contract,
                publish_definition,
                schema_to_transaction,
            },
            DanLayerValidationError,
        },
    };

    #[test]
    fn it_allows_valid_acceptances() {
        // initialise a blockchain with enough funds to spend at contract transactions
        let (mut blockchain, utxos) = init_test_blockchain();

        // publish the contract definition and constitution into a block
        let (private_key, public_key) = create_random_key_pair();
        let contract_id = publish_contract(&mut blockchain, &utxos, vec![public_key.clone()]);

        // create a valid contract acceptance transaction
        let commitment = fetch_constitution_commitment(blockchain.db(), contract_id).unwrap();
        let schema =
            create_contract_acceptance_schema(contract_id, commitment, utxos[2].clone(), private_key, public_key);
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

        // create a contract acceptance transaction
        let (private_key, public_key) = create_random_key_pair();
        let commitment = Commitment::default();
        let schema =
            create_contract_acceptance_schema(contract_id, commitment, utxos[1].clone(), private_key, public_key);
        let (tx, _) = schema_to_transaction(&schema);

        // try to validate the acceptance transaction and check that we get the error
        let err = assert_dan_validator_err(&blockchain, &tx);
        assert!(matches!(
            err,
            DanLayerValidationError::ContractConstitutionNotFound { .. }
        ))
    }

    #[test]
    fn it_rejects_duplicated_acceptances() {
        // initialise a blockchain with enough funds to spend at contract transactions
        let (mut blockchain, utxos) = init_test_blockchain();

        // publish the contract definition and constitution into a block
        let (private_key, public_key) = create_random_key_pair();
        let contract_id = publish_contract(&mut blockchain, &utxos, vec![public_key.clone()]);

        // publish a contract acceptance into a block
        let commitment = fetch_constitution_commitment(blockchain.db(), contract_id).unwrap();
        let schema = create_contract_acceptance_schema(
            contract_id,
            commitment.clone(),
            utxos[2].clone(),
            private_key.clone(),
            public_key.clone(),
        );
        create_block(&mut blockchain, "acceptance", schema);

        // create a (duplicated) contract acceptance transaction
        let schema =
            create_contract_acceptance_schema(contract_id, commitment, utxos[3].clone(), private_key, public_key);
        let (tx, _) = schema_to_transaction(&schema);

        // try to validate the duplicated acceptance transaction and check that we get the error
        let err = assert_dan_validator_err(&blockchain, &tx);
        assert!(matches!(err, DanLayerValidationError::DuplicateUtxo { .. }));
    }

    #[test]
    fn it_rejects_acceptances_of_non_committee_members() {
        // initialise a blockchain with enough funds to spend at contract transactions
        let (mut blockchain, utxos) = init_test_blockchain();

        // publish the contract definition and constitution into a block
        // we deliberately use a committee with only a defult public key to be able to trigger the committee error later
        let contract_id = publish_contract(&mut blockchain, &utxos, vec![PublicKey::default()]);

        // create a contract acceptance transaction
        // we use a public key that is not included in the constitution committee, to trigger the error
        let (private_key, public_key) = create_random_key_pair();
        let commitment = fetch_constitution_commitment(blockchain.db(), contract_id).unwrap();
        let schema =
            create_contract_acceptance_schema(contract_id, commitment, utxos[2].clone(), private_key, public_key);
        let (tx, _) = schema_to_transaction(&schema);

        // try to validate the acceptance transaction and check that we get the committee error
        let err = assert_dan_validator_err(&blockchain, &tx);
        assert!(matches!(err, DanLayerValidationError::ValidatorNotInCommittee { .. }));
    }

    #[test]
    fn it_rejects_expired_acceptances() {
        // initialise a blockchain with enough funds to spend at contract transactions
        let (mut blockchain, utxos) = init_test_blockchain();

        // publish the contract definition into a block
        let contract_id = publish_definition(&mut blockchain, utxos[0].clone());

        // publish the contract constitution into a block, with a very short (1 block) expiration time
        let (private_key, public_key) = create_random_key_pair();
        let committee = vec![public_key.clone()];
        let mut constitution = create_contract_constitution();
        constitution.validator_committee = committee.try_into().unwrap();
        constitution.acceptance_requirements.acceptance_period_expiry = 1;
        publish_constitution(&mut blockchain, utxos[1].clone(), contract_id, constitution);

        // publish some filler blocks in, just to make the expiration height pass
        let schema = txn_schema!(from: vec![utxos[2].clone()], to: vec![0.into()]);
        create_block(&mut blockchain, "filler1", schema);
        let schema = txn_schema!(from: vec![utxos[3].clone()], to: vec![0.into()]);
        create_block(&mut blockchain, "filler2", schema);

        // create a contract acceptance after the expiration block height
        let commitment = fetch_constitution_commitment(blockchain.db(), contract_id).unwrap();
        let schema =
            create_contract_acceptance_schema(contract_id, commitment, utxos[4].clone(), private_key, public_key);
        let (tx, _) = schema_to_transaction(&schema);

        // try to validate the acceptance transaction and check that we get the expiration error
        let err = assert_dan_validator_err(&blockchain, &tx);
        assert!(matches!(
            err,
            DanLayerValidationError::AcceptanceWindowHasExpired { .. }
        ));
    }

    #[test]
    fn it_rejects_acceptances_with_invalid_signatures() {
        // initialise a blockchain with enough funds to spend at contract transactions
        let (mut blockchain, utxos) = init_test_blockchain();

        // publish the contract definition and constitution into a block
        let (_, public_key) = create_random_key_pair();
        let contract_id = publish_contract(&mut blockchain, &utxos, vec![public_key.clone()]);

        // create a valid contract acceptance transaction, but with a signature done by a different private key
        let (altered_private_key, _) = create_random_key_pair();
        let commitment = fetch_constitution_commitment(blockchain.db(), contract_id).unwrap();
        let signature = create_acceptance_signature(contract_id, commitment, altered_private_key);
        let schema =
            create_contract_acceptance_schema_with_signature(contract_id, utxos[2].clone(), public_key, signature);
        let (tx, _) = schema_to_transaction(&schema);

        // try to validate the acceptance transaction and check that we get the error
        let err = assert_dan_validator_err(&blockchain, &tx);
        assert!(matches!(err, DanLayerValidationError::InvalidSignature { .. }));
    }
}
