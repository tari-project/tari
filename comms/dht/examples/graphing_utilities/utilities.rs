// Copyright 2020. The Tari Project
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

use crate::memory_net::{
    utilities::{get_short_name, NodeEventRx, TestNode},
    DrainBurst,
};
use lazy_static::lazy_static;
use petgraph::{
    dot::Dot,
    stable_graph::{NodeIndex, StableGraph},
    visit::{Bfs, IntoNodeReferences},
};
use std::{collections::HashMap, convert::TryFrom, fs, fs::File, io::Write, path::Path, process::Command, sync::Mutex};
use tari_comms::{connectivity::ConnectivitySelection, peer_manager::NodeId};

const TEMP_GRAPH_OUTPUT_DIR: &str = "/tmp/memorynet_temp";

lazy_static! {
    static ref GRAPH_FRAME_NUM: Mutex<HashMap<String, usize>> = Mutex::new(HashMap::new());
}

fn get_next_frame_num(name: &str) -> usize {
    let mut map = GRAPH_FRAME_NUM.lock().unwrap();
    let current: usize;
    match (*map).get_mut(&name.to_string()) {
        None => {
            current = 0;
            (*map).insert(name.to_string(), 1);
        },
        Some(count) => {
            current = *count;
            *count += 1;
        },
    }
    current
}

/// Construct a graph of a set of the provided seed nodes and test nodes for a given sequence name. The graph will be
/// saved as dot files in the temporary file location for this named sequence.
///
/// `name`: The name of the sequence that this graph is a part of, the next number in the sequence will be automatically
/// generated
/// `seed_nodes`: The set of seed nodes in the network
/// `network`: The set of network nodes to graph
/// `num_neighbours`: If you only want to graph up to the neighbours provide that number in this option, else all
/// connections are graphed.
pub async fn network_graph_snapshot(
    name: &str,
    seed_nodes: &[TestNode],
    network: &[TestNode],
    num_neighbours: Option<usize>,
) -> (StableGraph<NodeId, String>, StableGraph<NodeId, String>)
{
    let mut graph = StableGraph::new();
    let mut node_indices = HashMap::new();

    for node in seed_nodes.iter().chain(network.iter()) {
        let node_id = node.comms.node_identity().node_id().clone();
        let index: NodeIndex<petgraph::stable_graph::DefaultIx> = graph.add_node(node_id.clone());
        node_indices.insert(node_id.clone(), index);
    }

    let mut neighbour_graph = graph.clone();

    for node in seed_nodes.iter().chain(network.iter()) {
        let node_id = node.comms.node_identity().node_id().clone();

        let connected_peers = node
            .comms
            .connectivity()
            .select_connections(ConnectivitySelection::all_nodes(vec![]))
            .await
            .expect("Can't get connections");

        let node_index = node_indices.get(&node_id).expect("Can't find Node Index 1");
        for peer in connected_peers.iter() {
            let distance = node_id.distance(peer.peer_node_id()).get_bucket(25).2;
            let peer_node_index = node_indices.get(&peer.peer_node_id()).expect("Can't find Node Index 2");

            graph.add_edge(
                node_index.to_owned(),
                peer_node_index.to_owned(),
                distance.to_string()
            );
        }
        if let Some(n) = num_neighbours {
            let connected_neighbours = node
                .comms
                .connectivity()
                .select_connections(ConnectivitySelection::closest_to(node_id.clone(), n, vec![]))
                .await
                .expect("Can't get connections");

            let node_index = node_indices.get(&node_id).expect("Can't find Node Index 1");
            for neighbour in connected_neighbours.iter() {
                let distance = node_id.distance(neighbour.peer_node_id()).get_bucket(25).2;
                let peer_node_index = node_indices
                    .get(&neighbour.peer_node_id())
                    .expect("Can't find Node Index 2");

                neighbour_graph.add_edge(
                    node_index.to_owned(),
                    peer_node_index.to_owned(),
                    distance
                        .to_string(),
                );
            }
        }
    }

    let tmp_file_path = Path::new(TEMP_GRAPH_OUTPUT_DIR).join(name);

    let frame_num = get_next_frame_num(name);
    if frame_num == 0 {
        let path = tmp_file_path.to_str().expect("Can't clean output directory");

        let _ = fs::remove_dir_all(path);
        fs::create_dir_all(path).expect("Could not create temp graph directory");
    }

    let tmp_file_path_connections = tmp_file_path.join(format!("connections-{:03}.dot", frame_num));
    let mut file = File::create(tmp_file_path_connections).expect("Could not create dot file");
    file.write_all(Dot::new(&graph).to_string().as_bytes())
        .expect("Could not write dot file");

    if num_neighbours.is_some() {
        let tmp_file_path_neighbours = tmp_file_path.join(format!("neighbours-{:03}.dot", frame_num));
        let mut file = File::create(tmp_file_path_neighbours).expect("Could not create dot file");
        file.write_all(Dot::new(&neighbour_graph).to_string().as_bytes())
            .expect("Could not write dot file");
    }

    (graph, neighbour_graph)
}

