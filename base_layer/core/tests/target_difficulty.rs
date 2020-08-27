// Copyright 2019. The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

#[allow(dead_code)]
mod helpers;

use helpers::pow_blockchain::{calculate_accumulated_difficulty, create_test_pow_blockchain};
use tari_core::{
    consensus::{ConsensusConstantsBuilder, ConsensusManagerBuilder, Network},
    helpers::create_mem_db,
    proof_of_work::{get_target_difficulty, PowAlgorithm},
};

#[test]
fn test_target_difficulty_at_tip() {
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManagerBuilder::new(network).build();
    let constants = consensus_manager.consensus_constants();
    let block_window = constants.get_difficulty_block_window() as usize;
    let target_time = constants.get_diff_target_block_interval(PowAlgorithm::Blake);
    let max_block_time = constants.get_difficulty_max_block_interval(PowAlgorithm::Blake);
    let store = create_mem_db(&consensus_manager);

    let pow_algos = vec![
        PowAlgorithm::Blake, //  GB default
        PowAlgorithm::Blake,
        PowAlgorithm::Monero,
        PowAlgorithm::Blake,
        PowAlgorithm::Blake,
        PowAlgorithm::Monero,
        PowAlgorithm::Monero,
        PowAlgorithm::Blake,
        PowAlgorithm::Monero,
        PowAlgorithm::Blake,
    ];
    create_test_pow_blockchain(&store, pow_algos.clone(), &consensus_manager);
    let height = store.get_chain_metadata().unwrap().height_of_longest_chain.unwrap();

    let pow_algo = PowAlgorithm::Monero;
    let target_difficulties = store.fetch_target_difficulties(pow_algo, height, block_window).unwrap();
    assert_eq!(
        get_target_difficulty(
            target_difficulties,
            block_window,
            target_time,
            constants.min_pow_difficulty(pow_algo),
            max_block_time
        )
        .unwrap(),
        calculate_accumulated_difficulty(&store, pow_algo, vec![2, 5, 6, 8], &constants)
    );

    let pow_algo = PowAlgorithm::Blake;
    let target_difficulties = store.fetch_target_difficulties(pow_algo, height, block_window).unwrap();
    assert_eq!(
        get_target_difficulty(
            target_difficulties,
            block_window,
            target_time,
            constants.min_pow_difficulty(pow_algo),
            max_block_time
        )
        .unwrap(),
        calculate_accumulated_difficulty(&store, pow_algo, vec![0, 1, 3, 4, 7, 9], &constants)
    );
}

#[test]
fn test_target_difficulty_with_height() {
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManagerBuilder::new(network).build();
    let constants = consensus_manager.consensus_constants();
    let block_window = constants.get_difficulty_block_window() as usize;
    let target_time = constants.get_diff_target_block_interval(PowAlgorithm::Blake);
    let max_block_time = constants.get_difficulty_max_block_interval(PowAlgorithm::Blake);
    let store = create_mem_db(&consensus_manager);

    let pow_algos = vec![
        PowAlgorithm::Blake, // GB default
        PowAlgorithm::Monero,
        PowAlgorithm::Blake,
        PowAlgorithm::Blake,
        PowAlgorithm::Monero,
        PowAlgorithm::Blake,
    ];
    create_test_pow_blockchain(&store, pow_algos, &consensus_manager);

    let pow_algo = PowAlgorithm::Monero;
    assert_eq!(
        get_target_difficulty(
            store.fetch_target_difficulties(pow_algo, 5, block_window).unwrap(),
            block_window,
            target_time,
            constants.min_pow_difficulty(pow_algo),
            max_block_time
        )
        .unwrap(),
        calculate_accumulated_difficulty(&store, pow_algo, vec![1, 4], &constants)
    );

    let pow_algo = PowAlgorithm::Blake;
    assert_eq!(
        get_target_difficulty(
            store.fetch_target_difficulties(pow_algo, 5, block_window).unwrap(),
            block_window,
            target_time,
            constants.min_pow_difficulty(pow_algo),
            max_block_time
        )
        .unwrap(),
        calculate_accumulated_difficulty(&store, pow_algo, vec![0, 2, 3, 5], &constants)
    );

    let pow_algo = PowAlgorithm::Monero;
    assert_eq!(
        get_target_difficulty(
            store.fetch_target_difficulties(pow_algo, 2, block_window).unwrap(),
            block_window,
            target_time,
            constants.min_pow_difficulty(pow_algo),
            max_block_time
        )
        .unwrap(),
        calculate_accumulated_difficulty(&store, pow_algo, vec![1], &constants)
    );

    let pow_algo = PowAlgorithm::Blake;
    assert_eq!(
        get_target_difficulty(
            store.fetch_target_difficulties(pow_algo, 2, block_window).unwrap(),
            block_window,
            target_time,
            constants.min_pow_difficulty(pow_algo),
            max_block_time
        )
        .unwrap(),
        calculate_accumulated_difficulty(&store, pow_algo, vec![0, 2], &constants)
    );
    let pow_algo = PowAlgorithm::Monero;
    assert_eq!(
        get_target_difficulty(
            store.fetch_target_difficulties(pow_algo, 3, block_window).unwrap(),
            block_window,
            target_time,
            constants.min_pow_difficulty(pow_algo),
            max_block_time
        )
        .unwrap(),
        calculate_accumulated_difficulty(&store, pow_algo, vec![1], &constants)
    );

    let pow_algo = PowAlgorithm::Blake;
    assert_eq!(
        get_target_difficulty(
            store.fetch_target_difficulties(pow_algo, 3, block_window).unwrap(),
            block_window,
            target_time,
            constants.min_pow_difficulty(pow_algo),
            max_block_time
        )
        .unwrap(),
        calculate_accumulated_difficulty(&store, pow_algo, vec![0, 2, 3], &constants)
    );
}

#[test]
fn test_target_block_interval() {
    let target_block_interval = 120;
    let difficulty_block_window = 90;
    let constants_2_equal = ConsensusConstantsBuilder::new(Network::LocalNet)
        .with_algo_split([50, 50, 0])
        .build();

    let constants_2_split = ConsensusConstantsBuilder::new(Network::LocalNet)
        .with_algo_split([60, 40, 0])
        .build();

    let constants_1 = ConsensusConstantsBuilder::new(Network::LocalNet)
        .with_algo_split([100, 0, 0])
        .build();

    let constants_3_equal = ConsensusConstantsBuilder::new(Network::LocalNet)
        .with_algo_split([33, 33, 33])
        .build();

    assert_eq!(
        constants_3_equal.get_diff_target_block_interval(PowAlgorithm::Blake),
        363
    );
    assert_eq!(constants_1.get_diff_target_block_interval(PowAlgorithm::Blake), 0);
    assert_eq!(constants_1.get_diff_target_block_interval(PowAlgorithm::Monero), 120);
    assert_eq!(
        constants_2_equal.get_diff_target_block_interval(PowAlgorithm::Blake),
        240
    );
    assert_eq!(
        constants_2_split.get_diff_target_block_interval(PowAlgorithm::Blake),
        300
    );
    assert_eq!(
        constants_2_split.get_diff_target_block_interval(PowAlgorithm::Monero),
        200
    );
}
