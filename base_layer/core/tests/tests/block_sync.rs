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

use tari_core::base_node::state_machine_service::states::StateEvent;

use crate::helpers::{sync, sync::WhatToDelete};

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_block_sync_happy_path() {
    // env_logger::init(); // Set `$env:RUST_LOG = "trace"`

    // Create the network with Alice node and Bob node
    let (mut alice_state_machine, alice_node, bob_node, initial_block, consensus_manager, key_manager) =
        sync::create_network_with_local_and_peer_nodes().await;

    // Add some block to Bob's chain
    let _bob_blocks =
        sync::create_and_add_some_blocks(&bob_node, &initial_block, 5, &consensus_manager, &key_manager, &[3; 5]).await;
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
    let (mut alice_state_machine, alice_node, bob_node, initial_block, consensus_manager, key_manager) =
        sync::create_network_with_local_and_peer_nodes().await;

    // Add some block to Bob's chain
    let blocks = sync::create_and_add_some_blocks(
        &bob_node,
        &initial_block,
        10,
        &consensus_manager,
        &key_manager,
        &[3; 10],
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
    let (mut alice_state_machine, alice_node, bob_node, initial_block, consensus_manager, key_manager) =
        sync::create_network_with_local_and_peer_nodes().await;

    // Add some block to Bob's chain
    let blocks = sync::create_and_add_some_blocks(
        &bob_node,
        &initial_block,
        10,
        &consensus_manager,
        &key_manager,
        &[3; 10],
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
