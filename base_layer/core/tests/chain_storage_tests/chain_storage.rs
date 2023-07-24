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

use rand::{rngs::OsRng, RngCore};
use tari_common::configuration::Network;
use tari_common_types::types::BlockHash;
use tari_core::{
    blocks::{genesis_block, Block, BlockHeader},
    chain_storage::{
        create_lmdb_database,
        BlockAddResult,
        BlockchainBackend,
        BlockchainDatabase,
        BlockchainDatabaseConfig,
        ChainStorageError,
        DbTransaction,
        MmrTree,
        Validators,
    },
    consensus::{emission::Emission, ConsensusConstantsBuilder, ConsensusManager, ConsensusManagerBuilder},
    proof_of_work::Difficulty,
    test_helpers::blockchain::{
        create_store_with_consensus,
        create_store_with_consensus_and_validators,
        create_test_blockchain_db,
        create_test_db,
        TempDatabase,
    },
    transactions::{
        tari_amount::{uT, MicroMinotari, T},
        test_helpers::spend_utxos,
        CryptoFactories,
    },
    tx,
    txn_schema,
    validation::{mocks::MockValidator, DifficultyCalculator, ValidationError},
};
use tari_storage::lmdb_store::LMDBConfig;
use tari_test_utils::{paths::create_temporary_data_path, unpack_enum};

// use crate::helpers::database::create_test_db;
// use crate::helpers::database::create_store;
use crate::helpers::{
    block_builders::{
        append_block,
        chain_block,
        create_chain_header,
        create_genesis_block,
        find_header_with_achieved_difficulty,
        generate_block_with_achieved_difficulty,
        generate_new_block,
        generate_new_block_with_achieved_difficulty,
        generate_new_block_with_coinbase,
    },
    database::create_orphan_block,
    sample_blockchains::{create_new_blockchain, create_new_blockchain_lmdb},
};

#[test]
fn test_fetch_nonexistent_header() {
    let network = Network::LocalNet;
    let _consensus_manager = ConsensusManagerBuilder::new(network).build();
    let store = create_test_blockchain_db();

    assert_eq!(store.fetch_header(1).unwrap(), None);
}

#[test]
fn test_insert_and_fetch_header() {
    let network = Network::LocalNet;
    let _consensus_manager = ConsensusManagerBuilder::new(network).build();
    let store = create_test_blockchain_db();
    let genesis_block = store.fetch_tip_header().unwrap();
    let mut header1 = BlockHeader::from_previous(genesis_block.header());

    header1.kernel_mmr_size += 1;
    header1.output_mmr_size += 1;

    let chain1 = create_chain_header(header1.clone(), genesis_block.accumulated_data());

    store.insert_valid_headers(vec![chain1.clone()]).unwrap();
    let mut header2 = BlockHeader::from_previous(&header1);
    header2.kernel_mmr_size += 2;
    header2.output_mmr_size += 2;
    let chain2 = create_chain_header(header2.clone(), chain1.accumulated_data());

    store.insert_valid_headers(vec![chain2]).unwrap();
    store.fetch_header(0).unwrap();

    assert_eq!(store.fetch_header(1).unwrap().unwrap(), header1);
    assert_eq!(store.fetch_header(2).unwrap().unwrap(), header2);
}

#[test]
fn test_insert_and_fetch_orphan() {
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManagerBuilder::new(network).build();
    let store = create_test_blockchain_db();
    let txs = vec![
        (tx!(1000.into(), fee: 4.into(), inputs: 2, outputs: 1)).0,
        (tx!(2000.into(), fee: 6.into(), inputs: 1, outputs: 1)).0,
    ];
    let orphan = create_orphan_block(10, txs, &consensus_manager);
    let orphan_hash = orphan.hash();
    let mut txn = DbTransaction::new();
    txn.insert_orphan(orphan.clone().into());
    assert!(store.commit(txn).is_ok());
    assert_eq!(store.fetch_orphan(orphan_hash).unwrap(), orphan);
}

#[test]
fn test_store_and_retrieve_block() {
    let (db, blocks, _, _) = create_new_blockchain(Network::LocalNet);
    let hash = blocks[0].hash();
    // Check the metadata
    let metadata = db.get_chain_metadata().unwrap();
    assert_eq!(metadata.height_of_longest_chain(), 0);
    assert_eq!(metadata.best_block(), hash);
    assert_eq!(metadata.horizon_block(metadata.height_of_longest_chain()), 0);
    // Fetch the block back
    let block0 = db.fetch_block(0, true).unwrap();
    assert_eq!(block0.confirmations(), 1);
    // Compare the blocks
    let block0 = Block::from(block0);
    assert_eq!(blocks[0].block(), &block0);
}

#[test]
fn test_add_multiple_blocks() {
    // Create new database with genesis block
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManagerBuilder::new(network).build();
    let store = create_store_with_consensus(consensus_manager.clone());
    let metadata = store.get_chain_metadata().unwrap();
    assert_eq!(metadata.height_of_longest_chain(), 0);
    let block0 = store.fetch_block(0, true).unwrap();
    assert_eq!(metadata.best_block(), block0.hash());
    // Add another block
    let block1 = append_block(
        &store,
        &block0.try_into_chain_block().unwrap(),
        vec![],
        &consensus_manager,
        Difficulty::min(),
    )
    .unwrap();
    let metadata = store.get_chain_metadata().unwrap();
    let hash = block1.hash();
    assert_eq!(metadata.height_of_longest_chain(), 1);
    assert_eq!(metadata.best_block(), hash);
    // Adding blocks is idempotent
    assert_eq!(
        store.add_block(block1.to_arc_block()).unwrap(),
        BlockAddResult::BlockExists
    );
    // Check the metadata
    let metadata = store.get_chain_metadata().unwrap();
    assert_eq!(metadata.height_of_longest_chain(), 1);
    assert_eq!(metadata.best_block(), hash);
}

#[test]
fn test_checkpoints() {
    let network = Network::LocalNet;
    let (db, blocks, outputs, consensus_manager) = create_new_blockchain(network);

    let txn = txn_schema!(
        from: vec![outputs[0][0].clone()],
        to: vec![MicroMinotari(5_000), MicroMinotari(6_000)]
    );
    let (txn, _) = spend_utxos(txn);
    let block1 = append_block(&db, &blocks[0], vec![txn], &consensus_manager, Difficulty::min()).unwrap();
    // Get the checkpoint
    let block_a = db.fetch_block(0, false).unwrap();
    assert_eq!(block_a.confirmations(), 2);
    assert_eq!(blocks[0].block(), block_a.block());
    let block_b = db.fetch_block(1, false).unwrap();
    assert_eq!(block_b.confirmations(), 1);
    let block1 = serde_json::to_string(block1.block()).unwrap();
    let block_b = serde_json::to_string(&Block::from(block_b)).unwrap();
    assert_eq!(block1, block_b);
}

#[test]
#[allow(clippy::identity_op)]
fn test_rewind_to_height() {
    let _ = env_logger::builder().is_test(true).try_init();
    let network = Network::LocalNet;
    let (mut db, mut blocks, mut outputs, consensus_manager) = create_new_blockchain(network);

    // Block 1
    let schema = vec![txn_schema!(from: vec![outputs[0][0].clone()], to: vec![6 * T, 3 * T])];
    unpack_enum!(
        BlockAddResult::Ok(_b1) =
            generate_new_block(&mut db, &mut blocks, &mut outputs, schema, &consensus_manager).unwrap()
    );
    // Block 2
    let schema = vec![txn_schema!(from: vec![outputs[1][0].clone()], to: vec![3 * T, 1 * T])];
    unpack_enum!(
        BlockAddResult::Ok(_b2) =
            generate_new_block(&mut db, &mut blocks, &mut outputs, schema, &consensus_manager).unwrap()
    );
    // Block 3
    let schema = vec![
        txn_schema!(from: vec![outputs[2][0].clone()], to: vec![2 * T, 500_000 * uT]),
        txn_schema!(from: vec![outputs[1][1].clone()], to: vec![500_000 * uT]),
    ];
    unpack_enum!(
        BlockAddResult::Ok(_b3) =
            generate_new_block(&mut db, &mut blocks, &mut outputs, schema, &consensus_manager).unwrap()
    );

    db.rewind_to_height(3).unwrap();
    assert_eq!(db.get_height().unwrap(), 3);

    // Invalid rewind
    assert!(db.rewind_to_height(4).is_err());
    assert_eq!(db.get_height().unwrap(), 3);
    db.rewind_to_height(1).unwrap();
    assert_eq!(db.get_height().unwrap(), 1);
}

#[test]
fn test_coverage_chain_storage() {
    let validators = Validators::new(
        MockValidator::new(true),
        MockValidator::new(true),
        MockValidator::new(true),
    );
    let network = Network::LocalNet;
    let rules = ConsensusManagerBuilder::new(network).build();
    let db = create_test_db();
    assert_eq!(db.kernel_count().unwrap(), 0);
    let store = BlockchainDatabase::new(
        db,
        rules.clone(),
        validators,
        BlockchainDatabaseConfig::default(),
        DifficultyCalculator::new(rules.clone(), Default::default()),
    )
    .unwrap();

    let block0 = store.fetch_block(0, true).unwrap();
    append_block(
        &store,
        &block0.clone().try_into_chain_block().unwrap(),
        vec![],
        &rules,
        Difficulty::min(),
    )
    .unwrap();
    assert_eq!(store.fetch_all_reorgs().unwrap(), vec![]);
    assert_eq!(store.fetch_mmr_size(MmrTree::Kernel).unwrap(), 2);
    assert_eq!(store.fetch_mmr_size(MmrTree::Utxo).unwrap(), 2);

    let mut txn = DbTransaction::new();
    txn.insert_bad_block(*block0.hash(), 0);
    store.commit(txn).unwrap();
}

