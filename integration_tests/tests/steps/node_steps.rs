//   Copyright 2023. The Tari Project
//
//   Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//   following conditions are met:
//
//   1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//   disclaimer.
//
//   2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//   following disclaimer in the documentation and/or other materials provided with the distribution.
//
//   3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//   products derived from this software without specific prior written permission.
//
//   THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//   INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//   DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//   SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//   SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//   WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//   USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

#![allow(clippy::indexing_slicing)]
#![allow(clippy::cast_possible_truncation)]
// Overflow in test code panics, which is the desired failure mode for a test.
#![allow(clippy::arithmetic_side_effects)]
use std::{
    convert::{TryFrom, TryInto},
    time::Duration,
};

use cucumber::{given, then, when};
use futures::StreamExt;
use indexmap::IndexMap;
use minotari_app_grpc::tari_rpc::{
    self as grpc,
    GetBlocksRequest,
    GetNewBlockTemplateWithCoinbasesRequest,
    GetNewBlockWithCoinbasesRequest,
    ListHeadersRequest,
    NewBlockCoinbase,
    NewBlockTemplateRequest,
    PowAlgo,
    pow_algo::PowAlgos,
};
use minotari_node::BaseNodeConfig;
use minotari_wallet_grpc_client::grpc::Empty;
use tari_common_types::tari_address::TariAddress;
use tari_integration_tests::{
    DEFAULT_TIMEOUT,
    TariWorld,
    base_node_process::{spawn_base_node, spawn_base_node_with_config},
    get_peer_addresses,
    miner::mine_block_before_submit,
    wait_for,
};
use tari_node_components::blocks::Block;
use tari_transaction_components::{
    aggregated_body::AggregateBody,
    helpers::borsh::SerializedSize,
    weight::TransactionWeight,
};

#[given(expr = "I have a seed node {word}")]
#[when(expr = "I have a seed node {word}")]
async fn start_base_node(world: &mut TariWorld, name: String) {
    spawn_base_node(world, true, name, vec![]).await;
}

#[given(expr = "I have a base node {word} connected to all seed nodes")]
#[when(expr = "I have a base node {word} connected to all seed nodes")]
async fn start_base_node_connected_to_all_seed_nodes(world: &mut TariWorld, name: String) {
    spawn_base_node(world, false, name, world.all_seed_nodes().to_vec()).await;
}

#[given(expr = "I start base node {word}")]
#[when(expr = "I start base node {word}")]
async fn start_base_node_step(world: &mut TariWorld, name: String) {
    let mut is_seed_node = false;
    let mut seed_nodes = world.all_seed_nodes().to_vec();
    if let Some(node_ps) = world.base_nodes.get(&name) {
        is_seed_node = node_ps.is_seed_node;
        seed_nodes = node_ps.seed_nodes.clone();
    }
    spawn_base_node(world, is_seed_node, name, seed_nodes).await;
}

#[given(expr = "I have {int} base nodes connected to all seed nodes")]
#[when(expr = "I have {int} base nodes connected to all seed nodes")]
async fn multiple_base_nodes_connected_to_all_seeds(world: &mut TariWorld, nodes: u64) {
    for i in 0..nodes {
        let node = format!("Node_{i}");
        println!("Initializing node {}", node.clone());
        spawn_base_node(world, false, node, world.all_seed_nodes().to_vec()).await;
    }
}

#[when(expr = "I wait for base node {word} to connect to base node {word}")]
#[then(expr = "I wait for base node {word} to connect to base node {word}")]
async fn base_node_pending_connection_to(world: &mut TariWorld, first_node: String, second_node: String) {
    let mut node_client = world.get_node_client(&first_node).await.unwrap();
    let mut second_client = world.get_node_client(&second_node).await.unwrap();

    let second_client_pubkey = second_client.identify(Empty {}).await.unwrap().into_inner().public_key;

    wait_for!(
        timeout: DEFAULT_TIMEOUT,
        description: format!("base node {first_node} to connect to {second_node}"),
        condition: async {
            let res = node_client.list_connected_peers(Empty {}).await.unwrap().into_inner();
            Ok(res.connected_peers.iter().any(|p| p.public_key == second_client_pubkey))
        }
    );
}

#[when(expr = "I wait base node for {word} to have {int} base node connections")]
async fn wait_for_node_have_x_connections(world: &mut TariWorld, node: String, num_connections: usize) {
    let mut node_client = world.get_node_client(&node).await.unwrap();

    wait_for!(
        timeout: DEFAULT_TIMEOUT,
        description: format!("node {node} to have {num_connections} connections"),
        condition: async {
            let res = node_client.list_connected_peers(Empty {}).await.unwrap().into_inner();
            let count = res.connected_peers.len();
            if count >= num_connections {
                Ok(true)
            } else {
                Err(format!("connected to {count} peers"))
            }
        }
    );
}

#[then(expr = "all nodes are on the same chain at height {int}")]
async fn all_nodes_on_same_chain_at_height(world: &mut TariWorld, height: u64) {
    let mut nodes_at_height: IndexMap<&String, (u64, Vec<u8>)> = IndexMap::new();

    for (name, _) in &world.base_nodes {
        nodes_at_height.insert(name, (0, vec![]));
    }

    wait_for!(
        timeout: DEFAULT_TIMEOUT,
        description: format!("all nodes to synchronize at chain height {height}"),
        condition: async {
            for (name, _) in nodes_at_height
                .clone()
                .iter()
                .filter(|(_, (at_height, _))| at_height != &height)
            {
                let mut client = world.get_node_client(name).await.unwrap();
                let chain_tip = client.get_tip_info(Empty {}).await.unwrap().into_inner();
                let metadata = chain_tip.metadata.unwrap();
                nodes_at_height.insert(name, (metadata.best_block_height, metadata.best_block_hash));
            }

            let all_synced = nodes_at_height
                .values()
                .all(|(h, block_hash)| h == &height && block_hash == &nodes_at_height.values().last().unwrap().1);
            if all_synced {
                Ok(true)
            } else {
                Err(format!("{nodes_at_height:?}"))
            }
        }
    );
}

