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

use helpers::block_builders::chain_block;
use tari_core::{
    blocks::Block,
    chain_storage::{BlockchainDatabase, MemoryDatabase},
    consensus::{ConsensusConstants, ConsensusManagerBuilder, Network},
    helpers::create_mem_db,
    proof_of_work::{
        lwma_diff::LinearWeightedMovingAverage,
        DiffAdjManager,
        Difficulty,
        DifficultyAdjustment,
        PowAlgorithm,
    },
    transactions::types::HashDigest,
};
use tari_crypto::tari_utilities::epoch_time::EpochTime;

fn create_test_pow_blockchain(
    db: &BlockchainDatabase<MemoryDatabase<HashDigest>>,
    mut pow_algos: Vec<PowAlgorithm>,
    consensus_constants: &ConsensusConstants,
)
{
    pow_algos.remove(0);
    let block0 = db.fetch_block(0).unwrap().block().clone();
    append_to_pow_blockchain(db, block0, pow_algos, consensus_constants);
}

fn append_to_pow_blockchain(
    db: &BlockchainDatabase<MemoryDatabase<HashDigest>>,
    chain_tip: Block,
    pow_algos: Vec<PowAlgorithm>,
    consensus: &ConsensusConstants,
)
{
    let mut prev_block = chain_tip;
    for pow_algo in pow_algos {
        let new_block = chain_block(&prev_block, Vec::new(), consensus);
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
    consensus_constants: &ConsensusConstants,
) -> Difficulty
{
    let mut lwma = LinearWeightedMovingAverage::new(
        consensus_constants.get_difficulty_block_window() as usize,
        consensus_constants.get_diff_target_block_interval(),
        consensus_constants.min_pow_difficulty(),
        consensus_constants.get_difficulty_max_block_interval(),
    );
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
    let diff_adj_manager = DiffAdjManager::new(&consensus_manager.consensus_constants()).unwrap();

    assert_eq!(
        diff_adj_manager.get_target_difficulty(
            &store.metadata_read_access().unwrap(),
            &store.db_read_access().unwrap(),
            PowAlgorithm::Monero
        ),
        Ok(calculate_accumulated_difficulty(
            &store,
            vec![2, 5, 6],
            &consensus_manager.consensus_constants()
        ))
    );
    assert_eq!(
        diff_adj_manager.get_target_difficulty(
            &store.metadata_read_access().unwrap(),
            &store.db_read_access().unwrap(),
            PowAlgorithm::Blake
        ),
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
    let diff_adj_manager = DiffAdjManager::new(&consensus_manager.consensus_constants()).unwrap();
    let _ = consensus_manager.set_diff_manager(diff_adj_manager);

    let pow_algos = vec![
        PowAlgorithm::Blake, // GB default
        PowAlgorithm::Monero,
        PowAlgorithm::Blake,
        PowAlgorithm::Blake,
        PowAlgorithm::Monero,
        PowAlgorithm::Blake,
    ];
    create_test_pow_blockchain(&store, pow_algos, &consensus_manager.consensus_constants());
    assert_eq!(store.get_height(), Ok(Some(5)));
    assert_eq!(
        consensus_manager.get_target_difficulty(
            &store.metadata_read_access().unwrap(),
            &store.db_read_access().unwrap(),
            PowAlgorithm::Monero
        ),
        Ok(calculate_accumulated_difficulty(
            &store,
            vec![1, 4],
            &consensus_manager.consensus_constants()
        ))
    );
    assert_eq!(
        consensus_manager.get_target_difficulty(
            &store.metadata_read_access().unwrap(),
            &store.db_read_access().unwrap(),
            PowAlgorithm::Blake
        ),
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
        consensus_manager.get_target_difficulty(
            &store.metadata_read_access().unwrap(),
            &store.db_read_access().unwrap(),
            PowAlgorithm::Monero
        ),
        Ok(calculate_accumulated_difficulty(
            &store,
            vec![1, 4, 7, 9],
            &consensus_manager.consensus_constants()
        ))
    );
    assert_eq!(
        consensus_manager.get_target_difficulty(
            &store.metadata_read_access().unwrap(),
            &store.db_read_access().unwrap(),
            PowAlgorithm::Blake
        ),
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
    let diff_adj_manager = DiffAdjManager::new(&consensus_manager.consensus_constants()).unwrap();
    let _ = consensus_manager.set_diff_manager(diff_adj_manager);
    assert!(consensus_manager
        .get_target_difficulty_with_height(&store.db_read_access().unwrap(), PowAlgorithm::Monero, 5)
        .is_err());
    assert!(consensus_manager
        .get_target_difficulty_with_height(&store.db_read_access().unwrap(), PowAlgorithm::Blake, 5)
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
    let diff_adj_manager = DiffAdjManager::new(&consensus_manager.consensus_constants()).unwrap();
    let _ = consensus_manager.set_diff_manager(diff_adj_manager);

    assert_eq!(
        consensus_manager.get_target_difficulty_with_height(&store.db_read_access().unwrap(), PowAlgorithm::Monero, 5),
        Ok(calculate_accumulated_difficulty(
            &store,
            vec![1, 4],
            &consensus_manager.consensus_constants()
        ))
    );
    assert_eq!(
        consensus_manager.get_target_difficulty_with_height(&store.db_read_access().unwrap(), PowAlgorithm::Blake, 5),
        Ok(calculate_accumulated_difficulty(
            &store,
            vec![0, 2, 3, 5],
            &consensus_manager.consensus_constants()
        ))
    );

    assert_eq!(
        consensus_manager.get_target_difficulty_with_height(&store.db_read_access().unwrap(), PowAlgorithm::Monero, 2),
        Ok(calculate_accumulated_difficulty(
            &store,
            vec![1],
            &consensus_manager.consensus_constants()
        ))
    );
    assert_eq!(
        consensus_manager.get_target_difficulty_with_height(&store.db_read_access().unwrap(), PowAlgorithm::Blake, 2),
        Ok(calculate_accumulated_difficulty(
            &store,
            vec![0, 2],
            &consensus_manager.consensus_constants()
        ))
    );

    assert_eq!(
        consensus_manager.get_target_difficulty_with_height(&store.db_read_access().unwrap(), PowAlgorithm::Monero, 3),
        Ok(calculate_accumulated_difficulty(
            &store,
            vec![1],
            &consensus_manager.consensus_constants()
        ))
    );
    assert_eq!(
        consensus_manager.get_target_difficulty_with_height(&store.db_read_access().unwrap(), PowAlgorithm::Blake, 3),
        Ok(calculate_accumulated_difficulty(
            &store,
            vec![0, 2, 3],
            &consensus_manager.consensus_constants()
        ))
    );
}

#[test]
#[ignore] // TODO Wait for reorg logic to be refactored
fn test_full_sync_on_reorg() {
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManagerBuilder::new(network).build();
    let store = create_mem_db(&consensus_manager);
    let diff_adj_manager = DiffAdjManager::new(&consensus_manager.consensus_constants()).unwrap();

    let pow_algos = vec![
        PowAlgorithm::Blake, // GB default
        PowAlgorithm::Blake,
        PowAlgorithm::Blake,
        PowAlgorithm::Blake,
        PowAlgorithm::Monero,
    ];
    create_test_pow_blockchain(&store, pow_algos, &consensus_manager.consensus_constants());
    assert_eq!(store.get_height(), Ok(Some(4)));
    assert_eq!(
        diff_adj_manager.get_target_difficulty(
            &store.metadata_read_access().unwrap(),
            &store.db_read_access().unwrap(),
            PowAlgorithm::Monero
        ),
        Ok(Difficulty::from(1))
    );
    assert_eq!(
        diff_adj_manager.get_target_difficulty(
            &store.metadata_read_access().unwrap(),
            &store.db_read_access().unwrap(),
            PowAlgorithm::Blake
        ),
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
    append_to_pow_blockchain(&store, tip, pow_algos, &consensus_manager.consensus_constants());
    assert_eq!(
        diff_adj_manager.get_target_difficulty(
            &store.metadata_read_access().unwrap(),
            &store.db_read_access().unwrap(),
            PowAlgorithm::Monero
        ),
        Ok(Difficulty::from(2))
    );
    assert_eq!(
        diff_adj_manager.get_target_difficulty(
            &store.metadata_read_access().unwrap(),
            &store.db_read_access().unwrap(),
            PowAlgorithm::Blake
        ),
        Ok(Difficulty::from(9))
    );
}

#[test]
fn test_median_timestamp() {
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManagerBuilder::new(network).build();
    let store = create_mem_db(&consensus_manager);
    let diff_adj_manager = DiffAdjManager::new(&consensus_manager.consensus_constants()).unwrap();
    let pow_algos = vec![PowAlgorithm::Blake]; // GB default
    create_test_pow_blockchain(&store, pow_algos, &consensus_manager.consensus_constants());
    let start_timestamp = store.fetch_block(0).unwrap().block().header.timestamp.clone();
    let mut timestamp = diff_adj_manager
        .get_median_timestamp(&store.metadata_read_access().unwrap(), &store.db_read_access().unwrap())
        .expect("median returned an error");
    assert_eq!(timestamp, start_timestamp);

    let pow_algos = vec![PowAlgorithm::Blake];
    // lets add 1
    let tip = store.fetch_block(store.get_height().unwrap().unwrap()).unwrap().block;
    append_to_pow_blockchain(&store, tip, pow_algos.clone(), &consensus_manager.consensus_constants());
    let mut prev_timestamp: EpochTime =
        start_timestamp.increase(consensus_manager.consensus_constants().get_target_block_interval());
    timestamp = diff_adj_manager
        .get_median_timestamp(&store.metadata_read_access().unwrap(), &store.db_read_access().unwrap())
        .expect("median returned an error");
    assert_eq!(timestamp, prev_timestamp);
    // lets add 1
    let tip = store.fetch_block(store.get_height().unwrap().unwrap()).unwrap().block;
    append_to_pow_blockchain(&store, tip, pow_algos.clone(), &consensus_manager.consensus_constants());
    prev_timestamp = start_timestamp.increase(consensus_manager.consensus_constants().get_target_block_interval());
    timestamp = diff_adj_manager
        .get_median_timestamp(&store.metadata_read_access().unwrap(), &store.db_read_access().unwrap())
        .expect("median returned an error");
    assert_eq!(timestamp, prev_timestamp);

    // lets build up 11 blocks
    for i in 4..12 {
        let tip = store.fetch_block(store.get_height().unwrap().unwrap()).unwrap().block;
        append_to_pow_blockchain(&store, tip, pow_algos.clone(), &consensus_manager.consensus_constants());
        prev_timestamp =
            start_timestamp.increase(consensus_manager.consensus_constants().get_target_block_interval() * (i / 2));
        timestamp = diff_adj_manager
            .get_median_timestamp(&store.metadata_read_access().unwrap(), &store.db_read_access().unwrap())
            .expect("median returned an error");
        assert_eq!(timestamp, prev_timestamp);
    }

    // lets add many1 blocks
    for _i in 1..20 {
        let tip = store.fetch_block(store.get_height().unwrap().unwrap()).unwrap().block;
        append_to_pow_blockchain(&store, tip, pow_algos.clone(), &consensus_manager.consensus_constants());
        prev_timestamp = prev_timestamp.increase(consensus_manager.consensus_constants().get_target_block_interval());
        timestamp = diff_adj_manager
            .get_median_timestamp(&store.metadata_read_access().unwrap(), &store.db_read_access().unwrap())
            .expect("median returned an error");
        assert_eq!(timestamp, prev_timestamp);
    }
}

#[test]
fn test_median_timestamp_with_height() {
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManagerBuilder::new(network).build();
    let store = create_mem_db(&consensus_manager);
    let diff_adj_manager = DiffAdjManager::new(&consensus_manager.consensus_constants()).unwrap();
    let pow_algos = vec![
        PowAlgorithm::Blake, // GB default
        PowAlgorithm::Monero,
        PowAlgorithm::Blake,
        PowAlgorithm::Monero,
        PowAlgorithm::Blake,
    ];
    create_test_pow_blockchain(&store, pow_algos, &consensus_manager.consensus_constants());

    let header0_timestamp = store.fetch_header(0).unwrap().timestamp;
    let header1_timestamp = store.fetch_header(1).unwrap().timestamp;
    let header2_timestamp = store.fetch_header(2).unwrap().timestamp;

    let timestamp = diff_adj_manager
        .get_median_timestamp_at_height(&store.db_read_access().unwrap(), 0)
        .expect("median returned an error");
    assert_eq!(timestamp, header0_timestamp);

    let timestamp = diff_adj_manager
        .get_median_timestamp_at_height(&store.db_read_access().unwrap(), 3)
        .expect("median returned an error");
    assert_eq!(timestamp, header2_timestamp);

    let timestamp = diff_adj_manager
        .get_median_timestamp_at_height(&store.db_read_access().unwrap(), 2)
        .expect("median returned an error");
    assert_eq!(timestamp, header1_timestamp);

    let timestamp = diff_adj_manager
        .get_median_timestamp_at_height(&store.db_read_access().unwrap(), 4)
        .expect("median returned an error");
    assert_eq!(timestamp, header2_timestamp);
}

#[test]
fn test_median_timestamp_odd_order() {
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManagerBuilder::new(network).build();
    let store = create_mem_db(&consensus_manager);
    let diff_adj_manager = DiffAdjManager::new(&consensus_manager.consensus_constants()).unwrap();
    let pow_algos = vec![PowAlgorithm::Blake]; // GB default
    create_test_pow_blockchain(&store, pow_algos, &consensus_manager.consensus_constants());
    let start_timestamp = store.fetch_block(0).unwrap().block().header.timestamp.clone();
    let mut timestamp = diff_adj_manager
        .get_median_timestamp(&store.metadata_read_access().unwrap(), &store.db_read_access().unwrap())
        .expect("median returned an error");
    assert_eq!(timestamp, start_timestamp);
    let pow_algos = vec![PowAlgorithm::Blake];
    // lets add 1
    let tip = store.fetch_block(store.get_height().unwrap().unwrap()).unwrap().block;
    append_to_pow_blockchain(&store, tip, pow_algos.clone(), &consensus_manager.consensus_constants());
    let mut prev_timestamp: EpochTime =
        start_timestamp.increase(consensus_manager.consensus_constants().get_target_block_interval());
    timestamp = diff_adj_manager
        .get_median_timestamp(&store.metadata_read_access().unwrap(), &store.db_read_access().unwrap())
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
    timestamp = diff_adj_manager
        .get_median_timestamp(&store.metadata_read_access().unwrap(), &store.db_read_access().unwrap())
        .expect("median returned an error");
    // Median timestamp should be block 3 and not block 2
    assert_eq!(timestamp, prev_timestamp);
}
