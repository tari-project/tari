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

use std::sync::Arc;

use tari_common_types::types::{FixedHash, PublicKey};

use crate::{
    chain_storage::Validators,
    test_helpers::blockchain::TestBlockchain,
    transactions::{
        tari_amount::{MicroTari, T},
        test_helpers::schema_to_transaction,
        transaction_components::{
            CheckpointParameters,
            CommitteeMembers,
            ConstitutionChangeFlags,
            ConstitutionChangeRules,
            ContractAcceptanceRequirements,
            ContractConstitution,
            ContractDefinition,
            ContractSpecification,
            OutputFeatures,
            OutputType,
            SideChainConsensus,
            SideChainFeatures,
            Transaction,
            UnblindedOutput,
        },
    },
    txn_schema,
    validation::mocks::MockValidator,
};

pub fn create_blockchain_without_validation() -> TestBlockchain {
    TestBlockchain::with_validators(Validators {
        block: Arc::new(MockValidator::new(true)),
        header: Arc::new(MockValidator::new(true)),
        orphan: Arc::new(MockValidator::new(true)),
    })
}

pub fn create_contract_definition_features(contract_id: FixedHash) -> OutputFeatures {
    OutputFeatures {
        output_type: OutputType::ContractDefinition,
        sidechain_features: Some(
            SideChainFeatures::builder(contract_id)
                .with_contract_definition(ContractDefinition {
                    contract_name: [1u8; 32],
                    contract_issuer: PublicKey::default(),
                    contract_spec: ContractSpecification {
                        runtime: [2u8; 32],
                        public_functions: vec![],
                    },
                })
                .finish(),
        ),
        ..Default::default()
    }
}

pub fn create_contract_definition_transaction(
    inputs: Vec<UnblindedOutput>,
    outputs: Vec<MicroTari>,
    contract_id: FixedHash,
) -> (Transaction, Vec<UnblindedOutput>) {
    let features = create_contract_definition_features(contract_id);
    let (transactions, outputs) =
        schema_to_transaction(&[txn_schema!(from: inputs, to: outputs, fee: 5.into(), lock: 0, features: features)]);
    ((*transactions[0]).clone(), outputs)
}

pub fn create_contract_constitution_transaction(
    inputs: Vec<UnblindedOutput>,
    contract_id: FixedHash,
) -> (Transaction, Vec<UnblindedOutput>) {
    let features = OutputFeatures {
        output_type: OutputType::ContractConstitution,
        sidechain_features: Some(
            SideChainFeatures::builder(contract_id)
                .with_contract_constitution(ContractConstitution {
                    validator_committee: CommitteeMembers::default(),
                    acceptance_requirements: ContractAcceptanceRequirements {
                        acceptance_period_expiry: 0,
                        minimum_quorum_required: 0,
                    },
                    consensus: SideChainConsensus::Bft,
                    checkpoint_params: CheckpointParameters {
                        minimum_quorum_required: 1,
                        abandoned_interval: 20,
                        quarantine_interval: 20,
                    },
                    constitution_change_rules: ConstitutionChangeRules {
                        change_flags: ConstitutionChangeFlags::empty(),
                        requirements_for_constitution_change: None,
                    },
                    initial_reward: 10u64.into(),
                })
                .finish(),
        ),
        ..Default::default()
    };
    let (transactions, outputs) =
        schema_to_transaction(&[txn_schema!(from: inputs, to: vec![T], fee: 5.into(), lock: 0, features: features)]);
    ((*transactions[0]).clone(), outputs)
}
