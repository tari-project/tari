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

use tari_common::configuration::Network;
use tari_common_types::types::Commitment;
use tari_crypto::{commitment::HomomorphicCommitment, script};
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
    validation::{header_iter::HeaderIter, ChainBalanceValidator, FinalHorizonStateValidation},
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
}

#[test]
// TODO: Fix this test with the new DB structure
#[ignore = "to be fixed with new db structure"]
fn chain_balance_validation() {
    let factories = CryptoFactories::default();
    let consensus_manager = ConsensusManagerBuilder::new(Network::Weatherwax).build();
    let genesis = consensus_manager.get_genesis_block();
    let faucet_value = 5000 * uT;
    let (faucet_utxo, faucet_key, _) = create_utxo(
        faucet_value,
        &factories,
        OutputFeatures::default(),
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
        OutputFeatures::create_coinbase(1),
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
        OutputFeatures::create_coinbase(1),
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