#[then(expr = "all nodes are on the same network difficulty")]
async fn all_nodes_on_same_network_difficulty(world: &mut TariWorld) {
    let mut all_nodes_metadata = Vec::new();
    for (name, _) in &world.base_nodes {
        let mut client = world.get_node_client(name).await.unwrap();
        let chain_tip = client.get_tip_info(Empty {}).await.unwrap().into_inner();
        all_nodes_metadata.push(chain_tip.metadata.unwrap());
    }
    let first_metadata = all_nodes_metadata.first().unwrap();
    if all_nodes_metadata
        .iter()
        .any(|v| v.best_block_height != first_metadata.best_block_height)
    {
        panic!("base nodes not successfully synchronized at the same height");
    }
    if all_nodes_metadata
        .iter()
        .any(|v| v.accumulated_difficulty != first_metadata.accumulated_difficulty)
    {
        panic!("base nodes synchronized at the same height do all not have the same accumulated difficulty");
    }
}

#[then(expr = "all nodes are at height {int}")]
#[when(expr = "all nodes are at height {int}")]
async fn all_nodes_are_at_height(world: &mut TariWorld, height: u64) {
    let mut nodes_at_height: IndexMap<&String, u64> = IndexMap::new();

    for (name, _) in &world.base_nodes {
        nodes_at_height.insert(name, 0);
    }

    // Use a generous timeout (was ~14 minutes originally)
    wait_for!(
        timeout: Duration::from_secs(840),
        description: format!("all nodes to reach height {height}"),
        condition: async {
            for (name, _) in nodes_at_height
                .clone()
                .iter()
                .filter(|(_, at_height)| at_height != &&height)
            {
                let Ok(mut client) = world.get_node_client(name).await else {
                    continue;
                };
                let Ok(chain_tip) = client.get_tip_info(Empty {}).await else {
                    continue;
                };
                let chain_hgt = chain_tip.into_inner().metadata.unwrap().best_block_height;
                nodes_at_height.insert(name, chain_hgt);
            }

            if nodes_at_height.values().all(|h| h == &height) {
                Ok(true)
            } else {
                Err(format!("{nodes_at_height:?}"))
            }
        }
    );
}

#[when(expr = "node {word} is at height {int}")]
#[then(expr = "node {word} is at height {int}")]
async fn node_is_at_height(world: &mut TariWorld, base_node: String, height: u64) {
    let mut client = world.get_node_client(&base_node).await.unwrap();

    // Use a generous timeout — this step is used for reorg scenarios where peer discovery +
    // chain sync can take significant time, especially on slow CI machines.
    // Use 1s max poll interval to detect height changes promptly during sync.
    wait_for!(
        timeout: Duration::from_secs(300),
        max_interval: Duration::from_secs(1),
        description: format!("node {base_node} to reach height {height}"),
        condition: async {
            let chain_tip = client.get_tip_info(Empty {}).await.unwrap().into_inner();
            let chain_hgt = chain_tip.metadata.unwrap().best_block_height;
            if chain_hgt >= height {
                Ok(true)
            } else {
                Err(format!("current height {chain_hgt}"))
            }
        }
    );
}

/// The most blocks a healthy pruned node may lag its own pruning horizon by.
///
/// `prune_database_if_needed` only prunes when
/// `pruned_height < (tip - pruning_horizon) - pruning_interval`, and the integration harness sets
/// `pruning_interval = 1` for pruned nodes. That gate therefore fires on alternating blocks: after
/// a prune the node sits at `tip - horizon` (lag `horizon`), the next block fails the gate (lag
/// `horizon + 1`), and the block after that prunes again. So propagation alone keeps the lag in
/// `horizon..=horizon + 1`.
///
/// `horizon + 1` is therefore the derived bound, and the second block is empirical margin only — it
/// is deliberately not attributed to any particular mechanism. Do not read it as covering one
/// missed prune: see the PRECONDITION on `pruned_height_within_horizon` for the one known way the
/// lag can exceed the derived bound, whose magnitude a `+1` would not bound anyway.
///
/// NOTE: this is deliberately a constant rather than being read from the node's config.
/// `BaseNodeProcess.config` is cloned before the spawn task overrides `pruning_interval` to 1, so
/// reading `pruning_interval` back would yield the production default of 50 and a nonsense bound.
/// `pruning_horizon` is never mutated after that clone, which is why reading *it* is sound.
const MAX_PRUNE_LAG_SLACK: u64 = 2;