#[test]
fn test_rewind_past_horizon_height() {
    let network = Network::LocalNet;
    let block0 = genesis_block::get_esmeralda_genesis_block();
    let consensus_manager = ConsensusManagerBuilder::new(network).with_block(block0.clone()).build();
    let validators = Validators::new(
        MockValidator::new(true),
        MockValidator::new(true),
        MockValidator::new(true),
    );
    let db = create_test_db();
    let config = BlockchainDatabaseConfig {
        orphan_storage_capacity: 3,
        pruning_horizon: 2,
        pruning_interval: 1,
        ..Default::default()
    };
    let store = BlockchainDatabase::new(
        db,
        consensus_manager.clone(),
        validators,
        config,
        DifficultyCalculator::new(consensus_manager.clone(), Default::default()),
    )
    .unwrap();

    let block1 = append_block(&store, &block0, vec![], &consensus_manager, Difficulty::min()).unwrap();
    let block2 = append_block(&store, &block1, vec![], &consensus_manager, Difficulty::min()).unwrap();
    let block3 = append_block(&store, &block2, vec![], &consensus_manager, Difficulty::min()).unwrap();
    let _block4 = append_block(&store, &block3, vec![], &consensus_manager, Difficulty::min()).unwrap();

    let metadata = store.get_chain_metadata().unwrap();
    assert_eq!(metadata.height_of_longest_chain(), 4);
    // we should not be able to rewind to the future
    assert!(store.rewind_to_height(metadata.height_of_longest_chain() + 1).is_err());
    let horizon_height = metadata.pruned_height();
    assert_eq!(horizon_height, 2);
    // rewinding past pruning horizon should set us to height 0 so we can resync from gen block.
    assert!(store.rewind_to_height(horizon_height - 1).is_ok());
    let metadata = store.get_chain_metadata().unwrap();
    assert_eq!(metadata.height_of_longest_chain(), 0);
}

#[test]
#[allow(clippy::too_many_lines)]
fn test_handle_tip_reorg_with_zero_conf() {
    // GB --> A1 --> A2 --> A3(Low PoW)      [Main Chain]
    //          \--> B2 --> B3 -- B4 --> B5(Highest PoW)  [Forked Chain]

    // Create Main Chain
    let network = Network::LocalNet;
    let (mut store, mut blocks, mut outputs, consensus_manager) = create_new_blockchain(network);
    // Block A1
    let txs = vec![txn_schema!(
        from: vec![outputs[0][0].clone()],
        to: vec![10 * T, 10 * T, 10 * T, 10 * T]
    )];
    generate_new_block_with_achieved_difficulty(
        &mut store,
        &mut blocks,
        &mut outputs,
        txs,
        Difficulty::from(1),
        &consensus_manager,
    )
    .unwrap();
    // Block A2
    let txs_1 = txn_schema!(from: vec![outputs[1][3].clone()], to: vec![6 * T]);
    let (tx_1, utxos_1) = spend_utxos(txs_1);
    // create zero conf
    let txs_2 = txn_schema!(from: vec![utxos_1[0].clone()], to: vec![4 * T]);
    let (tx_2, utxos_2) = spend_utxos(txs_2);
    let txns = vec![tx_1, tx_2];

    outputs.push(utxos_2);
    generate_block_with_achieved_difficulty(&mut store, &mut blocks, txns, Difficulty::from(3), &consensus_manager)
        .unwrap();

    // Block A3
    let txs = vec![txn_schema!(from: vec![outputs[2][0].clone()], to: vec![2 * T])];
    assert!(generate_new_block_with_achieved_difficulty(
        &mut store,
        &mut blocks,
        &mut outputs,
        txs,
        Difficulty::from(1),
        &consensus_manager
    )
    .is_ok());
    assert_eq!(store.get_chain_metadata().unwrap().height_of_longest_chain(), 3);

    // Create Forked Chain

    let mut orphan_store = create_store_with_consensus(consensus_manager.clone());
    orphan_store.add_block(blocks[1].to_arc_block()).unwrap();
    let mut orphan_blocks = vec![blocks[0].clone(), blocks[1].clone()];
    let mut orphan_outputs = vec![outputs[0].clone(), outputs[1].clone()];
    // Block B2
    let txs = vec![txn_schema!(
        from: vec![
            orphan_outputs[1][0].clone(),
            orphan_outputs[1][1].clone(),
            orphan_outputs[1][2].clone(),
            orphan_outputs[1][3].clone(),
            orphan_outputs[1][4].clone(),
        ],
        to: vec![5 * T, 5 * T, 5 * T, 5 * T, 5 * T]
    )];
    generate_new_block_with_achieved_difficulty(
        &mut orphan_store,
        &mut orphan_blocks,
        &mut orphan_outputs,
        txs,
        Difficulty::from(7),
        &consensus_manager,
    )
    .unwrap();

    // Adding B2 to the main chain will produce a reorg to GB->A1->B2.
    if let Ok(BlockAddResult::ChainReorg { .. }) = store.add_block(orphan_blocks[2].to_arc_block()) {
    } else {
        panic!();
    }

    assert_eq!(store.fetch_tip_header().unwrap().header(), orphan_blocks[2].header());

    // Check that B2 was removed from the block orphans and A2 has been orphaned.
    assert!(store.fetch_orphan(*orphan_blocks[2].hash()).is_err());
    assert!(store.fetch_orphan(*blocks[2].hash()).is_ok());
    assert_eq!(store.get_chain_metadata().unwrap().height_of_longest_chain(), 2);

    // Block B3
    let txs = vec![
        txn_schema!(from: vec![orphan_outputs[2][0].clone()], to: vec![3 * T]),
        txn_schema!(from: vec![orphan_outputs[2][1].clone()], to: vec![3 * T]),
        txn_schema!(from: vec![orphan_outputs[2][2].clone()], to: vec![3 * T]),
        txn_schema!(from: vec![orphan_outputs[2][3].clone()], to: vec![3 * T]),
        txn_schema!(from: vec![orphan_outputs[2][4].clone()], to: vec![3 * T]),
        txn_schema!(from: vec![orphan_outputs[2][5].clone()], to: vec![3 * T]),
    ];
    assert!(generate_new_block_with_achieved_difficulty(
        &mut orphan_store,
        &mut orphan_blocks,
        &mut orphan_outputs,
        txs,
        Difficulty::from(1),
        &consensus_manager
    )
    .is_ok());

    if let Ok(BlockAddResult::Ok { .. }) = store.add_block(orphan_blocks[3].to_arc_block()) {
    } else {
        panic!();
    }
    // Block B4
    let txs = vec![
        txn_schema!(from: vec![orphan_outputs[3][0].clone()], to: vec![1500000 * uT]),
        txn_schema!(from: vec![orphan_outputs[3][1].clone()], to: vec![1500000 * uT]),
    ];
    assert!(generate_new_block_with_achieved_difficulty(
        &mut orphan_store,
        &mut orphan_blocks,
        &mut orphan_outputs,
        txs,
        Difficulty::from(1),
        &consensus_manager
    )
    .is_ok());

    if let Ok(BlockAddResult::Ok { .. }) = store.add_block(orphan_blocks[4].to_arc_block()) {
    } else {
        panic!();
    }

    // Block B5
    let txs = vec![
        txn_schema!(from: vec![orphan_outputs[4][0].clone()], to: vec![50000 * uT]),
        txn_schema!(from: vec![orphan_outputs[4][1].clone()], to: vec![50000 * uT]),
    ];
    assert!(generate_new_block_with_achieved_difficulty(
        &mut orphan_store,
        &mut orphan_blocks,
        &mut orphan_outputs,
        txs,
        Difficulty::from(1),
        &consensus_manager
    )
    .is_ok());

    if let Ok(BlockAddResult::Ok { .. }) = store.add_block(orphan_blocks[5].to_arc_block()) {
    } else {
        panic!();
    }
    assert_eq!(store.get_chain_metadata().unwrap().height_of_longest_chain(), 5);
}
#[test]
#[allow(clippy::too_many_lines)]
fn test_handle_tip_reorg() {
    // GB --> A1 --> A2(Low PoW)      [Main Chain]
    //          \--> B2(Highest PoW)  [Forked Chain]
    // Initially, the main chain is GB->A1->A2. B2 has a higher accumulated PoW and when B2 is added the main chain is
    // reorged to GB->A1->B2

    // Create Main Chain
    let network = Network::LocalNet;
    let (mut store, mut blocks, mut outputs, consensus_manager) = create_new_blockchain(network);
    // Block A1
    let txs = vec![txn_schema!(
        from: vec![outputs[0][0].clone()],
        to: vec![10 * T, 10 * T, 10 * T, 10 * T]
    )];
    generate_new_block_with_achieved_difficulty(
        &mut store,
        &mut blocks,
        &mut outputs,
        txs,
        Difficulty::from(1),
        &consensus_manager,
    )
    .unwrap();
    // Block A2
    let txs = vec![txn_schema!(from: vec![outputs[1][3].clone()], to: vec![6 * T])];
    generate_new_block_with_achieved_difficulty(
        &mut store,
        &mut blocks,
        &mut outputs,
        txs,
        Difficulty::from(3),
        &consensus_manager,
    )
    .unwrap();

    // Create Forked Chain

    let mut orphan_store = create_store_with_consensus(consensus_manager.clone());
    orphan_store.add_block(blocks[1].to_arc_block()).unwrap();
    let mut orphan_blocks = vec![blocks[0].clone(), blocks[1].clone()];
    let mut orphan_outputs = vec![outputs[0].clone(), outputs[1].clone()];
    // Block B2
    let txs = vec![txn_schema!(from: vec![orphan_outputs[1][0].clone()], to: vec![5 * T])];
    generate_new_block_with_achieved_difficulty(
        &mut orphan_store,
        &mut orphan_blocks,
        &mut orphan_outputs,
        txs,
        Difficulty::from(7),
        &consensus_manager,
    )
    .unwrap();

    // Adding B2 to the main chain will produce a reorg to GB->A1->B2.
    if let Ok(BlockAddResult::ChainReorg { .. }) = store.add_block(orphan_blocks[2].to_arc_block()) {
    } else {
        panic!();
    }
    assert_eq!(store.fetch_tip_header().unwrap().header(), orphan_blocks[2].header());

    // Check that B2 was removed from the block orphans and A2 has been orphaned.
    assert!(store.fetch_orphan(*orphan_blocks[2].hash()).is_err());
    assert!(store.fetch_orphan(*blocks[2].hash()).is_ok());
}

