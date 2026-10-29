//  Copyright 2022. The Tari Project
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

#![allow(clippy::indexing_slicing)]
use std::sync::Arc;

use tari_common::configuration::Network;
use tari_core::{
    base_node::state_machine_service::states::{BlockSync, StateEvent},
    chain_storage::{BlockchainDatabaseConfig, DbTransaction},
};
use tari_node_components::blocks::{Block, ChainBlock};
use tari_transaction_components::{
    aggregated_body::AggregateBody,
    tari_amount::T,
    test_helpers::schema_to_transaction,
    txn_schema,
};
use tempfile::tempdir;

use crate::helpers::{
    block_builders::create_coinbase,
    nodes::{BaseNodeBuilder, random_node_identity, wait_until_online},
    sync,
    sync::{WhatToDelete, state_event},
};

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_block_sync_happy_path() {
    // env_logger::builder().filter_level(log::LevelFilter::Trace).init();  //  > ./target/output.log 2>&1

    // Create the network with Alice node and Bob node
    let (mut state_machines, mut peer_nodes, initial_block, consensus_manager, key_manager, initial_coinbase) =
        sync::create_network_with_multiple_nodes(vec![
            BlockchainDatabaseConfig::default(),
            BlockchainDatabaseConfig::default(),
        ])
        .await;
    let mut alice_state_machine = state_machines.remove(0);
    let alice_node = peer_nodes.remove(0);
    let bob_node = peer_nodes.remove(0);

    // Add some block to Bob's chain
    let (_blocks, _coinbases) = sync::create_and_add_some_blocks(
        &bob_node,
        &initial_block,
        &initial_coinbase,
        5,
        &consensus_manager,
        &key_manager,
        &[3; 5],
        &None,
    );
    assert_eq!(bob_node.blockchain_db.get_height().unwrap(), 5);

    // Alice attempts header sync
    let mut header_sync = sync::initialize_sync_headers_with_ping_pong_data(&alice_node, &bob_node);
    let event = sync::sync_headers_execute(&mut alice_state_machine, &mut header_sync).await;
    match event.clone() {
        StateEvent::HeadersSynchronized(..) => {
            // Good, headers are synced
        },
        _ => panic!("Expected HeadersSynchronized event"),
    }

    // Alice attempts block sync
    println!();
    assert_eq!(alice_node.blockchain_db.get_height().unwrap(), 0);
    let mut block_sync = sync::initialize_sync_blocks(&bob_node);
    let event = sync::sync_blocks_execute(&mut alice_state_machine, &mut block_sync).await;
    match event {
        StateEvent::BlocksSynchronized => {
            // Good, blocks are synced
        },
        _ => panic!("Expected BlocksSynchronized event"),
    }
    assert_eq!(alice_node.blockchain_db.get_height().unwrap(), 5);

    // Alice attempts block sync again
    println!();
    let mut block_sync = sync::initialize_sync_blocks(&bob_node);
    let event = sync::sync_blocks_execute(&mut alice_state_machine, &mut block_sync).await;
    match event {
        StateEvent::BlocksSynchronized => {
            // Good, blocks are synced
        },
        _ => panic!("Expected BlocksSynchronized event"),
    }
    assert_eq!(alice_node.blockchain_db.get_height().unwrap(), 5);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_block_sync_peer_supplies_no_blocks_with_ban() {
    // env_logger::init(); // Set `$env:RUST_LOG = "trace"`

    // Create the network with Alice node and Bob node
    let (mut state_machines, mut peer_nodes, initial_block, consensus_manager, key_manager, initial_coinbase) =
        sync::create_network_with_multiple_nodes(vec![
            BlockchainDatabaseConfig::default(),
            BlockchainDatabaseConfig::default(),
        ])
        .await;
    let mut alice_state_machine = state_machines.remove(0);
    let alice_node = peer_nodes.remove(0);
    let bob_node = peer_nodes.remove(0);

    // Add some block to Bob's chain
    let (blocks, _coinbases) = sync::create_and_add_some_blocks(
        &bob_node,
        &initial_block,
        &initial_coinbase,
        10,
        &consensus_manager,
        &key_manager,
        &[3; 10],
        &None,
    );
    assert_eq!(bob_node.blockchain_db.get_height().unwrap(), 10);
    // Add blocks to Alice's chain
    sync::add_some_existing_blocks(&blocks[1..=5], &alice_node);
    assert_eq!(alice_node.blockchain_db.get_height().unwrap(), 5);

    // Alice attempts header sync
    let mut header_sync = sync::initialize_sync_headers_with_ping_pong_data(&alice_node, &bob_node);
    let event = sync::sync_headers_execute(&mut alice_state_machine, &mut header_sync).await;
    match event.clone() {
        StateEvent::HeadersSynchronized(..) => {
            // Good, headers are synced
        },
        _ => panic!("Expected HeadersSynchronized event"),
    }

    // Alice attempts block sync, Bob will not send any blocks and be banned
    println!();
    let mut block_sync = sync::initialize_sync_blocks(&bob_node);
    sync::delete_some_blocks_and_headers(&blocks[5..=10], WhatToDelete::Blocks, &bob_node);
    assert_eq!(bob_node.blockchain_db.get_height().unwrap(), 5);
    let event = sync::sync_blocks_execute(&mut alice_state_machine, &mut block_sync).await;
    match event {
        StateEvent::BlockSyncFailed => {
            // Good, Bob is banned.
        },
        _ => panic!("Expected BlockSyncFailed event"),
    }
    assert_eq!(alice_node.blockchain_db.get_height().unwrap(), 5);

    // Bob will be banned
    assert!(sync::wait_for_is_peer_banned(&alice_node, bob_node.node_identity.node_id(), 1).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_block_sync_peer_supplies_not_all_blocks_with_ban() {
    // env_logger::init(); // Set `$env:RUST_LOG = "trace"`

    // Create the network with Alice node and Bob node
    let (mut state_machines, mut peer_nodes, initial_block, consensus_manager, key_manager, initial_coinbase) =
        sync::create_network_with_multiple_nodes(vec![
            BlockchainDatabaseConfig::default(),
            BlockchainDatabaseConfig::default(),
        ])
        .await;
    let mut alice_state_machine = state_machines.remove(0);
    let alice_node = peer_nodes.remove(0);
    let bob_node = peer_nodes.remove(0);

    // Add some block to Bob's chain
    let (blocks, _coinbases) = sync::create_and_add_some_blocks(
        &bob_node,
        &initial_block,
        &initial_coinbase,
        10,
        &consensus_manager,
        &key_manager,
        &[3; 10],
        &None,
    );
    assert_eq!(bob_node.blockchain_db.get_height().unwrap(), 10);
    // Add blocks to Alice's chain
    sync::add_some_existing_blocks(&blocks[1..=5], &alice_node);
    assert_eq!(alice_node.blockchain_db.get_height().unwrap(), 5);

    // Alice attempts header sync
    let mut header_sync = sync::initialize_sync_headers_with_ping_pong_data(&alice_node, &bob_node);
    let event = sync::sync_headers_execute(&mut alice_state_machine, &mut header_sync).await;
    match event.clone() {
        StateEvent::HeadersSynchronized(..) => {
            // Good, headers are synced
        },
        _ => panic!("Expected HeadersSynchronized event"),
    }

    // Alice attempts block sync, Bob will not send all blocks and be banned
    println!();
    let mut block_sync = sync::initialize_sync_blocks(&bob_node);
    sync::delete_some_blocks_and_headers(&blocks[8..=10], WhatToDelete::Blocks, &bob_node);
    assert_eq!(bob_node.blockchain_db.get_height().unwrap(), 8);
    let event = sync::sync_blocks_execute(&mut alice_state_machine, &mut block_sync).await;
    match event {
        StateEvent::BlockSyncFailed => {
            // Good, Bob is banned.
        },
        _ => panic!("Expected BlockSyncFailed event"),
    }
    assert_eq!(alice_node.blockchain_db.get_height().unwrap(), 5);

    // Bob will be banned
    assert!(sync::wait_for_is_peer_banned(&alice_node, bob_node.node_identity.node_id(), 1).await);
}

#[allow(clippy::too_many_lines)]
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_block_sync_with_conbase_spend_happy_path_1() {
    //` cargo test --release --test core_integration_tests
    //` tests::horizon_sync::test_block_sync_with_conbase_spend_happy_path_1 > .\target\output.txt 2>&1
    // env_logger::init(); // Set `$env:RUST_LOG = "trace"`

    // Create the network with Bob (archival node) and Carol (archival node)
    let (mut state_machines, mut peer_nodes, initial_block, consensus_manager, key_manager, initial_coinbase) =
        sync::create_network_with_multiple_nodes(vec![
            // Carol is an archival node
            BlockchainDatabaseConfig::default(),
            // Bob is an archival node
            BlockchainDatabaseConfig::default(),
        ])
        .await;
    let mut carol_state_machine = state_machines.remove(0);
    let carol_node = peer_nodes.remove(0);
    let bob_node = peer_nodes.remove(0);

    // Create a blockchain that spends the genesys coinbase early on and then later spends some more coinbase outputs
    let follow_up_coinbases_to_spend = 4;
    let (blocks, _coinbases) = sync::create_block_chain_with_transactions(
        &bob_node,
        &initial_block,
        &initial_coinbase,
        &consensus_manager,
        &key_manager,
        3,
        10,                           // > follow_up_transaction_in_block + intermediate_height + 1
        2,                            // < intermediate_height,
        5,                            // > intermediate_height
        follow_up_coinbases_to_spend, // > spend_genesis_coinbase_in_block - 1, < follow_up_transaction_in_block
    );

    // Now rewind Bob's chain to height 1 (> pruning_horizon, < follow_up_transaction_in_block)
    sync::delete_some_blocks_and_headers(&blocks[1..=10], WhatToDelete::BlocksAndHeaders, &bob_node);
    assert_eq!(bob_node.blockchain_db.get_height().unwrap(), 1);
    assert_eq!(bob_node.blockchain_db.fetch_last_header().unwrap().height, 1);
    println!(
        "\nBob's blockchain height: {}\n",
        bob_node.blockchain_db.get_height().unwrap()
    );

    // 1. Carol attempts header sync sync from Bob
    println!("\n1. Carol attempts header sync sync from Bob\n");

    let mut header_sync_carol_from_bob = sync::initialize_sync_headers_with_ping_pong_data(&carol_node, &bob_node);
    let event = sync::sync_headers_execute(&mut carol_state_machine, &mut header_sync_carol_from_bob).await;
    let carol_header_height = carol_node.blockchain_db.fetch_last_header().unwrap().height;
    println!("Event: {} to header {}", state_event(&event), carol_header_height);
    assert_eq!(carol_header_height, 1);

    // 2. Carol attempts block sync from Bob to the tip (to height 1)
    println!("\n2. Carol attempts block sync from Bob to the tip (to height 1)\n");

    let mut block_sync = sync::initialize_sync_blocks(&bob_node);
    let event = sync::sync_blocks_execute(&mut carol_state_machine, &mut block_sync).await;
    println!(
        "Event: {} to block {}",
        state_event(&event),
        carol_node.blockchain_db.get_height().unwrap()
    );
    assert_eq!(event, StateEvent::BlocksSynchronized);
    assert_eq!(
        carol_node.blockchain_db.get_height().unwrap(),
        carol_node.blockchain_db.fetch_last_header().unwrap().height
    );
    // Bob will not be banned
    assert!(!sync::wait_for_is_peer_banned(&carol_node, bob_node.node_identity.node_id(), 1).await);

    // Give Bob some more blocks
    sync::add_some_existing_blocks(&blocks[2..=2], &bob_node);
    assert_eq!(bob_node.blockchain_db.get_height().unwrap(), 2);
    assert_eq!(bob_node.blockchain_db.fetch_last_header().unwrap().height, 2);
    println!(
        "\nBob's blockchain height: {}\n",
        bob_node.blockchain_db.get_height().unwrap()
    );

    // 3. Carol attempts header sync sync from Bob
    println!("\n3. Carol attempts header sync sync from Bob\n");

    let mut header_sync_carol_from_bob = sync::initialize_sync_headers_with_ping_pong_data(&carol_node, &bob_node);
    let event = sync::sync_headers_execute(&mut carol_state_machine, &mut header_sync_carol_from_bob).await;
    let carol_header_height = carol_node.blockchain_db.fetch_last_header().unwrap().height;
    println!("Event: {} to header {}", state_event(&event), carol_header_height);
    assert_eq!(carol_header_height, 2);

    // 4. Carol attempts block sync from Bob to the tip (to height 2)
    println!("\n4. Carol attempts block sync from Bob to the tip (to height 2)\n");

    let mut block_sync = sync::initialize_sync_blocks(&bob_node);
    let event = sync::sync_blocks_execute(&mut carol_state_machine, &mut block_sync).await;
    println!(
        "Event: {} to block {}",
        state_event(&event),
        carol_node.blockchain_db.get_height().unwrap()
    );
    assert_eq!(event, StateEvent::BlocksSynchronized);
    assert_eq!(
        carol_node.blockchain_db.get_height().unwrap(),
        carol_node.blockchain_db.fetch_last_header().unwrap().height
    );
    // Bob will not be banned
    assert!(!sync::wait_for_is_peer_banned(&carol_node, bob_node.node_identity.node_id(), 1).await);
}

#[allow(clippy::too_many_lines)]
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_block_sync_with_conbase_spend_happy_path_2() {
    //` cargo test --release --test core_integration_tests
    //` tests::horizon_sync::test_block_sync_with_conbase_spend_happy_path_2 > .\target\output.txt 2>&1
    // env_logger::init(); // Set `$env:RUST_LOG = "trace"`

    // Create the network with Bob (archival node) and Carol (archival node)
    let (mut state_machines, mut peer_nodes, initial_block, consensus_manager, key_manager, initial_coinbase) =
        sync::create_network_with_multiple_nodes(vec![
            // Carol is an archival node
            BlockchainDatabaseConfig::default(),
            // Bob is an archival node
            BlockchainDatabaseConfig::default(),
        ])
        .await;
    let mut carol_state_machine = state_machines.remove(0);
    let carol_node = peer_nodes.remove(0);
    let bob_node = peer_nodes.remove(0);

    // Create a blockchain that spends the genesys coinbase early on and then later spends some more coinbase outputs
    let follow_up_coinbases_to_spend = 4;
    let (_blocks, _coinbases) = sync::create_block_chain_with_transactions(
        &bob_node,
        &initial_block,
        &initial_coinbase,
        &consensus_manager,
        &key_manager,
        3,
        10,                           // > follow_up_transaction_in_block + intermediate_height + 1
        2,                            // < intermediate_height,
        5,                            // > intermediate_height
        follow_up_coinbases_to_spend, // > spend_genesis_coinbase_in_block - 1, < follow_up_transaction_in_block
    );

    // 1. Carol attempts header sync sync from Bob
    println!("\n1. Carol attempts header sync sync from Bob\n");

    let mut header_sync_carol_from_bob = sync::initialize_sync_headers_with_ping_pong_data(&carol_node, &bob_node);
    let event = sync::sync_headers_execute(&mut carol_state_machine, &mut header_sync_carol_from_bob).await;
    let carol_header_height = carol_node.blockchain_db.fetch_last_header().unwrap().height;
    println!("Event: {} to header {}", state_event(&event), carol_header_height);
    assert_eq!(carol_header_height, 10);

    // 2. Carol attempts block sync from Bob to the tip (to height 10)
    println!("\n2. Carol attempts block sync from Bob to the tip (to height 10)\n");

    let mut block_sync = sync::initialize_sync_blocks(&bob_node);
    let event = sync::sync_blocks_execute(&mut carol_state_machine, &mut block_sync).await;
    println!(
        "Event: {} to block {}",
        state_event(&event),
        carol_node.blockchain_db.get_height().unwrap()
    );
    assert_eq!(event, StateEvent::BlocksSynchronized);
    assert_eq!(
        carol_node.blockchain_db.get_height().unwrap(),
        carol_node.blockchain_db.fetch_last_header().unwrap().height
    );
    // Bob will not be banned
    assert!(!sync::wait_for_is_peer_banned(&carol_node, bob_node.node_identity.node_id(), 1).await);
}

/// A peer serving a body that is not the one the header commits to is banned, and the honest header is not marked as
/// a bad block.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_block_sync_peer_supplies_a_body_that_does_not_match_the_header() {
    // Create the network with Alice node and Bob node
    let (mut state_machines, mut peer_nodes, initial_block, consensus_manager, key_manager, initial_coinbase) =
        sync::create_network_with_multiple_nodes(vec![
            BlockchainDatabaseConfig::default(),
            BlockchainDatabaseConfig::default(),
        ])
        .await;
    let mut alice_state_machine = state_machines.remove(0);
    let alice_node = peer_nodes.remove(0);
    let bob_node = peer_nodes.remove(0);

    let (blocks, _coinbases) = sync::create_and_add_some_blocks(
        &bob_node,
        &initial_block,
        &initial_coinbase,
        5,
        &consensus_manager,
        &key_manager,
        &[3; 5],
        &None,
    );

    // Alice syncs the honest headers
    let mut header_sync = sync::initialize_sync_headers_with_ping_pong_data(&alice_node, &bob_node);
    let event = sync::sync_headers_execute(&mut alice_state_machine, &mut header_sync).await;
    assert!(matches!(event, StateEvent::HeadersSynchronized(..)), "{event:?}");

    // Bob replaces the kernel of his tip with another one, keeping the header and the outputs
    let tip = &blocks[5];
    sync::delete_some_blocks_and_headers(&blocks[4..=5], WhatToDelete::Blocks, &bob_node);
    let (_, other_kernel, _) = create_coinbase(
        consensus_manager.get_block_reward_at(tip.height()),
        tip.height() + consensus_manager.consensus_constants(0).coinbase_min_maturity(),
        None,
        &key_manager,
    );
    let fake = Block::new(
        tip.header().clone(),
        AggregateBody::new_sorted_unchecked(vec![], tip.block().body.outputs().clone(), vec![other_kernel]),
    );
    let fake = ChainBlock::try_construct(Arc::new(fake), tip.accumulated_data().clone()).unwrap();
    let mut txn = DbTransaction::new();
    txn.insert_tip_block_body(Arc::new(fake));
    txn.set_best_block(
        tip.height(),
        *tip.hash(),
        tip.accumulated_data().total_accumulated_difficulty,
        *blocks[4].hash(),
        tip.to_chain_header().timestamp(),
    );
    bob_node.blockchain_db.write(txn).unwrap();
    assert_eq!(bob_node.blockchain_db.get_height().unwrap(), 5);

    // Alice syncs the honest bodies, then rejects the fake one
    let mut block_sync = sync::initialize_sync_blocks(&bob_node);
    let event = sync::sync_blocks_execute(&mut alice_state_machine, &mut block_sync).await;
    assert!(matches!(event, StateEvent::BlockSyncFailed), "{event:?}");
    assert_eq!(alice_node.blockchain_db.get_height().unwrap(), 4);
    assert!(!alice_node.blockchain_db.bad_block_exists(*tip.hash()).unwrap().0);
    assert!(sync::wait_for_is_peer_banned(&alice_node, bob_node.node_identity.node_id(), 1).await);
}

/// A peer serving the honest bodies with the coinbase moved is banned, and the honest header is not marked as a bad
/// block: the MMR roots do not commit to where the coinbase sits among the other outputs, so only the sorting shows
/// that the body is not the one that was mined.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_block_sync_peer_supplies_the_body_with_its_coinbase_moved() {
    let (mut state_machines, mut peer_nodes, initial_block, consensus_manager, key_manager, initial_coinbase) =
        sync::create_network_with_multiple_nodes(vec![
            BlockchainDatabaseConfig::default(),
            BlockchainDatabaseConfig::default(),
        ])
        .await;
    let mut alice_state_machine = state_machines.remove(0);
    let alice_node = peer_nodes.remove(0);
    let bob_node = peer_nodes.remove(0);

    // Bob's tip spends the genesis coinbase, so it has outputs other than its coinbase
    let schema = txn_schema!(from: vec![initial_coinbase.clone()], to: vec![1 * T; 5]);
    let (txs, _) = schema_to_transaction(&[schema], &key_manager);
    let mut txs_per_block = vec![vec![]; 5];
    txs_per_block[4] = txs.into_iter().map(|t| Arc::try_unwrap(t).unwrap()).collect();
    let (blocks, _coinbases) = sync::create_and_add_some_blocks(
        &bob_node,
        &initial_block,
        &initial_coinbase,
        5,
        &consensus_manager,
        &key_manager,
        &[3; 5],
        &Some(txs_per_block),
    );
    let tip = &blocks[5];
    assert!(tip.block().body.outputs().len() > 1);

    // Mallory has Bob's chain and serves its bodies with the coinbase moved
    let (mallory_node, _) = BaseNodeBuilder::new(Network::LocalNet.into())
        .with_node_identity(random_node_identity())
        .with_peers(vec![alice_node.node_identity.clone()])
        .with_consensus_manager(consensus_manager.clone())
        .with_coinbase_moved_in_synced_bodies()
        .start(
            tempdir().unwrap().keep().to_str().unwrap(),
            BlockchainDatabaseConfig::default(),
        )
        .await;
    wait_until_online(&[&mallory_node]).await;
    sync::add_some_existing_blocks(&blocks[1..=5], &mallory_node);
    assert_eq!(mallory_node.blockchain_db.get_height().unwrap(), 5);

    // Alice syncs the honest headers from Bob, then the bodies from Mallory
    let mut header_sync = sync::initialize_sync_headers_with_ping_pong_data(&alice_node, &bob_node);
    let event = sync::sync_headers_execute(&mut alice_state_machine, &mut header_sync).await;
    assert!(matches!(event, StateEvent::HeadersSynchronized(..)), "{event:?}");
    let mut block_sync = sync::initialize_sync_blocks(&mallory_node);
    let event = sync::sync_blocks_execute(&mut alice_state_machine, &mut block_sync).await;
    assert!(matches!(event, StateEvent::BlockSyncFailed), "{event:?}");
    assert_eq!(alice_node.blockchain_db.get_height().unwrap(), 4);
    assert!(!alice_node.blockchain_db.bad_block_exists(*tip.hash()).unwrap().0);
    let mallory_id = mallory_node.node_identity.node_id();
    assert!(sync::wait_for_is_peer_banned(&alice_node, mallory_id, 1).await);
    let mallory = alice_node
        .comms
        .peer_manager()
        .find_by_node_id(mallory_id)
        .await
        .unwrap()
        .unwrap();
    assert!(
        mallory.banned_reason.contains("Mismatched block body MMR roots"),
        "{}",
        mallory.banned_reason
    );

    // Bob serves the honest body
    let mut block_sync = sync::initialize_sync_blocks(&bob_node);
    let event = sync::sync_blocks_execute(&mut alice_state_machine, &mut block_sync).await;
    assert_eq!(event, StateEvent::BlocksSynchronized);
    assert_eq!(alice_node.blockchain_db.get_height().unwrap(), 5);
}

/// Alice's header tip is on a fork that came from Xavier. Bob claims a different tip and answers NotFound for the
/// end hash; he is not to blame, so he is skipped without a ban and Xavier supplies the blocks.
#[allow(clippy::too_many_lines)]
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_block_sync_end_hash_not_found_from_other_peer_no_ban() {
    let (mut state_machines, mut peer_nodes, initial_block, consensus_manager, key_manager, initial_coinbase) =
        sync::create_network_with_multiple_nodes(vec![
            BlockchainDatabaseConfig::default(),
            BlockchainDatabaseConfig::default(),
            BlockchainDatabaseConfig::default(),
        ])
        .await;
    let mut alice_state_machine = state_machines.remove(0);
    let alice_node = peer_nodes.remove(0);
    let xavier_node = peer_nodes.remove(0);
    let bob_node = peer_nodes.remove(0);

    let (blocks, coinbases) = sync::create_and_add_some_blocks(
        &bob_node,
        &initial_block,
        &initial_coinbase,
        5,
        &consensus_manager,
        &key_manager,
        &[3; 5],
        &None,
    );
    sync::add_some_existing_blocks(&blocks[1..=2], &alice_node);
    sync::add_some_existing_blocks(&blocks[1..=2], &xavier_node);
    let (xavier_blocks, _coinbases) = sync::create_and_add_some_blocks(
        &xavier_node,
        &blocks[2],
        &coinbases[2],
        4,
        &consensus_manager,
        &key_manager,
        &[3; 4],
        &None,
    );
    assert_eq!(xavier_node.blockchain_db.get_height().unwrap(), 6);

    // Alice takes her header tip from Xavier
    let mut header_sync = sync::initialize_sync_headers_with_ping_pong_data(&alice_node, &xavier_node);
    let event = sync::sync_headers_execute(&mut alice_state_machine, &mut header_sync).await;
    assert!(matches!(event, StateEvent::HeadersSynchronized(..)), "{event:?}");
    assert_eq!(
        alice_node.blockchain_db.fetch_last_header().unwrap().hash(),
        *xavier_blocks[4].hash()
    );

    // Bob is tried first and does not have Xavier's tip
    let bob_peer = sync::sync_peer_from(&bob_node);
    assert_ne!(
        bob_peer.claimed_chain_metadata().best_block_hash(),
        xavier_blocks[4].hash()
    );
    let mut block_sync = BlockSync::from(vec![bob_peer, sync::sync_peer_from(&xavier_node)]);
    let event = sync::sync_blocks_execute(&mut alice_state_machine, &mut block_sync).await;
    assert!(matches!(event, StateEvent::BlocksSynchronized), "{event:?}");
    assert_eq!(alice_node.blockchain_db.get_height().unwrap(), 6);
    assert!(!sync::wait_for_is_peer_banned(&alice_node, bob_node.node_identity.node_id(), 1).await);
    assert!(!sync::wait_for_is_peer_banned(&alice_node, xavier_node.node_identity.node_id(), 1).await);
}

/// Alice's best block is on Xavier's fork. Bob does not have it and answers NotFound for the start hash; he is not
/// to blame, so he is skipped without a ban and Xavier supplies the blocks.
#[allow(clippy::too_many_lines)]
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_block_sync_start_hash_not_found_from_other_peer_no_ban() {
    let (mut state_machines, mut peer_nodes, initial_block, consensus_manager, key_manager, initial_coinbase) =
        sync::create_network_with_multiple_nodes(vec![
            BlockchainDatabaseConfig::default(),
            BlockchainDatabaseConfig::default(),
            BlockchainDatabaseConfig::default(),
        ])
        .await;
    let mut alice_state_machine = state_machines.remove(0);
    let alice_node = peer_nodes.remove(0);
    let xavier_node = peer_nodes.remove(0);
    let bob_node = peer_nodes.remove(0);

    let (blocks, coinbases) = sync::create_and_add_some_blocks(
        &bob_node,
        &initial_block,
        &initial_coinbase,
        5,
        &consensus_manager,
        &key_manager,
        &[3; 5],
        &None,
    );
    sync::add_some_existing_blocks(&blocks[1..=2], &xavier_node);
    let (xavier_blocks, _coinbases) = sync::create_and_add_some_blocks(
        &xavier_node,
        &blocks[2],
        &coinbases[2],
        4,
        &consensus_manager,
        &key_manager,
        &[3; 4],
        &None,
    );
    // Alice holds Xavier's chain with blocks up to 4 and headers up to 6
    sync::add_some_existing_blocks(&blocks[1..=2], &alice_node);
    sync::add_some_existing_blocks(&xavier_blocks[1..=4], &alice_node);
    sync::delete_some_blocks_and_headers(&xavier_blocks[2..=4], WhatToDelete::Blocks, &alice_node);
    assert_eq!(alice_node.blockchain_db.get_height().unwrap(), 4);
    assert_eq!(
        alice_node.blockchain_db.fetch_last_header().unwrap().hash(),
        *xavier_blocks[4].hash()
    );
    assert!(
        bob_node
            .blockchain_db
            .fetch_header_by_block_hash(*xavier_blocks[2].hash())
            .unwrap()
            .is_none()
    );

    let mut block_sync = BlockSync::from(vec![
        sync::sync_peer_from(&bob_node),
        sync::sync_peer_from(&xavier_node),
    ]);
    let event = sync::sync_blocks_execute(&mut alice_state_machine, &mut block_sync).await;
    assert!(matches!(event, StateEvent::BlocksSynchronized), "{event:?}");
    assert_eq!(alice_node.blockchain_db.get_height().unwrap(), 6);
    assert!(!sync::wait_for_is_peer_banned(&alice_node, bob_node.node_identity.node_id(), 1).await);
}

/// Alice took her header tip from Xavier, who serves headers past his own block tip. His claimed best block is not
/// that tip, so his NotFound is not held against him: he is skipped without a ban.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_block_sync_header_source_with_stale_claim_not_found_no_ban() {
    let (mut state_machines, mut peer_nodes, initial_block, consensus_manager, key_manager, initial_coinbase) =
        sync::create_network_with_multiple_nodes(vec![
            BlockchainDatabaseConfig::default(),
            BlockchainDatabaseConfig::default(),
        ])
        .await;
    let mut alice_state_machine = state_machines.remove(0);
    let alice_node = peer_nodes.remove(0);
    let xavier_node = peer_nodes.remove(0);

    let (blocks, _coinbases) = sync::create_and_add_some_blocks(
        &xavier_node,
        &initial_block,
        &initial_coinbase,
        6,
        &consensus_manager,
        &key_manager,
        &[3; 6],
        &None,
    );
    sync::add_some_existing_blocks(&blocks[1..=2], &alice_node);

    let mut header_sync = sync::initialize_sync_headers_with_ping_pong_data(&alice_node, &xavier_node);
    let event = sync::sync_headers_execute(&mut alice_state_machine, &mut header_sync).await;
    assert!(matches!(event, StateEvent::HeadersSynchronized(..)), "{event:?}");
    assert_eq!(
        alice_node.blockchain_db.fetch_last_header().unwrap().hash(),
        *blocks[6].hash()
    );

    // Xavier keeps his headers but only has blocks up to 4
    sync::delete_some_blocks_and_headers(&blocks[4..=6], WhatToDelete::Blocks, &xavier_node);
    let xavier_peer = sync::sync_peer_from(&xavier_node);
    assert_ne!(xavier_peer.claimed_chain_metadata().best_block_hash(), blocks[6].hash());

    let mut block_sync = BlockSync::from(vec![xavier_peer]);
    let event = sync::sync_blocks_execute(&mut alice_state_machine, &mut block_sync).await;
    assert!(matches!(event, StateEvent::BlockSyncFailed), "{event:?}");
    assert_eq!(alice_node.blockchain_db.get_height().unwrap(), 2);
    assert!(!sync::wait_for_is_peer_banned(&alice_node, xavier_node.node_identity.node_id(), 1).await);
}
