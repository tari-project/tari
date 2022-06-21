//  Copyright 2020, The Tari Project
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

use rand::Rng;
use tari_common::configuration::Network;
use tari_common_types::types::Commitment;
use tari_crypto::commitment::HomomorphicCommitment;
use tari_script::script;
use tari_test_utils::unpack_enum;
use tari_utilities::Hashable;

use crate::{
    blocks::{BlockHeader, BlockHeaderAccumulatedData, ChainBlock, ChainHeader},
    chain_storage::DbTransaction,
    consensus::{ConsensusConstantsBuilder, ConsensusManager, ConsensusManagerBuilder},
    covenants::Covenant,
    proof_of_work::AchievedTargetDifficulty,
    test_helpers::{blockchain::create_store_with_consensus, create_chain_header},
    transactions::{
        tari_amount::{uT, MicroTari},
        test_helpers::{create_random_signature_from_s_key, create_utxo},
        transaction_components::{KernelBuilder, KernelFeatures, OutputFeatures, TransactionKernel},
        CryptoFactories,
    },
    tx,
    validation::{
        header_iter::HeaderIter,
        header_validator::HeaderValidator,
        transaction_validators::TxInternalConsistencyValidator,
        ChainBalanceValidator,
        DifficultyCalculator,
        FinalHorizonStateValidation,
        HeaderValidation,
        MempoolTransactionValidation,
        ValidationError,
    },
};

mod header_validators {
    use super::*;

    #[test]
    fn header_iter_empty_and_invalid_height() {
        let consensus_manager = ConsensusManager::builder(Network::LocalNet).build();
        let genesis = consensus_manager.get_genesis_block();
        let db = create_store_with_consensus(consensus_manager);

        let iter = HeaderIter::new(&db, 0, 10);
        let headers = iter.map(Result::unwrap).collect::<Vec<_>>();
        assert_eq!(headers.len(), 1);

        assert_eq!(genesis.header(), &headers[0]);

        // Invalid header height
        let iter = HeaderIter::new(&db, 1, 10);
        let headers = iter.collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(headers.len(), 1);
    }

    #[test]
    fn header_iter_fetch_in_chunks() {
        let consensus_manager = ConsensusManagerBuilder::new(Network::LocalNet).build();
        let db = create_store_with_consensus(consensus_manager.clone());
        let headers = (1..=15).fold(vec![db.fetch_chain_header(0).unwrap()], |mut acc, i| {
            let prev = acc.last().unwrap();
            let mut header = BlockHeader::new(0);
            header.height = i;
            header.prev_hash = prev.hash().clone();
            // These have to be unique
            header.kernel_mmr_size = 2 + i;
            header.output_mmr_size = 4001 + i;

            let chain_header = create_chain_header(header, prev.accumulated_data());
            acc.push(chain_header);
            acc
        });
        db.insert_valid_headers(headers.into_iter().skip(1).collect()).unwrap();

        let iter = HeaderIter::new(&db, 11, 3);
        let headers = iter.map(Result::unwrap).collect::<Vec<_>>();
        assert_eq!(headers.len(), 12);
        let genesis = consensus_manager.get_genesis_block();
        assert_eq!(genesis.header(), &headers[0]);

        (1..=11).for_each(|i| {
            assert_eq!(headers[i].height, i as u64);
        })
    }

