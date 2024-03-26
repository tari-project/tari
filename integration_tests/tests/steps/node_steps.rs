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

use std::{convert::TryFrom, time::Duration};

use cucumber::{given, then, when};
use futures::StreamExt;
use indexmap::IndexMap;
use minotari_app_grpc::tari_rpc::{self as grpc, GetBlocksRequest, ListHeadersRequest};
use minotari_node::BaseNodeConfig;
use minotari_wallet_grpc_client::grpc::{Empty, GetIdentityRequest};
use tari_core::blocks::Block;
use tari_integration_tests::{
    base_node_process::{spawn_base_node, spawn_base_node_with_config},
    get_peer_addresses,
    miner::mine_block_before_submit,
    world::NodeClient,
    TariWorld,
};

use crate::steps::{HALF_SECOND, TWO_MINUTES_WITH_HALF_SECOND_SLEEP};

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

#[when(expr = "I have {int} base nodes connected to all seed nodes")]
async fn multiple_base_nodes_connected_to_all_seeds(world: &mut TariWorld, nodes: u64) {
    for i in 0..nodes {
        let node = format!("Node_{}", i);
        println!("Initializing node {}", node.clone());
        spawn_base_node(world, false, node, world.all_seed_nodes().to_vec()).await;
    }
}

#[when(expr = "I wait for {word} to connect to {word}")]
#[then(expr = "I wait for {word} to connect to {word}")]
async fn node_pending_connection_to(world: &mut TariWorld, first_node: String, second_node: String) {
    let mut node_client = world.get_base_node_or_wallet_client(&first_node).await.unwrap();
    let second_client = world.get_base_node_or_wallet_client(&second_node).await.unwrap();

    let second_client_pubkey = match second_client {
        NodeClient::Wallet(mut client) => {
            client
                .identify(GetIdentityRequest {})
                .await
                .unwrap()
                .into_inner()
                .public_key
        },
        NodeClient::BaseNode(mut client) => client.identify(Empty {}).await.unwrap().into_inner().public_key,
    };

    for _i in 0..100 {
        let res = match node_client {
            NodeClient::Wallet(ref mut client) => client.list_connected_peers(Empty {}).await.unwrap(),
            NodeClient::BaseNode(ref mut client) => client.list_connected_peers(Empty {}).await.unwrap(),
        };
        let res = res.into_inner();

        if res.connected_peers.iter().any(|p| p.public_key == second_client_pubkey) {
            return;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    panic!("Peer was not connected in time");
}

#[when(expr = "I wait for {word} to have {int} connections")]
async fn wait_for_node_have_x_connections(world: &mut TariWorld, node: String, num_connections: usize) {
    let mut node_client = world.get_base_node_or_wallet_client(&node).await.unwrap();

    for _i in 0..100 {
        let res = match node_client {
            NodeClient::Wallet(ref mut client) => client.list_connected_peers(Empty {}).await.unwrap(),
            NodeClient::BaseNode(ref mut client) => client.list_connected_peers(Empty {}).await.unwrap(),
        };
        let res = res.into_inner();

        if res.connected_peers.len() >= num_connections {
            return;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    panic!("Peer was not connected in time");
}

#[then(expr = "all nodes are on the same chain at height {int}")]
async fn all_nodes_on_same_chain_at_height(world: &mut TariWorld, height: u64) {
    let mut nodes_at_height: IndexMap<&String, (u64, Vec<u8>)> = IndexMap::new();

    for (name, _) in &world.base_nodes {
        nodes_at_height.insert(name, (0, vec![]));
    }

    for _ in 0..(TWO_MINUTES_WITH_HALF_SECOND_SLEEP * height) {
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

        if nodes_at_height
            .values()
            .all(|(h, block_hash)| h == &height && block_hash == &nodes_at_height.values().last().unwrap().1)
        {
            return;
        }

        tokio::time::sleep(Duration::from_millis(HALF_SECOND)).await;
    }

    panic!(
        "base nodes not successfully synchronized at height {}, {:?}",
        height, nodes_at_height
    );
}

#[then(expr = "all nodes are at height {int}")]
#[when(expr = "all nodes are at height {int}")]
async fn all_nodes_are_at_height(world: &mut TariWorld, height: u64) {
    let mut nodes_at_height: IndexMap<&String, u64> = IndexMap::new();

    for (name, _) in &world.base_nodes {
        nodes_at_height.insert(name, 0);
    }

    for _ in 0..(TWO_MINUTES_WITH_HALF_SECOND_SLEEP * 7) {
        // ~14 minutes matching the original implementation timeout
        for (name, _) in nodes_at_height
            .clone()
            .iter()
            .filter(|(_, at_height)| at_height != &&height)
        {
            let mut client = world.get_node_client(name).await.unwrap();

            let chain_tip = client.get_tip_info(Empty {}).await.unwrap().into_inner();
            let chain_hgt = chain_tip.metadata.unwrap().best_block_height;

            nodes_at_height.insert(name, chain_hgt);
        }

        if nodes_at_height.values().all(|h| h == &height) {
            return;
        }

        tokio::time::sleep(Duration::from_millis(HALF_SECOND)).await;
    }

    panic!(
        "base nodes not successfully synchronized at height {}, {:?}",
        height, nodes_at_height
    );
}

#[when(expr = "node {word} is at height {int}")]
#[then(expr = "node {word} is at height {int}")]
async fn node_is_at_height(world: &mut TariWorld, base_node: String, height: u64) {
    let mut client = world.get_node_client(&base_node).await.unwrap();
    let mut chain_hgt = 0;

    for _ in 0..=(TWO_MINUTES_WITH_HALF_SECOND_SLEEP) {
        let chain_tip = client.get_tip_info(Empty {}).await.unwrap().into_inner();
        chain_hgt = chain_tip.metadata.unwrap().best_block_height;

        if chain_hgt >= height {
            return;
        }

        tokio::time::sleep(Duration::from_millis(HALF_SECOND)).await;
    }

    // base node didn't synchronize successfully at height, so we bail out
    panic!(
        "base node didn't synchronize successfully with height {}, current chain height {}",
        height, chain_hgt
    );
}

#[then(expr = "node {word} has a pruned height of {int}")]
async fn pruned_height_of(world: &mut TariWorld, node: String, height: u64) {
    let mut client = world.get_node_client(&node).await.unwrap();
    let mut last_pruned_height = 0;

    for _ in 0..=TWO_MINUTES_WITH_HALF_SECOND_SLEEP {
        let chain_tip = client.get_tip_info(Empty {}).await.unwrap().into_inner();
        last_pruned_height = chain_tip.metadata.unwrap().pruned_height;

        if last_pruned_height == height {
            return;
        }

        tokio::time::sleep(Duration::from_millis(HALF_SECOND)).await;
    }

    panic!(
        "Node {} pruned height is {} and never reached expected pruned height of {}",
        node, last_pruned_height, height
    )
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
        spawn_base_node(world, true, format!("seed_node_{}", node), vec![]).await;
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
        .unwrap_or_else(|| panic!("Couldn't find transaction {}", tx_name));
    let sig = &tx.body.kernels()[0].excess_sig;
    let mut last_state = "UNCHECKED: DEFAULT TEST STATE";

    // Some state changes take up to 30 minutes to make
    for _ in 0..(TWO_MINUTES_WITH_HALF_SECOND_SLEEP * 2) {
        let resp = client
            .transaction_state(grpc::TransactionStateRequest {
                excess_sig: Some(sig.into()),
            })
            .await?;

        let inner = resp.into_inner();

        last_state = match inner.result {
            0 => "UNKNOWN",
            1 => "MEMPOOL",
            2 => "MINED",
            3 => "NOT_STORED",
            _ => panic!("not getting a good result"),
        };

        if last_state == state {
            return Ok(());
        }

        tokio::time::sleep(Duration::from_millis(HALF_SECOND * 2)).await;
    }

    panic!(
        "The node {} has tx {} in state {} instead of the expected {}",
        node, tx_name, last_state, state
    );
}

#[then(expr = "I wait until base node {word} has {int} unconfirmed transactions in its mempool")]
async fn base_node_has_unconfirmed_transaction_in_mempool(world: &mut TariWorld, node: String, num_transactions: u64) {
    let mut client = world.get_node_client(&node).await.unwrap();
    let mut unconfirmed_txs = 0;

    for _ in 0..(TWO_MINUTES_WITH_HALF_SECOND_SLEEP) {
        let resp = client.get_mempool_stats(Empty {}).await.unwrap();
        let inner = resp.into_inner();

        unconfirmed_txs = inner.unconfirmed_txs;

        if inner.unconfirmed_txs == num_transactions {
            return;
        }

        tokio::time::sleep(Duration::from_millis(HALF_SECOND)).await;
    }

    panic!(
        "The node {} has {} unconfirmed txs instead of the expected {}",
        node, unconfirmed_txs, num_transactions
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
        .unwrap_or_else(|| panic!("Couldn't find transaction {}", tx_name));
    let sig = &tx.body.kernels()[0].excess_sig;

    let mut node_pool_status: IndexMap<&String, &str> = IndexMap::new();

    let nodes = world.base_nodes.iter().clone();
    let nodes_count = world.base_nodes.len();

    for (name, _) in nodes.clone() {
        node_pool_status.insert(name, "UNCHECKED: DEFAULT TEST STATE");
    }

    let can_fail = ((can_fail_percent as f64 * nodes.len() as f64) / 100.0).ceil() as u64;

    for _ in 0..(TWO_MINUTES_WITH_HALF_SECOND_SLEEP / 2) {
        for (name, _) in node_pool_status
            .clone()
            .iter()
            .filter(|(_, in_pool)| ***in_pool != pool)
        {
            let mut client = world.get_node_client(name).await?;

            let resp = client
                .transaction_state(grpc::TransactionStateRequest {
                    excess_sig: Some(sig.into()),
                })
                .await?;

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

        if node_pool_status.values().filter(|v| ***v == pool).count() >= (nodes_count - can_fail as usize) {
            return Ok(());
        }

        tokio::time::sleep(Duration::from_millis(HALF_SECOND / 2)).await;
    }

    panic!(
        "More than {}% ({} node(s)) failed to get {} in {}, {:?}",
        can_fail_percent, can_fail, tx_name, pool, node_pool_status
    );
}

#[then(expr = "I submit transaction {word} to {word}")]
#[when(expr = "I submit transaction {word} to {word}")]
pub async fn submit_transaction_to(world: &mut TariWorld, tx_name: String, node: String) -> anyhow::Result<()> {
    let mut client = world.get_node_client(&node).await?;
    let tx = world
        .transactions
        .get(&tx_name)
        .unwrap_or_else(|| panic!("Couldn't find transaction {}", tx_name));
    let resp = client
        .submit_transaction(grpc::SubmitTransactionRequest {
            transaction: Some(grpc::Transaction::try_from(tx.clone()).unwrap()),
        })
        .await?;

    let result = resp.into_inner();

    if result.result == 1 {
        Ok(())
    } else {
        panic!("Transaction {} wasn't submit to {}", tx_name, node)
    }
}

#[when(expr = "I submit transaction {word} to {word} and it does not succeed")]
pub async fn submit_failed_transaction_to(world: &mut TariWorld, tx_name: String, node: String) -> anyhow::Result<()> {
    let mut client = world.get_node_client(&node).await?;
    let tx = world
        .transactions
        .get(&tx_name)
        .unwrap_or_else(|| panic!("Couldn't find transaction {}", tx_name));
    let resp = client
        .submit_transaction(grpc::SubmitTransactionRequest {
            transaction: Some(grpc::Transaction::try_from(tx.clone()).unwrap()),
        })
        .await?;

    let result = resp.into_inner();

    if result.result == 1 {
        panic!(
            "Transaction {} was submitted, but should not have been to {}",
            tx_name, node
        )
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

    spawn_base_node_with_config(world, false, pruned_node, vec![base_node], base_node_config).await;
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
    let mut node_client = world.get_node_client(&node_name).await.unwrap();
    let tip = node_client.get_tip_info(Empty {}).await.unwrap().into_inner();
    let state = match state.as_str() {
        "START_UP" => 0,
        "HEADER_SYNC" => 1,
        "HORIZON_SYNC" => 2,
        "CONNECTING" => 3,
        "BLOCK_SYNC" => 4,
        "LISTENING" => 5,
        "SYNC_FAILED" => 6,
        _ => panic!("Invalid state"),
    };
    assert_eq!(state, tip.base_node_state);
}

#[then(expr = "node {word} is at the same height as node {word}")]
async fn base_node_is_at_same_height_as_node(world: &mut TariWorld, base_node: String, peer_node: String) {
    let mut peer_node_client = world.get_node_client(&peer_node).await.unwrap();
    let req = Empty {};
    let mut expected_height = peer_node_client
        .get_tip_info(req.clone())
        .await
        .unwrap()
        .into_inner()
        .metadata
        .unwrap()
        .best_block_height;

    let mut base_node_client = world.get_node_client(&base_node).await.unwrap();
    let mut current_height = 0;
    let num_retries = 100;

    'outer: for _ in 0..12 {
        'inner: for _ in 0..num_retries {
            current_height = base_node_client
                .get_tip_info(req.clone())
                .await
                .unwrap()
                .into_inner()
                .metadata
                .unwrap()
                .best_block_height;
            if current_height >= expected_height {
                break 'inner;
            }

            tokio::time::sleep(Duration::from_secs(5)).await;
        }

        expected_height = peer_node_client
            .get_tip_info(req.clone())
            .await
            .unwrap()
            .into_inner()
            .metadata
            .unwrap()
            .best_block_height;

        current_height = base_node_client
            .get_tip_info(req.clone())
            .await
            .unwrap()
            .into_inner()
            .metadata
            .unwrap()
            .best_block_height;

        if current_height == expected_height {
            break 'outer;
        }
    }

    if current_height == expected_height {
        println!(
            "Base node {} is at the same height {} as node {}",
            &base_node, current_height, &peer_node
        );
    } else {
        panic!(
            "Base node {} failed to synchronize at the same height as node {}",
            base_node, peer_node
        );
    }
}

#[when(expr = "I stop node {word}")]
#[then(expr = "I stop node {word}")]
async fn stop_node(world: &mut TariWorld, node: String) {
    let base_ps = world.base_nodes.get_mut(&node).unwrap();
    println!("Stopping node {}", node);
    base_ps.kill();
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
                &node, block_height, height
            );
        }
        println!("Valid block height {}, listed by node {}", height, &node);
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
                &node, height, header_height
            );
        }
        println!("correct listing of height header {} by node {}", height, &node);
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
    node_a_ps.kill();
    tokio::time::sleep(Duration::from_secs(15)).await;
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
    let mut longest_chain = 0;

    for _ in 0..(TWO_MINUTES_WITH_HALF_SECOND_SLEEP * 11) {
        let tip_info = client.get_tip_info(Empty {}).await.unwrap().into_inner();
        let metadata = tip_info.metadata.unwrap();
        longest_chain = metadata.best_block_height;

        if tip_info.initial_sync_achieved {
            return;
        }

        tokio::time::sleep(Duration::from_millis(HALF_SECOND)).await;
    }

    panic!(
        "Node {} never reached initial sync. Stuck at tip {}",
        node, longest_chain
    )
}

#[when(expr = "I have {int} base nodes with pruning horizon {int} force syncing on node {word}")]
async fn force_sync_node_with_an_army_of_pruned_nodes(
    world: &mut TariWorld,
    nodes_count: u64,
    horizon: u64,
    node: String,
) {
    for i in 0..=nodes_count {
        let node_name = format!("BaseNode-{}", i);

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
    let mut last_num_of_peers = 0;

    for _ in 0..(TWO_MINUTES_WITH_HALF_SECOND_SLEEP) {
        last_num_of_peers = 0;

        let mut peers_stream = client.get_peers(grpc::GetPeersRequest {}).await.unwrap().into_inner();

        while let Some(resp) = peers_stream.next().await {
            if let Ok(resp) = resp {
                if let Some(_peer) = resp.peer {
                    last_num_of_peers += 1
                }
            }
        }

        if last_num_of_peers >= usize::try_from(num_peers).unwrap() {
            return;
        }

        tokio::time::sleep(Duration::from_millis(HALF_SECOND)).await;
    }

    panic!(
        "Node {} only received {} of {} expected peers",
        node, last_num_of_peers, num_peers
    )
}
