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

use tari_core::{
    base_node::{state_machine_service::states::StateEvent, sync::HeaderSyncStatus},
    chain_storage::BlockchainDatabaseConfig,
};

use crate::helpers::{sync, sync::WhatToDelete};

#[allow(clippy::too_many_lines)]
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_header_sync_happy_path() {
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

    // Add 1 block to Bob's chain
    let (bob_blocks, bob_coinbases) = sync::create_and_add_some_blocks(
        &bob_node,
        &initial_block,
        &initial_coinbase,
        1,
        &consensus_manager,
        &key_manager,
        &[3],
        &None,
    )
    .await;
    assert_eq!(bob_node.blockchain_db.get_height().unwrap(), 1);

    // Alice attempts header sync, still on the genesys block, headers will be lagging
    let mut header_sync = sync::initialize_sync_headers_with_ping_pong_data(&alice_node, &bob_node);
    let event = sync::sync_headers_execute(&mut alice_state_machine, &mut header_sync).await;
    // "Lagging"
    match event.clone() {
        StateEvent::HeadersSynchronized(_val, sync_result) => {
            assert_eq!(sync_result.headers_returned, 1);
            assert_eq!(sync_result.peer_fork_hash_index, 0);
            if let HeaderSyncStatus::Lagging(val) = sync_result.header_sync_status {
                assert_eq!(val.best_block_header.height(), 0);
                assert_eq!(val.reorg_steps_back, 0);
            } else {
                panic!("Should be 'Lagging'");
            }
        },
        _ => panic!("Expected HeadersSynchronized event"),
    }

    // Alice attempts header sync again, still on the genesys block, headers will be in sync
    let mut header_sync = sync::initialize_sync_headers_with_ping_pong_data(&alice_node, &bob_node);
    let event = sync::sync_headers_execute(&mut alice_state_machine, &mut header_sync).await;
    // "InSyncOrAhead"
    match event.clone() {
        StateEvent::HeadersSynchronized(_val, sync_result) => {
            assert_eq!(sync_result.headers_returned, 0);
            assert_eq!(sync_result.peer_fork_hash_index, 0);
            if let HeaderSyncStatus::InSyncOrAhead = sync_result.header_sync_status {
                // Good, headers were in sync
            } else {
                panic!("Should be 'InSyncOrAhead'");
            }
        },
        _ => panic!("Expected StateEvent::HeadersSynchronized event"),
    }

    // Bob adds another block
    let (_blocks, _coinbases) = sync::create_and_add_some_blocks(
        &bob_node,
        &bob_blocks[1],
        &bob_coinbases[1],
        1,
        &consensus_manager,
        &key_manager,
        &[3],
        &None,
    )
    .await;
    assert_eq!(bob_node.blockchain_db.get_height().unwrap(), 2);

    // Alice attempts header sync, still on the genesys block, headers will be lagging
    let mut header_sync = sync::initialize_sync_headers_with_ping_pong_data(&alice_node, &bob_node);
    let event = sync::sync_headers_execute(&mut alice_state_machine, &mut header_sync).await;
    // "Lagging"
    match event {
        StateEvent::HeadersSynchronized(_val, sync_result) => {
            assert_eq!(sync_result.headers_returned, 1);
            assert_eq!(sync_result.peer_fork_hash_index, 0);
            if let HeaderSyncStatus::Lagging(val) = sync_result.header_sync_status {
                assert_eq!(val.best_block_header.height(), 0);
                assert_eq!(val.reorg_steps_back, 0);
            } else {
                panic!("Should be 'Lagging'");
            }
        },
        _ => panic!("Expected StateEvent::HeadersSynchronized event"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_header_sync_with_fork_happy_path() {
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

    // Add 1 block to Bob's chain
    let (bob_blocks, bob_coinbases) = sync::create_and_add_some_blocks(
        &bob_node,
        &initial_block,
        &initial_coinbase,
        1,
        &consensus_manager,
        &key_manager,
        &[3],
        &None,
    )
    .await;
    assert_eq!(bob_node.blockchain_db.get_height().unwrap(), 1);

    // Bob adds another block
    let (bob_blocks, bob_coinbases) = sync::create_and_add_some_blocks(
        &bob_node,
        &bob_blocks[1],
        &bob_coinbases[1],
        1,
        &consensus_manager,
        &key_manager,
        &[3],
        &None,
    )
    .await;
    assert_eq!(bob_node.blockchain_db.get_height().unwrap(), 2);

    // Alice adds 3 (different) blocks, with POW on par with Bob's chain, but with greater height
    let _alice_blocks = sync::create_and_add_some_blocks(
        &alice_node,
        &initial_block,
        &initial_coinbase,
        3,
        &consensus_manager,
        &key_manager,
        &[3, 2, 1],
        &None,
    )
    .await;
    assert_eq!(alice_node.blockchain_db.get_height().unwrap(), 3);
    assert_eq!(
        alice_node
            .blockchain_db
            .get_chain_metadata()
            .unwrap()
            .accumulated_difficulty(),
        bob_node
            .blockchain_db
            .get_chain_metadata()
            .unwrap()
            .accumulated_difficulty()
    );

    // Alice attempts header sync, but POW is on par
    let mut header_sync = sync::initialize_sync_headers_with_ping_pong_data(&alice_node, &bob_node);
    let event = sync::sync_headers_execute(&mut alice_state_machine, &mut header_sync).await;
    match event.clone() {
        StateEvent::Continue => {
            // Good - Header sync not attempted, sync peer does not have better POW
        },
        _ => panic!("Expected StateEvent::Continue event"),
    }
    // All is good, Bob is not banned
    assert!(!sync::wait_for_is_peer_banned(&alice_node, bob_node.node_identity.node_id(), 1).await);

    // Bob adds more blocks and draws ahead of Alice
    let _blocks = sync::create_and_add_some_blocks(
        &bob_node,
        &bob_blocks[1],
        &bob_coinbases[1],
        2,
        &consensus_manager,
        &key_manager,
        &[3; 2],
        &None,
    )
    .await;
    assert_eq!(bob_node.blockchain_db.get_height().unwrap(), 4);

    // Alice attempts header sync to Bob's chain with higher POW, headers will be lagging with reorg steps
    let mut header_sync = sync::initialize_sync_headers_with_ping_pong_data(&alice_node, &bob_node);
    let event = sync::sync_headers_execute(&mut alice_state_machine, &mut header_sync).await;
    // "Lagging"
    match event {
        StateEvent::HeadersSynchronized(_val, sync_result) => {
            assert_eq!(sync_result.headers_returned, 4);
            assert_eq!(sync_result.peer_fork_hash_index, 3);
            if let HeaderSyncStatus::Lagging(val) = sync_result.header_sync_status {
                assert_eq!(val.best_block_header.height(), 3);
                assert_eq!(val.reorg_steps_back, 3);
            } else {
                panic!("Should be 'Lagging'");
            }
        },
        _ => panic!("Expected StateEvent::HeadersSynchronized event"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_header_sync_uneven_headers_and_blocks_happy_path() {
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

    // Add blocks and headers to Bob's chain, with more headers than blocks
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
    sync::delete_some_blocks_and_headers(&blocks[5..=10], WhatToDelete::Blocks, &bob_node);
    sync::delete_some_blocks_and_headers(&blocks[7..=10], WhatToDelete::Headers, &bob_node);
    assert_eq!(bob_node.blockchain_db.get_height().unwrap(), 5);
    assert_eq!(bob_node.blockchain_db.fetch_last_header().unwrap().height, 7);

    // Add blocks and headers to Alice's chain, with more headers than blocks
    sync::add_some_existing_blocks(&blocks[1..=10], &alice_node);
    sync::delete_some_blocks_and_headers(&blocks[2..=10], WhatToDelete::Blocks, &alice_node);
    assert_eq!(alice_node.blockchain_db.get_height().unwrap(), 2);
    assert_eq!(alice_node.blockchain_db.fetch_last_header().unwrap().height, 10);

    // Alice attempts header sync, but her headers are ahead
    let mut header_sync = sync::initialize_sync_headers_with_ping_pong_data(&alice_node, &bob_node);
    let event = sync::sync_headers_execute(&mut alice_state_machine, &mut header_sync).await;
    match event {
        StateEvent::HeadersSynchronized(_val, sync_result) => {
            assert_eq!(sync_result.headers_returned, 0);
            assert_eq!(sync_result.peer_fork_hash_index, 3);
            if let HeaderSyncStatus::InSyncOrAhead = sync_result.header_sync_status {
                // Good, headers were in sync
            } else {
                panic!("Should be 'InSyncOrAhead'");
            }
        },
        _ => panic!("Expected HeadersSynchronized event"),
    }
    // All is good, Bob is not banned
    assert!(!sync::wait_for_is_peer_banned(&alice_node, bob_node.node_identity.node_id(), 1).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_header_sync_uneven_headers_and_blocks_peer_lies_about_pow_no_ban() {
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

    // Add blocks and headers to Bob's chain, with more headers than blocks
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
    sync::delete_some_blocks_and_headers(&blocks[5..=10], WhatToDelete::Blocks, &bob_node);
    sync::delete_some_blocks_and_headers(&blocks[7..=10], WhatToDelete::Headers, &bob_node);
    assert_eq!(bob_node.blockchain_db.get_height().unwrap(), 5);
    assert_eq!(bob_node.blockchain_db.fetch_last_header().unwrap().height, 7);

    // Add blocks and headers to Alice's chain, with more headers than blocks
    sync::add_some_existing_blocks(&blocks[1..=10], &alice_node);
    sync::delete_some_blocks_and_headers(&blocks[2..=10], WhatToDelete::Blocks, &alice_node);
    assert_eq!(alice_node.blockchain_db.get_height().unwrap(), 2);
    assert_eq!(alice_node.blockchain_db.fetch_last_header().unwrap().height, 10);

    // Alice attempts header sync, her headers are ahead, but Bob will lie about his POW
    // Note: This behaviour is undetected!
    let mut header_sync = sync::initialize_sync_headers_with_ping_pong_data(&alice_node, &bob_node);
    // Remove blocks from Bpb's chain so his claimed metadata is better than what it actually is
    sync::delete_some_blocks_and_headers(&blocks[4..=5], WhatToDelete::Blocks, &bob_node);
    assert_eq!(bob_node.blockchain_db.get_height().unwrap(), 4);
    assert_eq!(bob_node.blockchain_db.fetch_last_header().unwrap().height, 7);
    assert!(
        header_sync.clone().into_sync_peers()[0]
            .claimed_chain_metadata()
            .accumulated_difficulty() >
            bob_node
                .blockchain_db
                .get_chain_metadata()
                .unwrap()
                .accumulated_difficulty()
    );
    let event = sync::sync_headers_execute(&mut alice_state_machine, &mut header_sync).await;
    match event {
        StateEvent::HeadersSynchronized(_val, sync_result) => {
            assert_eq!(sync_result.headers_returned, 0);
            assert_eq!(sync_result.peer_fork_hash_index, 3);
            if let HeaderSyncStatus::InSyncOrAhead = sync_result.header_sync_status {
                // Note: This behaviour is undetected! Bob cannot be banned here, and should be banned by block sync.
            } else {
                panic!("Should be 'InSyncOrAhead'");
            }
        },
        _ => panic!("Expected HeadersSynchronized event"),
    }
    // Bob will not be banned (but should be banned by block sync!)
    assert!(!sync::wait_for_is_peer_banned(&alice_node, bob_node.node_identity.node_id(), 1).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_header_sync_even_headers_and_blocks_peer_lies_about_pow_with_ban() {
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

    // Add blocks and headers to Bob's chain
    let (blocks, _coinbases) = sync::create_and_add_some_blocks(
        &bob_node,
        &initial_block,
        &initial_coinbase,
        6,
        &consensus_manager,
        &key_manager,
        &[3; 6],
        &None,
    )
    .await;
    assert_eq!(bob_node.blockchain_db.get_height().unwrap(), 6);
    assert_eq!(bob_node.blockchain_db.fetch_last_header().unwrap().height, 6);

    // Add blocks and headers to Alice's chain (less than Bob's)
    sync::add_some_existing_blocks(&blocks[1..=5], &alice_node);
    assert_eq!(alice_node.blockchain_db.get_height().unwrap(), 5);
    assert_eq!(alice_node.blockchain_db.fetch_last_header().unwrap().height, 5);

    // Alice attempts header sync, but Bob will not supply any blocks
    let mut header_sync = sync::initialize_sync_headers_with_ping_pong_data(&alice_node, &bob_node);
    // Remove blocks and headers from Bpb's chain so his claimed metadata is better than what it actually is
    sync::delete_some_blocks_and_headers(&blocks[3..=6], WhatToDelete::BlocksAndHeaders, &bob_node);
    assert_eq!(bob_node.blockchain_db.get_height().unwrap(), 3);
    assert_eq!(bob_node.blockchain_db.fetch_last_header().unwrap().height, 3);
    assert!(
        header_sync.clone().into_sync_peers()[0]
            .claimed_chain_metadata()
            .accumulated_difficulty() >
            bob_node
                .blockchain_db
                .get_chain_metadata()
                .unwrap()
                .accumulated_difficulty()
    );
    let event = sync::sync_headers_execute(&mut alice_state_machine, &mut header_sync).await;
    match event {
        StateEvent::HeaderSyncFailed(err) => {
            assert_eq!(&err, "No more sync peers available: Header sync failed");
        },
        _ => panic!("Expected HeaderSyncFailed event"),
    }
    // Bob will be banned
    assert!(sync::wait_for_is_peer_banned(&alice_node, bob_node.node_identity.node_id(), 1).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_header_sync_even_headers_and_blocks_peer_metadata_improve_with_reorg() {
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

    // Add blocks and headers to Bob's chain
    let (blocks, coinbases) = sync::create_and_add_some_blocks(
        &bob_node,
        &initial_block,
        &initial_coinbase,
        6,
        &consensus_manager,
        &key_manager,
        &[3; 6],
        &None,
    )
    .await;
    assert_eq!(bob_node.blockchain_db.get_height().unwrap(), 6);
    assert_eq!(bob_node.blockchain_db.fetch_last_header().unwrap().height, 6);

    // Add blocks and headers to Alice's chain (less than Bob's)
    sync::add_some_existing_blocks(&blocks[1..=5], &alice_node);
    assert_eq!(alice_node.blockchain_db.get_height().unwrap(), 5);
    assert_eq!(alice_node.blockchain_db.fetch_last_header().unwrap().height, 5);

    // Alice attempts header sync, but Bob's ping-pong data will be outdated when header sync is executed
    let mut header_sync = sync::initialize_sync_headers_with_ping_pong_data(&alice_node, &bob_node);
    // Bob's chain will reorg with improved metadata
    sync::delete_some_blocks_and_headers(&blocks[4..=6], WhatToDelete::Blocks, &bob_node);
    let _blocks = sync::create_and_add_some_blocks(
        &bob_node,
        &blocks[4],
        &coinbases[4],
        3,
        &consensus_manager,
        &key_manager,
        &[3; 3],
        &None,
    )
    .await;
    assert_eq!(bob_node.blockchain_db.get_height().unwrap(), 7);
    assert_eq!(bob_node.blockchain_db.fetch_last_header().unwrap().height, 7);
    let event = sync::sync_headers_execute(&mut alice_state_machine, &mut header_sync).await;
    match event {
        StateEvent::HeadersSynchronized(_val, sync_result) => {
            assert_eq!(sync_result.headers_returned, 3);
            assert_eq!(sync_result.peer_fork_hash_index, 1);
            if let HeaderSyncStatus::Lagging(val) = sync_result.header_sync_status {
                assert_eq!(val.best_block_header.height(), 5);
                assert_eq!(val.reorg_steps_back, 1);
            } else {
                panic!("Should be 'Lagging'");
            }
        },
        _ => panic!("Expected HeadersSynchronized event"),
    }
    // Bob will not be banned
    assert!(!sync::wait_for_is_peer_banned(&alice_node, bob_node.node_identity.node_id(), 1).await);
}