    #[test]
    fn it_validates_that_version_is_in_range() {
        let consensus_manager = ConsensusManagerBuilder::new(Network::LocalNet).build();
        let db = create_store_with_consensus(consensus_manager.clone());

        let genesis = db.fetch_chain_header(0).unwrap();

        let mut header = BlockHeader::from_previous(genesis.header());
        header.version = u16::MAX;

        let validator = HeaderValidator::new(consensus_manager.clone());

        let difficulty_calculator = DifficultyCalculator::new(consensus_manager, Default::default());
        let err = validator
            .validate(&*db.db_read_access().unwrap(), &header, &difficulty_calculator)
            .unwrap_err();
        assert!(matches!(err, ValidationError::InvalidBlockchainVersion {
            version: u16::MAX
        }));
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn chain_balance_validation() {
    let factories = CryptoFactories::default();
    let consensus_manager = ConsensusManagerBuilder::new(Network::Dibbler).build();
    let genesis = consensus_manager.get_genesis_block();
    let faucet_value = 5000 * uT;
    let (faucet_utxo, faucet_key, _) = create_utxo(
        faucet_value,
        &factories,
        &OutputFeatures::default(),
        &script!(Nop),
        &Covenant::default(),
    );
    let (pk, sig) = create_random_signature_from_s_key(faucet_key, 0.into(), 0);
    let excess = Commitment::from_public_key(&pk);
    let kernel = TransactionKernel::new_current_version(KernelFeatures::empty(), MicroTari::from(0), 0, excess, sig);
    // let _faucet_hash = faucet_utxo.hash();
    let mut gen_block = genesis.block().clone();
    gen_block.body.add_output(faucet_utxo);
    gen_block.body.add_kernels(&mut vec![kernel]);
    let mut utxo_sum = HomomorphicCommitment::default();
    let mut kernel_sum = HomomorphicCommitment::default();
    for output in gen_block.body.outputs() {
        utxo_sum = &output.commitment + &utxo_sum;
    }
    for kernel in gen_block.body.kernels() {
        kernel_sum = &kernel.excess + &kernel_sum;
    }
    let genesis = ChainBlock::try_construct(Arc::new(gen_block), genesis.accumulated_data().clone()).unwrap();
    let total_faucet = faucet_value + consensus_manager.consensus_constants(0).faucet_value();
    let constants = ConsensusConstantsBuilder::new(Network::LocalNet)
        .with_consensus_constants(consensus_manager.consensus_constants(0).clone())
        .with_faucet_value(total_faucet)
        .build();
    // Create a LocalNet consensus manager that uses rincewind consensus constants and has a custom rincewind genesis
    // block that contains an extra faucet utxo
    let consensus_manager = ConsensusManagerBuilder::new(Network::LocalNet)
        .with_block(genesis.clone())
        .add_consensus_constants(constants)
        .build();

    let db = create_store_with_consensus(consensus_manager.clone());

    let validator = ChainBalanceValidator::new(consensus_manager.clone(), factories.clone());
    // Validate the genesis state
    validator
        .validate(&*db.db_read_access().unwrap(), 0, &utxo_sum, &kernel_sum)
        .unwrap();

    //---------------------------------- Add a new coinbase and header --------------------------------------------//
    let mut txn = DbTransaction::new();
    let coinbase_value = consensus_manager.get_block_reward_at(1);
    let (coinbase, coinbase_key, _) = create_utxo(
        coinbase_value,
        &factories,
        &OutputFeatures::create_coinbase(1, rand::thread_rng().gen::<u8>()),
        &script!(Nop),
        &Covenant::default(),
    );
    // let _coinbase_hash = coinbase.hash();
    let (pk, sig) = create_random_signature_from_s_key(coinbase_key, 0.into(), 0);
    let excess = Commitment::from_public_key(&pk);
    let kernel = KernelBuilder::new()
        .with_signature(&sig)
        .with_excess(&excess)
        .with_features(KernelFeatures::COINBASE_KERNEL)
        .build()
        .unwrap();

    let mut header1 = BlockHeader::from_previous(genesis.header());
    header1.kernel_mmr_size += 1;
    header1.output_mmr_size += 1;
    let achieved_difficulty = AchievedTargetDifficulty::try_construct(
        genesis.header().pow_algo(),
        genesis.accumulated_data().target_difficulty,
        genesis.accumulated_data().achieved_difficulty,
    )
    .unwrap();
    let accumulated_data = BlockHeaderAccumulatedData::builder(genesis.accumulated_data())
        .with_hash(header1.hash())
        .with_achieved_target_difficulty(achieved_difficulty)
        .with_total_kernel_offset(header1.total_kernel_offset.clone())
        .build()
        .unwrap();
    let header1 = ChainHeader::try_construct(header1, accumulated_data).unwrap();
    txn.insert_chain_header(header1.clone());

    let mut mmr_position = 4;
    let mut mmr_leaf_index = 4;

    txn.insert_kernel(kernel.clone(), header1.hash().clone(), mmr_position);
    txn.insert_utxo(coinbase.clone(), header1.hash().clone(), 1, mmr_leaf_index);

    db.commit(txn).unwrap();
    utxo_sum = &coinbase.commitment + &utxo_sum;
    kernel_sum = &kernel.excess + &kernel_sum;
    validator
        .validate(&*db.db_read_access().unwrap(), 1, &utxo_sum, &kernel_sum)
        .unwrap();

    //---------------------------------- Try to inflate --------------------------------------------//
    let mut txn = DbTransaction::new();

    let v = consensus_manager.get_block_reward_at(2) + uT;
    let (coinbase, key, _) = create_utxo(
        v,
        &factories,
        &OutputFeatures::create_coinbase(1, rand::thread_rng().gen::<u8>()),
        &script!(Nop),
        &Covenant::default(),
    );
    let (pk, sig) = create_random_signature_from_s_key(key, 0.into(), 0);
    let excess = Commitment::from_public_key(&pk);
    let kernel = KernelBuilder::new()
        .with_signature(&sig)
        .with_excess(&excess)
        .with_features(KernelFeatures::COINBASE_KERNEL)
        .build()
        .unwrap();

    let mut header2 = BlockHeader::from_previous(header1.header());
    header2.kernel_mmr_size += 1;
    header2.output_mmr_size += 1;
    let achieved_difficulty = AchievedTargetDifficulty::try_construct(
        genesis.header().pow_algo(),
        genesis.accumulated_data().target_difficulty,
        genesis.accumulated_data().achieved_difficulty,
    )
    .unwrap();
    let accumulated_data = BlockHeaderAccumulatedData::builder(genesis.accumulated_data())
        .with_hash(header2.hash())
        .with_achieved_target_difficulty(achieved_difficulty)
        .with_total_kernel_offset(header2.total_kernel_offset.clone())
        .build()
        .unwrap();
    let header2 = ChainHeader::try_construct(header2, accumulated_data).unwrap();
    txn.insert_chain_header(header2.clone());
    utxo_sum = &coinbase.commitment + &utxo_sum;
    kernel_sum = &kernel.excess + &kernel_sum;
    mmr_leaf_index += 1;
    txn.insert_utxo(coinbase, header2.hash().clone(), 2, mmr_leaf_index);
    mmr_position += 1;
    txn.insert_kernel(kernel, header2.hash().clone(), mmr_position);

    db.commit(txn).unwrap();

    validator
        .validate(&*db.db_read_access().unwrap(), 2, &utxo_sum, &kernel_sum)
        .unwrap_err();
}

mod transaction_validator {
    use std::convert::TryInto;

    use tari_common_types::types::{FixedHash, PublicKey, Signature};
    use tari_utilities::hex::Hex;

    use super::*;
    use crate::{
        block_spec,
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
                RequirementsForConstitutionChange,
                SideChainConsensus,
                Transaction,
                UnblindedOutput,
            },
        },
        txn_schema,
        validation::transaction_validators::TxConsensusValidator,
    };

    #[test]
    fn it_rejects_coinbase_outputs() {
        let consensus_manager = ConsensusManagerBuilder::new(Network::LocalNet).build();
        let db = create_store_with_consensus(consensus_manager);
        let factories = CryptoFactories::default();
        let validator = TxInternalConsistencyValidator::new(factories, true, db);
        let features = OutputFeatures::create_coinbase(0, 0);
        let (tx, _, _) = tx!(MicroTari(100_000), fee: MicroTari(5), inputs: 1, outputs: 1, features: features);
        let err = validator.validate(&tx).unwrap_err();
        unpack_enum!(ValidationError::ErroneousCoinbaseOutput = err);
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
}
