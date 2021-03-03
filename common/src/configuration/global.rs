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
//
//! # Global configuration of tari base layer system

use super::ConfigurationError;
use config::{Config, ConfigError, Environment};
use multiaddr::Multiaddr;
use std::{
    convert::TryInto,
    fmt::{Display, Formatter, Result as FormatResult},
    net::SocketAddr,
    num::{NonZeroU16, TryFromIntError},
    path::PathBuf,
    str::FromStr,
    time::Duration,
};
use tari_storage::lmdb_store::LMDBConfig;

const DB_INIT_DEFAULT_MB: usize = 1000;
const DB_GROW_DEFAULT_MB: usize = 500;
const DB_RESIZE_DEFAULT_MB: usize = 100;

const DB_INIT_MIN_MB: i64 = 100;
const DB_GROW_MIN_MB: i64 = 20;
const DB_RESIZE_MIN_MB: i64 = 10;

//-------------------------------------        Main Configuration Struct      --------------------------------------//

#[derive(Debug, Clone)]
pub struct GlobalConfig {
    pub network: Network,
    pub comms_transport: CommsTransport,
    pub allow_test_addresses: bool,
    pub listnener_liveness_max_sessions: usize,
    pub listener_liveness_allowlist_cidrs: Vec<String>,
    pub data_dir: PathBuf,
    pub db_type: DatabaseType,
    pub db_config: LMDBConfig,
    pub orphan_storage_capacity: usize,
    pub orphan_db_clean_out_threshold: usize,
    pub pruning_horizon: u64,
    pub pruned_mode_cleanup_interval: u64,
    pub core_threads: Option<usize>,
    pub max_threads: Option<usize>,
    pub base_node_identity_file: PathBuf,
    pub public_address: Multiaddr,
    pub grpc_enabled: bool,
    pub grpc_base_node_address: SocketAddr,
    pub grpc_console_wallet_address: SocketAddr,
    pub peer_seeds: Vec<String>,
    pub dns_seeds: Vec<String>,
    pub dns_seeds_name_server: SocketAddr,
    pub dns_seeds_use_dnssec: bool,
    pub peer_db_path: PathBuf,
    pub enable_mining: bool,
    pub enable_wallet: bool,
    pub num_mining_threads: usize,
    pub base_node_tor_identity_file: PathBuf,
    pub wallet_db_file: PathBuf,
    pub console_wallet_db_file: PathBuf,
    pub wallet_identity_file: PathBuf,
    pub console_wallet_identity_file: PathBuf,
    pub wallet_tor_identity_file: PathBuf,
    pub console_wallet_tor_identity_file: PathBuf,
    pub wallet_peer_db_path: PathBuf,
    pub console_wallet_peer_db_path: PathBuf,
    pub buffer_size_base_node: usize,
    pub buffer_size_base_node_wallet: usize,
    pub buffer_rate_limit_base_node: usize,
    pub buffer_rate_limit_base_node_wallet: usize,
    pub fetch_blocks_timeout: Duration,
    pub fetch_utxos_timeout: Duration,
    pub service_request_timeout: Duration,
    pub base_node_query_timeout: Duration,
    pub transaction_broadcast_monitoring_timeout: Duration,
    pub transaction_chain_monitoring_timeout: Duration,
    pub transaction_direct_send_timeout: Duration,
    pub transaction_broadcast_send_timeout: Duration,
    pub transaction_routing_mechanism: String,
    pub transaction_num_confirmations_required: u64,
    pub console_wallet_password: Option<String>,
    pub wallet_command_send_wait_stage: String,
    pub wallet_command_send_wait_timeout: u64,
    pub wallet_base_node_service_peers: Vec<String>,
    pub wallet_base_node_service_refresh_interval: u64,
    pub wallet_base_node_service_request_max_age: u64,
    pub prevent_fee_gt_amount: bool,
    pub monerod_url: String,
    pub monerod_username: String,
    pub monerod_password: String,
    pub monerod_use_auth: bool,
    pub proxy_host_address: SocketAddr,
    pub proxy_submit_to_origin: bool,
    pub force_sync_peers: Vec<String>,
    pub wait_for_initial_sync_at_startup: bool,
    pub max_randomx_vms: usize,
    pub console_wallet_notify_file: Option<PathBuf>,
    pub auto_ping_interval: u64,
    pub blocks_behind_before_considered_lagging: u64,
    pub flood_ban_max_msg_count: usize,
}