/// Assert that a pruned node has pruned up to (approximately) its pruning horizon.
///
/// Asserting an *exact* pruned height is not reproducible. Because of the alternating gate
/// described on [`MAX_PRUNE_LAG_SLACK`], which heights are reachable at all depends on how the
/// blocks arrived: the block-sync loop does not prune per block, so a node that falls behind and
/// catches up lands on the opposite parity and can never reach the height a propagation-only run
/// would have reached.
///
/// What *is* stable is the band. This asserts both edges of it:
/// - lower: `tip - pruned_height <= horizon + MAX_PRUNE_LAG_SLACK`, i.e. the node pruned up to its horizon and kept
///   doing so. This is the substance of the test — it rejects a regression where pruning runs once and then stalls,
///   which a bare `pruned_height > 0` check would let through.
/// - upper: `pruned_height <= tip - horizon`, i.e. it never pruned away history the horizon says it must keep. This is
///   near-tautological today (`prune_to_height` is always called with exactly that target) but is a cheap guard against
///   a future change that prunes too aggressively.
///
/// The horizon is read from the node's own configuration, so the feature file carries no magic
/// numbers.
///
/// PRECONDITION: the caller must have already pinned this node at the network tip, and left the
/// network tip stationary (e.g. with an "all nodes are on the same chain at height N" step, mining
/// no further blocks until this step returns). Both bounds depend on it:
///
/// - lower: `DecideNextSync` prunes to `best_peer_tip - horizon` taken from the sync peer's *claimed* metadata, which
///   is frozen at the Listening-time liveness ping (`SyncPeer` exposes no setter for it). If the peer keeps mining
///   while the sync is in flight, header sync carries the local tip past that target and the lag settles at `horizon +
///   n`, where `n` is however many blocks the peer advanced during the sync window. That is unbounded, so
///   [`MAX_PRUNE_LAG_SLACK`] does not cover it — a stationary peer tip does, by making `n` zero.
/// - upper: measured against the node's *own* tip, while `DecideNextSync` prunes relative to the *peer* tip, which can
///   transiently sit above `local_tip - horizon`.
///
/// Reusing this step while the node is still behind, or while blocks are still being mined, could
/// therefore fail either bound spuriously.
#[then(expr = "node {word} has a pruned height within its pruning horizon")]
async fn pruned_height_within_horizon(world: &mut TariWorld, node: String) {
    let pruning_horizon = world.get_node(&node).unwrap().config.storage.pruning_horizon;
    assert!(
        pruning_horizon > 0,
        "node {node} is not a pruned node (pruning horizon is 0), so it will never prune"
    );
    let max_lag = pruning_horizon + MAX_PRUNE_LAG_SLACK;

    let mut client = world.get_node_client(&node).await.unwrap();

    // Phase 1: wait for the node to prune up to its horizon. Pruning is driven by block arrival, so
    // it can trail the tip for a while before settling into the band.
    wait_for!(
        timeout: DEFAULT_TIMEOUT,
        description: format!(
            "node {node} to prune to within {max_lag} blocks of its tip (pruning horizon {pruning_horizon})"
        ),
        condition: async {
            let metadata = client.get_tip_info(Empty {}).await.unwrap().into_inner().metadata.unwrap();
            let tip = metadata.best_block_height;
            let pruned_height = metadata.pruned_height;
            let lag = tip.saturating_sub(pruned_height);
            if pruned_height > 0 && lag <= max_lag {
                Ok(true)
            } else {
                Err(format!("pruned height {pruned_height} at tip {tip} (lag {lag}, want <= {max_lag})"))
            }
        }
    );

    // Phase 2: the node must not have pruned beyond its horizon. Both values come from a single
    // metadata snapshot, so they are consistent with one another.
    let metadata = client
        .get_tip_info(Empty {})
        .await
        .unwrap()
        .into_inner()
        .metadata
        .unwrap();
    let tip = metadata.best_block_height;
    let pruned_height = metadata.pruned_height;
    let max_prunable = tip.saturating_sub(pruning_horizon);
    assert!(
        pruned_height <= max_prunable,
        "node {node} pruned past its horizon: pruned height {pruned_height} exceeds {max_prunable} (tip {tip} - \
         pruning horizon {pruning_horizon})"
    );
}

#[given(expr = "I have a base node {word} connected to seed {word}")]
#[when(expr = "I have a base node {word} connected to seed {word}")]
async fn base_node_connected_to_seed(world: &mut TariWorld, base_node: String, seed: String) {
    spawn_base_node(world, false, base_node, vec![seed]).await;
}

#[when(expr = "I have a base node {word}")]
#[given(expr = "I have a base node {word}")]
async fn create_and_add_base_node(world: &mut TariWorld, base_node: String) {
    spawn_base_node(world, false, base_node, vec![]).await;
}

#[given(expr = "I have {int} seed nodes")]
async fn have_seed_nodes(world: &mut TariWorld, seed_nodes: u64) {
    for node in 0..seed_nodes {
        spawn_base_node(world, true, format!("seed_node_{node}"), vec![]).await;
    }
}

#[then(expr = "{word} has {word} in {word} state")]
async fn transaction_in_state(
    world: &mut TariWorld,
    node: String,
    tx_name: String,
    state: String,
) -> anyhow::Result<()> {
    let mut client = world.get_node_client(&node).await?;
    let tx = world
        .transactions
        .get(&tx_name)
        .unwrap_or_else(|| panic!("Couldn't find transaction {tx_name}"));
    let sig = &tx.body.kernels()[0].excess_sig;
    let state_clone = state.clone();
    wait_for!(
        timeout: Duration::from_secs(240),
        description: format!("node {node} to have tx {tx_name} in state {state}"),
        condition: async {
            let resp = client
                .transaction_state(grpc::TransactionStateRequest {
                    excess_sig: Some(sig.into()),
                })
                .await
                .map_err(|e| format!("gRPC error: {e}"))?;

            let inner = resp.into_inner();
            let current_state = match inner.result {
                0 => "UNKNOWN",
                1 => "MEMPOOL",
                2 => "MINED",
                3 => "NOT_STORED",
                _ => panic!("not getting a good result"),
            };

            if current_state == state_clone {
                Ok(true)
            } else {
                Err(format!("current state: {current_state}"))
            }
        }
    );
    Ok(())
}

#[then(expr = "I wait until base node {word} has {int} unconfirmed transactions in its mempool")]
async fn base_node_has_unconfirmed_transaction_in_mempool(world: &mut TariWorld, node: String, num_transactions: u64) {
    let mut client = world.get_node_client(&node).await.unwrap();

    wait_for!(
        timeout: DEFAULT_TIMEOUT,
        description: format!("node {node} to have {num_transactions} unconfirmed txs in mempool"),
        condition: async {
            let resp = client.get_mempool_stats(Empty {}).await.unwrap();
            let inner = resp.into_inner();
            if inner.unconfirmed_txs == num_transactions {
                Ok(true)
            } else {
                Err(format!("has {} unconfirmed txs", inner.unconfirmed_txs))
            }
        }
    );
}

