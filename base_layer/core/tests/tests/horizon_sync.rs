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

use std::cmp::min;

use tari_core::{
    base_node::state_machine_service::states::{HorizonStateSync, StateEvent},
    chain_storage::BlockchainDatabaseConfig,
};

use crate::helpers::{
    sync,
    sync::{decide_horizon_sync, state_event, WhatToDelete},
};

#[allow(clippy::too_many_lines)]
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_initial_horizon_sync_from_archival_node_happy_path() {
    //` cargo test --release --test core_integration_tests
    //` tests::horizon_sync::test_initial_horizon_sync_from_archival_node_happy_path > .\target\output.txt 2>&1
    // env_logger::init(); // Set `$env:RUST_LOG = "trace"`

    // Create the network with Alice (pruning node) and Bob (archival node)
    let pruning_horizon = 5;
    let (mut state_machines, mut peer_nodes, initial_block, consensus_manager, key_manager, initial_coinbase) =
        sync::create_network_with_multiple_nodes(vec![
            BlockchainDatabaseConfig {
                orphan_storage_capacity: 5,
                pruning_horizon,
                pruning_interval: 5,
                track_reorgs: false,
                cleanup_orphans_at_startup: false,
            },
            BlockchainDatabaseConfig::default(),
        ])
        .await;
    let mut alice_state_machine = state_machines.remove(0);
    let alice_node = peer_nodes.remove(0);
    let bob_node = peer_nodes.remove(0);

    // Create a blockchain that spends the genesys coinbase early on and then later spends some more coinbase outputs
    let follow_up_coinbases_to_spend = 15;
    let (blocks, coinbases) = sync::create_block_chain_with_transactions(
        &bob_node,
        &initial_block,
        &initial_coinbase,
        &consensus_manager,
        &key_manager,
        pruning_horizon,
        30,                           // > follow_up_transaction_in_block + pruning_horizon + 1
        3,                            // < pruning_horizon
        16,                           // > pruning_horizon
        follow_up_coinbases_to_spend, // > spend_genesis_coinbase_in_block - 1, < follow_up_transaction_in_block
    )
    .await;

    // Now rewind Bob's chain to height 10 (> pruning_horizon, < follow_up_transaction_in_block)
    sync::delete_some_blocks_and_headers(&blocks[10..=30], WhatToDelete::BlocksAndHeaders, &bob_node);
    assert_eq!(bob_node.blockchain_db.get_height().unwrap(), 10);
    assert_eq!(bob_node.blockchain_db.fetch_last_header().unwrap().height, 10);

    // 1. Alice attempts horizon sync without having done header sync
    println!("\n1. Alice attempts horizon sync without having done header sync\n");

    let mut horizon_sync = sync::initialize_horizon_sync_without_header_sync(&bob_node);
    let event = sync::horizon_sync_execute(&mut alice_state_machine, &mut horizon_sync).await;

    println!(
        "Event: {} to block {}",
        state_event(&event),
        alice_node.blockchain_db.get_height().unwrap()
    );
    assert_eq!(event, StateEvent::HorizonStateSynchronized);
    assert_eq!(alice_node.blockchain_db.get_height().unwrap(), 0);
    // Bob will not be banned
    assert!(!sync::wait_for_is_peer_banned(&alice_node, bob_node.node_identity.node_id(), 1).await);

    // 2. Alice does header sync (to height 10)
    println!("\n2. Alice does header sync (to height 10)\n");

    let mut header_sync = sync::initialize_sync_headers_with_ping_pong_data(&alice_node, &bob_node);
    let _event = sync::sync_headers_execute(&mut alice_state_machine, &mut header_sync).await;
    assert_eq!(alice_node.blockchain_db.fetch_last_header().unwrap().height, 10);
    // Bob will not be banned
    assert!(!sync::wait_for_is_peer_banned(&alice_node, bob_node.node_identity.node_id(), 1).await);

    // 3. Alice attempts horizon sync after header sync (to height 5; includes genesys block UTXO spend)
    println!("\n3. Alice attempts horizon sync after header sync (to height 5; includes genesys block UTXO spend)\n");
    let output_hash = initial_coinbase.hash(&key_manager).await.unwrap();
    assert!(alice_node.blockchain_db.fetch_output(output_hash).unwrap().is_some());
    let commitment = initial_coinbase.commitment(&key_manager).await.unwrap();
    assert!(alice_node
        .blockchain_db
        .fetch_unspent_output_hash_by_commitment(commitment.clone())
        .unwrap()
        .is_some());

    let event = decide_horizon_sync(&mut alice_state_machine, header_sync.clone()).await;
    let mut horizon_sync = match event {
        StateEvent::ProceedToHorizonSync(sync_peers) => HorizonStateSync::from(sync_peers),
        _ => panic!("3. Alice should proceed to horizon sync"),
    };
    let event = sync::horizon_sync_execute(&mut alice_state_machine, &mut horizon_sync).await;

    println!(
        "Event: {} to block {}",
        state_event(&event),
        alice_node.blockchain_db.get_height().unwrap()
    );
    assert_eq!(event, StateEvent::HorizonStateSynchronized);
    assert_eq!(
        alice_node.blockchain_db.get_height().unwrap(),
        alice_node.blockchain_db.fetch_last_header().unwrap().height - pruning_horizon
    );
    assert!(alice_node.blockchain_db.fetch_output(output_hash).unwrap().is_none());
    assert!(alice_node
        .blockchain_db
        .fetch_unspent_output_hash_by_commitment(commitment)
        .unwrap()
        .is_none());
    // Bob will not be banned
    assert!(!sync::wait_for_is_peer_banned(&alice_node, bob_node.node_identity.node_id(), 1).await);

    // 4. Alice attempts horizon sync again without any change in the blockchain
    println!("\n4. Alice attempts horizon sync again without any change in the blockchain\n");

    let event = decide_horizon_sync(&mut alice_state_machine, header_sync).await;
    let mut horizon_sync = match event {
        StateEvent::ProceedToHorizonSync(sync_peers) => HorizonStateSync::from(sync_peers),
        _ => panic!("4. Alice should proceed to horizon sync"),
    };
    let event = sync::horizon_sync_execute(&mut alice_state_machine, &mut horizon_sync).await;

    println!(
        "Event: {} to block {}",
        state_event(&event),
        alice_node.blockchain_db.get_height().unwrap()
    );
    assert_eq!(event, StateEvent::HorizonStateSynchronized);
    assert_eq!(
        alice_node.blockchain_db.get_height().unwrap(),
        alice_node.blockchain_db.fetch_last_header().unwrap().height - pruning_horizon
    );
    // Bob will not be banned
    assert!(!sync::wait_for_is_peer_banned(&alice_node, bob_node.node_identity.node_id(), 1).await);

    // 5. Alice attempts block sync to the tip (to height 10)
    println!("\n5. Alice attempts block sync to the tip (to height 10)\n");

    let mut block_sync = sync::initialize_sync_blocks(&bob_node);
    let event = sync::sync_blocks_execute(&mut alice_state_machine, &mut block_sync).await;
    println!(
        "Event: {} to block {}",
        state_event(&event),
        alice_node.blockchain_db.get_height().unwrap()
    );
    assert_eq!(event, StateEvent::BlocksSynchronized);
    assert_eq!(
        alice_node.blockchain_db.get_height().unwrap(),
        alice_node.blockchain_db.fetch_last_header().unwrap().height
    );
    // Bob will not be banned
    assert!(!sync::wait_for_is_peer_banned(&alice_node, bob_node.node_identity.node_id(), 1).await);

    // Give Bob some more blocks (containing the block with the spend transaction at height 16)
    sync::add_some_existing_blocks(&blocks[11..=25], &bob_node);
    assert_eq!(bob_node.blockchain_db.get_height().unwrap(), 25);
    assert_eq!(bob_node.blockchain_db.fetch_last_header().unwrap().height, 25);

    // 6. Alice does header sync to the new height (to height 25)
    println!("\n6. Alice does header sync to the new height (to height 25)\n");

    let mut header_sync = sync::initialize_sync_headers_with_ping_pong_data(&alice_node, &bob_node);
    let _event = sync::sync_headers_execute(&mut alice_state_machine, &mut header_sync).await;
    assert_eq!(alice_node.blockchain_db.fetch_last_header().unwrap().height, 25);
    // Bob will not be banned
    assert!(!sync::wait_for_is_peer_banned(&alice_node, bob_node.node_identity.node_id(), 1).await);

    // 7. Alice attempts horizon sync to the new pruning height (to height 20 - STXOs should be pruned) Outputs created
    //    after height 10 and spent up to height 20 with corresponding inputs should not be streamed; we do not have way
    //    to verify this except looking at the detail log files.
    println!("\n7. Alice attempts horizon sync to the new pruning height (to height 20 - STXOs should be pruned)\n");
    let spent_coinbases = coinbases
        .iter()
        .skip(1)
        .take(10) // To current height
        .collect::<Vec<_>>();
    for output in &spent_coinbases {
        let output_hash = output.hash(&key_manager).await.unwrap();
        assert!(alice_node.blockchain_db.fetch_output(output_hash).unwrap().is_some());
        let commitment = output.commitment(&key_manager).await.unwrap();
        assert!(alice_node
            .blockchain_db
            .fetch_unspent_output_hash_by_commitment(commitment)
            .unwrap()
            .is_some());
    }

    let event = decide_horizon_sync(&mut alice_state_machine, header_sync).await;
    let mut horizon_sync = match event {
        StateEvent::ProceedToHorizonSync(sync_peers) => HorizonStateSync::from(sync_peers),
        _ => panic!("7. Alice should proceed to horizon sync"),
    };
    let event = sync::horizon_sync_execute(&mut alice_state_machine, &mut horizon_sync).await;

    println!(
        "Event: {} to block {}",
        state_event(&event),
        alice_node.blockchain_db.get_height().unwrap()
    );
    assert_eq!(event, StateEvent::HorizonStateSynchronized);
    assert_eq!(
        alice_node.blockchain_db.get_height().unwrap(),
        alice_node.blockchain_db.fetch_last_header().unwrap().height - pruning_horizon
    );
    for output in &spent_coinbases {
        let output_hash = output.hash(&key_manager).await.unwrap();
        assert!(alice_node.blockchain_db.fetch_output(output_hash).unwrap().is_none());
        let commitment = output.commitment(&key_manager).await.unwrap();
        assert!(alice_node
            .blockchain_db
            .fetch_unspent_output_hash_by_commitment(commitment)
            .unwrap()
            .is_none());
    }
    // Bob will not be banned
    assert!(!sync::wait_for_is_peer_banned(&alice_node, bob_node.node_identity.node_id(), 1).await);

    // Give Bob some more blocks (containing the block with the spend transaction at height 16)
    sync::add_some_existing_blocks(&blocks[26..=30], &bob_node);
    assert_eq!(bob_node.blockchain_db.get_height().unwrap(), 30);
    assert_eq!(bob_node.blockchain_db.fetch_last_header().unwrap().height, 30);

    // 8. Alice does header sync to the new height (to height 30)
    println!("\n8. Alice does header sync to the new height (to height 30)\n");

    let mut header_sync = sync::initialize_sync_headers_with_ping_pong_data(&alice_node, &bob_node);
    let _event = sync::sync_headers_execute(&mut alice_state_machine, &mut header_sync).await;
    assert_eq!(alice_node.blockchain_db.fetch_last_header().unwrap().height, 30);
    // Bob will not be banned
    assert!(!sync::wait_for_is_peer_banned(&alice_node, bob_node.node_identity.node_id(), 1).await);

    // 9. Alice attempts horizon sync to the new pruning height (to height 25)
    println!("\n9. Alice attempts horizon sync to the new pruning height (to height 25)\n");

    let event = decide_horizon_sync(&mut alice_state_machine, header_sync).await;
    let mut horizon_sync = match event {
        StateEvent::ProceedToHorizonSync(sync_peers) => HorizonStateSync::from(sync_peers),
        _ => panic!("9. Alice should proceed to horizon sync"),
    };
    let event = sync::horizon_sync_execute(&mut alice_state_machine, &mut horizon_sync).await;

    println!(
        "Event: {} to block {}",
        state_event(&event),
        alice_node.blockchain_db.get_height().unwrap()
    );
    assert_eq!(event, StateEvent::HorizonStateSynchronized);
    assert_eq!(
        alice_node.blockchain_db.get_height().unwrap(),
        alice_node.blockchain_db.fetch_last_header().unwrap().height - pruning_horizon
    );
    // Bob will not be banned
    assert!(!sync::wait_for_is_peer_banned(&alice_node, bob_node.node_identity.node_id(), 1).await);
}

