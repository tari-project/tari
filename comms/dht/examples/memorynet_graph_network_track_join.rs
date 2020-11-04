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

// Size of network
const NUM_SEED_NODES: usize = 2;
// Size of network
const NUM_NODES: usize = 40;
// Must be at least 2
const NUM_WALLETS: usize = 6;
const QUIET_MODE: bool = true;
/// Number of neighbouring nodes each node should include in the connection pool
const NUM_NEIGHBOURING_NODES: usize = 8;
/// Number of randomly-selected nodes each node should include in the connection pool
const NUM_RANDOM_NODES: usize = 4;
/// The number of messages that should be propagated out
const PROPAGATION_FACTOR: usize = 4;

const DEFAULT_GRAPH_OUTPUT_DIR: &str = "/tmp/memorynet";

mod graphing_utilities;
mod memory_net;

use crate::{
    graphing_utilities::utilities::{
        create_message_propagation_graphs,
        network_graph_snapshot,
        run_python_network_graph_render,
        track_join_message_drain_messaging_events,
        PythonRenderType,
    },
    memory_net::utilities::{
        drain_messaging_events,
        make_node,
        make_node_from_node_identities,
        make_node_identity,
        shutdown_all,
        take_a_break,
    },
};
use clap::{App, Arg};
use env_logger::Env;
use futures::channel::mpsc;
use std::{path::Path, time::Duration};
use tari_comms::peer_manager::PeerFeatures;

#[tokio_macros::main]
async fn main() {
    let _ = env_logger::from_env(Env::default())
        .format_timestamp_millis()
        .try_init();
    let matches = App::new("MemoryNet")
        .version("0.1.0")
        .arg(
            Arg::with_name("output_dir")
                .short("o")
                .long("output")
                .takes_value(true)
                .value_name("PATH")
                .help("Graph output directory"),
        )
        .get_matches();

    let graph_output_dir = Path::new(matches.value_of("output_dir").unwrap_or(DEFAULT_GRAPH_OUTPUT_DIR))
        .to_str()
        .expect("Couldn't use provided output directory path");

    banner!(
        "Bringing up virtual network consisting of {} seed nodes, {} nodes and {} wallets",
        NUM_SEED_NODES,
        NUM_NODES,
        NUM_WALLETS
    );

    let (messaging_events_tx, mut messaging_events_rx) = mpsc::unbounded();

    let mut seed_identities = Vec::new();
    for _ in 0..NUM_SEED_NODES {
        seed_identities.push(make_node_identity(PeerFeatures::COMMUNICATION_NODE));
    }

    let mut seed_nodes = Vec::new();
    for i in 0..NUM_SEED_NODES {
        seed_nodes.push(
            make_node_from_node_identities(
                seed_identities[i].clone(),
                seed_identities
                    .iter()
                    .filter(|s| s.node_id() != seed_identities[i].node_id())
                    .map(|n| n.clone())
                    .collect(),
                messaging_events_tx.clone(),
                NUM_NEIGHBOURING_NODES,
                NUM_RANDOM_NODES,
                PROPAGATION_FACTOR,
                QUIET_MODE,
            )
            .await,
        );
    }

    let mut nodes = Vec::new();

    for i in 0..NUM_NODES {
        nodes.push(
            make_node(
                PeerFeatures::COMMUNICATION_NODE,
                vec![seed_nodes[i % NUM_SEED_NODES].node_identity()],
                messaging_events_tx.clone(),
                NUM_NEIGHBOURING_NODES,
                NUM_RANDOM_NODES,
                PROPAGATION_FACTOR,
                QUIET_MODE,
            )
            .await,
        );
    }

    // Wait for all the nodes to startup and connect to seed nodes
    take_a_break(NUM_NODES).await;
    // Taking this one snapshot of the network before the first non-seed node joins
    let _ = network_graph_snapshot(
        "base_node_track_join",
        &seed_nodes,
        &nodes,
        Some(NUM_NEIGHBOURING_NODES),
    )
    .await;

    log::info!("------------------------------- BASE NODES JOIN -------------------------------");
    for index in 0..NUM_NODES {
        {
            let node = nodes.get_mut(index).expect("Couldn't get TestNode");
            println!(
                "Node '{}' is joining the network via the seed node '{}'",
                node,
                node.seed_peers[0].node_id.short_str(),
            );
            node.comms
                .connectivity()
                .wait_for_connectivity(Duration::from_secs(10))
                .await
                .unwrap();

            node.dht.dht_requester().send_join().await.unwrap();
        }
    }

    take_a_break(NUM_NODES).await;
    let _ = network_graph_snapshot(
        "base_node_track_join",
        &seed_nodes,
        &nodes,
        Some(NUM_NEIGHBOURING_NODES),
    )
    .await;

    let new_node = make_node(
        PeerFeatures::COMMUNICATION_NODE,
        vec![seed_nodes[0].node_identity()],
        messaging_events_tx.clone(),
        NUM_NEIGHBOURING_NODES,
        NUM_RANDOM_NODES,
        PROPAGATION_FACTOR,
        QUIET_MODE,
    )
    .await;

    nodes.push(new_node);

    take_a_break(NUM_NODES).await;
    let _ = network_graph_snapshot(
        "base_node_track_join",
        &seed_nodes,
        &nodes,
        Some(NUM_NEIGHBOURING_NODES),
    )
    .await;

    // Clear all the messages till now
    let _ = drain_messaging_events(&mut messaging_events_rx, false).await;

    log::info!("------------------------------- NEW BASE NODE JOIN -------------------------------",);
    println!("------------------------------- NEW BASE NODE JOIN -------------------------------",);

    let (_, neighbour_graph) =
        network_graph_snapshot("join_propagation", &seed_nodes, &nodes, Some(NUM_NEIGHBOURING_NODES)).await;

    // Have new node join
    {
        println!(
            "New Node '{}' is joining the network via the seed node '{}'",
            nodes[nodes.len() - 1],
            nodes[nodes.len() - 1].seed_peers[0].node_id.short_str(),
        );
        nodes[nodes.len() - 1]
            .comms
            .connectivity()
            .wait_for_connectivity(Duration::from_secs(10))
            .await
            .unwrap();

        nodes[nodes.len() - 1].dht.dht_requester().send_join().await.unwrap();
    }

    take_a_break(NUM_NODES).await;

    // Log all the message sent in that join attempt
    banner!("Summary of message propagation");
    let message_tree = track_join_message_drain_messaging_events(&mut messaging_events_rx).await;

    create_message_propagation_graphs("join_propagation", neighbour_graph, message_tree).await;

    // Take a snapshot after the join has occured
    let _ = network_graph_snapshot(
        "base_node_track_join",
        &seed_nodes,
        &nodes,
        Some(NUM_NEIGHBOURING_NODES),
    )
    .await;

    banner!("Rendering graph");
    if let Err(e) = run_python_network_graph_render(
        "base_node_track_join",
        graph_output_dir,
        PythonRenderType::NetworkGraphFull,
    ) {
        println!("Error rendering graphs: {}", e);
    }

    if let Err(e) = run_python_network_graph_render("join_propagation", graph_output_dir, PythonRenderType::Propagation)
    {
        println!("Error rendering graphs: {}", e);
    }

    shutdown_all(nodes).await;
    shutdown_all(seed_nodes).await;
}
