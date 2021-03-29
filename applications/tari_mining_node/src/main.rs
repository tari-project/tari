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
use tari_app_grpc::tari_rpc::{base_node_client::BaseNodeClient, wallet_client::WalletClient};
use tari_app_utilities::{initialization::init_configuration, utilities::ExitCodes};
use tari_common::{configuration::bootstrap::ApplicationType, ConfigBootstrap, DefaultConfigLoader, GlobalConfig};
use tokio::{runtime::Runtime, time::delay_for};
use tonic::transport::Channel;
use utils::{coinbase_request, extract_outputs_and_kernels};

mod config;
mod difficulty;
mod errors;
mod miner;
mod utils;

use crate::miner::MiningReport;
use errors::{err_empty, MinerError};
use miner::Miner;
use std::time::Instant;

/// Application entry point
fn main() {
    let mut rt = Runtime::new().expect("Failed to start tokio runtime");
    match rt.block_on(main_inner()) {
        Ok(_) => std::process::exit(0),
        Err(exit_code) => {
            eprintln!("Fatal error: {}", exit_code);
            error!("Exiting with code: {}", exit_code);
            std::process::exit(exit_code.as_i32())
        },
    }
}

async fn main_inner() -> Result<(), ExitCodes> {
    let (bootstrap, global, cfg) = init_configuration(ApplicationType::MiningNode)?;
    println!("{:?}", bootstrap);
    let config = <MinerConfig as DefaultConfigLoader>::load_from(&cfg).expect("Failed to load config");
    let (mut node_conn, mut wallet_conn) = connect(&config, &global).await.map_err(ExitCodes::grpc)?;

    let mut blocks_found: u64 = 0;
    loop {
        debug!("Starting new mining cycle");
        match mining_cycle(&mut node_conn, &mut wallet_conn, &config, &bootstrap).await {
            err @ Err(MinerError::GrpcConnection(_)) | err @ Err(MinerError::GrpcStatus(_)) => {
                // Any GRPC error we will try to reconnect with a standard delay
                error!("Connection error: {:?}", err);
                loop {
                    debug!("Holding for {:?}", config.wait_timeout());
                    delay_for(config.wait_timeout()).await;
                    match connect(&config, &global).await {
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
            Err(MinerError::MinerLostBlock(h)) => {
                info!("Height {} already mined by other node. Restarting ...", h);
            },
            Err(err) => {
                error!("Error: {:?}", err);
                debug!("Holding for {:?}", config.wait_timeout());
                delay_for(config.wait_timeout()).await;
            },
            _ => {
                blocks_found += 1;
                if let Some(max_blocks) = bootstrap.miner_max_blocks {
                    if blocks_found >= max_blocks {
                        return Ok(());
                    }
                }
            },
        }
    }
}

async fn connect(
    config: &MinerConfig,
    global: &GlobalConfig,
) -> Result<(BaseNodeClient<Channel>, WalletClient<Channel>), MinerError>
{
    let base_node_addr = config.base_node_addr(&global);
    info!("Connecting to base node at {}", base_node_addr);
    let node_conn = BaseNodeClient::connect(base_node_addr.clone()).await?;
    let wallet_addr = config.wallet_addr(&global);
    info!("Connecting to wallet at {}", wallet_addr);
    let wallet_conn = WalletClient::connect(wallet_addr.clone()).await?;

    Ok((node_conn, wallet_conn))
}

async fn mining_cycle(
    node_conn: &mut BaseNodeClient<Channel>,
    wallet_conn: &mut WalletClient<Channel>,
    config: &MinerConfig,
    bootstrap: &ConfigBootstrap,
) -> Result<(), MinerError>
{
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
        validate_tip(node_conn, height).await?;
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
    let template_time = Instant::now();
    let mut reporting_timeout = Instant::now();
    while let Some(report) = reports.next().await {
        if let Some(header) = report.header.clone() {
            let mut submit = true;
            if let Some(min_diff) = bootstrap.miner_min_diff {
                if report.difficulty < min_diff {
                    submit = false;
                }
            }
            if let Some(max_diff) = bootstrap.miner_max_diff {
                if report.difficulty > max_diff {
                    submit = false;
                }
            }
            if submit {
                // Mined a block fitting the difficulty
                info!(
                    "Miner {} found block header {:?} with difficulty {:?}",
                    report.miner, header, report.difficulty,
                );
                let mut mined_block = block.clone();
                mined_block.header = Some(header);
                // 5. Sending block to the node
                node_conn.submit_block(mined_block).await?;
                break;
            } else {
                display_report(&report, config, template_time).await;
            }
        } else {
            display_report(&report, config, template_time).await;
        }
        if config.mine_on_tip_only && reporting_timeout.elapsed() > config.validate_tip_timeout_sec() {
            validate_tip(node_conn, report.height).await?;
            reporting_timeout = Instant::now();
        }
    }
    // Not waiting for threads to stop, they should stop in a short while after `reports` dropped
    Ok(())
}

async fn display_report(report: &MiningReport, config: &MinerConfig, template_time: Instant) {
    let hashrate = report.hashes as f64 / report.elapsed.as_micros() as f64;
    let estimated_time = report.target_difficulty as f64 / (hashrate * config.num_mining_threads as f64 * 1000000.0);
    let remaining = estimated_time as i32 - template_time.elapsed().as_secs() as i32;
    debug!(
        "Miner {} reported {:.2}MH/s with total {:.2}MH/s over {} threads. Height: {}. Target: {}, Estimated block in \
         approx. {}m{}s (+/- Ave. {:.0}s)",
        report.miner,
        hashrate,
        hashrate * config.num_mining_threads as f64,
        config.num_mining_threads,
        report.height,
        report.target_difficulty,
        remaining / 60,
        remaining % 60,
        estimated_time,
    );
}

/// If config
async fn validate_tip(node_conn: &mut BaseNodeClient<Channel>, height: u64) -> Result<(), MinerError> {
    let tip = node_conn
        .get_tip_info(tari_app_grpc::tari_rpc::Empty {})
        .await?
        .into_inner();
    if !tip.initial_sync_achieved || tip.metadata.is_none() {
        return Err(MinerError::NodeNotReady);
    }
    let longest_height = tip.metadata.unwrap().height_of_longest_chain;
    if height <= longest_height {
        return Err(MinerError::MinerLostBlock(height));
    }
    Ok(())
}