#[allow(clippy::too_many_lines)]
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_consecutive_horizon_sync_from_prune_node_happy_path() {
    //` cargo test --release --test core_integration_tests
    //` tests::horizon_sync::test_initial_horizon_sync_from_prune_node_happy_path > .\target\output.txt 2>&1
    // env_logger::init(); // Set `$env:RUST_LOG = "trace"`

    // Create the network with Alice (pruning node) and Bob (archival node) and Carol (pruning node)
    let pruning_horizon_alice = 4;
    let pruning_horizon_carol = 12;
    let (mut state_machines, mut peer_nodes, initial_block, consensus_manager, key_manager, initial_coinbase) =
        sync::create_network_with_multiple_nodes(vec![
            // Alice is a pruned node
            BlockchainDatabaseConfig {
                orphan_storage_capacity: 5,
                pruning_horizon: pruning_horizon_alice,
                pruning_interval: 5,
                track_reorgs: false,
                cleanup_orphans_at_startup: false,
            },
            // Carol is a pruned node
            BlockchainDatabaseConfig {
                orphan_storage_capacity: 5,
                pruning_horizon: pruning_horizon_carol,
                pruning_interval: 5,
                track_reorgs: false,
                cleanup_orphans_at_startup: false,
            },
            // Bob is an archival node
            BlockchainDatabaseConfig::default(),
        ])
        .await;
    let mut alice_state_machine = state_machines.remove(0);
    let mut carol_state_machine = state_machines.remove(0);
    let alice_node = peer_nodes.remove(0);
    let carol_node = peer_nodes.remove(0);
    let bob_node = peer_nodes.remove(0);

    // Create a blockchain that spends the genesys coinbase early on and then later spends some more coinbase outputs
    let follow_up_coinbases_to_spend = 5;
    let (blocks, _coinbases) = sync::create_block_chain_with_transactions(
        &bob_node,
        &initial_block,
        &initial_coinbase,
        &consensus_manager,
        &key_manager,
        min(pruning_horizon_alice, pruning_horizon_carol),
        28,                           // > follow_up_transaction_in_block + pruning_horizon_carol + 1
        2,                            // < pruning_horizon_alice, < pruning_horizon_carol
        14,                           // > pruning_horizon_alice, > pruning_horizon_carol
        follow_up_coinbases_to_spend, // > spend_genesis_coinbase_in_block - 1, < follow_up_transaction_in_block
    )
    .await;

    // Now rewind Bob's chain to height 8 (> pruning_horizon, < follow_up_transaction_in_block)
    sync::delete_some_blocks_and_headers(&blocks[8..=28], WhatToDelete::BlocksAndHeaders, &bob_node);
    assert_eq!(bob_node.blockchain_db.get_height().unwrap(), 8);
    assert_eq!(bob_node.blockchain_db.fetch_last_header().unwrap().height, 8);
    println!(
        "\nBob's blockchain height: {}\n",
        bob_node.blockchain_db.get_height().unwrap()
    );

    // 1. Alice attempts initial horizon sync from Bob (to pruning height 4; includes genesys block UTXO spend)
    println!(
        "\n1. Alice attempts initial horizon sync from Bob (to pruning height 4; includes genesys block UTXO spend)\n"
    );
    let output_hash = initial_coinbase.hash(&key_manager).await.unwrap();
    assert!(alice_node.blockchain_db.fetch_output(output_hash).unwrap().is_some());
    let commitment = initial_coinbase.commitment(&key_manager).await.unwrap();
    assert!(alice_node
        .blockchain_db
        .fetch_unspent_output_hash_by_commitment(commitment.clone())
        .unwrap()
        .is_some());

    let header_sync_alice_from_bob = sync::initialize_sync_headers_with_ping_pong_data(&alice_node, &bob_node);
    let event = sync::sync_headers_execute(&mut alice_state_machine, &mut header_sync_alice_from_bob.clone()).await;
    let alice_header_height = alice_node.blockchain_db.fetch_last_header().unwrap().height;
    println!("Event: {} to header {}", state_event(&event), alice_header_height);
    assert_eq!(alice_header_height, 8);
    let event = decide_horizon_sync(&mut alice_state_machine, header_sync_alice_from_bob).await;
    let mut horizon_sync = match event {
        StateEvent::ProceedToHorizonSync(sync_peers) => HorizonStateSync::from(sync_peers),
        _ => panic!("1. Alice should proceed to horizon sync"),
    };
    let event = sync::horizon_sync_execute(&mut alice_state_machine, &mut horizon_sync).await;

    println!(
        "Event: {} to block {}",
        state_event(&event),
        alice_node.blockchain_db.get_height().unwrap()
    );
    assert_eq!(event, StateEvent::HorizonStateSynchronized);
    assert_eq!(
        alice_node.blockchain_db.get_height().unwrap(),
        alice_header_height - pruning_horizon_alice
    );
    assert!(alice_node.blockchain_db.fetch_output(output_hash).unwrap().is_none());
    assert!(alice_node
        .blockchain_db
        .fetch_unspent_output_hash_by_commitment(commitment)
        .unwrap()
        .is_none());
    // Bob will not be banned
    assert!(!sync::wait_for_is_peer_banned(&alice_node, bob_node.node_identity.node_id(), 1).await);

    // 2. Carol attempts initial horizon sync from Bob with inadequate height
    println!("\n2. Carol attempts initial horizon sync from Bob with inadequate height\n");

    let mut header_sync_carol_from_bob = sync::initialize_sync_headers_with_ping_pong_data(&carol_node, &bob_node);
    let event = sync::sync_headers_execute(&mut carol_state_machine, &mut header_sync_carol_from_bob).await;
    let carol_header_height = carol_node.blockchain_db.fetch_last_header().unwrap().height;
    println!("Event: {} to header {}", state_event(&event), carol_header_height);
    assert_eq!(carol_header_height, 8);
    let event = decide_horizon_sync(&mut carol_state_machine, header_sync_carol_from_bob).await;
    match event {
        StateEvent::ProceedToBlockSync(_) => println!("Carol chose `ProceedToBlockSync` instead"),
        _ => panic!("2. Carol should not choose '{:?}'", event),
    }

    // Give Bob some more blocks
    sync::add_some_existing_blocks(&blocks[9..=13], &bob_node);
    assert_eq!(bob_node.blockchain_db.get_height().unwrap(), 13);
    assert_eq!(bob_node.blockchain_db.fetch_last_header().unwrap().height, 13);
    println!(
        "\nBob's blockchain height: {}\n",
        bob_node.blockchain_db.get_height().unwrap()
    );

    // 3. Alice attempts horizon sync from Bob (to pruning height 9)
    println!("\n3. Alice attempts horizon sync from Bob (to pruning height 9)\n");

    let mut header_sync_alice_from_bob = sync::initialize_sync_headers_with_ping_pong_data(&alice_node, &bob_node);
    let event = sync::sync_headers_execute(&mut alice_state_machine, &mut header_sync_alice_from_bob).await;
    let alice_header_height = alice_node.blockchain_db.fetch_last_header().unwrap().height;
    println!("Event: {} to header {}", state_event(&event), alice_header_height);
    assert_eq!(alice_header_height, 13);
    let event = decide_horizon_sync(&mut alice_state_machine, header_sync_alice_from_bob).await;
    let mut horizon_sync = match event {
        StateEvent::ProceedToHorizonSync(sync_peers) => HorizonStateSync::from(sync_peers),
        _ => panic!("3. Alice should proceed to horizon sync"),
    };
    let event = sync::horizon_sync_execute(&mut alice_state_machine, &mut horizon_sync).await;

    println!(
        "Event: {} to block {}",
        state_event(&event),
        alice_node.blockchain_db.get_height().unwrap()
    );
    assert_eq!(event, StateEvent::HorizonStateSynchronized);
    assert_eq!(
        alice_node.blockchain_db.get_height().unwrap(),
        alice_header_height - pruning_horizon_alice
    );
    // Bob will not be banned
    assert!(!sync::wait_for_is_peer_banned(&alice_node, bob_node.node_identity.node_id(), 1).await);

    // 4. Alice attempts block sync from Bob to the tip (to height 13)
    println!("\n4. Alice attempts block sync from Bob to the tip (to height 13)\n");

    let mut block_sync = sync::initialize_sync_blocks(&bob_node);
    let event = sync::sync_blocks_execute(&mut alice_state_machine, &mut block_sync).await;
    println!(
        "Event: {} to block {}",
        state_event(&event),
        alice_node.blockchain_db.get_height().unwrap()
    );
    assert_eq!(event, StateEvent::BlocksSynchronized);
    assert_eq!(
        alice_node.blockchain_db.get_height().unwrap(),
        alice_node.blockchain_db.fetch_last_header().unwrap().height
    );
    // Bob will not be banned
    assert!(!sync::wait_for_is_peer_banned(&alice_node, bob_node.node_identity.node_id(), 1).await);

    // 5 Carol attempts initial horizon sync from Alice with adequate height (but Alice is not an archival node)
    println!(
        "\n5. Carol attempts initial horizon sync from Alice with adequate height (but Alice is not an archival \
         node)\n"
    );

    let mut header_sync_carol_from_alice = sync::initialize_sync_headers_with_ping_pong_data(&carol_node, &alice_node);
    let event = sync::sync_headers_execute(&mut carol_state_machine, &mut header_sync_carol_from_alice).await;
    let carol_header_height = carol_node.blockchain_db.fetch_last_header().unwrap().height;
    println!("Event: {} to header {}", state_event(&event), carol_header_height);
    assert_eq!(carol_header_height, 13);
    let event = decide_horizon_sync(&mut carol_state_machine, header_sync_carol_from_alice).await;
    match event {
        StateEvent::Continue => println!("Carol chose `Continue` instead"),
        _ => panic!("5. Carol should not choose '{:?}'", event),
    }
    // Alice will not be banned
    assert!(!sync::wait_for_is_peer_banned(&carol_node, alice_node.node_identity.node_id(), 1).await);

    // 6. Carol attempts initial horizon sync from Bob with adequate height (to pruning height 1)
    println!("\n6. Carol attempts initial horizon sync from Bob with adequate height (to height 1)\n");

    let mut header_sync_carol_from_bob = sync::initialize_sync_headers_with_ping_pong_data(&carol_node, &bob_node);
    let event = sync::sync_headers_execute(&mut carol_state_machine, &mut header_sync_carol_from_bob).await;
    let carol_header_height = carol_node.blockchain_db.fetch_last_header().unwrap().height;
    println!("Event: {} to header {}", state_event(&event), carol_header_height);
    assert_eq!(carol_header_height, 13);
    let event = decide_horizon_sync(&mut carol_state_machine, header_sync_carol_from_bob).await;
    let mut horizon_sync = match event {
        StateEvent::ProceedToHorizonSync(sync_peers) => HorizonStateSync::from(sync_peers),
        _ => panic!("6. Carol should proceed to horizon sync"),
    };
    let event = sync::horizon_sync_execute(&mut carol_state_machine, &mut horizon_sync).await;

    println!(
        "Event: {} to block {}",
        state_event(&event),
        carol_node.blockchain_db.get_height().unwrap()
    );
    assert_eq!(event, StateEvent::HorizonStateSynchronized);
    assert_eq!(
        carol_node.blockchain_db.get_height().unwrap(),
        carol_header_height - pruning_horizon_carol
    );
    // Bob will not be banned
    assert!(!sync::wait_for_is_peer_banned(&carol_node, bob_node.node_identity.node_id(), 1).await);

    // Give Bob some more blocks
    sync::add_some_existing_blocks(&blocks[14..=18], &bob_node);
    assert_eq!(bob_node.blockchain_db.get_height().unwrap(), 18);
    assert_eq!(bob_node.blockchain_db.fetch_last_header().unwrap().height, 18);
    println!(
        "\nBob's blockchain height: {}\n",
        bob_node.blockchain_db.get_height().unwrap()
    );

    // 7. Alice attempts horizon sync from Bob (to pruning height 14)
    println!("\n7. Alice attempts horizon sync from Bob (to pruning height 14)\n");

    let mut header_sync_alice_from_bob = sync::initialize_sync_headers_with_ping_pong_data(&alice_node, &bob_node);
    let event = sync::sync_headers_execute(&mut alice_state_machine, &mut header_sync_alice_from_bob).await;
    let alice_header_height = alice_node.blockchain_db.fetch_last_header().unwrap().height;
    println!("Event: {} to header {}", state_event(&event), alice_header_height);
    assert_eq!(alice_header_height, 18);
    let event = decide_horizon_sync(&mut alice_state_machine, header_sync_alice_from_bob).await;
    let mut horizon_sync = match event {
        StateEvent::ProceedToHorizonSync(sync_peers) => HorizonStateSync::from(sync_peers),
        _ => panic!("7. Alice should proceed to horizon sync"),
    };
    let event = sync::horizon_sync_execute(&mut alice_state_machine, &mut horizon_sync).await;

    println!(
        "Event: {} to block {}",
        state_event(&event),
        alice_node.blockchain_db.get_height().unwrap()
    );
    assert_eq!(event, StateEvent::HorizonStateSynchronized);
    assert_eq!(
        alice_node.blockchain_db.get_height().unwrap(),
        alice_header_height - pruning_horizon_alice
    );
    // Bob will not be banned
    assert!(!sync::wait_for_is_peer_banned(&alice_node, bob_node.node_identity.node_id(), 1).await);

    // 8. Alice attempts block sync from Bob to the tip (to height 18)
    println!("\n8. Alice attempts block sync from Bob to the tip (to height 18)\n");

    let mut block_sync = sync::initialize_sync_blocks(&bob_node);
    let event = sync::sync_blocks_execute(&mut alice_state_machine, &mut block_sync).await;
    println!(
        "Event: {} to block {}",
        state_event(&event),
        alice_node.blockchain_db.get_height().unwrap()
    );
    assert_eq!(event, StateEvent::BlocksSynchronized);
    assert_eq!(
        alice_node.blockchain_db.get_height().unwrap(),
        alice_node.blockchain_db.fetch_last_header().unwrap().height
    );
    // Bob will not be banned
    assert!(!sync::wait_for_is_peer_banned(&alice_node, bob_node.node_identity.node_id(), 1).await);

    // 9. Carol attempts horizon sync from Alice with inadequate pruning horizon (to height 6)
    println!("\n9. Carol attempts horizon sync from Alice with inadequate pruning horizon (to height 6)\n");

    let mut header_sync_carol_from_alice = sync::initialize_sync_headers_with_ping_pong_data(&carol_node, &alice_node);
    let event = sync::sync_headers_execute(&mut carol_state_machine, &mut header_sync_carol_from_alice).await;
    let carol_header_height = carol_node.blockchain_db.fetch_last_header().unwrap().height;
    println!("Event: {} to header {}", state_event(&event), carol_header_height);
    assert_eq!(carol_header_height, 18);
    let event = decide_horizon_sync(&mut carol_state_machine, header_sync_carol_from_alice).await;
    match event {
        StateEvent::Continue => println!("Carol chose `Continue` instead"),
        _ => panic!("9. Carol should not choose '{:?}'", event),
    }
    // Alice will not be banned
    assert!(!sync::wait_for_is_peer_banned(&carol_node, alice_node.node_identity.node_id(), 1).await);

    // Give Bob some more blocks
    sync::add_some_existing_blocks(&blocks[14..=22], &bob_node);
    assert_eq!(bob_node.blockchain_db.get_height().unwrap(), 22);
    assert_eq!(bob_node.blockchain_db.fetch_last_header().unwrap().height, 22);
    println!(
        "\nBob's blockchain height: {}\n",
        bob_node.blockchain_db.get_height().unwrap()
    );

    // 10. Carol attempts horizon sync from Bob (to pruning height 10)
    println!("\n10. Carol attempts horizon sync from Bob (to pruning height 10)\n");

    let mut header_sync_carol_from_bob = sync::initialize_sync_headers_with_ping_pong_data(&carol_node, &bob_node);
    let event = sync::sync_headers_execute(&mut carol_state_machine, &mut header_sync_carol_from_bob).await;
    let carol_header_height = carol_node.blockchain_db.fetch_last_header().unwrap().height;
    println!("Event: {} to header {}", state_event(&event), carol_header_height);
    assert_eq!(carol_header_height, 22);
    let event = decide_horizon_sync(&mut carol_state_machine, header_sync_carol_from_bob).await;
    let mut horizon_sync = match event {
        StateEvent::ProceedToHorizonSync(sync_peers) => HorizonStateSync::from(sync_peers),
        _ => panic!("10. Carol should proceed to horizon sync"),
    };
    let event = sync::horizon_sync_execute(&mut carol_state_machine, &mut horizon_sync).await;

    println!(
        "Event: {} to block {}",
        state_event(&event),
        carol_node.blockchain_db.get_height().unwrap()
    );
    assert_eq!(event, StateEvent::HorizonStateSynchronized);
    assert_eq!(
        carol_node.blockchain_db.get_height().unwrap(),
        carol_header_height - pruning_horizon_carol
    );
    // Bob will not be banned
    assert!(!sync::wait_for_is_peer_banned(&carol_node, bob_node.node_identity.node_id(), 1).await);

    // 11. Carol attempts block sync from Bob to the tip (to height 22)
    println!("\n11. Carol attempts block sync from Bob to the tip (to height 22)\n");

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

    // 12. Alice attempts horizon sync from Carol with adequate pruning horizon (to height 18)
    println!("\n12. Alice attempts horizon sync from Carol with adequate pruning horizon (to height 18)\n");

    let mut header_sync_alice_from_carol = sync::initialize_sync_headers_with_ping_pong_data(&alice_node, &carol_node);
    let event = sync::sync_headers_execute(&mut alice_state_machine, &mut header_sync_alice_from_carol).await;
    let alice_header_height = alice_node.blockchain_db.fetch_last_header().unwrap().height;
    println!("Event: {} to header {}", state_event(&event), alice_header_height);
    assert_eq!(alice_header_height, 22);
    let event = decide_horizon_sync(&mut alice_state_machine, header_sync_alice_from_carol).await;
    let mut horizon_sync = match event {
        StateEvent::ProceedToHorizonSync(sync_peers) => HorizonStateSync::from(sync_peers),
        _ => panic!("12. Alice should proceed to horizon sync"),
    };
    let event = sync::horizon_sync_execute(&mut alice_state_machine, &mut horizon_sync).await;

    println!(
        "Event: {} to block {}",
        state_event(&event),
        alice_node.blockchain_db.get_height().unwrap()
    );
    assert_eq!(event, StateEvent::HorizonStateSynchronized);
    assert_eq!(
        alice_node.blockchain_db.get_height().unwrap(),
        alice_header_height - pruning_horizon_alice
    );
    // Carol will not be banned
    assert!(!sync::wait_for_is_peer_banned(&alice_node, carol_node.node_identity.node_id(), 1).await);
}

