// Copyright 2019. The Tari Project
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
use clap::{value_t, App, Arg};
use futures::{future, future::FutureExt, pin_mut, select, StreamExt};
use log::*;
use serde::Deserialize;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use derive_error::Error;
use tari_common::{load_configuration, GlobalConfig};
use tari_core::{
    blocks::{Block, BlockBuilder, BlockHeader},
    consensus::ConsensusRules,
};
use tari_testnet_miner::{basenode::*, cli::*, miner::Miner};
use tari_utilities::hex::Hex;
use tokio::{io::AsyncBufReadExt, net::signal, runtime::Runtime};
use tokio_executor::threadpool::ThreadPool;

#[derive(Debug, Error)]
enum ConfigError {
    /// The config was broken
    BrokenConfig,
    // directory error
    DirectoryError,
    // problem running basenode
    BaseNodeError,
}

const LOG_TARGET: &str = "applications::testnet_miner";

// Removing GRPC for now as the basenode coms are not going to be through GRPC for this miner,
// wallet might still be so leaving the code as an example
// pub mod testnet_miner {
//     tonic::include_proto!("testnet_miner_rpc");
// }
// use testnet_miner::{client::TestNetMinerClient, BlockHeaderMessage, BlockHeight, BlockMessage, VoidParams};
// let mut base_node = TestNetMinerClient::connect(settings.base_node_address.unwrap())?;
//     let request = tonic::Request::new(VoidParams {});

//     let response = base_node.get_block(request).await?;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if let Err(e) = tari_common::dir_utils::create_data_directory() {
        println!(
            "We couldn't create a default Tari data directory and have to quit now. This makes us sad :(\n {}",
            e.to_string()
        );
        return Result::Err(Box::new(ConfigError::DirectoryError) as Box<dyn std::error::Error>);
    }

    let (global_config, node_id) = read_settings()?;

    let mut miner = Miner::new(ConsensusRules::current());
    let mut block = get_block();
    let mining_flag = miner.get_mine_flag();
    let mut stop_flag = Arc::new(AtomicBool::new(false));

    let mut pool = ThreadPool::new();
    // Set up the Tokio runtime for basenode
    let rt_base_node = match setup_runtime(&global_config) {
        Ok(rt_base_node) => rt_base_node,
        Err(s) => {
            error!(target: LOG_TARGET, "{}", s);
            return Result::Err(Box::new(ConfigError::BaseNodeError) as Box<dyn std::error::Error>);
        },
    };

    // Build, node, build!
    let (comms, node) = match configure_and_initialize_node(&global_config, node_id, &rt_base_node) {
        Ok(n) => n,
        Err(e) => {
            error!(target: LOG_TARGET, "Could not instantiate node instance. {}", e);
            return Result::Err(Box::new(ConfigError::BaseNodeError) as Box<dyn std::error::Error>);
        },
    };

    // Configure the shutdown daemon to listen for CTRL-C
    let flag = node.get_flag();
    if let Err(e) = handle_ctrl_c(&rt_base_node, flag, mining_flag, stop_flag.clone()) {
        error!(target: LOG_TARGET, "Could not configure Ctrl-C handling. {}", e);
        return Result::Err(Box::new(ConfigError::BaseNodeError) as Box<dyn std::error::Error>);
    };

    // Run, node, run!
    let main = async move {
        node.run().await;
        debug!(
            target: LOG_TARGET,
            "The node has finished all it's work. initiating Comms stack shutdown"
        );
        match comms.shutdown() {
            Ok(()) => info!(target: LOG_TARGET, "The comms stack reported a clean shutdown"),
            Err(e) => warn!(
                target: LOG_TARGET,
                "The comms stack did not shut down cleanly: {}",
                e.to_string()
            ),
        }
    };
    rt_base_node.spawn(main);

    loop {
        let height: u64 = if block.header.height <= 2016 {
            1
        } else {
            block.header.height - 2016
        };

        miner.add_block(block);
        let header = get_blockheader();

        // create threads

        let t_miner = mine(&mut miner, header, &mut pool).fuse();
        let t_tip = check_tip(0).fuse();
        pin_mut!(t_miner);
        pin_mut!(t_tip);

        select! {
        () = t_miner => {info!(target: LOG_TARGET, "Mined a block");
                        send_block();
                        },
        () = t_tip => info!(target: LOG_TARGET, "Canceled mining on current block, tip changed"),}
        if stop_flag.load(Ordering::Relaxed) {
            break;
        }
        block = get_block();
    }
    pool.shutdown().await;
    rt_base_node.shutdown_on_idle();
    Ok(())
}

async fn mine(miner: &mut Miner, header: BlockHeader, pool: &mut ThreadPool) {
    miner.mine(header, pool).await;
}