#[then(expr = "{word} is in the {word} of all nodes")]
async fn tx_in_state_all_nodes(world: &mut TariWorld, tx_name: String, pool: String) -> anyhow::Result<()> {
    tx_in_state_all_nodes_with_allowed_failure(world, tx_name, pool, 0).await
}
// casting is okay in tests
#[allow(clippy::cast_possible_truncation)]
#[then(expr = "{word} is in the {word} of all nodes, where {int}% can fail")]
async fn tx_in_state_all_nodes_with_allowed_failure(
    world: &mut TariWorld,
    tx_name: String,
    pool: String,
    can_fail_percent: u64,
) -> anyhow::Result<()> {
    let tx = world
        .transactions
        .get(&tx_name)
        .unwrap_or_else(|| panic!("Couldn't find transaction {tx_name}"));
    let sig = &tx.body.kernels()[0].excess_sig;

    let mut node_pool_status: IndexMap<&String, &str> = IndexMap::new();

    let nodes = world.base_nodes.iter().clone();
    let nodes_count = world.base_nodes.len();

    for (name, _) in nodes.clone() {
        node_pool_status.insert(name, "UNCHECKED: DEFAULT TEST STATE");
    }

    let can_fail = ((can_fail_percent as f64 * nodes.len() as f64) / 100.0).ceil() as u64;
    let pool_clone = pool.clone();

    wait_for!(
        timeout: DEFAULT_TIMEOUT,
        description: format!("{tx_name} to be in {pool} of all nodes (allowing {can_fail_percent}% failure)"),
        condition: async {
            for (name, _) in node_pool_status
                .clone()
                .iter()
                .filter(|(_, in_pool)| ***in_pool != pool_clone)
            {
                let mut client = world.get_node_client(name).await.map_err(|e| e.to_string())?;

                let resp = client
                    .transaction_state(grpc::TransactionStateRequest {
                        excess_sig: Some(sig.into()),
                    })
                    .await
                    .map_err(|e| e.to_string())?;

                let inner = resp.into_inner();
                let res_state = match inner.result {
                    0 => "UNKNOWN",
                    1 => "MEMPOOL",
                    2 => "MINED",
                    3 => "NOT_STORED",
                    _ => panic!("not getting a good result"),
                };

                node_pool_status.insert(name, res_state);
            }

            if node_pool_status.values().filter(|v| ***v == pool_clone).count() >= (nodes_count - can_fail as usize) {
                Ok(true)
            } else {
                Err(format!("{node_pool_status:?}"))
            }
        }
    );
    Ok(())
}

#[then(expr = "I submit transaction {word} to {word}")]
#[when(expr = "I submit transaction {word} to {word}")]
pub async fn submit_transaction_to(world: &mut TariWorld, tx_name: String, node: String) -> anyhow::Result<()> {
    let mut client = world.get_node_client(&node).await?;
    let tx = world
        .transactions
        .get(&tx_name)
        .unwrap_or_else(|| panic!("Couldn't find transaction {tx_name}"));
    let resp = client
        .submit_transaction(grpc::SubmitTransactionRequest {
            transaction: Some(grpc::Transaction::try_from(tx.clone()).unwrap()),
        })
        .await?;

    let result = resp.into_inner();

    if result.result == 1 {
        Ok(())
    } else {
        panic!("Transaction {tx_name} wasn't submit to {node}")
    }
}

#[when(expr = "I submit transaction {word} to {word} and it does not succeed")]
pub async fn submit_failed_transaction_to(world: &mut TariWorld, tx_name: String, node: String) -> anyhow::Result<()> {
    let mut client = world.get_node_client(&node).await?;
    let tx = world
        .transactions
        .get(&tx_name)
        .unwrap_or_else(|| panic!("Couldn't find transaction {tx_name}"));
    let resp = client
        .submit_transaction(grpc::SubmitTransactionRequest {
            transaction: Some(grpc::Transaction::try_from(tx.clone()).unwrap()),
        })
        .await?;

    let result = resp.into_inner();

    if result.result == 1 {
        panic!("Transaction {tx_name} was submitted, but should not have been to {node}")
    } else {
        Ok(())
    }
}

#[when(expr = "I have a pruned node {word} connected to node {word} with pruning horizon set to {int}")]
#[given(expr = "I have a pruned node {word} connected to node {word} with pruning horizon set to {int}")]
async fn prune_node_connected_to_base_node(
    world: &mut TariWorld,
    pruned_node: String,
    base_node: String,
    pruning_horizon: u64,
) {
    let mut base_node_config = BaseNodeConfig::default();
    base_node_config.storage.pruning_horizon = pruning_horizon;
    let peers = vec![base_node.clone()];
    base_node_config.force_sync_peers = get_peer_addresses(world, &peers).await.into();

    spawn_base_node_with_config(world, false, pruned_node, peers, base_node_config).await;
}

#[when(expr = "I have a base node {word} connected to node {word}")]
async fn base_node_connected_to_node(world: &mut TariWorld, base_node: String, peer_node: String) {
    spawn_base_node(world, false, base_node, vec![peer_node]).await;
}

#[when(expr = "I have a base node {word} connected to nodes {word}")]
async fn base_node_connected_to_nodes(world: &mut TariWorld, base_node: String, nodes: String) {
    let nodes = nodes.split(',').map(|s| s.to_string()).collect::<Vec<String>>();
    spawn_base_node(world, false, base_node, nodes).await;
}

#[then(expr = "node {word} is in state {word}")]
async fn node_state(world: &mut TariWorld, node_name: String, state: String) {
    let expected_state = match state.as_str() {
        "START_UP" => 0,
        "HEADER_SYNC" => 1,
        "HORIZON_SYNC" => 2,
        "CONNECTING" => 3,
        "BLOCK_SYNC" => 4,
        "LISTENING" => 5,
        "SYNC_FAILED" => 6,
        _ => panic!("Invalid state"),
    };
    let mut node_client = world.get_node_client(&node_name).await.unwrap();

    wait_for!(
        timeout: DEFAULT_TIMEOUT,
        description: format!("node {node_name} to reach state {state}"),
        condition: async {
            let tip = node_client.get_tip_info(Empty {}).await.unwrap().into_inner();
            if tip.base_node_state == expected_state {
                Ok(true)
            } else {
                Err(format!("current state: {}", tip.base_node_state))
            }
        }
    );
}

