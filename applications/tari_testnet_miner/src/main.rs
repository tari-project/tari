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
use futures::{future::FutureExt, pin_mut, select};
use log::*;
use serde::Deserialize;
use std::{
    sync::atomic::Ordering,
    time::{Duration, Instant},
};
use tari_core::{
    blocks::{Block, BlockBuilder, BlockHeader},
    consensus::{ConsensusConstants, ConsensusManager},
};
use tari_testnet_miner::miner::Miner;
use tokio::io::AsyncBufReadExt;
use tokio_executor::threadpool::ThreadPool;

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

#[derive(Debug, Default, Deserialize)]
struct Settings {
    wallet_address: Option<String>,
    base_node_address: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = env_logger::try_init();

    info!(target: LOG_TARGET, "Settings loaded");

    info!(target: LOG_TARGET, "Requesting new block");

    let mut miner = Miner::new(ConsensusManager::default());
    let mut block = get_block();
    let mining_flag = miner.get_mine_flag();

    let mut pool = ThreadPool::new();
    loop {
        let _height: u64 = if block.header.height <= 2016 {
            1
        } else {
            block.header.height - 2016
        };

        miner.add_block(block);
        let header = get_blockheader();

        // create threads

        let t_miner = mine(&mut miner, header, &mut pool).fuse();
        let t_cli = stop_mining().fuse();
        let t_tip = check_tip(0).fuse();
        pin_mut!(t_miner);
        pin_mut!(t_cli);
        pin_mut!(t_tip);

        let mut stop_flag = false;

        select! {
        () = t_miner => {info!(target: LOG_TARGET, "Mined a block");
                        send_block();
                        },
        () = t_cli => {info!(target: LOG_TARGET, "Canceled mining");
                        mining_flag.store(true, Ordering::Relaxed);
                        stop_flag = true;
                        },
        () = t_tip => info!(target: LOG_TARGET, "Canceled mining on current block, tip changed"),}
        if stop_flag {
            break;
        }
        block = get_block();
    }
    pool.shutdown().await;
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
fn read_settings() -> Settings {
    let mut settings = Settings::default();
    let matches = App::new("Tari test-net miner")
        .version("0.1")
        .arg(
            Arg::with_name("config")
                .value_name("FILE")
                .long("config")
                .short("c")
                .help("The relative path of the miner config.toml file")
                .takes_value(true)
                .required(false),
        )
        .arg(
            Arg::with_name("wallet_address")
                .long("wallet_address")
                .short("w")
                .help("The address the wallet should use to connect to")
                .takes_value(true)
                .required(false),
        )
        .arg(
            Arg::with_name("base_node_address")
                .long("base_node_address")
                .short("b")
                .help("This is the address the server should use to connect to the base_node for blocks")
                .takes_value(true)
                .required(false),
        )
        .get_matches();
    if matches.is_present("config") {
        let mut settings_file = config::Config::default();
        settings_file
            .merge(config::File::with_name(matches.value_of("config").unwrap()))
            .expect("Could not open specified config file");
        settings = settings_file.try_into().unwrap();
    }
    if let Some(_c) = matches.values_of("wallet_address") {
        if let Ok(v) = value_t!(matches, "wallet_address", String) {
            settings.wallet_address = Some(v)
        }
    }
    if let Some(_c) = matches.values_of("base_node_address") {
        if let Ok(v) = value_t!(matches, "base_node_address", String) {
            settings.base_node_address = Some(v);
        }
    }
    settings
}

// todo get block here
fn get_block() -> Block {
    BlockBuilder::new()
        .with_header(BlockHeader::new(ConsensusConstants::current().blockchain_version()))
        .build()
}

// todo get blockheader here
fn get_blockheader() -> BlockHeader {
    BlockHeader::new(ConsensusConstants::current().blockchain_version())
}

// todo get tip height here
fn get_chain_tip_height() -> u64 {
    0
}

// todo propagate block
fn send_block() {
    println!("sending mined block out");
}
