// Copyright 2021. The Tari Project
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
//
use config::MinerConfig;
use futures::stream::StreamExt;
use log::*;

mod config;
mod difficulty;
mod errors;
mod miner;
mod stratum;
mod utils;

use crate::{
    miner::MiningReport,
    stratum::{stratum_controller::controller::Controller, stratum_miner::miner::StratumMiner},
};
use errors::{err_empty, MinerError};
use miner::Miner;
use std::{
    convert::TryFrom,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Instant,
};
use tari_app_grpc::tari_rpc::{base_node_client::BaseNodeClient, wallet_client::WalletClient};
use tari_app_utilities::{
    initialization::init_configuration,
    utilities::{ExitCodes, ExitCodes::ConfigError},
};
use tari_common::{configuration::bootstrap::ApplicationType, ConfigBootstrap, DefaultConfigLoader};
use tari_core::blocks::BlockHeader;
use tari_crypto::{ristretto::RistrettoPublicKey, tari_utilities::hex::Hex};
use tokio::{runtime::Runtime, time::sleep};
use tonic::transport::Channel;
use utils::{coinbase_request, extract_outputs_and_kernels};

/// Application entry point
fn main() {
    let rt = Runtime::new().expect("Failed to start tokio runtime");
    match rt.block_on(main_inner()) {
        Ok(_) => std::process::exit(0),
        Err(exit_code) => {
            eprintln!("Fatal error: {:?}", exit_code);
            error!("Exiting with code: {:?}", exit_code);
            std::process::exit(exit_code.as_i32())
        },
    }
}

