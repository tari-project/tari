//   Copyright 2022. The Tari Project
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

mod utils;

use std::{io, path::PathBuf, time::Duration};

use cucumber::{gherkin::Scenario, given, then, when, writer, World as _, WriterExt as _};
use futures::StreamExt;
use indexmap::IndexMap;
use tari_app_grpc::tari_rpc as grpc;
use tari_common::initialize_logging;
use tari_core::transactions::transaction_components::{Transaction, TransactionOutput};
use tari_crypto::tari_utilities::ByteArray;
use tari_integration_tests::error::GrpcBaseNodeError;
use tari_utilities::hex::Hex;
use tari_wallet::transaction_service::config::TransactionRoutingMechanism;
use tari_wallet_grpc_client::grpc::{
    Empty,
    GetBalanceRequest,
    GetCompletedTransactionsRequest,
    GetIdentityRequest,
    GetTransactionInfoRequest,
};
use thiserror::Error;

use crate::utils::{
    base_node_process::{spawn_base_node, BaseNodeProcess},
    miner::{
        mine_block_with_coinbase_on_node,
        mine_blocks,
        mine_blocks_without_wallet,
        register_miner_process,
        MinerProcess,
    },
    transaction::build_transaction_with_output,
    wallet_process::{create_wallet_client, spawn_wallet, WalletProcess},
};

#[derive(Error, Debug)]
pub enum TariWorldError {
    #[error("Base node process not found: {0}")]
    BaseNodeProcessNotFound(String),
    #[error("Wallet process not found: {0}")]
    WalletProcessNotFound(String),
    #[error("Base node error: {0}")]
    GrpcBaseNodeError(#[from] GrpcBaseNodeError),
}

#[derive(Debug, Default, cucumber::World)]
pub struct TariWorld {
    seed_nodes: Vec<String>,
    base_nodes: IndexMap<String, BaseNodeProcess>,
    wallets: IndexMap<String, WalletProcess>,
    miners: IndexMap<String, MinerProcess>,
    transactions: IndexMap<String, Transaction>,
    // mapping from hex string of public key of wallet client to tx_id's
    wallet_tx_ids: IndexMap<String, Vec<u64>>,
    utxos: IndexMap<String, (u64, TransactionOutput)>,
}

impl TariWorld {
    async fn get_node_client<S: AsRef<str>>(
        &self,
        name: S,
    ) -> anyhow::Result<tari_base_node_grpc_client::BaseNodeGrpcClient<tonic::transport::Channel>> {
        self.base_nodes
            .get(name.as_ref())
            .ok_or_else(|| TariWorldError::BaseNodeProcessNotFound(name.as_ref().to_string()))?
            .get_grpc_client()
            .await
    }

    #[allow(dead_code)]
    async fn get_wallet_client<S: AsRef<str>>(
        &self,
        name: S,
    ) -> anyhow::Result<tari_wallet_grpc_client::WalletGrpcClient<tonic::transport::Channel>> {
        self.wallets
            .get(name.as_ref())
            .ok_or_else(|| TariWorldError::WalletProcessNotFound(name.as_ref().to_string()))?
            .get_grpc_client()
            .await
    }

    fn get_node<S: AsRef<str>>(&self, node_name: S) -> anyhow::Result<&BaseNodeProcess> {
        Ok(self
            .base_nodes
            .get(node_name.as_ref())
            .ok_or_else(|| TariWorldError::BaseNodeProcessNotFound(node_name.as_ref().to_string()))?)
    }

    pub fn all_seed_nodes(&self) -> &[String] {
        self.seed_nodes.as_slice()
    }