#[test]
fn test_handle_tip_reset() {
    // GB --> A1 --> A2(Low PoW)      [Main Chain]
    //          \--> B2(Highest PoW)  [Forked Chain]
    // Initially, the main chain is GB->A1->A2. B2 has a higher accumulated PoW and when B2 is added the main chain is
    // reorged to GB->A1->B2

    // Create Main Chain
    let network = Network::LocalNet;
    let (mut store, mut blocks, mut outputs, consensus_manager) = create_new_blockchain(network);
    // Block A1
    let txs = vec![txn_schema!(
        from: vec![outputs[0][0].clone()],
        to: vec![10 * T, 10 * T, 10 * T, 10 * T]
    )];
    generate_new_block_with_achieved_difficulty(
        &mut store,
        &mut blocks,
        &mut outputs,
        txs,
        Difficulty::from(1),
        &consensus_manager,
    )
    .unwrap();
    // Block A2
    let txs = vec![txn_schema!(from: vec![outputs[1][3].clone()], to: vec![6 * T])];
    generate_new_block_with_achieved_difficulty(
        &mut store,
        &mut blocks,
        &mut outputs,
        txs,
        Difficulty::from(3),
        &consensus_manager,
    )
    .unwrap();

    // Create Forked Chain

    let mut orphan_store = create_store_with_consensus(consensus_manager.clone());
    orphan_store.add_block(blocks[1].to_arc_block()).unwrap();
    let mut orphan_blocks = vec![blocks[0].clone(), blocks[1].clone()];
    let mut orphan_outputs = vec![outputs[0].clone(), outputs[1].clone()];
    // Block B2
    let txs = vec![txn_schema!(from: vec![orphan_outputs[1][0].clone()], to: vec![5 * T])];
    generate_new_block_with_achieved_difficulty(
        &mut orphan_store,
        &mut orphan_blocks,
        &mut orphan_outputs,
        txs,
        Difficulty::from(7),
        &consensus_manager,
    )
    .unwrap();

    // Adding B2 to the main chain will produce a reorg to GB->A1->B2.
    if let Ok(BlockAddResult::ChainReorg { .. }) = store.add_block(orphan_blocks[2].to_arc_block()) {
    } else {
        panic!();
    }

    assert_eq!(store.fetch_tip_header().unwrap().header().height, 2);
    store.rewind_to_height(1).unwrap();
    assert_eq!(store.fetch_tip_header().unwrap().header().height, 1);
    // both tips should be in the orphan pool
    assert!(store.fetch_orphan(*orphan_blocks[2].hash()).is_ok());
    assert!(store.fetch_orphan(*blocks[2].hash()).is_ok());
    store.swap_to_highest_pow_chain().unwrap();
    // should no be on B2

    assert_eq!(store.fetch_tip_header().unwrap().header().height, 2);
    assert_eq!(store.fetch_tip_header().unwrap().hash(), orphan_blocks[2].hash());
    assert!(store.fetch_orphan(*blocks[2].hash()).is_ok());

    store.swap_to_highest_pow_chain().unwrap();
    // Chain should not have swapped
    assert_eq!(store.fetch_tip_header().unwrap().hash(), orphan_blocks[2].hash());
    assert!(store.fetch_orphan(*blocks[2].hash()).is_ok());

    // lets reset to A1 again
    store.rewind_to_height(1).unwrap();
    assert_eq!(store.fetch_tip_header().unwrap().header().height, 1);
    store.cleanup_all_orphans().unwrap();
    store.swap_to_highest_pow_chain().unwrap();
    // current main chain should be the highest so is it still?
    assert_eq!(store.fetch_tip_header().unwrap().header().height, 1);
    assert_eq!(store.fetch_tip_header().unwrap().hash(), blocks[1].hash());
}

#[test]
#[allow(clippy::identity_op)]
#[allow(clippy::too_many_lines)]
fn test_handle_reorg() {
    // GB --> A1 --> A2 --> A3 -----> A4(Low PoW)     [Main Chain]
    //          \--> B2 --> B3(?) --> B4(Medium PoW)  [Forked Chain 1]
    //                        \-----> C4(Highest PoW) [Forked Chain 2]
    // Initially, the main chain is GB->A1->A2->A3->A4 with orphaned blocks B2, B4, C4. When B3 arrives late and is
    // added to the blockchain then a reorg is triggered and the main chain is reorganized to GB->A1->B2->B3->C4.

    // Create Main Chain
    let network = Network::LocalNet;
    let (mut store, mut blocks, mut outputs, consensus_manager) = create_new_blockchain(network);
    // Block A1
    let txs = vec![txn_schema!(
        from: vec![outputs[0][0].clone()],
        to: vec![10 * T, 10 * T, 10 * T, 10 * T]
    )];
    assert!(generate_new_block_with_achieved_difficulty(
        &mut store,
        &mut blocks,
        &mut outputs,
        txs,
        Difficulty::from(1),
        &consensus_manager
    )
    .is_ok());
    // Block A2
    let txs = vec![txn_schema!(from: vec![outputs[1][3].clone()], to: vec![6 * T])];
    assert!(generate_new_block_with_achieved_difficulty(
        &mut store,
        &mut blocks,
        &mut outputs,
        txs,
        Difficulty::from(3),
        &consensus_manager
    )
    .is_ok());
    // Block A3
    let txs = vec![txn_schema!(from: vec![outputs[2][0].clone()], to: vec![2 * T])];
    assert!(generate_new_block_with_achieved_difficulty(
        &mut store,
        &mut blocks,
        &mut outputs,
        txs,
        Difficulty::from(1),
        &consensus_manager
    )
    .is_ok());
    // Block A4
    let txs = vec![txn_schema!(from: vec![outputs[1][0].clone()], to: vec![2 * T])];
    assert!(generate_new_block_with_achieved_difficulty(
        &mut store,
        &mut blocks,
        &mut outputs,
        txs,
        Difficulty::from(1),
        &consensus_manager
    )
    .is_ok());

    // Create Forked Chain 1
    let mut orphan1_store = create_store_with_consensus(consensus_manager.clone());
    orphan1_store
        .add_block(blocks[1].to_arc_block())
        .unwrap()
        .assert_added(); // A1
    let mut orphan1_blocks = vec![blocks[0].clone(), blocks[1].clone()];
    let mut orphan1_outputs = vec![outputs[0].clone(), outputs[1].clone()];
    // Block B2
    let txs = vec![txn_schema!(from: vec![orphan1_outputs[1][0].clone()], to: vec![5 * T])];
    assert!(generate_new_block_with_achieved_difficulty(
        &mut orphan1_store,
        &mut orphan1_blocks,
        &mut orphan1_outputs,
        txs,
        Difficulty::from(1),
        &consensus_manager
    )
    .is_ok());
    // Block B3
    let txs = vec![
        txn_schema!(from: vec![orphan1_outputs[1][3].clone()], to: vec![3 * T]),
        txn_schema!(from: vec![orphan1_outputs[2][0].clone()], to: vec![3 * T]),
    ];
    assert!(generate_new_block_with_achieved_difficulty(
        &mut orphan1_store,
        &mut orphan1_blocks,
        &mut orphan1_outputs,
        txs,
        Difficulty::from(1),
        &consensus_manager
    )
    .is_ok());
    // Block B4
    let txs = vec![txn_schema!(from: vec![orphan1_outputs[3][0].clone()], to: vec![1 * T])];
    assert!(generate_new_block_with_achieved_difficulty(
        &mut orphan1_store,
        &mut orphan1_blocks,
        &mut orphan1_outputs,
        txs,
        Difficulty::from(5),
        &consensus_manager
    )
    .is_ok());

    // Create Forked Chain 2
    let mut orphan2_store = create_store_with_consensus(consensus_manager.clone());
    orphan2_store
        .add_block(blocks[1].to_arc_block())
        .unwrap()
        .assert_added(); // A1
    orphan2_store
        .add_block(orphan1_blocks[2].to_arc_block())
        .unwrap()
        .assert_added(); // B2
    orphan2_store
        .add_block(orphan1_blocks[3].to_arc_block())
        .unwrap()
        .assert_added(); // B3
    let mut orphan2_blocks = vec![
        blocks[0].clone(),
        blocks[1].clone(),
        orphan1_blocks[2].clone(),
        orphan1_blocks[3].clone(),
    ];
    let mut orphan2_outputs = vec![
        outputs[0].clone(),
        outputs[1].clone(),
        orphan1_outputs[2].clone(),
        orphan1_outputs[3].clone(),
    ];
    // Block C4
    let txs = vec![txn_schema!(from: vec![orphan2_outputs[3][1].clone()], to: vec![1 * T])];
    assert!(generate_new_block_with_achieved_difficulty(
        &mut orphan2_store,
        &mut orphan2_blocks,
        &mut orphan2_outputs,
        txs,
        Difficulty::from(20),
        &consensus_manager
    )
    .is_ok());

    // Now add the fork blocks C4, B2, B4 and B3 (out of order) to the first DB and observe a reorg. Blocks are added
    // out of order to test the forward and reverse chaining.
    store
        .add_block(orphan2_blocks[4].to_arc_block())
        .unwrap()
        .assert_orphaned(); // C4
    store
        .add_block(orphan1_blocks[2].to_arc_block())
        .unwrap()
        .assert_orphaned(); // B2
    store
        .add_block(orphan1_blocks[4].to_arc_block())
        .unwrap()
        .assert_orphaned(); // B4
    store
        .add_block(orphan1_blocks[3].to_arc_block())
        .unwrap()
        .assert_reorg(3, 3); // B3
    assert_eq!(store.fetch_tip_header().unwrap().header(), orphan2_blocks[4].header());

    // Check that B2,B3 and C4 were removed from the block orphans and A2,A3,A4 and B4 has been orphaned.
    assert!(store.fetch_orphan(*orphan1_blocks[2].hash()).is_err()); // B2
    assert!(store.fetch_orphan(*orphan1_blocks[3].hash()).is_err()); // B3
    assert!(store.fetch_orphan(*orphan2_blocks[4].hash()).is_err()); // C4
    assert!(store.fetch_orphan(*blocks[2].hash()).is_ok()); // A2
    assert!(store.fetch_orphan(*blocks[3].hash()).is_ok()); // A3
    assert!(store.fetch_orphan(*blocks[4].hash()).is_ok()); // A4
    assert!(store.fetch_orphan(*blocks[4].hash()).is_ok()); // B4
}