async fn main_inner() -> Result<(), ExitCodes> {
    let (bootstrap, global, cfg) = init_configuration(ApplicationType::MiningNode)?;
    let mut config = <MinerConfig as DefaultConfigLoader>::load_from(&cfg).expect("Failed to load config");
    config.mine_on_tip_only = global.mine_on_tip_only;
    config.num_mining_threads = global.num_mining_threads;
    config.validate_tip_timeout_sec = global.validate_tip_timeout_sec;
    config.mining_worker_name = global.mining_worker_name.clone();
    config.mining_wallet_address = global.mining_wallet_address.clone();
    config.mining_pool_address = global.mining_pool_address.clone();
    debug!("{:?}", bootstrap);
    debug!("{:?}", config);

    if !config.mining_wallet_address.is_empty() && !config.mining_pool_address.is_empty() {
        let url = config.mining_pool_address.clone();
        let mut miner_address = config.mining_wallet_address.clone();
        let _ = RistrettoPublicKey::from_hex(&miner_address)
            .map_err(|_| ConfigError("Miner is not configured with a valid wallet address.".to_string()))?;
        if !config.mining_worker_name.is_empty() {
            miner_address += &format!("{}{}", ".", &config.mining_worker_name);
        }
        let mut mc = Controller::new().unwrap_or_else(|e| {
            panic!("Error loading mining controller: {}", e);
        });
        let cc = stratum::controller::Controller::new(&url, Some(miner_address), None, None, mc.tx.clone())
            .unwrap_or_else(|e| {
                panic!("Error loading stratum client controller: {:?}", e);
            });
        let miner_stopped = Arc::new(AtomicBool::new(false));
        let client_stopped = Arc::new(AtomicBool::new(false));

        mc.set_client_tx(cc.tx.clone());
        let mut miner = StratumMiner::new(config);
        if let Err(e) = miner.start_solvers() {
            println!("Error. Please check logs for further info.");
            println!("Error details:");
            println!("{:?}", e);
            println!("Exiting");
        }

        let miner_stopped_internal = miner_stopped.clone();
        let _ = thread::Builder::new()
            .name("mining_controller".to_string())
            .spawn(move || {
                if let Err(e) = mc.run(miner) {
                    error!("Error. Please check logs for further info: {:?}", e);
                    return;
                }
                miner_stopped_internal.store(true, Ordering::Relaxed);
            });

        let client_stopped_internal = client_stopped.clone();
        let _ = thread::Builder::new()
            .name("client_controller".to_string())
            .spawn(move || {
                cc.run();
                client_stopped_internal.store(true, Ordering::Relaxed);
            });

        loop {
            if miner_stopped.load(Ordering::Relaxed) && client_stopped.load(Ordering::Relaxed) {
                thread::sleep(std::time::Duration::from_millis(100));
                break;
            }
            thread::sleep(std::time::Duration::from_millis(100));
        }
        Ok(())
    } else {
        config.mine_on_tip_only = global.mine_on_tip_only;
        debug!("mine_on_tip_only is {}", config.mine_on_tip_only);

        let (mut node_conn, mut wallet_conn) = connect(&config).await.map_err(ExitCodes::grpc)?;

        let mut blocks_found: u64 = 0;
        loop {
            debug!("Starting new mining cycle");
            match mining_cycle(&mut node_conn, &mut wallet_conn, &config, &bootstrap).await {
                err @ Err(MinerError::GrpcConnection(_)) | err @ Err(MinerError::GrpcStatus(_)) => {
                    // Any GRPC error we will try to reconnect with a standard delay
                    error!("Connection error: {:?}", err);
                    loop {
                        debug!("Holding for {:?}", config.wait_timeout());
                        sleep(config.wait_timeout()).await;
                        match connect(&config).await {
                            Ok((nc, wc)) => {
                                node_conn = nc;
                                wallet_conn = wc;
                                break;
                            },
                            Err(err) => {
                                error!("Connection error: {:?}", err);
                                continue;
                            },
                        }
                    }
                },
                Err(MinerError::MineUntilHeightReached(h)) => {
                    info!("Prescribed blockchain height {} reached. Aborting ...", h);
                    return Ok(());
                },
                Err(MinerError::MinerLostBlock(h)) => {
                    info!("Height {} already mined by other node. Restarting ...", h);
                },
                Err(err) => {
                    error!("Error: {:?}", err);
                    debug!("Holding for {:?}", config.wait_timeout());
                    sleep(config.wait_timeout()).await;
                },
                Ok(submitted) => {
                    if submitted {
                        blocks_found += 1;
                    }
                    if let Some(max_blocks) = bootstrap.miner_max_blocks {
                        if blocks_found >= max_blocks {
                            return Ok(());
                        }
                    }
                },
            }
        }
    }
}

async fn connect(config: &MinerConfig) -> Result<(BaseNodeClient<Channel>, WalletClient<Channel>), MinerError> {
    let base_node_addr = config.base_node_grpc_address.clone();
    info!("Connecting to base node at {}", base_node_addr);
    let node_conn = BaseNodeClient::connect(base_node_addr).await?;
    let wallet_addr = config.wallet_grpc_address.clone();
    info!("Connecting to wallet at {}", wallet_addr);
    let wallet_conn = WalletClient::connect(wallet_addr).await?;

    Ok((node_conn, wallet_conn))
}

