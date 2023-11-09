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
use minotari_app_grpc::tari_rpc::{self as grpc, GetTransactionInfoRequest};
use rand::Rng;
use tari_utilities::hex::Hex;
use tari_common_types::types::{BlockHash, PublicKey};
use tari_core::blocks::Block;
use tari_integration_tests::{
    base_node_process::spawn_base_node,
    miner::{
        mine_block,
        mine_block_before_submit,
        mine_block_with_coinbase_on_node,
        mine_blocks_without_wallet,
        register_miner_process,
    },
    wallet_process::{create_wallet_client, spawn_wallet},
    TariWorld,
};

use crate::steps::{node_steps::submit_transaction_to, wallet_steps::create_tx_spending_coinbase};

#[when(expr = "I have mine-before-tip mining node {word} connected to base node {word} and wallet {word}")]
#[when(expr = "I have mining node {word} connected to base node {word} and wallet {word}")]
pub async fn create_miner(world: &mut TariWorld, miner_name: String, bn_name: String, wallet_name: String) {
    register_miner_process(world, miner_name, bn_name, wallet_name);
}

#[when(expr = "mining node {word} mines {int} blocks")]
#[given(expr = "mining node {word} mines {int} blocks")]
async fn run_miner(world: &mut TariWorld, miner_name: String, num_blocks: u64) {
    world
        .get_miner(miner_name)
        .unwrap()
        .mine(world, Some(num_blocks), None, None)
        .await;
}

#[then(expr = "I mine {int} blocks on {word}")]
#[when(expr = "I mine {int} blocks on {word}")]
async fn mine_blocks_on(world: &mut TariWorld, blocks: u64, base_node: String) {
    let mut client = world
        .get_node_client(&base_node)
        .await
        .expect("Couldn't get the node client to mine with");
    mine_blocks_without_wallet(&mut client, blocks, 0, &world.key_manager).await;
}

#[when(expr = "mining node {word} mines {int} blocks with min difficulty {int} and max difficulty {int}")]
#[then(expr = "mining node {word} mines {int} blocks with min difficulty {int} and max difficulty {int}")]
async fn mining_node_mines_blocks_with_difficulty(
    world: &mut TariWorld,
    miner: String,
    blocks: u64,
    min_difficulty: u64,
    max_difficulty: u64,
) {
    let miner_ps = world.miners.get(&miner).unwrap();
    miner_ps
        .mine(world, Some(blocks), Some(min_difficulty), Some(max_difficulty))
        .await;
}

#[when(expr = "I mine a block on {word} with coinbase {word}")]
async fn mine_block_with_coinbase_on_node_step(world: &mut TariWorld, base_node: String, coinbase_name: String) {
    mine_block_with_coinbase_on_node(world, base_node, coinbase_name).await;
}

#[when(expr = "I mine {int} custom weight blocks on {word} with weight {int}")]
async fn mine_custom_weight_blocks_with_height(world: &mut TariWorld, num_blocks: u64, node_name: String, weight: u64) {
    let mut client = world
        .get_node_client(&node_name)
        .await
        .expect("Couldn't get the node client to mine with");
    mine_blocks_without_wallet(&mut client, num_blocks, weight, &world.key_manager).await;
}

#[then(expr = "I have a SHA3 miner {word} connected to node {word}")]
#[when(expr = "I have a SHA3 miner {word} connected to node {word}")]
async fn sha3_miner_connected_to_base_node(world: &mut TariWorld, miner: String, base_node: String) {
    spawn_base_node(world, false, miner.clone(), vec![base_node.clone()]).await;
    let base_node = world.base_nodes.get(&base_node).unwrap();
    let peers = base_node.seed_nodes.clone();
    world.wallet_connected_to_base_node.insert(miner.clone(), miner.clone());
    spawn_wallet(world, miner.clone(), Some(miner.clone()), peers, None, None).await;
    register_miner_process(world, miner.clone(), miner.clone(), miner);
}