impl GlobalConfig {
    pub fn convert_from(mut cfg: Config) -> Result<Self, ConfigurationError> {
        // Add in settings from the environment (with a prefix of TARI_NODE)
        // Eg.. `TARI_NODE_DEBUG=1 ./target/app` would set the `debug` key
        cfg.merge(Environment::with_prefix("tari").separator("__"))
            .map_err(|e| ConfigurationError::new("environment variable", &e.to_string()))?;
        let network = cfg
            .get_str("base_node.network")
            .map_err(|e| ConfigurationError::new("base_node.network", &e.to_string()))?
            .parse()?;

        convert_node_config(network, cfg)
    }
}

fn convert_node_config(network: Network, cfg: Config) -> Result<GlobalConfig, ConfigurationError> {
    let net_str = network.to_string().to_lowercase();

    let key = config_string("base_node", &net_str, "data_dir");
    let data_dir: PathBuf = cfg
        .get_str(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))?
        .into();

    let key = config_string("base_node", &net_str, "db_type");
    let db_type = cfg
        .get_str(&key)
        .map(|s| s.to_lowercase())
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))?;

    let db_type = match db_type.as_str() {
        "memory" => Ok(DatabaseType::Memory),
        "lmdb" => Ok(DatabaseType::LMDB(data_dir.join("db"))),
        invalid_opt => Err(ConfigurationError::new(
            "base_node.db_type",
            &format!("Invalid option: {}", invalid_opt),
        )),
    }?;

    let key = config_string("base_node", &net_str, "db_init_size_mb");
    let init_size_mb = match cfg.get_int(&key) {
        Ok(mb) if mb < DB_INIT_MIN_MB => {
            return Err(ConfigurationError::new(
                &key,
                &format!("DB initial size must be at least {} MB.", DB_INIT_MIN_MB),
            ))
        },
        Ok(mb) => mb as usize,
        Err(e) => match e {
            ConfigError::NotFound(_) => DB_INIT_DEFAULT_MB, // default
            other => return Err(ConfigurationError::new(&key, &other.to_string())),
        },
    };

    let key = config_string("base_node", &net_str, "db_grow_size_mb");
    let grow_size_mb = match cfg.get_int(&key) {
        Ok(mb) if mb < DB_GROW_MIN_MB => {
            return Err(ConfigurationError::new(
                &key,
                &format!("DB grow size must be at least {} MB.", DB_GROW_MIN_MB),
            ))
        },
        Ok(mb) => mb as usize,
        Err(e) => match e {
            ConfigError::NotFound(_) => DB_GROW_DEFAULT_MB, // default
            other => return Err(ConfigurationError::new(&key, &other.to_string())),
        },
    };

    let key = config_string("base_node", &net_str, "db_resize_threshold_mb");
    let resize_threshold_mb = match cfg.get_int(&key) {
        Ok(mb) if mb < DB_RESIZE_MIN_MB => {
            return Err(ConfigurationError::new(
                &key,
                &format!("DB resize threshold must be at least {} MB.", DB_RESIZE_MIN_MB),
            ))
        },
        Ok(mb) if mb as usize >= grow_size_mb => {
            return Err(ConfigurationError::new(
                &key,
                "DB resize threshold must be less than grow size.",
            ))
        },
        Ok(mb) if mb as usize >= init_size_mb => {
            return Err(ConfigurationError::new(
                &key,
                "DB resize threshold must be less than init size.",
            ))
        },
        Ok(mb) => mb as usize,
        Err(e) => match e {
            ConfigError::NotFound(_) => DB_RESIZE_DEFAULT_MB, // default
            other => return Err(ConfigurationError::new(&key, &other.to_string())),
        },
    };

    let db_config = LMDBConfig::new_from_mb(init_size_mb, grow_size_mb, resize_threshold_mb);

    let key = config_string("base_node", &net_str, "orphan_storage_capacity");
    let orphan_storage_capacity = cfg
        .get_int(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))? as usize;

    let key = config_string("base_node", &net_str, "orphan_db_clean_out_threshold");
    let orphan_db_clean_out_threshold = cfg
        .get_int(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))? as usize;

    let key = config_string("base_node", &net_str, "pruning_horizon");
    let pruning_horizon = cfg
        .get_int(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))? as u64;

    let key = config_string("base_node", &net_str, "pruned_mode_cleanup_interval");
    let pruned_mode_cleanup_interval = cfg
        .get_int(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))? as u64;

    // Thread counts
    let key = config_string("base_node", &net_str, "core_threads");
    let core_threads =
        optional(cfg.get_int(&key).map(|n| n as usize)).map_err(|e| ConfigurationError::new(&key, &e.to_string()))?;

    let key = config_string("base_node", &net_str, "max_threads");
    let max_threads =
        optional(cfg.get_int(&key).map(|n| n as usize)).map_err(|e| ConfigurationError::new(&key, &e.to_string()))?;

    // Max RandomX VMs
    let key = config_string("base_node", &net_str, "max_randomx_vms");
    let max_randomx_vms = optional(cfg.get_int(&key).map(|n| n as usize))
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))?
        .unwrap_or(2) as usize;

    // Base node identity path
    let key = config_string("base_node", &net_str, "base_node_identity_file");
    let base_node_identity_file = cfg
        .get_str(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))?
        .into();

    // Wallet identity path
    let key = config_string("base_node", &net_str, "wallet_identity_file");
    let wallet_identity_file = cfg
        .get_str(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))?
        .into();

    // Console wallet identity path
    let key = config_string("base_node", &net_str, "console_wallet_identity_file");
    let console_wallet_identity_file = cfg
        .get_str(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))?
        .into();

    let key = config_string("base_node", &net_str, "wallet_tor_identity_file");
    let wallet_tor_identity_file = cfg
        .get_str(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))?
        .into();

    let key = config_string("base_node", &net_str, "console_wallet_tor_identity_file");
    let console_wallet_tor_identity_file = cfg
        .get_str(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))?
        .into();

    // Tor private key persistence
    let key = config_string("base_node", &net_str, "base_node_tor_identity_file");
    let base_node_tor_identity_file = cfg
        .get_str(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))?
        .into();

    // Transport
    let comms_transport = network_transport_config(&cfg, &net_str)?;

    let key = config_string("base_node", &net_str, "allow_test_addresses");
    let allow_test_addresses = cfg
        .get_bool(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))?;

    // Public address
    let key = config_string("base_node", &net_str, "public_address");
    let public_address = cfg
        .get_str(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))
        .and_then(|addr| {
            addr.parse::<Multiaddr>()
                .map_err(|e| ConfigurationError::new(&key, &e.to_string()))
        })?;

    // GPRC enabled
    let key = config_string("base_node", &net_str, "grpc_enabled");
    let grpc_enabled = cfg
        .get_bool(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))?;

    let key = config_string("base_node", &net_str, "grpc_base_node_address");
    let grpc_base_node_address = cfg
        .get_str(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))
        .and_then(|addr| {
            addr.parse::<SocketAddr>()
                .map_err(|e| ConfigurationError::new(&key, &e.to_string()))
        })?;

    let key = config_string("base_node", &net_str, "grpc_console_wallet_address");
    let grpc_console_wallet_address = cfg
        .get_str(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))
        .and_then(|addr| {
            addr.parse::<SocketAddr>()
                .map_err(|e| ConfigurationError::new(&key, &e.to_string()))
        })?;

    // Peer and DNS seeds
    let key = config_string("base_node", &net_str, "peer_seeds");
    // Peer seeds can be an array or a comma separated list (e.g. in an ENVVAR)
    let peer_seeds = match cfg.get_array(&key) {
        Ok(seeds) => seeds.into_iter().map(|v| v.into_str().unwrap()).collect(),
        Err(..) => match cfg.get_str(&key) {
            Ok(s) => s.split(',').map(|v| v.to_string()).collect(),
            Err(err) => return Err(ConfigurationError::new(&key, &err.to_string())),
        },
    };

    let key = config_string("base_node", &net_str, "dns_seeds_name_server");
    let dns_seeds_name_server = cfg
        .get_str(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))
        .and_then(|s| {
            s.parse::<SocketAddr>()
                .map_err(|e| ConfigurationError::new(&key, &e.to_string()))
        })?;

    let key = config_string("base_node", &net_str, "dns_seeds_use_dnssec");
    let dns_seeds_use_dnssec = cfg
        .get_bool(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))?;

    let key = config_string("base_node", &net_str, "dns_seeds");
    let dns_seeds = optional(cfg.get_array(&key))?
        .unwrap_or_default()
        .into_iter()
        .map(|v| v.into_str().unwrap())
        .collect::<Vec<_>>();

    // Peer DB path
    let peer_db_path = data_dir.join("peer_db");
    let wallet_peer_db_path = data_dir.join("wallet_peer_db");
    let console_wallet_peer_db_path = data_dir.join("console_wallet_peer_db");

    // set base node mining
    let key = config_string("base_node", &net_str, "enable_mining");
    let enable_mining = cfg
        .get_bool(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))?;

    // set base node wallet
    let key = config_string("base_node", &net_str, "enable_wallet");
    let enable_wallet = cfg
        .get_bool(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))?;

    let key = config_string("base_node", &net_str, "num_mining_threads");
    let num_mining_threads = cfg
        .get_int(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))? as usize;

    let key = config_string("base_node", &net_str, "flood_ban_max_msg_count");
    let flood_ban_max_msg_count = cfg
        .get_int(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))? as usize;

    // block sync
    let key = config_string("base_node", &net_str, "force_sync_peers");
    let force_sync_peers = optional(
        cfg.get_array(&key)
            .map(|values| values.into_iter().map(|v| v.into_str().unwrap()).collect()),
    )?
    .unwrap_or_default();

    // Liveness auto ping interval
    let key = config_string("base_node", &net_str, "auto_ping_interval");
    let auto_ping_interval = match cfg.get_int(&key) {
        Ok(seconds) => seconds as u64,
        Err(ConfigError::NotFound(_)) => 30,
        Err(e) => return Err(ConfigurationError::new(&key, &e.to_string())),
    };

    // blocks_behind_before_considered_lagging when a node should switch over from listening to lagging
    let key = config_string("base_node", &net_str, "blocks_behind_before_considered_lagging");
    let blocks_behind_before_considered_lagging = optional(cfg.get_int(&key))?.unwrap_or(0) as u64;

    // set wallet_db_file
    let key = "wallet.wallet_db_file".to_string();
    let wallet_db_file = cfg
        .get_str(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))?
        .into();

    // set console_wallet_db_file
    let key = "wallet.console_wallet_db_file".to_string();
    let console_wallet_db_file = cfg
        .get_str(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))?
        .into();

    let key = "wallet.base_node_query_timeout";
    let base_node_query_timeout = Duration::from_secs(
        cfg.get_int(&key)
            .map_err(|e| ConfigurationError::new(&key, &e.to_string()))? as u64,
    );

    let key = "wallet.transaction_broadcast_monitoring_timeout";
    let transaction_broadcast_monitoring_timeout = Duration::from_secs(
        cfg.get_int(&key)
            .map_err(|e| ConfigurationError::new(&key, &e.to_string()))? as u64,
    );

    let key = "wallet.transaction_chain_monitoring_timeout";
    let transaction_chain_monitoring_timeout = Duration::from_secs(
        cfg.get_int(&key)
            .map_err(|e| ConfigurationError::new(&key, &e.to_string()))? as u64,
    );

    let key = "wallet.transaction_direct_send_timeout";
    let transaction_direct_send_timeout = Duration::from_secs(
        cfg.get_int(&key)
            .map_err(|e| ConfigurationError::new(&key, &e.to_string()))? as u64,
    );

    let key = "wallet.transaction_broadcast_send_timeout";
    let transaction_broadcast_send_timeout = Duration::from_secs(
        cfg.get_int(&key)
            .map_err(|e| ConfigurationError::new(&key, &e.to_string()))? as u64,
    );

    let key = "wallet.transaction_num_confirmations_required";
    let transaction_num_confirmations_required = optional(cfg.get_int(&key))?.unwrap_or(3) as u64;

    let key = "wallet.prevent_fee_gt_amount";
    let prevent_fee_gt_amount = cfg
        .get_bool(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))?;

    let key = "wallet.transaction_routing_mechanism";
    let transaction_routing_mechanism =
        optional(cfg.get_str(key))?.unwrap_or_else(|| "DirectAndStoreAndForward".to_string());

    let key = "wallet.command_send_wait_stage";
    let wallet_command_send_wait_stage = optional(cfg.get_str(key))?.unwrap_or_else(|| "Broadcast".to_string());

    let key = "wallet.command_send_wait_timeout";
    let wallet_command_send_wait_timeout = optional(cfg.get_int(key))?.map(|i| i as u64).unwrap_or(300);

    let key = "wallet.base_node_service_peers";
    // Wallet base node service peers can be an array or a comma separated list (e.g. in an ENVVAR)
    let wallet_base_node_service_peers = match cfg.get_array(&key) {
        Ok(peers) => peers.into_iter().map(|v| v.into_str().unwrap()).collect(),
        Err(..) => match cfg.get_str(&key) {
            Ok(s) => s.split(',').map(|v| v.to_string()).collect(),
            Err(err) => return Err(ConfigurationError::new(&key, &err.to_string())),
        },
    };

    let key = "wallet.password";
    let console_wallet_password = optional(cfg.get_str(key))?;

    let key = "wallet.notify";
    let console_wallet_notify_file = optional(cfg.get_str(key))?.map(PathBuf::from);

    let key = "wallet.base_node_service_refresh_interval";
    let wallet_base_node_service_refresh_interval = match cfg.get_int(key) {
        Ok(seconds) => seconds as u64,
        Err(ConfigError::NotFound(_)) => 10,
        Err(e) => return Err(ConfigurationError::new(&key, &e.to_string())),
    };

    let key = "wallet.base_node_service_request_max_age";
    let wallet_base_node_service_request_max_age = match cfg.get_int(key) {
        Ok(seconds) => seconds as u64,
        Err(ConfigError::NotFound(_)) => 60,
        Err(e) => return Err(ConfigurationError::new(&key, &e.to_string())),
    };

    let key = "common.liveness_max_sessions";
    let liveness_max_sessions = cfg
        .get_int(key)
        .map_err(|e| ConfigurationError::new(key, &e.to_string()))?
        .try_into()
        .map_err(|e: TryFromIntError| ConfigurationError::new(&key, &e.to_string()))?;

    let key = "common.liveness_allowlist_cidrs";
    let liveness_allowlist_cidrs = cfg
        .get_array(key)
        .map(|values| values.iter().map(ToString::to_string).collect())
        .unwrap_or_else(|_| vec!["127.0.0.1/32".to_string()]);

    let key = "common.buffer_size_base_node";
    let buffer_size_base_node = cfg
        .get_int(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))? as usize;

    let key = "common.buffer_size_base_node_wallet";
    let buffer_size_base_node_wallet = cfg
        .get_int(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))? as usize;

    let key = "common.buffer_rate_limit_base_node";
    let buffer_rate_limit_base_node = cfg
        .get_int(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))? as usize;

    let key = "common.buffer_rate_limit_base_node_wallet";
    let buffer_rate_limit_base_node_wallet =
        cfg.get_int(&key)
            .map_err(|e| ConfigurationError::new(&key, &e.to_string()))? as usize;

    let key = "common.fetch_blocks_timeout";
    let fetch_blocks_timeout = Duration::from_secs(
        cfg.get_int(&key)
            .map_err(|e| ConfigurationError::new(&key, &e.to_string()))? as u64,
    );

    let key = "common.fetch_utxos_timeout";
    let fetch_utxos_timeout = Duration::from_secs(
        cfg.get_int(&key)
            .map_err(|e| ConfigurationError::new(&key, &e.to_string()))? as u64,
    );

    let key = "common.service_request_timeout";
    let service_request_timeout = Duration::from_secs(
        cfg.get_int(&key)
            .map_err(|e| ConfigurationError::new(&key, &e.to_string()))? as u64,
    );

    let key = config_string("merge_mining_proxy", &net_str, "monerod_url");
    let monerod_url = cfg
        .get_str(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))?;

    let key = config_string("merge_mining_proxy", &net_str, "monerod_use_auth");
    let monerod_use_auth = cfg
        .get_bool(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))?;

    let key = config_string("merge_mining_proxy", &net_str, "monerod_username");
    let monerod_username = cfg
        .get_str(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))?;

    let key = config_string("merge_mining_proxy", &net_str, "monerod_password");
    let monerod_password = cfg
        .get_str(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))?;

    let key = config_string("merge_mining_proxy", &net_str, "proxy_host_address");
    let proxy_host_address = cfg
        .get_str(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))
        .and_then(|addr| {
            addr.parse::<SocketAddr>()
                .map_err(|e| ConfigurationError::new(&key, &e.to_string()))
        })?;

    let key = config_string("merge_mining_proxy", &net_str, "wait_for_initial_sync_at_startup");
    let wait_for_initial_sync_at_startup = cfg
        .get_bool(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))?;

    let key = config_string("merge_mining_proxy", &net_str, "proxy_submit_to_origin");
    let proxy_submit_to_origin = cfg.get_bool(&key).unwrap_or_else(|_| true);

    Ok(GlobalConfig {
        network,
        comms_transport,
        allow_test_addresses,
        listnener_liveness_max_sessions: liveness_max_sessions,
        listener_liveness_allowlist_cidrs: liveness_allowlist_cidrs,
        data_dir,
        db_type,
        db_config,
        orphan_storage_capacity,
        orphan_db_clean_out_threshold,
        pruning_horizon,
        pruned_mode_cleanup_interval,
        core_threads,
        max_threads,
        base_node_identity_file,
        public_address,
        grpc_enabled,
        grpc_base_node_address,
        grpc_console_wallet_address,
        peer_seeds,
        dns_seeds,
        dns_seeds_name_server,
        dns_seeds_use_dnssec,
        peer_db_path,
        enable_mining,
        enable_wallet,
        num_mining_threads,
        base_node_tor_identity_file,
        wallet_identity_file,
        console_wallet_identity_file,
        wallet_db_file,
        console_wallet_db_file,
        wallet_tor_identity_file,
        console_wallet_tor_identity_file,
        wallet_peer_db_path,
        console_wallet_peer_db_path,
        buffer_size_base_node,
        buffer_size_base_node_wallet,
        buffer_rate_limit_base_node,
        buffer_rate_limit_base_node_wallet,
        fetch_blocks_timeout,
        fetch_utxos_timeout,
        service_request_timeout,
        base_node_query_timeout,
        transaction_broadcast_monitoring_timeout,
        transaction_chain_monitoring_timeout,
        transaction_direct_send_timeout,
        transaction_broadcast_send_timeout,
        transaction_routing_mechanism,
        transaction_num_confirmations_required,
        console_wallet_password,
        wallet_command_send_wait_stage,
        wallet_command_send_wait_timeout,
        wallet_base_node_service_peers,
        wallet_base_node_service_refresh_interval,
        wallet_base_node_service_request_max_age,
        prevent_fee_gt_amount,
        proxy_host_address,
        proxy_submit_to_origin,
        monerod_url,
        monerod_username,
        monerod_password,
        monerod_use_auth,
        force_sync_peers,
        wait_for_initial_sync_at_startup,
        max_randomx_vms,
        console_wallet_notify_file,
        auto_ping_interval,
        blocks_behind_before_considered_lagging,
        flood_ban_max_msg_count,
    })
}

