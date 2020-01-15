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

use helpers::block_builders::{chain_block, create_genesis_block};
use tari_core::{
    blocks::Block,
    chain_storage::{BlockchainDatabase, MemoryDatabase},
    consensus::ConsensusConstants,
    helpers::create_mem_db,
    proof_of_work::{
        lwma_diff::LinearWeightedMovingAverage,
        DiffAdjManager,
        Difficulty,
        DifficultyAdjustment,
        PowAlgorithm,
    },
};
use tari_transactions::types::{CryptoFactories, HashDigest};
use tari_utilities::epoch_time::EpochTime;

fn create_test_pow_blockchain(db: &BlockchainDatabase<MemoryDatabase<HashDigest>>, mut pow_algos: Vec<PowAlgorithm>) {
    let factories = CryptoFactories::default();
    let (mut block0, _) = create_genesis_block(&db, &factories);
    block0.header.pow.pow_algo = pow_algos.remove(0);
    block0.header.timestamp = EpochTime::from(1575018842);
    db.add_block(block0.clone()).unwrap();
    append_to_pow_blockchain(db, block0, pow_algos);
}

fn append_to_pow_blockchain(
    db: &BlockchainDatabase<MemoryDatabase<HashDigest>>,
    chain_tip: Block,
    pow_algos: Vec<PowAlgorithm>,
)
{
    let mut prev_block = chain_tip;
    let consensus = ConsensusConstants::current();
    for pow_algo in pow_algos {
        let new_block = chain_block(&prev_block, Vec::new());
        let mut new_block = db.calculate_mmr_roots(new_block).unwrap();
        new_block.header.timestamp = prev_block
            .header
            .timestamp
            .increase(consensus.get_target_block_interval());
        new_block.header.pow.pow_algo = pow_algo;
        db.add_block(new_block.clone()).unwrap();
        prev_block = new_block;
    }
}

// Calculated the accumulated difficulty for the selected blocks in the blockchain db.
fn calculate_accumulated_difficulty(
    db: &BlockchainDatabase<MemoryDatabase<HashDigest>>,
    heights: Vec<u64>,
) -> Difficulty
{
    let mut lwma = LinearWeightedMovingAverage::default();
    for height in heights {
        let header = db.fetch_header(height).unwrap();
        let accumulated_difficulty = header.achieved_difficulty() +
            match header.pow.pow_algo {
                PowAlgorithm::Monero => header.pow.accumulated_monero_difficulty,
                PowAlgorithm::Blake => header.pow.accumulated_blake_difficulty,
            };
        lwma.add(header.timestamp, accumulated_difficulty).unwrap();
    }
    lwma.get_difficulty()
}

#[test]
fn test_initial_sync() {
    let store = create_mem_db();
    let diff_adj_manager = DiffAdjManager::new(store.clone()).unwrap();
    assert!(diff_adj_manager.get_target_difficulty(&PowAlgorithm::Monero).is_err());
    assert!(diff_adj_manager.get_target_difficulty(&PowAlgorithm::Blake).is_err());

    let pow_algos = vec![
        PowAlgorithm::Blake, // GB default
        PowAlgorithm::Blake,
        PowAlgorithm::Monero,
        PowAlgorithm::Blake,
        PowAlgorithm::Blake,
        PowAlgorithm::Monero,
        PowAlgorithm::Monero,
        PowAlgorithm::Blake,
    ];

    create_test_pow_blockchain(&store, pow_algos.clone());
    let diff_adj_manager = DiffAdjManager::new(store.clone()).unwrap();

    assert_eq!(
        diff_adj_manager.get_target_difficulty(&PowAlgorithm::Monero),
        Ok(calculate_accumulated_difficulty(&store, vec![2, 5, 6]))
    );
    assert_eq!(
        diff_adj_manager.get_target_difficulty(&PowAlgorithm::Blake),
        Ok(calculate_accumulated_difficulty(&store, vec![0, 1, 3, 4, 7]))
    );
}