#[test]
#[allow(clippy::too_many_lines)]
fn test_reorgs_should_update_orphan_tips() {
    // Create a main chain GB -> A1 -> A2
    // Create an orphan chain GB -> B1
    // Add a block B2 that forces a reorg to B2
    // Check that A2 is in the orphan chain tips db
    // Add a block A3 that forces a reorg to A3
    // Check that B2 is in the orphan chain tips db
    // Add 2 blocks B3 and B4 that force a reorg to B4
    // Check that A3 is in the orphan chain tips db
    // Add 2 blocks A4 and A5 that force a reorg to A5
    // Check that B4 is in the orphan chain tips db

    let network = Network::LocalNet;
    let (store, blocks, outputs, consensus_manager) = create_new_blockchain(network);

    // Create "A" Chain
    let mut a_store = create_store_with_consensus(consensus_manager.clone());
    let mut a_blocks = vec![blocks[0].clone()];
    let mut a_outputs = vec![outputs[0].clone()];

    // Block A1
    let txs = vec![txn_schema!(from: vec![a_outputs[0][0].clone()], to: vec![50 * T])];
    generate_new_block_with_achieved_difficulty(
        &mut a_store,
        &mut a_blocks,
        &mut a_outputs,
        txs,
        Difficulty::from(1),
        &consensus_manager,
    )
    .unwrap();

    store.add_block(a_blocks[1].to_arc_block()).unwrap().assert_added();

    // Block A2
    let txs = vec![txn_schema!(from: vec![a_outputs[1][1].clone()], to: vec![30 * T])];
    generate_new_block_with_achieved_difficulty(
        &mut a_store,
        &mut a_blocks,
        &mut a_outputs,
        txs,
        Difficulty::from(3),
        &consensus_manager,
    )
    .unwrap();

    store.add_block(a_blocks[2].to_arc_block()).unwrap().assert_added();
    let a2_hash = *a_blocks[2].hash();

    // Create "B" Chain
    let mut b_store = create_store_with_consensus(consensus_manager.clone());
    let mut b_blocks = vec![blocks[0].clone()];
    let mut b_outputs = vec![outputs[0].clone()];

    // Block B1
    let txs = vec![txn_schema!(from: vec![b_outputs[0][0].clone()], to: vec![50 * T])];
    generate_new_block_with_achieved_difficulty(
        &mut b_store,
        &mut b_blocks,
        &mut b_outputs,
        txs,
        Difficulty::from(2),
        &consensus_manager,
    )
    .unwrap();

    store.add_block(b_blocks[1].to_arc_block()).unwrap().assert_orphaned();
    let b1_hash = *b_blocks[1].hash();

    // check that B1 is in orphan tips
    let orphan_tip_b1 = store
        .db_read_access()
        .unwrap()
        .fetch_orphan_chain_tip_by_hash(&b1_hash)
        .unwrap();
    assert!(orphan_tip_b1.is_some());
    assert_eq!(orphan_tip_b1.unwrap().hash(), &b1_hash);

    // Block B2
    let txs = vec![txn_schema!(from: vec![b_outputs[1][0].clone()], to: vec![40 * T])];
    generate_new_block_with_achieved_difficulty(
        &mut b_store,
        &mut b_blocks,
        &mut b_outputs,
        txs,
        Difficulty::from(4),
        &consensus_manager,
    )
    .unwrap();

    store.add_block(b_blocks[2].to_arc_block()).unwrap().assert_reorg(2, 2);
    let b2_hash = *b_blocks[2].hash();

    // check that A2 is now in the orphan chain tip db
    let orphan_tip_a2 = store
        .db_read_access()
        .unwrap()
        .fetch_orphan_chain_tip_by_hash(&a2_hash)
        .unwrap();
    assert!(orphan_tip_a2.is_some());
    assert_eq!(orphan_tip_a2.unwrap().hash(), &a2_hash);

    // check that B1 was removed from orphan chain tips
    let orphan_tip_b1 = store
        .db_read_access()
        .unwrap()
        .fetch_orphan_chain_tip_by_hash(&b1_hash)
        .unwrap();
    assert!(orphan_tip_b1.is_none());

    // Block A3
    let txs = vec![txn_schema!(from: vec![a_outputs[2][0].clone()], to: vec![25 * T])];
    generate_new_block_with_achieved_difficulty(
        &mut a_store,
        &mut a_blocks,
        &mut a_outputs,
        txs,
        Difficulty::from(5), // A chain accumulated difficulty 9
        &consensus_manager,
    )
    .unwrap();

    store.add_block(a_blocks[3].to_arc_block()).unwrap().assert_reorg(3, 2);
    let a3_hash = *a_blocks[3].hash();

    // check that B2 is now in the orphan chain tip db
    let orphan_tip_b2 = store
        .db_read_access()
        .unwrap()
        .fetch_orphan_chain_tip_by_hash(&b2_hash)
        .unwrap();
    assert!(orphan_tip_b2.is_some());
    assert_eq!(orphan_tip_b2.unwrap().hash(), &b2_hash);

    // check that A2 was removed from orphan chain tips
    let orphan_tip_a2 = store
        .db_read_access()
        .unwrap()
        .fetch_orphan_chain_tip_by_hash(&a2_hash)
        .unwrap();
    assert!(orphan_tip_a2.is_none());

    // Block B3
    let txs = vec![txn_schema!(from: vec![b_outputs[2][0].clone()], to: vec![30 * T])];
    generate_new_block_with_achieved_difficulty(
        &mut b_store,
        &mut b_blocks,
        &mut b_outputs,
        txs,
        Difficulty::from(1), // B chain accumulated difficulty 7
        &consensus_manager,
    )
    .unwrap();

    store.add_block(b_blocks[3].to_arc_block()).unwrap().assert_orphaned();
    let b3_hash = *b_blocks[3].hash();

    // Block B4
    let txs = vec![txn_schema!(from: vec![b_outputs[3][0].clone()], to: vec![20 * T])];
    generate_new_block_with_achieved_difficulty(
        &mut b_store,
        &mut b_blocks,
        &mut b_outputs,
        txs,
        Difficulty::from(5), // B chain accumulated difficulty 12
        &consensus_manager,
    )
    .unwrap();

    store.add_block(b_blocks[4].to_arc_block()).unwrap().assert_reorg(4, 3);
    let b4_hash = *b_blocks[4].hash();

    // check that A3 is now in the orphan chain tip db
    let orphan_tip_a3 = store
        .db_read_access()
        .unwrap()
        .fetch_orphan_chain_tip_by_hash(&a3_hash)
        .unwrap();
    assert!(orphan_tip_a3.is_some());
    assert_eq!(orphan_tip_a3.unwrap().hash(), &a3_hash);

    // check that B3 was removed from orphan chain tips
    let orphan_tip_b3 = store
        .db_read_access()
        .unwrap()
        .fetch_orphan_chain_tip_by_hash(&b3_hash)
        .unwrap();
    assert!(orphan_tip_b3.is_none());

    // Block A4
    let txs = vec![txn_schema!(from: vec![a_outputs[3][0].clone()], to: vec![20 * T])];
    generate_new_block_with_achieved_difficulty(
        &mut a_store,
        &mut a_blocks,
        &mut a_outputs,
        txs,
        Difficulty::from(2), // A chain accumulated difficulty 11
        &consensus_manager,
    )
    .unwrap();

    store.add_block(a_blocks[4].to_arc_block()).unwrap().assert_orphaned();

    // Block A5
    let txs = vec![txn_schema!(from: vec![a_outputs[4][0].clone()], to: vec![10 * T])];
    generate_new_block_with_achieved_difficulty(
        &mut a_store,
        &mut a_blocks,
        &mut a_outputs,
        txs,
        Difficulty::from(4), // A chain accumulated difficulty 15
        &consensus_manager,
    )
    .unwrap();

    store.add_block(a_blocks[5].to_arc_block()).unwrap().assert_reorg(5, 4);

    // check that B4 is now in the orphan chain tip db
    let orphan_tip_b4 = store
        .db_read_access()
        .unwrap()
        .fetch_orphan_chain_tip_by_hash(&b4_hash)
        .unwrap();
    assert!(orphan_tip_b4.is_some());
    assert_eq!(orphan_tip_b4.unwrap().hash(), &b4_hash);

    // check that A3 was removed from orphan chain tips
    let orphan_tip_a3 = store
        .db_read_access()
        .unwrap()
        .fetch_orphan_chain_tip_by_hash(&a3_hash)
        .unwrap();
    assert!(orphan_tip_a3.is_none());

    // Check that B1 - B4 are orphans
    assert!(store.fetch_orphan(*b_blocks[1].hash()).is_ok()); // B1
    assert!(store.fetch_orphan(*b_blocks[2].hash()).is_ok()); // B2
    assert!(store.fetch_orphan(*b_blocks[3].hash()).is_ok()); // B3
    assert!(store.fetch_orphan(*b_blocks[4].hash()).is_ok()); // B4

    // And blocks A1 - A5 are not
    assert!(store.fetch_orphan(*a_blocks[1].hash()).is_err()); // A1
    assert!(store.fetch_orphan(*a_blocks[2].hash()).is_err()); // A2
    assert!(store.fetch_orphan(*a_blocks[3].hash()).is_err()); // A3
    assert!(store.fetch_orphan(*a_blocks[4].hash()).is_err()); // A4
    assert!(store.fetch_orphan(*a_blocks[5].hash()).is_err()); // A5
}

#[test]
fn test_handle_reorg_with_no_removed_blocks() {
    // GB --> A1
    //          \--> B2 (?) --> B3)
    // Initially, the main chain is GB->A1 with orphaned blocks B3. When B2 arrives late and is
    // added to the blockchain then a reorg is triggered and the main chain is reorganized to GB->A1->B2->B3.

    // Create Main Chain
    let network = Network::LocalNet;
    let (mut store, mut blocks, mut outputs, consensus_manager) = create_new_blockchain(network);

    // Block A1
    let txs = vec![txn_schema!(
        from: vec![outputs[0][0].clone()],
        to: vec![10 * T, 10 * T, 10 * T, 10 * T]
    )];
    generate_new_block_with_achieved_difficulty(
        &mut store,
        &mut blocks,
        &mut outputs,
        txs,
        Difficulty::from(1),
        &consensus_manager,
    )
    .unwrap();

    // Create Forked Chain 1
    let mut orphan1_store = create_store_with_consensus(consensus_manager.clone());
    orphan1_store.add_block(blocks[1].to_arc_block()).unwrap(); // A1
    let mut orphan1_blocks = vec![blocks[0].clone(), blocks[1].clone()];
    let mut orphan1_outputs = vec![outputs[0].clone(), outputs[1].clone()];
    // Block B2
    let txs = vec![txn_schema!(from: vec![orphan1_outputs[1][0].clone()], to: vec![5 * T])];
    generate_new_block_with_achieved_difficulty(
        &mut orphan1_store,
        &mut orphan1_blocks,
        &mut orphan1_outputs,
        txs,
        Difficulty::from(1),
        &consensus_manager,
    )
    .unwrap();
    // Block B3
    let txs = vec![
        txn_schema!(from: vec![orphan1_outputs[1][3].clone()], to: vec![3 * T]),
        txn_schema!(from: vec![orphan1_outputs[2][0].clone()], to: vec![3 * T]),
    ];
    generate_new_block_with_achieved_difficulty(
        &mut orphan1_store,
        &mut orphan1_blocks,
        &mut orphan1_outputs,
        txs,
        Difficulty::from(1),
        &consensus_manager,
    )
    .unwrap();

    // Now add the fork blocks B3 and B2 (out of order) to the first DB and ensure a reorg.
    // see https://github.com/tari-project/tari/issues/2101#issuecomment-679188619
    store.add_block(orphan1_blocks[3].to_arc_block()).unwrap(); // B3
    let result = store.add_block(orphan1_blocks[2].to_arc_block()).unwrap(); // B2
    match result {
        BlockAddResult::Ok(_) => panic!("Adding multiple blocks without removing any failed to cause a reorg!"),
        BlockAddResult::ChainReorg { removed, added } => {
            assert_eq!(added.len(), 2);
            assert_eq!(removed.len(), 0);
        },
        _ => panic!(),
    }

    assert_eq!(store.fetch_tip_header().unwrap().header(), orphan1_blocks[3].header());
}

