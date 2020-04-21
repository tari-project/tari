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

use helpers::pow_blockchain::{append_to_pow_blockchain, calculate_accumulated_difficulty, create_test_pow_blockchain};
use tari_core::{
    consensus::{ConsensusManagerBuilder, Network},
    helpers::create_mem_db,
    proof_of_work::PowAlgorithm,
};

#[test]
fn test_initial_sync() {
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManagerBuilder::new(network).build();
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
    ];
    create_test_pow_blockchain(&store, pow_algos.clone(), &consensus_manager.consensus_constants());

    assert_eq!(
        consensus_manager.get_target_difficulty(&*store.db_read_access().unwrap(), PowAlgorithm::Monero),
        Ok(calculate_accumulated_difficulty(
            &store,
            vec![2, 5, 6],
            &consensus_manager.consensus_constants()
        ))
    );
    assert_eq!(
        consensus_manager.get_target_difficulty(&*store.db_read_access().unwrap(), PowAlgorithm::Blake),
        Ok(calculate_accumulated_difficulty(
            &store,
            vec![0, 1, 3, 4, 7],
            &consensus_manager.consensus_constants()
        ))
    );
}

#[test]
fn test_sync_to_chain_tip() {
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManagerBuilder::new(network).build();
    let store = create_mem_db(&consensus_manager);

    let pow_algos = vec![
        PowAlgorithm::Blake, // Genesis block default
        PowAlgorithm::Monero,
        PowAlgorithm::Blake,
        PowAlgorithm::Blake,
        PowAlgorithm::Monero,
        PowAlgorithm::Blake,
    ];
    create_test_pow_blockchain(&store, pow_algos, &consensus_manager.consensus_constants());
    assert_eq!(store.get_height(), Ok(Some(5)));
    assert_eq!(
        consensus_manager.get_target_difficulty(&*store.db_read_access().unwrap(), PowAlgorithm::Monero),
        Ok(calculate_accumulated_difficulty(
            &store,
            vec![1, 4],
            &consensus_manager.consensus_constants()
        ))
    );
    assert_eq!(
        consensus_manager.get_target_difficulty(&*store.db_read_access().unwrap(), PowAlgorithm::Blake),
        Ok(calculate_accumulated_difficulty(
            &store,
            vec![0, 2, 3, 5],
            &consensus_manager.consensus_constants()
        ))
    );

    let pow_algos = vec![
        PowAlgorithm::Blake,
        PowAlgorithm::Monero,
        PowAlgorithm::Blake,
        PowAlgorithm::Monero,
    ];
    let tip = store.fetch_block(store.get_height().unwrap().unwrap()).unwrap().block;
    append_to_pow_blockchain(&store, tip, pow_algos, &consensus_manager.consensus_constants());
    assert_eq!(store.get_height(), Ok(Some(9)));
    assert_eq!(
        consensus_manager.get_target_difficulty(&*store.db_read_access().unwrap(), PowAlgorithm::Monero),
        Ok(calculate_accumulated_difficulty(
            &store,
            vec![1, 4, 7, 9],
            &consensus_manager.consensus_constants()
        ))
    );
    assert_eq!(
        consensus_manager.get_target_difficulty(&*store.db_read_access().unwrap(), PowAlgorithm::Blake),
        Ok(calculate_accumulated_difficulty(
            &store,
            vec![0, 2, 3, 5, 6, 8],
            &consensus_manager.consensus_constants()
        ))
    );
}

#[test]
fn test_target_difficulty_with_height() {
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManagerBuilder::new(network).build();
    let store = create_mem_db(&consensus_manager);
    assert!(consensus_manager
        .get_target_difficulty_with_height(&*store.db_read_access().unwrap(), PowAlgorithm::Monero, 5)
        .is_err());
    assert!(consensus_manager
        .get_target_difficulty_with_height(&*store.db_read_access().unwrap(), PowAlgorithm::Blake, 5)
        .is_err());

    let pow_algos = vec![
        PowAlgorithm::Blake, // GB default
        PowAlgorithm::Monero,
        PowAlgorithm::Blake,
        PowAlgorithm::Blake,
        PowAlgorithm::Monero,
        PowAlgorithm::Blake,
    ];
    create_test_pow_blockchain(&store, pow_algos, &consensus_manager.consensus_constants());

    assert_eq!(
        consensus_manager.get_target_difficulty_with_height(&*store.db_read_access().unwrap(), PowAlgorithm::Monero, 5),
        Ok(calculate_accumulated_difficulty(
            &store,
            vec![1, 4],
            &consensus_manager.consensus_constants()
        ))
    );
    assert_eq!(
        consensus_manager.get_target_difficulty_with_height(&*store.db_read_access().unwrap(), PowAlgorithm::Blake, 5),
        Ok(calculate_accumulated_difficulty(
            &store,
            vec![0, 2, 3, 5],
            &consensus_manager.consensus_constants()
        ))
    );

    assert_eq!(
        consensus_manager.get_target_difficulty_with_height(&*store.db_read_access().unwrap(), PowAlgorithm::Monero, 2),
        Ok(calculate_accumulated_difficulty(
            &store,
            vec![1],
            &consensus_manager.consensus_constants()
        ))
    );
    assert_eq!(
        consensus_manager.get_target_difficulty_with_height(&*store.db_read_access().unwrap(), PowAlgorithm::Blake, 2),
        Ok(calculate_accumulated_difficulty(
            &store,
            vec![0, 2],
            &consensus_manager.consensus_constants()
        ))
    );

    assert_eq!(
        consensus_manager.get_target_difficulty_with_height(&*store.db_read_access().unwrap(), PowAlgorithm::Monero, 3),
        Ok(calculate_accumulated_difficulty(
            &store,
            vec![1],
            &consensus_manager.consensus_constants()
        ))
    );
    assert_eq!(
        consensus_manager.get_target_difficulty_with_height(&*store.db_read_access().unwrap(), PowAlgorithm::Blake, 3),
        Ok(calculate_accumulated_difficulty(
            &store,
            vec![0, 2, 3],
            &consensus_manager.consensus_constants()
        ))
    );
}
