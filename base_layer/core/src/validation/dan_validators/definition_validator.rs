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

use super::helpers::{fetch_contract_features, get_sidechain_features, validate_output_type};
use crate::{
    chain_storage::{BlockchainBackend, BlockchainDatabase},
    transactions::transaction_components::{OutputType, TransactionOutput},
    validation::ValidationError,
};

pub fn validate_definition<B: BlockchainBackend>(
    db: &BlockchainDatabase<B>,
    output: &TransactionOutput,
) -> Result<(), ValidationError> {
    validate_output_type(output, OutputType::ContractDefinition)?;

    let sidechain_features = get_sidechain_features(output)?;
    let contract_id = sidechain_features.contract_id;

    validate_duplication(db, contract_id)?;

    Ok(())
}

fn validate_duplication<B: BlockchainBackend>(
    db: &BlockchainDatabase<B>,
    contract_id: FixedHash,
) -> Result<(), ValidationError> {
    match fetch_contract_features(db, contract_id, OutputType::ContractDefinition)? {
        Some(_) => {
            let msg = format!(
                "Duplicated contract definition for contract_id ({:?})",
                contract_id.to_hex()
            );
            Err(ValidationError::DanLayerError(msg))
        },
        None => Ok(()),
    }
}

#[cfg(test)]
mod test {
    use crate::validation::dan_validators::test_helpers::{
        assert_dan_error,
        create_contract_definition_schema,
        init_test_blockchain,
        publish_definition,
        schema_to_transaction,
    };

    #[test]
    fn it_rejects_duplicated_definitions() {
        // initialise a blockchain with enough funds to spend at contract transactions
        let (mut blockchain, change) = init_test_blockchain();

        // publish the contract definition into a block
        let _contract_id = publish_definition(&mut blockchain, change[0].clone());

        // construct a transaction for the duplicated contract definition
        let (_, schema) = create_contract_definition_schema(change[1].clone());
        let (tx, _) = schema_to_transaction(&schema);

        // try to validate the duplicated definition transaction and check that we get the error
        assert_dan_error(&blockchain, &tx, "Duplicated contract definition");
    }
}