#[then(expr = "node {word} is at the same height as node {word}")]
async fn base_node_is_at_same_height_as_node(world: &mut TariWorld, base_node: String, peer_node: String) {
    let mut peer_node_client = world.get_node_client(&peer_node).await.unwrap();
    let req = Empty {};
    let mut expected_height = peer_node_client
        .get_tip_info(req)
        .await
        .unwrap()
        .into_inner()
        .metadata
        .unwrap()
        .best_block_height;

    let mut base_node_client = world.get_node_client(&base_node).await.unwrap();

    wait_for!(
        timeout: Duration::from_secs(600),
        description: format!("node {base_node} to reach same height as node {peer_node}"),
        condition: async {
            expected_height = peer_node_client
                .get_tip_info(req)
                .await
                .unwrap()
                .into_inner()
                .metadata
                .unwrap()
                .best_block_height;

            let current_height = base_node_client
                .get_tip_info(req)
                .await
                .unwrap()
                .into_inner()
                .metadata
                .unwrap()
                .best_block_height;

            if current_height >= expected_height {
                Ok(true)
            } else {
                Err(format!("current {current_height}, expected {expected_height}"))
            }
        }
    );
    println!("Base node {base_node} is at the same height as node {peer_node}");
}

#[given(expr = "I stop node {word}")]
#[when(expr = "I stop node {word}")]
#[then(expr = "I stop node {word}")]
async fn stop_node(world: &mut TariWorld, node: String) {
    let base_ps = world.base_nodes.get_mut(&node).unwrap();
    println!("Stopping node {node}");
    base_ps.kill().await;
}

#[then(expr = "node {word} lists heights {int} to {int}")]
async fn node_lists_heights(world: &mut TariWorld, node: String, start: u64, end: u64) {
    let mut node_client = world.get_node_client(&node).await.unwrap();
    let heights = (start..=end).collect::<Vec<_>>();
    let blocks_req = GetBlocksRequest { heights };
    let mut blocks_stream = node_client.get_blocks(blocks_req).await.unwrap().into_inner();

    let mut height = start;
    while let Some(block) = blocks_stream.next().await {
        let block = block.unwrap().block.unwrap();
        let block_height = block.header.unwrap().height;
        if height != block_height {
            panic!(
                "Invalid block height for node {}: expected height {} != current height {}",
                node, block_height, height
            );
        }
        println!("Valid block height {}, listed by node {}", height, node);
        height += 1;
    }
}

#[then(expr = "node {word} lists headers {int} to {int} with correct heights")]
async fn node_lists_headers_with_correct_heights(world: &mut TariWorld, node: String, start: u64, end: u64) {
    let mut node_client = world.get_node_client(&node).await.unwrap();
    let list_headers_req = ListHeadersRequest {
        from_height: start,
        num_headers: end - start + 1,
        sorting: 1,
    };
    let mut headers_stream = node_client.list_headers(list_headers_req).await.unwrap().into_inner();

    let mut height = start;
    while let Some(header) = headers_stream.next().await {
        let header_res = header.unwrap();
        let header_height = header_res.header.unwrap().height;

        if header_height != height {
            panic!(
                "incorrect listing of height headers by node {}: expected height to be {} but got height {}",
                node, height, header_height
            );
        }
        println!("correct listing of height header {} by node {}", height, node);
        height += 1;
    }
}

#[then(expr = "all nodes are at height {int}*{int}")]
#[when(expr = "all nodes are at height {int}*{int}")]
async fn all_nodes_are_at_product_height(world: &mut TariWorld, a: u64, b: u64) {
    all_nodes_are_at_height(world, a * b).await;
}

#[when(expr = "I connect node {word} to node {word}")]
async fn connect_node_to_other_node(world: &mut TariWorld, node_a: String, node_b: String) {
    let node_a_ps = world.base_nodes.get_mut(&node_a).unwrap();
    let mut node_a_peers = node_a_ps.seed_nodes.clone();
    let is_seed_node = node_a_ps.is_seed_node;
    node_a_peers.push(node_b);
    node_a_ps.kill().await;
    spawn_base_node(world, is_seed_node, node_a, node_a_peers).await;
}

#[then(expr = "meddling with block template data from node {word} is not allowed")]
async fn no_meddling_with_data(world: &mut TariWorld, node: String) {
    let mut client = world.get_node_client(&node).await.unwrap();

    // No meddling
    let chain_tip = client.get_tip_info(Empty {}).await.unwrap().into_inner();
    let current_height = chain_tip.metadata.unwrap().best_block_height;
    let script_key_id = &world.script_key_id().await;
    let block = mine_block_before_submit(
        &mut client,
        &world.key_manager,
        script_key_id,
        &world.default_payment_address.clone(),
        false,
        &world.consensus_manager.clone(),
    )
    .await;
    let _sumbmit_res = client.submit_block(block).await.unwrap();

    let chain_tip = client.get_tip_info(Empty {}).await.unwrap().into_inner();
    let new_height = chain_tip.metadata.unwrap().best_block_height;
    assert_eq!(
        current_height + 1,
        new_height,
        "validating that the chain increased by 1 from {} to {} but was actually {}",
        current_height,
        current_height + 1,
        new_height
    );

    // Meddle with kernal_mmr_size
    let script_key_id = &world.script_key_id().await;
    let mut block: Block = Block::try_from(
        mine_block_before_submit(
            &mut client,
            &world.key_manager,
            script_key_id,
            &world.default_payment_address.clone(),
            false,
            &world.consensus_manager.clone(),
        )
        .await,
    )
    .unwrap();
    block.header.kernel_mmr_size += 1;
    match client.submit_block(grpc::Block::try_from(block).unwrap()).await {
        Ok(_) => panic!("The block should not have been valid"),
        Err(e) => assert_eq!(
            "Chain storage error: Validation error: Block validation error: MMR size for Kernel does not match. \
             Expected: 2, received: 3"
                .to_string(),
            e.message()
        ),
    }

    // Meddle with output_mmr_size
    let script_key_id = &world.script_key_id().await;
    let mut block: Block = Block::try_from(
        mine_block_before_submit(
            &mut client,
            &world.key_manager,
            script_key_id,
            &world.default_payment_address.clone(),
            false,
            &world.consensus_manager.clone(),
        )
        .await,
    )
    .unwrap();
    block.header.output_smt_size += 1;
    match client.submit_block(grpc::Block::try_from(block).unwrap()).await {
        Ok(_) => panic!("The block should not have been valid"),
        Err(e) => assert_eq!(
            "Chain storage error: Validation error: Block validation error: MMR size for UTXO does not match. \
             Expected: 2, received: 3"
                .to_string(),
            e.message()
        ),
    }
}