#[test]
fn test_sync_to_chain_tip() {
    let store = create_mem_db();
    let diff_adj_manager = DiffAdjManager::new(store.clone()).unwrap();

    let pow_algos = vec![
        PowAlgorithm::Blake, // GB default
        PowAlgorithm::Monero,
        PowAlgorithm::Blake,
        PowAlgorithm::Blake,
        PowAlgorithm::Monero,
        PowAlgorithm::Blake,
    ];
    create_test_pow_blockchain(&store, pow_algos);
    assert_eq!(store.get_height(), Ok(Some(5)));
    assert_eq!(
        diff_adj_manager.get_target_difficulty(&PowAlgorithm::Monero),
        Ok(calculate_accumulated_difficulty(&store, vec![1, 4]))
    );
    assert_eq!(
        diff_adj_manager.get_target_difficulty(&PowAlgorithm::Blake),
        Ok(calculate_accumulated_difficulty(&store, vec![0, 2, 3, 5]))
    );

    let pow_algos = vec![
        PowAlgorithm::Blake,
        PowAlgorithm::Monero,
        PowAlgorithm::Blake,
        PowAlgorithm::Monero,
    ];
    let tip = store.fetch_block(store.get_height().unwrap().unwrap()).unwrap().block;
    append_to_pow_blockchain(&store, tip, pow_algos);
    assert_eq!(store.get_height(), Ok(Some(9)));
    assert_eq!(
        diff_adj_manager.get_target_difficulty(&PowAlgorithm::Monero),
        Ok(calculate_accumulated_difficulty(&store, vec![1, 4, 7, 9]))
    );
    assert_eq!(
        diff_adj_manager.get_target_difficulty(&PowAlgorithm::Blake),
        Ok(calculate_accumulated_difficulty(&store, vec![0, 2, 3, 5, 6, 8]))
    );
}

#[test]
fn test_target_difficulty_with_height() {
    let store = create_mem_db();
    let diff_adj_manager = DiffAdjManager::new(store.clone()).unwrap();
    assert!(diff_adj_manager
        .get_target_difficulty_at_height(&PowAlgorithm::Monero, 5)
        .is_err());
    assert!(diff_adj_manager
        .get_target_difficulty_at_height(&PowAlgorithm::Blake, 5)
        .is_err());

    let pow_algos = vec![
        PowAlgorithm::Blake, // GB default
        PowAlgorithm::Monero,
        PowAlgorithm::Blake,
        PowAlgorithm::Blake,
        PowAlgorithm::Monero,
        PowAlgorithm::Blake,
    ];
    create_test_pow_blockchain(&store, pow_algos);
    let diff_adj_manager = DiffAdjManager::new(store.clone()).unwrap();

    assert_eq!(
        diff_adj_manager.get_target_difficulty_at_height(&PowAlgorithm::Monero, 5),
        Ok(calculate_accumulated_difficulty(&store, vec![1, 4]))
    );
    assert_eq!(
        diff_adj_manager.get_target_difficulty_at_height(&PowAlgorithm::Blake, 5),
        Ok(calculate_accumulated_difficulty(&store, vec![0, 2, 3, 5]))
    );

    assert_eq!(
        diff_adj_manager.get_target_difficulty_at_height(&PowAlgorithm::Monero, 2),
        Ok(calculate_accumulated_difficulty(&store, vec![1]))
    );
    assert_eq!(
        diff_adj_manager.get_target_difficulty_at_height(&PowAlgorithm::Blake, 2),
        Ok(calculate_accumulated_difficulty(&store, vec![0, 2]))
    );

    assert_eq!(
        diff_adj_manager.get_target_difficulty_at_height(&PowAlgorithm::Monero, 3),
        Ok(calculate_accumulated_difficulty(&store, vec![1]))
    );
    assert_eq!(
        diff_adj_manager.get_target_difficulty_at_height(&PowAlgorithm::Blake, 3),
        Ok(calculate_accumulated_difficulty(&store, vec![0, 2, 3]))
    );
}

