//  Copyright 2024. The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{convert::TryFrom, str::FromStr, sync::Arc, thread, time::Instant};

use futures::stream::StreamExt;
use log::*;
use minotari_app_grpc::{
    authentication::ClientAuthenticationInterceptor,
    tari_rpc::{
        base_node_client::BaseNodeClient,
        pow_algo::PowAlgos,
        sha_p2_pool_client::ShaP2PoolClient,
        Block,
        Difficulty,
        GetNewBlockRequest,
        PowAlgo,
        SubmitBlockRequest,
        SubmitBlockResponse,
        TransactionOutput as GrpcTransactionOutput,
    },
    tls::protocol_string,
};
use minotari_app_utilities::parse_miner_input::{
    base_node_socket_address,
    verify_base_node_grpc_mining_responses,
    wallet_payment_address,
    BaseNodeGrpcClient,
    ShaP2PoolGrpcClient,
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
        transaction_components::{encrypted_data::PaymentId, CoinBaseExtra},
    },
};
use tari_crypto::ristretto::RistrettoPublicKey;
use tari_utilities::hex::Hex;
use tokio::{sync::Mutex, time::sleep};
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
    let cfg = load_configuration(
        config_path.as_path(),
        true,
        cli.non_interactive_mode,
        &cli,
        cli.common.network,
    )?;
    let mut config = MinerConfig::load_from(&cfg).expect("Failed to load config");
    config.set_base_path(cli.common.get_base_path());

    debug!(target: LOG_TARGET_FILE, "{:?}", config);
    let key_manager = create_memory_db_key_manager().map_err(|err| {
        ExitError::new(
            ExitCode::KeyManagerServiceError,
            "'wallet_payment_address' ".to_owned() + &err.to_string(),
        )
    })?;
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
        let mut mc = Controller::new(config.num_mining_threads).map_err(|e| {
            debug!(target: LOG_TARGET_FILE, "Error loading mining controller: {}", e);
            ExitError::new(
                ExitCode::UnknownError,
                format!("Error loading mining controller: {}", e),
            )
        })?;
        let cc = crate::stratum::controller::Controller::new(&url, Some(miner_address), None, None, mc.tx.clone())
            .map_err(|e| {
                debug!(
                    target: LOG_TARGET_FILE,
                    "Error loading stratum client controller: {:?}", e
                );
                ExitError::new(
                    ExitCode::UnknownError,
                    format!("Error loading mining controller: {}", e),
                )
            })?;
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
        let node_clients = connect(&config)
            .await
            .map_err(|e| ExitError::new(ExitCode::GrpcError, e.to_string()))?;
        let mut base_node_client = node_clients.base_node_client;
        let mut p2pool_node_client = node_clients.p2pool_node_client;

        if let Err(e) = verify_base_node_responses(&mut base_node_client, &config).await {
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
                &mut base_node_client,
                p2pool_node_client.clone(),
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
                                base_node_client = nc.base_node_client;
                                p2pool_node_client = nc.p2pool_node_client;
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

pub struct NodeClientResult {
    base_node_client: BaseNodeGrpcClient,
    p2pool_node_client: Option<ShaP2PoolGrpcClient>,
}

async fn connect(config: &MinerConfig) -> Result<NodeClientResult, MinerError> {
    // always connect to base node first
    let base_node_client = match connect_base_node(config).await {
        Ok(client) => client,
        Err(e) => {
            error!(target: LOG_TARGET, "Could not connect to base node: {}", e);
            let msg = "Could not connect to base node. \nIs the base node's gRPC running? Try running it with \
                       `--enable-grpc` or enable it in the config.";
            println!("{}", msg);
            return Err(e);
        },
    };

    // init client to sha p2pool grpc if enabled
    let mut p2pool_node_client = None;
    if config.sha_p2pool_enabled {
        p2pool_node_client = match connect_sha_p2pool(config).await {
            Ok(client) => Some(client),
            Err(e) => {
                error!(target: LOG_TARGET, "Could not connect to base node: {}", e);
                let msg = "Could not connect to base node. \nIs the base node's gRPC running? Try running it with \
                           `--enable-grpc` or enable it in the config.";
                println!("{}", msg);
                return Err(e);
            },
        };
    }

    Ok(NodeClientResult {
        base_node_client,
        p2pool_node_client,
    })
}

async fn connect_sha_p2pool(config: &MinerConfig) -> Result<ShaP2PoolGrpcClient, MinerError> {
    let socketaddr = base_node_socket_address(config.base_node_grpc_address.clone(), config.network)?;
    let base_node_addr = format!(
        "{}{}",
        protocol_string(config.base_node_grpc_tls_domain_name.is_some()),
        socketaddr,
    );

    info!(target: LOG_TARGET, "👛 Connecting to p2pool node at {}", base_node_addr);
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
    let node_conn = ShaP2PoolClient::with_interceptor(
        channel,
        ClientAuthenticationInterceptor::create(&config.base_node_grpc_authentication)?,
    );

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

struct GetNewBlockResponse {
    block: Block,
    tari_target_difficulty: u64,
    p2pool_target_difficulty: Option<u64>,
}

/// Gets a new block from base node or p2pool node if its enabled in config
async fn get_new_block(
    base_node_client: &mut BaseNodeGrpcClient,
    sha_p2pool_client: Arc<Mutex<Option<ShaP2PoolGrpcClient>>>,
    config: &MinerConfig,
    cli: &Cli,
    key_manager: &MemoryDbKeyManager,
    wallet_payment_address: &TariAddress,
    consensus_manager: &ConsensusManager,
) -> Result<GetNewBlockResponse, MinerError> {
    if config.sha_p2pool_enabled {
        if let Some(client) = sha_p2pool_client.lock().await.as_mut() {
            return get_new_block_p2pool_node(config, client, wallet_payment_address).await;
        }
    }

    get_new_block_base_node(
        base_node_client,
        config,
        cli,
        key_manager,
        wallet_payment_address,
        consensus_manager,
    )
    .await
}

async fn get_new_block_base_node(
    base_node_client: &mut BaseNodeGrpcClient,
    config: &MinerConfig,
    cli: &Cli,
    key_manager: &MemoryDbKeyManager,
    wallet_payment_address: &TariAddress,
    consensus_manager: &ConsensusManager,
) -> Result<GetNewBlockResponse, MinerError> {
    debug!(target: LOG_TARGET, "Getting new block template");
    let template_response = base_node_client
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
        validate_tip(base_node_client, height, cli.mine_until_height).await?;
    }

    debug!(target: LOG_TARGET, "Getting coinbase");
    let miner_data = template_response.miner_data.ok_or_else(|| err_empty("miner_data"))?;
    let fee = MicroMinotari::from(miner_data.total_fees);
    let reward = MicroMinotari::from(miner_data.reward);
    let (coinbase_output, coinbase_kernel) = generate_coinbase(
        fee,
        reward,
        height,
        &CoinBaseExtra::try_from(config.coinbase_extra.as_bytes().to_vec())?,
        key_manager,
        wallet_payment_address,
        true,
        consensus_manager.consensus_constants(height),
        config.range_proof_type,
        PaymentId::Empty,
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
    let tari_target_difficulty = miner_data.tari_target_difficulty;

    debug!(target: LOG_TARGET, "Asking base node to assemble the MMR roots");
    let block_result = base_node_client.get_new_block(block_template).await?.into_inner();
    Ok(GetNewBlockResponse {
        block: block_result.block.ok_or_else(|| err_empty("block"))?,
        tari_target_difficulty,
        p2pool_target_difficulty: None,
    })
}

async fn get_new_block_p2pool_node(
    config: &MinerConfig,
    sha_p2pool_client: &mut ShaP2PoolGrpcClient,
    wallet_payment_address: &TariAddress,
) -> Result<GetNewBlockResponse, MinerError> {
    let pow_algo = PowAlgo {
        pow_algo: PowAlgos::Sha3x.into(),
    };
    let coinbase_extra = if config.coinbase_extra.trim().is_empty() {
        String::new()
    } else {
        config.coinbase_extra.clone()
    };
    let block_result = sha_p2pool_client
        .get_new_block(GetNewBlockRequest {
            pow: Some(pow_algo),
            coinbase_extra,
            wallet_payment_address: wallet_payment_address.to_base58(),
        })
        .await?
        .into_inner();
    let new_block_result = block_result.block.ok_or_else(|| err_empty("block result"))?;
    let block = new_block_result.block.ok_or_else(|| err_empty("block response"))?;
    Ok(GetNewBlockResponse {
        block,
        tari_target_difficulty: block_result.tari_target_difficulty,
        p2pool_target_difficulty: Some(block_result.p2pool_target_difficulty),
    })
}

async fn submit_block(
    config: &MinerConfig,
    base_node_client: &mut BaseNodeGrpcClient,
    sha_p2pool_client: Option<&mut ShaP2PoolGrpcClient>,
    submit_to_base_node: bool,
    block: Block,
    wallet_payment_address: &TariAddress,
    difficulty: u64,
) -> Result<SubmitBlockResponse, MinerError> {
    let height = block.header.clone().unwrap_or_default().height;
    let (mut resp_bn, mut resp_p2p) = (None, None);
    if submit_to_base_node {
        debug!(target: LOG_TARGET, "Submitting to base node");
        let bn_timer = Instant::now();
        resp_bn = Some(
            base_node_client
                .submit_block(block.clone())
                .await
                .map_err(MinerError::GrpcStatus)?
                .into_inner(),
        );
        debug!(
            target: LOG_TARGET,
            "Submitted block #{} to Minotari node in {:.2?} (SubmitBlock)",
            height,
            bn_timer.elapsed(),
        );
    }
    if config.sha_p2pool_enabled {
        if let Some(client) = sha_p2pool_client {
            let p2p_timer = Instant::now();
            debug!(target: LOG_TARGET, "Submitting to p2pool");
            resp_p2p = Some(
                client
                    .submit_block(SubmitBlockRequest {
                        block: Some(block),
                        wallet_payment_address: wallet_payment_address.to_hex(),
                        achieved_difficulty: Some(Difficulty { difficulty }),
                    })
                    .await
                    .map_err(MinerError::GrpcStatus)?
                    .into_inner(),
            );
            debug!(
                target: LOG_TARGET,
                "Submitted block #{} to p2pool in {:.2?} (SubmitBlock)",
                height,
                p2p_timer.elapsed(),
            );
        } else {
            error!(target: LOG_TARGET, "p2pool mode enabled, but no p2pool node to submit block to");
        }
    }

    match (resp_bn, resp_p2p) {
        (Some(resp_1), Some(resp_2)) => {
            if resp_1 != resp_2 {
                return Err(MinerError::LogicalError(
                    "Base node and p2pool node did not agree on block submission".to_string(),
                ));
            }
            Ok(resp_1)
        },
        (Some(resp), None) => Ok(resp),
        (None, Some(resp)) => Ok(resp),
        (None, None) => Err(MinerError::LogicalError(
            "No p2pool node or base node to submit block to".to_string(),
        )),
    }
}

#[allow(clippy::too_many_lines)]
async fn mining_cycle(
    base_node_client: &mut BaseNodeGrpcClient,
    sha_p2pool_client: Option<ShaP2PoolGrpcClient>,
    config: &MinerConfig,
    cli: &Cli,
    key_manager: &MemoryDbKeyManager,
    wallet_payment_address: &TariAddress,
    consensus_manager: &ConsensusManager,
) -> Result<bool, MinerError> {
    let sha_p2pool_client = Arc::new(Mutex::new(sha_p2pool_client));
    let block_result = get_new_block(
        base_node_client,
        sha_p2pool_client.clone(),
        config,
        cli,
        key_manager,
        wallet_payment_address,
        consensus_manager,
    )
    .await?;
    let block = block_result.block;
    let header = block.clone().header.ok_or_else(|| err_empty("block.header"))?;

    debug!(target: LOG_TARGET, "Initializing miner");
    let mut reports = Miner::init_mining(
        header.clone(),
        if let Some(val) = block_result.p2pool_target_difficulty {
            val
        } else {
            block_result.tari_target_difficulty
        },
        config.num_mining_threads,
        false,
    );
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
                let submit_to_base_node = if block_result.p2pool_target_difficulty.is_some() {
                    report.difficulty >= block_result.tari_target_difficulty
                } else {
                    true
                };
                submit_block(
                    config,
                    base_node_client,
                    sha_p2pool_client.lock().await.as_mut(),
                    submit_to_base_node,
                    mined_block,
                    wallet_payment_address,
                    report.difficulty,
                )
                .await?;
                block_submitted = true;
                break;
            } else {
                display_report(&report, config.num_mining_threads).await;
            }
        } else {
            display_report(&report, config.num_mining_threads).await;
        }
        if config.mine_on_tip_only && reporting_timeout.elapsed() > config.validate_tip_interval() {
            validate_tip(base_node_client, report.height, cli.mine_until_height).await?;
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
