// Copyright 2020, The Tari Project
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

//! # MemoryNet
//!
//! This example runs a small in-memory network.
//! It's primary purpose is to test and debug the behaviour of the DHT.
//!
//! The following happens:
//! 1. A single "seed node", `NUM_NODES` "base nodes" and `NUM_WALLETS` "wallets" are generated and started.
//! 1. All "base nodes" join the network via the "seed node"
//! 1. All "wallets" join the network via a random "base node"
//! 1. The first "wallet" in the list attempts to discover the last "wallet" in the list
//!
//! The suggested way to run this is:
//!
//! `RUST_BACKTRACE=1 RUST_LOG=trace cargo run --example memorynet 2> /tmp/debug.log`

mod memory_net;

use crate::memory_net::utilities::{
    discovery,
    do_network_wide_propagation,
    do_store_and_forward_message_propagation,
    drain_messaging_events,
    get_name,
    make_node,
    print_network_connectivity_stats,
    print_network_peer_list_stats,
    shutdown_all,
    take_a_break,
};
use futures::{channel::mpsc, future};
use rand::{rngs::OsRng, Rng};
use std::{iter::repeat_with, time::Duration};
use tari_comms::peer_manager::PeerFeatures;

// Size of network. Must be at least 2
const NUM_NODES: usize = 16;
// Must be at least 2
const NUM_WALLETS: usize = 4;
const QUIET_MODE: bool = true;
/// Number of neighbouring nodes each node should include in the connection pool
const NUM_NEIGHBOURING_NODES: usize = 8;
/// Number of randomly-selected nodes each node should include in the connection pool
const NUM_RANDOM_NODES: usize = 4;
/// The number of messages that should be propagated out
const PROPAGATION_FACTOR: usize = 4;

