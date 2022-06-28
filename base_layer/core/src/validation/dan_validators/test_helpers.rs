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

use std::convert::TryInto;

use tari_common_types::types::{FixedHash, PublicKey, Signature};
use tari_crypto::commitment::HomomorphicCommitmentFactory;
use tari_p2p::Network;

use super::TxDanLayerValidator;
use crate::{
    block_spec,
    consensus::ConsensusManagerBuilder,
    test_helpers::blockchain::TestBlockchain,
    transactions::{
        tari_amount::T,
        test_helpers::{spend_utxos, TransactionSchema},
        transaction_components::{
            bytes_into_fixed_string,
            CheckpointParameters,
            CommitteeSignatures,
            ConstitutionChangeFlags,
            ConstitutionChangeRules,
            ContractAcceptanceRequirements,
            ContractAmendment,
            ContractCheckpoint,
            ContractConstitution,
            ContractDefinition,
            ContractSpecification,
            ContractUpdateProposal,
            OutputFeatures,
            RequirementsForConstitutionChange,
            SideChainConsensus,
            Transaction,
            UnblindedOutput,
        },
        CryptoFactories,
    },
    txn_schema,
    validation::{dan_validators::DanLayerValidationError, MempoolTransactionValidation, ValidationError},
};

pub fn init_test_blockchain() -> (TestBlockchain, Vec<UnblindedOutput>) {
    // initialize a brand new taest blockchain with a genesis block
    let consensus_manager = ConsensusManagerBuilder::new(Network::LocalNet).build();
    let mut blockchain = TestBlockchain::create(consensus_manager);
    let (_, coinbase_a) = blockchain.add_next_tip(block_spec!("genesis")).unwrap();

    // create a block with some UTXOs to spend later at contract transactions
    let schema = txn_schema!(from: vec![coinbase_a], to: vec![50 * T; 10]);
    let change_outputs = create_block(&mut blockchain, "change", schema);

    (blockchain, change_outputs)
}

pub fn publish_contract(
    blockchain: &mut TestBlockchain,
    inputs: &[UnblindedOutput],
    committee: Vec<PublicKey>,
) -> FixedHash {
    // publish the contract definition into a block
    let contract_id = publish_definition(blockchain, inputs[0].clone());

    // construct a transaction for the duplicated contract definition
    let mut constitution = create_contract_constitution();
    constitution.validator_committee = committee.try_into().unwrap();
    publish_constitution(blockchain, inputs[1].clone(), contract_id, constitution);
    contract_id
}

pub fn publish_definition(blockchain: &mut TestBlockchain, change: UnblindedOutput) -> FixedHash {
    let (contract_id, schema) = create_contract_definition_schema(change);
    create_block(blockchain, "definition", schema);

    contract_id
}

pub fn publish_constitution(
    blockchain: &mut TestBlockchain,
    change: UnblindedOutput,
    contract_id: FixedHash,
    constitution: ContractConstitution,
) {
    let schema = create_contract_constitution_schema(contract_id, change, constitution);
    create_block(blockchain, "constitution", schema);
}

pub fn publish_checkpoint(
    blockchain: &mut TestBlockchain,
    block_name: &'static str,
    input: UnblindedOutput,
    contract_id: FixedHash,
    checkpoint_number: u64,
) {
    let checkpoint = create_contract_checkpoint(checkpoint_number);
    let schema = create_contract_checkpoint_schema(contract_id, input, checkpoint);
    // TODO: need to change block spec to accept dynamic strings for name
    create_block(blockchain, block_name, schema);
}

pub fn publish_update_proposal(
    blockchain: &mut TestBlockchain,
    change: UnblindedOutput,
    contract_id: FixedHash,
    proposal_id: u64,
    updated_constitution: ContractConstitution,
) {
    let schema = create_contract_proposal_schema(contract_id, change, proposal_id, updated_constitution);
    create_block(blockchain, "proposal", schema);
}

pub fn schema_to_transaction(schema: &TransactionSchema) -> (Transaction, Vec<UnblindedOutput>) {
    let mut utxos = Vec::new();

    let (tx, mut output) = spend_utxos(schema.clone());
    utxos.append(&mut output);

    (tx, utxos)
}

pub fn create_block(
    blockchain: &mut TestBlockchain,
    block_name: &'static str,
    schema: TransactionSchema,
) -> Vec<UnblindedOutput> {
    let (tx, outputs) = schema_to_transaction(&schema);
    let (_, _) = blockchain
        .append_to_tip(block_spec!(block_name, transactions: vec![tx]))
        .unwrap();

    outputs
}

pub fn create_contract_definition_schema(input: UnblindedOutput) -> (FixedHash, TransactionSchema) {
    let definition = ContractDefinition {
        contract_name: bytes_into_fixed_string("name"),
        contract_issuer: PublicKey::default(),
        contract_spec: ContractSpecification {
            runtime: bytes_into_fixed_string("runtime"),
            public_functions: vec![],
        },
    };
    let commitment = CryptoFactories::default()
        .commitment
        .commit(&input.spending_key, &input.value.into());
    let contract_id = definition.calculate_contract_id(&commitment);
    let definition_features = OutputFeatures::for_contract_definition(&commitment, definition);

    let tx_schema =
        txn_schema!(from: vec![input], to: vec![0.into()], fee: 5.into(), lock: 0, features: definition_features);

    (contract_id, tx_schema)
}