#[allow(clippy::too_many_lines)]
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_initial_horizon_sync_from_prune_node_happy_path() {
    //` cargo test --release --test core_integration_tests
    //` tests::horizon_sync::test_initial_horizon_sync_from_prune_node_happy_path > .\target\output.txt 2>&1
    // env_logger::init(); // Set `$env:RUST_LOG = "trace"`

    // Create the network with Alice (pruning node) and Bob (archival node) and Carol (pruning node)
    let pruning_horizon_alice = 4;
    let pruning_horizon_carol = 12;
    let (mut state_machines, mut peer_nodes, initial_block, consensus_manager, key_manager, initial_coinbase) =
        sync::create_network_with_multiple_nodes(vec![
            // Alice is a pruned node
            BlockchainDatabaseConfig {
                orphan_storage_capacity: 5,
                pruning_horizon: pruning_horizon_alice,
                pruning_interval: 5,
                track_reorgs: false,
                cleanup_orphans_at_startup: false,
            },
            // Carol is a pruned node
            BlockchainDatabaseConfig {
                orphan_storage_capacity: 5,
                pruning_horizon: pruning_horizon_carol,
                pruning_interval: 5,
                track_reorgs: false,
                cleanup_orphans_at_startup: false,
            },
            // Bob is an archival node
            BlockchainDatabaseConfig::default(),
        ])
        .await;
    let mut alice_state_machine = state_machines.remove(0);
    let mut carol_state_machine = state_machines.remove(0);
    let alice_node = peer_nodes.remove(0);
    let carol_node = peer_nodes.remove(0);
    let bob_node = peer_nodes.remove(0);

    // Create a blockchain that spends the genesys coinbase early on and then later spends some more coinbase outputs
    let follow_up_coinbases_to_spend = 5;
    let (_blocks, _coinbases) = sync::create_block_chain_with_transactions(
        &bob_node,
        &initial_block,
        &initial_coinbase,
        &consensus_manager,
        &key_manager,
        min(pruning_horizon_alice, pruning_horizon_carol),
        28,                           // > follow_up_transaction_in_block + pruning_horizon_carol + 1
        2,                            // < pruning_horizon_alice, < pruning_horizon_carol
        14,                           // > pruning_horizon_alice, > pruning_horizon_carol
        follow_up_coinbases_to_spend, // > spend_genesis_coinbase_in_block - 1, < follow_up_transaction_in_block
    )
    .await;

    // 1. Carol attempts initial horizon sync from Bob archival node (to pruning height 16)
    println!("\n1. Carol attempts initial horizon sync from Bob archival node (to pruning height 16)\n");

    let output_hash = initial_coinbase.hash(&key_manager).await.unwrap();
    assert!(carol_node.blockchain_db.fetch_output(output_hash).unwrap().is_some());
    let commitment = initial_coinbase.commitment(&key_manager).await.unwrap();
    assert!(carol_node
        .blockchain_db
        .fetch_unspent_output_hash_by_commitment(commitment.clone())
        .unwrap()
        .is_some());

    let mut header_sync_carol_from_bob = sync::initialize_sync_headers_with_ping_pong_data(&carol_node, &bob_node);
    let event = sync::sync_headers_execute(&mut carol_state_machine, &mut header_sync_carol_from_bob).await;
    let carol_header_height = carol_node.blockchain_db.fetch_last_header().unwrap().height;
    println!("Event: {} to header {}", state_event(&event), carol_header_height);
    assert_eq!(carol_header_height, 28);
    let event = decide_horizon_sync(&mut carol_state_machine, header_sync_carol_from_bob).await;
    let mut horizon_sync = match event {
        StateEvent::ProceedToHorizonSync(sync_peers) => HorizonStateSync::from(sync_peers),
        _ => panic!("1. Carol should proceed to horizon sync"),
    };
    let event = sync::horizon_sync_execute(&mut carol_state_machine, &mut horizon_sync).await;

    println!(
        "Event: {} to block {}",
        state_event(&event),
        carol_node.blockchain_db.get_height().unwrap()
    );
    assert_eq!(event, StateEvent::HorizonStateSynchronized);
    assert_eq!(
        carol_node.blockchain_db.get_height().unwrap(),
        carol_header_height - pruning_horizon_carol
    );

    assert!(carol_node.blockchain_db.fetch_output(output_hash).unwrap().is_none());
    assert!(carol_node
        .blockchain_db
        .fetch_unspent_output_hash_by_commitment(commitment.clone())
        .unwrap()
        .is_none());

    // Bob will not be banned
    assert!(!sync::wait_for_is_peer_banned(&carol_node, bob_node.node_identity.node_id(), 1).await);

    // 2. Carol attempts block sync from Bob to the tip (to height 28)
    println!("\n2. Carol attempts block sync from Bob to the tip (to height 28)\n");

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

    // 3. Alice attempts initial horizon sync from Carol prune node (to height 24)
    println!("\n3. Alice attempts initial horizon sync from Carol prune node (to height 24)\n");

    assert!(alice_node.blockchain_db.fetch_output(output_hash).unwrap().is_some());
    assert!(alice_node
        .blockchain_db
        .fetch_unspent_output_hash_by_commitment(commitment.clone())
        .unwrap()
        .is_some());

    let mut header_sync_alice_from_carol = sync::initialize_sync_headers_with_ping_pong_data(&alice_node, &carol_node);
    let event = sync::sync_headers_execute(&mut alice_state_machine, &mut header_sync_alice_from_carol).await;
    let alice_header_height = alice_node.blockchain_db.fetch_last_header().unwrap().height;
    println!("Event: {} to header {}", state_event(&event), alice_header_height);
    assert_eq!(alice_header_height, 28);
    let event = decide_horizon_sync(&mut alice_state_machine, header_sync_alice_from_carol).await;
    let mut horizon_sync = match event {
        StateEvent::ProceedToHorizonSync(sync_peers) => HorizonStateSync::from(sync_peers),
        _ => panic!("3. Alice should proceed to horizon sync"),
    };
    let event = sync::horizon_sync_execute(&mut alice_state_machine, &mut horizon_sync).await;

    println!(
        "Event: {} to block {}",
        state_event(&event),
        alice_node.blockchain_db.get_height().unwrap()
    );
    assert_eq!(event, StateEvent::HorizonStateSynchronized);
    assert_eq!(
        alice_node.blockchain_db.get_height().unwrap(),
        alice_header_height - pruning_horizon_alice
    );

    assert!(alice_node.blockchain_db.fetch_output(output_hash).unwrap().is_none());
    assert!(alice_node
        .blockchain_db
        .fetch_unspent_output_hash_by_commitment(commitment.clone())
        .unwrap()
        .is_none());

    // Carol will not be banned
    assert!(!sync::wait_for_is_peer_banned(&alice_node, carol_node.node_identity.node_id(), 1).await);

    // 4. Alice attempts block sync from Carol prune node to the tip (to height 28)
    println!("\n4. Alice attempts block sync from Carol prune node to the tip (to height 28)\n");

    let mut block_sync = sync::initialize_sync_blocks(&carol_node);
    let event = sync::sync_blocks_execute(&mut alice_state_machine, &mut block_sync).await;
    println!(
        "Event: {} to block {}",
        state_event(&event),
        alice_node.blockchain_db.get_height().unwrap()
    );
    assert_eq!(event, StateEvent::BlocksSynchronized);
    assert_eq!(
        alice_node.blockchain_db.get_height().unwrap(),
        alice_node.blockchain_db.fetch_last_header().unwrap().height
    );
    // Carol will not be banned
    assert!(!sync::wait_for_is_peer_banned(&alice_node, carol_node.node_identity.node_id(), 1).await);
}