/// Changes ConfigError::NotFound into None
fn optional<T>(result: Result<T, ConfigError>) -> Result<Option<T>, ConfigError> {
    match result {
        Ok(v) => Ok(Some(v)),
        Err(ConfigError::NotFound(_)) => Ok(None),
        Err(err) => Err(err),
    }
}

fn network_transport_config(cfg: &Config, network: &str) -> Result<CommsTransport, ConfigurationError> {
    let get_conf_str = |key| {
        cfg.get_str(key)
            .map_err(|err| ConfigurationError::new(key, &err.to_string()))
    };

    let get_conf_multiaddr = |key| {
        let path_str = get_conf_str(key)?;
        path_str
            .parse::<Multiaddr>()
            .map_err(|err| ConfigurationError::new(key, &err.to_string()))
    };

    let transport_key = config_string("base_node", network, "transport");
    let transport = get_conf_str(&transport_key)?;

    match transport.to_lowercase().as_str() {
        "tcp" => {
            let key = config_string("base_node", network, "tcp_listener_address");
            let listener_address = get_conf_multiaddr(&key)?;
            let key = config_string("base_node", network, "tcp_tor_socks_address");
            let tor_socks_address = get_conf_multiaddr(&key).ok();
            let key = config_string("base_node", network, "tcp_tor_socks_auth");
            let tor_socks_auth = get_conf_str(&key).ok().and_then(|auth_str| auth_str.parse().ok());

            Ok(CommsTransport::Tcp {
                listener_address,
                tor_socks_auth,
                tor_socks_address,
            })
        },
        "tor" => {
            let key = config_string("base_node", network, "tor_control_address");
            let control_server_address = get_conf_multiaddr(&key)?;

            let key = config_string("base_node", network, "tor_control_auth");
            let auth_str = get_conf_str(&key)?;
            let auth = auth_str
                .parse()
                .map_err(|err: String| ConfigurationError::new(&key, &err))?;

            let key = config_string("base_node", network, "tor_forward_address");
            let forward_address = get_conf_multiaddr(&key)?;
            let key = config_string("base_node", network, "tor_onion_port");
            let onion_port = cfg
                .get::<NonZeroU16>(&key)
                .map_err(|err| ConfigurationError::new(&key, &err.to_string()))?;

            let key = config_string("base_node", network, "tor_socks_address_override");
            let socks_address_override = match get_conf_str(&key).ok() {
                Some(addr) => Some(
                    addr.parse::<Multiaddr>()
                        .map_err(|err| ConfigurationError::new(&key, &err.to_string()))?,
                ),
                None => None,
            };

            Ok(CommsTransport::TorHiddenService {
                control_server_address,
                auth,
                socks_address_override,
                forward_address,
                onion_port,
            })
        },
        "socks5" => {
            let key = config_string("base_node", network, "socks5_proxy_address");
            let proxy_address = get_conf_multiaddr(&key)?;

            let key = config_string("base_node", network, "socks5_auth");
            let auth_str = get_conf_str(&key)?;
            let auth = auth_str
                .parse()
                .map_err(|err: String| ConfigurationError::new(&key, &err))?;

            let key = config_string("base_node", network, "socks5_listener_address");
            let listener_address = get_conf_multiaddr(&key)?;

            Ok(CommsTransport::Socks5 {
                proxy_address,
                listener_address,
                auth,
            })
        },
        t => Err(ConfigurationError::new(
            &transport_key,
            &format!("Invalid transport type '{}'", t),
        )),
    }
}