    pub async fn after(&mut self, _scenario: &Scenario) {
        //     self.base_nodes.clear();
        //     self.seed_nodes.clear();
        //     self.wallets.clear();
        //     self.miners.clear();
    }
}

#[given(expr = "I have a seed node {word}")]
async fn start_base_node(world: &mut TariWorld, name: String) {
    spawn_base_node(world, true, name, vec![], None).await;
}

#[given(expr = "a wallet {word} connected to base node {word}")]
async fn start_wallet(world: &mut TariWorld, wallet_name: String, node_name: String) {
    let seeds = world.base_nodes.get(&node_name).unwrap().seed_nodes.clone();
    spawn_wallet(world, wallet_name, Some(node_name), seeds, None).await;
}

#[when(expr = "I have a base node {word} connected to all seed nodes")]
async fn start_base_node_connected_to_all_seed_nodes(world: &mut TariWorld, name: String) {
    spawn_base_node(world, false, name, world.all_seed_nodes().to_vec(), None).await;
}

#[when(expr = "I have wallet {word} connected to all seed nodes")]
async fn start_wallet_connected_to_all_seed_nodes(world: &mut TariWorld, name: String) {
    spawn_wallet(world, name, None, world.all_seed_nodes().to_vec(), None).await;
}

#[given(expr = "a miner {word} connected to base node {word} and wallet {word}")]
async fn create_miner(world: &mut TariWorld, miner_name: String, bn_name: String, wallet_name: String) {
    register_miner_process(world, miner_name, bn_name, wallet_name);
}

#[when(expr = "I wait {int} seconds")]
async fn wait_seconds(_world: &mut TariWorld, seconds: u64) {
    tokio::time::sleep(Duration::from_secs(seconds)).await;
}

#[when(expr = "I wait for {word} to connect to {word}")]
#[then(expr = "{word} is connected to {word}")]
async fn node_pending_connection_to(
    world: &mut TariWorld,
    first_node: String,
    second_node: String,
) -> anyhow::Result<()> {
    let mut first_node = world.get_node_client(first_node).await?;
    let second_node = world.get_node(second_node)?;

    for _i in 0..100 {
        let res = first_node.list_connected_peers(Empty {}).await?;
        let res = res.into_inner();

        if res
            .connected_peers
            .iter()
            .any(|p| p.public_key == second_node.identity.public_key().as_bytes())
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    panic!("Peer was not connected in time");
}

#[when(expr = "mining node {word} mines {int} blocks")]
#[given(expr = "mining node {word} mines {int} blocks")]
async fn run_miner(world: &mut TariWorld, miner_name: String, num_blocks: u64) {
    mine_blocks(world, miner_name, num_blocks).await;
}

#[then(expr = "all nodes are at height {int}")]
#[when(expr = "all nodes are at height {int}")]
async fn all_nodes_are_at_height(world: &mut TariWorld, height: u64) -> anyhow::Result<()> {
    let num_retries = 100;
    let mut already_sync = true;

    for _ in 0..num_retries {
        for (_, bn) in world.base_nodes.iter() {
            let mut client = bn.get_grpc_client().await?;

            let chain_tip = client.get_tip_info(Empty {}).await?.into_inner();
            let chain_hgt = chain_tip.metadata.unwrap().height_of_longest_chain;

            if chain_hgt < height {
                already_sync = false;
            }
        }

        if already_sync {
            return Ok(());
        }

        already_sync = true;
        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    if !already_sync {
        panic!("base nodes not successfully synchronized at height {}", height);
    }

    Ok(())
}

#[when(expr = "node {word} is at height {int}")]
#[then(expr = "node {word} is at height {int}")]
async fn node_is_at_height(world: &mut TariWorld, base_node: String, height: u64) {
    let num_retries = 100;

    let mut client = world
        .base_nodes
        .get(&base_node)
        .unwrap()
        .get_grpc_client()
        .await
        .unwrap();
    let mut chain_hgt = 0;

    for _ in 0..=num_retries {
        let chain_tip = client.get_tip_info(Empty {}).await.unwrap().into_inner();
        chain_hgt = chain_tip.metadata.unwrap().height_of_longest_chain;

        if chain_hgt >= height {
            return;
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    // base node didn't synchronize successfully at height, so we bail out
    panic!(
        "base node didn't synchronize successfully with height {}, current chain height {}",
        height, chain_hgt
    );
}

#[when(expr = "I have mining node {word} connected to base node {word} and wallet {word}")]
async fn miner_connected_to_base_node_and_wallet(
    world: &mut TariWorld,
    miner: String,
    base_node: String,
    wallet: String,
) {
    register_miner_process(world, miner, base_node, wallet);
}

#[when(expr = "I wait for wallet {word} to have at least {int} uT")]
async fn wait_for_wallet_to_have_micro_tari(world: &mut TariWorld, wallet: String, amount: u64) {
    let wallet = world.wallets.get(&wallet).unwrap();
    let num_retries = 100;

    let mut client = wallet.get_grpc_client().await.unwrap();
    let mut curr_amount = 0;

    for _ in 0..=num_retries {
        curr_amount = client
            .get_balance(GetBalanceRequest {})
            .await
            .unwrap()
            .into_inner()
            .available_balance;

        if curr_amount >= amount {
            return;
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    // failed to get wallet right amount, so we panic
    panic!(
        "wallet failed to get right amount {}, current amount is {}",
        amount, curr_amount
    );
}

#[given(expr = "I have a base node {word} connected to seed {word}")]
#[when(expr = "I have a base node {word} connected to seed {word}")]
async fn base_node_connected_to_seed(world: &mut TariWorld, base_node: String, seed: String) {
    spawn_base_node(world, false, base_node, vec![seed], None).await;
}

#[then(expr = "I mine {int} blocks on {word}")]
#[when(expr = "I mine {int} blocks on {word}")]
async fn mine_blocks_on(world: &mut TariWorld, base_node: String, blocks: u64) {
    let mut client = world
        .base_nodes
        .get(&base_node)
        .unwrap()
        .get_grpc_client()
        .await
        .unwrap();
    mine_blocks_without_wallet(&mut client, blocks).await;
}

#[when(expr = "I have wallet {word} connected to base node {word}")]
async fn wallet_connected_to_base_node(world: &mut TariWorld, base_node: String, wallet: String) {
    let bn = world.base_nodes.get(&base_node).unwrap();
    let peer_seeds = bn.seed_nodes.clone();
    spawn_wallet(world, wallet, Some(base_node), peer_seeds, None).await;
}

#[when(expr = "mining node {word} mines {int} blocks with min difficulty {int} and max difficulty {int}")]
async fn mining_node_mines_blocks_with_difficulty(
    _world: &mut TariWorld,
    _miner: String,
    _blocks: u64,
    _min_difficulty: u64,
    _max_difficulty: u64,
) {
}

#[when(expr = "I have a base node {word}")]
#[given(expr = "I have a base node {word}")]
async fn create_and_add_base_node(world: &mut TariWorld, base_node: String) {
    spawn_base_node(world, false, base_node, vec![], None).await;
}

#[given(expr = "I have {int} seed nodes")]
async fn have_seed_nodes(world: &mut TariWorld, seed_nodes: u64) {
    for node in 0..seed_nodes {
        spawn_base_node(world, true, format!("seed_node_{}", node), vec![], None).await;
    }
}

#[when(expr = "I have wallet {word} connected to seed node {word}")]
async fn have_wallet_connect_to_seed_node(world: &mut TariWorld, wallet: String, seed_node: String) {
    spawn_wallet(world, wallet, None, vec![seed_node], None).await;
}

#[when(expr = "I mine a block on {word} with coinbase {word}")]
async fn mine_block_with_coinbase_on_node_step(world: &mut TariWorld, base_node: String, coinbase_name: String) {
    mine_block_with_coinbase_on_node(world, base_node, coinbase_name).await;
}

#[given(expr = "I have a pruned node {word} connected to node {word} with pruning horizon set to {int}")]
async fn prune_node_connected_to_base_node(
    world: &mut TariWorld,
    pruned_node: String,
    base_node: String,
    pruning_horizon: u64,
) {
    spawn_base_node(world, false, pruned_node, vec![base_node], Some(pruning_horizon)).await;
}

#[when(expr = "wallet {word} detects all transactions as Mined_Confirmed")]
async fn wallect_detects_all_txs_as_mined_confirmed(world: &mut TariWorld, wallet_name: String) {
    let mut client = create_wallet_client(world, wallet_name).await.unwrap();
    let wallet_identity = client.identify(GetIdentityRequest {}).await.unwrap().into_inner();
    let wallet_pubkey = wallet_identity.public_key.to_hex();
    let tx_ids = world.wallet_tx_ids.get(&wallet_pubkey).unwrap();

    let num_retries = 100;

    for tx_id in tx_ids {
        println!("waiting for tx with tx_id = {} to be mined_confirmed", tx_id);
        'inner: for _ in 0..num_retries {
            let request = GetTransactionInfoRequest {
                transaction_ids: vec![*tx_id],
            };
            let tx_info = client.get_transaction_info(request).await.unwrap().into_inner();
            let tx_info = tx_info.transactions.first().unwrap();
            match tx_info.status() {
                grpc::TransactionStatus::MinedConfirmed => break 'inner,
                _ => {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    continue;
                },
            }
        }
    }
}

#[then(expr = "I have a SHA3 miner {word} connected to node {word}")]
#[when(expr = "I have a SHA3 miner {word} connected to node {word}")]
async fn sha3_miner_connected_to_base_node(world: &mut TariWorld, miner: String, base_node: String) {
    spawn_base_node(world, false, miner.clone(), vec![base_node.clone()], None).await;
    let base_node = world.base_nodes.get(&base_node).unwrap();
    let peers = base_node.seed_nodes.clone();
    spawn_wallet(world, miner.clone(), Some(miner.clone()), peers, None).await;
    register_miner_process(world, miner.clone(), miner.clone(), miner);
}

#[when(expr = "I list all {word} transactions for wallet {word}")]
#[then(expr = "I list all {word} transactions for wallet {word}")]
async fn list_all_txs_for_wallet(world: &mut TariWorld, transaction_type: String, wallet: String) {
    if vec!["COINBASE", "NORMAL"].contains(&transaction_type.as_str()) {
        panic!("Invalid transaction type. Values should be COINBASE or NORMAL, for now");
    }

    let mut client = create_wallet_client(world, wallet.clone()).await.unwrap();

    let request = GetCompletedTransactionsRequest {};
    let mut completed_txs = client.get_completed_transactions(request).await.unwrap().into_inner();

    while let Ok(tx) = completed_txs.next().await.unwrap() {
        let tx_info = tx.transaction.unwrap();
        if (tx_info.message.contains("Coinbase Transaction for Block ") && transaction_type == "COINBASE") ||
            (!tx_info.message.contains("Coinbase Transaction for Block ") && transaction_type == "NORMAL")
        {
            println!("Transaction with status COINBASE found for wallet {}: ", wallet);
        } else {
            continue;
        }
        println!("\n");
        println!("TxId: {}", tx_info.tx_id);
        println!("Status: {}", tx_info.status);
        println!("IsCancelled: {}", tx_info.is_cancelled);
    }
}

#[when(expr = "wallet {word} has at least {int} transactions that are all {word} and not cancelled")]
#[then(expr = "wallet {word} has at least {int} transactions that are all {word} and not cancelled")]
async fn wallet_has_at_least_num_txs(world: &mut TariWorld, wallet: String, num_txs: u64, transaction_status: String) {
    let mut client = create_wallet_client(world, wallet.clone()).await.unwrap();
    let wallet_identity = client.identify(GetIdentityRequest {}).await.unwrap().into_inner();
    let wallet_pubkey = wallet_identity.public_key.to_hex();
    let tx_ids = world.wallet_tx_ids.get(&wallet_pubkey).unwrap();

    let transaction_status = match transaction_status.as_str() {
        "TRANSACTION_STATUS_COMPLETED" => 0i32,
        "TRANSACTION_STATUS_BROADCAST" => 1i32,
        "TRANSACTION_STATUS_MINED_UNCONFIRMED" => 2i32,
        "TRANSACTION_STATUS_IMPORTED" => 3i32,
        "TRANSACTION_STATUS_PENDING" => 4i32,
        "TRANSACTION_STATUS_COINBASE" => 5i32,
        "TRANSACTION_STATUS_MINED_CONFIRMED" => 6i32,
        "TRANSACTION_STATUS_NOT_FOUND" => 7i32,
        "TRANSACTION_STATUS_REJECTED" => 8i32,
        "TRANSACTION_STATUS_FAUX_UNCONFIRMED" => 9i32,
        "TRANSACTION_STATUS_FAUX_CONFIRMED" => 10i32,
        "TRANSACTION_STATUS_QUEUED" => 11i32,
        _ => panic!("Invalid transaction status {}", transaction_status),
    };

    let request = GetTransactionInfoRequest {
        transaction_ids: tx_ids.clone(),
    };
    let num_retries = 100;

    for _ in 0..num_retries {
        let txs_info = client.get_transaction_info(request.clone()).await.unwrap().into_inner();
        let txs_info = txs_info.transactions;
        if txs_info
            .iter()
            .filter(|x| x.status == transaction_status)
            .collect::<Vec<_>>()
            .len() as u64 >=
            num_txs
        {
            return;
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    panic!(
        "Wallet {} failed to has at least num {} txs with status {}",
        wallet, num_txs, transaction_status
    );
}

#[when(expr = "I create a transaction {word} spending {word} to {word}")]
async fn create_tx_spending_coinbase(world: &mut TariWorld, transaction: String, inputs: String, output: String) {
    let inputs = inputs.split(',').collect::<Vec<&str>>();
    let utxos = inputs
        .iter()
        .map(|i| {
            let (a, o) = world.utxos.get(&i.to_string()).unwrap();
            (*a, o.clone())
        })
        .collect::<Vec<_>>();
    let (amount, utxo, tx) = build_transaction_with_output(utxos.as_slice());
    world.utxos.insert(output, (amount, utxo));
    world.transactions.insert(transaction, tx);
}

#[when(expr = "I wait for wallet {word} to have less than {int} uT")]
async fn wait_for_wallet_to_have_less_than_micro_tari(world: &mut TariWorld, wallet: String, amount: u64) {
    let mut client = create_wallet_client(world, wallet.clone()).await.unwrap();
    println!("Waiting for wallet {} to have less than {} uT", wallet, amount);

    let num_retries = 100;
    let request = GetBalanceRequest {};

    for _ in 0..num_retries {
        let balance_res = client.get_balance(request.clone()).await.unwrap().into_inner();
        let current_balance = balance_res.available_balance;
        if current_balance < amount {
            println!(
                "Wallet {} now has less than {}, with current balance {}",
                wallet, amount, current_balance
            );
            return;
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    panic!(
        "Wallet {} didn't get less than {} after num_retries {}",
        wallet, amount, num_retries
    );
}

#[when(expr = "I have non-default wallet {word} connected to all seed nodes using {word}")]
async fn non_default_wallet_connected_to_all_seed_nodes(world: &mut TariWorld, wallet: String, mechanism: String) {
    let routing_mechanism = TransactionRoutingMechanism::from(mechanism);
    spawn_wallet(
        world,
        wallet,
        None,
        world.all_seed_nodes().to_vec(),
        Some(routing_mechanism),
    ).await;
}

#[when(expr = "I print the cucumber world")]
async fn print_world(world: &mut TariWorld) {
    eprintln!();
    eprintln!("======================================");
    eprintln!("============= TEST NODES =============");
    eprintln!("======================================");
    eprintln!();

    // base nodes
    for (name, node) in world.base_nodes.iter() {
        eprintln!(
            "Base node \"{}\": grpc port \"{}\", temp dir path \"{}\"",
            name, node.grpc_port, node.temp_dir_path
        );
    }

    // wallets
    for (name, node) in world.wallets.iter() {
        eprintln!(
            "Wallet \"{}\": grpc port \"{}\", temp dir path \"{}\"",
            name, node.grpc_port, node.temp_dir_path
        );
    }

    eprintln!();
    eprintln!("======================================");
    eprintln!();
}

#[tokio::main]
async fn main() {
    initialize_logging(
        &PathBuf::from("log4rs/base_node.yml"),
        include_str!("../log4rs/base_node.yml"),
    )
    .expect("logging not configured");
    TariWorld::cucumber()
        // following config needed to use eprint statements in the tests
        .max_concurrent_scenarios(1)
        .with_writer(
            writer::Basic::raw(io::stdout(), writer::Coloring::Never, 0)
                .summarized()
                .assert_normalized(),
        )
        .after(|_feature,_rule,scenario,_ev,maybe_world| {
            Box::pin(async move {
                if let Some(maybe_world) = maybe_world {
                    maybe_world.after(scenario).await;
                }
            })
        })
        .run_and_exit("tests/features/")
        .await;
}