/// Run the Python visualization script that will render the graphs stored in the temporary directory for the named
/// sequence. The rendered graphs will be saved in a named subdirectory in the provided output directory
pub fn run_python_network_graph_render(
    name: &str,
    output_dir: &str,
    graph_type: PythonRenderType,
) -> Result<(), String>
{
    let temp_path = Path::new(TEMP_GRAPH_OUTPUT_DIR).join(name);
    let tmp_file_path = match temp_path.to_str() {
        None => return Err("Could not parse temp file directory".to_string()),
        Some(p) => p,
    };

    let output_path = Path::new(output_dir).join(name);
    let output_file_path = match output_path.to_str() {
        None => return Err("Could not parse temp file directory".to_string()),
        Some(p) => p,
    };

    let plot_full_network = (graph_type == PythonRenderType::NetworkGraphFull ||
        graph_type == PythonRenderType::NetworkGraphOnlyConnections)
        .to_string();
    let plot_full_neighbours = (graph_type == PythonRenderType::NetworkGraphFull ||
        graph_type == PythonRenderType::NetworkGraphOnlyNeighbours)
        .to_string();

    let arguments = match graph_type {
        PythonRenderType::Propagation => vec![
            "./comms/dht/examples/graphing_utilities/render_graph_sequence_propagation.py",
            tmp_file_path,
            output_file_path,
        ],

        _ => vec![
            "./comms/dht/examples/graphing_utilities/render_graph_sequence.py",
            tmp_file_path,
            output_file_path,
            plot_full_network.as_str(),
            plot_full_neighbours.as_str(),
        ],
    };

    let result = Command::new("python3")
        .args(arguments)
        .spawn()
        .map_err(|_| "Could not execute Python command".to_string())?;
    let output = result
        .wait_with_output()
        .map_err(|e| format!("Python command did not complete: {}", e))?;
    match output.status.code() {
        Some(0) => Ok(()),
        _ => Err(format!(
            "stdout: {}, stderr:{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )),
    }
}

/// This function will take a starting network layout and a message propagation tree and then plot the propagations as a
/// breadth first traversal of the message propagation tree on top of the network layout
/// *IMPORTANT* You must have called `network_graph_snapshot(...)` for the network_graph using the same name you used as
/// the network_graph
pub async fn create_message_propagation_graphs(
    name: &str,
    mut network_graph: StableGraph<NodeId, String>,
    message_tree: StableGraph<NodeId, String>,
)
{
    let mut bfs = Bfs::new(&message_tree, NodeIndex::new(0));

    network_graph.clear_edges();

    let tmp_file_path = Path::new(TEMP_GRAPH_OUTPUT_DIR).join(name);
    let mut hop = 0;
    while let Some(visited) = bfs.next(&message_tree) {
        let neighbours = message_tree.neighbors(visited);
        let from_index = network_graph
            .node_references()
            .find_map(|(index, weight)| {
                if weight == &message_tree[visited] {
                    Some(index)
                } else {
                    None
                }
            })
            .expect("Should be able to find node");

        for n in neighbours {
            let to_index = network_graph
                .node_references()
                .find_map(
                    |(index, weight)| {
                        if weight == &message_tree[n] {
                            Some(index)
                        } else {
                            None
                        }
                    },
                )
                .expect("Should be able to find node2");
            network_graph.add_edge(from_index, to_index, "".to_string());
        }
        let current_file_path = tmp_file_path.join(format!("hop-{:03}.dot", hop));
        hop += 1;
        let mut file = File::create(current_file_path).expect("Could not create dot file");
        file.write_all(Dot::new(&network_graph).to_string().as_bytes())
            .expect("Could not write dot file");
        network_graph.clear_edges();
    }
}

#[derive(PartialEq)]
pub enum PythonRenderType {
    NetworkGraphFull,
    NetworkGraphOnlyConnections,
    NetworkGraphOnlyNeighbours,
    Propagation,
}

/// This function will drain the message event queue and then build a message propagation tree assuming the first sender
/// is the starting node
pub async fn track_join_message_drain_messaging_events(messaging_rx: &mut NodeEventRx) -> StableGraph<NodeId, String> {
    let drain_fut = DrainBurst::new(messaging_rx);

    let messages = drain_fut.await;
    let num_messages = messages.len();

    let mut graph = StableGraph::new();
    let mut node_indices: HashMap<NodeId, NodeIndex> = HashMap::new();

    for (from_node, to_node) in &messages {
        if !node_indices.contains_key(from_node) {
            let index = graph.add_node(from_node.clone());
            let _ = node_indices.insert(from_node.clone(), index);
        }
        if !node_indices.contains_key(to_node) {
            let index = graph.add_node(to_node.clone());
            let _ = node_indices.insert(to_node.clone(), index);
        }
    }

    for (from_node, to_node) in &messages {
        let from_index = *node_indices.get(from_node).unwrap();
        let to_index = *node_indices.get(to_node).unwrap();
        graph.update_edge(from_index, to_index, "".to_string());
    }

    // Print the tree using a Breadth first traversal
    let mut bfs = Bfs::new(&graph, NodeIndex::new(0));

    while let Some(visited) = bfs.next(&graph) {
        let neighbours = graph.neighbors(visited);
        let neighbour_names: Vec<String> = neighbours.map(|n| get_short_name(&graph[n])).collect();
        print!(
            "{} sent {} messages to ",
            get_short_name(&graph[visited]),
            neighbour_names.len(),
        );
        println!("{}", neighbour_names.join(", "));
    }

    println!("{} messages sent between nodes", num_messages);

    graph
}
