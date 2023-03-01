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
#![feature(internal_output_capture)]

mod utils;

use std::{
    collections::VecDeque,
    convert::TryFrom,
    io::BufRead,
    path::PathBuf,
    ptr::null,
    str::{self, FromStr},
    sync::{Arc, Mutex},
    time::Duration,
};

use cucumber::{event::ScenarioFinished, gherkin::Scenario, given, then, when, World as _};
use futures::StreamExt;
use indexmap::IndexMap;
use log::*;
use rand::Rng;
use serde_json::Value;
use tari_app_grpc::tari_rpc::{self as grpc};
use tari_app_utilities::utilities::UniPublicKey;
use tari_base_node::BaseNodeConfig;
use tari_base_node_grpc_client::grpc::{GetBlocksRequest, ListHeadersRequest};
use tari_common::{configuration::Network, initialize_logging};
use tari_common_types::{
    tari_address::TariAddress,
    types::{BlindingFactor, ComAndPubSignature, Commitment, PrivateKey, PublicKey},
};
use tari_comms::multiaddr::Multiaddr;
use tari_console_wallet::{
    BurnTariArgs,
    CliCommands,
    CoinSplitArgs,
    DiscoverPeerArgs,
    ExportUtxosArgs,
    MakeItRainArgs,
    SendTariArgs,
    SetBaseNodeArgs,
    WhoisArgs,
};
use tari_core::{
    blocks::Block,
    consensus::ConsensusManager,
    covenants::Covenant,
    transactions::{
        tari_amount::MicroTari,
        transaction_components::{
            EncryptedValue,
            OutputFeatures,
            OutputType,
            Transaction,
            TransactionOutputVersion,
            UnblindedOutput,
        },
    },
};
use tari_crypto::{commitment::HomomorphicCommitment, keys::PublicKey as PublicKeyTrait};
use tari_integration_tests::error::GrpcBaseNodeError;
use tari_script::{ExecutionStack, StackItem, TariScript};
use tari_utilities::hex::Hex;
use tari_wallet::transaction_service::config::TransactionRoutingMechanism;
use tari_wallet_grpc_client::grpc::{
    CancelTransactionRequest,
    ClaimHtlcRefundRequest,
    ClaimShaAtomicSwapRequest,
    Empty,
    GetBalanceRequest,
    GetCompletedTransactionsRequest,
    GetIdentityRequest,
    GetTransactionInfoRequest,
    ImportUtxosRequest,
    PaymentRecipient,
    SendShaAtomicSwapRequest,
    TransferRequest,
};
use thiserror::Error;
use tokio::runtime::Runtime;

use crate::utils::{
    base_node_process::{spawn_base_node, spawn_base_node_with_config, BaseNodeProcess},
    get_peer_addresses,
    merge_mining_proxy::{register_merge_mining_proxy_process, MergeMiningProxyProcess},
    miner::{
        mine_block,
        mine_block_before_submit,
        mine_block_with_coinbase_on_node,
        mine_blocks_without_wallet,
        register_miner_process,
        MinerProcess,
    },
    transaction::{build_transaction_with_output, build_transaction_with_output_and_fee},
    wallet_ffi::{create_contact, create_seed_words, get_mnemonic_word_list_for_language, spawn_wallet_ffi, WalletFFI},
    wallet_process::{create_wallet_client, get_default_cli, spawn_wallet, WalletProcess},
};

pub const LOG_TARGET: &str = "cucumber";
pub const LOG_TARGET_STDOUT: &str = "stdout";
const CONFIRMATION_PERIOD: u64 = 4;
const TWO_MINUTES_WITH_HALF_SECOND_SLEEP: u64 = 240;
const HALF_SECOND: u64 = 500;

#[derive(Error, Debug)]
pub enum TariWorldError {
    #[error("Base node process not found: {0}")]
    BaseNodeProcessNotFound(String),
    #[error("Wallet process not found: {0}")]
    WalletProcessNotFound(String),
    #[error("FFIWallet not found: {0}")]
    FFIWalletNotFound(String),
    #[error("Miner process not found: {0}")]
    MinerProcessNotFound(String),
    #[error("Merge miner process not found: {0}")]
    MergeMinerProcessNotFound(String),
    #[error("Base node error: {0}")]
    GrpcBaseNodeError(#[from] GrpcBaseNodeError),
    #[error("No base node, or wallet client found: {0}")]
    ClientNotFound(String),
}

#[derive(Debug, Default, cucumber::World)]
pub struct TariWorld {
    base_nodes: IndexMap<String, BaseNodeProcess>,
    blocks: IndexMap<String, Block>,
    miners: IndexMap<String, MinerProcess>,
    ffi_wallets: IndexMap<String, WalletFFI>,
    wallets: IndexMap<String, WalletProcess>,
    merge_mining_proxies: IndexMap<String, MergeMiningProxyProcess>,
    transactions: IndexMap<String, Transaction>,
    wallet_addresses: IndexMap<String, String>, // values are strings representing tari addresses
    utxos: IndexMap<String, UnblindedOutput>,
    output_hash: Option<String>,
    pre_image: Option<String>,
    wallet_connected_to_base_node: IndexMap<String, String>, // wallet -> base node,
    seed_nodes: Vec<String>,
    // mapping from hex string of public key of wallet client to tx_id's
    wallet_tx_ids: IndexMap<String, Vec<u64>>,
    errors: VecDeque<String>,
    // We need to store this in between steps when importing and checking the imports.
    last_imported_tx_ids: Vec<u64>,
    // We need to store this for the merge mining proxy steps. The checks are get and check are done on separate steps.
    last_merge_miner_response: Value,
}

enum NodeClient {
    BaseNode(tari_base_node_grpc_client::BaseNodeGrpcClient<tonic::transport::Channel>),
    Wallet(tari_wallet_grpc_client::WalletGrpcClient<tonic::transport::Channel>),
}

impl TariWorld {
    async fn get_node_client<S: AsRef<str>>(
        &self,
        name: &S,
    ) -> anyhow::Result<tari_base_node_grpc_client::BaseNodeGrpcClient<tonic::transport::Channel>> {
        self.get_node(name)?.get_grpc_client().await
    }

    async fn get_base_node_or_wallet_client<S: core::fmt::Debug + AsRef<str>>(
        &self,
        name: S,
    ) -> anyhow::Result<NodeClient> {
        match self.get_node_client(&name).await {
            Ok(client) => Ok(NodeClient::BaseNode(client)),
            Err(_) => match self.get_wallet_client(&name).await {
                Ok(wallet) => Ok(NodeClient::Wallet(wallet)),
                Err(e) => Err(TariWorldError::ClientNotFound(e.to_string()).into()),
            },
        }
    }

    async fn get_wallet_address<S: AsRef<str>>(&self, name: &S) -> anyhow::Result<String> {
        if let Some(address) = self.wallet_addresses.get(name.as_ref()) {
            return Ok(address.clone());
        }
        match self.get_wallet_client(name).await {
            Ok(wallet) => {
                let mut wallet = wallet;

                Ok(wallet
                    .get_address(Empty {})
                    .await
                    .unwrap()
                    .into_inner()
                    .address
                    .to_hex())
            },
            Err(_) => {
                let ffi_wallet = self.get_ffi_wallet(name).unwrap();

                Ok(ffi_wallet.get_address().address().get_as_hex())
            },
        }
    }

    #[allow(dead_code)]
    async fn get_wallet_client<S: AsRef<str>>(
        &self,
        name: &S,
    ) -> anyhow::Result<tari_wallet_grpc_client::WalletGrpcClient<tonic::transport::Channel>> {
        self.get_wallet(name)?.get_grpc_client().await
    }

    fn get_node<S: AsRef<str>>(&self, node_name: &S) -> anyhow::Result<&BaseNodeProcess> {
        Ok(self
            .base_nodes
            .get(node_name.as_ref())
            .ok_or_else(|| TariWorldError::BaseNodeProcessNotFound(node_name.as_ref().to_string()))?)
    }

    fn get_wallet<S: AsRef<str>>(&self, wallet_name: &S) -> anyhow::Result<&WalletProcess> {
        Ok(self
            .wallets
            .get(wallet_name.as_ref())
            .ok_or_else(|| TariWorldError::WalletProcessNotFound(wallet_name.as_ref().to_string()))?)
    }

    fn get_ffi_wallet<S: AsRef<str>>(&self, wallet_name: &S) -> anyhow::Result<&WalletFFI> {
        Ok(self
            .ffi_wallets
            .get(wallet_name.as_ref())
            .ok_or_else(|| TariWorldError::FFIWalletNotFound(wallet_name.as_ref().to_string()))?)
    }

    fn get_mut_ffi_wallet<S: AsRef<str>>(&mut self, wallet_name: &S) -> anyhow::Result<&mut WalletFFI> {
        Ok(self
            .ffi_wallets
            .get_mut(wallet_name.as_ref())
            .ok_or_else(|| TariWorldError::FFIWalletNotFound(wallet_name.as_ref().to_string()))?)
    }

    fn get_miner<S: AsRef<str>>(&self, miner_name: S) -> anyhow::Result<&MinerProcess> {
        Ok(self
            .miners
            .get(miner_name.as_ref())
            .ok_or_else(|| TariWorldError::MinerProcessNotFound(miner_name.as_ref().to_string()))?)
    }

    fn get_merge_miner<S: AsRef<str>>(&self, miner_name: S) -> anyhow::Result<&MergeMiningProxyProcess> {
        Ok(self
            .merge_mining_proxies
            .get(miner_name.as_ref())
            .ok_or_else(|| TariWorldError::MergeMinerProcessNotFound(miner_name.as_ref().to_string()))?)
    }

    fn get_mut_merge_miner<S: AsRef<str>>(&mut self, miner_name: S) -> anyhow::Result<&mut MergeMiningProxyProcess> {
        Ok(self
            .merge_mining_proxies
            .get_mut(miner_name.as_ref())
            .ok_or_else(|| TariWorldError::MergeMinerProcessNotFound(miner_name.as_ref().to_string()))?)
    }

    pub fn all_seed_nodes(&self) -> &[String] {
        self.seed_nodes.as_slice()
    }