#[test]
#[ignore] // TODO Wait for reorg logic to be refactored
fn test_full_sync_on_reorg() {
    let store = create_mem_db();
    let diff_adj_manager = DiffAdjManager::new(store.clone()).unwrap();

    let pow_algos = vec![
        PowAlgorithm::Blake, // GB default
        PowAlgorithm::Blake,
        PowAlgorithm::Blake,
        PowAlgorithm::Blake,
        PowAlgorithm::Monero,
    ];
    create_test_pow_blockchain(&store, pow_algos);
    assert_eq!(store.get_height(), Ok(Some(4)));
    assert_eq!(
        diff_adj_manager.get_target_difficulty(&PowAlgorithm::Monero),
        Ok(Difficulty::from(1))
    );
    assert_eq!(
        diff_adj_manager.get_target_difficulty(&PowAlgorithm::Blake),
        Ok(Difficulty::from(18))
    );

    let pow_algos = vec![
        PowAlgorithm::Blake,
        PowAlgorithm::Blake,
        PowAlgorithm::Monero,
        PowAlgorithm::Monero,
        PowAlgorithm::Blake,
        PowAlgorithm::Monero,
        PowAlgorithm::Blake,
        PowAlgorithm::Monero,
    ];
    assert_eq!(store.get_height(), Ok(Some(8)));
    let tip = store.fetch_block(8).unwrap().block;
    append_to_pow_blockchain(&store, tip, pow_algos);
    assert_eq!(
        diff_adj_manager.get_target_difficulty(&PowAlgorithm::Monero),
        Ok(Difficulty::from(2))
    );
    assert_eq!(
        diff_adj_manager.get_target_difficulty(&PowAlgorithm::Blake),
        Ok(Difficulty::from(9))
    );
}

#[test]
fn test_median_timestamp() {
    let store = create_mem_db();
    let diff_adj_manager = DiffAdjManager::new(store.clone()).unwrap();
    let consensus = ConsensusConstants::current();
    let pow_algos = vec![PowAlgorithm::Blake]; // GB default
    create_test_pow_blockchain(&store, pow_algos);
    let mut timestamp = diff_adj_manager
        .get_median_timestamp()
        .expect("median returned an error");
    assert_eq!(timestamp, 1575018842.into());
    let mut prev_timestamp: EpochTime = 1575018842.into();
    let pow_algos = vec![PowAlgorithm::Blake];
    // lets add 1
    let tip = store.fetch_block(store.get_height().unwrap().unwrap()).unwrap().block;
    append_to_pow_blockchain(&store, tip, pow_algos.clone());
    prev_timestamp = 1575018842.into();
    prev_timestamp = prev_timestamp.increase(consensus.get_target_block_interval());
    timestamp = diff_adj_manager
        .get_median_timestamp()
        .expect("median returned an error");
    assert_eq!(timestamp, prev_timestamp);
    // lets add 1
    let tip = store.fetch_block(store.get_height().unwrap().unwrap()).unwrap().block;
    append_to_pow_blockchain(&store, tip, pow_algos.clone());
    prev_timestamp = 1575018842.into();
    prev_timestamp = prev_timestamp.increase(consensus.get_target_block_interval());
    timestamp = diff_adj_manager
        .get_median_timestamp()
        .expect("median returned an error");
    assert_eq!(timestamp, prev_timestamp);

    // lets build up 11 blocks
    for i in 4..12 {
        let tip = store.fetch_block(store.get_height().unwrap().unwrap()).unwrap().block;
        append_to_pow_blockchain(&store, tip, pow_algos.clone());
        prev_timestamp = 1575018842.into();
        prev_timestamp = prev_timestamp.increase(consensus.get_target_block_interval() * (i / 2));
        timestamp = diff_adj_manager
            .get_median_timestamp()
            .expect("median returned an error");
        assert_eq!(timestamp, prev_timestamp);
    }

    // lets add many1 blocks
    for _i in 1..20 {
        let tip = store.fetch_block(store.get_height().unwrap().unwrap()).unwrap().block;
        append_to_pow_blockchain(&store, tip, pow_algos.clone());
        prev_timestamp = prev_timestamp.increase(consensus.get_target_block_interval());
        timestamp = diff_adj_manager
            .get_median_timestamp()
            .expect("median returned an error");
        assert_eq!(timestamp, prev_timestamp);
    }
}

