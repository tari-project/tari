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

use crate::{
    chain_storage::{BlockchainBackend, BlockchainDatabase},
    transactions::transaction_components::{OutputType, Transaction},
    validation::ValidationError,
};

pub fn validate_contract_acceptances<B: BlockchainBackend>(
    db: &BlockchainDatabase<B>,
    tx: &Transaction,
) -> Result<(), ValidationError> {
    for output in tx.body().outputs() {
        // we only want to validate contract acceptances
        if output.features.output_type != OutputType::ContractValidatorAcceptance {
            continue;
        }
        // !output.features.is_sidechain_contract()
        let sidechain_features = output.features.sidechain_features.as_ref().unwrap();
        let contract_id = sidechain_features.contract_id;
        let validator_node_publick_key = &sidechain_features
            .acceptance
            .as_ref()
            .unwrap()
            .validator_node_public_key;

        let contract_outputs = db
            .fetch_contract_outputs_by_contract_id_and_type(contract_id, OutputType::ContractConstitution)
            .unwrap();
        if contract_outputs.is_empty() {
            continue;
        }
        let constitution_output = contract_outputs
            .first()
            .unwrap()
            .output
            .as_transaction_output()
            .unwrap();
        let constitution = constitution_output
            .features
            .sidechain_features
            .as_ref()
            .unwrap()
            .constitution
            .as_ref()
            .unwrap();

        let is_validator_in_committee = constitution
            .validator_committee
            .members()
            .contains(validator_node_publick_key);
        if !is_validator_in_committee {
            let msg = format!(
                "Invalid contract acceptance: validator node public key is not in committee ({:?})",
                validator_node_publick_key
            );
            return Err(ValidationError::ConsensusError(msg));
        }

        // TODO: check that the signature of the transaction is valid
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use std::{convert::TryInto, sync::Arc};

    use tari_common_types::types::{FixedHash, PublicKey, Signature};
    use tari_p2p::Network;
    use tari_utilities::hex::Hex;

    use crate::{
        block_spec,
        consensus::ConsensusManagerBuilder,
        test_helpers::blockchain::TestBlockchain,
        transactions::{
            tari_amount::T,
            test_helpers::{spend_utxos, TransactionSchema},
            transaction_components::{
                vec_into_fixed_string,
                CheckpointParameters,
                CommitteeMembers,
                ConstitutionChangeFlags,
                ConstitutionChangeRules,
                ContractAcceptanceRequirements,
                ContractConstitution,
                ContractDefinition,
                ContractSpecification,
                OutputFeatures,
                RequirementsForConstitutionChange,
                SideChainConsensus,
                Transaction,
                UnblindedOutput,
            },
        },
        txn_schema,
        validation::{transaction_validators::TxConsensusValidator, MempoolTransactionValidation, ValidationError},
    };

    #[test]
    fn it_rejects_contract_acceptances_of_non_committee_member() {
        let consensus_manager = ConsensusManagerBuilder::new(Network::LocalNet).build();
        let mut blockchain = TestBlockchain::create(consensus_manager);
        let (_, coinbase_a) = blockchain.add_next_tip(block_spec!("1")).unwrap();

        let schema = txn_schema!(from: vec![coinbase_a], to: vec![50 * T, 50 * T, 50 * T]);
        let change_outputs = create_block(&mut blockchain, "2", schema);

        let (contract_id, schema) = create_contract_definition_schema(change_outputs[0].clone());
        // let schema = txn_schema!(from: vec![change_outputs[0].clone()], to: vec![10 * T]);
        create_block(&mut blockchain, "3", schema);

        // let schema = txn_schema!(from: vec![change_outputs[1].clone()], to: vec![10 * T]);
        let schema = create_contract_constitution_schema(contract_id, change_outputs[1].clone());
        create_block(&mut blockchain, "4", schema);

        let schema = create_contract_acceptance_schema(contract_id, change_outputs[2].clone());
        let (txs, _) = schema_to_transaction(&[schema]);

        let validator = TxConsensusValidator::new(blockchain.db().clone());
        let err = validator.validate(txs.first().unwrap()).unwrap_err();

        match err {
            ValidationError::ConsensusError(message) => {
                assert!(message.contains("Invalid contract acceptance: validator node public key is not in committee"))
            },
            _ => panic!("Expected a consensus error"),
        }
    }

    pub fn schema_to_transaction(txns: &[TransactionSchema]) -> (Vec<Arc<Transaction>>, Vec<UnblindedOutput>) {
        let mut tx = Vec::new();
        let mut utxos = Vec::new();
        txns.iter().for_each(|schema| {
            let (txn, mut output) = spend_utxos(schema.clone());
            tx.push(Arc::new(txn));
            utxos.append(&mut output);
        });
        (tx, utxos)
    }

    pub fn create_block(
        blockchain: &mut TestBlockchain,
        block_name: &'static str,
        schema: TransactionSchema,
    ) -> Vec<UnblindedOutput> {
        let (txs, outputs) = schema_to_transaction(&[schema]);
        let (_, _) = blockchain
            .append_to_tip(block_spec!(block_name, transactions: txs.iter().map(|t| (**t).clone()).collect()))
            .unwrap();

        outputs
    }

    fn create_contract_definition_schema(input: UnblindedOutput) -> (FixedHash, TransactionSchema) {
        let definition = ContractDefinition {
            contract_name: vec_into_fixed_string("name".as_bytes().to_vec()),
            contract_issuer: PublicKey::default(),
            contract_spec: ContractSpecification {
                runtime: vec_into_fixed_string("runtime".as_bytes().to_vec()),
                public_functions: vec![],
            },
        };
        let contract_id = definition.calculate_contract_id();
        let definition_features = OutputFeatures::for_contract_definition(definition);

        let tx_schema =
            txn_schema!(from: vec![input], to: vec![0.into()], fee: 5.into(), lock: 0, features: definition_features);

        (contract_id, tx_schema)
    }

    fn create_contract_constitution_schema(contract_id: FixedHash, input: UnblindedOutput) -> TransactionSchema {
        let validator_committee: CommitteeMembers = vec![PublicKey::default()].try_into().unwrap();
        let constitution = ContractConstitution {
            validator_committee,
            acceptance_requirements: ContractAcceptanceRequirements {
                acceptance_period_expiry: 100,
                minimum_quorum_required: 5,
            },
            consensus: SideChainConsensus::MerkleRoot,
            checkpoint_params: CheckpointParameters {
                minimum_quorum_required: 5,
                abandoned_interval: 100,
            },
            constitution_change_rules: ConstitutionChangeRules {
                change_flags: ConstitutionChangeFlags::all(),
                requirements_for_constitution_change: Some(RequirementsForConstitutionChange {
                    minimum_constitution_committee_signatures: 5,
                    constitution_committee: Some(
                        vec![PublicKey::default(); CommitteeMembers::MAX_MEMBERS]
                            .try_into()
                            .unwrap(),
                    ),
                }),
            },
            initial_reward: 100.into(),
        };
        let constitution_features = OutputFeatures::for_contract_constitution(contract_id, constitution);

        txn_schema!(from: vec![input], to: vec![0.into()], fee: 5.into(), lock: 0, features: constitution_features)
    }

    fn create_contract_acceptance_schema(contract_id: FixedHash, input: UnblindedOutput) -> TransactionSchema {
        // let validator_node_public_key = PublicKey::default();
        let validator_node_public_key =
            PublicKey::from_hex("70350e09c474809209824c6e6888707b7dd09959aa227343b5106382b856f73a").unwrap();
        let signature = Signature::default();

        let acceptance_features =
            OutputFeatures::for_contract_acceptance(contract_id, validator_node_public_key, signature);

        let mut tx =
            txn_schema!(from: vec![input], to: vec![0.into()], fee: 5.into(), lock: 0, features: acceptance_features);
        tx.output_version = None;

        tx
    }
}