pub fn create_contract_constitution_schema(
    contract_id: FixedHash,
    input: UnblindedOutput,
    constitution: ContractConstitution,
) -> TransactionSchema {
    let constitution_features = OutputFeatures::for_contract_constitution(contract_id, constitution);
    txn_schema!(from: vec![input], to: vec![0.into()], fee: 5.into(), lock: 0, features: constitution_features)
}

pub fn create_contract_constitution() -> ContractConstitution {
    ContractConstitution {
        validator_committee: vec![].try_into().unwrap(),
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
                constitution_committee: Some(vec![].try_into().unwrap()),
            }),
        },
        initial_reward: 100.into(),
    }
}

pub fn create_contract_checkpoint(checkpoint_number: u64) -> ContractCheckpoint {
    ContractCheckpoint {
        checkpoint_number,
        merkle_root: FixedHash::zero(),
        signatures: CommitteeSignatures::default(),
    }
}

pub fn create_contract_checkpoint_schema(
    contract_id: FixedHash,
    input: UnblindedOutput,
    checkpoint: ContractCheckpoint,
) -> TransactionSchema {
    let features = OutputFeatures::for_contract_checkpoint(contract_id, checkpoint);
    txn_schema!(from: vec![input], to: vec![0.into()], fee: 5.into(), lock: 0, features: features)
}

pub fn create_contract_acceptance_schema(
    contract_id: FixedHash,
    input: UnblindedOutput,
    validator_node_public_key: PublicKey,
) -> TransactionSchema {
    let signature = Signature::default();

    let acceptance_features =
        OutputFeatures::for_contract_acceptance(contract_id, validator_node_public_key, signature);

    txn_schema!(from: vec![input], to: vec![0.into()], fee: 5.into(), lock: 0, features: acceptance_features)
}

pub fn create_contract_proposal_schema(
    contract_id: FixedHash,
    input: UnblindedOutput,
    proposal_id: u64,
    updated_constitution: ContractConstitution,
) -> TransactionSchema {
    let proposal = ContractUpdateProposal {
        proposal_id,
        signature: Signature::default(),
        updated_constitution,
    };

    let proposal_features = OutputFeatures::for_contract_update_proposal(contract_id, proposal);

    txn_schema!(from: vec![input], to: vec![0.into()], fee: 5.into(), lock: 0, features: proposal_features)
}

pub fn create_contract_update_proposal_acceptance_schema(
    contract_id: FixedHash,
    input: UnblindedOutput,
    proposal_id: u64,
    validator_node_public_key: PublicKey,
) -> TransactionSchema {
    let signature = Signature::default();

    let acceptance_features = OutputFeatures::for_contract_update_proposal_acceptance(
        contract_id,
        proposal_id,
        validator_node_public_key,
        signature,
    );

    txn_schema!(from: vec![input], to: vec![0.into()], fee: 5.into(), lock: 0, features: acceptance_features)
}

pub fn create_contract_amendment_schema(
    contract_id: FixedHash,
    input: UnblindedOutput,
    proposal_id: u64,
    updated_constitution: ContractConstitution,
) -> TransactionSchema {
    let amendment = ContractAmendment {
        proposal_id,
        updated_constitution: updated_constitution.clone(),
        validator_committee: updated_constitution.validator_committee,
        validator_signatures: vec![Signature::default()].try_into().unwrap(),
        activation_window: 100,
    };

    let amendment_features = OutputFeatures::for_contract_amendment(contract_id, amendment);

    txn_schema!(from: vec![input], to: vec![0.into()], fee: 5.into(), lock: 0, features: amendment_features)
}

fn perform_validation(blockchain: &TestBlockchain, transaction: &Transaction) -> Result<(), DanLayerValidationError> {
    let validator = TxDanLayerValidator::new(blockchain.db().clone());
    match validator.validate(transaction) {
        Ok(()) => Ok(()),
        Err(ValidationError::DanLayerError(err)) => Err(err),
        _ => panic!("Expected a consensus error"),
    }
}

pub fn assert_dan_validator_err(blockchain: &TestBlockchain, transaction: &Transaction) -> DanLayerValidationError {
    perform_validation(blockchain, transaction).unwrap_err()
}

pub fn assert_dan_validator_fail(blockchain: &TestBlockchain, transaction: &Transaction, expected_message: &str) {
    let err = assert_dan_validator_err(blockchain, transaction);
    assert!(
        err.to_string().contains(expected_message),
        "Message \"{}\" does not contain \"{}\"",
        err,
        expected_message
    );
}

pub fn assert_dan_validator_success(blockchain: &TestBlockchain, transaction: &Transaction) {
    perform_validation(blockchain, transaction).unwrap()
}