#[then(expr = "I generate a block {word} with {int} coinbases from node {word} for wallet {word}")]
async fn generate_block_with_many_coinbases(
    world: &mut TariWorld,
    block_name: String,
    number_of_coinbases: u64,
    node_name: String,
    wallet_name: String,
) {
    let mut client = world.get_node_client(&node_name).await.unwrap();
    let wallet_address = world.get_wallet_address(&wallet_name).await.unwrap();

    let template_req = NewBlockTemplateRequest {
        algo: Some(PowAlgo {
            pow_algo: PowAlgos::Sha3x.into(),
        }),
        max_weight: 0,
    };
    let template_response = client.get_new_block_template(template_req).await.unwrap().into_inner();
    let miner_data = template_response.miner_data.unwrap();

    let mut coinbases = Vec::with_capacity(usize::try_from(number_of_coinbases).unwrap());
    let mut value = 0;
    for i in 0..number_of_coinbases {
        let share_value = if i == number_of_coinbases - 1 {
            miner_data.reward - value
        } else {
            miner_data.reward / number_of_coinbases
        };
        coinbases.push(NewBlockCoinbase {
            address: wallet_address.clone(),
            value: share_value,
            stealth_payment: true,
            revealed_value_proof: true,
            coinbase_extra: Vec::new(),
        });
        value += share_value;
    }

    let template_req = GetNewBlockTemplateWithCoinbasesRequest {
        algo: Some(PowAlgo {
            pow_algo: PowAlgos::Sha3x.into(),
        }),
        max_weight: 0,
        coinbases,
    };

    let template_response = client
        .get_new_block_template_with_coinbases(template_req)
        .await
        .unwrap()
        .into_inner();
    let new_block = template_response.block.clone().unwrap();

    let block = Block::try_from(template_response.block.unwrap()).unwrap();
    let coinbase_outputs = block
        .body
        .outputs()
        .iter()
        .filter(|o| o.is_coinbase())
        .cloned()
        .collect::<Vec<_>>();
    let outputs = block
        .body
        .outputs()
        .iter()
        .filter(|o| !o.is_coinbase())
        .cloned()
        .collect::<Vec<_>>();
    let block_size = block.get_serialized_size().unwrap();

    println!(
        "Custom block: {}, size: {} bytes, coinbases: {}, kernels: {}, outputs: {}, inputs: {}, weight: {}",
        block.header.height,
        block_size,
        coinbase_outputs.len(),
        block.body.kernels().len(),
        outputs.len(),
        block.body.inputs().len(),
        block.body.calculate_weight(&TransactionWeight::latest()).unwrap(),
    );

    assert_eq!(coinbase_outputs.len() as u64, number_of_coinbases);

    match client.submit_block(new_block).await {
        Ok(_) => (),
        Err(e) => panic!("The block should have been valid, {e}"),
    }

    world.blocks.insert(block_name, block);
}

#[then(expr = "block {word} has serialized size at least {int} bytes")]
async fn block_has_serialized_size_greater_than(world: &mut TariWorld, block: String, size: u64) {
    let block = world.blocks.get(&block).unwrap();
    assert!(
        u64::try_from(block.get_serialized_size().unwrap()).unwrap() >= size,
        "Block {} with weight {} has serialized size of {} which is not at least {}",
        block.header.height,
        block.body.calculate_weight(&TransactionWeight::latest()).unwrap(),
        block.get_serialized_size().unwrap(),
        size,
    );
}

#[then(expr = "generate a block with 2 coinbases from node {word}")]
async fn generate_block_with_2_coinbases(world: &mut TariWorld, node: String) {
    let mut client = world.get_node_client(&node).await.unwrap();

    let template_req = NewBlockTemplateRequest {
        algo: Some(PowAlgo {
            pow_algo: PowAlgos::Sha3x.into(),
        }),
        max_weight: 0,
    };

    let template_response = client.get_new_block_template(template_req).await.unwrap().into_inner();

    let block_template = template_response.new_block_template.clone().unwrap();
    let miner_data = template_response.miner_data.unwrap();
    let amount = miner_data.reward + miner_data.total_fees;
    let request = GetNewBlockWithCoinbasesRequest {
        new_template: Some(block_template),
        coinbases: vec![
            NewBlockCoinbase {
                address: TariAddress::from_base58(
                    "f4L8GRWsXqz26DM3qAGErLtVknYzmTe2fYP2yKFn4biFXYJMP61W9MeD726QJ7ytWhRGyewTZzTzjZ7tEPskDptwRub",
                )
                .unwrap()
                .to_base58(),
                value: amount - 1000,
                stealth_payment: false,
                revealed_value_proof: true,
                coinbase_extra: Vec::new(),
            },
            NewBlockCoinbase {
                address: TariAddress::from_base58(
                    "f4HS8b64MDbvdaG5fiNgtsHhnoeCPaniS5M7iFuvEMDoyh9uikhWmYbnRtjdgHHVPjAXr7oSW61VSH5QvHU8jps1JXW",
                )
                .unwrap()
                .to_base58(),
                value: 1000,
                stealth_payment: false,
                revealed_value_proof: true,
                coinbase_extra: Vec::new(),
            },
        ],
    };

    let new_block = client.get_new_block_with_coinbases(request).await.unwrap().into_inner();

    let new_block = new_block.block.unwrap();
    let mut coinbase_kernel_count = 0;
    let mut coinbase_utxo_count = 0;
    let body: AggregateBody = new_block.body.clone().unwrap().try_into().unwrap();
    for kernel in body.kernels() {
        if kernel.is_coinbase() {
            coinbase_kernel_count += 1;
        }
    }
    for utxo in body.outputs() {
        if utxo.is_coinbase() {
            coinbase_utxo_count += 1;
        }
    }
    assert_eq!(coinbase_kernel_count, 1);
    assert_eq!(coinbase_utxo_count, 2);

    match client.submit_block(new_block).await {
        Ok(_) => (),
        Err(e) => panic!("The block should have been valid, {e}"),
    }
}

