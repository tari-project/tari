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
// use crate::helpers::database::create_store;
// use crate::helpers::database::create_test_db;
use tari_core::{
    chain_storage::{fetch_headers, BlockchainBackend},
    consensus::{ConsensusManagerBuilder, Network},
    proof_of_work::{get_median_timestamp, PowAlgorithm},
    test_helpers::blockchain::create_store,
};
use tari_crypto::tari_utilities::epoch_time::EpochTime;

pub fn get_header_timestamps<B: BlockchainBackend>(db: &B, height: u64, timestamp_count: u64) -> Vec<EpochTime> {
    let min_height = height.checked_sub(timestamp_count).unwrap_or(0);
    fetch_headers(db, min_height, height)
        .unwrap()
        .iter()
        .map(|h| h.timestamp)
        .collect::<Vec<_>>()
}

#[test]
fn test_median_timestamp_with_height() {
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManagerBuilder::new(network).build();
    let store = create_store();
    let pow_algos: Vec<PowAlgorithm>;
    pow_algos = vec![
        PowAlgorithm::Blake, // GB default
        PowAlgorithm::Monero,
        PowAlgorithm::Blake,
        PowAlgorithm::Monero,
        PowAlgorithm::Blake,
    ];
    create_test_pow_blockchain(&store, pow_algos, &consensus_manager);
    let timestamp_count = 10;

    let header0_timestamp = store.fetch_header(0).unwrap().timestamp;
    let header1_timestamp = store.fetch_header(1).unwrap().timestamp;
    let header2_timestamp = store.fetch_header(2).unwrap().timestamp;

    let db = &*store.db_read_access().unwrap();
    let median_timestamp =
        get_median_timestamp(get_header_timestamps(db, 0, timestamp_count)).expect("median returned an error");
    assert_eq!(median_timestamp, header0_timestamp);

    let median_timestamp =
        get_median_timestamp(get_header_timestamps(db, 3, timestamp_count)).expect("median returned an error");
    assert_eq!(median_timestamp, (header1_timestamp + header2_timestamp) / 2);

    let median_timestamp =
        get_median_timestamp(get_header_timestamps(db, 2, timestamp_count)).expect("median returned an error");
    assert_eq!(median_timestamp, header1_timestamp);

    let median_timestamp =
        get_median_timestamp(get_header_timestamps(db, 4, timestamp_count)).expect("median returned an error");
    assert_eq!(median_timestamp, header2_timestamp);
}

#[test]
fn test_median_timestamp_odd_order() {
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManagerBuilder::new(network).build();
    let timestamp_count = consensus_manager.consensus_constants(0).get_median_timestamp_count() as u64;
    let store = create_store();
    let pow_algos = vec![PowAlgorithm::Blake]; // GB default
    create_test_pow_blockchain(&store, pow_algos, &consensus_manager);
    let mut timestamps = vec![store.fetch_block(0).unwrap().block().header.timestamp.clone()];
    let height = store.get_chain_metadata().unwrap().height_of_longest_chain();
    let mut median_timestamp = get_median_timestamp(get_header_timestamps(
        &*store.db_read_access().unwrap(),
        height,
        timestamp_count,
    ))
    .expect("median returned an error");
    assert_eq!(median_timestamp, timestamps[0]);
    let pow_algos = vec![PowAlgorithm::Blake];
    // lets add 1
    let tip = store.fetch_block(store.get_height().unwrap()).unwrap().block;
    append_to_pow_blockchain(&store, tip, pow_algos.clone(), &consensus_manager);
    timestamps.push(timestamps[0].increase(120));
    let height = store.get_chain_metadata().unwrap().height_of_longest_chain();
    median_timestamp = get_median_timestamp(get_header_timestamps(
        &*store.db_read_access().unwrap(),
        height,
        timestamp_count,
    ))
    .expect("median returned an error");
    assert_eq!(median_timestamp, (timestamps[0] + timestamps[1]) / 2);

    // lets add 1 that's further back then
    let append_height = store.get_height().unwrap().unwrap();
    let prev_block = store.fetch_block(append_height).unwrap().block().clone();
    let new_block = chain_block(&prev_block, Vec::new(), &consensus_manager);
    let mut new_block = store.calculate_mmr_roots(new_block).unwrap();
    timestamps.push(timestamps[0].increase(60));
    new_block.header.timestamp = timestamps[2];
    new_block.header.pow.pow_algo = PowAlgorithm::Blake;
    store.add_block(new_block.into()).unwrap();

    timestamps.push(timestamps[2].increase(60));
    let height = store.get_chain_metadata().unwrap().height_of_longest_chain();
    median_timestamp = get_median_timestamp(get_header_timestamps(
        &*store.db_read_access().unwrap(),
        height,
        timestamp_count,
    ))
    .expect("median returned an error");
    // Median timestamp should be block 3 and not block 2
    assert_eq!(median_timestamp, timestamps[2]);
}
