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

use super::helpers::{fetch_contract_features, get_sidechain_features, validate_output_type};
use crate::{
    chain_storage::{BlockchainBackend, BlockchainDatabase},
    transactions::transaction_components::{OutputType, TransactionOutput},
    validation::{dan_validators::DanLayerValidationError, ValidationError},
};

pub fn validate_constitution<B: BlockchainBackend>(
    db: &BlockchainDatabase<B>,
    output: &TransactionOutput,
) -> Result<(), ValidationError> {
    validate_output_type(output, OutputType::ContractConstitution)?;

    let sidechain_features = get_sidechain_features(output)?;
    let contract_id = sidechain_features.contract_id;

    validate_definition_existence(db, contract_id)?;
    validate_uniqueness(db, contract_id)?;

    Ok(())
}

pub fn validate_definition_existence<B: BlockchainBackend>(
    db: &BlockchainDatabase<B>,
    contract_id: FixedHash,
) -> Result<(), ValidationError> {
    let features = fetch_contract_features(db, contract_id, OutputType::ContractDefinition)?;
    if features.is_empty() {
        return Err(ValidationError::DanLayerError(
            DanLayerValidationError::ContractDefnintionNotFound { contract_id },
        ));
    }

    Ok(())
}

fn validate_uniqueness<B: BlockchainBackend>(
    db: &BlockchainDatabase<B>,
    contract_id: FixedHash,
) -> Result<(), ValidationError> {
    let features = fetch_contract_features(db, contract_id, OutputType::ContractConstitution)?;
    let is_duplicated = !features.is_empty();
    if is_duplicated {
        return Err(ValidationError::DanLayerError(
            DanLayerValidationError::DuplicatedUtxo {
                contract_id,
                output_type: OutputType::ContractConstitution,
                details: String::new(),
            },
        ));
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use tari_common_types::types::FixedHash;
    use tari_test_utils::unpack_enum;

    use crate::{
        transactions::transaction_components::OutputType,
        validation::dan_validators::{
            test_helpers::{
                assert_dan_validator_err,
                assert_dan_validator_success,
                create_contract_constitution,
                create_contract_constitution_schema,
                init_test_blockchain,
                publish_constitution,
                publish_definition,
                schema_to_transaction,
            },
            DanLayerValidationError,
        },
    };

    #[test]
    fn it_allows_valid_constitutions() {
        // initialise a blockchain with enough funds to spend at contract transactions
        let (mut blockchain, utxos) = init_test_blockchain();

        // publish the contract definition into a block
        let contract_id = publish_definition(&mut blockchain, utxos[0].clone());

        // construct a valid constitution transaction
        let constitution = create_contract_constitution();
        let schema = create_contract_constitution_schema(contract_id, utxos[2].clone(), constitution);
        let (tx, _) = schema_to_transaction(&schema);

        assert_dan_validator_success(&blockchain, &tx);
    }

    #[test]
    fn definition_must_exist() {
        // initialise a blockchain with enough funds to spend at contract transactions
        let (blockchain, utxos) = init_test_blockchain();

        // construct a transaction for a constitution, without a prior definition
        let contract_id = FixedHash::default();
        let constitution = create_contract_constitution();
        let schema = create_contract_constitution_schema(contract_id, utxos[2].clone(), constitution);
        let (tx, _) = schema_to_transaction(&schema);

        // try to validate the constitution transaction and check that we get the error
        let err = assert_dan_validator_err(&blockchain, &tx);
        assert!(matches!(
            err,
            DanLayerValidationError::ContractDefnintionNotFound { .. }
        ))
    }

    #[test]
    fn it_rejects_duplicated_constitutions() {
        // initialise a blockchain with enough funds to spend at contract transactions
        let (mut blockchain, utxos) = init_test_blockchain();

        // publish the contract definition into a block
        let expected_contract_id = publish_definition(&mut blockchain, utxos[0].clone());

        // publish the contract constitution into a block
        let constitution = create_contract_constitution();
        publish_constitution(
            &mut blockchain,
            utxos[1].clone(),
            expected_contract_id,
            constitution.clone(),
        );

        // construct a transaction for the duplicated contract constitution
        let schema = create_contract_constitution_schema(expected_contract_id, utxos[2].clone(), constitution);
        let (tx, _) = schema_to_transaction(&schema);

        // try to validate the duplicated constitution transaction and check that we get the error
        let err = assert_dan_validator_err(&blockchain, &tx);
        unpack_enum!(
            DanLayerValidationError::DuplicatedUtxo {
                output_type,
                contract_id,
                ..
            } = err
        );
        assert_eq!(output_type, OutputType::ContractConstitution);
        assert_eq!(contract_id, expected_contract_id);
    }
}
