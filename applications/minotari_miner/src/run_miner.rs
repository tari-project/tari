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

use std::{convert::TryFrom, str::FromStr, thread, time::Instant};

use futures::stream::StreamExt;
use log::*;
use minotari_app_grpc::{
    authentication::ClientAuthenticationInterceptor,
    tari_rpc::{base_node_client::BaseNodeClient, TransactionOutput as GrpcTransactionOutput},
    tls::protocol_string,
};
use minotari_app_utilities::parse_miner_input::{
    base_node_socket_address,
    verify_base_node_grpc_mining_responses,
    wallet_payment_address,
    BaseNodeGrpcClient,
};
use tari_common::{
    exit_codes::{ExitCode, ExitError},
    load_configuration,
    DefaultConfigLoader,
};
use tari_common_types::tari_address::TariAddress;
use tari_core::{
    blocks::BlockHeader,
    consensus::ConsensusManager,
    transactions::{
        generate_coinbase,
        key_manager::{create_memory_db_key_manager, MemoryDbKeyManager},
        tari_amount::MicroMinotari,
    },
};
use tari_crypto::ristretto::RistrettoPublicKey;
use tari_utilities::hex::Hex;
use tokio::time::sleep;
use tonic::transport::{Certificate, ClientTlsConfig, Endpoint};

use crate::{
    cli::Cli,
    config::MinerConfig,
    errors::{err_empty, MinerError},
    miner::{Miner, MiningReport},
    stratum::stratum_controller::controller::Controller,
};

pub const LOG_TARGET: &str = "minotari::miner::main";
pub const LOG_TARGET_FILE: &str = "minotari::logging::miner::main";

#[allow(clippy::too_many_lines)]
pub async fn start_miner(cli: Cli) -> Result<(), ExitError> {
    let config_path = cli.common.config_path();
    let cfg = load_configuration(config_path.as_path(), true, cli.non_interactive_mode, &cli)?;
    let mut config = MinerConfig::load_from(&cfg).expect("Failed to load config");
    config.set_base_path(cli.common.get_base_path());

    debug!(target: LOG_TARGET_FILE, "{:?}", config);
    let key_manager = create_memory_db_key_manager();
    let wallet_payment_address = wallet_payment_address(config.wallet_payment_address.clone(), config.network)
        .map_err(|err| {
            ExitError::new(
                ExitCode::WalletPaymentAddress,
                "'wallet_payment_address' ".to_owned() + &err.to_string(),
            )
        })?;
    debug!(target: LOG_TARGET_FILE, "wallet_payment_address: {}", wallet_payment_address);
    let consensus_manager = ConsensusManager::builder(config.network)
        .build()
        .map_err(|err| ExitError::new(ExitCode::ConsensusManagerBuilderError, err.to_string()))?;

    if !config.stratum_mining_wallet_address.is_empty() && !config.stratum_mining_pool_address.is_empty() {
        let url = config.stratum_mining_pool_address.clone();
        let mut miner_address = config.stratum_mining_wallet_address.clone();
        let _ = RistrettoPublicKey::from_hex(&miner_address).map_err(|_| {
            ExitError::new(
                ExitCode::ConfigError,
                "Miner is not configured with a valid wallet address.",
            )
        })?;
        if !config.mining_worker_name.is_empty() {
            miner_address += &format!("{}{}", ".", config.mining_worker_name);
        }
        let mut mc = Controller::new(config.num_mining_threads).unwrap_or_else(|e| {
            debug!(target: LOG_TARGET_FILE, "Error loading mining controller: {}", e);
            panic!("Error loading mining controller: {}", e);
        });
        let cc = crate::stratum::controller::Controller::new(&url, Some(miner_address), None, None, mc.tx.clone())
            .unwrap_or_else(|e| {
                debug!(
                    target: LOG_TARGET_FILE,
                    "Error loading stratum client controller: {:?}", e
                );
                panic!("Error loading stratum client controller: {:?}", e);
            });
        mc.set_client_tx(cc.tx.clone());

        let _join_handle = thread::Builder::new()
            .name("client_controller".to_string())
            .spawn(move || {
                cc.run();
            });

        mc.run()
            .await
            .map_err(|err| ExitError::new(ExitCode::UnknownError, format!("Stratum error: {:?}", err)))?;

        Ok(())
    } else {
        let mut node_conn = connect(&config)
            .await
            .map_err(|e| ExitError::new(ExitCode::GrpcError, e.to_string()))?;
        if let Err(e) = verify_base_node_responses(&mut node_conn, &config).await {
            if let MinerError::BaseNodeNotResponding(_) = e {
                error!(target: LOG_TARGET, "{}", e.to_string());
                println!();
                let msg = "Could not connect to the base node. \nAre the base node's gRPC mining methods allowed in \
                           its 'config.toml'? Please ensure these methods are enabled in:\n  \
                           'grpc_server_allow_methods': \"get_new_block_template\", \"get_tip_info\", \
                           \"get_new_block\", \"submit_block\"";
                println!("{}", msg);
                println!();
                return Err(ExitError::new(ExitCode::GrpcError, e.to_string()));
            }
        }

        let mut blocks_found: u64 = 0;
        loop {
            debug!(target: LOG_TARGET, "Starting new mining cycle");
            match mining_cycle(
                &mut node_conn,
                &config,
                &cli,
                &key_manager,
                &wallet_payment_address,
                &consensus_manager,
            )
            .await
            {
                err @ Err(MinerError::GrpcConnection(_)) | err @ Err(MinerError::GrpcStatus(_)) => {
                    // Any GRPC error we will try to reconnect with a standard delay
                    error!(target: LOG_TARGET, "Connection error: {:?}", err);
                    loop {
                        info!(target: LOG_TARGET, "Holding for {:?}", config.wait_timeout());
                        sleep(config.wait_timeout()).await;
                        match connect(&config).await {
                            Ok(nc) => {
                                node_conn = nc;
                                break;
                            },
                            Err(err) => {
                                error!(target: LOG_TARGET, "Connection error: {:?}", err);
                                continue;
                            },
                        }
                    }
                },
                Err(MinerError::MineUntilHeightReached(h)) => {
                    warn!(
                        target: LOG_TARGET,
                        "Prescribed blockchain height {} reached. Aborting ...", h
                    );
                    return Ok(());
                },
                Err(MinerError::MinerLostBlock(h)) => {
                    warn!(
                        target: LOG_TARGET,
                        "Height {} already mined by other node. Restarting ...", h
                    );
                },
                Err(err) => {
                    error!(target: LOG_TARGET, "Error: {:?}", err);
                    sleep(config.wait_timeout()).await;
                },
                Ok(submitted) => {
                    info!(target: LOG_TARGET, "💰 Found block");
                    if submitted {
                        blocks_found += 1;
                    }
                    if let Some(max_blocks) = cli.miner_max_blocks {
                        if blocks_found >= max_blocks {
                            return Ok(());
                        }
                    }
                },
            }
        }
    }
}

