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

use std::{
    borrow::Borrow,
    convert::{Infallible, TryFrom},
    io,
    ops::DerefMut,
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use cucumber::{given, then, when, writer, World as _, WriterExt as _};
use indexmap::IndexMap;
use tari_common_types::types::PublicKey;
use tari_comms::peer_manager::{PeerFeatures, PeerFlags};
use tari_crypto::tari_utilities::hex::Hex;
use tari_validator_node::error::GrpcBaseNodeError;
use thiserror::Error;
use tokio::sync::RwLock;
use utils::{
    miner::{mine_blocks, register_miner_process},
    wallet_process::spawn_wallet,
};

use crate::utils::{
    base_node_process::{spawn_base_node, BaseNodeProcess},
    miner::MinerProcess,
    wallet_process::WalletProcess,
};

#[derive(Error, Debug)]
pub enum TariWorldError {
    #[error("Base node process not found: {0}")]
    BaseNodeProcessNotFound(String),
    #[error("Base node error: {0}")]
    GrpcBaseNodeError(#[from] GrpcBaseNodeError),
}

#[derive(Debug, Default, cucumber::World)]
pub struct TariWorld {
    base_nodes: IndexMap<String, BaseNodeProcess>,
    wallets: IndexMap<String, WalletProcess>,
    miners: IndexMap<String, MinerProcess>,
}

impl TariWorld {
    async fn add_peer(world: &mut TariWorld, name: String, peer_name: String, is_seed_peer: bool) {
        let mut peer = world
            .base_nodes
            .get(peer_name.as_str())
            .expect("peer node")
            .cx
            .lock()
            .await
            .as_ref()
            .expect("peer node context")
            .base_node_identity()
            .to_peer();

        if is_seed_peer {
            peer.add_flags(PeerFlags::SEED);
        }

        world
            .base_nodes
            .get_mut(name.as_str())
            .expect("node")
            .cx
            .lock()
            .await
            .as_ref()
            .expect("node context lock")
            .base_node_comms()
            .peer_manager()
            .add_peer(peer)
            .await
            .expect("added peer");
    }
}

#[given(expr = "I have a seed node {word}")]
async fn start_base_node(world: &mut TariWorld, name: String) {
    spawn_base_node(world, true, name.clone()).await;
}

#[given(expr = "a wallet {word} connected to base node {word}")]
async fn start_wallet(world: &mut TariWorld, wallet_name: String, node_name: String) {
    spawn_wallet(world, wallet_name, node_name.clone()).await;
}

#[when(expr = "I have a base node {word} connected to all seed nodes")]
async fn connect_to_all_seed_nodes(world: &mut TariWorld, name: String) {
    spawn_base_node(world, false, name.clone()).await;
}

#[given(expr = "a miner {word} connected to base node {word} and wallet {word}")]
async fn create_miner(world: &mut TariWorld, miner_name: String, bn_name: String, wallet_name: String) {
    register_miner_process(world, miner_name, bn_name, wallet_name);
}

#[when(expr = "miner {word} mines {int} new blocks")]
async fn run_miner(world: &mut TariWorld, miner_name: String, num_blocks: u64) {
    mine_blocks(world, miner_name, num_blocks).await;
}

#[when(expr = "I wait {int} seconds")]
async fn wait_seconds(_world: &mut TariWorld, seconds: u64) {
    tokio::time::sleep(Duration::from_secs(seconds)).await;
}

#[when(expr = "I wait for {word} to connect to {word}")]
async fn node_pending_connection_to(world: &mut TariWorld, pending_node: String, pending_for: String) {
    //
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
    TariWorld::cucumber()
        // following config needed to use eprint statements in the tests
        .max_concurrent_scenarios(1)
        .with_writer(
            writer::Basic::raw(io::stdout(), writer::Coloring::Never, 0)
                .summarized()
                .assert_normalized(),
        )
        .run_and_exit("tests/features/")
        .await;
}