#[then(expr = "generate a block with 2 coinbases as a single request from node {word}")]
async fn generate_block_with_2_as_single_request_coinbases(world: &mut TariWorld, node: String) {
    let mut client = world.get_node_client(&node).await.unwrap();

    let template_req = GetNewBlockTemplateWithCoinbasesRequest {
        algo: Some(PowAlgo {
            pow_algo: PowAlgos::Sha3x.into(),
        }),
        max_weight: 0,
        coinbases: vec![
            NewBlockCoinbase {
                address: TariAddress::from_base58(
                    "f4L8GRWsXqz26DM3qAGErLtVknYzmTe2fYP2yKFn4biFXYJMP61W9MeD726QJ7ytWhRGyewTZzTzjZ7tEPskDptwRub",
                )
                .unwrap()
                .to_base58(),
                value: 1,
                stealth_payment: false,
                revealed_value_proof: true,
                coinbase_extra: Vec::new(),
            },
            NewBlockCoinbase {
                address: TariAddress::from_base58(
                    "f4HS8b64MDbvdaG5fiNgtsHhnoeCPaniS5M7iFuvEMDoyh9uikhWmYbnRtjdgHHVPjAXr7oSW61VSH5QvHU8jps1JXW",
                )
                .unwrap()
                .to_base58(),
                value: 2,
                stealth_payment: false,
                revealed_value_proof: true,
                coinbase_extra: Vec::new(),
            },
        ],
    };
    let new_block = client
        .get_new_block_template_with_coinbases(template_req)
        .await
        .unwrap()
        .into_inner();

    let new_block = new_block.block.unwrap();
    let mut coinbase_kernel_count = 0;
    let mut coinbase_utxo_count = 0;
    let body: AggregateBody = new_block.body.clone().unwrap().try_into().unwrap();
    for kernel in body.kernels() {
        if kernel.is_coinbase() {
            coinbase_kernel_count += 1;
        }
    }
    println!("{body}");
    for utxo in body.outputs() {
        if utxo.is_coinbase() {
            coinbase_utxo_count += 1;
        }
    }
    assert_eq!(coinbase_kernel_count, 1);
    assert_eq!(coinbase_utxo_count, 2);
    let mut num_6154266699 = 0;
    let mut num_12308533399 = 0;
    for output in body.outputs() {
        if output.minimum_value_promise.as_u64() == 6154266699 {
            num_6154266699 += 1;
        }
        if output.minimum_value_promise.as_u64() == 12308533399 {
            num_12308533399 += 1;
        }
    }

    assert_eq!(num_6154266699, 1);
    assert_eq!(num_12308533399, 1);

    match client.submit_block(new_block).await {
        Ok(_) => (),
        Err(e) => panic!("The block should have been valid, {e}"),
    }
}

#[then(expr = "generate a block with zero value coinbase as a single request from node {word}")]
async fn generate_block_as_single_request_with_zero_coinbase(world: &mut TariWorld, node: String) {
    let mut client = world.get_node_client(&node).await.unwrap();

    let template_req = GetNewBlockTemplateWithCoinbasesRequest {
        algo: Some(PowAlgo {
            pow_algo: PowAlgos::Sha3x.into(),
        }),
        max_weight: 0,
        coinbases: vec![NewBlockCoinbase {
            address: TariAddress::from_base58(
                "f4L8GRWsXqz26DM3qAGErLtVknYzmTe2fYP2yKFn4biFXYJMP61W9MeD726QJ7ytWhRGyewTZzTzjZ7tEPskDptwRub",
            )
            .unwrap()
            .to_base58(),
            value: 0,
            stealth_payment: false,
            revealed_value_proof: true,
            coinbase_extra: Vec::new(),
        }],
    };
    let new_block = client
        .get_new_block_template_with_coinbases(template_req)
        .await
        .unwrap()
        .into_inner();

    let new_block = new_block.block.unwrap();
    let mut coinbase_kernel_count = 0;
    let mut coinbase_utxo_count = 0;
    let body: AggregateBody = new_block.body.clone().unwrap().try_into().unwrap();
    for kernel in body.kernels() {
        if kernel.is_coinbase() {
            coinbase_kernel_count += 1;
        }
    }
    println!("{body}");
    for utxo in body.outputs() {
        if utxo.is_coinbase() {
            coinbase_utxo_count += 1;
        }
    }
    assert_eq!(coinbase_kernel_count, 1);
    assert_eq!(coinbase_utxo_count, 1);

    // Verify that the zero coinbase was automatically set to the full block reward
    let coinbase_output = body.outputs().iter().find(|o| o.is_coinbase()).unwrap();
    assert!(
        coinbase_output.minimum_value_promise.as_u64() > 0,
        "Zero coinbase should have been automatically set to block reward"
    );

    match client.submit_block(new_block).await {
        Ok(_) => (),
        Err(e) => panic!("The block should have been valid, {e}"),
    }
}

#[then(expr = "generate a block with zero value coinbase from node {word}")]
async fn generate_block_with_zero_coinbase(world: &mut TariWorld, node: String) {
    let mut client = world.get_node_client(&node).await.unwrap();

    let template_req = NewBlockTemplateRequest {
        algo: Some(PowAlgo {
            pow_algo: PowAlgos::Sha3x.into(),
        }),
        max_weight: 0,
    };

    let template_response = client.get_new_block_template(template_req).await.unwrap().into_inner();

    let block_template = template_response.new_block_template.clone().unwrap();
    let request = GetNewBlockWithCoinbasesRequest {
        new_template: Some(block_template),
        coinbases: vec![NewBlockCoinbase {
            address: TariAddress::from_base58(
                "f4L8GRWsXqz26DM3qAGErLtVknYzmTe2fYP2yKFn4biFXYJMP61W9MeD726QJ7ytWhRGyewTZzTzjZ7tEPskDptwRub",
            )
            .unwrap()
            .to_base58(),
            value: 0,
            stealth_payment: false,
            revealed_value_proof: true,
            coinbase_extra: Vec::new(),
        }],
    };

    let new_block = client.get_new_block_with_coinbases(request).await.unwrap().into_inner();

    let new_block = new_block.block.unwrap();
    let mut coinbase_kernel_count = 0;
    let mut coinbase_utxo_count = 0;
    let body: AggregateBody = new_block.body.clone().unwrap().try_into().unwrap();
    for kernel in body.kernels() {
        if kernel.is_coinbase() {
            coinbase_kernel_count += 1;
        }
    }
    for utxo in body.outputs() {
        if utxo.is_coinbase() {
            coinbase_utxo_count += 1;
        }
    }
    assert_eq!(coinbase_kernel_count, 1);
    assert_eq!(coinbase_utxo_count, 1);

    // Verify that the zero coinbase was automatically set to the full block reward
    let coinbase_output = body.outputs().iter().find(|o| o.is_coinbase()).unwrap();
    assert!(
        coinbase_output.minimum_value_promise.as_u64() > 0,
        "Zero coinbase should have been automatically set to block reward"
    );

    match client.submit_block(new_block).await {
        Ok(_) => (),
        Err(e) => panic!("The block should have been valid, {e}"),
    }
}

