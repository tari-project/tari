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

use tari_core::{base_node::state_machine_service::states::StateEvent, chain_storage::BlockchainDatabaseConfig};

use crate::helpers::{
    sync,
    sync::{state_event, WhatToDelete},
};

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_block_sync_happy_path() {
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
    let (_blocks, _coinbases) = sync::create_and_add_some_blocks(
        &bob_node,
        &initial_block,
        &initial_coinbase,
        5,
        &consensus_manager,
        &key_manager,
        &[3; 5],
        &None,
    )
    .await;
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
    )
    .await;
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
    )
    .await;
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
    )
    .await;

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
    )
    .await;

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