async fn stop_mining() {
    loop {
        println!("Mining, press c to close and stop");
        let mut stdin_reader = tokio::io::BufReader::new(tokio::io::stdin());
        let mut buf = Vec::new();
        stdin_reader.read_until(b'\n', &mut buf).await;
        if buf == b"c\n" {
            break;
        }
    }
}

// This checks for a tip increase meaning we need to change to a new block for mining as we are not mining on the
// largest chain anymore. Todo this should check diff and not height.
async fn check_tip(current_tip: u64) {
    loop {
        tokio::timer::delay(Instant::now() + Duration::from_millis(1000)).await;
        if current_tip < get_chain_tip_height() {
            break;
        }
    }
}

/// Function to read in the settings, either from the config file or the cli
fn read_settings(
) -> Result<(GlobalConfig, tari_comms::peer_manager::node_identity::NodeIdentity), Box<dyn std::error::Error>> {
    print_banner();
    let arguments = parse_cli_args();
    // Load and apply configuration file
    let cfg = match load_configuration(&arguments.bootstrap) {
        Ok(cfg) => cfg,
        Err(s) => {
            error!(target: LOG_TARGET, "{}", s);
            return Result::Err(Box::new(ConfigError::BrokenConfig));
        },
    };

    // Populate the configuration struct
    let node_config = match GlobalConfig::convert_from(cfg) {
        Ok(c) => c,
        Err(e) => {
            error!(target: LOG_TARGET, "The configuration file has an error. {}", e);
            return Result::Err(Box::new(ConfigError::BrokenConfig));
        },
    };

    // Load or create the Node identity
    let node_id = match load_identity(&node_config.identity_file, &node_config.address) {
        Ok(id) => id,
        Err(e) => {
            if !arguments.create_id {
                error!(
                    target: LOG_TARGET,
                    "Node identity information not found. {}. You can update the configuration file to point to a \
                     valid node identity file, or re-run the node with the --create_id flag to create anew identity.",
                    e
                );
                return Result::Err(Box::new(ConfigError::BrokenConfig));
            }
            debug!(target: LOG_TARGET, "Node id not found. {}. Creating new ID", e);
            match create_and_save_id(&node_config.identity_file, &node_config.address) {
                Ok(id) => {
                    info!(
                        target: LOG_TARGET,
                        "New node identity [{}] with public key {} has been created.",
                        id.identity.node_id.to_hex(),
                        id.public_key().to_hex()
                    );
                    id
                },
                Err(e) => {
                    error!(target: LOG_TARGET, "Could not create new node id. {}.", e);
                    return Result::Err(Box::new(ConfigError::BrokenConfig));
                },
            }
        },
    };
    Ok((node_config, node_id))
}

// todo get block here
fn get_block() -> Block {
    BlockBuilder::new()
        .with_header(BlockHeader::new(ConsensusRules::current().blockchain_version()))
        .build()
}

// todo get blockheader here
fn get_blockheader() -> BlockHeader {
    BlockHeader::new(ConsensusRules::current().blockchain_version())
}

// todo get tip height here
fn get_chain_tip_height() -> u64 {
    0
}

// todo propagate block
fn send_block() {
    println!("sending mined block out");
}

fn setup_runtime(config: &GlobalConfig) -> Result<Runtime, String> {
    let num_core_threads = config.core_threads;
    let num_blocking_threads = config.blocking_threads;

    debug!(
        target: LOG_TARGET,
        "Configuring the node to run on {} core threads and {} blocking worker threads.",
        num_core_threads,
        num_blocking_threads
    );
    tokio::runtime::Builder::new()
        .blocking_threads(num_blocking_threads)
        .core_threads(num_core_threads)
        .build()
        .map_err(|e| format!("There was an error while building the node runtime. {}", e.to_string()))
}

/// Set the interrupt flag on the node when Ctrl-C is entered
fn handle_ctrl_c(
    rt: &Runtime,
    flag: Arc<AtomicBool>,
    mining_flag: Arc<AtomicBool>,
    stop_flag: Arc<AtomicBool>,
) -> Result<(), String>
{
    let ctrl_c = signal::ctrl_c().map_err(|e| e.to_string())?;
    let s = ctrl_c.take(1).for_each(move |_| {
        info!(
            target: LOG_TARGET,
            "Termination signal received from user. Shutting miner down."
        );
        // We need to shutdown the basenode, the mining workers and the mining main thread loop.
        flag.store(true, Ordering::SeqCst);
        mining_flag.store(true, Ordering::SeqCst);
        stop_flag.store(true, Ordering::SeqCst);
        future::ready(())
    });
    rt.spawn(s);
    Ok(())
}