#[test]
fn test_handle_reorg_failure_recovery() {
    // GB --> A1 --> A2 --> A3 -----> A4(Low PoW)     [Main Chain]
    //          \--> B2 --> B3(double spend - rejected by db)  [Forked Chain 1]
    //          \--> B2 --> B3'(validation failed)      [Forked Chain 1]
    // Checks the following cases:
    // 1. recovery from failure to commit reorged blocks, and
    // 2. recovery from failed block validation.

    let block_validator = MockValidator::new(true);
    let validators = Validators::new(block_validator, MockValidator::new(true), MockValidator::new(true));
    // Create Main Chain
    let network = Network::LocalNet;
    let (mut store, mut blocks, mut outputs, consensus_manager) =
        create_new_blockchain_lmdb(network, validators, Default::default());
    // Block A1
    let txs = vec![txn_schema!(
        from: vec![outputs[0][0].clone()],
        to: vec![10 * T, 10 * T, 10 * T, 10 * T]
    )];
    generate_new_block_with_achieved_difficulty(
        &mut store,
        &mut blocks,
        &mut outputs,
        txs,
        Difficulty::from(1),
        &consensus_manager,
    )
    .unwrap();
    // Block A2
    let txs = vec![txn_schema!(from: vec![outputs[1][3].clone()], to: vec![6 * T])];
    generate_new_block_with_achieved_difficulty(
        &mut store,
        &mut blocks,
        &mut outputs,
        txs,
        Difficulty::from(1),
        &consensus_manager,
    )
    .unwrap();
    // Block A3
    let txs = vec![txn_schema!(from: vec![outputs[2][0].clone()], to: vec![2 * T])];
    generate_new_block_with_achieved_difficulty(
        &mut store,
        &mut blocks,
        &mut outputs,
        txs,
        Difficulty::from(2),
        &consensus_manager,
    )
    .unwrap();
    // Block A4
    let txs = vec![txn_schema!(from: vec![outputs[1][0].clone()], to: vec![2 * T])];
    generate_new_block_with_achieved_difficulty(
        &mut store,
        &mut blocks,
        &mut outputs,
        txs,
        Difficulty::from(2),
        &consensus_manager,
    )
    .unwrap();

    // Create Forked Chain 1
    let mut orphan1_store = create_store_with_consensus(consensus_manager.clone());
    orphan1_store.add_block(blocks[1].to_arc_block()).unwrap(); // A1
    let mut orphan1_blocks = vec![blocks[0].clone(), blocks[1].clone()];
    let mut orphan1_outputs = vec![outputs[0].clone(), outputs[1].clone()];
    // Block B2
    let txs = vec![txn_schema!(from: vec![orphan1_outputs[1][0].clone()], to: vec![5 * T])];
    generate_new_block_with_achieved_difficulty(
        &mut orphan1_store,
        &mut orphan1_blocks,
        &mut orphan1_outputs,
        txs,
        Difficulty::from(1),
        &consensus_manager,
    )
    .unwrap();
    // Block B3 (Incorrect height)
    let double_spend_block = {
        let schemas = vec![
            txn_schema!(from: vec![orphan1_outputs[1][3].clone()], to: vec![3 * T]),
            // Double spend
            // txn_schema!(from: vec![orphan1_outputs[1][3].clone()], to: vec![3 * T]),
        ];
        let mut txns = Vec::new();
        let mut block_utxos = Vec::new();
        for schema in schemas {
            let (tx, mut utxos) = spend_utxos(schema);
            txns.push(tx);
            block_utxos.append(&mut utxos);
        }
        orphan1_outputs.push(block_utxos);

        let template = chain_block(orphan1_blocks.last().unwrap().block(), txns, &consensus_manager);
        let mut block = orphan1_store.prepare_new_block(template).unwrap();
        block.header.nonce = OsRng.next_u64();
        block.header.height += 1;
        find_header_with_achieved_difficulty(&mut block.header, Difficulty::from_u64(2).unwrap());
        block
    };

    // Add an orphaned B2
    let result = store.add_block(orphan1_blocks[2].to_arc_block()).unwrap(); // B2
    unpack_enum!(BlockAddResult::OrphanBlock = result);

    // Add invalid block B3. Our database should recover
    let res = store.add_block(double_spend_block.into()).unwrap(); // B3
    unpack_enum!(BlockAddResult::OrphanBlock = res);
    let tip_header = store.fetch_tip_header().unwrap();
    assert_eq!(tip_header.height(), 4);
    assert_eq!(tip_header.header(), blocks[4].header());

    assert!(store.fetch_orphan(*blocks[2].hash()).is_err()); // A2
    assert!(store.fetch_orphan(*blocks[3].hash()).is_err()); // A3
    assert!(store.fetch_orphan(*blocks[4].hash()).is_err()); // A4
}

#[test]
fn test_store_and_retrieve_blocks() {
    let validators = Validators::new(
        MockValidator::new(true),
        MockValidator::new(true),
        MockValidator::new(true),
    );
    let network = Network::LocalNet;
    let rules = ConsensusManagerBuilder::new(network).build();
    let db = create_test_db();
    let store = BlockchainDatabase::new(
        db,
        rules.clone(),
        validators,
        BlockchainDatabaseConfig::default(),
        DifficultyCalculator::new(rules.clone(), Default::default()),
    )
    .unwrap();

    let block0 = store.fetch_block(0, true).unwrap();
    let block1 = append_block(
        &store,
        &block0.clone().try_into_chain_block().unwrap(),
        vec![],
        &rules,
        Difficulty::min(),
    )
    .unwrap();
    let block2 = append_block(&store, &block1, vec![], &rules, Difficulty::min()).unwrap();
    assert_eq!(
        store.fetch_block(0, true).unwrap().try_into_chain_block().unwrap(),
        block0.clone().try_into_chain_block().unwrap()
    );
    assert_eq!(
        store.fetch_block(1, true).unwrap().try_into_chain_block().unwrap(),
        block1
    );
    assert_eq!(
        store.fetch_block(2, true).unwrap().try_into_chain_block().unwrap(),
        block2
    );

    let block3 = append_block(&store, &block2, vec![], &rules, Difficulty::min()).unwrap();
    assert_eq!(
        store.fetch_block(0, true).unwrap().try_into_chain_block().unwrap(),
        block0.try_into_chain_block().unwrap()
    );
    assert_eq!(
        store.fetch_block(1, true).unwrap().try_into_chain_block().unwrap(),
        block1
    );
    assert_eq!(
        store.fetch_block(2, true).unwrap().try_into_chain_block().unwrap(),
        block2
    );
    assert_eq!(
        store.fetch_block(3, true).unwrap().try_into_chain_block().unwrap(),
        block3
    );
}

#[test]
#[allow(clippy::identity_op)]
fn test_store_and_retrieve_blocks_from_contents() {
    let network = Network::LocalNet;
    let (mut db, mut blocks, mut outputs, consensus_manager) = create_new_blockchain(network);

    // Block 1
    let schema = vec![txn_schema!(from: vec![outputs[0][0].clone()], to: vec![6 * T, 3 * T])];
    unpack_enum!(
        BlockAddResult::Ok(_b1) =
            generate_new_block(&mut db, &mut blocks, &mut outputs, schema, &consensus_manager).unwrap()
    );
    // Block 2
    let schema = vec![txn_schema!(from: vec![outputs[1][0].clone()], to: vec![3 * T, 1 * T])];
    unpack_enum!(
        BlockAddResult::Ok(_b2) =
            generate_new_block(&mut db, &mut blocks, &mut outputs, schema, &consensus_manager).unwrap()
    );
    let kernel_sig = blocks[1].block().body.kernels()[0].clone().excess_sig;
    let utxo_commit = blocks.last().unwrap().block().body.outputs()[0].clone().commitment;
    assert_eq!(
        db.fetch_block_with_kernel(kernel_sig)
            .unwrap()
            .unwrap()
            .try_into_chain_block()
            .unwrap(),
        blocks[1]
    );

    assert_eq!(
        db.fetch_block_with_utxo(utxo_commit)
            .unwrap()
            .unwrap()
            .try_into_chain_block()
            .unwrap(),
        blocks[2]
    );
}