#[tokio_macros::main]
#[allow(clippy::same_item_push)]
async fn main() {
    env_logger::init();

    banner!(
        "Bringing up virtual network consisting of a seed node, {} nodes and {} wallets",
        NUM_NODES,
        NUM_WALLETS
    );

    let (node_message_tx, mut messaging_events_rx) = mpsc::unbounded();

    let seed_node = vec![
        make_node(
            PeerFeatures::COMMUNICATION_NODE,
            vec![],
            node_message_tx.clone(),
            NUM_NEIGHBOURING_NODES,
            NUM_RANDOM_NODES,
            PROPAGATION_FACTOR,
            QUIET_MODE,
        )
        .await,
    ];

    let mut nodes = future::join_all(
        repeat_with(|| {
            make_node(
                PeerFeatures::COMMUNICATION_NODE,
                vec![seed_node[0].node_identity()],
                node_message_tx.clone(),
                NUM_NEIGHBOURING_NODES,
                NUM_RANDOM_NODES,
                PROPAGATION_FACTOR,
                QUIET_MODE,
            )
        })
        .take(NUM_NODES),
    )
    .await;

    let mut wallets = future::join_all(
        repeat_with(|| {
            make_node(
                PeerFeatures::COMMUNICATION_CLIENT,
                vec![nodes[OsRng.gen_range(0, NUM_NODES)].node_identity()],
                node_message_tx.clone(),
                NUM_NEIGHBOURING_NODES,
                NUM_RANDOM_NODES,
                PROPAGATION_FACTOR,
                QUIET_MODE,
            )
        })
        .take(NUM_WALLETS),
    )
    .await;

    // Every node knows about every other node/client - uncomment this if you want to see the effect of "perfect network
    // knowledge" on the network.
    //
    // for n in &nodes {
    //     for ni in &nodes {
    //         if n.node_identity().node_id() != ni.node_identity().node_id() {
    //             n.comms
    //                 .peer_manager()
    //                 .add_peer(ni.node_identity().to_peer())
    //                 .await
    //                 .unwrap();
    //         }
    //     }
    //     for ni in &wallets {
    //         n.comms
    //             .peer_manager()
    //             .add_peer(ni.node_identity().to_peer())
    //             .await
    //             .unwrap();
    //     }
    // }

    // Wait for all the nodes to startup and connect to seed node
    take_a_break(NUM_NODES).await;

    // log::info!("------------------------------- BASE NODE JOIN -------------------------------");
    // for index in 0..nodes.len() {
    //     {
    //         let node = nodes.get_mut(index).expect("Couldn't get TestNode");
    //         println!(
    //             "Node '{}' is joining the network via the seed node '{}'",
    //             node, seed_node[0]
    //         );
    //         node.comms
    //             .connectivity()
    //             .wait_for_connectivity(Duration::from_secs(10))
    //             .await
    //             .unwrap();
    //
    //         node.dht.dht_requester().send_join().await.unwrap();
    //     }
    // }

    take_a_break(NUM_NODES).await;

    // peer_list_summary(&nodes).await;

    for wallet in wallets.iter_mut() {
        log::info!("------------------------------- WALLET JOIN -------------------------------");
        println!(
            "Wallet '{}' is joining the network via node '{}'",
            wallet,
            get_name(&wallet.seed_peers[0].node_id)
        );
        wallet
            .comms
            .connectivity()
            .wait_for_connectivity(Duration::from_secs(60))
            .await
            .unwrap();
        wallet.dht.dht_requester().send_join().await.unwrap();
    }

    take_a_break(NUM_NODES).await;
    let mut total_messages = 0;
    total_messages += drain_messaging_events(&mut messaging_events_rx, false).await;

    print_network_peer_list_stats(&nodes, &nodes).await;
    print_network_peer_list_stats(&nodes, &wallets).await;
    print_network_connectivity_stats(&nodes, &wallets, QUIET_MODE).await;

    {
        let count = seed_node[0].comms.peer_manager().count().await;
        let num_connections = seed_node[0]
            .comms
            .connectivity()
            .get_active_connections()
            .await
            .unwrap()
            .len();
        println!("Seed node knows {} peers ({} connections)", count, num_connections);
    }

    take_a_break(NUM_NODES).await;

    log::info!("------------------------------- DISCOVERY -------------------------------");
    let (discovery_messages, discovery_successes, discovery_sent) = discovery(&wallets, &mut messaging_events_rx).await;
    total_messages += discovery_messages;

    total_messages += drain_messaging_events(&mut messaging_events_rx, false).await;

    log::info!("------------------------------- SAF/DIRECTED PROPAGATION -------------------------------");
    let mut total_saf_messages = 0;
    let mut total_saf_succeeded = 0;
    let mut total_saf_timeouts = 0;
    let total_saf_done = 5;
    for _ in 0..5 {
        let random_wallet = wallets.remove(OsRng.gen_range(0, wallets.len() - 1));
        let (num_msgs, random_wallet, num_successes, num_attempts) = do_store_and_forward_message_propagation(
            random_wallet,
            &wallets,
            &nodes,
            node_message_tx.clone(),
            &mut messaging_events_rx,
            NUM_NEIGHBOURING_NODES,
            NUM_RANDOM_NODES,
            PROPAGATION_FACTOR,
            QUIET_MODE,
        )
        .await;
        total_saf_messages += num_msgs;
        total_messages += num_msgs;
        if num_successes > 0 {
            total_saf_succeeded += 1;
        }
        if num_successes < num_attempts {
            total_saf_timeouts += 1;
        }
        // Put the wallet back
        wallets.push(random_wallet);
    }

    log::info!("------------------------------- PROPAGATION -------------------------------");
    let (num_prop_successes, num_prop_total) = do_network_wide_propagation(&mut nodes, None).await;

    total_messages += drain_messaging_events(&mut messaging_events_rx, false).await;

    println!("{} messages sent in total across the network", total_messages);

    print_network_peer_list_stats(&nodes, &wallets).await;
    print_network_connectivity_stats(&nodes, &wallets, QUIET_MODE).await;

    banner!("Summary");
    println!("Total messages sent: {}", total_messages);
    println!("Discovery messages: {}", discovery_messages);
    println!("Total discoveries: {}/{}", discovery_successes, discovery_sent);
    println!("SAF messages: {}", total_saf_messages);
    println!("SAF successes: {}/{}", total_saf_succeeded, total_saf_done);
    println!("SAF timeouts:{}/{}", total_saf_timeouts, total_saf_done);
    println!("Prop successes: {}/{}", num_prop_successes, num_prop_total);

    banner!("That's it folks! Network is shutting down...");
    log::info!("------------------------------- SHUTDOWN -------------------------------");

    shutdown_all(nodes).await;
    shutdown_all(wallets).await;
}