#[then(expr = "while mining via SHA3 miner {word} all transactions in wallet {word} are found to be Mined_Confirmed")]
async fn while_mining_all_txs_in_wallet_are_mined_confirmed(world: &mut TariWorld, miner: String, wallet: String) {
    let mut wallet_client = create_wallet_client(world, wallet.clone()).await.unwrap();
    let wallet_address = world.get_wallet_address(&wallet).await.unwrap();
    let wallet_tx_ids = world.wallet_tx_ids.get(&wallet_address).unwrap();

    if wallet_tx_ids.is_empty() {
        panic!("Wallet {} has no available transactions", wallet);
    }

    let miner_ps = world.miners.get(&miner).unwrap();
    let num_retries = 100;
    println!(
        "Detecting {} Mined_Confirmed transactions for wallet {}",
        wallet_tx_ids.len(),
        wallet
    );

    for tx_id in wallet_tx_ids {
        'inner: for retry in 0..=num_retries {
            let req = GetTransactionInfoRequest {
                transaction_ids: vec![*tx_id],
            };
            let res = wallet_client.get_transaction_info(req).await.unwrap().into_inner();
            let tx_status = res.transactions.first().unwrap().status;
            // TRANSACTION_STATUS_MINED_CONFIRMED code is currently 6
            if tx_status == 6 {
                println!(
                    "Wallet transaction with id {} has been detected with status Mined_Confirmed",
                    tx_id
                );
                break 'inner;
            }

            if retry == num_retries {
                panic!(
                    "Unable to have wallet transaction with tx_id = {} with status Mined_Confirmed",
                    tx_id
                );
            }

            println!("Mine a block for tx_id {} to have status Mined_Confirmed", tx_id);
            miner_ps.mine(world, Some(1), None, None).await;

            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    }
}

#[then(expr = "while mining via node {word} all transactions in wallet {word} are found to be Mined_Confirmed")]
async fn while_mining_in_node_all_txs_in_wallet_are_mined_confirmed(
    world: &mut TariWorld,
    node: String,
    wallet: String,
    wallet_payment: String,
) {
    let mut wallet_client = create_wallet_client(world, wallet.clone()).await.unwrap();
    let wallet_address = world.get_wallet_address(&wallet).await.unwrap();
    let payment_address = world.get_wallet_payment_address(&wallet_payment).await.unwrap();
    let wallet_payment_address = PublicKey::from_hex(&payment_address).unwrap();
    let wallet_tx_ids = world.wallet_tx_ids.get(&wallet_address).unwrap();

    if wallet_tx_ids.is_empty() {
        panic!("Wallet {} on node {} has no available transactions", &wallet, &node);
    }

    let mut node_client = world.get_node_client(&node).await.unwrap();
    let num_retries = 100;
    let mut mined_status_flag = false;

    println!(
        "Detecting transactions on wallet {}, while mining on node {}, to be Mined_Confirmed",
        &wallet, &node
    );

    for tx_id in wallet_tx_ids {
        println!(
            "Waiting for transaction with id {} to have status Mined_Confirmed, while mining on node {}",
            tx_id, &node
        );

        'inner: for _ in 0..num_retries {
            let req = GetTransactionInfoRequest {
                transaction_ids: vec![*tx_id],
            };
            let res = wallet_client.get_transaction_info(req).await.unwrap().into_inner();
            let tx_status = res.transactions.first().unwrap().status;
            // TRANSACTION_STATUS_MINED_CONFIRMED code is currently 6
            if tx_status == 6 {
                println!("Transaction with id {} has been Mined_Confirmed", tx_id);
                mined_status_flag = true;
                break 'inner;
            }

            println!("Mine a block for tx_id {} to have status Mined_Confirmed", tx_id);
            mine_block(&mut node_client, &mut wallet_client, &wallet_payment_address).await;

            tokio::time::sleep(Duration::from_secs(5)).await;
        }

        if !mined_status_flag {
            panic!(
                "Failed to have transaction with id {} on wallet {}, while mining on node {}, to be Mined_Confirmed",
                tx_id, &wallet, &node
            );
        }
    }

    println!(
        "Wallet {} has all transactions Mined_Confirmed, while mining on node {}",
        &wallet, &node
    );
}

#[when(expr = "I have a SHA3 miner {word} connected to all seed nodes")]
async fn sha3_miner_connected_to_all_seed_nodes(world: &mut TariWorld, sha3_miner: String) {
    spawn_base_node(world, false, sha3_miner.clone(), world.seed_nodes.clone()).await;

    spawn_wallet(
        world,
        sha3_miner.clone(),
        Some(sha3_miner.clone()),
        world.seed_nodes.clone(),
        None,
        None,
    )
    .await;

    register_miner_process(world, sha3_miner.clone(), sha3_miner.clone(), sha3_miner);
}

