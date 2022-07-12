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

use tari_common_types::types::{Commitment, FixedHash};

use super::helpers::{fetch_contract_constitution, get_sidechain_features};
use crate::{
    chain_storage::{BlockchainBackend, BlockchainDatabase},
    transactions::transaction_components::{
        CheckpointChallenge,
        CommitteeSignatures,
        ContractCheckpoint,
        ContractConstitution,
        OutputType,
        SideChainFeatures,
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
    validate_committee(&constitution, &checkpoint.signatures)?;

    let prev_cp = fetch_current_contract_checkpoint(db, contract_id)?;
    validate_checkpoint_number(prev_cp.as_ref(), checkpoint)?;

    validate_signatures(checkpoint, &contract_id)?;

    Ok(())
}

fn get_checkpoint(sidechain_features: &SideChainFeatures) -> Result<&ContractCheckpoint, DanLayerValidationError> {
    match sidechain_features.checkpoint.as_ref() {
        Some(checkpoint) => Ok(checkpoint),
        None => Err(DanLayerValidationError::MissingContractData {
            contract_id: sidechain_features.contract_id,
            output_type: OutputType::ContractCheckpoint,
        }),
    }
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

fn validate_committee(
    constitution: &ContractConstitution,
    signatures: &CommitteeSignatures,
) -> Result<(), DanLayerValidationError> {
    let committee = &constitution.validator_committee;
    let are_all_signers_in_committee = signatures.into_iter().all(|s| committee.contains(s.signer()));
    if !are_all_signers_in_committee {
        return Err(DanLayerValidationError::InconsistentCommittee);
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

#[cfg(test)]
mod test {
    use super::create_checkpoint_challenge;
    use crate::validation::dan_validators::{
        test_helpers::{
            assert_dan_validator_err,
            assert_dan_validator_success,
            create_committee_signatures,
            create_contract_checkpoint,
            create_contract_checkpoint_schema,
            create_random_key_pair,
            init_test_blockchain,
            publish_checkpoint,
            publish_contract,
            publish_definition,
            schema_to_transaction,
        },
        DanLayerValidationError,
    };

    #[test]
    fn it_allows_initial_checkpoint_output_with_zero_checkpoint_number() {
        // initialise a blockchain with enough funds to spend at contract transactions
        let (mut blockchain, utxos) = init_test_blockchain();

        // Publish a new contract
        let contract_id = publish_contract(&mut blockchain, &utxos, vec![]);

        // Create checkpoint 0 with no prior checkpoints
        let checkpoint = create_contract_checkpoint(0);
        let schema = create_contract_checkpoint_schema(contract_id, utxos[2].clone(), checkpoint);
        let (tx, _) = schema_to_transaction(&schema);

        assert_dan_validator_success(&blockchain, &tx);
    }

    #[test]
    fn it_allows_checkpoint_output_with_correct_sequential_checkpoint_number() {
        // initialise a blockchain with enough funds to spend at contract transactions
        let (mut blockchain, utxos) = init_test_blockchain();

        // Publish a new contract
        let contract_id = publish_contract(&mut blockchain, &utxos, vec![]);

        publish_checkpoint(&mut blockchain, "cp0", utxos[2].clone(), contract_id, 0);
        publish_checkpoint(&mut blockchain, "cp1", utxos[3].clone(), contract_id, 1);
        // Create checkpoint 0 with no prior checkpoints
        let checkpoint = create_contract_checkpoint(2);
        let schema = create_contract_checkpoint_schema(contract_id, utxos[4].clone(), checkpoint);
        let (tx, _) = schema_to_transaction(&schema);

        assert_dan_validator_success(&blockchain, &tx);
    }

    #[test]
    fn it_rejects_initial_checkpoint_output_with_non_zero_checkpoint_number() {
        // initialise a blockchain with enough funds to spend at contract transactions
        let (mut blockchain, utxos) = init_test_blockchain();

        // Publish a new contract
        let contract_id = publish_contract(&mut blockchain, &utxos, vec![]);

        // Create checkpoint 1 with no prior checkpoints
        let checkpoint = create_contract_checkpoint(1);
        let schema = create_contract_checkpoint_schema(contract_id, utxos[2].clone(), checkpoint);
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
        let (mut blockchain, utxos) = init_test_blockchain();

        // Publish a new contract
        let contract_id = publish_contract(&mut blockchain, &utxos, vec![]);

        publish_checkpoint(&mut blockchain, "cp0", utxos[2].clone(), contract_id, 0);
        publish_checkpoint(&mut blockchain, "cp1", utxos[3].clone(), contract_id, 1);
        // Create checkpoint 0 with no prior checkpoints
        let checkpoint = create_contract_checkpoint(3);
        let schema = create_contract_checkpoint_schema(contract_id, utxos[2].clone(), checkpoint);
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
        let (mut blockchain, utxos) = init_test_blockchain();

        // publish the contract definition into a block
        let contract_id = publish_definition(&mut blockchain, utxos[0].clone());

        // skip the contract constitution publication

        // Create a checkpoint
        let checkpoint = create_contract_checkpoint(0);
        let schema = create_contract_checkpoint_schema(contract_id, utxos[1].clone(), checkpoint);
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
        let (mut blockchain, utxos) = init_test_blockchain();

        // Publish a new contract specifying a committee with only one member ("alice")
        let alice = create_random_key_pair();
        let contract_id = publish_contract(&mut blockchain, &utxos, vec![alice.1.clone()]);

        // Create a checkpoint, with a committe that has an extra member ("bob") not present in the constiution
        let mut checkpoint = create_contract_checkpoint(0);
        let bob = create_random_key_pair();
        let challenge = create_checkpoint_challenge(&checkpoint, &contract_id);
        checkpoint.signatures = create_committee_signatures(vec![alice, bob], challenge.as_ref());
        let schema = create_contract_checkpoint_schema(contract_id, utxos[1].clone(), checkpoint);
        let (tx, _) = schema_to_transaction(&schema);

        // try to validate the acceptance transaction and check that we get the error
        let err = assert_dan_validator_err(&blockchain, &tx);
        assert!(matches!(err, DanLayerValidationError::InconsistentCommittee { .. }));
    }

    #[test]
    fn it_rejects_checkpoint_with_invalid_signatures() {
        // initialise a blockchain with enough funds to spend at contract transactions
        let (mut blockchain, utxos) = init_test_blockchain();

        // Publish a new contract specifying a committee with two members
        let alice = create_random_key_pair();
        let mut bob = create_random_key_pair();
        let contract_id = publish_contract(&mut blockchain, &utxos, vec![alice.1.clone(), bob.1.clone()]);

        // To generate an invalid signature, lets swap bob private key for a random private key but keep the public key
        let (altered_private_key, _) = create_random_key_pair();
        bob.0 = altered_private_key;

        // Create a checkpoint with the altered key pair,
        // bob private key is altered compared to the one use in the contract constitution
        let mut checkpoint = create_contract_checkpoint(0);
        let challenge = create_checkpoint_challenge(&checkpoint, &contract_id);
        checkpoint.signatures = create_committee_signatures(vec![alice, bob], challenge.as_ref());

        // Create the invalid transaction
        let schema = create_contract_checkpoint_schema(contract_id, utxos[1].clone(), checkpoint);
        let (tx, _) = schema_to_transaction(&schema);

        // try to validate the checkpoint transaction and check that we get the error
        let err = assert_dan_validator_err(&blockchain, &tx);
        assert!(matches!(err, DanLayerValidationError::InvalidSignature { .. }));
    }
}
