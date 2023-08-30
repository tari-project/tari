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

use std::{cmp, sync::Arc};

use tari_common::configuration::Network;
use tari_common_types::types::Commitment;
use tari_crypto::commitment::HomomorphicCommitment;
use tari_script::TariScript;
use tari_test_utils::unpack_enum;

use crate::{
    blocks::{BlockHeader, BlockHeaderAccumulatedData, ChainBlock, ChainHeader},
    chain_storage::{BlockchainBackend, BlockchainDatabase, ChainStorageError, DbTransaction},
    consensus::{ConsensusConstantsBuilder, ConsensusManager, ConsensusManagerBuilder},
    covenants::Covenant,
    proof_of_work::AchievedTargetDifficulty,
    test_helpers::{blockchain::create_store_with_consensus, create_chain_header},
    transactions::{
        key_manager::TxoStage,
        tari_amount::{uT, MicroMinotari},
        test_helpers::{
            create_random_signature_from_secret_key,
            create_test_core_key_manager_with_memory_db,
            create_utxo,
        },
        transaction_components::{KernelBuilder, KernelFeatures, OutputFeatures, TransactionKernel},
        CryptoFactories,
    },
    tx,
    validation::{ChainBalanceValidator, DifficultyCalculator, FinalHorizonStateValidation, ValidationError},
};

mod header_validators {
    use tari_utilities::epoch_time::EpochTime;

    use super::*;
    use crate::{
        block_specs,
        test_helpers::blockchain::{create_main_chain, create_new_blockchain},
        validation::{header::HeaderFullValidator, HeaderChainLinkedValidator},
    };