#[test]
fn test_restore_metadata_and_pruning_horizon_update() {
    // Perform test
    let validators = Validators::new(
        MockValidator::new(true),
        MockValidator::new(true),
        MockValidator::new(true),
    );
    let network = Network::LocalNet;
    let block0 = genesis_block::get_esmeralda_genesis_block();
    let rules = ConsensusManagerBuilder::new(network).with_block(block0.clone()).build();
    let mut config = BlockchainDatabaseConfig::default();
    let block_hash: BlockHash;
    let temp_path = create_temporary_data_path();
    {
        let mut db = TempDatabase::from_path(&temp_path);
        db.disable_delete_on_drop();
        config.pruning_horizon = 1000;
        let db = BlockchainDatabase::new(
            db,
            rules.clone(),
            validators.clone(),
            config,
            DifficultyCalculator::new(rules.clone(), Default::default()),
        )
        .unwrap();

        let block1 = append_block(&db, &block0, vec![], &rules, Difficulty::min()).unwrap();
        db.add_block(block1.to_arc_block()).unwrap();
        block_hash = *block1.hash();
        let metadata = db.get_chain_metadata().unwrap();
        assert_eq!(metadata.height_of_longest_chain(), 1);
        assert_eq!(metadata.best_block(), &block_hash);
        assert_eq!(metadata.pruning_horizon(), 1000);
    }
    // Restore blockchain db with larger pruning horizon

    {
        config.pruning_horizon = 2000;
        let mut db = TempDatabase::from_path(&temp_path);
        db.disable_delete_on_drop();
        let db = BlockchainDatabase::new(
            db,
            rules.clone(),
            validators.clone(),
            config,
            DifficultyCalculator::new(rules.clone(), Default::default()),
        )
        .unwrap();

        let metadata = db.get_chain_metadata().unwrap();
        assert_eq!(metadata.height_of_longest_chain(), 1);
        assert_eq!(metadata.best_block(), &block_hash);
        assert_eq!(metadata.pruning_horizon(), 2000);
    }
    // Restore blockchain db with smaller pruning horizon update
    {
        config.pruning_horizon = 900;
        let db = TempDatabase::from_path(&temp_path);
        let db = BlockchainDatabase::new(
            db,
            rules.clone(),
            validators,
            config,
            DifficultyCalculator::new(rules, Default::default()),
        )
        .unwrap();

        let metadata = db.get_chain_metadata().unwrap();
        assert_eq!(metadata.height_of_longest_chain(), 1);
        assert_eq!(metadata.best_block(), &block_hash);
        assert_eq!(metadata.pruning_horizon(), 900);
    }
}
static EMISSION: [u64; 2] = [10, 10];
#[test]
fn test_invalid_block() {
    let factories = CryptoFactories::default();
    let network = Network::LocalNet;
    let consensus_constants = ConsensusConstantsBuilder::new(network)
        .with_emission_amounts(100_000_000.into(), &EMISSION, 100.into())
        .build();
    let (block0, output) = create_genesis_block( &consensus_constants);
    let consensus_manager = ConsensusManagerBuilder::new(network)
        .add_consensus_constants(consensus_constants)
        .with_block(block0.clone())
        .build();
    let validator = MockValidator::new(true);
    let is_valid = validator.shared_flag();
    let validators = Validators::new(MockValidator::new(true), MockValidator::new(true), validator);
    let mut store = create_store_with_consensus_and_validators(consensus_manager.clone(), validators);

    let mut blocks = vec![block0];
    let mut outputs = vec![vec![output]];
    let block0_hash = *blocks[0].hash();
    let metadata = store.get_chain_metadata().unwrap();
    assert_eq!(metadata.height_of_longest_chain(), 0);
    assert_eq!(metadata.best_block(), &block0_hash);
    assert_eq!(store.fetch_block(0, true).unwrap().block().hash(), block0_hash);
    assert!(store.fetch_block(1, true).is_err());

    // Block 1
    let txs = vec![txn_schema!(
        from: vec![outputs[0][0].clone()],
        to: vec![10 * T, 5 * T, 10 * T, 15 * T]
    )];
    let coinbase_value = consensus_manager.emission_schedule().block_reward(1);
    unpack_enum!(
        BlockAddResult::Ok(_b1) = generate_new_block_with_coinbase(
            &mut store,
            &factories,
            &mut blocks,
            &mut outputs,
            txs,
            coinbase_value,
            &consensus_manager
        )
        .unwrap()
    );
    let block1_hash = *blocks[1].hash();
    let metadata = store.get_chain_metadata().unwrap();
    assert_eq!(metadata.height_of_longest_chain(), 1);
    assert_eq!(metadata.best_block(), &block1_hash);
    assert_eq!(store.fetch_block(0, true).unwrap().hash(), &block0_hash);
    assert_eq!(store.fetch_block(1, true).unwrap().hash(), &block1_hash);
    assert!(store.fetch_block(2, true).is_err());

    // Invalid Block 2 - Double spends genesis block output
    is_valid.set(false);
    let txs = vec![txn_schema!(from: vec![outputs[0][0].clone()], to: vec![20 * T, 20 * T])];
    let coinbase_value = consensus_manager.emission_schedule().block_reward(2);
    unpack_enum!(
        ChainStorageError::InvalidOperation(_msg) = generate_new_block_with_coinbase(
            &mut store,
            &factories,
            &mut blocks,
            &mut outputs,
            txs,
            coinbase_value,
            &consensus_manager
        )
        .unwrap_err()
    );
    let metadata = store.get_chain_metadata().unwrap();
    assert_eq!(metadata.height_of_longest_chain(), 1);
    assert_eq!(metadata.best_block(), &block1_hash);
    assert_eq!(store.fetch_block(0, true).unwrap().hash(), &block0_hash);
    assert_eq!(store.fetch_block(1, true).unwrap().hash(), &block1_hash);
    assert!(store.fetch_block(2, true).is_err());

    // Valid Block 2
    is_valid.set(true);
    let txs = vec![txn_schema!(from: vec![outputs[1][0].clone()], to: vec![4 * T, 4 * T])];
    let coinbase_value = consensus_manager.emission_schedule().block_reward(2);
    unpack_enum!(
        BlockAddResult::Ok(_b1) = generate_new_block_with_coinbase(
            &mut store,
            &factories,
            &mut blocks,
            &mut outputs,
            txs,
            coinbase_value,
            &consensus_manager
        )
        .unwrap()
    );
    let block2_hash = blocks[2].hash();
    let metadata = store.get_chain_metadata().unwrap();
    assert_eq!(metadata.height_of_longest_chain(), 2);
    assert_eq!(metadata.best_block(), block2_hash);
    assert_eq!(store.fetch_block(0, true).unwrap().hash(), &block0_hash);
    assert_eq!(store.fetch_block(1, true).unwrap().hash(), &block1_hash);
    assert_eq!(store.fetch_block(2, true).unwrap().hash(), block2_hash);
    assert!(store.fetch_block(3, true).is_err());
}

#[test]
fn test_orphan_cleanup_on_block_add() {
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManagerBuilder::new(network).build();
    let validators = Validators::new(
        MockValidator::new(true),
        MockValidator::new(true),
        MockValidator::new(true),
    );
    let db = create_test_db();
    let config = BlockchainDatabaseConfig {
        orphan_storage_capacity: 3,
        pruning_horizon: 0,
        pruning_interval: 50,
        ..Default::default()
    };
    let store = BlockchainDatabase::new(
        db,
        consensus_manager.clone(),
        validators,
        config,
        DifficultyCalculator::new(consensus_manager.clone(), Default::default()),
    )
    .unwrap();

    let orphan1 = create_orphan_block(500, vec![], &consensus_manager);
    let orphan2 = create_orphan_block(5, vec![], &consensus_manager);
    let orphan3 = create_orphan_block(30, vec![], &consensus_manager);
    let orphan4 = create_orphan_block(700, vec![], &consensus_manager);
    let orphan5 = create_orphan_block(43, vec![], &consensus_manager);
    let orphan6 = create_orphan_block(75, vec![], &consensus_manager);
    let orphan7 = create_orphan_block(150, vec![], &consensus_manager);
    let orphan1_hash = orphan1.hash();
    let orphan2_hash = orphan2.hash();
    let orphan3_hash = orphan3.hash();
    let orphan4_hash = orphan4.hash();
    let orphan5_hash = orphan5.hash();
    let orphan6_hash = orphan6.hash();
    let orphan7_hash = orphan7.hash();
    assert_eq!(
        store.add_block(orphan1.clone().into()).unwrap(),
        BlockAddResult::OrphanBlock
    );
    assert_eq!(store.add_block(orphan2.into()).unwrap(), BlockAddResult::OrphanBlock);
    assert_eq!(store.add_block(orphan3.into()).unwrap(), BlockAddResult::OrphanBlock);
    assert_eq!(
        store.add_block(orphan4.clone().into()).unwrap(),
        BlockAddResult::OrphanBlock
    );
    assert_eq!(store.add_block(orphan5.into()).unwrap(), BlockAddResult::OrphanBlock);
    assert_eq!(store.add_block(orphan6.into()).unwrap(), BlockAddResult::OrphanBlock);
    assert_eq!(
        store.add_block(orphan7.clone().into()).unwrap(),
        BlockAddResult::OrphanBlock
    );

    store.cleanup_orphans().unwrap();
    assert_eq!(store.db_read_access().unwrap().orphan_count().unwrap(), 3);
    assert_eq!(store.fetch_orphan(orphan1_hash).unwrap(), orphan1);
    assert!(store.fetch_orphan(orphan2_hash).is_err());
    assert!(store.fetch_orphan(orphan3_hash).is_err());
    assert_eq!(store.fetch_orphan(orphan4_hash).unwrap(), orphan4);
    assert!(store.fetch_orphan(orphan5_hash).is_err());
    assert!(store.fetch_orphan(orphan6_hash).is_err());
    assert_eq!(store.fetch_orphan(orphan7_hash).unwrap(), orphan7);
}

#[test]
fn test_horizon_height_orphan_cleanup() {
    let network = Network::LocalNet;
    let block0 = genesis_block::get_esmeralda_genesis_block();
    let consensus_manager = ConsensusManagerBuilder::new(network).with_block(block0.clone()).build();
    let validators = Validators::new(
        MockValidator::new(true),
        MockValidator::new(true),
        MockValidator::new(true),
    );
    let db = create_test_db();
    let config = BlockchainDatabaseConfig {
        orphan_storage_capacity: 3,
        pruning_horizon: 2,
        pruning_interval: 50,
        ..Default::default()
    };
    let store = BlockchainDatabase::new(
        db,
        consensus_manager.clone(),
        validators,
        config,
        DifficultyCalculator::new(consensus_manager.clone(), Default::default()),
    )
    .unwrap();
    let orphan1 = create_orphan_block(2, vec![], &consensus_manager);
    let orphan2 = create_orphan_block(3, vec![], &consensus_manager);
    let orphan3 = create_orphan_block(1, vec![], &consensus_manager);
    let orphan4 = create_orphan_block(4, vec![], &consensus_manager);
    let orphan1_hash = orphan1.hash();
    let orphan2_hash = orphan2.hash();
    let orphan3_hash = orphan3.hash();
    let orphan4_hash = orphan4.hash();
    assert_eq!(store.add_block(orphan1.into()).unwrap(), BlockAddResult::OrphanBlock);
    assert_eq!(
        store.add_block(orphan2.clone().into()).unwrap(),
        BlockAddResult::OrphanBlock
    );
    assert_eq!(store.add_block(orphan3.into()).unwrap(), BlockAddResult::OrphanBlock);
    assert_eq!(store.db_read_access().unwrap().orphan_count().unwrap(), 3);

    let block1 = append_block(&store, &block0, vec![], &consensus_manager, Difficulty::min()).unwrap();
    let block2 = append_block(&store, &block1, vec![], &consensus_manager, Difficulty::min()).unwrap();
    let block3 = append_block(&store, &block2, vec![], &consensus_manager, Difficulty::min()).unwrap();
    let _block4 = append_block(&store, &block3, vec![], &consensus_manager, Difficulty::min()).unwrap();

    // Adding another orphan block will trigger the orphan cleanup as the storage limit was reached
    assert_eq!(
        store.add_block(orphan4.clone().into()).unwrap(),
        BlockAddResult::OrphanBlock
    );

    assert_eq!(store.db_read_access().unwrap().orphan_count().unwrap(), 2);
    assert!(store.fetch_orphan(orphan1_hash).is_err());
    assert_eq!(store.fetch_orphan(orphan2_hash).unwrap(), orphan2);
    assert!(store.fetch_orphan(orphan3_hash).is_err());
    assert_eq!(store.fetch_orphan(orphan4_hash).unwrap(), orphan4);
}