async fn connect(config: &MinerConfig) -> Result<BaseNodeGrpcClient, MinerError> {
    let node_conn = match connect_base_node(config).await {
        Ok(client) => client,
        Err(e) => {
            error!(target: LOG_TARGET, "Could not connect to base node: {}", e);
            let msg = "Could not connect to base node. \nIs the base node's gRPC running? Try running it with \
                       `--enable-grpc` or enable it in the config.";
            println!("{}", msg);
            return Err(e);
        },
    };

    Ok(node_conn)
}

async fn connect_base_node(config: &MinerConfig) -> Result<BaseNodeGrpcClient, MinerError> {
    let socketaddr = base_node_socket_address(config.base_node_grpc_address.clone(), config.network)?;
    let base_node_addr = format!(
        "{}{}",
        protocol_string(config.base_node_grpc_tls_domain_name.is_some()),
        socketaddr,
    );

    info!(target: LOG_TARGET, "👛 Connecting to base node at {}", base_node_addr);
    let mut endpoint = Endpoint::from_str(&base_node_addr)?;

    if let Some(domain_name) = config.base_node_grpc_tls_domain_name.as_ref() {
        let pem = tokio::fs::read(config.config_dir.join(&config.base_node_grpc_ca_cert_filename))
            .await
            .map_err(|e| MinerError::TlsConnectionError(e.to_string()))?;
        let ca = Certificate::from_pem(pem);

        let tls = ClientTlsConfig::new().ca_certificate(ca).domain_name(domain_name);
        endpoint = endpoint
            .tls_config(tls)
            .map_err(|e| MinerError::TlsConnectionError(e.to_string()))?;
    }

    let channel = endpoint
        .connect()
        .await
        .map_err(|e| MinerError::TlsConnectionError(e.to_string()))?;
    let node_conn = BaseNodeClient::with_interceptor(
        channel,
        ClientAuthenticationInterceptor::create(&config.base_node_grpc_authentication)?,
    );

    Ok(node_conn)
}