#[then(expr = "I generate a block {word} with zero value coinbase from node {word} for wallet {word}")]
async fn i_generate_block_with_zero_coinbase(
    world: &mut TariWorld,
    block_name: String,
    node_name: String,
    wallet_name: String,
) {
    let mut client = world.get_node_client(&node_name).await.unwrap();
    let wallet_address = world.get_wallet_address(&wallet_name).await.unwrap();

    let template_req = GetNewBlockTemplateWithCoinbasesRequest {
        algo: Some(PowAlgo {
            pow_algo: PowAlgos::Sha3x.into(),
        }),
        max_weight: 0,
        coinbases: vec![NewBlockCoinbase {
            address: wallet_address,
            value: 0,
            stealth_payment: true,
            revealed_value_proof: true,
            coinbase_extra: Vec::new(),
        }],
    };

    let template_response = client
        .get_new_block_template_with_coinbases(template_req)
        .await
        .unwrap()
        .into_inner();
    let new_block = template_response.block.clone().unwrap();

    let block = Block::try_from(template_response.block.unwrap()).unwrap();
    let coinbase_outputs = block
        .body
        .outputs()
        .iter()
        .filter(|o| o.is_coinbase())
        .cloned()
        .collect::<Vec<_>>();

    assert_eq!(coinbase_outputs.len(), 1);

    // Verify that the zero coinbase was automatically set to the full block reward
    let coinbase_output = coinbase_outputs.first().unwrap();
    assert!(
        coinbase_output.minimum_value_promise.as_u64() > 0,
        "Zero coinbase should have been automatically set to block reward"
    );

    match client.submit_block(new_block).await {
        Ok(_) => (),
        Err(e) => panic!("The block should have been valid, {e}"),
    }

    world.blocks.insert(block_name, block);
}

#[when(expr = "I have a lagging delayed node {word} connected to node {word} with \
               blocks_behind_before_considered_lagging {int}")]
async fn lagging_delayed_node(world: &mut TariWorld, delayed_node: String, node: String, delay: u64) {
    let mut base_node_config = BaseNodeConfig::default();
    base_node_config.state_machine.blocks_behind_before_considered_lagging = delay;

    spawn_base_node_with_config(world, false, delayed_node, vec![node], base_node_config).await;
}

#[then(expr = "node {word} has reached initial sync")]
async fn node_reached_sync(world: &mut TariWorld, node: String) {
    let mut client = world.get_node_client(&node).await.unwrap();

    wait_for!(
        timeout: Duration::from_secs(660),
        description: format!("node {node} to reach initial sync"),
        condition: async {
            let tip_info = client.get_tip_info(Empty {}).await.unwrap().into_inner();
            let longest_chain = tip_info.metadata.unwrap().best_block_height;
            if tip_info.initial_sync_achieved {
                Ok(true)
            } else {
                Err(format!("stuck at tip {longest_chain}"))
            }
        }
    );
}

#[when(expr = "I have {int} base nodes with pruning horizon {int} force syncing on node {word}")]
async fn force_sync_node_with_an_army_of_pruned_nodes(
    world: &mut TariWorld,
    nodes_count: u64,
    horizon: u64,
    node: String,
) {
    for i in 0..nodes_count {
        let node_name = format!("BaseNode-{i}");

        let mut base_node_config = BaseNodeConfig::default();
        let peers = vec![node.clone()];
        base_node_config.force_sync_peers = get_peer_addresses(world, &peers).await.into();
        base_node_config.storage.pruning_horizon = horizon;

        spawn_base_node_with_config(world, false, node_name, peers, base_node_config).await;
    }
}

#[then(expr = "{word} has at least {int} peers")]
async fn has_at_least_num_peers(world: &mut TariWorld, node: String, num_peers: u64) {
    let mut client = world.get_node_client(&node).await.unwrap();

    wait_for!(
        timeout: DEFAULT_TIMEOUT,
        description: format!("node {node} to have at least {num_peers} peers"),
        condition: async {
            let mut count = 0usize;
            let mut peers_stream = client.get_peers(grpc::GetPeersRequest {}).await.unwrap().into_inner();
            while let Some(resp) = peers_stream.next().await {
                if let Ok(resp) = resp &&
                    let Some(_peer) = resp.peer
                {
                    count += 1
                }
            }
            if count >= usize::try_from(num_peers).unwrap() {
                Ok(true)
            } else {
                Err(format!("has {count} peers"))
            }
        }
    );
}

#[when(expr = "I wait for base node {word} to have {int} base node connections")]
async fn wait_for_base_node_connections(world: &mut TariWorld, node: String, num_connections: u64) {
    let mut client = world.get_node_client(&node).await.unwrap();

    wait_for!(
        timeout: DEFAULT_TIMEOUT,
        description: format!("node {node} to have at least {num_connections} connections"),
        condition: async {
            let mut count = 0usize;
            let mut peers_stream = client.get_peers(grpc::GetPeersRequest {}).await.unwrap().into_inner();
            while let Some(resp) = peers_stream.next().await {
                if let Ok(resp) = resp &&
                    resp.peer.is_some()
                {
                    count += 1;
                }
            }
            if count >= num_connections as usize {
                Ok(true)
            } else {
                Err(format!("has {count} connections"))
            }
        }
    );
}