async fn mining_cycle(
    node_conn: &mut BaseNodeClient<Channel>,
    wallet_conn: &mut WalletClient<Channel>,
    config: &MinerConfig,
    bootstrap: &ConfigBootstrap,
) -> Result<bool, MinerError> {
    // 1. Receive new block template
    let template = node_conn
        .get_new_block_template(config.pow_algo_request())
        .await?
        .into_inner();
    let mut block_template = template
        .new_block_template
        .clone()
        .ok_or_else(|| err_empty("new_block_template"))?;

    // Validate that template is on tip
    if config.mine_on_tip_only {
        let height = block_template
            .header
            .as_ref()
            .ok_or_else(|| err_empty("header"))?
            .height;
        validate_tip(node_conn, height, bootstrap.mine_until_height).await?;
    }

    // 2. Get coinbase from wallet and add it to new block template body
    let request = coinbase_request(&template)?;
    let coinbase = wallet_conn.get_coinbase(request).await?.into_inner();
    let (output, kernel) = extract_outputs_and_kernels(coinbase)?;
    let body = block_template
        .body
        .as_mut()
        .ok_or_else(|| err_empty("new_block_template.body"))?;
    body.outputs.push(output);
    body.kernels.push(kernel);
    let target_difficulty = template
        .miner_data
        .ok_or_else(|| err_empty("miner_data"))?
        .target_difficulty;

    // 3. Receive new block data
    let block_result = node_conn.get_new_block(block_template).await?.into_inner();
    let block = block_result.block.ok_or_else(|| err_empty("block"))?;
    let header = block.clone().header.ok_or_else(|| err_empty("block.header"))?;

    // 4. Initialize miner and start receiving mining statuses in the loop
    let mut reports = Miner::init_mining(header.clone(), target_difficulty, config.num_mining_threads);
    let mut reporting_timeout = Instant::now();
    let mut block_submitted = false;
    while let Some(report) = reports.next().await {
        if let Some(header) = report.header.clone() {
            let mut submit = true;
            if let Some(min_diff) = bootstrap.miner_min_diff {
                if report.difficulty < min_diff {
                    submit = false;
                    debug!(
                        "Mined difficulty {} below minimum difficulty {}. Not submitting.",
                        report.difficulty, min_diff
                    );
                }
            }
            if let Some(max_diff) = bootstrap.miner_max_diff {
                if report.difficulty > max_diff {
                    submit = false;
                    debug!(
                        "Mined difficulty {} greater than maximum difficulty {}. Not submitting.",
                        report.difficulty, max_diff
                    );
                }
            }
            if submit {
                // Mined a block fitting the difficulty
                let block_header = BlockHeader::try_from(header.clone()).map_err(MinerError::Conversion)?;
                info!(
                    "Miner {} found block header {} with difficulty {:?}",
                    report.miner, block_header, report.difficulty,
                );
                let mut mined_block = block.clone();
                mined_block.header = Some(header);
                // 5. Sending block to the node
                node_conn.submit_block(mined_block).await?;
                block_submitted = true;
                break;
            } else {
                display_report(&report, config).await;
            }
        } else {
            display_report(&report, config).await;
        }
        if config.mine_on_tip_only && reporting_timeout.elapsed() > config.validate_tip_timeout_sec() {
            validate_tip(node_conn, report.height, bootstrap.mine_until_height).await?;
            reporting_timeout = Instant::now();
        }
    }

    // Not waiting for threads to stop, they should stop in a short while after `reports` dropped
    Ok(block_submitted)
}

async fn display_report(report: &MiningReport, config: &MinerConfig) {
    let hashrate = report.hashes as f64 / report.elapsed.as_micros() as f64;
    debug!(
        "Miner {} reported {:.2}MH/s with total {:.2}MH/s over {} threads. Height: {}. Target: {})",
        report.miner,
        hashrate,
        hashrate * config.num_mining_threads as f64,
        config.num_mining_threads,
        report.height,
        report.target_difficulty,
    );
}

/// If config
async fn validate_tip(
    node_conn: &mut BaseNodeClient<Channel>,
    height: u64,
    mine_until_height: Option<u64>,
) -> Result<(), MinerError> {
    let tip = node_conn
        .get_tip_info(tari_app_grpc::tari_rpc::Empty {})
        .await?
        .into_inner();
    let longest_height = tip.clone().metadata.unwrap().height_of_longest_chain;
    if let Some(height) = mine_until_height {
        if longest_height >= height {
            return Err(MinerError::MineUntilHeightReached(height));
        }
    }
    if height <= longest_height {
        return Err(MinerError::MinerLostBlock(height));
    }
    if !tip.initial_sync_achieved || tip.metadata.is_none() {
        return Err(MinerError::NodeNotReady);
    }
    if height <= longest_height {
        return Err(MinerError::MinerLostBlock(height));
    }
    Ok(())
}