    #[test]
    fn header_iter_empty_and_invalid_height() {
        let consensus_manager = ConsensusManager::builder(Network::LocalNet).build().unwrap();
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
        let consensus_manager = ConsensusManagerBuilder::new(Network::LocalNet).build().unwrap();
        let db = create_store_with_consensus(consensus_manager.clone());
        let headers = (1..=15).fold(vec![db.fetch_chain_header(0).unwrap()], |mut acc, i| {
            let prev = acc.last().unwrap();
            let mut header = BlockHeader::new(0);
            header.height = i;
            header.prev_hash = *prev.hash();
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
        let consensus_manager = ConsensusManagerBuilder::new(Network::LocalNet).build().unwrap();
        let db = create_store_with_consensus(consensus_manager.clone());

        let genesis = db.fetch_chain_header(0).unwrap();

        let mut header = BlockHeader::from_previous(genesis.header());
        header.version = u16::MAX;
        let difficulty_calculator = DifficultyCalculator::new(consensus_manager.clone(), Default::default());
        let validator = HeaderFullValidator::new(consensus_manager, difficulty_calculator);

        let err = validator
            .validate(&*db.db_read_access().unwrap(), &header, genesis.header(), &[], None)
            .unwrap_err();
        assert!(matches!(err, ValidationError::InvalidBlockchainVersion {
            version: u16::MAX
        }));
    }

    #[tokio::test]
    async fn it_does_a_sanity_check_on_the_number_of_timestamps_provided() {
        let consensus_manager = ConsensusManagerBuilder::new(Network::LocalNet).build().unwrap();
        let db = create_new_blockchain();

        let (_, blocks) = create_main_chain(&db, block_specs!(["1->GB"], ["2->1"], ["3->2"])).await;
        let last_block = blocks.get("3").unwrap();

        let candidate_header = BlockHeader::from_previous(last_block.header());
        let difficulty_calculator = DifficultyCalculator::new(consensus_manager.clone(), Default::default());
        let validator = HeaderFullValidator::new(consensus_manager, difficulty_calculator);
        let mut timestamps = db.fetch_block_timestamps(*blocks.get("3").unwrap().hash()).unwrap();

        // First, lets check that everything else is valid
        validator
            .validate(
                &*db.db_read_access().unwrap(),
                &candidate_header,
                last_block.header(),
                &timestamps,
                None,
            )
            .unwrap();

        // Add an extra timestamp
        timestamps.push(EpochTime::now());
        let err = validator
            .validate(
                &*db.db_read_access().unwrap(),
                &candidate_header,
                last_block.header(),
                &timestamps,
                None,
            )
            .unwrap_err();
        assert!(matches!(err, ValidationError::IncorrectNumberOfTimestampsProvided {
            actual: 5,
            expected: 4
        }));
    }
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn chain_balance_validation() {
    let factories = CryptoFactories::default();
    let consensus_manager = ConsensusManagerBuilder::new(Network::Esmeralda).build().unwrap();
    let genesis = consensus_manager.get_genesis_block();
    let faucet_value = 5000 * uT;
    let key_manager = create_test_core_key_manager_with_memory_db();
    let (faucet_utxo, faucet_key_id, _) = create_utxo(
        faucet_value,
        &key_manager,
        &OutputFeatures::default(),
        &TariScript::default(),
        &Covenant::default(),
        MicroMinotari::zero(),
    )
    .await;
    let (pk, sig) = create_random_signature_from_secret_key(
        &key_manager,
        faucet_key_id,
        0.into(),
        0,
        KernelFeatures::empty(),
        TxoStage::Output,
    )
    .await;
    let excess = Commitment::from_public_key(&pk);
    let kernel =
        TransactionKernel::new_current_version(KernelFeatures::empty(), MicroMinotari::from(0), 0, excess, sig, None);
    let mut gen_block = genesis.block().clone();
    gen_block.body.add_output(faucet_utxo);
    gen_block.body.add_kernels([kernel]);
    let mut utxo_sum = HomomorphicCommitment::default();
    let mut kernel_sum = HomomorphicCommitment::default();
    let burned_sum = HomomorphicCommitment::default();
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
        .build()
        .unwrap();

    let db = create_store_with_consensus(consensus_manager.clone());

    let validator = ChainBalanceValidator::new(consensus_manager.clone(), factories.clone());
    // Validate the genesis state
    validator
        .validate(&*db.db_read_access().unwrap(), 0, &utxo_sum, &kernel_sum, &burned_sum)
        .unwrap();

    //---------------------------------- Add a new coinbase and header --------------------------------------------//
    let mut txn = DbTransaction::new();
    let coinbase_value = consensus_manager.get_block_reward_at(1);
    let (coinbase, coinbase_key_id, _) = create_utxo(
        coinbase_value,
        &key_manager,
        &OutputFeatures::create_coinbase(1, None),
        &TariScript::default(),
        &Covenant::default(),
        MicroMinotari::zero(),
    )
    .await;

    let (pk, sig) = create_random_signature_from_secret_key(
        &key_manager,
        coinbase_key_id,
        0.into(),
        0,
        KernelFeatures::create_coinbase(),
        TxoStage::Output,
    )
    .await;
    let excess = Commitment::from_public_key(&pk);
    let kernel = KernelBuilder::new()
        .with_signature(sig)
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

    txn.insert_kernel(kernel.clone(), *header1.hash(), mmr_position);
    txn.insert_utxo(coinbase.clone(), *header1.hash(), 1, mmr_leaf_index, 0);

    db.commit(txn).unwrap();
    utxo_sum = &coinbase.commitment + &utxo_sum;
    kernel_sum = &kernel.excess + &kernel_sum;
    validator
        .validate(&*db.db_read_access().unwrap(), 1, &utxo_sum, &kernel_sum, &burned_sum)
        .unwrap();

    //---------------------------------- Try to inflate --------------------------------------------//
    let mut txn = DbTransaction::new();

    let v = consensus_manager.get_block_reward_at(2) + uT;
    let (coinbase, spending_key_id, _) = create_utxo(
        v,
        &key_manager,
        &OutputFeatures::create_coinbase(1, None),
        &TariScript::default(),
        &Covenant::default(),
        MicroMinotari::zero(),
    )
    .await;
    let (pk, sig) = create_random_signature_from_secret_key(
        &key_manager,
        spending_key_id,
        0.into(),
        0,
        KernelFeatures::create_coinbase(),
        TxoStage::Output,
    )
    .await;
    let excess = Commitment::from_public_key(&pk);
    let kernel = KernelBuilder::new()
        .with_signature(sig)
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
    txn.insert_utxo(coinbase, *header2.hash(), 2, mmr_leaf_index, 0);
    mmr_position += 1;
    txn.insert_kernel(kernel, *header2.hash(), mmr_position);

    db.commit(txn).unwrap();

    validator
        .validate(&*db.db_read_access().unwrap(), 2, &utxo_sum, &kernel_sum, &burned_sum)
        .unwrap_err();
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn chain_balance_validation_burned() {
    let factories = CryptoFactories::default();
    let consensus_manager = ConsensusManagerBuilder::new(Network::Esmeralda).build().unwrap();
    let genesis = consensus_manager.get_genesis_block();
    let faucet_value = 5000 * uT;
    let key_manager = create_test_core_key_manager_with_memory_db();
    let (faucet_utxo, faucet_key_id, _) = create_utxo(
        faucet_value,
        &key_manager,
        &OutputFeatures::default(),
        &TariScript::default(),
        &Covenant::default(),
        MicroMinotari::zero(),
    )
    .await;
    let (pk, sig) = create_random_signature_from_secret_key(
        &key_manager,
        faucet_key_id,
        0.into(),
        0,
        KernelFeatures::empty(),
        TxoStage::Output,
    )
    .await;
    let excess = Commitment::from_public_key(&pk);
    let kernel =
        TransactionKernel::new_current_version(KernelFeatures::empty(), MicroMinotari::from(0), 0, excess, sig, None);
    let mut gen_block = genesis.block().clone();
    gen_block.body.add_output(faucet_utxo);
    gen_block.body.add_kernels([kernel]);
    let mut utxo_sum = HomomorphicCommitment::default();
    let mut kernel_sum = HomomorphicCommitment::default();
    let mut burned_sum = HomomorphicCommitment::default();
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
        .build()
        .unwrap();

    let db = create_store_with_consensus(consensus_manager.clone());

    let validator = ChainBalanceValidator::new(consensus_manager.clone(), factories.clone());
    // Validate the genesis state
    validator
        .validate(&*db.db_read_access().unwrap(), 0, &utxo_sum, &kernel_sum, &burned_sum)
        .unwrap();

    //---------------------------------- Add block (coinbase + burned) --------------------------------------------//
    let mut txn = DbTransaction::new();
    let coinbase_value = consensus_manager.get_block_reward_at(1) - MicroMinotari::from(100);
    let (coinbase, coinbase_key_id, _) = create_utxo(
        coinbase_value,
        &key_manager,
        &OutputFeatures::create_coinbase(1, None),
        &TariScript::default(),
        &Covenant::default(),
        MicroMinotari::zero(),
    )
    .await;
    let (pk, sig) = create_random_signature_from_secret_key(
        &key_manager,
        coinbase_key_id,
        0.into(),
        0,
        KernelFeatures::create_coinbase(),
        TxoStage::Output,
    )
    .await;
    let excess = Commitment::from_public_key(&pk);
    let kernel = KernelBuilder::new()
        .with_signature(sig)
        .with_excess(&excess)
        .with_features(KernelFeatures::COINBASE_KERNEL)
        .build()
        .unwrap();

    let (burned, burned_key_id, _) = create_utxo(
        100.into(),
        &key_manager,
        &OutputFeatures::create_burn_output(),
        &TariScript::default(),
        &Covenant::default(),
        MicroMinotari::zero(),
    )
    .await;

    let (pk2, sig2) = create_random_signature_from_secret_key(
        &key_manager,
        burned_key_id,
        0.into(),
        0,
        KernelFeatures::create_burn(),
        TxoStage::Output,
    )
    .await;
    let excess2 = Commitment::from_public_key(&pk2);
    let kernel2 = KernelBuilder::new()
        .with_signature(sig2)
        .with_excess(&excess2)
        .with_features(KernelFeatures::create_burn())
        .with_burn_commitment(Some(burned.commitment.clone()))
        .build()
        .unwrap();
    burned_sum = &burned_sum + kernel2.get_burn_commitment().unwrap();
    let mut header1 = BlockHeader::from_previous(genesis.header());
    header1.kernel_mmr_size += 2;
    header1.output_mmr_size += 2;
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

    txn.insert_kernel(kernel.clone(), *header1.hash(), mmr_position);
    txn.insert_utxo(coinbase.clone(), *header1.hash(), 1, mmr_leaf_index, 0);

    mmr_position = 5;
    mmr_leaf_index = 5;

    txn.insert_kernel(kernel2.clone(), *header1.hash(), mmr_position);
    txn.insert_pruned_utxo(burned.hash(), *header1.hash(), header1.height(), mmr_leaf_index, 0);

    db.commit(txn).unwrap();
    utxo_sum = &coinbase.commitment + &utxo_sum;
    kernel_sum = &(&kernel.excess + &kernel_sum) + &kernel2.excess;
    validator
        .validate(&*db.db_read_access().unwrap(), 1, &utxo_sum, &kernel_sum, &burned_sum)
        .unwrap();
}

mod transaction_validator {
    use super::*;
    use crate::{
        transactions::{
            test_helpers::create_test_core_key_manager_with_memory_db,
            transaction_components::TransactionError,
        },
        validation::transaction::TransactionInternalConsistencyValidator,
    };

    #[tokio::test]
    async fn it_rejects_coinbase_outputs() {
        let key_manager = create_test_core_key_manager_with_memory_db();
        let consensus_manager = ConsensusManagerBuilder::new(Network::LocalNet).build().unwrap();
        let db = create_store_with_consensus(consensus_manager.clone());
        let factories = CryptoFactories::default();
        let validator = TransactionInternalConsistencyValidator::new(true, consensus_manager, factories);
        let features = OutputFeatures::create_coinbase(0, None);
        let tx = match tx!(MicroMinotari(100_000), fee: MicroMinotari(5), inputs: 1, outputs: 1, features: features, &key_manager)
        {
            Ok((tx, _, _)) => tx,
            Err(e) => panic!("Error found: {}", e),
        };
        let tip = db.get_chain_metadata().unwrap();
        let err = validator.validate_with_current_tip(&tx, tip).unwrap_err();
        unpack_enum!(ValidationError::ErroneousCoinbaseOutput = err);
    }

    #[tokio::test]
    async fn coinbase_extra_must_be_empty() {
        let key_manager = create_test_core_key_manager_with_memory_db();
        let consensus_manager = ConsensusManagerBuilder::new(Network::LocalNet).build().unwrap();
        let db = create_store_with_consensus(consensus_manager.clone());
        let factories = CryptoFactories::default();
        let validator = TransactionInternalConsistencyValidator::new(true, consensus_manager, factories);
        let mut features = OutputFeatures { ..Default::default() };
        features.coinbase_extra = b"deadbeef".to_vec();
        let tx = match tx!(MicroMinotari(100_000), fee: MicroMinotari(5), inputs: 1, outputs: 1, features: features, &key_manager)
        {
            Ok((tx, _, _)) => tx,
            Err(e) => panic!("Error found: {}", e),
        };
        let tip = db.get_chain_metadata().unwrap();
        let err = validator.validate_with_current_tip(&tx, tip).unwrap_err();
        assert!(matches!(
            err,
            ValidationError::TransactionError(TransactionError::NonCoinbaseHasOutputFeaturesCoinbaseExtra)
        ));
    }
}

/// Iterator that emits BlockHeaders until a given height. This iterator loads headers in chunks of size `chunk_size`
/// for a low memory footprint. The chunk buffer is allocated once and reused.
pub struct HeaderIter<'a, B> {
    chunk: Vec<BlockHeader>,
    chunk_size: usize,
    cursor: usize,
    is_error: bool,
    height: u64,
    db: &'a BlockchainDatabase<B>,
}

impl<'a, B> HeaderIter<'a, B> {
    #[allow(dead_code)]
    pub fn new(db: &'a BlockchainDatabase<B>, height: u64, chunk_size: usize) -> Self {
        Self {
            db,
            chunk_size,
            cursor: 0,
            is_error: false,
            height,
            chunk: Vec::with_capacity(chunk_size),
        }
    }

    fn get_next_chunk(&self) -> (u64, u64) {
        #[allow(clippy::cast_possible_truncation)]
        let upper_bound = cmp::min(self.cursor + self.chunk_size, self.height as usize);
        (self.cursor as u64, upper_bound as u64)
    }
}

impl<B: BlockchainBackend> Iterator for HeaderIter<'_, B> {
    type Item = Result<BlockHeader, ChainStorageError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.is_error {
            return None;
        }

        if self.chunk.is_empty() {
            let (start, end) = self.get_next_chunk();
            // We're done: No more block headers to fetch
            if start > end {
                return None;
            }

            match self.db.fetch_headers(start..=end) {
                Ok(headers) => {
                    if headers.is_empty() {
                        return None;
                    }
                    self.cursor += headers.len();
                    self.chunk.extend(headers);
                },
                Err(err) => {
                    // On the next call, the iterator will end
                    self.is_error = true;
                    return Some(Err(err));
                },
            }
        }

        Some(Ok(self.chunk.remove(0)))
    }
}