fn config_string(prefix: &str, network: &str, key: &str) -> String {
    format!("{}.{}.{}", prefix, network, key)
}

//---------------------------------------------       Network type        ------------------------------------------//
#[derive(Clone, Debug, PartialEq, Copy)]
pub enum Network {
    MainNet,
    Rincewind,
    LocalNet,
    Ridcully,
    Stibbons,
}

impl FromStr for Network {
    type Err = ConfigurationError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_lowercase().as_str() {
            "rincewind" => Ok(Self::Rincewind),
            "ridcully" => Ok(Self::Ridcully),
            "stibbons" => Ok(Self::Stibbons),
            "mainnet" => Ok(Self::MainNet),
            "localnet" => Ok(Self::LocalNet),
            invalid => Err(ConfigurationError::new(
                "network",
                &format!("Invalid network option: {}", invalid),
            )),
        }
    }
}

impl Display for Network {
    fn fmt(&self, f: &mut Formatter) -> FormatResult {
        let msg = match self {
            Self::MainNet => "mainnet",
            Self::Rincewind => "rincewind",
            Self::Ridcully => "ridcully",
            Self::Stibbons => "stibbons",
            Self::LocalNet => "localnet",
        };
        f.write_str(msg)
    }
}

//---------------------------------------------      Database type        ------------------------------------------//
#[derive(Debug, Clone)]
pub enum DatabaseType {
    LMDB(PathBuf),
    Memory,
}

