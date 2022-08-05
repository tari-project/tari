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

use std::{collections::HashSet, iter::FromIterator};

use tari_common_types::types::{FixedHash, PublicKey};

use super::helpers::{
    create_checkpoint_challenge,
    fetch_contract_constitution,
    get_checkpoint,
    get_commitee_members,
    get_sidechain_features,
};
use crate::{
    chain_storage::{BlockchainBackend, BlockchainDatabase},
    transactions::transaction_components::{
        ContractCheckpoint,
        ContractConstitution,
        SignerSignature,
        TransactionOutput,
    },
    validation::{
        dan_validators::{helpers::fetch_current_contract_checkpoint, DanLayerValidationError},
        ValidationError,
    },
};

pub fn validate_contract_checkpoint<B: BlockchainBackend>(
    db: &BlockchainDatabase<B>,
    output: &TransactionOutput,
) -> Result<(), ValidationError> {
    let sidechain_features = get_sidechain_features(output)?;
    let contract_id = sidechain_features.contract_id;
    let checkpoint = get_checkpoint(sidechain_features)?;

    let constitution = fetch_contract_constitution(db, contract_id)?;
    validate_committee(checkpoint, &constitution)?;

    let prev_cp = fetch_current_contract_checkpoint(db, contract_id)?;
    validate_checkpoint_number(prev_cp.as_ref(), checkpoint)?;

    validate_signatures(checkpoint, &contract_id)?;

    Ok(())
}

fn validate_checkpoint_number(
    prev_checkpoint: Option<&ContractCheckpoint>,
    current_checkpoint: &ContractCheckpoint,
) -> Result<(), DanLayerValidationError> {
    let expected_number = prev_checkpoint.map(|cp| cp.checkpoint_number + 1).unwrap_or(0);
    if current_checkpoint.checkpoint_number == expected_number {
        Ok(())
    } else {
        Err(DanLayerValidationError::CheckpointNonSequentialNumber {
            got: current_checkpoint.checkpoint_number,
            expected: expected_number,
        })
    }
}

#[allow(clippy::mutable_key_type)]
fn validate_committee(
    checkpoint: &ContractCheckpoint,
    constitution: &ContractConstitution,
) -> Result<(), DanLayerValidationError> {
    // retrieve the list of commitee member keys of the constiution and the checkpoint
    let checkpoint_members = get_commitee_members(&checkpoint.signatures);
    let constitution_members = constitution.validator_committee.members();

    // we use HashSets to avoid dealing with duplicated members and to easily compare elements
    let checkpoint_member_set = HashSet::<&PublicKey>::from_iter(checkpoint_members);
    let constitution_member_set = HashSet::<&PublicKey>::from_iter(constitution_members.iter());

    // an non-empty difference (calculated from the checkpoint) means that there are non-constitution members
    let are_invalid_members = checkpoint_member_set.difference(&constitution_member_set).count() > 0;
    if are_invalid_members {
        return Err(DanLayerValidationError::InconsistentCommittee);
    }

    // the intersection allow us to calculate the effective quorum of the checkpoint
    let checkpoint_quorum = checkpoint_member_set.intersection(&constitution_member_set).count() as u32;
    let required_quorum = constitution.checkpoint_params.minimum_quorum_required;
    let is_quorum_met = checkpoint_quorum >= required_quorum;
    if !is_quorum_met {
        return Err(DanLayerValidationError::InsufficientQuorum {
            got: checkpoint_quorum,
            minimum: required_quorum,
        });
    }

    Ok(())
}