#[given(expr = "I have a SHA3 miner {word} connected to seed node {word}")]
#[when(expr = "I have a SHA3 miner {word} connected to seed node {word}")]
async fn sha3_miner_connected_to_seed_node(world: &mut TariWorld, sha3_miner: String, seed_node: String) {
    println!("Create base node for SHA3 miner {}", &sha3_miner);
    spawn_base_node(world, false, sha3_miner.clone(), vec![seed_node.clone()]).await;

    println!("Create wallet for SHA3 miner {}", &sha3_miner);
    spawn_wallet(
        world,
        sha3_miner.clone(),
        Some(sha3_miner.clone()),
        vec![seed_node],
        None,
        None,
    )
    .await;

    println!("Register SHA3 miner {}", &sha3_miner);
    register_miner_process(world, sha3_miner.clone(), sha3_miner.clone(), sha3_miner);
}

#[when(expr = "I have individual mining nodes connected to each wallet and base node {word}")]
async fn mining_nodes_connected_to_each_wallet_and_base_node(world: &mut TariWorld, base_node: String) {
    let wallets = world.wallets.clone();

    for (ind, wallet_name) in wallets.keys().enumerate() {
        let miner = format!("Miner_{}", ind);
        register_miner_process(world, miner, base_node.clone(), wallet_name.clone());
    }
}

#[then(expr = "I have each mining node mine {int} blocks")]
async fn mining_node_mine_blocks(world: &mut TariWorld, blocks: u64) {
    let miners = world.miners.clone();
    for (miner, miner_ps) in miners {
        println!("Miner {} is mining {} blocks", miner, blocks);
        miner_ps.mine(world, Some(blocks), None, None).await;
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

#[when(expr = "I mine but do not submit a block {word} on {word}")]
async fn mine_without_submit(world: &mut TariWorld, block: String, node: String) {
    let mut client = world.get_node_client(&node).await.unwrap();

    let unmined_block: Block =
        Block::try_from(mine_block_before_submit(&mut client, &world.key_manager).await).unwrap();
    world.blocks.insert(block, unmined_block);
}

#[then(expr = "I update the parent of block {word} to be an orphan")]
async fn make_block_orphan(world: &mut TariWorld, block_name: String) {
    let mut block = world.blocks.remove(&block_name).expect("Couldn't find unmined block");
    block.header.prev_hash = BlockHash::zero();
    world.blocks.insert(block_name, block);
}

#[then(expr = "I update block {word} to have an invalid mmr")]
async fn make_block_invalid(world: &mut TariWorld, block_name: String) {
    let mut block = world.blocks.remove(&block_name).expect("Couldn't find unmined block");
    block.header.output_mr = BlockHash::zero();
    world.blocks.insert(block_name, block);
}

#[when(expr = "I submit block {word} to {word}")]
async fn submit_block_after(world: &mut TariWorld, block_name: String, node: String) {
    let mut client = world.get_node_client(&node).await.unwrap();
    let block = world.blocks.get(&block_name).expect("Couldn't find unmined block");
    match client.submit_block(grpc::Block::try_from(block.clone()).unwrap()).await {
        Ok(_) => {},
        Err(e) => {
            // The kind of errors we want don't actually get returned
            world.errors.push_back(e.message().to_string());
        },
    }
}

#[when(expr = "I spend outputs {word} via {word}")]
async fn spend_outputs_via(world: &mut TariWorld, inputs: String, node: String) {
    let num = rand::thread_rng().gen::<u8>();
    let tx_name = format!("TX-{}", num);
    let utxo_name = format!("UTXO-{}", num);

    create_tx_spending_coinbase(world, tx_name.clone(), inputs, utxo_name.clone()).await;
    submit_transaction_to(world, tx_name, node).await.unwrap();
}

#[when(expr = "I mine {int} blocks with difficulty {int} on {word}")]
async fn num_blocks_with_difficulty(world: &mut TariWorld, num_blocks: u64, difficulty: u64, node: String) {
    let wallet_name = format!("wallet-{}", &node);
    if world.wallets.get(&wallet_name).is_none() {
        spawn_wallet(world, wallet_name.clone(), Some(node.clone()), vec![], None, None).await;
    };

    let miner_name = format!("miner-{}", &node);
    if world.miners.get(&miner_name).is_none() {
        register_miner_process(world, miner_name.clone(), node.clone(), wallet_name.clone());
    }

    let miner = world.miners.get(&miner_name).unwrap();
    miner
        .mine(world, Some(num_blocks), Some(difficulty), Some(difficulty))
        .await;
}