//---------------------------------------------     Network Transport     ------------------------------------------//
#[derive(Debug, Clone)]
pub enum TorControlAuthentication {
    None,
    Password(String),
}

fn parse_key_value(s: &str, split_chr: char) -> (String, Option<&str>) {
    let mut parts = s.splitn(2, split_chr);
    (
        parts
            .next()
            .expect("splitn always emits at least one part")
            .to_lowercase(),
        parts.next(),
    )
}

impl FromStr for TorControlAuthentication {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (auth_type, maybe_value) = parse_key_value(s, '=');
        match auth_type.as_str() {
            "none" => Ok(TorControlAuthentication::None),
            "password" => {
                let password = maybe_value.ok_or_else(|| {
                    "Invalid format for 'password' tor authentication type. It should be in the format \
                     'password=xxxxxx'."
                        .to_string()
                })?;
                Ok(TorControlAuthentication::Password(password.to_string()))
            },
            s => Err(format!("Invalid tor auth type '{}'", s)),
        }
    }
}

#[derive(Debug, Clone)]
pub enum SocksAuthentication {
    None,
    UsernamePassword(String, String),
}

impl FromStr for SocksAuthentication {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (auth_type, maybe_value) = parse_key_value(s, '=');
        match auth_type.as_str() {
            "none" => Ok(SocksAuthentication::None),
            "username_password" => {
                let (username, password) = maybe_value
                    .and_then(|value| {
                        let (un, pwd) = parse_key_value(value, ':');
                        // If pwd is None, return None
                        pwd.map(|p| (un, p))
                    })
                    .ok_or_else(|| {
                        "Invalid format for 'username-password' socks authentication type. It should be in the format \
                         'username_password=my_username:xxxxxx'."
                            .to_string()
                    })?;
                Ok(SocksAuthentication::UsernamePassword(username, password.to_string()))
            },
            s => Err(format!("Invalid tor auth type '{}'", s)),
        }
    }
}

#[derive(Debug, Clone)]
pub enum CommsTransport {
    /// Use TCP to join the Tari network. This transport can only communicate with TCP/IP addresses, so peers with
    /// e.g. tor onion addresses will not be contactable.
    Tcp {
        listener_address: Multiaddr,
        tor_socks_address: Option<Multiaddr>,
        tor_socks_auth: Option<SocksAuthentication>,
    },
    /// Configures the node to run over a tor hidden service using the Tor proxy. This transport recognises ip/tcp,
    /// onion v2, onion v3 and DNS addresses.
    TorHiddenService {
        /// The address of the control server
        control_server_address: Multiaddr,
        socks_address_override: Option<Multiaddr>,
        /// The address used to receive proxied traffic from the tor proxy to the Tari node. This port must be
        /// available
        forward_address: Multiaddr,
        auth: TorControlAuthentication,
        onion_port: NonZeroU16,
    },
    /// Use a SOCKS5 proxy transport. This transport recognises any addresses supported by the proxy.
    Socks5 {
        proxy_address: Multiaddr,
        auth: SocksAuthentication,
        listener_address: Multiaddr,
    },
}