pub fn validate_signatures(checkpoint: &ContractCheckpoint, contract_id: &FixedHash) -> Result<(), ValidationError> {
    let challenge = create_checkpoint_challenge(checkpoint, contract_id);
    let signatures = &checkpoint.signatures;

    let are_all_signatures_valid = signatures
        .into_iter()
        .all(|s| SignerSignature::verify(s.signature(), s.signer(), challenge));
    if !are_all_signatures_valid {
        return Err(ValidationError::DanLayerError(
            DanLayerValidationError::InvalidSignature,
        ));
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use std::convert::TryInto;

    use tari_common_types::types::FixedHash;

    use crate::{
        consensus::ConsensusHasher,
        validation::dan_validators::{
            helpers::create_checkpoint_challenge,
            test_helpers::{
                assert_dan_validator_err,
                assert_dan_validator_success,
                create_committee_signatures,
                create_contract_checkpoint,
                create_contract_checkpoint_schema,
                create_contract_constitution,
                create_random_key_pair,
                init_test_blockchain,
                publish_checkpoint,
                publish_constitution,
                publish_contract,
                publish_definition,
                schema_to_transaction,
            },
            DanLayerValidationError,
        },
    };

    #[test]
    fn it_allows_initial_checkpoint_output_with_zero_checkpoint_number() {
        // initialise a blockchain with enough funds to spend at contract transactions
        let (mut blockchain, mut utxos) = init_test_blockchain();

        // Publish a new contract
        let contract_id = publish_contract(&mut blockchain, &mut utxos, vec![]);

        // Create checkpoint 0 with no prior checkpoints
        let checkpoint = create_contract_checkpoint(0);
        let schema = create_contract_checkpoint_schema(contract_id, utxos.next().unwrap(), checkpoint);
        let (tx, _) = schema_to_transaction(&schema);

        assert_dan_validator_success(&blockchain, &tx);
    }

    #[test]
    fn it_allows_checkpoint_output_with_correct_sequential_checkpoint_number() {
        // initialise a blockchain with enough funds to spend at contract transactions
        let (mut blockchain, mut utxos) = init_test_blockchain();

        // Publish a new contract
        let contract_id = publish_contract(&mut blockchain, &mut utxos, vec![]);

        publish_checkpoint(&mut blockchain, "cp0", utxos.next().unwrap(), contract_id, 0);
        publish_checkpoint(&mut blockchain, "cp1", utxos.next().unwrap(), contract_id, 1);
        // Create checkpoint 0 with no prior checkpoints
        let checkpoint = create_contract_checkpoint(2);
        let schema = create_contract_checkpoint_schema(contract_id, utxos.next().unwrap(), checkpoint);
        let (tx, _) = schema_to_transaction(&schema);

        assert_dan_validator_success(&blockchain, &tx);
    }

    #[test]
    fn it_rejects_initial_checkpoint_output_with_non_zero_checkpoint_number() {
        // initialise a blockchain with enough funds to spend at contract transactions
        let (mut blockchain, mut utxos) = init_test_blockchain();

        // Publish a new contract
        let contract_id = publish_contract(&mut blockchain, &mut utxos, vec![]);

        // Create checkpoint 1 with no prior checkpoints
        let checkpoint = create_contract_checkpoint(1);
        let schema = create_contract_checkpoint_schema(contract_id, utxos.next().unwrap(), checkpoint);
        let (tx, _) = schema_to_transaction(&schema);

        let err = assert_dan_validator_err(&blockchain, &tx);
        assert!(matches!(err, DanLayerValidationError::CheckpointNonSequentialNumber {
            got: 1,
            expected: 0
        }))
    }

    #[test]
    fn it_rejects_checkpoint_output_with_non_sequential_checkpoint_number() {
        // initialise a blockchain with enough funds to spend at contract transactions
        let (mut blockchain, mut utxos) = init_test_blockchain();

        // Publish a new contract
        let contract_id = publish_contract(&mut blockchain, &mut utxos, vec![]);

        publish_checkpoint(&mut blockchain, "cp0", utxos.next().unwrap(), contract_id, 0);
        publish_checkpoint(&mut blockchain, "cp1", utxos.next().unwrap(), contract_id, 1);
        // Create checkpoint 0 with no prior checkpoints
        let checkpoint = create_contract_checkpoint(3);
        let schema = create_contract_checkpoint_schema(contract_id, utxos.next().unwrap(), checkpoint);
        let (tx, _) = schema_to_transaction(&schema);

        let err = assert_dan_validator_err(&blockchain, &tx);
        assert!(matches!(err, DanLayerValidationError::CheckpointNonSequentialNumber {
            got: 3,
            expected: 2
        }))
    }

    #[test]
    fn constitution_must_exist() {
        // initialise a blockchain with enough funds to spend at contract transactions
        let (mut blockchain, mut utxos) = init_test_blockchain();

        // publish the contract definition into a block
        let contract_id = publish_definition(&mut blockchain, utxos.next().unwrap());

        // skip the contract constitution publication

        // Create a checkpoint
        let checkpoint = create_contract_checkpoint(0);
        let schema = create_contract_checkpoint_schema(contract_id, utxos.next().unwrap(), checkpoint);
        let (tx, _) = schema_to_transaction(&schema);

        // try to validate the acceptance transaction and check that we get the error
        let err = assert_dan_validator_err(&blockchain, &tx);
        assert!(matches!(
            err,
            DanLayerValidationError::ContractConstitutionNotFound { .. }
        ));
    }

    #[test]
    fn it_rejects_checkpoints_with_non_committee_members() {
        // initialise a blockchain with enough funds to spend at contract transactions
        let (mut blockchain, mut utxos) = init_test_blockchain();

        // Publish a new contract specifying a committee with only one member ("alice")
        let alice = create_random_key_pair();
        let contract_id = publish_contract(&mut blockchain, &mut utxos, vec![alice.1.clone()]);

        // Create a checkpoint, with a committe that has an extra member ("bob") not present in the constiution
        let mut checkpoint = create_contract_checkpoint(0);
        let bob = create_random_key_pair();
        let challenge = create_checkpoint_challenge(&checkpoint, &contract_id);
        checkpoint.signatures = create_committee_signatures(vec![alice, bob], challenge.as_ref());
        let schema = create_contract_checkpoint_schema(contract_id, utxos.next().unwrap(), checkpoint);
        let (tx, _) = schema_to_transaction(&schema);

        // try to validate the acceptance transaction and check that we get the error
        let err = assert_dan_validator_err(&blockchain, &tx);
        assert!(matches!(err, DanLayerValidationError::InconsistentCommittee { .. }));
    }

    #[test]
    fn it_rejects_checkpoint_with_invalid_signatures() {
        // initialise a blockchain with enough funds to spend at contract transactions
        let (mut blockchain, mut utxos) = init_test_blockchain();

        // Publish a new contract specifying a committee with two members
        let alice = create_random_key_pair();
        let bob = create_random_key_pair();
        let contract_id = publish_contract(&mut blockchain, &mut utxos, vec![alice.1.clone(), bob.1.clone()]);

        // To create an invalid signature, let's use a challenge from a different checkpoint
        let mut checkpoint = create_contract_checkpoint(0);
        let challenge: FixedHash = ConsensusHasher::default()
            .chain(&"invalid data".as_bytes())
            .finalize()
            .into();
        checkpoint.signatures = create_committee_signatures(vec![alice, bob], challenge.as_ref());

        // Create the invalid transaction
        let schema = create_contract_checkpoint_schema(contract_id, utxos.next().unwrap(), checkpoint);
        let (tx, _) = schema_to_transaction(&schema);

        // try to validate the checkpoint transaction and check that we get the error
        let err = assert_dan_validator_err(&blockchain, &tx);
        assert!(matches!(err, DanLayerValidationError::InvalidSignature { .. }));
    }

    #[test]
    fn it_rejects_checkpoints_with_insufficient_quorum() {
        // initialise a blockchain with enough funds to spend at contract transactions
        let (mut blockchain, mut utxos) = init_test_blockchain();

        // publish the contract definition into a block
        let contract_id = publish_definition(&mut blockchain, utxos.next().unwrap());

        // Publish a new contract constitution specifying a minimum quorum of 2
        let mut constitution = create_contract_constitution();
        let alice = create_random_key_pair();
        let bob = create_random_key_pair();
        let carol = create_random_key_pair();
        constitution.validator_committee = vec![alice.1.clone(), bob.1, carol.1].try_into().unwrap();
        constitution.checkpoint_params.minimum_quorum_required = 2;
        publish_constitution(&mut blockchain, utxos.next().unwrap(), contract_id, constitution);

        // create a checkpoint with an insufficient quorum
        let mut checkpoint = create_contract_checkpoint(0);
        let challenge = create_checkpoint_challenge(&checkpoint, &contract_id);
        checkpoint.signatures = create_committee_signatures(vec![alice], challenge.as_ref());
        let schema = create_contract_checkpoint_schema(contract_id, utxos.next().unwrap(), checkpoint);
        let (tx, _) = schema_to_transaction(&schema);

        // try to validate the acceptance transaction and check that we get the error
        let err = assert_dan_validator_err(&blockchain, &tx);
        assert!(matches!(err, DanLayerValidationError::InsufficientQuorum {
            got: 1,
            minimum: 2
        }));
    }

    #[test]
    fn it_accepts_checkpoints_with_sufficient_quorum() {
        // initialise a blockchain with enough funds to spend at contract transactions
        let (mut blockchain, mut utxos) = init_test_blockchain();

        // publish the contract definition into a block
        let contract_id = publish_definition(&mut blockchain, utxos.next().unwrap());

        // Publish a new contract constitution specifying a minimum quorum of 2
        let mut constitution = create_contract_constitution();
        let alice = create_random_key_pair();
        let bob = create_random_key_pair();
        constitution.validator_committee = vec![alice.1.clone(), bob.1.clone()].try_into().unwrap();
        constitution.checkpoint_params.minimum_quorum_required = 2;
        publish_constitution(&mut blockchain, utxos.next().unwrap(), contract_id, constitution);

        // create a checkpoint with an enough quorum
        let mut checkpoint = create_contract_checkpoint(0);
        let challenge = create_checkpoint_challenge(&checkpoint, &contract_id);
        checkpoint.signatures = create_committee_signatures(vec![alice, bob], challenge.as_ref());
        let schema = create_contract_checkpoint_schema(contract_id, utxos.next().unwrap(), checkpoint);
        let (tx, _) = schema_to_transaction(&schema);

        assert_dan_validator_success(&blockchain, &tx);
    }
}