#[test]
fn test_median_timestamp_with_height() {
    let store = create_mem_db();
    let diff_adj_manager = DiffAdjManager::new(store.clone()).unwrap();
    let pow_algos = vec![
        PowAlgorithm::Blake, // GB default
        PowAlgorithm::Monero,
        PowAlgorithm::Blake,
        PowAlgorithm::Monero,
        PowAlgorithm::Blake,
    ];
    create_test_pow_blockchain(&store, pow_algos);

    let header0_timestamp = store.fetch_header(0).unwrap().timestamp;
    let header1_timestamp = store.fetch_header(1).unwrap().timestamp;
    let header2_timestamp = store.fetch_header(2).unwrap().timestamp;

    let timestamp = diff_adj_manager
        .get_median_timestamp_at_height(0)
        .expect("median returned an error");
    assert_eq!(timestamp, header0_timestamp);

    let timestamp = diff_adj_manager
        .get_median_timestamp_at_height(3)
        .expect("median returned an error");
    assert_eq!(timestamp, header2_timestamp);

    let timestamp = diff_adj_manager
        .get_median_timestamp_at_height(2)
        .expect("median returned an error");
    assert_eq!(timestamp, header1_timestamp);

    let timestamp = diff_adj_manager
        .get_median_timestamp_at_height(4)
        .expect("median returned an error");
    assert_eq!(timestamp, header2_timestamp);
}

#[test]
fn test_median_timestamp_odd_order() {
    let store = create_mem_db();
    let diff_adj_manager = DiffAdjManager::new(store.clone()).unwrap();
    let consensus = ConsensusConstants::current();
    let pow_algos = vec![PowAlgorithm::Blake]; // GB default
    create_test_pow_blockchain(&store, pow_algos);
    let mut timestamp = diff_adj_manager
        .get_median_timestamp()
        .expect("median returned an error");
    assert_eq!(timestamp, 1575018842.into());
    let mut prev_timestamp: EpochTime = 1575018842.into();
    let pow_algos = vec![PowAlgorithm::Blake];
    // lets add 1
    let tip = store.fetch_block(store.get_height().unwrap().unwrap()).unwrap().block;
    append_to_pow_blockchain(&store, tip, pow_algos.clone());
    prev_timestamp = 1575018842.into();
    prev_timestamp = prev_timestamp.increase(consensus.get_target_block_interval());
    timestamp = diff_adj_manager
        .get_median_timestamp()
        .expect("median returned an error");
    assert_eq!(timestamp, prev_timestamp);

    // lets add 1 that's further back then
    let append_height = store.get_height().unwrap().unwrap();
    let prev_block = store.fetch_block(append_height).unwrap().block().clone();
    let new_block = chain_block(&prev_block, Vec::new());
    let mut new_block = store.calculate_mmr_roots(new_block).unwrap();
    new_block.header.timestamp = EpochTime::from(1575018842).increase(consensus.get_target_block_interval() / 2);
    new_block.header.pow.pow_algo = PowAlgorithm::Blake;
    store.add_block(new_block).unwrap();

    prev_timestamp = 1575018842.into();
    prev_timestamp = prev_timestamp.increase(consensus.get_target_block_interval() / 2);
    timestamp = diff_adj_manager
        .get_median_timestamp()
        .expect("median returned an error");
    // Median timestamp should be block 3 and not block 2
    assert_eq!(timestamp, prev_timestamp);
}
