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

use helpers::{
    block_builders::chain_block,
    pow_blockchain::{append_to_pow_blockchain, create_test_pow_blockchain},
};
use tari_core::{
    consensus::{ConsensusManagerBuilder, Network},
    helpers::create_mem_db,
    proof_of_work::PowAlgorithm,
};
use tari_crypto::tari_utilities::epoch_time::EpochTime;

#[test]
fn test_median_timestamp() {
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManagerBuilder::new(network).build();
    let store = create_mem_db(&consensus_manager);
    let pow_algos = vec![PowAlgorithm::Blake]; // GB default
    create_test_pow_blockchain(&store, pow_algos, &consensus_manager);
    let start_timestamp = store.fetch_block(0).unwrap().block().header.timestamp.clone();
    let mut timestamp = consensus_manager
        .get_median_timestamp(&*store.db_read_access().unwrap())
        .expect("median returned an error");
    assert_eq!(timestamp, start_timestamp);

    let pow_algos = vec![PowAlgorithm::Blake];
    // lets add 1
    let tip = store.fetch_block(store.get_height().unwrap().unwrap()).unwrap().block;
    append_to_pow_blockchain(&store, tip, pow_algos.clone(), &consensus_manager);
    let mut prev_timestamp: EpochTime =
        start_timestamp.increase(consensus_manager.consensus_constants().get_target_block_interval());
    timestamp = consensus_manager
        .get_median_timestamp(&*store.db_read_access().unwrap())
        .expect("median returned an error");
    assert_eq!(timestamp, prev_timestamp);
    // lets add 1
    let tip = store.fetch_block(store.get_height().unwrap().unwrap()).unwrap().block;
    append_to_pow_blockchain(&store, tip, pow_algos.clone(), &consensus_manager);
    prev_timestamp = start_timestamp.increase(consensus_manager.consensus_constants().get_target_block_interval());
    timestamp = consensus_manager
        .get_median_timestamp(&*store.db_read_access().unwrap())
        .expect("median returned an error");
    assert_eq!(timestamp, prev_timestamp);

    // lets build up 11 blocks
    for i in 4..12 {
        let tip = store.fetch_block(store.get_height().unwrap().unwrap()).unwrap().block;
        append_to_pow_blockchain(&store, tip, pow_algos.clone(), &consensus_manager);
        prev_timestamp =
            start_timestamp.increase(consensus_manager.consensus_constants().get_target_block_interval() * (i / 2));
        timestamp = consensus_manager
            .get_median_timestamp(&*store.db_read_access().unwrap())
            .expect("median returned an error");
        assert_eq!(timestamp, prev_timestamp);
    }

    // lets add many1 blocks
    for _i in 1..20 {
        let tip = store.fetch_block(store.get_height().unwrap().unwrap()).unwrap().block;
        append_to_pow_blockchain(&store, tip, pow_algos.clone(), &consensus_manager);
        prev_timestamp = prev_timestamp.increase(consensus_manager.consensus_constants().get_target_block_interval());
        timestamp = consensus_manager
            .get_median_timestamp(&*store.db_read_access().unwrap())
            .expect("median returned an error");
        assert_eq!(timestamp, prev_timestamp);
    }
}

#[test]
fn test_median_timestamp_with_height() {
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManagerBuilder::new(network).build();
    let store = create_mem_db(&consensus_manager);
    let pow_algos = vec![
        PowAlgorithm::Blake, // GB default
        PowAlgorithm::Monero,
        PowAlgorithm::Blake,
        PowAlgorithm::Monero,
        PowAlgorithm::Blake,
    ];
    create_test_pow_blockchain(&store, pow_algos, &consensus_manager);

    let header0_timestamp = store.fetch_header(0).unwrap().timestamp;
    let header1_timestamp = store.fetch_header(1).unwrap().timestamp;
    let header2_timestamp = store.fetch_header(2).unwrap().timestamp;

    let timestamp = consensus_manager
        .get_median_timestamp_at_height(&*store.db_read_access().unwrap(), 0)
        .expect("median returned an error");
    assert_eq!(timestamp, header0_timestamp);

    let timestamp = consensus_manager
        .get_median_timestamp_at_height(&*store.db_read_access().unwrap(), 3)
        .expect("median returned an error");
    assert_eq!(timestamp, header2_timestamp);

    let timestamp = consensus_manager
        .get_median_timestamp_at_height(&*store.db_read_access().unwrap(), 2)
        .expect("median returned an error");
    assert_eq!(timestamp, header1_timestamp);

    let timestamp = consensus_manager
        .get_median_timestamp_at_height(&*store.db_read_access().unwrap(), 4)
        .expect("median returned an error");
    assert_eq!(timestamp, header2_timestamp);
}

#[test]
fn test_median_timestamp_odd_order() {
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManagerBuilder::new(network).build();
    let store = create_mem_db(&consensus_manager);
    let pow_algos = vec![PowAlgorithm::Blake]; // GB default
    create_test_pow_blockchain(&store, pow_algos, &consensus_manager);
    let start_timestamp = store.fetch_block(0).unwrap().block().header.timestamp.clone();
    let mut timestamp = consensus_manager
        .get_median_timestamp(&*store.db_read_access().unwrap())
        .expect("median returned an error");
    assert_eq!(timestamp, start_timestamp);
    let pow_algos = vec![PowAlgorithm::Blake];
    // lets add 1
    let tip = store.fetch_block(store.get_height().unwrap().unwrap()).unwrap().block;
    append_to_pow_blockchain(&store, tip, pow_algos.clone(), &consensus_manager);
    let mut prev_timestamp: EpochTime =
        start_timestamp.increase(consensus_manager.consensus_constants().get_target_block_interval());
    timestamp = consensus_manager
        .get_median_timestamp(&*store.db_read_access().unwrap())
        .expect("median returned an error");
    assert_eq!(timestamp, prev_timestamp);

    // lets add 1 that's further back then
    let append_height = store.get_height().unwrap().unwrap();
    let prev_block = store.fetch_block(append_height).unwrap().block().clone();
    let new_block = chain_block(&prev_block, Vec::new(), &consensus_manager.consensus_constants());
    let mut new_block = store.calculate_mmr_roots(new_block).unwrap();
    new_block.header.timestamp =
        start_timestamp.increase(&consensus_manager.consensus_constants().get_target_block_interval() / 2);
    new_block.header.pow.pow_algo = PowAlgorithm::Blake;
    store.add_block(new_block).unwrap();

    prev_timestamp = start_timestamp.increase(consensus_manager.consensus_constants().get_target_block_interval() / 2);
    timestamp = consensus_manager
        .get_median_timestamp(&*store.db_read_access().unwrap())
        .expect("median returned an error");
    // Median timestamp should be block 3 and not block 2
    assert_eq!(timestamp, prev_timestamp);
}