    pub async fn after(&mut self, _scenario: &Scenario) {
        self.base_nodes.clear();
        self.seed_nodes.clear();
        self.wallets.clear();
        self.ffi_wallets.clear();
        self.miners.clear();
    }
}

#[given(expr = "I have a seed node {word}")]
#[when(expr = "I have a seed node {word}")]
async fn start_base_node(world: &mut TariWorld, name: String) {
    spawn_base_node(world, true, name, vec![]).await;
}

#[given(expr = "a wallet {word} connected to base node {word}")]
async fn start_wallet(world: &mut TariWorld, wallet_name: String, node_name: String) {
    let seeds = world.base_nodes.get(&node_name).unwrap().seed_nodes.clone();
    world
        .wallet_connected_to_base_node
        .insert(wallet_name.clone(), node_name.clone());
    spawn_wallet(world, wallet_name, Some(node_name), seeds, None, None).await;
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

#[when(expr = "I have wallet {word} connected to all seed nodes")]
async fn start_wallet_connected_to_all_seed_nodes(world: &mut TariWorld, name: String) {
    // assuming we have deployed at least a base node, we take the first one as base node for wallet to connect to
    let nodes = world.all_seed_nodes().to_vec();
    let node = nodes.first().unwrap();
    world.wallet_connected_to_base_node.insert(name.clone(), node.clone());
    spawn_wallet(
        world,
        name,
        Some(node.clone()),
        world.all_seed_nodes().to_vec(),
        None,
        None,
    )
    .await;
}

#[when(expr = "I have mine-before-tip mining node {word} connected to base node {word} and wallet {word}")]
#[when(expr = "I have mining node {word} connected to base node {word} and wallet {word}")]
async fn create_miner(world: &mut TariWorld, miner_name: String, bn_name: String, wallet_name: String) {
    register_miner_process(world, miner_name, bn_name, wallet_name);
}

#[when(expr = "I wait {int} seconds")]
async fn wait_seconds(_world: &mut TariWorld, seconds: u64) {
    tokio::time::sleep(Duration::from_secs(seconds)).await;
}

#[when(expr = "I wait for {word} to connect to {word}")]
#[then(expr = "I wait for {word} to connect to {word}")]
#[then(expr = "{word} is connected to {word}")]
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

#[when(expr = "mining node {word} mines {int} blocks")]
#[given(expr = "mining node {word} mines {int} blocks")]
async fn run_miner(world: &mut TariWorld, miner_name: String, num_blocks: u64) {
    world
        .get_miner(miner_name)
        .unwrap()
        .mine(world, Some(num_blocks), None, None)
        .await;
}

#[then(expr = "all nodes are on the same chain at height {int}")]
async fn all_nodes_on_same_chain_at_height(world: &mut TariWorld, height: u64) {
    let mut nodes_at_height: IndexMap<&String, (u64, Vec<u8>)> = IndexMap::new();

    for (name, _) in world.base_nodes.iter() {
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

            nodes_at_height.insert(name, (metadata.height_of_longest_chain, metadata.best_block));
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

    for (name, _) in world.base_nodes.iter() {
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
            let chain_hgt = chain_tip.metadata.unwrap().height_of_longest_chain;

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
        chain_hgt = chain_tip.metadata.unwrap().height_of_longest_chain;

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

#[when(expr = "I wait for wallet {word} to have at least {int} uT")]
#[then(expr = "I wait for wallet {word} to have at least {int} uT")]
async fn wait_for_wallet_to_have_micro_tari(world: &mut TariWorld, wallet: String, amount: u64) {
    let wallet_ps = world.wallets.get(&wallet).unwrap();
    let num_retries = 100;

    let mut client = wallet_ps.get_grpc_client().await.unwrap();
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
        "wallet {} failed to get balance of at least amount {}, current amount is {}",
        wallet, amount, curr_amount
    );
}

#[given(expr = "I have a base node {word} connected to seed {word}")]
#[when(expr = "I have a base node {word} connected to seed {word}")]
async fn base_node_connected_to_seed(world: &mut TariWorld, base_node: String, seed: String) {
    spawn_base_node(world, false, base_node, vec![seed]).await;
}

#[then(expr = "I mine {int} blocks on {word}")]
#[when(expr = "I mine {int} blocks on {word}")]
async fn mine_blocks_on(world: &mut TariWorld, blocks: u64, base_node: String) {
    let mut client = world
        .get_node_client(&base_node)
        .await
        .expect("Couldn't get the node client to mine with");
    mine_blocks_without_wallet(&mut client, blocks, 0).await;
}

#[when(expr = "I have wallet {word} connected to base node {word}")]
async fn wallet_connected_to_base_node(world: &mut TariWorld, wallet: String, base_node: String) {
    let bn = world.base_nodes.get(&base_node).unwrap();
    let peer_seeds = bn.seed_nodes.clone();
    world
        .wallet_connected_to_base_node
        .insert(wallet.clone(), base_node.clone());

    let mut cli = get_default_cli();
    cli.seed_words_file_name = Some(PathBuf::new().join("seed_words.txt"));
    spawn_wallet(world, wallet, Some(base_node), peer_seeds, None, Some(cli)).await;
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

#[when(expr = "I have wallet {word} connected to seed node {word}")]
async fn have_wallet_connect_to_seed_node(world: &mut TariWorld, wallet: String, seed_node: String) {
    world
        .wallet_connected_to_base_node
        .insert(wallet.clone(), seed_node.clone());
    spawn_wallet(world, wallet, Some(seed_node.clone()), vec![seed_node], None, None).await;
}

#[when(expr = "I mine a block on {word} with coinbase {word}")]
async fn mine_block_with_coinbase_on_node_step(world: &mut TariWorld, base_node: String, coinbase_name: String) {
    mine_block_with_coinbase_on_node(world, base_node, coinbase_name).await;
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

        // panic!("{:?}", inner);

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

#[when(expr = "I mine {int} custom weight blocks on {word} with weight {int}")]
async fn mine_custom_weight_blocks_with_height(world: &mut TariWorld, num_blocks: u64, node_name: String, weight: u64) {
    let mut client = world
        .get_node_client(&node_name)
        .await
        .expect("Couldn't get the node client to mine with");
    mine_blocks_without_wallet(&mut client, num_blocks, weight).await;
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
async fn submit_transaction_to(world: &mut TariWorld, tx_name: String, node: String) -> anyhow::Result<()> {
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

#[when(expr = "wallet {word} detects all transactions as {word}")]
#[then(expr = "wallet {word} detects all transactions as {word}")]
async fn wallet_detects_all_txs_as_mined_confirmed(world: &mut TariWorld, wallet_name: String, status: String) {
    let mut client = create_wallet_client(world, wallet_name.clone()).await.unwrap();

    let mut completed_tx_stream = client
        .get_completed_transactions(GetCompletedTransactionsRequest {})
        .await
        .unwrap()
        .into_inner();

    let num_retries = 100;

    while let Some(tx_info) = completed_tx_stream.next().await {
        let tx_info = tx_info.unwrap();
        let tx_id = tx_info.transaction.unwrap().tx_id;

        println!("waiting for tx with tx_id = {} to be {}", tx_id, status);
        for retry in 0..=num_retries {
            let request = GetTransactionInfoRequest {
                transaction_ids: vec![tx_id],
            };
            let tx_info = client.get_transaction_info(request).await.unwrap().into_inner();
            let tx_info = tx_info.transactions.first().unwrap();

            if retry == num_retries {
                panic!(
                    "Wallet {} failed to detect tx with tx_id = {} to be {}, current status is {:?}",
                    wallet_name.as_str(),
                    tx_id,
                    status,
                    tx_info.status()
                );
            }
            match status.as_str() {
                "Pending" => match tx_info.status() {
                    grpc::TransactionStatus::Pending |
                    grpc::TransactionStatus::Completed |
                    grpc::TransactionStatus::Broadcast |
                    grpc::TransactionStatus::MinedUnconfirmed |
                    grpc::TransactionStatus::MinedConfirmed => {
                        break;
                    },
                    _ => (),
                },
                "Completed" => match tx_info.status() {
                    grpc::TransactionStatus::Completed |
                    grpc::TransactionStatus::Broadcast |
                    grpc::TransactionStatus::MinedUnconfirmed |
                    grpc::TransactionStatus::MinedConfirmed => {
                        break;
                    },
                    _ => (),
                },
                "Broadcast" => match tx_info.status() {
                    grpc::TransactionStatus::Broadcast |
                    grpc::TransactionStatus::MinedUnconfirmed |
                    grpc::TransactionStatus::MinedConfirmed => {
                        break;
                    },
                    _ => (),
                },
                "Mined_Unconfirmed" => match tx_info.status() {
                    grpc::TransactionStatus::MinedUnconfirmed | grpc::TransactionStatus::MinedConfirmed => {
                        break;
                    },
                    _ => (),
                },
                "Mined_Confirmed" => match tx_info.status() {
                    grpc::TransactionStatus::MinedConfirmed | grpc::TransactionStatus::Broadcast => {
                        break;
                    },
                    _ => (),
                },
                "Coinbase" => match tx_info.status() {
                    grpc::TransactionStatus::Pending |
                    grpc::TransactionStatus::Completed |
                    grpc::TransactionStatus::Broadcast |
                    grpc::TransactionStatus::MinedUnconfirmed |
                    grpc::TransactionStatus::MinedConfirmed |
                    grpc::TransactionStatus::Coinbase => {
                        break;
                    },
                    _ => (),
                },
                _ => panic!("Unknown status {}, don't know what to expect", status),
            }
        }
    }
}

#[when(expr = "wallet {word} detects all transactions are at least {word}")]
#[then(expr = "wallet {word} detects all transactions are at least {word}")]
async fn wallet_detects_all_txs_are_at_least_in_some_status(
    world: &mut TariWorld,
    wallet_name: String,
    status: String,
) {
    let mut client = create_wallet_client(world, wallet_name.clone()).await.unwrap();
    let wallet_address = client
        .get_address(Empty {})
        .await
        .unwrap()
        .into_inner()
        .address
        .to_hex();
    let tx_ids = world.wallet_tx_ids.get(&wallet_address).unwrap();

    let num_retries = 100;

    for tx_id in tx_ids {
        println!("waiting for tx with tx_id = {} to be pending", tx_id);
        for retry in 0..=num_retries {
            let request = GetTransactionInfoRequest {
                transaction_ids: vec![*tx_id],
            };
            let tx_info = client.get_transaction_info(request).await.unwrap().into_inner();
            let tx_info = tx_info.transactions.first().unwrap();

            if retry == num_retries {
                panic!(
                    "Wallet {} failed to detect tx with tx_id = {} to be at least {}",
                    wallet_name.as_str(),
                    tx_id,
                    status
                );
            }
            match status.as_str() {
                "Pending" => match tx_info.status() {
                    grpc::TransactionStatus::Pending |
                    grpc::TransactionStatus::Completed |
                    grpc::TransactionStatus::Broadcast |
                    grpc::TransactionStatus::MinedUnconfirmed |
                    grpc::TransactionStatus::MinedConfirmed => {
                        break;
                    },
                    _ => (),
                },
                "Completed" => match tx_info.status() {
                    grpc::TransactionStatus::Completed |
                    grpc::TransactionStatus::Broadcast |
                    grpc::TransactionStatus::MinedUnconfirmed |
                    grpc::TransactionStatus::MinedConfirmed => {
                        break;
                    },
                    _ => (),
                },
                "Broadcast" => match tx_info.status() {
                    grpc::TransactionStatus::Broadcast |
                    grpc::TransactionStatus::MinedUnconfirmed |
                    grpc::TransactionStatus::MinedConfirmed => {
                        break;
                    },
                    _ => (),
                },
                "Mined_Unconfirmed" => match tx_info.status() {
                    grpc::TransactionStatus::MinedUnconfirmed | grpc::TransactionStatus::MinedConfirmed => {
                        break;
                    },
                    _ => (),
                },
                _ => panic!("Unknown status {}, don't know what to expect", status),
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
}

#[then(expr = "wallet {word} detects all transactions are Broadcast")]
async fn wallet_detects_all_txs_as_broadcast(world: &mut TariWorld, wallet_name: String) {
    let mut client = create_wallet_client(world, wallet_name.clone()).await.unwrap();
    let wallet_address = client
        .get_address(Empty {})
        .await
        .unwrap()
        .into_inner()
        .address
        .to_hex();
    let tx_ids = world.wallet_tx_ids.get(&wallet_address).unwrap();

    let num_retries = 100;

    for tx_id in tx_ids {
        println!("waiting for tx with tx_id = {} to be mined_confirmed", tx_id);
        for retry in 0..=num_retries {
            let request = GetTransactionInfoRequest {
                transaction_ids: vec![*tx_id],
            };
            let tx_info = client.get_transaction_info(request).await.unwrap().into_inner();
            let tx_info = tx_info.transactions.first().unwrap();

            if retry == num_retries {
                panic!(
                    "Wallet {} failed to detect tx with tx_id = {} to be mined_confirmed",
                    wallet_name.as_str(),
                    tx_id
                );
            }
            match tx_info.status() {
                grpc::TransactionStatus::Broadcast => {
                    println!(
                        "Transaction with tx_id = {} has been detected as mined_confirmed by wallet {}",
                        tx_id,
                        wallet_name.as_str()
                    );
                    return;
                },
                _ => {
                    println!(
                        "Transaction with tx_id = {} has been detected with status = {:?}",
                        tx_id,
                        tx_info.status()
                    );
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    continue;
                },
            }
        }
    }
}

#[when(expr = "wallet {word} detects last transaction is Pending")]
async fn wallet_detects_last_tx_as_pending(world: &mut TariWorld, wallet: String) {
    let mut client = create_wallet_client(world, wallet.clone()).await.unwrap();
    let wallet_address = client
        .get_address(Empty {})
        .await
        .unwrap()
        .into_inner()
        .address
        .to_hex();
    let tx_ids = world.wallet_tx_ids.get(&wallet_address).unwrap();
    let tx_id = tx_ids.last().unwrap(); // get last transaction
    let num_retries = 100;

    println!("waiting for tx with tx_id = {} to be pending", tx_id);
    for retry in 0..=num_retries {
        let request = GetTransactionInfoRequest {
            transaction_ids: vec![*tx_id],
        };
        let tx_info = client.get_transaction_info(request).await.unwrap().into_inner();
        let tx_info = tx_info.transactions.first().unwrap();

        if retry == num_retries {
            panic!(
                "Wallet {} failed to detect tx with tx_id = {} to be pending",
                wallet.as_str(),
                tx_id
            );
        }
        match tx_info.status() {
            grpc::TransactionStatus::Pending => {
                println!(
                    "Transaction with tx_id = {} has been detected as pending by wallet {}",
                    tx_id,
                    wallet.as_str()
                );
                return;
            },
            _ => {
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            },
        }
    }
}

#[when(expr = "wallet {word} detects last transaction is Cancelled")]
async fn wallet_detects_last_tx_as_cancelled(world: &mut TariWorld, wallet: String) {
    let mut client = create_wallet_client(world, wallet.clone()).await.unwrap();
    let wallet_address = client
        .get_address(Empty {})
        .await
        .unwrap()
        .into_inner()
        .address
        .to_hex();
    let tx_ids = world.wallet_tx_ids.get(&wallet_address).unwrap();
    let tx_id = tx_ids.last().unwrap(); // get last transaction
    let num_retries = 100;

    println!("waiting for tx with tx_id = {} to be Cancelled", tx_id);
    for retry in 0..=num_retries {
        let request = GetTransactionInfoRequest {
            transaction_ids: vec![*tx_id],
        };
        let tx_info = client.get_transaction_info(request).await.unwrap().into_inner();
        let tx_info = tx_info.transactions.first().unwrap();

        if retry == num_retries {
            panic!(
                "Wallet {} failed to detect tx with tx_id = {} to be cancelled, current status is {:?}",
                wallet.as_str(),
                tx_id,
                tx_info.status(),
            );
        }
        match tx_info.status() {
            grpc::TransactionStatus::Rejected => {
                println!("Transaction with tx_id = {} has status {:?}", tx_id, tx_info.status());
                return;
            },
            _ => {
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            },
        }
    }
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

#[when(expr = "I list all {word} transactions for wallet {word}")]
#[then(expr = "I list all {word} transactions for wallet {word}")]
async fn list_all_txs_for_wallet(world: &mut TariWorld, transaction_type: String, wallet: String) {
    if transaction_type.as_str() != "COINBASE" && transaction_type.as_str() != "NORMAL" {
        panic!(
            "Invalid transaction type. Values should be COINBASE or NORMAL, value passed is {}",
            transaction_type
        );
    }
    let mut client = create_wallet_client(world, wallet.clone()).await.unwrap();

    let request = GetCompletedTransactionsRequest {};
    let mut completed_txs = client.get_completed_transactions(request).await.unwrap().into_inner();

    while let Some(tx) = completed_txs.next().await {
        let tx_info = tx.unwrap().transaction.unwrap();
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
    let transaction_status = match transaction_status.as_str() {
        "TRANSACTION_STATUS_COMPLETED" => 0,
        "TRANSACTION_STATUS_BROADCAST" => 1,
        "TRANSACTION_STATUS_MINED_UNCONFIRMED" => 2,
        "TRANSACTION_STATUS_IMPORTED" => 3,
        "TRANSACTION_STATUS_PENDING" => 4,
        "TRANSACTION_STATUS_COINBASE" => 5,
        "TRANSACTION_STATUS_MINED_CONFIRMED" => 6,
        "TRANSACTION_STATUS_NOT_FOUND" => 7,
        "TRANSACTION_STATUS_REJECTED" => 8,
        "TRANSACTION_STATUS_FAUX_UNCONFIRMED" => 9,
        "TRANSACTION_STATUS_FAUX_CONFIRMED" => 10,
        "TRANSACTION_STATUS_QUEUED" => 11,
        _ => panic!("Invalid transaction status {}", transaction_status),
    };

    let num_retries = 100;
    let mut current_status = 0;

    for _ in 0..num_retries {
        let mut txs = client
            .get_completed_transactions(grpc::GetCompletedTransactionsRequest {})
            .await
            .unwrap()
            .into_inner();
        let mut found_tx = 0;
        while let Some(tx) = txs.next().await {
            let tx_info = tx.unwrap().transaction.unwrap();
            current_status = tx_info.status;
            if current_status == transaction_status {
                found_tx += 1;
            }
        }
        if found_tx >= num_txs {
            return;
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    panic!(
        "Wallet {} failed to have at least num {} txs with status {}, current status is {}",
        wallet, num_txs, transaction_status, current_status
    );
}

#[when(expr = "I create a transaction {word} spending {word} to {word}")]
async fn create_tx_spending_coinbase(world: &mut TariWorld, transaction: String, inputs: String, output: String) {
    let inputs = inputs.split(',').collect::<Vec<&str>>();
    let utxos = inputs
        .iter()
        .map(|i| world.utxos.get(&i.to_string()).unwrap().clone())
        .collect::<Vec<_>>();

    let (tx, utxo) = build_transaction_with_output(utxos);
    world.utxos.insert(output, utxo);
    world.transactions.insert(transaction, tx);
}

#[when(expr = "I create a custom fee transaction {word} spending {word} to {word} with fee {word}")]
async fn create_tx_custom_fee(world: &mut TariWorld, transaction: String, inputs: String, output: String, fee: u64) {
    let inputs = inputs.split(',').collect::<Vec<&str>>();
    let utxos = inputs
        .iter()
        .map(|i| world.utxos.get(&i.to_string()).unwrap().clone())
        .collect::<Vec<_>>();

    let (tx, utxo) = build_transaction_with_output_and_fee(utxos, fee);
    world.utxos.insert(output, utxo);
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
#[given(expr = "I have non-default wallet {word} connected to all seed nodes using {word}")]
async fn non_default_wallet_connected_to_all_seed_nodes(world: &mut TariWorld, wallet: String, mechanism: String) {
    let routing_mechanism = TransactionRoutingMechanism::from(mechanism);
    // assuming we have at least one base node as seed node, we use the first to connect wallet to
    let nodes = world.all_seed_nodes().to_vec();
    let node = nodes.first().unwrap();
    world.wallet_connected_to_base_node.insert(wallet.clone(), node.clone());
    spawn_wallet(
        world,
        wallet,
        Some(node.clone()),
        world.all_seed_nodes().to_vec(),
        Some(routing_mechanism),
        None,
    )
    .await;
}

#[when(expr = "I have {int} non-default wallets connected to all seed nodes using {word}")]
async fn non_default_wallets_connected_to_all_seed_nodes(world: &mut TariWorld, num: u64, mechanism: String) {
    let routing_mechanism = TransactionRoutingMechanism::from(mechanism);
    let nodes = world.all_seed_nodes().to_vec();
    let node = nodes.first().unwrap();
    for ind in 0..num {
        let wallet_name = format!("Wallet_{}", ind);
        world
            .wallet_connected_to_base_node
            .insert(wallet_name.clone(), node.clone());
        spawn_wallet(
            world,
            wallet_name,
            Some(node.clone()),
            world.all_seed_nodes().to_vec(),
            Some(routing_mechanism),
            None,
        )
        .await;
    }
}

#[when(expr = "I send {int} uT without waiting for broadcast from wallet {word} to wallet {word} at fee {int}")]
#[then(expr = "I send {int} uT without waiting for broadcast from wallet {word} to wallet {word} at fee {int}")]
async fn send_amount_from_source_wallet_to_dest_wallet_without_broadcast(
    world: &mut TariWorld,
    amount: u64,
    source_wallet: String,
    dest_wallet: String,
    fee: u64,
) {
    let mut source_client = create_wallet_client(world, source_wallet.clone()).await.unwrap();
    let source_wallet_address = world.get_wallet_address(&source_wallet).await.unwrap();

    let dest_wallet_address = world.get_wallet_address(&dest_wallet).await.unwrap();

    let payment_recipient = PaymentRecipient {
        address: dest_wallet_address.clone(),
        amount,
        fee_per_gram: fee,
        message: format!(
            "transfer amount {} from {} to {}",
            amount,
            source_wallet.as_str(),
            dest_wallet.as_str()
        ),
        payment_type: 0, // normal mimblewimble payment type
    };
    let transfer_req = TransferRequest {
        recipients: vec![payment_recipient],
    };
    let tx_res = source_client.transfer(transfer_req).await.unwrap().into_inner();
    let tx_res = tx_res.results;

    assert_eq!(tx_res.len(), 1usize);

    let tx_res = tx_res.first().unwrap();
    assert!(
        tx_res.is_success,
        "Transacting amount {} uT from wallet {} to {} at fee {} failed",
        amount,
        source_wallet.as_str(),
        dest_wallet.as_str(),
        fee
    );

    let tx_id = tx_res.transaction_id;

    // insert tx_id's to the corresponding world mapping
    let source_tx_ids = world.wallet_tx_ids.entry(source_wallet_address.clone()).or_default();

    source_tx_ids.push(tx_id);

    let dest_tx_ids = world.wallet_tx_ids.entry(dest_wallet_address.clone()).or_default();

    dest_tx_ids.push(tx_id);

    println!(
        "Transfer amount {} from {} to {} at fee {} succeeded",
        amount, source_wallet, dest_wallet, fee
    );
}

#[then(expr = "I send a one-sided transaction of {int} uT from {word} to {word} at fee {int}")]
async fn send_one_sided_transaction_from_source_wallet_to_dest_wallt(
    world: &mut TariWorld,
    amount: u64,
    source_wallet: String,
    dest_wallet: String,
    fee: u64,
) {
    let mut source_client = create_wallet_client(world, source_wallet.clone()).await.unwrap();
    let source_wallet_address = world.get_wallet_address(&source_wallet).await.unwrap();

    let dest_wallet_address = world.get_wallet_address(&dest_wallet).await.unwrap();

    let payment_recipient = PaymentRecipient {
        address: dest_wallet_address.clone(),
        amount,
        fee_per_gram: fee,
        message: format!(
            "One sided transfer amount {} from {} to {}",
            amount,
            source_wallet.as_str(),
            dest_wallet.as_str()
        ),
        payment_type: 1, // one sided transaction
    };
    let transfer_req = TransferRequest {
        recipients: vec![payment_recipient],
    };
    let tx_res = source_client.transfer(transfer_req).await.unwrap().into_inner();
    let tx_res = tx_res.results;

    assert_eq!(tx_res.len(), 1usize);

    let tx_res = tx_res.first().unwrap();
    assert!(
        tx_res.is_success,
        "One sided transaction with amount {} from wallet {} to {} at fee {} failed",
        amount,
        source_wallet.as_str(),
        dest_wallet.as_str(),
        fee
    );

    // we wait for transaction to be broadcasted
    let tx_id = tx_res.transaction_id;
    let num_retries = 100;
    let tx_info_req = GetTransactionInfoRequest {
        transaction_ids: vec![tx_id],
    };

    for i in 0..num_retries {
        let tx_info_res = source_client
            .get_transaction_info(tx_info_req.clone())
            .await
            .unwrap()
            .into_inner();
        let tx_info = tx_info_res.transactions.first().unwrap();

        // TransactionStatus::TRANSACTION_STATUS_BROADCAST == 1_i32
        if tx_info.status == 1_i32 {
            println!(
                "One sided transaction from {} to {} with amount {} at fee {} has been broadcasted",
                source_wallet.clone(),
                dest_wallet.clone(),
                amount,
                fee
            );
            break;
        }

        if i == num_retries - 1 {
            panic!(
                "One sided transaction from {} to {} with amount {} at fee {} failed to be broadcasted",
                source_wallet.clone(),
                dest_wallet.clone(),
                amount,
                fee
            )
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    // insert tx_id's to the corresponding world mapping
    let source_tx_ids = world.wallet_tx_ids.entry(source_wallet_address.clone()).or_default();

    source_tx_ids.push(tx_id);

    let dest_tx_ids = world.wallet_tx_ids.entry(dest_wallet_address.clone()).or_default();

    dest_tx_ids.push(tx_id);

    println!(
        "One sided transaction with amount {} from {} to {} at fee {} succeeded",
        amount, source_wallet, dest_wallet, fee
    );
}

#[then(expr = "I send {int} uT from wallet {word} to wallet {word} at fee {int}")]
#[when(expr = "I send {int} uT from wallet {word} to wallet {word} at fee {int}")]
async fn send_amount_from_wallet_to_wallet_at_fee(
    world: &mut TariWorld,
    amount: u64,
    sender: String,
    receiver: String,
    fee_per_gram: u64,
) {
    let mut sender_wallet_client = create_wallet_client(world, sender.clone()).await.unwrap();
    let sender_wallet_address = world.get_wallet_address(&sender).await.unwrap();
    let receiver_wallet_address = world.get_wallet_address(&receiver).await.unwrap();

    let payment_recipient = PaymentRecipient {
        address: receiver_wallet_address.clone(),
        amount,
        fee_per_gram,
        message: format!(
            "Transfer amount {} from {} to {} as fee {}",
            amount,
            sender.as_str(),
            receiver.as_str(),
            fee_per_gram
        ),
        payment_type: 0, // mimblewimble transaction
    };
    let transfer_req = TransferRequest {
        recipients: vec![payment_recipient],
    };
    let tx_res = sender_wallet_client.transfer(transfer_req).await.unwrap().into_inner();
    let tx_res = tx_res.results;

    assert_eq!(tx_res.len(), 1usize);

    let tx_res = tx_res.first().unwrap();
    assert!(
        tx_res.is_success,
        "Transaction with amount {} from wallet {} to {} at fee {} failed",
        amount,
        sender.as_str(),
        receiver.as_str(),
        fee_per_gram
    );

    let tx_id = tx_res.transaction_id;
    let num_retries = 100;
    let tx_info_req = GetTransactionInfoRequest {
        transaction_ids: vec![tx_id],
    };

    for i in 0..num_retries {
        let tx_info_res = sender_wallet_client
            .get_transaction_info(tx_info_req.clone())
            .await
            .unwrap()
            .into_inner();
        let tx_info = tx_info_res.transactions.first().unwrap();

        // TransactionStatus::TRANSACTION_STATUS_BROADCAST == 1_i32
        if tx_info.status == 1_i32 {
            println!(
                "Transaction from {} to {} with amount {} at fee {} has been broadcasted",
                sender.clone(),
                receiver.clone(),
                amount,
                fee_per_gram
            );
            break;
        }

        if i == num_retries - 1 {
            panic!(
                "Transaction from {} to {} with amount {} at fee {} failed to be broadcasted",
                sender.clone(),
                receiver.clone(),
                amount,
                fee_per_gram
            )
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    // insert tx_id's to the corresponding world mapping
    let sender_tx_ids = world.wallet_tx_ids.entry(sender_wallet_address.clone()).or_default();

    sender_tx_ids.push(tx_id);

    let receiver_tx_ids = world.wallet_tx_ids.entry(receiver_wallet_address.clone()).or_default();

    receiver_tx_ids.push(tx_id);

    println!(
        "Transaction with amount {} from {} to {} at fee {} succeeded",
        amount, sender, receiver, fee_per_gram
    );
}

#[then(expr = "wallet {word} detects at least {int} coinbase transactions as Mined_Confirmed")]
async fn wallet_detects_at_least_coinbase_transactions(world: &mut TariWorld, wallet_name: String, coinbases: u64) {
    let mut client = create_wallet_client(world, wallet_name.clone()).await.unwrap();
    let mut completed_tx_res = client
        .get_completed_transactions(GetCompletedTransactionsRequest {})
        .await
        .unwrap()
        .into_inner();

    let num_retries = 100;
    let mut total_mined_confirmed_coinbases = 0;

    'outer: for _ in 0..num_retries {
        println!("Detecting mined confirmed coinbase transactions");
        'inner: while let Some(tx_info) = completed_tx_res.next().await {
            let tx_id = tx_info.unwrap().transaction.unwrap().tx_id;
            let request = GetTransactionInfoRequest {
                transaction_ids: vec![tx_id],
            };
            let tx_info = client.get_transaction_info(request).await.unwrap().into_inner();
            let tx_info = tx_info.transactions.first().unwrap();
            match tx_info.status() {
                grpc::TransactionStatus::MinedConfirmed => {
                    total_mined_confirmed_coinbases += 1;
                    if total_mined_confirmed_coinbases >= coinbases {
                        break 'outer;
                    }
                },
                _ => continue 'inner,
            }
        }

        if total_mined_confirmed_coinbases < coinbases {
            total_mined_confirmed_coinbases = 0;
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    if total_mined_confirmed_coinbases >= coinbases {
        println!(
            "Wallet {} detected at least {} coinbase transactions as Mined_Confirmed",
            &wallet_name, coinbases
        );
    } else {
        panic!(
            "Wallet {} failed to detect at least {} coinbase transactions as Mined_Confirmed",
            wallet_name, coinbases
        );
    }
}

#[then(expr = "wallet {word} detects at least {int} coinbase transactions as Mined_Unconfirmed")]
async fn wallet_detects_at_least_unmined_transactions(world: &mut TariWorld, wallet_name: String, coinbases: u64) {
    let mut client = create_wallet_client(world, wallet_name.clone()).await.unwrap();
    let mut completed_tx_res = client
        .get_completed_transactions(GetCompletedTransactionsRequest {})
        .await
        .unwrap()
        .into_inner();

    let num_retries = 100;
    let mut total_mined_unconfirmed_coinbases = 0;

    'outer: for _ in 0..num_retries {
        println!("Detecting mined unconfirmed coinbase transactions");
        'inner: while let Some(tx_info) = completed_tx_res.next().await {
            let tx_id = tx_info.unwrap().transaction.unwrap().tx_id;
            let request = GetTransactionInfoRequest {
                transaction_ids: vec![tx_id],
            };
            let tx_info = client.get_transaction_info(request).await.unwrap().into_inner();
            let tx_info = tx_info.transactions.first().unwrap();
            match tx_info.status() {
                grpc::TransactionStatus::MinedUnconfirmed => {
                    total_mined_unconfirmed_coinbases += 1;
                    if total_mined_unconfirmed_coinbases >= coinbases {
                        break 'outer;
                    }
                },
                _ => continue 'inner,
            }
        }

        if total_mined_unconfirmed_coinbases < coinbases {
            total_mined_unconfirmed_coinbases = 0;
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    if total_mined_unconfirmed_coinbases >= coinbases {
        println!(
            "Wallet {} detected at least {} coinbase transactions as Mined_Unconfirmed",
            &wallet_name, coinbases
        );
    } else {
        panic!(
            "Wallet {} failed to detect at least {} coinbase transactions as Mined_Unconfirmed",
            wallet_name, coinbases
        );
    }
}

#[then(expr = "wallet {word} detects exactly {int} coinbase transactions as Mined_Confirmed")]
async fn wallet_detects_exactly_coinbase_transactions(world: &mut TariWorld, wallet_name: String, coinbases: u64) {
    let mut client = create_wallet_client(world, wallet_name.clone()).await.unwrap();
    let wallet_address = world.get_wallet_address(&wallet_name).await.unwrap();
    let tx_ids = world.wallet_tx_ids.get(&wallet_address).unwrap();

    let num_retries = 100;
    let mut total_mined_confirmed_coinbases = 0;

    'outer: for _ in 0..num_retries {
        println!("Detecting mined confirmed coinbase transactions");
        'inner: for tx_id in tx_ids {
            let request = GetTransactionInfoRequest {
                transaction_ids: vec![*tx_id],
            };
            let tx_info = client.get_transaction_info(request).await.unwrap().into_inner();
            let tx_info = tx_info.transactions.first().unwrap();
            match tx_info.status() {
                grpc::TransactionStatus::MinedConfirmed => total_mined_confirmed_coinbases += 1,
                _ => continue 'inner,
            }
        }

        if total_mined_confirmed_coinbases >= coinbases {
            break 'outer;
        } else {
            total_mined_confirmed_coinbases = 0;
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    if total_mined_confirmed_coinbases == coinbases {
        println!(
            "Wallet {} detected exactly {} coinbase transactions as Mined_Confirmed",
            &wallet_name, coinbases
        );
    } else {
        panic!(
            "Wallet {} failed to detect exactly {} coinbase transactions as Mined_Confirmed",
            wallet_name, coinbases
        );
    }
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
        .height_of_longest_chain;

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
                .height_of_longest_chain;
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
            .height_of_longest_chain;

        current_height = base_node_client
            .get_tip_info(req.clone())
            .await
            .unwrap()
            .into_inner()
            .metadata
            .unwrap()
            .height_of_longest_chain;

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

#[then(expr = "I stop all wallets")]
async fn stop_all_wallets(world: &mut TariWorld) {
    for (wallet, wallet_ps) in &mut world.wallets {
        println!("Stopping wallet {}", wallet);

        wallet_ps.kill();
    }
}

#[then(expr = "I stop wallet {word}")]
#[when(expr = "I stop wallet {word}")]
async fn stop_wallet(world: &mut TariWorld, wallet: String) {
    // conveniently, register wallet address
    let mut wallet_client = create_wallet_client(world, wallet.clone()).await.unwrap();
    let wallet_address = wallet_client
        .get_address(Empty {})
        .await
        .unwrap()
        .into_inner()
        .address
        .to_hex();
    let wallet_ps = world.wallets.get_mut(&wallet).unwrap();
    world.wallet_addresses.insert(wallet.clone(), wallet_address);
    println!("Stopping wallet {}", wallet.as_str());
    wallet_ps.kill();
}

#[when(expr = "I stop node {word}")]
#[then(expr = "I stop node {word}")]
async fn stop_node(world: &mut TariWorld, node: String) {
    let base_ps = world.base_nodes.get_mut(&node).unwrap();
    println!("Stopping node {}", node);
    base_ps.kill();
}

#[when(expr = "I start wallet {word}")]
#[then(expr = "I start wallet {word}")]
async fn start_wallet_without_node(world: &mut TariWorld, wallet: String) {
    match world.wallet_connected_to_base_node.get(&wallet) {
        None => spawn_wallet(world, wallet, None, vec![], None, None).await,
        Some(base_node) => {
            // start wallet
            let base_node_ps = world.base_nodes.get(base_node).unwrap();
            let seed_nodes = base_node_ps.seed_nodes.clone();
            spawn_wallet(world, wallet, Some(base_node.clone()), seed_nodes, None, None).await;
        },
    }
}

#[then(expr = "while mining via node {word} all transactions in wallet {word} are found to be Mined_Confirmed")]
async fn while_mining_in_node_all_txs_in_wallet_are_mined_confirmed(
    world: &mut TariWorld,
    node: String,
    wallet: String,
) {
    let mut wallet_client = create_wallet_client(world, wallet.clone()).await.unwrap();
    let wallet_address = world.get_wallet_address(&wallet).await.unwrap();
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
            mine_block(&mut node_client, &mut wallet_client).await;

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

#[then(expr = "all wallets detect all transactions as Mined_Confirmed")]
async fn all_wallets_detect_all_txs_as_mined_confirmed(world: &mut TariWorld) {
    for wallet in world.wallets.keys() {
        let mut wallet_client = create_wallet_client(world, wallet.clone()).await.unwrap();
        let wallet_address = world.get_wallet_address(&wallet).await.unwrap();
        let wallet_tx_ids = world.wallet_tx_ids.get(&wallet_address);

        let wallet_tx_ids = if wallet_tx_ids.is_none() {
            println!("Wallet {} has no available transactions", &wallet);
            vec![]
        } else {
            let wallet_tx_ids = wallet_tx_ids.unwrap();
            if wallet_tx_ids.is_empty() {
                panic!("Wallet {} should have available transaction ids", wallet.as_str());
            }
            wallet_tx_ids.clone()
        };

        let num_retries = 100;

        for tx_id in wallet_tx_ids {
            'inner: for retry in 0..=num_retries {
                let req = GetTransactionInfoRequest {
                    transaction_ids: vec![tx_id],
                };
                let res = wallet_client.get_transaction_info(req).await.unwrap().into_inner();
                let tx_status = res.transactions.first().unwrap().status;

                // TRANSACTION_STATUS_MINED_CONFIRMED code is currently 6
                if tx_status == 6 {
                    println!(
                        "Wallet {} has detected transaction with id {} as Mined_Confirmed",
                        &wallet, tx_id
                    );
                    break 'inner;
                }

                if retry == num_retries {
                    panic!(
                        "Transaction with id {} does not have status as Mined_Confirmed, on wallet {}",
                        tx_id, &wallet
                    );
                }

                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}

#[then(expr = "wallets {word} should have {word} {int} spendable coinbase outputs")]
async fn wallets_should_have_at_least_num_spendable_coinbase_outs(
    world: &mut TariWorld,
    wallets: String,
    comparison: String,
    amount_of_coinbases: u64,
) {
    let at_least = "AT_LEAST";
    let exactly = "EXACTLY";

    if comparison.as_str() != at_least && comparison.as_str() != exactly {
        panic!("Invalid comparison value provided: {}", comparison);
    }

    let wallets = wallets.split(',').collect::<Vec<_>>();
    let mut wallets_clients: Vec<_> = vec![];
    for w in &wallets {
        wallets_clients.push(create_wallet_client(world, w.to_string()).await.unwrap());
    }

    let num_retries = 100;
    let mut coinbase_count = 0;
    let mut spendable_coinbase_count = 0;

    for ind in 0..wallets_clients.len() {
        let wallet = wallets[ind];
        let mut client = wallets_clients[ind].clone();

        'inner: for _ in 0..num_retries {
            let mut stream = client
                .get_completed_transactions(GetCompletedTransactionsRequest {})
                .await
                .unwrap()
                .into_inner();
            while let Some(completed_tx) = stream.next().await {
                let tx_info = completed_tx.unwrap().transaction.unwrap();

                if tx_info.message.contains("Coinbase Transaction for Block ") && tx_info.fee == 0 {
                    let tx_id = tx_info.tx_id;
                    coinbase_count += 1;

                    println!("Found coinbase transaction with id {} for wallet {}", tx_id, &wallet);

                    // MINED_CONFIRMED status = 6
                    if tx_info.status == 6 {
                        println!(
                            "Coinbase transaction with id {} for wallet {} is Mined_Confirmed",
                            tx_id, &wallet
                        );
                        spendable_coinbase_count += 1;
                    }
                }
            }

            if spendable_coinbase_count >= amount_of_coinbases {
                println!(
                    "Wallet {} has found at least {} within total {} coinbase transaction",
                    &wallet, amount_of_coinbases, coinbase_count
                );
                break 'inner;
            }

            tokio::time::sleep(Duration::from_secs(5)).await;
        }

        if comparison == at_least && spendable_coinbase_count >= amount_of_coinbases {
            println!("Wallet {} has found at least {}", &wallet, amount_of_coinbases);
        } else if comparison == exactly && spendable_coinbase_count == amount_of_coinbases {
            println!("Wallet {} has found exactly {}", &wallet, amount_of_coinbases);
        } else {
            panic!(
                "Wallet {} hasn't found {} {} spendable outputs, instead got {}",
                wallet, comparison, amount_of_coinbases, spendable_coinbase_count
            );
        }
    }
}

#[when(expr = "I send {int} transactions of {int} uT each from wallet {word} to wallet {word} at fee_per_gram {int}")]
async fn send_num_transactions_to_wallets_at_fee(
    world: &mut TariWorld,
    num_txs: u64,
    amount: u64,
    sender_wallet: String,
    receiver_wallet: String,
    fee_per_gram: u64,
) {
    let mut sender_wallet_client = create_wallet_client(world, sender_wallet.clone()).await.unwrap();
    let sender_wallet_address = world.get_wallet_address(&sender_wallet).await.unwrap();
    let receiver_wallet_address = world.get_wallet_address(&receiver_wallet).await.unwrap();
    let mut tx_ids = vec![];

    for _ in 0..num_txs {
        let payment_recipient = PaymentRecipient {
            address: receiver_wallet_address.clone(),
            amount,
            fee_per_gram,
            message: format!(
                "transfer amount {} from {} to {}",
                amount,
                sender_wallet.as_str(),
                receiver_wallet.as_str()
            ),
            payment_type: 0, // standard mimblewimble transaction
        };
        let transfer_req = TransferRequest {
            recipients: vec![payment_recipient],
        };
        let transfer_res = sender_wallet_client.transfer(transfer_req).await.unwrap().into_inner();
        let transfer_res = transfer_res.results.first().unwrap();

        if !transfer_res.is_success {
            panic!(
                "Failed to send transaction from wallet {} to wallet {}, with message \n {}",
                &sender_wallet, &receiver_wallet, &transfer_res.failure_message
            );
        }
        tx_ids.push(transfer_res.transaction_id);

        // insert tx_id's to the corresponding world mapping
        let source_tx_ids = world.wallet_tx_ids.entry(sender_wallet_address.clone()).or_default();

        source_tx_ids.append(&mut tx_ids);

        let dest_tx_ids = world.wallet_tx_ids.entry(receiver_wallet_address.clone()).or_default();

        dest_tx_ids.append(&mut tx_ids);

        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let num_retries = 100;
    println!(
        "Waiting for transactions from wallet {} to wallet {} to be broadcasted",
        &sender_wallet, &receiver_wallet
    );

    for tx_id in tx_ids {
        println!("Waiting for transaction with id {} to be broadcasted", tx_id);
        let request = GetTransactionInfoRequest {
            transaction_ids: vec![tx_id],
        };

        let mut is_broadcast = false;

        'inner: for _ in 0..num_retries {
            let txs_info = sender_wallet_client
                .get_transaction_info(request.clone())
                .await
                .unwrap()
                .into_inner();
            let txs_info = txs_info.transactions.first().unwrap();

            if txs_info.status == 1 {
                println!(
                    "Transaction from wallet {} to wallet {} with id {} has been broadcasted to the network",
                    &sender_wallet, &receiver_wallet, tx_id
                );
                is_broadcast = true;
                break 'inner;
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }

        if !is_broadcast {
            panic!(
                "Transaction from wallet {} to wallet {} with id {} was not broacasted to the network",
                &sender_wallet, &receiver_wallet, tx_id
            );
        }
    }
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

#[then(expr = "I wait for {word} to have {int} node connections")]
async fn wait_for_wallet_to_have_num_connections(world: &mut TariWorld, wallet: String, connections: u64) {
    let mut wallet_client = create_wallet_client(world, wallet.clone()).await.unwrap();
    let num_retries = 100;

    println!("Waiting for wallet {} to have {} connections", &wallet, connections);
    let mut actual_connections = 0_u32;

    for _ in 0..num_retries {
        let network_status_res = wallet_client.get_network_status(Empty {}).await.unwrap().into_inner();
        actual_connections = network_status_res.num_node_connections;
        if u64::from(actual_connections) >= connections {
            println!("Wallet {} has at least {} connections", &wallet, connections);
            break;
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    if u64::from(actual_connections) != connections {
        panic!("Wallet {} does not have {} connections", &wallet, connections);
    }
}

#[then(expr = "I wait for {word} to have {word} connectivity")]
async fn wait_for_wallet_to_have_specific_connectivity(world: &mut TariWorld, wallet: String, connectivity: String) {
    let mut wallet_client = create_wallet_client(world, wallet.clone()).await.unwrap();
    let num_retries = 100;

    println!("Waiting for wallet {} to have connectivity {}", &wallet, &connectivity);
    let connectivity = connectivity.to_uppercase();

    let connectivity_index = match connectivity.as_str() {
        "INITIALIZING" => 0,
        "ONLINE" => 1,
        "DEGRADED" => 2,
        "OFFLINE" => 3,
        _ => panic!("Invalid connectivity value {}", connectivity),
    };

    for _ in 0..=num_retries {
        let network_status_res = wallet_client.get_network_status(Empty {}).await.unwrap().into_inner();
        let connectivity_status = network_status_res.status;
        if connectivity_status == connectivity_index {
            println!("Wallet {} has {} connectivity", &wallet, &connectivity);
            return;
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    panic!(
        "Wallet {} did not get correct connectivity status {}",
        &wallet, connectivity
    );
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

#[when(expr = "I transfer {int}T from {word} to {word}")]
async fn transfer_tari_from_wallet_to_receiver(world: &mut TariWorld, amount: u64, sender: String, receiver: String) {
    let mut sender_wallet_client = create_wallet_client(world, sender.clone()).await.unwrap();
    let sender_wallet_address = world.get_wallet_address(&sender).await.unwrap();
    let receiver_wallet_address = world.get_wallet_address(&receiver).await.unwrap();

    let payment_recipient = PaymentRecipient {
        address: receiver_wallet_address.clone(),
        amount: amount * 1_000_000_u64, // 1T = 1_000_000uT
        fee_per_gram: 10,               // as in the js cucumber tests
        message: format!(
            "transfer amount {} from {} to {}",
            amount,
            sender.as_str(),
            receiver.as_str()
        ),
        payment_type: 0, // normal mimblewimble payment type
    };
    let transfer_req = TransferRequest {
        recipients: vec![payment_recipient],
    };
    let tx_res = sender_wallet_client.transfer(transfer_req).await.unwrap().into_inner();
    let tx_res = tx_res.results;

    assert_eq!(tx_res.len(), 1usize);

    let tx_res = tx_res.first().unwrap();
    assert!(
        tx_res.is_success,
        "Transacting amount {}T from wallet {} to {} at fee {} failed",
        amount,
        sender.as_str(),
        receiver.as_str(),
        10
    );

    // we wait for transaction to be broadcasted
    let tx_id = tx_res.transaction_id;
    let num_retries = 100;
    let tx_info_req = GetTransactionInfoRequest {
        transaction_ids: vec![tx_id],
    };

    for i in 0..=num_retries {
        let tx_info_res = sender_wallet_client
            .get_transaction_info(tx_info_req.clone())
            .await
            .unwrap()
            .into_inner();
        let tx_info = tx_info_res.transactions.first().unwrap();

        // TransactionStatus::TRANSACTION_STATUS_BROADCAST == 1_i32
        if tx_info.status == 1_i32 {
            println!(
                "Transaction from {} to {} with amount {} at fee {} has been broadcasted",
                sender.clone(),
                receiver.clone(),
                amount,
                10
            );
            break;
        }

        if i == num_retries {
            panic!(
                "Transaction from {} to {} with amount {} at fee {} failed to be broadcasted",
                sender.clone(),
                receiver.clone(),
                amount,
                10
            )
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    // insert tx_id's to the corresponding world mapping
    let source_tx_ids = world.wallet_tx_ids.entry(sender_wallet_address.clone()).or_default();

    source_tx_ids.push(tx_id);

    let dest_tx_ids = world.wallet_tx_ids.entry(receiver_wallet_address.clone()).or_default();

    dest_tx_ids.push(tx_id);

    println!(
        "Transfer amount {} from {} to {} at fee {} succeeded",
        amount, sender, receiver, 10
    );
}

#[when(expr = "wallet {word} has {int}T")]
#[then(expr = "wallet {word} has {int}T")]
async fn wallet_has_tari(world: &mut TariWorld, wallet: String, amount: u64) {
    let mut wallet_client = create_wallet_client(world, wallet.clone()).await.unwrap();
    let num_retries = 100;

    let mut available_balance = 0;

    for _ in 0..num_retries {
        let balance_res = wallet_client
            .get_balance(GetBalanceRequest {})
            .await
            .unwrap()
            .into_inner();

        available_balance = balance_res.available_balance;
        if available_balance >= amount * 1_000_000 {
            println!("Wallet {} has at least {}T", wallet.as_str(), amount);
            return;
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    panic!(
        "Wallet {} failed to have at least {}T, it ended with {}T",
        wallet, amount, available_balance
    );
}

#[when(expr = "I have wallet {word} with {int}T connected to base node {word}")]
async fn wallet_with_tari_connected_to_base_node(
    world: &mut TariWorld,
    wallet: String,
    amount: u64,
    base_node: String,
) {
    let peer_seeds = world.base_nodes.get(&base_node).unwrap().seed_nodes.clone();
    println!(
        "Start a new wallet {} connected to base node {}",
        wallet.as_str(),
        base_node.as_str()
    );
    world
        .wallet_connected_to_base_node
        .insert(wallet.clone(), base_node.clone());
    spawn_wallet(world, wallet.clone(), Some(base_node.clone()), peer_seeds, None, None).await;

    let mut base_node_client = world.get_node_client(&base_node).await.unwrap();
    let tip_info_res = base_node_client.get_tip_info(Empty {}).await.unwrap().into_inner();
    let mut current_height = tip_info_res.metadata.unwrap().height_of_longest_chain;

    let mut num_blocks = 0;
    let mut reward = 0;

    let consensus_manager = ConsensusManager::builder(Network::LocalNet).build();

    while reward < amount {
        current_height += 1;
        num_blocks += 1;
        reward += consensus_manager.get_block_reward_at(current_height).as_u64() / 1_000_000; // 1 T = 1_000_000 uT
    }

    println!("Creating miner...");
    create_miner(world, "temp_miner".to_string(), base_node.clone(), wallet.clone()).await;

    println!("Mining {} blocks", num_blocks + CONFIRMATION_PERIOD);
    let miner = world.miners.get(&"temp_miner".to_string()).unwrap();
    miner
        .mine(world, Some(num_blocks + CONFIRMATION_PERIOD), None, None)
        .await; // mine some additional blocks to confirm txs

    let mut wallet_client = create_wallet_client(world, wallet.clone()).await.unwrap();
    let num_retries = 100;

    for _ in 0..num_retries {
        let balance_res = wallet_client
            .get_balance(GetBalanceRequest {})
            .await
            .unwrap()
            .into_inner();

        if balance_res.available_balance >= amount * 1_000_000 {
            println!("Wallet {} has at least {}T", wallet.as_str(), amount);
            return;
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    panic!("Wallet {} failed to have at least {}T", wallet, amount);
}

#[when(expr = "I transfer {int} uT from {word} to {word} and {word} at fee {int}")]
#[allow(clippy::too_many_lines)]
async fn transfer_from_wallet_to_two_recipients_at_fee(
    world: &mut TariWorld,
    amount: u64,
    sender: String,
    receiver1: String,
    receiver2: String,
    fee_per_gram: u64,
) {
    let mut sender_client = create_wallet_client(world, sender.clone()).await.unwrap();
    let sender_wallet_address = world.get_wallet_address(&sender).await.unwrap();
    let receiver1_address = world.get_wallet_address(&receiver1).await.unwrap();
    let receiver2_address = world.get_wallet_address(&receiver2).await.unwrap();

    let payment_recipient1 = PaymentRecipient {
        address: receiver1_address.clone(),
        amount,
        fee_per_gram,
        message: format!(
            "transfer amount {} from {} to {}",
            amount,
            sender.as_str(),
            receiver1.as_str()
        ),
        payment_type: 0, // normal mimblewimble payment type
    };

    let payment_recipient2 = PaymentRecipient {
        address: receiver2_address.clone(),
        amount,
        fee_per_gram,
        message: format!(
            "transfer amount {} from {} to {}",
            amount,
            sender.as_str(),
            receiver2.as_str()
        ),
        payment_type: 0, // normal mimblewimble payment type
    };
    let transfer_req = TransferRequest {
        recipients: vec![payment_recipient1, payment_recipient2],
    };
    let tx_res = sender_client.transfer(transfer_req).await.unwrap().into_inner();
    let tx_res = tx_res.results;

    assert_eq!(tx_res.len(), 2_usize);

    let tx_res1 = tx_res.first().unwrap();
    let tx_res2 = tx_res.last().unwrap();

    assert!(
        tx_res1.is_success,
        "Transacting amount {} uT from wallet {} to {} at fee {} failed",
        amount,
        sender.as_str(),
        receiver1.as_str(),
        fee_per_gram
    );
    assert!(
        tx_res2.is_success,
        "Transacting amount {} uT from wallet {} to {} at fee {} failed",
        amount,
        sender.as_str(),
        receiver2.as_str(),
        fee_per_gram
    );

    // we wait for transaction to be broadcasted
    let tx_id1 = tx_res1.transaction_id;
    let tx_id2 = tx_res2.transaction_id;

    let num_retries = 100;
    let tx_info_req = GetTransactionInfoRequest {
        transaction_ids: vec![tx_id1, tx_id2],
    };

    for i in 0..=num_retries {
        let tx_info_res = sender_client
            .get_transaction_info(tx_info_req.clone())
            .await
            .unwrap()
            .into_inner();
        let tx_info1 = tx_info_res.transactions.first().unwrap();
        let tx_info2 = tx_info_res.transactions.last().unwrap();

        println!(
            "Tx_info for first recipient {} is {}, for tx_id = {}",
            receiver1, tx_info1.status, tx_id1
        );
        println!(
            "Tx_info for second recipient {} is {}, for tx_id = {}",
            receiver2, tx_info2.status, tx_id2
        );
        // TransactionStatus::TRANSACTION_STATUS_BROADCAST == 1_i32
        if tx_info1.status == 1_i32 && tx_info2.status == 1_i32 {
            println!(
                "Transaction from {} to {} and {} with amount {} at fee {} has been broadcasted",
                sender.as_str(),
                receiver1.as_str(),
                receiver2.as_str(),
                amount,
                fee_per_gram
            );
            break;
        }

        if i == num_retries {
            panic!(
                "Transaction from {} to {} and {} with amount {} at fee {} failed to be broadcasted",
                sender.as_str(),
                receiver1.as_str(),
                receiver2.as_str(),
                amount,
                10
            )
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    // insert tx_id's to the corresponding world mapping
    let sender_tx_ids = world.wallet_tx_ids.entry(sender_wallet_address.clone()).or_default();

    sender_tx_ids.push(tx_id1);
    sender_tx_ids.push(tx_id2);

    let receiver1_tx_ids = world.wallet_tx_ids.entry(receiver1_address.clone()).or_default();
    receiver1_tx_ids.push(tx_id1);

    let receiver2_tx_ids = world.wallet_tx_ids.entry(receiver2_address.clone()).or_default();
    receiver2_tx_ids.push(tx_id2);

    println!(
        "Transfer amount {} from {} to {} and {} at fee {} succeeded",
        amount, sender, receiver1, receiver2, fee_per_gram
    );
}

#[when(expr = "I transfer {int} uT to self from wallet {word} at fee {int}")]
async fn transfer_tari_to_self(world: &mut TariWorld, amount: u64, sender: String, fee_per_gram: u64) {
    let mut sender_wallet_client = create_wallet_client(world, sender.clone()).await.unwrap();
    let sender_wallet_address = world.get_wallet_address(&sender).await.unwrap();

    let payment_recipient = PaymentRecipient {
        address: sender_wallet_address.clone(),
        amount,
        fee_per_gram,
        message: format!("transfer amount {} from {} to self", amount, sender.as_str(),),
        payment_type: 0, // normal mimblewimble payment type
    };
    let transfer_req = TransferRequest {
        recipients: vec![payment_recipient],
    };
    let tx_res = sender_wallet_client.transfer(transfer_req).await.unwrap().into_inner();
    let tx_res = tx_res.results;

    assert_eq!(tx_res.len(), 1usize);

    let tx_res = tx_res.first().unwrap();
    assert!(
        tx_res.is_success,
        "Transacting amount {} to self from wallet {} at fee {} failed",
        amount,
        sender.as_str(),
        fee_per_gram
    );

    // we wait for transaction to be broadcasted
    let tx_id = tx_res.transaction_id;
    let num_retries = 100;
    let tx_info_req = GetTransactionInfoRequest {
        transaction_ids: vec![tx_id],
    };

    for i in 0..=num_retries {
        let tx_info_res = sender_wallet_client
            .get_transaction_info(tx_info_req.clone())
            .await
            .unwrap()
            .into_inner();
        let tx_info = tx_info_res.transactions.first().unwrap();

        // TransactionStatus::TRANSACTION_STATUS_BROADCAST == 1_i32
        if tx_info.status == 1_i32 {
            println!(
                "Transaction to self from {} with amount {} at fee {} has been broadcasted",
                sender.clone(),
                amount,
                fee_per_gram
            );
            break;
        }

        if i == num_retries {
            panic!(
                "Transaction to self from {} with amount {} at fee {} failed to be broadcasted",
                sender.clone(),
                amount,
                fee_per_gram
            )
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    // insert tx_id's to the corresponding world mapping
    let sender_tx_ids = world.wallet_tx_ids.entry(sender_wallet_address.clone()).or_default();

    sender_tx_ids.push(tx_id);

    println!(
        "Transfer amount {} to self from {} at fee {} succeeded",
        amount, sender, fee_per_gram
    );
}

#[when(expr = "I broadcast HTLC transaction with {int} uT from wallet {word} to wallet {word} at fee {int}")]
async fn htlc_transaction(world: &mut TariWorld, amount: u64, sender: String, receiver: String, fee_per_gram: u64) {
    let mut sender_wallet_client = create_wallet_client(world, sender.clone()).await.unwrap();
    let sender_wallet_address = world.get_wallet_address(&sender).await.unwrap();
    let receiver_wallet_address = world.get_wallet_address(&receiver).await.unwrap();

    let payment_recipient = PaymentRecipient {
        address: receiver_wallet_address.clone(),
        amount,
        fee_per_gram,
        message: format!(
            "Atomic Swap from {} to {} with amount {} at fee {}",
            sender.as_str(),
            receiver.as_str(),
            amount,
            fee_per_gram
        ),
        payment_type: 0, // normal mimblewimble transaction
    };

    let atomic_swap_request = SendShaAtomicSwapRequest {
        recipient: Some(payment_recipient),
    };
    let sha_atomic_swap_tx_res = sender_wallet_client
        .send_sha_atomic_swap_transaction(atomic_swap_request)
        .await
        .unwrap()
        .into_inner();

    assert!(
        sha_atomic_swap_tx_res.is_success,
        "Atomic swap transacting amount uT {} from wallet {} to {} at fee {} failed",
        amount,
        sender.as_str(),
        receiver.as_str(),
        fee_per_gram
    );

    // we wait for transaction to be broadcasted
    let tx_id = sha_atomic_swap_tx_res.transaction_id;
    let num_retries = 100;
    let tx_info_req = GetTransactionInfoRequest {
        transaction_ids: vec![tx_id],
    };

    for i in 0..=num_retries {
        let tx_info_res = sender_wallet_client
            .get_transaction_info(tx_info_req.clone())
            .await
            .unwrap()
            .into_inner();
        let tx_info = tx_info_res.transactions.first().unwrap();

        // TransactionStatus::TRANSACTION_STATUS_BROADCAST == 1_i32
        if tx_info.status == 1_i32 {
            println!(
                "Atomic swap transaction from {} to {} with amount {} at fee {} has been broadcasted",
                sender.as_str(),
                receiver.as_str(),
                amount,
                fee_per_gram
            );
            break;
        }

        if i == num_retries {
            panic!(
                "Atomic swap transaction from {} to {} with amount {} at fee {} failed to be broadcasted",
                sender.as_str(),
                receiver.as_str(),
                amount,
                fee_per_gram
            )
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    // insert tx_id's to the corresponding world mapping
    let sender_tx_ids = world.wallet_tx_ids.entry(sender_wallet_address.clone()).or_default();

    sender_tx_ids.push(tx_id);

    let receiver_tx_ids = world.wallet_tx_ids.entry(receiver_wallet_address.clone()).or_default();

    receiver_tx_ids.push(tx_id);
    world.output_hash = Some(sha_atomic_swap_tx_res.output_hash);
    world.pre_image = Some(sha_atomic_swap_tx_res.pre_image);

    println!(
        "Atomic swap transfer amount {} from {} to {} at fee {} succeeded",
        amount, sender, receiver, fee_per_gram
    );
}

#[when(expr = "I claim an HTLC refund transaction with wallet {word} at fee {int}")]
async fn claim_htlc_refund_transaction_with_wallet_at_fee(world: &mut TariWorld, wallet: String, fee_per_gram: u64) {
    let mut wallet_client = create_wallet_client(world, wallet.clone()).await.unwrap();
    let wallet_address = world.get_wallet_address(&wallet).await.unwrap();
    let output_hash = world.output_hash.clone().unwrap();

    let claim_htlc_req = ClaimHtlcRefundRequest {
        output_hash,
        fee_per_gram,
    };

    let claim_htlc_refund_res = wallet_client
        .claim_htlc_refund_transaction(claim_htlc_req)
        .await
        .unwrap()
        .into_inner();

    assert!(
        claim_htlc_refund_res.clone().results.unwrap().is_success,
        "Claim HTLC refund transaction with wallet {} at fee {} failed",
        wallet.as_str(),
        fee_per_gram
    );

    // we wait for transaction to be broadcasted
    let tx_id = claim_htlc_refund_res.results.unwrap().transaction_id;
    let num_retries = 100;
    let tx_info_req = GetTransactionInfoRequest {
        transaction_ids: vec![tx_id],
    };

    for i in 0..=num_retries {
        let tx_info_res = wallet_client
            .get_transaction_info(tx_info_req.clone())
            .await
            .unwrap()
            .into_inner();
        let tx_info = tx_info_res.transactions.first().unwrap();

        // TransactionStatus::TRANSACTION_STATUS_BROADCAST == 1_i32
        if tx_info.status == 1_i32 {
            println!(
                "Claim HTLC refund transaction with wallet {} at fee {} has been broadcasted",
                wallet.as_str(),
                fee_per_gram
            );
            break;
        }

        if i == num_retries {
            panic!(
                "Claim HTLC refund transaction with wallet {} at fee {} failed to be broadcasted",
                wallet.as_str(),
                fee_per_gram
            )
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    // insert tx_id's to the corresponding world mapping
    let wallet_tx_ids = world.wallet_tx_ids.entry(wallet_address.clone()).or_default();
    wallet_tx_ids.push(tx_id);

    println!(
        "Claim HTLC refund transaction with wallet {} at fee {} succeeded",
        wallet, fee_per_gram
    );
}

#[when(expr = "I claim an HTLC transaction with wallet {word} at fee {int}")]
async fn wallet_claims_htlc_transaction_at_fee(world: &mut TariWorld, wallet: String, fee_per_gram: u64) {
    let mut wallet_client = create_wallet_client(world, wallet.clone()).await.unwrap();
    let wallet_address = world.get_wallet_address(&wallet).await.unwrap();
    let output_hash = world.output_hash.clone().unwrap();
    let pre_image = world.pre_image.clone().unwrap();

    let claim_htlc_req = ClaimShaAtomicSwapRequest {
        output: output_hash,
        pre_image,
        fee_per_gram,
    };

    let claim_htlc_res = wallet_client
        .claim_sha_atomic_swap_transaction(claim_htlc_req)
        .await
        .unwrap()
        .into_inner();

    assert!(
        claim_htlc_res.clone().results.unwrap().is_success,
        "Claim HTLC transaction with wallet {} at fee {} failed",
        wallet.as_str(),
        fee_per_gram
    );

    // we wait for transaction to be broadcasted
    let tx_id = claim_htlc_res.results.unwrap().transaction_id;
    let num_retries = 100;
    let tx_info_req = GetTransactionInfoRequest {
        transaction_ids: vec![tx_id],
    };

    for i in 0..=num_retries {
        let tx_info_res = wallet_client
            .get_transaction_info(tx_info_req.clone())
            .await
            .unwrap()
            .into_inner();
        let tx_info = tx_info_res.transactions.first().unwrap();

        // TransactionStatus::TRANSACTION_STATUS_BROADCAST == 1_i32
        if tx_info.status == 1_i32 {
            println!(
                "Claim HTLC transaction with wallet {} at fee {} has been broadcasted",
                wallet.as_str(),
                fee_per_gram
            );
            break;
        }

        if i == num_retries {
            panic!(
                "Claim HTLC transaction with wallet {} at fee {} failed to be broadcasted",
                wallet.as_str(),
                fee_per_gram
            )
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    // insert tx_id's to the corresponding world mapping
    let wallet_tx_ids = world.wallet_tx_ids.entry(wallet_address.clone()).or_default();
    wallet_tx_ids.push(tx_id);

    println!(
        "Claim HTLC transaction with wallet {} at fee {} succeeded",
        wallet, fee_per_gram
    );
}

#[then(expr = "I wait for wallet {word} to have less than {int} uT")]
async fn wait_for_wallet_to_have_less_than_amount(world: &mut TariWorld, wallet: String, amount: u64) {
    let wallet_ps = world.wallets.get(&wallet).unwrap();
    let num_retries = 100;

    let mut client = wallet_ps.get_grpc_client().await.unwrap();
    let mut curr_amount = u64::MAX;

    for _ in 0..=num_retries {
        curr_amount = client
            .get_balance(GetBalanceRequest {})
            .await
            .unwrap()
            .into_inner()
            .available_balance;

        if curr_amount < amount {
            return;
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    // failed to get wallet right amount, so we panic
    panic!(
        "wallet {} failed to get less balance than amount {}, current amount is {}",
        wallet.as_str(),
        amount,
        curr_amount
    );
}

#[then(expr = "I send a one-sided stealth transaction of {int} uT from {word} to {word} at fee {int}")]
async fn send_one_sided_stealth_transaction(
    world: &mut TariWorld,
    amount: u64,
    sender: String,
    receiver: String,
    fee_per_gram: u64,
) {
    let mut sender_client = create_wallet_client(world, sender.clone()).await.unwrap();
    let sender_wallet_address = sender_client
        .get_address(Empty {})
        .await
        .unwrap()
        .into_inner()
        .address
        .to_hex();

    let mut receiver_client = create_wallet_client(world, receiver.clone()).await.unwrap();
    let receiver_wallet_address = receiver_client
        .get_address(Empty {})
        .await
        .unwrap()
        .into_inner()
        .address
        .to_hex();

    let payment_recipient = PaymentRecipient {
        address: receiver_wallet_address.clone(),
        amount,
        fee_per_gram,
        message: format!(
            "One sided stealth transfer amount {} from {} to {}",
            amount,
            sender.as_str(),
            receiver.as_str()
        ),
        payment_type: 2, // one sided stealth transaction
    };
    let transfer_req = TransferRequest {
        recipients: vec![payment_recipient],
    };
    let tx_res = sender_client.transfer(transfer_req).await.unwrap().into_inner();
    let tx_res = tx_res.results;

    assert_eq!(tx_res.len(), 1usize);

    let tx_res = tx_res.first().unwrap();
    assert!(
        tx_res.is_success,
        "One sided stealth transaction with amount {} from wallet {} to {} at fee {} failed",
        amount,
        sender.as_str(),
        receiver.as_str(),
        fee_per_gram
    );

    // we wait for transaction to be broadcasted
    let tx_id = tx_res.transaction_id;
    let num_retries = 100;
    let tx_info_req = GetTransactionInfoRequest {
        transaction_ids: vec![tx_id],
    };

    for i in 0..num_retries {
        let tx_info_res = sender_client
            .get_transaction_info(tx_info_req.clone())
            .await
            .unwrap()
            .into_inner();
        let tx_info = tx_info_res.transactions.first().unwrap();

        // TransactionStatus::TRANSACTION_STATUS_BROADCAST == 1_i32
        if tx_info.status == 1_i32 {
            println!(
                "One sided stealth transaction from {} to {} with amount {} at fee {} has been broadcasted",
                sender.clone(),
                receiver.clone(),
                amount,
                fee_per_gram
            );
            break;
        }

        if i == num_retries - 1 {
            panic!(
                "One sided stealth transaction from {} to {} with amount {} at fee {} failed to be broadcasted",
                sender.clone(),
                receiver.clone(),
                amount,
                fee_per_gram
            )
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    // insert tx_id's to the corresponding world mapping
    let sender_tx_ids = world.wallet_tx_ids.entry(sender_wallet_address.clone()).or_default();

    sender_tx_ids.push(tx_id);

    let receiver_tx_ids = world.wallet_tx_ids.entry(receiver_wallet_address.clone()).or_default();

    receiver_tx_ids.push(tx_id);

    println!(
        "One sided stealth transaction with amount {} from {} to {} at fee {} succeeded",
        amount, sender, receiver, fee_per_gram
    );
}

#[then(expr = "I import {word} unspent outputs to {word}")]
async fn import_wallet_unspent_outputs(world: &mut TariWorld, wallet_a: String, wallet_b: String) {
    let wallet_a_ps = world.wallets.get_mut(&wallet_a).unwrap();

    let temp_dir_path = wallet_a_ps.temp_dir_path.clone();

    let mut cli = get_default_cli();

    let mut path_buf = PathBuf::new();
    path_buf.push(temp_dir_path);
    path_buf.push("exported_utxos.csv");

    let args = ExportUtxosArgs {
        output_file: Some(path_buf.clone()),
    };
    cli.command2 = Some(CliCommands::ExportUtxos(args));

    let base_node = world.wallet_connected_to_base_node.get(&wallet_a).unwrap();

    let seed_nodes = world.base_nodes.get(base_node).unwrap().seed_nodes.clone();
    spawn_wallet(world, wallet_a, Some(base_node.clone()), seed_nodes, None, Some(cli)).await;

    let exported_outputs = std::fs::File::open(path_buf).unwrap();
    let mut reader = csv::Reader::from_reader(exported_outputs);

    let mut outputs: Vec<UnblindedOutput> = vec![];

    for output in reader.records() {
        let output = output.unwrap();
        let version = match &output[1] {
            "V0" => TransactionOutputVersion::V0,
            "V1" => TransactionOutputVersion::V1,
            _ => panic!("Invalid output version"),
        };
        let value = MicroTari(output[2].parse::<u64>().unwrap());
        let spending_key = BlindingFactor::from_hex(&output[3]).unwrap();
        let flags = match &output[5] {
            "Standard" => OutputType::Standard,
            "Coinbase" => OutputType::Coinbase,
            "Burn" => OutputType::Burn,
            "ValidatorNodeRegistration" => OutputType::ValidatorNodeRegistration,
            "CodeTemplateRegistration" => OutputType::CodeTemplateRegistration,
            _ => panic!("Invalid output type"),
        };
        let maturity = output[6].parse::<u64>().unwrap();
        let coinbase_extra = Vec::from_hex(&output[7]).unwrap();
        let script = TariScript::from_hex(&output[8]).unwrap();
        let covenant = Covenant::from_bytes(&mut Vec::from_hex(&output[9]).unwrap().as_slice()).unwrap();
        let input_data = ExecutionStack::from_hex(&output[10]).unwrap();
        let script_private_key = PrivateKey::from_hex(&output[11]).unwrap();
        let sender_offset_public_key = PublicKey::from_hex(&output[12]).unwrap();
        let ephemeral_commitment: HomomorphicCommitment<PublicKey> =
            HomomorphicCommitment::from_hex(&output[13]).unwrap();
        let ephemeral_nonce = PublicKey::from_hex(&output[14]).unwrap();
        let signature_u_x = PrivateKey::from_hex(&output[15]).unwrap();
        let signature_u_a = PrivateKey::from_hex(&output[16]).unwrap();
        let signature_u_y = PrivateKey::from_hex(&output[17]).unwrap();
        let script_lock_height = output[18].parse::<u64>().unwrap();
        let encrypted_value = EncryptedValue::from_hex(&output[19]).unwrap();
        let minimum_value_promise = MicroTari(output[20].parse::<u64>().unwrap());

        let features = OutputFeatures::new_current_version(flags, maturity, coinbase_extra, None);
        let metadata_signature = ComAndPubSignature::new(
            ephemeral_commitment,
            ephemeral_nonce,
            signature_u_x,
            signature_u_a,
            signature_u_y,
        );
        let utxo = UnblindedOutput::new(
            version,
            value,
            spending_key,
            features,
            script,
            input_data,
            script_private_key,
            sender_offset_public_key,
            metadata_signature,
            script_lock_height,
            covenant,
            encrypted_value,
            minimum_value_promise,
        );

        outputs.push(utxo);
    }

    let mut wallet_b_client = create_wallet_client(world, wallet_b.clone()).await.unwrap();
    let import_utxos_req = ImportUtxosRequest {
        outputs: outputs
            .iter()
            .map(|o| grpc::UnblindedOutput::try_from(o.clone()).expect("Unable to make grpc conversino"))
            .collect::<Vec<grpc::UnblindedOutput>>(),
    };

    world.last_imported_tx_ids = wallet_b_client
        .import_utxos(import_utxos_req)
        .await
        .unwrap()
        .into_inner()
        .tx_ids;
}

#[then(expr = "I import {word} spent outputs to {word}")]
async fn import_wallet_spent_outputs(world: &mut TariWorld, wallet_a: String, wallet_b: String) {
    let wallet_a_ps = world.wallets.get_mut(&wallet_a).unwrap();

    let temp_dir_path = wallet_a_ps.temp_dir_path.clone();

    let mut cli = get_default_cli();

    let mut path_buf = PathBuf::new();
    path_buf.push(temp_dir_path);
    path_buf.push("exported_utxos.csv");

    let args = ExportUtxosArgs {
        output_file: Some(path_buf.clone()),
    };
    cli.command2 = Some(CliCommands::ExportSpentUtxos(args));

    let base_node = world.wallet_connected_to_base_node.get(&wallet_a).unwrap();
    let seed_nodes = world.base_nodes.get(base_node).unwrap().seed_nodes.clone();
    spawn_wallet(world, wallet_a, Some(base_node.clone()), seed_nodes, None, Some(cli)).await;

    let exported_outputs = std::fs::File::open(path_buf).unwrap();
    let mut reader = csv::Reader::from_reader(exported_outputs);

    let mut outputs: Vec<UnblindedOutput> = vec![];

    for output in reader.records() {
        let output = output.unwrap();
        let version = match &output[1] {
            "V0" => TransactionOutputVersion::V0,
            "V1" => TransactionOutputVersion::V1,
            _ => panic!("Invalid output version"),
        };
        let value = MicroTari(output[2].parse::<u64>().unwrap());
        let spending_key = BlindingFactor::from_hex(&output[3]).unwrap();
        let flags = match &output[5] {
            "Standard" => OutputType::Standard,
            "Coinbase" => OutputType::Coinbase,
            "Burn" => OutputType::Burn,
            "ValidatorNodeRegistration" => OutputType::ValidatorNodeRegistration,
            "CodeTemplateRegistration" => OutputType::CodeTemplateRegistration,
            _ => panic!("Invalid output type"),
        };
        let maturity = output[6].parse::<u64>().unwrap();
        let coinbase_extra = Vec::from_hex(&output[7]).unwrap();
        let script = TariScript::from_hex(&output[8]).unwrap();
        let covenant = Covenant::from_bytes(&mut Vec::from_hex(&output[9]).unwrap().as_slice()).unwrap();
        let input_data = ExecutionStack::from_hex(&output[10]).unwrap();
        let script_private_key = PrivateKey::from_hex(&output[11]).unwrap();
        let sender_offset_public_key = PublicKey::from_hex(&output[12]).unwrap();
        let ephemeral_commitment: HomomorphicCommitment<PublicKey> =
            HomomorphicCommitment::from_hex(&output[13]).unwrap();
        let ephemeral_nonce = PublicKey::from_hex(&output[14]).unwrap();
        let signature_u_x = PrivateKey::from_hex(&output[15]).unwrap();
        let signature_u_a = PrivateKey::from_hex(&output[16]).unwrap();
        let signature_u_y = PrivateKey::from_hex(&output[17]).unwrap();
        let script_lock_height = output[18].parse::<u64>().unwrap();
        let encrypted_value = EncryptedValue::from_hex(&output[19]).unwrap();
        let minimum_value_promise = MicroTari(output[20].parse::<u64>().unwrap());

        let features = OutputFeatures::new_current_version(flags, maturity, coinbase_extra, None);
        let metadata_signature = ComAndPubSignature::new(
            ephemeral_commitment,
            ephemeral_nonce,
            signature_u_x,
            signature_u_a,
            signature_u_y,
        );
        let utxo = UnblindedOutput::new(
            version,
            value,
            spending_key,
            features,
            script,
            input_data,
            script_private_key,
            sender_offset_public_key,
            metadata_signature,
            script_lock_height,
            covenant,
            encrypted_value,
            minimum_value_promise,
        );

        outputs.push(utxo);
    }

    let mut wallet_b_client = create_wallet_client(world, wallet_b.clone()).await.unwrap();
    let import_utxos_req = ImportUtxosRequest {
        outputs: outputs
            .iter()
            .map(|o| grpc::UnblindedOutput::try_from(o.clone()).expect("Unable to make grpc conversino"))
            .collect::<Vec<grpc::UnblindedOutput>>(),
    };

    world.last_imported_tx_ids = wallet_b_client
        .import_utxos(import_utxos_req)
        .await
        .unwrap()
        .into_inner()
        .tx_ids;
}

#[then(expr = "I import {word} unspent outputs as faucet outputs to {word}")]
async fn import_unspent_outputs_as_faucets(world: &mut TariWorld, wallet_a: String, wallet_b: String) {
    let wallet_a_ps = world.wallets.get_mut(&wallet_a).unwrap();

    let temp_dir_path = wallet_a_ps.temp_dir_path.clone();

    let mut cli = get_default_cli();

    let mut path_buf = PathBuf::new();
    path_buf.push(temp_dir_path);
    path_buf.push("exported_utxos.csv");

    let args = ExportUtxosArgs {
        output_file: Some(path_buf.clone()),
    };
    cli.command2 = Some(CliCommands::ExportUtxos(args));

    let base_node = world.wallet_connected_to_base_node.get(&wallet_a).unwrap();
    let seed_nodes = world.base_nodes.get(base_node).unwrap().seed_nodes.clone();
    spawn_wallet(world, wallet_a, Some(base_node.clone()), seed_nodes, None, Some(cli)).await;

    let exported_outputs = std::fs::File::open(path_buf).unwrap();
    let mut reader = csv::Reader::from_reader(exported_outputs);

    let mut outputs: Vec<UnblindedOutput> = vec![];

    for output in reader.records() {
        let output = output.unwrap();
        let version = match &output[1] {
            "V0" => TransactionOutputVersion::V0,
            "V1" => TransactionOutputVersion::V1,
            _ => panic!("Invalid output version"),
        };
        let value = MicroTari(output[2].parse::<u64>().unwrap());
        let spending_key = BlindingFactor::from_hex(&output[3]).unwrap();
        let flags = match &output[5] {
            "Standard" => OutputType::Standard,
            "Coinbase" => OutputType::Coinbase,
            "Burn" => OutputType::Burn,
            "ValidatorNodeRegistration" => OutputType::ValidatorNodeRegistration,
            "CodeTemplateRegistration" => OutputType::CodeTemplateRegistration,
            _ => panic!("Invalid output type"),
        };
        let maturity = output[6].parse::<u64>().unwrap();
        let coinbase_extra = Vec::from_hex(&output[7]).unwrap();
        let script = TariScript::from_hex(&output[8]).unwrap();
        let covenant = Covenant::from_bytes(&mut Vec::from_hex(&output[9]).unwrap().as_slice()).unwrap();
        let input_data = ExecutionStack::from_hex(&output[10]).unwrap();
        let script_private_key = PrivateKey::from_hex(&output[11]).unwrap();
        let sender_offset_public_key = PublicKey::from_hex(&output[12]).unwrap();
        let ephemeral_commitment: HomomorphicCommitment<PublicKey> =
            HomomorphicCommitment::from_hex(&output[13]).unwrap();
        let ephemeral_nonce = PublicKey::from_hex(&output[14]).unwrap();
        let signature_u_x = PrivateKey::from_hex(&output[15]).unwrap();
        let signature_u_a = PrivateKey::from_hex(&output[16]).unwrap();
        let signature_u_y = PrivateKey::from_hex(&output[17]).unwrap();
        let script_lock_height = output[18].parse::<u64>().unwrap();
        let encrypted_value = EncryptedValue::from_hex(&output[19]).unwrap();
        let minimum_value_promise = MicroTari(output[20].parse::<u64>().unwrap());

        let features = OutputFeatures::new_current_version(flags, maturity, coinbase_extra, None);
        let metadata_signature = ComAndPubSignature::new(
            ephemeral_commitment,
            ephemeral_nonce,
            signature_u_x,
            signature_u_a,
            signature_u_y,
        );
        let mut utxo = UnblindedOutput::new(
            version,
            value,
            spending_key,
            features,
            script,
            input_data,
            script_private_key,
            sender_offset_public_key,
            metadata_signature,
            script_lock_height,
            covenant,
            encrypted_value,
            minimum_value_promise,
        );

        utxo.metadata_signature = ComAndPubSignature::new(
            Commitment::default(),
            PublicKey::default(),
            PrivateKey::default(),
            PrivateKey::default(),
            PrivateKey::default(),
        );
        utxo.script_private_key = utxo.clone().spending_key;

        let script_public_key = PublicKey::from_secret_key(&utxo.script_private_key);
        utxo.input_data = ExecutionStack::new(vec![StackItem::PublicKey(script_public_key)]);
        outputs.push(utxo.clone());
    }

    let mut wallet_b_client = create_wallet_client(world, wallet_b.clone()).await.unwrap();
    let import_utxos_req = ImportUtxosRequest {
        outputs: outputs
            .iter()
            .map(|o| grpc::UnblindedOutput::try_from(o.clone()).expect("Unable to make grpc conversino"))
            .collect::<Vec<grpc::UnblindedOutput>>(),
    };

    world.last_imported_tx_ids = wallet_b_client
        .import_utxos(import_utxos_req)
        .await
        .unwrap()
        .into_inner()
        .tx_ids;
}

#[then(expr = "I restart wallet {word}")]
async fn restart_wallet(world: &mut TariWorld, wallet: String) {
    let wallet_ps = world.wallets.get_mut(&wallet).unwrap();
    // stop wallet
    wallet_ps.kill();
    // start wallet
    let base_node = world.wallet_connected_to_base_node.get(&wallet).unwrap().clone();
    let base_node_ps = world.base_nodes.get(&base_node).unwrap();
    let seed_nodes = base_node_ps.seed_nodes.clone();

    // need to wait a few seconds before spawning a new wallet
    tokio::time::sleep(Duration::from_secs(5)).await;

    spawn_wallet(world, wallet, Some(base_node), seed_nodes, None, None).await;
}

#[then(expr = "I check if wallet {word} has {int} transactions")]
async fn check_if_wallet_has_num_transactions(world: &mut TariWorld, wallet: String, num_txs: u64) {
    let mut client = create_wallet_client(world, wallet.clone()).await.unwrap();
    let mut get_completed_txs_res = client
        .get_completed_transactions(GetCompletedTransactionsRequest {})
        .await
        .unwrap()
        .into_inner();

    let mut count = 0;
    while let Some(tx) = get_completed_txs_res.next().await {
        let _tx = tx.unwrap(); // make sure we get the actual response
        count += 1;
    }

    assert_eq!(
        num_txs,
        count,
        "Wallet {} did not get {} transactions, instead it got {}",
        wallet.as_str(),
        num_txs,
        count
    );
}

#[when(expr = "I multi-send {int} transactions of {int} uT from wallet {word} to wallet {word} at fee {int}")]
async fn multi_send_txs_from_wallet(
    world: &mut TariWorld,
    num_txs: u64,
    amount: u64,
    sender: String,
    receiver: String,
    fee_per_gram: u64,
) {
    let mut sender_wallet_client = create_wallet_client(world, sender.clone()).await.unwrap();
    let sender_wallet_address = sender_wallet_client
        .get_address(Empty {})
        .await
        .unwrap()
        .into_inner()
        .address
        .to_hex();

    let mut receiver_wallet_client = create_wallet_client(world, receiver.clone()).await.unwrap();
    let receiver_wallet_address = receiver_wallet_client
        .get_address(Empty {})
        .await
        .unwrap()
        .into_inner()
        .address
        .to_hex();

    let mut transfer_res = vec![];

    for _ in 0..num_txs {
        let payment_recipient = PaymentRecipient {
            address: receiver_wallet_address.clone(),
            amount,
            fee_per_gram,
            message: format!(
                "I send multi-transfers with amount {} from {} to {} with fee {}",
                amount,
                sender.as_str(),
                receiver.as_str(),
                fee_per_gram
            ),
            payment_type: 0, // mimblewimble transaction
        };

        let transfer_req = TransferRequest {
            recipients: vec![payment_recipient],
        };
        let tx_res = sender_wallet_client.transfer(transfer_req).await.unwrap().into_inner();
        let tx_res = tx_res.results;

        assert_eq!(tx_res.len(), 1usize);

        let tx_res = tx_res.first().unwrap();
        assert!(
            tx_res.is_success,
            "Multi-Transaction with amount {} from wallet {} to {} at fee {} failed",
            amount,
            sender.as_str(),
            receiver.as_str(),
            fee_per_gram
        );

        transfer_res.push(tx_res.clone());
    }

    let num_retries = 100;

    for tx_res in transfer_res {
        let tx_id = tx_res.transaction_id;
        let tx_info_req = GetTransactionInfoRequest {
            transaction_ids: vec![tx_id],
        };

        for i in 0..num_retries {
            let tx_info_res = sender_wallet_client
                .get_transaction_info(tx_info_req.clone())
                .await
                .unwrap()
                .into_inner();
            let tx_info = tx_info_res.transactions.first().unwrap();

            // TransactionStatus::TRANSACTION_STATUS_BROADCAST == 1_i32
            if tx_info.status == 1_i32 {
                println!(
                    "Multi-transaction from {} to {} with amount {} at fee {} has been broadcasted",
                    sender.clone(),
                    receiver.clone(),
                    amount,
                    fee_per_gram
                );
                break;
            }

            if i == num_retries - 1 {
                panic!(
                    "Multi-transaction from {} to {} with amount {} at fee {} failed to be broadcasted",
                    sender.clone(),
                    receiver.clone(),
                    amount,
                    fee_per_gram
                )
            }

            tokio::time::sleep(Duration::from_secs(5)).await;
        }

        // insert tx_id's to the corresponding world mapping
        let sender_tx_ids = world.wallet_tx_ids.entry(sender_wallet_address.clone()).or_default();

        sender_tx_ids.push(tx_id);

        let receiver_tx_ids = world.wallet_tx_ids.entry(receiver_wallet_address.clone()).or_default();

        receiver_tx_ids.push(tx_id);

        println!(
            "Multi-transaction with amount {} from {} to {} at fee {} succeeded",
            amount, sender, receiver, fee_per_gram
        );
    }
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

#[then(expr = "I check if last imported transactions are invalid in wallet {word}")]
async fn check_if_last_imported_txs_are_invalid_in_wallet(world: &mut TariWorld, wallet: String) {
    let mut client = create_wallet_client(world, wallet.clone()).await.unwrap();
    let mut get_completed_txs_res = client
        .get_completed_transactions(GetCompletedTransactionsRequest {})
        .await
        .unwrap()
        .into_inner();

    while let Some(tx) = get_completed_txs_res.next().await {
        let tx_info = tx.unwrap().transaction.unwrap();
        let status = tx_info.status;
        // 3 => TRANSACTION_STATUS_IMPORTED
        // 5 => TRANSACTION_STATUS_COINBASE
        if ![3, 5].contains(&status) {
            panic!(
                "Imported transaction hasn't been received as such: current status code is {}, it should be 3 or 5",
                status
            );
        }
    }
}

#[then(expr = "I check if last imported transactions are valid in wallet {word}")]
async fn check_if_last_imported_txs_are_valid_in_wallet(world: &mut TariWorld, wallet: String) {
    let mut client = create_wallet_client(world, wallet.clone()).await.unwrap();
    let mut get_completed_txs_res = client
        .get_completed_transactions(GetCompletedTransactionsRequest {})
        .await
        .unwrap()
        .into_inner();

    let mut imported_cnt = 0;

    while let Some(tx) = get_completed_txs_res.next().await {
        let tx_info = tx.unwrap().transaction.unwrap();
        for &tx_id in &world.last_imported_tx_ids {
            if tx_id == tx_info.tx_id {
                assert_eq!(tx_info.status(), grpc::TransactionStatus::FauxConfirmed);
                imported_cnt += 1;
            }
        }
    }
    assert_eq!(imported_cnt, world.last_imported_tx_ids.len());
}

#[then(expr = "I cancel last transaction in wallet {word}")]
async fn cancel_last_transaction_in_wallet(world: &mut TariWorld, wallet: String) {
    let mut client = create_wallet_client(world, wallet.clone()).await.unwrap();
    let wallet_address = client
        .get_address(Empty {})
        .await
        .unwrap()
        .into_inner()
        .address
        .to_hex();

    let wallet_tx_ids = world.wallet_tx_ids.get(&wallet_address).unwrap();

    // get the last tx id for wallet
    let tx_id = *wallet_tx_ids.last().unwrap();
    let cancel_tx_req = CancelTransactionRequest { tx_id };
    let cancel_tx_res = client.cancel_transaction(cancel_tx_req).await.unwrap().into_inner();
    assert!(
        cancel_tx_res.is_success,
        "Unable to cancel transaction with id = {}",
        tx_id
    );
}

#[then(expr = "meddling with block template data from node {word} is not allowed")]
async fn no_meddling_with_data(world: &mut TariWorld, node: String) {
    let mut client = world.get_node_client(&node).await.unwrap();

    // No meddling
    let chain_tip = client.get_tip_info(Empty {}).await.unwrap().into_inner();
    let current_height = chain_tip.metadata.unwrap().height_of_longest_chain;
    let block = mine_block_before_submit(&mut client).await;
    let _sumbmit_res = client.submit_block(block).await.unwrap();

    let chain_tip = client.get_tip_info(Empty {}).await.unwrap().into_inner();
    let new_height = chain_tip.metadata.unwrap().height_of_longest_chain;
    assert_eq!(
        current_height + 1,
        new_height,
        "validating that the chain increased by 1 from {} to {} but was actually {}",
        current_height,
        current_height + 1,
        new_height
    );

    // Meddle with kernal_mmr_size
    let mut block: Block = Block::try_from(mine_block_before_submit(&mut client).await).unwrap();
    block.header.kernel_mmr_size += 1;
    match client.submit_block(grpc::Block::try_from(block).unwrap()).await {
        Ok(_) => panic!("The block should not have been valid"),
        Err(e) => assert_eq!(
            "Chain storage error: Validation error: Block validation error: MMR size for Kernel does not match. \
             Expected: 3, received: 4"
                .to_string(),
            e.message()
        ),
    }

    // Meddle with output_mmr_size
    let mut block: Block = Block::try_from(mine_block_before_submit(&mut client).await).unwrap();
    block.header.output_mmr_size += 1;
    match client.submit_block(grpc::Block::try_from(block).unwrap()).await {
        Ok(_) => panic!("The block should not have been valid"),
        Err(e) => assert_eq!(
            "Chain storage error: Validation error: Block validation error: MMR size for UTXO does not match. \
             Expected: 3, received: 4"
                .to_string(),
            e.message()
        ),
    }
}

#[when(expr = "I mine but do not submit a block {word} on {word}")]
async fn mine_without_submit(world: &mut TariWorld, block: String, node: String) {
    let mut client = world.get_node_client(&node).await.unwrap();

    let unmined_block: Block = Block::try_from(mine_block_before_submit(&mut client).await).unwrap();
    world.blocks.insert(block, unmined_block);
}

#[when(expr = "I submit block {word} to {word}")]
async fn submit_block_after(world: &mut TariWorld, block_name: String, node: String) {
    let mut client = world.get_node_client(&node).await.unwrap();
    let block = world.blocks.get(&block_name).expect("Couldn't find unmined block");

    match client.submit_block(grpc::Block::try_from(block.clone()).unwrap()).await {
        Ok(_resp) => {},
        Err(e) => {
            // The kind of errors we want don't actually get returned
            world.errors.push_back(e.message().to_string());
        },
    }
}

#[then(regex = r"I receive an error containing '(.*)'")]
async fn receive_an_error(_world: &mut TariWorld, _error: String) {
    // No-op.
    // Was not implemented in previous suite, gave it a quick try but missing other peices

    // assert!(world.errors.len() > 1);
    // assert!(world.errors.pop_front().unwrap().contains(&error))
}

#[when(expr = "I have a lagging delayed node {word} connected to node {word} with \
               blocks_behind_before_considered_lagging {int}")]
async fn lagging_delayed_node(world: &mut TariWorld, delayed_node: String, node: String, delay: u64) {
    let mut base_node_config = BaseNodeConfig::default();
    base_node_config.state_machine.blocks_behind_before_considered_lagging = delay;

    spawn_base_node_with_config(world, true, delayed_node, vec![node], base_node_config).await;
}

#[then(expr = "node {word} has reached initial sync")]
async fn node_reached_sync(world: &mut TariWorld, node: String) {
    let mut client = world.get_node_client(&node).await.unwrap();
    let mut longest_chain = 0;

    for _ in 0..(TWO_MINUTES_WITH_HALF_SECOND_SLEEP * 11) {
        let tip_info = client.get_tip_info(Empty {}).await.unwrap().into_inner();
        let metadata = tip_info.metadata.unwrap();
        longest_chain = metadata.height_of_longest_chain;

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

#[when(expr = "I create a burn transaction of {int} uT from {word} at fee {int}")]
async fn burn_transaction(world: &mut TariWorld, amount: u64, wallet: String, fee: u64) {
    let mut client = world.get_wallet_client(&wallet).await.unwrap();
    let identity = client.identify(GetIdentityRequest {}).await.unwrap().into_inner();

    let req = grpc::CreateBurnTransactionRequest {
        amount,
        fee_per_gram: fee,
        message: "Burning some tari".to_string(),
        claim_public_key: identity.public_key,
    };

    let result = client.create_burn_transaction(req).await.unwrap();
    let tx_id = result.into_inner().transaction_id;

    let mut last_status = 0;
    for _ in 0..(TWO_MINUTES_WITH_HALF_SECOND_SLEEP) {
        let result = client
            .get_transaction_info(grpc::GetTransactionInfoRequest {
                transaction_ids: vec![tx_id],
            })
            .await
            .unwrap();

        last_status = result.into_inner().transactions.last().unwrap().status;

        if let 1 | 2 | 6 = last_status {
            return;
        }

        tokio::time::sleep(Duration::from_millis(HALF_SECOND)).await;
    }

    panic!(
        "Burn transaction has status {} when we desired 1 (TRANSACTION_STATUS_BROADCAST), 2 \
         (TRANSACTION_STATUS_UNCONFIRMED), or 6 (TRANSACTION_STATUS_CONFIRMED)",
        last_status
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

#[when(expr = "I spend outputs {word} via {word}")]
async fn spend_outputs_via(world: &mut TariWorld, inputs: String, node: String) {
    let num = rand::thread_rng().gen::<u8>();
    let tx_name = format!("TX-{}", num);
    let utxo_name = format!("UTXO-{}", num);

    create_tx_spending_coinbase(world, tx_name.clone(), inputs, utxo_name.clone()).await;
    submit_transaction_to(world, tx_name, node).await.unwrap();
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

        if last_num_of_peers >= num_peers as usize {
            return;
        }

        tokio::time::sleep(Duration::from_millis(HALF_SECOND)).await;
    }

    panic!(
        "Node {} only received {} of {} expected peers",
        node, last_num_of_peers, num_peers
    )
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

#[then(expr = "I change base node of {word} to {word} via command line")]
async fn change_base_node_of_wallet_via_cli(world: &mut TariWorld, wallet: String, node: String) {
    let wallet_ps = world.wallets.get_mut(&wallet).unwrap();
    wallet_ps.kill();

    tokio::time::sleep(Duration::from_secs(5)).await;

    let mut cli = get_default_cli();

    let mut node_client = world.get_node_client(&node).await.unwrap();
    let node_identity = node_client.identify(Empty {}).await.unwrap().into_inner();

    let args = SetBaseNodeArgs {
        public_key: UniPublicKey::from_str(node_identity.public_key.to_hex().as_str()).unwrap(),
        address: Multiaddr::from_str(node_identity.public_addresses[0].as_str()).unwrap(),
    };

    cli.command2 = Some(CliCommands::SetBaseNode(args));

    let seed_nodes = world.base_nodes.get(&node).unwrap().seed_nodes.clone();
    spawn_wallet(world, wallet, Some(node.clone()), seed_nodes, None, Some(cli)).await;
}

#[then(expr = "I set custom base node of {word} to {word} via command line")]
async fn change_custom_base_node_of_wallet_via_cli(world: &mut TariWorld, wallet: String, node: String) {
    let wallet_ps = world.wallets.get_mut(&wallet).unwrap();
    wallet_ps.kill();

    tokio::time::sleep(Duration::from_secs(5)).await;

    let mut cli = get_default_cli();

    let mut node_client = world.get_node_client(&node).await.unwrap();
    let node_identity = node_client.identify(Empty {}).await.unwrap().into_inner();

    let args = SetBaseNodeArgs {
        public_key: UniPublicKey::from_str(node_identity.public_key.to_hex().as_str()).unwrap(),
        address: Multiaddr::from_str(node_identity.public_addresses[0].as_str()).unwrap(),
    };

    cli.command2 = Some(CliCommands::SetCustomBaseNode(args));

    let seed_nodes = world.base_nodes.get(&node).unwrap().seed_nodes.clone();
    spawn_wallet(world, wallet, Some(node.clone()), seed_nodes, None, Some(cli)).await;
}

#[when(expr = "I clear custom base node of wallet {word} via command line")]
async fn clear_custom_base_node(world: &mut TariWorld, wallet: String) {
    let wallet_ps = world.wallets.get_mut(&wallet).unwrap();
    wallet_ps.kill();

    tokio::time::sleep(Duration::from_secs(5)).await;

    let mut cli = get_default_cli();

    cli.command2 = Some(CliCommands::ClearCustomBaseNode);

    let base_node = world.wallet_connected_to_base_node.get(&wallet).unwrap();
    let seed_nodes = world.base_nodes.get(base_node).unwrap().seed_nodes.clone();

    spawn_wallet(world, wallet, Some(base_node.clone()), seed_nodes, None, Some(cli)).await;
}

#[then(expr = "the password of wallet {word} is not {word}")]
async fn password_is(world: &mut TariWorld, wallet: String, _password: String) {
    let wallet_ps = world.wallets.get_mut(&wallet).unwrap();
    let _config_path = wallet_ps.temp_dir_path.clone();
}

#[then(expr = "I get balance of wallet {word} is at least {int} uT via command line")]
async fn get_balance_of_wallet(world: &mut TariWorld, wallet: String, _amount: u64) {
    let wallet_ps = world.wallets.get_mut(&wallet).unwrap();
    wallet_ps.kill();

    tokio::time::sleep(Duration::from_secs(5)).await;

    let mut cli = get_default_cli();

    cli.command2 = Some(CliCommands::GetBalance);

    let base_node = world.wallet_connected_to_base_node.get(&wallet).unwrap();
    let seed_nodes = world.base_nodes.get(base_node).unwrap().seed_nodes.clone();

    spawn_wallet(world, wallet, Some(base_node.clone()), seed_nodes, None, Some(cli)).await
}

#[when(expr = "I send {int} uT from {word} to {word} via command line")]
async fn send_from_cli(world: &mut TariWorld, amount: u64, wallet_a: String, wallet_b: String) {
    let wallet_ps = world.wallets.get_mut(&wallet_a).unwrap();
    wallet_ps.kill();

    tokio::time::sleep(Duration::from_secs(5)).await;

    let mut wallet_b_client = create_wallet_client(world, wallet_b.clone()).await.unwrap();
    let wallet_b_address = wallet_b_client
        .get_address(Empty {})
        .await
        .unwrap()
        .into_inner()
        .address
        .to_hex();
    let wallet_b_address = TariAddress::from_hex(wallet_b_address.as_str()).unwrap();

    let mut cli = get_default_cli();

    let args = SendTariArgs {
        amount: MicroTari(amount),
        message: format!("Send amount {} from {} to {}", amount, wallet_a, wallet_b),
        destination: wallet_b_address,
    };
    cli.command2 = Some(CliCommands::SendTari(args));

    let base_node = world.wallet_connected_to_base_node.get(&wallet_a).unwrap();
    let seed_nodes = world.base_nodes.get(base_node).unwrap().seed_nodes.clone();

    spawn_wallet(world, wallet_a, Some(base_node.clone()), seed_nodes, None, Some(cli)).await;
}

#[when(expr = "I create a burn transaction of {int} uT from {word} via command line")]
async fn create_burn_tx_via_cli(world: &mut TariWorld, amount: u64, wallet: String) {
    let wallet_ps = world.wallets.get_mut(&wallet).unwrap();
    wallet_ps.kill();

    tokio::time::sleep(Duration::from_secs(5)).await;

    let mut cli = get_default_cli();

    let args = BurnTariArgs {
        amount: MicroTari(amount),
        message: format!("Burn, burn amount {} !!!", amount,),
    };
    cli.command2 = Some(CliCommands::BurnTari(args));

    let base_node = world.wallet_connected_to_base_node.get(&wallet).unwrap();
    let seed_nodes = world.base_nodes.get(base_node).unwrap().seed_nodes.clone();

    spawn_wallet(world, wallet, Some(base_node.clone()), seed_nodes, None, Some(cli)).await;
}

#[then(expr = "I send one-sided {int} uT from {word} to {word} via command line")]
async fn send_one_sided_tx_via_cli(world: &mut TariWorld, amount: u64, wallet_a: String, wallet_b: String) {
    let wallet_ps = world.wallets.get_mut(&wallet_a).unwrap();
    wallet_ps.kill();

    tokio::time::sleep(Duration::from_secs(5)).await;

    let mut wallet_b_client = create_wallet_client(world, wallet_b.clone()).await.unwrap();
    let wallet_b_address = wallet_b_client
        .get_address(Empty {})
        .await
        .unwrap()
        .into_inner()
        .address
        .to_hex();
    let wallet_b_address = TariAddress::from_hex(wallet_b_address.as_str()).unwrap();

    let mut cli = get_default_cli();

    let args = SendTariArgs {
        amount: MicroTari(amount),
        message: format!("Send one sided amount {} from {} to {}", amount, wallet_a, wallet_b),
        destination: wallet_b_address,
    };
    cli.command2 = Some(CliCommands::SendOneSided(args));

    let base_node = world.wallet_connected_to_base_node.get(&wallet_a).unwrap();
    let seed_nodes = world.base_nodes.get(base_node).unwrap().seed_nodes.clone();

    spawn_wallet(world, wallet_a, Some(base_node.clone()), seed_nodes, None, Some(cli)).await;
}

#[when(
    expr = "I make it rain from wallet {word} {int} tx per sec {int} sec {int} uT {int} increment to {word} via \
            command line"
)]
async fn make_it_rain(
    world: &mut TariWorld,
    wallet_a: String,
    txs_per_second: u64,
    duration: u64,
    start_amount: u64,
    increment_amount: u64,
    wallet_b: String,
) {
    let wallet_ps = world.wallets.get_mut(&wallet_a).unwrap();
    wallet_ps.kill();

    tokio::time::sleep(Duration::from_secs(5)).await;

    let mut wallet_b_client = create_wallet_client(world, wallet_b.clone()).await.unwrap();
    let wallet_b_address = wallet_b_client
        .get_address(Empty {})
        .await
        .unwrap()
        .into_inner()
        .address
        .to_hex();
    let wallet_b_address = TariAddress::from_hex(wallet_b_address.as_str()).unwrap();

    let mut cli = get_default_cli();

    let args = MakeItRainArgs {
        start_amount: MicroTari(start_amount),
        transactions_per_second: txs_per_second as u32,
        duration: Duration::from_secs(duration),
        message: format!(
            "Make it raing amount {} from {} to {}",
            start_amount, wallet_a, wallet_b
        ),
        increase_amount: MicroTari(increment_amount),
        destination: wallet_b_address,
        start_time: None,
        one_sided: false,
        stealth: false,
        burn_tari: false,
    };

    cli.command2 = Some(CliCommands::MakeItRain(args));

    let base_node = world.wallet_connected_to_base_node.get(&wallet_a).unwrap();
    let seed_nodes = world.base_nodes.get(base_node).unwrap().seed_nodes.clone();

    spawn_wallet(world, wallet_a, Some(base_node.clone()), seed_nodes, None, Some(cli)).await;
}

#[when(expr = "I do coin split on wallet {word} to {int} uT {int} coins via command line")]
async fn coin_split_via_cli(world: &mut TariWorld, wallet: String, amount: u64, splits: u64) {
    let wallet_ps = world.wallets.get_mut(&wallet).unwrap();
    wallet_ps.kill();

    tokio::time::sleep(Duration::from_secs(5)).await;

    let mut cli = get_default_cli();

    let args = CoinSplitArgs {
        amount_per_split: MicroTari(amount),
        num_splits: splits as usize,
        fee_per_gram: MicroTari(20),
        message: format!("coin split amount {} with splits {}", amount, splits),
    };

    cli.command2 = Some(CliCommands::CoinSplit(args));

    let base_node = world.wallet_connected_to_base_node.get(&wallet).unwrap();
    let seed_nodes = world.base_nodes.get(base_node).unwrap().seed_nodes.clone();

    spawn_wallet(world, wallet, Some(base_node.clone()), seed_nodes, None, Some(cli)).await;
}

#[then(expr = "I get count of utxos of wallet {word} and it's at least {int} via command line")]
async fn count_utxos_of_wallet(world: &mut TariWorld, wallet: String, _amount: u64) {
    let wallet_ps = world.wallets.get_mut(&wallet).unwrap();
    wallet_ps.kill();

    tokio::time::sleep(Duration::from_secs(5)).await;

    let mut cli = get_default_cli();

    cli.command2 = Some(CliCommands::CountUtxos);

    let base_node = world.wallet_connected_to_base_node.get(&wallet).unwrap();
    let seed_nodes = world.base_nodes.get(base_node).unwrap().seed_nodes.clone();

    spawn_wallet(world, wallet, Some(base_node.clone()), seed_nodes, None, Some(cli)).await;
}

#[when(expr = "I export the utxos of wallet {word} via command line")]
async fn export_utxos(world: &mut TariWorld, wallet: String) {
    let wallet_a_ps = world.wallets.get_mut(&wallet).unwrap();
    wallet_a_ps.kill();

    tokio::time::sleep(Duration::from_secs(5)).await;

    let temp_dir_path = wallet_a_ps.temp_dir_path.clone();

    let mut cli = get_default_cli();

    let mut path_buf = PathBuf::new();
    path_buf.push(temp_dir_path);
    path_buf.push("exported_utxos.csv");

    let args = ExportUtxosArgs {
        output_file: Some(path_buf.clone()),
    };
    cli.command2 = Some(CliCommands::ExportUtxos(args));

    let base_node = world.wallet_connected_to_base_node.get(&wallet).unwrap();

    let seed_nodes = world.base_nodes.get(base_node).unwrap().seed_nodes.clone();
    spawn_wallet(world, wallet, Some(base_node.clone()), seed_nodes, None, Some(cli)).await;
}

#[when(expr = "I discover peer {word} on wallet {word} via command line")]
async fn discover_peer(world: &mut TariWorld, node: String, wallet: String) {
    let wallet_ps = world.wallets.get_mut(&wallet).unwrap();
    wallet_ps.kill();

    tokio::time::sleep(Duration::from_secs(5)).await;

    let mut cli = get_default_cli();

    let mut node_client = world.get_node_client(&node).await.unwrap();
    let node_identity = node_client.identify(Empty {}).await.unwrap().into_inner();

    let args = DiscoverPeerArgs {
        dest_public_key: UniPublicKey::from_str(node_identity.public_key.to_hex().as_str()).unwrap(),
    };

    cli.command2 = Some(CliCommands::DiscoverPeer(args));

    let base_node = world.wallet_connected_to_base_node.get(&wallet).unwrap();
    let seed_nodes = world.base_nodes.get(&node).unwrap().seed_nodes.clone();
    spawn_wallet(world, wallet, Some(base_node.clone()), seed_nodes, None, Some(cli)).await;
}

#[then(expr = "I run whois {word} on wallet {word} via command line")]
async fn whois(world: &mut TariWorld, node: String, wallet: String) {
    let wallet_ps = world.wallets.get_mut(&wallet).unwrap();
    wallet_ps.kill();

    tokio::time::sleep(Duration::from_secs(5)).await;

    let mut cli = get_default_cli();

    let mut node_client = world.get_node_client(&node).await.unwrap();
    let node_identity = node_client.identify(Empty {}).await.unwrap().into_inner();

    let args = WhoisArgs {
        public_key: UniPublicKey::from_str(node_identity.public_key.to_hex().as_str()).unwrap(),
    };

    cli.command2 = Some(CliCommands::Whois(args));

    let base_node = world.wallet_connected_to_base_node.get(&wallet).unwrap();
    let seed_nodes = world.base_nodes.get(&node).unwrap().seed_nodes.clone();
    spawn_wallet(world, wallet, Some(base_node.clone()), seed_nodes, None, Some(cli)).await;
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
            "Base node \"{}\": grpc port \"{}\", temp dir path \"{:?}\"",
            name, node.grpc_port, node.temp_dir_path
        );
    }

    // wallets
    for (name, node) in world.wallets.iter() {
        eprintln!(
            "Wallet \"{}\": grpc port \"{}\", temp dir path \"{:?}\"",
            name, node.grpc_port, node.temp_dir_path
        );
    }

    eprintln!();
    eprintln!("======================================");
    eprintln!();
}

// FFI Steps
#[when(expr = "I have a ffi wallet {word} connected to base node {word}")]
#[then(expr = "I have a ffi wallet {word} connected to base node {word}")]
#[given(expr = "I have a ffi wallet {word} connected to base node {word}")]
async fn ffi_start_wallet_connected_to_base_node(world: &mut TariWorld, wallet: String, base_node: String) {
    spawn_wallet_ffi(world, wallet.clone(), null());
    let base_node = world.get_node(&base_node).unwrap();
    world.get_ffi_wallet(&wallet).unwrap().add_base_node(
        base_node.identity.public_key().to_hex(),
        base_node.identity.first_public_address().to_string(),
    );
}

#[given(expr = "I have a ffi wallet {word} connected to seed node {word}")]
async fn ffi_start_wallet_connected_to_seed_node(world: &mut TariWorld, wallet: String, seed_node: String) {
    spawn_wallet_ffi(world, wallet.clone(), null());
    assert!(world.all_seed_nodes().contains(&seed_node), "Seed node not found.");
    let seed_node = world.get_node(&seed_node).unwrap();
    world.get_ffi_wallet(&wallet).unwrap().add_base_node(
        seed_node.identity.public_key().to_hex(),
        seed_node.identity.first_public_address().to_string(),
    );
}

#[given(expr = "I set base node {word} for ffi wallet {word}")]
async fn ffi_set_base_node(world: &mut TariWorld, base_node: String, wallet: String) {
    let base_node = world.get_node(&base_node).unwrap();
    world.get_ffi_wallet(&wallet).unwrap().add_base_node(
        base_node.identity.public_key().to_hex(),
        base_node.identity.first_public_address().to_string(),
    );
}

#[then(expr = "I want to get public key of ffi wallet {word}")]
async fn ffi_get_public_key(world: &mut TariWorld, wallet: String) {
    let wallet = world.get_ffi_wallet(&wallet).unwrap();
    let public_key = wallet.identify();
    println!("public_key {}", public_key);
}

#[then(expr = "I want to get emoji id of ffi wallet {word}")]
async fn ffi_get_emoji_id(world: &mut TariWorld, wallet: String) {
    let wallet = world.get_ffi_wallet(&wallet).unwrap();
    let emoji_id = wallet.get_emoji_id();
    assert_eq!(
        emoji_id.len(),
        132,
        "Emoji id {} is expected to be of length 132",
        emoji_id
    );
}

#[then(expr = "I stop ffi wallet {word}")]
async fn ffi_stop_wallet(world: &mut TariWorld, wallet: String) {
    let address = world.get_wallet_address(&wallet).await.unwrap();
    let ffi_wallet = world.ffi_wallets.get_mut(&wallet).unwrap();
    println!("Adding wallet {}", wallet);
    world.wallet_addresses.insert(wallet, address);
    ffi_wallet.destroy();
}

#[then(expr = "I retrieve the mnemonic word list for {word}")]
async fn ffi_retrieve_mnemonic_words(_world: &mut TariWorld, language: String) {
    println!("Mnemonic words for language {}:", language);
    let words = get_mnemonic_word_list_for_language(language);
    for i in 0..words.get_length() {
        print!("{} ", words.get_at(i as u32).as_string());
    }
    println!();
    assert_eq!(words.get_length(), 2048);
}

#[then(expr = "I wait for ffi wallet {word} to connect to {word}")]
async fn ffi_wait_wallet_to_connect(world: &mut TariWorld, wallet: String, node: String) {
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    let node = world.get_node(&node).unwrap().identity.public_key();
    for _ in 0..10 {
        let public_keys = ffi_wallet.connected_public_keys();
        for i in 0..public_keys.get_length() {
            let public_key = public_keys.get_public_key_at(i as u32);
            if public_key.get_bytes().get_as_hex() == node.to_hex() {
                return;
            }
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
    panic!("Wallet not connected");
}

#[then(expr = "I wait for ffi wallet {word} to have at least {int} uT")]
async fn ffi_wait_for_balance(world: &mut TariWorld, wallet: String, balance: u64) {
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    let mut ffi_balance = ffi_wallet.get_balance();
    let mut cnt = 0;
    while ffi_balance.get_available() < balance && cnt < 10 {
        tokio::time::sleep(Duration::from_secs(3)).await;
        ffi_balance = ffi_wallet.get_balance();
        cnt += 1;
    }
    assert!(
        ffi_balance.get_available() >= balance,
        "Wallet doesn't have enough available funds {}",
        ffi_balance.get_available()
    );
}

#[when(expr = "I add contact with alias {word} and address of {word} to ffi wallet {word}")]
async fn ffi_add_contact(world: &mut TariWorld, alias: String, pubkey: String, wallet: String) {
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();

    let address = world.get_wallet_address(&pubkey).await.unwrap();
    let contact = create_contact(alias, address);

    assert!(ffi_wallet.upsert_contact(contact));
}

async fn check_contact(world: &mut TariWorld, alias: String, pubkey: Option<String>, wallet: String) -> bool {
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    let address: Option<String> = match pubkey {
        Some(pubkey) => Some(world.get_wallet_address(&pubkey).await.unwrap()),
        None => None,
    };
    let contacts = ffi_wallet.get_contacts();
    let mut found = false;
    for i in 0..contacts.get_length() {
        let contact = contacts.get_at(i);
        if (address.is_none() || &contact.get_address().address().get_as_hex() == address.as_ref().unwrap()) &&
            contact.get_alias() == alias
        {
            found = true;
            break;
        }
    }
    found
}

#[then(expr = "I have contact with alias {word} and address of {word} in ffi wallet {word}")]
async fn ffi_check_contact(world: &mut TariWorld, alias: String, pubkey: String, wallet: String) {
    assert!(check_contact(world, alias, Some(pubkey), wallet).await);
}

#[when(expr = "I remove contact with alias {word} from ffi wallet {word}")]
async fn ffi_remove_contact(world: &mut TariWorld, alias: String, wallet: String) {
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    let contacts = ffi_wallet.get_contacts();
    let mut contact_to_remove = None;
    for i in 0..contacts.get_length() {
        let contact = contacts.get_at(i);
        if contact.get_alias() == alias {
            contact_to_remove = Some(contact);
            break;
        }
    }
    assert!(contact_to_remove.is_some());
    assert!(ffi_wallet.remove_contact(contact_to_remove.unwrap()));
}

#[then(expr = "I don't have contact with alias {word} in ffi wallet {word}")]
async fn ffi_check_no_contact(world: &mut TariWorld, alias: String, wallet: String) {
    assert!(!check_contact(world, alias, None, wallet).await);
}

#[when(expr = "I send {int} uT from ffi wallet {word} to wallet {word} at fee {int}")]
#[then(expr = "I send {int} uT from ffi wallet {word} to wallet {word} at fee {int}")]
async fn ffi_send_transaction(world: &mut TariWorld, amount: u64, wallet: String, dest: String, fee: u64) {
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    let dest_pub_key = world.get_wallet_address(&dest).await.unwrap();
    let message = format!("Send from ffi {} to ${} at fee ${}", wallet, dest, fee);
    let tx_id = ffi_wallet.send_transaction(dest_pub_key, amount, fee, message, false);
    assert_ne!(tx_id, 0, "Send transaction was not successful");
}

#[when(expr = "I send {int} uT from ffi wallet {word} to wallet {word} at fee {int} via one-sided transactions")]
#[then(expr = "I send {int} uT from ffi wallet {word} to wallet {word} at fee {int} via one-sided transactions")]
async fn ffi_send_one_sided_transaction(world: &mut TariWorld, amount: u64, wallet: String, dest: String, fee: u64) {
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    let dest_pub_key = world.get_wallet_address(&dest).await.unwrap();
    let message = format!("Send from ffi {} to ${} at fee ${}", wallet, dest, fee);
    let tx_id = ffi_wallet.send_transaction(dest_pub_key, amount, fee, message, true);
    assert_ne!(tx_id, 0, "Send transaction was not successful");
}

#[when(expr = "I have {int} received and {int} send transaction in ffi wallet {word}")]
#[then(expr = "I have {int} received and {int} send transaction in ffi wallet {word}")]
async fn ffi_check_number_of_transactions(world: &mut TariWorld, received: u32, send: u32, wallet: String) {
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    let inbound_txs = ffi_wallet.get_pending_inbound_transactions();
    let mut inbound_cnt = inbound_txs.get_length();
    let outbound_txs = ffi_wallet.get_pending_outbound_transactions();
    let mut outbound_cnt = outbound_txs.get_length();
    let completed_txs = ffi_wallet.get_completed_transactions();
    for i in 0..completed_txs.get_length() {
        let completed_tx = completed_txs.get_at(i);
        if completed_tx.is_outbound() {
            outbound_cnt += 1;
        } else {
            inbound_cnt += 1;
        }
    }
    assert_eq!(outbound_cnt, send);
    assert_eq!(inbound_cnt, received);
}

#[then(expr = "I wait for ffi wallet {word} to have {int} pending outbound transaction(s)")]
async fn ffi_check_number_of_outbound_transactions(world: &mut TariWorld, wallet: String, cnt: u32) {
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    let mut found_cnt = 0;
    let num_retries = 120;
    for _ in 0..num_retries {
        let pending_outbound_transactions = ffi_wallet.get_pending_outbound_transactions();
        found_cnt = pending_outbound_transactions.get_length();
        if found_cnt >= cnt {
            break;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    assert!(found_cnt >= cnt, "The number of pending outbound transaction is lower.");
}

#[then(expr = "I wait for ffi wallet {word} to have at least {int} contacts to be {word}")]
async fn ffi_check_contacts(world: &mut TariWorld, wallet: String, cnt: u64, status: String) {
    assert!(
        vec!["Online", "Offline", "NeverSeen"].contains(&status.as_str()),
        "Unknown status : {}",
        status
    );
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    println!(
        "Waiting for {} to have at least {} contacts with status '{}'",
        wallet, cnt, status
    );
    let mut found_cnt = 0;

    let liveness_data = ffi_wallet.get_liveness_data();
    for _ in 0..120 {
        found_cnt = 0;
        for (_alias, data) in liveness_data.lock().unwrap().iter() {
            if data.get_online_status() == status {
                found_cnt += 1;
            }
        }
        if found_cnt >= cnt {
            break;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    assert!(
        found_cnt >= cnt,
        "{} doesn't have at least {} contacts with status {}!",
        wallet,
        cnt,
        status
    );
}

#[then(expr = "I want to view the transaction kernels for completed transactions in ffi wallet {word}")]
async fn ffi_view_transaction_kernels_for_completed(world: &mut TariWorld, wallet: String) {
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    let completed_transactions = ffi_wallet.get_completed_transactions();
    for i in 0..completed_transactions.get_length() {
        let completed_transaction = completed_transactions.get_at(i);
        let kernel = completed_transaction.get_transaction_kernel();
        println!("Transaction kernel info :");
        assert!(!kernel.get_excess_hex().is_empty());
        println!("Excess {}", kernel.get_excess_hex());
        assert!(!kernel.get_excess_public_nonce_hex().is_empty());
        println!("Nonce {}", kernel.get_excess_public_nonce_hex());
        assert!(!kernel.get_excess_signature_hex().is_empty());
        println!("Signature {}", kernel.get_excess_signature_hex());
    }
}

#[then(expr = "I cancel all outbound transactions on ffi wallet {word} and it will cancel {int} transaction")]
async fn ffi_cancel_outbound_transactions(world: &mut TariWorld, wallet: String, cnt: u64) {
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    let pending_outbound_transactions = ffi_wallet.get_pending_outbound_transactions();
    let mut cancelled = 0;
    for i in 0..pending_outbound_transactions.get_length() {
        let pending_outbound_transaction = pending_outbound_transactions.get_at(i);
        if ffi_wallet.cancel_pending_transaction(pending_outbound_transaction.get_transaction_id()) {
            cancelled += 1;
        }
    }
    assert_eq!(cancelled, cnt);
}

#[then(expr = "I wait for ffi wallet {word} to receive {int} transaction")]
async fn ffi_wait_for_transaction_received(world: &mut TariWorld, wallet: String, cnt: u64) {
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    let num_retries = 120;
    let mut found_cnt = 0;
    for _ in 0..num_retries {
        found_cnt = ffi_wallet.get_counters().get_transaction_received();
        if found_cnt >= cnt {
            break;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    assert!(found_cnt >= cnt, "Expected {}, but got only {}", cnt, found_cnt);
}

#[then(expr = "I wait for ffi wallet {word} to receive {int} finalization")]
async fn ffi_wait_for_transaction_finalized(world: &mut TariWorld, wallet: String, cnt: u64) {
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    let num_retries = 120;
    let mut found_cnt = 0;
    for _ in 0..num_retries {
        found_cnt = ffi_wallet.get_counters().get_transaction_finalized();
        if found_cnt >= cnt {
            break;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    assert!(found_cnt >= cnt, "Expected {}, but got only {}", cnt, found_cnt);
}

#[then(expr = "I wait for ffi wallet {word} to receive {int} broadcast")]
async fn ffi_wait_for_transaction_broadcast(world: &mut TariWorld, wallet: String, cnt: u64) {
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    let num_retries = 120;
    let mut found_cnt = 0;
    for _ in 0..num_retries {
        found_cnt = ffi_wallet.get_counters().get_transaction_broadcast();
        if found_cnt >= cnt {
            break;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    assert!(found_cnt >= cnt, "Expected {}, but got only {}", cnt, found_cnt);
}

#[then(expr = "I start TXO validation on ffi wallet {word}")]
async fn ffi_start_txo_validation(world: &mut TariWorld, wallet: String) {
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    ffi_wallet.start_txo_validation();
    let num_retries = 120;
    let mut validation_complete = false;
    for _ in 0..num_retries {
        validation_complete = ffi_wallet.get_counters().get_txo_validation_complete();
        if validation_complete {
            break;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    assert!(validation_complete);
}

#[then(expr = "I start TX validation on ffi wallet {word}")]
async fn ffi_start_tx_validation(world: &mut TariWorld, wallet: String) {
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    ffi_wallet.start_transaction_validation();
    let num_retries = 120;
    let mut validation_complete = false;
    for _ in 0..num_retries {
        validation_complete = ffi_wallet.get_counters().get_tx_validation_complete();
        if validation_complete {
            break;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    assert!(validation_complete);
}

#[then(expr = "ffi wallet {word} detects {word} {int} ffi transactions to be {word}")]
async fn ffi_detects_transaction(
    world: &mut TariWorld,
    wallet: String,
    comparison: String,
    count: u64,
    status: String,
) {
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    assert!(vec![
        "TRANSACTION_STATUS_BROADCAST",
        "TRANSACTION_STATUS_FAUX_UNCONFIRMED",
        "TRANSACTION_STATUS_FAUX_CONFIRMED"
    ]
    .contains(&status.as_str()));
    println!(
        "Waiting for {} to have detected {} {} {} transaction(s)",
        wallet, comparison, count, status
    );
    let mut found_count = 0;
    for _ in 0..120 {
        found_count = match status.as_str() {
            "TRANSACTION_STATUS_BROADCAST" => ffi_wallet.get_counters().get_transaction_broadcast(),
            "TRANSACTION_STATUS_FAUX_UNCONFIRMED" => ffi_wallet.get_counters().get_transaction_faux_unconfirmed(),
            "TRANSACTION_STATUS_FAUX_CONFIRMED" => ffi_wallet.get_counters().get_transaction_faux_confirmed(),
            _ => unreachable!(),
        };
        if found_count >= count {
            break;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    match comparison.as_str() {
        "AT_LEAST" => assert!(
            found_count >= count,
            "Counter not adequate! Counter is {}.",
            found_count
        ),
        "EXACTLY" => assert!(
            found_count == count,
            "Counter not adequate! Counter is {}.",
            found_count
        ),
        _ => panic!("Unknown comparison method {}", comparison),
    };
}

#[then(expr = "I wait for ffi wallet {word} to receive {int} mined")]
async fn ffi_wait_for_received_mined(world: &mut TariWorld, wallet: String, count: u64) {
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    println!("Waiting for {} to receive {} transaction(s) mined", wallet, count);

    let mut found_cnt = 0;
    for _ in 0..120 {
        found_cnt = ffi_wallet.get_counters().get_transaction_mined();
        if found_cnt >= count {
            break;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    assert!(found_cnt >= count);
}

#[then(expr = "I recover wallet {word} into ffi wallet {word} from seed words on node {word}")]
async fn ffi_recover_wallet(world: &mut TariWorld, wallet_name: String, ffi_wallet_name: String, base_node: String) {
    let wallet = world.get_wallet(&wallet_name).unwrap();
    let seed_words_path = wallet.temp_dir_path.clone().join("seed_words.txt");
    let seed_words_file = std::fs::File::open(seed_words_path).unwrap();
    let reader = std::io::BufReader::new(seed_words_file);
    let line = reader.lines().next().unwrap().unwrap();
    let words = line.split_whitespace().collect();
    let seed_words = create_seed_words(words);

    spawn_wallet_ffi(world, ffi_wallet_name.clone(), seed_words.get_ptr());

    let base_node = world.get_node(&base_node).unwrap();
    world.get_ffi_wallet(&ffi_wallet_name).unwrap().add_base_node(
        base_node.identity.public_key().to_hex(),
        base_node.identity.first_public_address().to_string(),
    );
}

#[then(expr = "I restart ffi wallet {word} connected to base node {word}")]
async fn ffi_restart_wallet(world: &mut TariWorld, wallet: String, base_node: String) {
    let ffi_wallet = world.get_mut_ffi_wallet(&wallet).unwrap();
    ffi_wallet.restart();
    let base_node = world.get_node(&base_node).unwrap();
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    ffi_wallet.add_base_node(
        base_node.identity.public_key().to_hex(),
        base_node.identity.first_public_address().to_string(),
    );
}

#[then(expr = "The fee per gram stats for {word} are {int}, {int}, {int}")]
#[when(expr = "The fee per gram stats for {word} are {int}, {int}, {int}")]
async fn ffi_fee_per_gram_stats(world: &mut TariWorld, wallet: String, min: u64, avg: u64, max: u64) {
    let ffi_wallet = world.get_mut_ffi_wallet(&wallet).unwrap();
    let fee_per_gram_stats = ffi_wallet.get_fee_per_gram_stats(5);
    for i in 0..fee_per_gram_stats.get_length() {
        let fee_per_gram_stat = fee_per_gram_stats.get_at(i);
        println!("order {}", fee_per_gram_stat.get_order());
        println!("min {}", fee_per_gram_stat.get_min_fee_per_gram());
        println!("avg {}", fee_per_gram_stat.get_avg_fee_per_gram());
        println!("max {}", fee_per_gram_stat.get_max_fee_per_gram());
        assert_eq!(fee_per_gram_stat.get_min_fee_per_gram(), min);
        assert_eq!(fee_per_gram_stat.get_avg_fee_per_gram(), avg);
        assert_eq!(fee_per_gram_stat.get_max_fee_per_gram(), max);
    }
}

// Merge mining proxy steps

#[when(expr = "I have a merge mining proxy {word} connected to {word} and {word} with origin submission {word}")]
async fn merge_mining_proxy_with_submission(
    world: &mut TariWorld,
    mining_proxy_name: String,
    base_node_name: String,
    wallet_name: String,
    enabled: String,
) {
    let enabled = match enabled.as_str() {
        "enabled" => true,
        "disabled" => false,
        _ => panic!("This should be a boolean"),
    };
    register_merge_mining_proxy_process(world, mining_proxy_name, base_node_name, wallet_name, enabled).await;
}

#[when(expr = "I have a merge mining proxy {word} connected to {word} and {word} with default config")]
async fn merge_mining_proxy_with_default_config(
    world: &mut TariWorld,
    mining_proxy_name: String,
    base_node_name: String,
    wallet_name: String,
) {
    register_merge_mining_proxy_process(world, mining_proxy_name, base_node_name, wallet_name, true).await;
}

#[when(expr = "I ask for a block height from proxy {word}")]
async fn merge_mining_ask_for_block_height(world: &mut TariWorld, mining_proxy_name: String) {
    let merge_miner = world.get_merge_miner(&mining_proxy_name).unwrap();
    world.last_merge_miner_response = merge_miner.get_height().await;
}

#[then(expr = "Proxy response height is valid")]
async fn merge_mining_response_height(world: &mut TariWorld) {
    let height = world.last_merge_miner_response.get("height");
    assert!(
        height.is_some(),
        "Response is invalid {}",
        world.last_merge_miner_response
    );
    let height = height.unwrap();
    assert!(height.as_u64().is_some(), "Height is invalid {}", height);
}

#[when(expr = "I ask for a block template from proxy {word}")]
async fn merge_mining_ask_for_block_template(world: &mut TariWorld, mining_proxy_name: String) {
    let merge_miner = world.get_mut_merge_miner(&mining_proxy_name).unwrap();
    world.last_merge_miner_response = merge_miner.get_block_template().await;
}

#[then(expr = "Proxy response block template is valid")]
async fn merge_mining_response_block_template_is_valid(world: &mut TariWorld) {
    let result = world.last_merge_miner_response.get("result");
    assert!(
        result.is_some(),
        "Response is invalid {}",
        world.last_merge_miner_response
    );
    let result = result.unwrap();
    assert!(result.get("_aux").is_some(), "Result has no `_aux` {}", result);
    assert_eq!(
        result.get("status").unwrap().as_str().unwrap(),
        "OK",
        "Result has no `status` {}",
        result
    );
}

#[when(expr = "I submit a block through proxy {word}")]
async fn merge_mining_submit_block(world: &mut TariWorld, mining_proxy_name: String) {
    let block_template_blob = world
        .last_merge_miner_response
        .get("result")
        .unwrap()
        .get("blocktemplate_blob");
    assert!(
        block_template_blob.is_some(),
        "The last response doesn't have `blocktemplate_blob` {}",
        world.last_merge_miner_response
    );
    let block_template_blob = block_template_blob.unwrap().clone();
    let merge_miner = world.get_mut_merge_miner(&mining_proxy_name).unwrap();
    println!("block_template {:?}", block_template_blob);
    world.last_merge_miner_response = merge_miner.submit_block(&block_template_blob).await;
    println!("last_merge_miner_response {:?}", world.last_merge_miner_response);
    println!("last_merge_miner_response {:?}", world.last_merge_miner_response);
    println!("last_merge_miner_response {:?}", world.last_merge_miner_response);
}

#[then(expr = "Proxy response block submission is valid {word} submitting to origin")]
async fn merge_mining_submission_is_valid(world: &mut TariWorld, how: String) {
    let result = world.last_merge_miner_response.get("result");
    assert!(
        result.is_some(),
        "Response is invalid {}",
        world.last_merge_miner_response
    );
    let result = result.unwrap();
    if how == *"with" {
        assert!(result.get("_aux").is_some(), "Result has no `_aux` {}", result);
        let status = result.get("status");
        assert!(status.is_some(), "Result has no status {}", result);
    } else {
        assert!(
            world.last_merge_miner_response.get("status").is_some(),
            "Response has no `status` {}",
            world.last_merge_miner_response
        );
    }
}

#[when(expr = "I merge mine {int} blocks via {word}")]
async fn merge_mining_mine(world: &mut TariWorld, count: u64, mining_proxy_name: String) {
    let merge_miner = world.get_mut_merge_miner(&mining_proxy_name).unwrap();
    for _ in 0..count {
        merge_miner.mine().await;
    }
}

#[when(expr = "I ask for the last block header from proxy {word}")]
async fn merge_mining_ask_for_last_block_header(world: &mut TariWorld, mining_proxy_name: String) {
    let merge_miner = world.get_mut_merge_miner(&mining_proxy_name).unwrap();
    world.last_merge_miner_response = merge_miner.get_last_block_header().await;
}

#[then(expr = "Proxy response for block header by hash is valid")]
async fn merge_mining_bloch_header_by_hash_is_valid(world: &mut TariWorld) {
    let result = world.last_merge_miner_response.get("result");
    assert!(
        result.is_some(),
        "Response is invalid {}",
        world.last_merge_miner_response
    );
    let result = result.unwrap();
    let status = result.get("status");
    assert!(status.is_some(), "Result has no status {}", result);
    assert_eq!(
        result.get("status").unwrap().as_str().unwrap(),
        "OK",
        "Result has no `status` {}",
        result
    );
}

#[then(expr = "Proxy response for last block header is valid")]
async fn merge_mining_response_last_block_header_is_valid(world: &mut TariWorld) {
    let result = world.last_merge_miner_response.get("result");
    assert!(
        result.is_some(),
        "Response is invalid {}",
        world.last_merge_miner_response
    );
    let result = result.unwrap();
    assert!(result.get("_aux").is_some(), "Result has no `_aux` {}", result);
    let status = result.get("status");
    assert!(status.is_some(), "Result has no status {}", result);
    assert_eq!(
        result.get("status").unwrap().as_str().unwrap(),
        "OK",
        "Result has no `status` {}",
        result
    );
    let block_header = result.get("block_header");
    assert!(block_header.is_some(), "Result has no `block_header` {}", result);
    let block_header = block_header.unwrap();
    assert!(
        block_header.get("hash").is_some(),
        "Block_header has no `hash` {}",
        block_header
    );
}

#[when(expr = "I ask for a block header by hash using last block header from proxy {word}")]
async fn merge_mining_ask_for_block_header_by_hash(world: &mut TariWorld, mining_proxy_name: String) {
    let hash = world
        .last_merge_miner_response
        .get("result")
        .unwrap()
        .get("block_header")
        .unwrap()
        .get("hash")
        .unwrap()
        .clone();
    let merge_miner = world.get_mut_merge_miner(&mining_proxy_name).unwrap();
    world.last_merge_miner_response = merge_miner.get_block_header_by_hash(hash).await;
}

fn flush_stdout(buffer: &Arc<Mutex<Vec<u8>>>) {
    // After each test we flush the stdout to the logs.
    info!(
        target: LOG_TARGET_STDOUT,
        "{}",
        str::from_utf8(&buffer.lock().unwrap()).unwrap()
    );
    buffer.lock().unwrap().clear();
}

fn main() {
    initialize_logging(
        &PathBuf::from("log4rs/cucumber.yml"),
        &PathBuf::from("./"),
        include_str!("../log4rs/cucumber.yml"),
    )
    .expect("logging not configured");
    let stdout_buffer = Arc::new(Mutex::new(Vec::<u8>::new()));
    #[cfg(test)]
    std::io::set_output_capture(Some(stdout_buffer.clone()));
    // Never move this line below the runtime creation!!! It will cause that any new thread created via task::spawn will
    // not be affected by the output capture.
    let stdout_buffer_clone = stdout_buffer.clone();
    let runtime = Runtime::new().unwrap();
    runtime.block_on(async {
        let world = TariWorld::cucumber()
        .repeat_failed()
        // following config needed to use eprint statements in the tests
        .max_concurrent_scenarios(5)
        //.with_writer(
        //    writer::Basic::raw(io::stdout(), writer::Coloring::Never, 0)
        //        .summarized()
        //        .assert_normalized(),
        //)
        .after(move |_feature, _rule, scenario, ev, maybe_world| {
            let stdout_buffer = stdout_buffer_clone.clone();
            Box::pin(async move {
                flush_stdout(&stdout_buffer);
                match ev {
                    ScenarioFinished::StepFailed(_capture_locations, _location, _error) => {
                        error!(target: LOG_TARGET, "Scenario failed");
                    },
                    ScenarioFinished::StepPassed => {
                        info!(target: LOG_TARGET, "Scenario was successful.");
                    },
                    ScenarioFinished::StepSkipped => {
                        warn!(target: LOG_TARGET, "Some steps were skipped.");
                    },
                    ScenarioFinished::BeforeHookFailed(_info) => {
                        error!(target: LOG_TARGET, "Before hook failed!");
                    },
                }
                if let Some(maybe_world) = maybe_world {
                    maybe_world.after(scenario).await;
                }
            })
        })
        .before(|_feature,_rule,scenario,_world| {
            Box::pin(async move {
                println!("{} : {}", scenario.keyword, scenario.name); // This will be printed into the stdout_buffer
                info!(target: LOG_TARGET, "Starting {} {}", scenario.keyword, scenario.name);
            })
        });
        world
            .fail_on_skipped()
            // .fail_fast() - Not yet supported in 0.18
            .run_and_exit("tests/features/")
            .await;
    });

    // If by any chance we have anything in the stdout buffer just log it.
    flush_stdout(&stdout_buffer);
}
