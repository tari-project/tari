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

use super::helpers::{get_sidechain_features, validate_output_type};
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
    let outputs = db
        .fetch_contract_outputs_by_contract_id_and_type(contract_id, OutputType::ContractDefinition)
        .map_err(|err| ValidationError::DanLayerError(format!("Could not search outputs: {}", err)))?;

    let is_duplicated = !outputs.is_empty();
    if is_duplicated {
        let msg = format!(
            "Duplicated contract definition for contract_id ({:?})",
            contract_id.to_hex()
        );
        return Err(ValidationError::DanLayerError(msg));
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use tari_p2p::Network;

    use crate::{
        block_spec,
        consensus::ConsensusManagerBuilder,
        test_helpers::blockchain::TestBlockchain,
        transactions::tari_amount::T,
        txn_schema,
        validation::{
            dan_validators::{
                test_helpers::{create_block, create_contract_definition_schema, schema_to_transaction},
                TxDanLayerValidator,
            },
            MempoolTransactionValidation,
            ValidationError,
        },
    };

    #[test]
    fn it_rejects_duplicated_definitions() {
        // initialize a brand new taest blockchain with a genesis block
        let consensus_manager = ConsensusManagerBuilder::new(Network::LocalNet).build();
        let mut blockchain = TestBlockchain::create(consensus_manager);
        let (_, coinbase_a) = blockchain.add_next_tip(block_spec!("1")).unwrap();

        // create a block with some UTXOs to spend later at contract transactions
        let schema = txn_schema!(from: vec![coinbase_a], to: vec![50 * T, 50 * T, 50 * T]);
        let change_outputs = create_block(&mut blockchain, "2", schema);

        // publish the contract definition into a block
        let (_, schema) = create_contract_definition_schema(change_outputs[0].clone());
        create_block(&mut blockchain, "3", schema);

        // construct a transaction for the duplicated contract definition
        let (_, schema) = create_contract_definition_schema(change_outputs[1].clone());
        let (txs, _) = schema_to_transaction(&[schema]);

        // try to validate the duplicated definition transaction and check that we get the error
        let validator = TxDanLayerValidator::new(blockchain.db().clone());
        let err = validator.validate(txs.first().unwrap()).unwrap_err();
        match err {
            ValidationError::DanLayerError(message) => {
                assert!(message.contains("Duplicated contract definition"))
            },
            _ => panic!("Expected a consensus error"),
        }
    }
}