#[test]
#[allow(clippy::too_many_lines)]
fn test_orphan_cleanup_on_reorg() {
    // Create Main Chain
    let network = Network::LocalNet;
    let factories = CryptoFactories::default();
    let consensus_constants = ConsensusConstantsBuilder::new(network).build();
    let (block0, output) = create_genesis_block( &consensus_constants);
    let consensus_manager = ConsensusManagerBuilder::new(network)
        .add_consensus_constants(consensus_constants)
        .with_block(block0.clone())
        .build();
    let validators = Validators::new(
        MockValidator::new(true),
        MockValidator::new(true),
        MockValidator::new(true),
    );
    let db = create_test_db();
    let config = BlockchainDatabaseConfig {
        orphan_storage_capacity: 3,
        pruning_horizon: 0,
        pruning_interval: 50,
        ..Default::default()
    };
    let mut store = BlockchainDatabase::new(
        db,
        consensus_manager.clone(),
        validators,
        config,
        DifficultyCalculator::new(consensus_manager.clone(), Default::default()),
    )
    .unwrap();
    let mut blocks = vec![block0];
    let mut outputs = vec![vec![output]];

    // Block A1
    generate_new_block_with_achieved_difficulty(
        &mut store,
        &mut blocks,
        &mut outputs,
        vec![],
        Difficulty::from(2),
        &consensus_manager,
    )
    .unwrap();
    // Block A2
    generate_new_block_with_achieved_difficulty(
        &mut store,
        &mut blocks,
        &mut outputs,
        vec![],
        Difficulty::from(3),
        &consensus_manager,
    )
    .unwrap();
    // Block A3
    generate_new_block_with_achieved_difficulty(
        &mut store,
        &mut blocks,
        &mut outputs,
        vec![],
        Difficulty::from(3),
        &consensus_manager,
    )
    .unwrap();
    // Block A4
    generate_new_block_with_achieved_difficulty(
        &mut store,
        &mut blocks,
        &mut outputs,
        vec![],
        Difficulty::from(3),
        &consensus_manager,
    )
    .unwrap();

    // Create Forked Chain
    let mut orphan_store = create_store_with_consensus(consensus_manager.clone());
    let mut orphan_blocks = vec![blocks[0].clone()];
    let mut orphan_outputs = vec![outputs[0].clone()];
    // Block B1
    generate_new_block_with_achieved_difficulty(
        &mut orphan_store,
        &mut orphan_blocks,
        &mut orphan_outputs,
        vec![],
        Difficulty::from(2),
        &consensus_manager,
    )
    .unwrap();
    // Block B2
    generate_new_block_with_achieved_difficulty(
        &mut orphan_store,
        &mut orphan_blocks,
        &mut orphan_outputs,
        vec![],
        Difficulty::from(10),
        &consensus_manager,
    )
    .unwrap();
    // Block B3
    generate_new_block_with_achieved_difficulty(
        &mut orphan_store,
        &mut orphan_blocks,
        &mut orphan_outputs,
        vec![],
        Difficulty::from(15),
        &consensus_manager,
    )
    .unwrap();

    // Fill orphan block pool
    let orphan1 = create_orphan_block(1, vec![], &consensus_manager);
    let orphan2 = create_orphan_block(1, vec![], &consensus_manager);
    assert_eq!(store.add_block(orphan1.into()).unwrap(), BlockAddResult::OrphanBlock);
    assert_eq!(store.add_block(orphan2.into()).unwrap(), BlockAddResult::OrphanBlock);

    // Adding B1 and B2 to the main chain will produce a reorg from GB->A1->A2->A3->A4 to GB->B1->B2->B3.
    assert_eq!(
        store.add_block(orphan_blocks[1].to_arc_block()).unwrap(),
        BlockAddResult::OrphanBlock
    );

    if let Ok(BlockAddResult::ChainReorg { .. }) = store.add_block(orphan_blocks[2].to_arc_block()) {
    } else {
        panic!();
    }

    // Check that A2, A3 and A4 is in the orphan block pool, A1 and the other orphans were discarded by the orphan
    // cleanup.
    store.cleanup_orphans().unwrap();
    assert_eq!(store.db_read_access().unwrap().orphan_count().unwrap(), 3);
    assert_eq!(store.fetch_orphan(*blocks[2].hash()).unwrap(), *blocks[2].block());
    assert_eq!(store.fetch_orphan(*blocks[3].hash()).unwrap(), *blocks[3].block());
    assert_eq!(store.fetch_orphan(*blocks[4].hash()).unwrap(), *blocks[4].block());
}

#[test]
fn test_orphan_cleanup_delete_all_orphans() {
    let path = create_temporary_data_path();
    let network = Network::LocalNet;
    let validators = Validators::new(
        MockValidator::new(true),
        MockValidator::new(true),
        MockValidator::new(true),
    );
    let mut config = BlockchainDatabaseConfig {
        orphan_storage_capacity: 5,
        pruning_horizon: 0,
        pruning_interval: 50,
        ..Default::default()
    };
    // Test cleanup during runtime
    {
        let consensus_manager = ConsensusManager::builder(network).build();
        let db = create_lmdb_database(&path, LMDBConfig::default(), consensus_manager.clone()).unwrap();
        let store = BlockchainDatabase::new(
            db,
            consensus_manager.clone(),
            validators.clone(),
            config,
            DifficultyCalculator::new(consensus_manager.clone(), Default::default()),
        )
        .unwrap();

        let orphan1 = create_orphan_block(500, vec![], &consensus_manager);
        let orphan2 = create_orphan_block(5, vec![], &consensus_manager);
        let orphan3 = create_orphan_block(30, vec![], &consensus_manager);
        let orphan4 = create_orphan_block(700, vec![], &consensus_manager);
        let orphan5 = create_orphan_block(43, vec![], &consensus_manager);

        // Add orphans and verify
        assert_eq!(
            store.add_block(orphan1.clone().into()).unwrap(),
            BlockAddResult::OrphanBlock
        );
        assert_eq!(
            store.add_block(orphan2.clone().into()).unwrap(),
            BlockAddResult::OrphanBlock
        );
        assert_eq!(
            store.add_block(orphan3.clone().into()).unwrap(),
            BlockAddResult::OrphanBlock
        );
        assert_eq!(
            store.add_block(orphan4.clone().into()).unwrap(),
            BlockAddResult::OrphanBlock
        );
        assert_eq!(
            store.add_block(orphan5.clone().into()).unwrap(),
            BlockAddResult::OrphanBlock
        );
        assert_eq!(store.db_read_access().unwrap().orphan_count().unwrap(), 5);

        // Cleanup orphans and verify
        assert!(store.cleanup_all_orphans().is_ok());
        assert_eq!(store.db_read_access().unwrap().orphan_count().unwrap(), 0);

        // Add orphans again
        assert_eq!(store.add_block(orphan1.into()).unwrap(), BlockAddResult::OrphanBlock);
        assert_eq!(store.add_block(orphan2.into()).unwrap(), BlockAddResult::OrphanBlock);
        assert_eq!(store.add_block(orphan3.into()).unwrap(), BlockAddResult::OrphanBlock);
        assert_eq!(store.add_block(orphan4.into()).unwrap(), BlockAddResult::OrphanBlock);
        assert_eq!(store.add_block(orphan5.into()).unwrap(), BlockAddResult::OrphanBlock);
    }

    // Test orphans are present on open
    {
        let consensus_manager = ConsensusManager::builder(Network::LocalNet).build();
        let db = create_lmdb_database(&path, LMDBConfig::default(), consensus_manager.clone()).unwrap();
        let store = BlockchainDatabase::new(
            db,
            consensus_manager.clone(),
            validators.clone(),
            config,
            DifficultyCalculator::new(consensus_manager, Default::default()),
        )
        .unwrap();
        assert_eq!(store.db_read_access().unwrap().orphan_count().unwrap(), 5);
    }

    // Test orphans cleanup on open
    {
        let consensus_manager = ConsensusManager::builder(Network::LocalNet).build();
        let db = create_lmdb_database(&path, LMDBConfig::default(), consensus_manager.clone()).unwrap();
        config.cleanup_orphans_at_startup = true;
        let store = BlockchainDatabase::new(
            db,
            consensus_manager.clone(),
            validators,
            config,
            DifficultyCalculator::new(consensus_manager, Default::default()),
        )
        .unwrap();
        assert_eq!(store.db_read_access().unwrap().orphan_count().unwrap(), 0);
    }

    if std::path::Path::new(&path).exists() {
        std::fs::remove_dir_all(&path).expect("Could not clean up directory")
    }
}

#[test]
fn test_fails_validation() {
    let network = Network::LocalNet;
    let factories = CryptoFactories::default();
    let consensus_constants = ConsensusConstantsBuilder::new(network).build();
    let (block0, output) = create_genesis_block(&factories, &consensus_constants);
    let consensus_manager = ConsensusManagerBuilder::new(network)
        .add_consensus_constants(consensus_constants)
        .with_block(block0.clone())
        .build();
    let validators = Validators::new(
        MockValidator::new(false),
        MockValidator::new(true),
        MockValidator::new(true),
    );
    let db = create_test_db();
    let config = BlockchainDatabaseConfig {
        orphan_storage_capacity: 3,
        pruning_horizon: 0,
        pruning_interval: 50,
        ..Default::default()
    };
    let mut store = BlockchainDatabase::new(
        db,
        consensus_manager.clone(),
        validators,
        config,
        DifficultyCalculator::new(consensus_manager.clone(), Default::default()),
    )
    .unwrap();
    let mut blocks = vec![block0];
    let mut outputs = vec![vec![]];

    let schemas = vec![txn_schema!(from: vec![output], to: vec![2 * T, 500_000 * uT])];
    let err = generate_new_block_with_achieved_difficulty(
        &mut store,
        &mut blocks,
        &mut outputs,
        schemas,
        Difficulty::from(3),
        &consensus_manager,
    )
    .unwrap_err();
    unpack_enum!(ChainStorageError::ValidationError { source } = err);
    unpack_enum!(ValidationError::CustomError(_s) = source);

    let metadata = store.get_chain_metadata().unwrap();
    assert_eq!(metadata.height_of_longest_chain(), 0);
}

