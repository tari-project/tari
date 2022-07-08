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

use super::helpers::{fetch_contract_constitution, get_sidechain_features};
use crate::{
    chain_storage::{BlockchainBackend, BlockchainDatabase},
    transactions::transaction_components::{ContractCheckpoint, OutputType, SideChainFeatures, TransactionOutput},
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
    fetch_contract_constitution(db, contract_id)?;

    let prev_cp = fetch_current_contract_checkpoint(db, contract_id)?;
    validate_checkpoint_number(prev_cp.as_ref(), sidechain_features)?;

    Ok(())
}

fn validate_checkpoint_number(
    prev_checkpoint: Option<&ContractCheckpoint>,
    sidechain_features: &SideChainFeatures,
) -> Result<(), DanLayerValidationError> {
    let checkpoint = sidechain_features
        .checkpoint
        .as_ref()
        .ok_or(DanLayerValidationError::MissingContractData {
            contract_id: sidechain_features.contract_id,
            output_type: OutputType::ContractCheckpoint,
        })?;

    let expected_number = prev_checkpoint.map(|cp| cp.checkpoint_number + 1).unwrap_or(0);
    if checkpoint.checkpoint_number == expected_number {
        Ok(())
    } else {
        Err(DanLayerValidationError::CheckpointNonSequentialNumber {
            got: checkpoint.checkpoint_number,
            expected: expected_number,
        })
    }
}

#[cfg(test)]
mod test {
    use crate::validation::dan_validators::{
        test_helpers::{
            assert_dan_validator_err,
            assert_dan_validator_success,
            create_contract_checkpoint,
            create_contract_checkpoint_schema,
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

        // Create checkpoint 0 with no prior checkpoints
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
}