async fn verify_base_node_responses(
    node_conn: &mut BaseNodeGrpcClient,
    config: &MinerConfig,
) -> Result<(), MinerError> {
    if let Err(e) = verify_base_node_grpc_mining_responses(node_conn, config.pow_algo_request()).await {
        return Err(MinerError::BaseNodeNotResponding(e));
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
async fn mining_cycle(
    node_conn: &mut BaseNodeGrpcClient,
    config: &MinerConfig,
    cli: &Cli,
    key_manager: &MemoryDbKeyManager,
    wallet_payment_address: &TariAddress,
    consensus_manager: &ConsensusManager,
) -> Result<bool, MinerError> {
    debug!(target: LOG_TARGET, "Getting new block template");
    let template_response = node_conn
        .get_new_block_template(config.pow_algo_request())
        .await?
        .into_inner();
    let mut block_template = template_response
        .new_block_template
        .clone()
        .ok_or_else(|| err_empty("new_block_template"))?;
    let height = block_template
        .header
        .as_ref()
        .ok_or_else(|| err_empty("header"))?
        .height;

    if config.mine_on_tip_only {
        debug!(
            target: LOG_TARGET,
            "Checking if base node is synced, because mine_on_tip_only is true"
        );
        validate_tip(node_conn, height, cli.mine_until_height).await?;
    }

    debug!(target: LOG_TARGET, "Getting coinbase");
    let miner_data = template_response.miner_data.ok_or_else(|| err_empty("miner_data"))?;
    let fee = MicroMinotari::from(miner_data.total_fees);
    let reward = MicroMinotari::from(miner_data.reward);
    let (coinbase_output, coinbase_kernel) = generate_coinbase(
        fee,
        reward,
        height,
        config.coinbase_extra.as_bytes(),
        key_manager,
        wallet_payment_address,
        config.stealth_payment,
        consensus_manager.consensus_constants(height),
        config.range_proof_type,
    )
    .await
    .map_err(|e| MinerError::CoinbaseError(e.to_string()))?;
    debug!(target: LOG_TARGET, "Coinbase kernel: {}", coinbase_kernel);
    debug!(target: LOG_TARGET, "Coinbase output: {}", coinbase_output);

    let body = block_template
        .body
        .as_mut()
        .ok_or_else(|| err_empty("new_block_template.body"))?;
    let grpc_output = GrpcTransactionOutput::try_from(coinbase_output.clone()).map_err(MinerError::Conversion)?;
    body.outputs.push(grpc_output);
    body.kernels.push(coinbase_kernel.into());
    let target_difficulty = miner_data.target_difficulty;

    debug!(target: LOG_TARGET, "Asking base node to assemble the MMR roots");
    let block_result = node_conn.get_new_block(block_template).await?.into_inner();
    let block = block_result.block.ok_or_else(|| err_empty("block"))?;
    let header = block.clone().header.ok_or_else(|| err_empty("block.header"))?;

    debug!(target: LOG_TARGET, "Initializing miner");
    let mut reports = Miner::init_mining(header.clone(), target_difficulty, config.num_mining_threads, false);
    let mut reporting_timeout = Instant::now();
    let mut block_submitted = false;
    while let Some(report) = reports.next().await {
        if let Some(header) = report.header.clone() {
            let mut submit = true;
            if let Some(min_diff) = cli.miner_min_diff {
                if report.difficulty < min_diff {
                    submit = false;
                    debug!(
                        target: LOG_TARGET_FILE,
                        "Mined difficulty {} below minimum difficulty {}. Not submitting.", report.difficulty, min_diff
                    );
                }
            }
            if let Some(max_diff) = cli.miner_max_diff {
                if report.difficulty > max_diff {
                    submit = false;
                    debug!(
                        target: LOG_TARGET_FILE,
                        "Mined difficulty {} greater than maximum difficulty {}. Not submitting.",
                        report.difficulty,
                        max_diff
                    );
                }
            }
            if submit {
                // Mined a block fitting the difficulty
                let block_header = BlockHeader::try_from(header.clone()).map_err(MinerError::Conversion)?;
                debug!(
                    target: LOG_TARGET,
                    "Miner found block header {} with difficulty {:?}", block_header, report.difficulty,
                );
                let mut mined_block = block.clone();
                mined_block.header = Some(header);
                // 5. Sending block to the node
                node_conn.submit_block(mined_block).await?;
                block_submitted = true;
                break;
            } else {
                display_report(&report, config.num_mining_threads).await;
            }
        } else {
            display_report(&report, config.num_mining_threads).await;
        }
        if config.mine_on_tip_only && reporting_timeout.elapsed() > config.validate_tip_interval() {
            validate_tip(node_conn, report.height, cli.mine_until_height).await?;
            reporting_timeout = Instant::now();
        }
    }

    // Not waiting for threads to stop, they should stop in a short while after `reports` dropped
    Ok(block_submitted)
}

pub async fn display_report(report: &MiningReport, num_mining_threads: usize) {
    let hashrate = report.hashes as f64 / report.elapsed.as_micros() as f64;
    info!(
        target: LOG_TARGET,
        "⛏ Miner {:0>2} reported {:.2}MH/s with total {:.2}MH/s over {} threads. Height: {}. Target: {})",
        report.miner,
        hashrate,
        hashrate * num_mining_threads as f64,
        num_mining_threads,
        report.height,
        report.target_difficulty,
    );
}

/// If config
async fn validate_tip(
    node_conn: &mut BaseNodeGrpcClient,
    height: u64,
    mine_until_height: Option<u64>,
) -> Result<(), MinerError> {
    let tip = node_conn
        .get_tip_info(minotari_app_grpc::tari_rpc::Empty {})
        .await?
        .into_inner();
    let longest_height = tip.clone().metadata.unwrap().best_block_height;
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