#[test]
fn pruned_mode_cleanup_and_fetch_block() {
    let network = Network::LocalNet;
    let block0 = genesis_block::get_esmeralda_genesis_block();
    let consensus_manager = ConsensusManagerBuilder::new(network).with_block(block0.clone()).build();
    let validators = Validators::new(
        MockValidator::new(true),
        MockValidator::new(true),
        MockValidator::new(true),
    );
    let db = create_test_db();
    let config = BlockchainDatabaseConfig {
        orphan_storage_capacity: 3,
        pruning_horizon: 3,
        pruning_interval: 1,
        ..Default::default()
    };
    let store = BlockchainDatabase::new(
        db,
        consensus_manager.clone(),
        validators,
        config,
        DifficultyCalculator::new(consensus_manager.clone(), Default::default()),
    )
    .unwrap();
    let block1 = append_block(&store, &block0, vec![], &consensus_manager, Difficulty::min()).unwrap();
    let block2 = append_block(&store, &block1, vec![], &consensus_manager, Difficulty::min()).unwrap();
    let block3 = append_block(&store, &block2, vec![], &consensus_manager, Difficulty::min()).unwrap();

    let metadata = store.get_chain_metadata().unwrap();
    assert_eq!(metadata.pruned_height(), 0);

    let block4 = append_block(&store, &block3, vec![], &consensus_manager, Difficulty::min()).unwrap();
    let _block5 = append_block(&store, &block4, vec![], &consensus_manager, Difficulty::min()).unwrap();

    let metadata = store.get_chain_metadata().unwrap();
    assert_eq!(metadata.pruned_height(), 2);
    assert_eq!(metadata.height_of_longest_chain(), 5);
    assert_eq!(metadata.pruning_horizon(), 3);
}

mod malleability {
    use tari_common_types::types::{ComAndPubSignature, RangeProof};
    use tari_core::{
        blocks::Block,
        covenant,
        transactions::{test_helpers::generate_keys, transaction_components::TransactionOutputVersion},
    };
    use tari_script::{Opcode, TariScript};
    use tari_utilities::hex::Hex;

    use crate::helpers::block_malleability::*;

    mod input {
        use tari_core::transactions::transaction_components::TransactionInputVersion;
        use tari_script::StackItem;

        use super::*;

        // This test hightlights that the "version" field is not being included in the input hash
        // so a consensus change is needed for the input to include it
        #[test]
        fn test_version() {
            check_input_malleability(|block: &mut Block| {
                let input = &mut block.body.inputs_mut()[0];
                let mod_version = match input.version {
                    TransactionInputVersion::V0 => TransactionInputVersion::V1,
                    _ => TransactionInputVersion::V0,
                };
                input.version = mod_version;
            });
        }

        #[test]
        fn test_spent_output() {
            check_input_malleability(|block: &mut Block| {
                // to modify the spent output, we will substitue it for a copy of a different output
                // we will use one of the outputs of the current transaction
                // because of how the test blockchain is created, they will never be equal
                let output = &block.body.outputs()[0].clone();
                let input = &mut block.body.inputs_mut()[0];
                input.add_output_data(
                    output.version,
                    output.features.clone(),
                    output.commitment.clone(),
                    output.script.clone(),
                    output.sender_offset_public_key.clone(),
                    output.covenant.clone(),
                    output.encrypted_data,
                    output.minimum_value_promise,
                );
            });
        }

        #[test]
        fn test_input_data() {
            check_input_malleability(|block: &mut Block| {
                block.body.inputs_mut()[0]
                    .input_data
                    .push(StackItem::Hash(*b"I can't do whatever I want......"))
                    .unwrap();
            });
        }

        #[test]
        fn test_script_signature() {
            check_input_malleability(|block: &mut Block| {
                let input = &mut block.body.inputs_mut()[0];
                input.script_signature = ComAndPubSignature::default();
            });
        }
    }

    mod output {
        use super::*;

        #[test]
        fn test_version() {
            check_output_malleability(|block: &mut Block| {
                let output = &mut block.body.outputs_mut()[0];
                let mod_version = match output.version {
                    TransactionOutputVersion::V0 => TransactionOutputVersion::V1,
                    _ => TransactionOutputVersion::V0,
                };
                output.version = mod_version;
            });
        }

        #[test]
        fn test_features() {
            check_output_malleability(|block: &mut Block| {
                let output = &mut block.body.outputs_mut()[0];
                output.features.maturity += 1;
            });
        }

        #[test]
        fn test_commitment() {
            check_output_malleability(|block: &mut Block| {
                let output = &mut block.body.outputs_mut()[0];
                let mod_commitment = &output.commitment + &output.commitment;
                output.commitment = mod_commitment;
            });
        }

        #[test]
        fn test_proof() {
            check_output_malleability(|block: &mut Block| {
                let output = &mut block.body.outputs_mut()[0];
                let mod_proof = RangeProof::from_hex(&(output.proof.as_ref().unwrap().to_hex() + "00")).unwrap();
                output.proof = Some(mod_proof);
            });
        }

        #[test]
        fn test_script() {
            check_output_malleability(|block: &mut Block| {
                let output = &mut block.body.outputs_mut()[0];
                let mut script_bytes = output.script.to_bytes();
                Opcode::PushZero.to_bytes(&mut script_bytes);
                let mod_script = TariScript::from_bytes(&script_bytes).unwrap();
                output.script = mod_script;
            });
        }

        // This test hightlights that the "sender_offset_public_key" field is not being included in the output hash
        // so a consensus change is needed for the output to include it
        #[test]
        fn test_sender_offset_public_key() {
            check_output_malleability(|block: &mut Block| {
                let output = &mut block.body.outputs_mut()[0];

                // "gerate_keys" should return a random, different key than the present one
                let mod_pk = generate_keys().pk;
                output.sender_offset_public_key = mod_pk;
            });
        }

        #[test]
        fn test_metadata_signature() {
            check_output_malleability(|block: &mut Block| {
                let output = &mut block.body.outputs_mut()[0];
                output.metadata_signature = ComAndPubSignature::default();
            });
        }

        #[test]
        fn test_covenant() {
            check_output_malleability(|block: &mut Block| {
                let output = &mut block.body.outputs_mut()[0];
                let mod_covenant = covenant!(absolute_height(@uint(42)));
                output.covenant = mod_covenant;
            });
        }
    }

    mod kernel {
        use tari_common_types::types::Signature;
        use tari_core::transactions::tari_amount::MicroMinotari;

        use super::*;

        // the "version" field only has one value (V0) so malleability test is not possible for it
        // the "features" field has only a constant value at the moment, so no malleability test possible

        #[test]
        fn test_fee() {
            check_kernel_malleability(|block: &mut Block| {
                let kernel = &mut block.body.kernels_mut()[0];
                kernel.fee += MicroMinotari::from(1);
            });
        }

        #[test]
        fn test_lock_height() {
            check_kernel_malleability(|block: &mut Block| {
                let kernel = &mut block.body.kernels_mut()[0];
                kernel.lock_height += 1;
            });
        }

        #[test]
        fn test_excess() {
            check_kernel_malleability(|block: &mut Block| {
                let kernel = &mut block.body.kernels_mut()[0];
                let mod_excess = &kernel.excess + &kernel.excess;
                kernel.excess = mod_excess;
            });
        }

        #[test]
        fn test_excess_sig() {
            check_kernel_malleability(|block: &mut Block| {
                let kernel = &mut block.body.kernels_mut()[0];
                // "gerate_keys" should return a group of random keys, different from the ones in the field
                let keys = generate_keys();
                kernel.excess_sig = Signature::new(keys.pk, keys.k);
            });
        }
    }
}

#[allow(clippy::identity_op)]
#[test]
fn test_fetch_deleted_position_block_hash() {
    // Create Main Chain
    let network = Network::LocalNet;
    let (mut store, mut blocks, mut outputs, consensus_manager) = create_new_blockchain(network);
    // Block 1
    let txs = vec![txn_schema!(
        from: vec![outputs[0][0].clone()],
        to: vec![11 * T, 12 * T, 13 * T, 14 * T]
    )];
    generate_new_block_with_achieved_difficulty(
        &mut store,
        &mut blocks,
        &mut outputs,
        txs,
        Difficulty::from(1),
        &consensus_manager,
    )
    .unwrap()
    .assert_added();
    // Block 2
    let txs = vec![txn_schema!(from: vec![outputs[1][3].clone()], to: vec![6 * T])];
    generate_new_block_with_achieved_difficulty(
        &mut store,
        &mut blocks,
        &mut outputs,
        txs,
        Difficulty::from(3),
        &consensus_manager,
    )
    .unwrap()
    .assert_added();
    // Blocks 3 - 12 so we can test the search in the bottom and top half
    for i in 0..10 {
        generate_new_block_with_achieved_difficulty(
            &mut store,
            &mut blocks,
            &mut outputs,
            vec![],
            Difficulty::from(4 + i),
            &consensus_manager,
        )
        .unwrap()
        .assert_added();
    }
    // Block 13
    let txs = vec![txn_schema!(from: vec![outputs[2][0].clone()], to: vec![2 * T])];
    generate_new_block_with_achieved_difficulty(
        &mut store,
        &mut blocks,
        &mut outputs,
        txs,
        Difficulty::from(30),
        &consensus_manager,
    )
    .unwrap()
    .assert_added();
    // Block 14
    let txs = vec![txn_schema!(from: vec![outputs[13][0].clone()], to: vec![1 * T])];
    generate_new_block_with_achieved_difficulty(
        &mut store,
        &mut blocks,
        &mut outputs,
        txs,
        Difficulty::from(50),
        &consensus_manager,
    )
    .unwrap()
    .assert_added();

    let block1_hash = store.fetch_header(1).unwrap().unwrap().hash();
    let block2_hash = store.fetch_header(2).unwrap().unwrap().hash();
    let block13_hash = store.fetch_header(13).unwrap().unwrap().hash();
    let block14_hash = store.fetch_header(14).unwrap().unwrap().hash();

    let deleted_positions = store
        .fetch_complete_deleted_bitmap_at(block14_hash)
        .unwrap()
        .bitmap()
        .to_vec();

    let headers = store
        .fetch_header_hash_by_deleted_mmr_positions(deleted_positions)
        .unwrap();
    let mut headers = headers.into_iter().map(Option::unwrap).collect::<Vec<_>>();
    headers.sort_by(|(a, _), (b, _)| a.cmp(b));

    assert_eq!(headers[3], (14, block14_hash));
    assert_eq!(headers[2], (13, block13_hash));
    assert_eq!(headers[1], (2, block2_hash));
    assert_eq!(headers[0], (1, block1_hash));
}
