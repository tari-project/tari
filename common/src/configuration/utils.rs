use crate::{dir_utils::default_subdir, ConfigBootstrap, LOG_TARGET};
use config::Config;
use log::{debug, info};
use multiaddr::{Multiaddr, Protocol};
use std::{fs, fs::File, io::Write, path::Path};

//-------------------------------------           Main API functions         --------------------------------------//

pub fn load_configuration(bootstrap: &ConfigBootstrap) -> Result<Config, String> {
    debug!(
        target: LOG_TARGET,
        "Loading configuration file from  {}",
        bootstrap.config.to_str().unwrap_or("[??]")
    );
    let mut cfg = default_config(bootstrap);
    // Load the configuration file
    let filename = bootstrap
        .config
        .to_str()
        .ok_or_else(|| "Invalid config file path".to_string())?;
    let config_file = config::File::with_name(filename);
    match cfg.merge(config_file) {
        Ok(_) => {
            info!(target: LOG_TARGET, "Configuration file loaded.");
            Ok(cfg)
        },
        Err(e) => Err(format!(
            "There was an error loading the configuration file. {}",
            e.to_string()
        )),
    }
}

/// Installs a new configuration file template, copied from `tari_sample.toml` to the given path.
pub fn install_default_config_file(path: &Path) -> Result<(), std::io::Error> {
    let source = include_str!("../../config/presets/tari_sample.toml");
    if let Some(d) = path.parent() {
        fs::create_dir_all(d)?
    };
    let mut file = File::create(path)?;
    file.write_all(source.as_ref())
}

//-------------------------------------      Configuration file defaults      --------------------------------------//

/// Generate the global Tari configuration instance.
///
/// The `Config` object that is returned holds _all_ the default values possible in the `~/.tari/config.toml` file.
/// These will typically be overridden by userland settings in envars, the config file, or the command line.
pub fn default_config(bootstrap: &ConfigBootstrap) -> Config {
    let mut cfg = Config::new();
    let local_ip_addr = get_local_ip().unwrap_or_else(|| "/ip4/1.2.3.4".parse().unwrap());

    // Common settings
    cfg.set_default("common.message_cache_size", 10).unwrap();
    cfg.set_default("common.message_cache_ttl", 1440).unwrap();
    cfg.set_default("common.peer_allowlist", Vec::<String>::new()).unwrap();
    cfg.set_default("common.liveness_max_sessions", 0).unwrap();
    cfg.set_default(
        "common.peer_database ",
        default_subdir("peers", Some(&bootstrap.base_path)),
    )
    .unwrap();
    cfg.set_default("common.denylist_ban_period", 1440).unwrap();
    cfg.set_default("common.buffer_size_base_node", 1_500).unwrap();
    cfg.set_default("common.buffer_size_base_node_wallet", 50_000).unwrap();
    cfg.set_default("common.buffer_rate_limit_base_node", 1_000).unwrap();
    cfg.set_default("common.buffer_rate_limit_base_node_wallet", 1_000)
        .unwrap();
    cfg.set_default("common.fetch_blocks_timeout", 150).unwrap();
    cfg.set_default("common.fetch_utxos_timeout", 600).unwrap();
    cfg.set_default("common.service_request_timeout", 180).unwrap();

    // Wallet settings
    cfg.set_default("wallet.grpc_enabled", false).unwrap();
    cfg.set_default("wallet.grpc_address", "127.0.0.1:18040").unwrap();
    cfg.set_default(
        "wallet.wallet_db_file",
        default_subdir("wallet/wallet.dat", Some(&bootstrap.base_path)),
    )
    .unwrap();
    cfg.set_default(
        "wallet.console_wallet_db_file",
        default_subdir("wallet/console-wallet.dat", Some(&bootstrap.base_path)),
    )
    .unwrap();
    cfg.set_default("wallet.base_node_query_timeout", 60).unwrap();
    cfg.set_default("wallet.transaction_broadcast_monitoring_timeout", 60)
        .unwrap();
    cfg.set_default("wallet.transaction_chain_monitoring_timeout", 60)
        .unwrap();
    cfg.set_default("wallet.transaction_direct_send_timeout", 20).unwrap();
    cfg.set_default("wallet.transaction_broadcast_send_timeout", 60)
        .unwrap();
    cfg.set_default("wallet.prevent_fee_gt_amount", true).unwrap();
    cfg.set_default("wallet.transaction_routing_mechanism", "DirectAndStoreAndForward")
        .unwrap();
    cfg.set_default("wallet.command_send_wait_stage", "Broadcast").unwrap();
    cfg.set_default("wallet.command_send_wait_timeout", 300).unwrap();
    cfg.set_default("wallet.base_node_service_peers", Vec::<String>::new())
        .unwrap();

    //---------------------------------- Mainnet Defaults --------------------------------------------//

    cfg.set_default("base_node.network", "mainnet").unwrap();

    // Mainnet base node defaults
    cfg.set_default("base_node.mainnet.db_type", "lmdb").unwrap();
    cfg.set_default("base_node.mainnet.orphan_storage_capacity", 720)
        .unwrap();
    cfg.set_default("base_node.mainnet.orphan_db_clean_out_threshold", 0)
        .unwrap();
    cfg.set_default("base_node.mainnet.pruning_horizon", 0).unwrap();
    cfg.set_default("base_node.mainnet.pruned_mode_cleanup_interval", 50)
        .unwrap();
    cfg.set_default("base_node.mainnet.peer_seeds", Vec::<String>::new())
        .unwrap();
    cfg.set_default("base_node.mainnet.dns_seeds", Vec::<String>::new())
        .unwrap();
    cfg.set_default("base_node.mainnet.dns_seeds_name_server", "1.1.1.1:53")
        .unwrap();
    cfg.set_default("base_node.mainnet.dns_seeds_use_dnssec", true).unwrap();
    cfg.set_default(
        "base_node.mainnet.data_dir",
        default_subdir("mainnet/", Some(&bootstrap.base_path)),
    )
    .unwrap();
    cfg.set_default(
        "base_node.mainnet.base_node_identity_file",
        default_subdir("config/base_node_id.json", Some(&bootstrap.base_path)),
    )
    .unwrap();
    cfg.set_default(
        "base_node.mainnet.base_node_tor_identity_file",
        default_subdir("config/base_node_tor.json", Some(&bootstrap.base_path)),
    )
    .unwrap();
    cfg.set_default(
        "base_node.mainnet.wallet_identity_file",
        default_subdir("config/wallet_id.json", Some(&bootstrap.base_path)),
    )
    .unwrap();
    cfg.set_default(
        "base_node.mainnet.console_wallet_identity_file",
        default_subdir("config/console_wallet_id.json", Some(&bootstrap.base_path)),
    )
    .unwrap();
    cfg.set_default(
        "base_node.mainnet.wallet_tor_identity_file",
        default_subdir("config/wallet_tor.json", Some(&bootstrap.base_path)),
    )
    .unwrap();
    cfg.set_default(
        "base_node.mainnet.console_wallet_tor_identity_file",
        default_subdir("config/console_wallet_tor.json", Some(&bootstrap.base_path)),
    )
    .unwrap();
    cfg.set_default(
        "base_node.mainnet.public_address",
        format!("{}/tcp/18041", local_ip_addr),
    )
    .unwrap();
    cfg.set_default("base_node.mainnet.grpc_enabled", false).unwrap();
    cfg.set_default("base_node.mainnet.allow_test_addresses", false)
        .unwrap();
    cfg.set_default("base_node.mainnet.grpc_base_node_address", "127.0.0.1:18142")
        .unwrap();
    cfg.set_default("base_node.mainnet.grpc_console_wallet_address", "127.0.0.1:18143")
        .unwrap();
    cfg.set_default("base_node.mainnet.enable_mining", false).unwrap();
    cfg.set_default("base_node.mainnet.enable_wallet", true).unwrap();
    cfg.set_default("base_node.mainnet.num_mining_threads", 1).unwrap();
    cfg.set_default("base_node.mainnet.flood_ban_max_msg_count", 1000)
        .unwrap();

    //---------------------------------- Stibbons Defaults --------------------------------------------//

    cfg.set_default("base_node.stibbons.db_type", "lmdb").unwrap();
    cfg.set_default("base_node.stibbons.orphan_storage_capacity", 720)
        .unwrap();
    cfg.set_default("base_node.stibbons.orphan_db_clean_out_threshold", 0)
        .unwrap();
    cfg.set_default("base_node.stibbons.pruning_horizon", 0).unwrap();
    cfg.set_default("base_node.stibbons.pruned_mode_cleanup_interval", 50)
        .unwrap();
    cfg.set_default("base_node.stibbons.flood_ban_max_msg_count", 1000)
        .unwrap();
    cfg.set_default("base_node.stibbons.peer_seeds", Vec::<String>::new())
        .unwrap();
    cfg.set_default(
        "base_node.stibbons.data_dir",
        default_subdir("stibbons/", Some(&bootstrap.base_path)),
    )
    .unwrap();
    cfg.set_default(
        "base_node.stibbons.base_node_tor_identity_file",
        default_subdir("config/base_node_tor.json", Some(&bootstrap.base_path)),
    )
    .unwrap();
    cfg.set_default(
        "base_node.stibbons.wallet_identity_file",
        default_subdir("config/wallet_id.json", Some(&bootstrap.base_path)),
    )
    .unwrap();
    cfg.set_default(
        "base_node.stibbons.console_wallet_identity_file",
        default_subdir("config/console_wallet_id.json", Some(&bootstrap.base_path)),
    )
    .unwrap();
    cfg.set_default(
        "base_node.stibbons.wallet_tor_identity_file",
        default_subdir("config/wallet_tor.json", Some(&bootstrap.base_path)),
    )
    .unwrap();
    cfg.set_default(
        "base_node.stibbons.console_wallet_tor_identity_file",
        default_subdir("config/console_wallet_tor.json", Some(&bootstrap.base_path)),
    )
    .unwrap();
    cfg.set_default(
        "base_node.stibbons.base_node_identity_file",
        default_subdir("config/base_node_id.json", Some(&bootstrap.base_path)),
    )
    .unwrap();
    cfg.set_default(
        "base_node.stibbons.public_address",
        format!("{}/tcp/18141", local_ip_addr),
    )
    .unwrap();

    cfg.set_default("base_node.stibbons.allow_test_addresses", false)
        .unwrap();
    cfg.set_default("base_node.stibbons.grpc_enabled", false).unwrap();
    cfg.set_default("base_node.stibbons.grpc_base_node_address", "127.0.0.1:18142")
        .unwrap();
    cfg.set_default("base_node.stibbons.grpc_console_wallet_address", "127.0.0.1:18143")
        .unwrap();
    cfg.set_default("base_node.stibbons.enable_mining", false).unwrap();
    cfg.set_default("base_node.stibbons.enable_wallet", true).unwrap();
    cfg.set_default("base_node.stibbons.num_mining_threads", 1).unwrap();

    cfg.set_default("base_node.stibbons.dns_seeds_name_server", "1.1.1.1:53")
        .unwrap();
    cfg.set_default("base_node.stibbons.dns_seeds_use_dnssec", true)
        .unwrap();
    cfg.set_default("base_node.stibbons.auto_ping_interval", 30).unwrap();

    cfg.set_default("wallet.base_node_service_peers", Vec::<String>::new())
        .unwrap();

    set_transport_defaults(&mut cfg);
    set_merge_mining_defaults(&mut cfg);

    cfg
}

fn set_merge_mining_defaults(cfg: &mut Config) {
    cfg.set_default("merge_mining_proxy.mainnet.monerod_url", "http://18.133.55.120:38081")
        .unwrap();
    cfg.set_default("merge_mining_proxy.mainnet.proxy_host_address", "127.0.0.1:7878")
        .unwrap();
    cfg.set_default("merge_mining_proxy.mainnet.monerod_use_auth", "false")
        .unwrap();
    cfg.set_default("merge_mining_proxy.mainnet.monerod_username", "")
        .unwrap();
    cfg.set_default("merge_mining_proxy.mainnet.monerod_password", "")
        .unwrap();
    cfg.set_default("merge_mining_proxy.mainnet.wait_for_initial_sync_at_startup", true)
        .unwrap();

    cfg.set_default("merge_mining_proxy.stibbons.monerod_url", "http://18.133.55.120:38081")
        .unwrap();
    cfg.set_default("merge_mining_proxy.stibbons.proxy_host_address", "127.0.0.1:7878")
        .unwrap();
    cfg.set_default("merge_mining_proxy.stibbons.proxy_submit_to_origin", true)
        .unwrap();
    cfg.set_default("merge_mining_proxy.stibbons.monerod_use_auth", "false")
        .unwrap();
    cfg.set_default("merge_mining_proxy.stibbons.monerod_username", "")
        .unwrap();
    cfg.set_default("merge_mining_proxy.stibbons.monerod_password", "")
        .unwrap();
    cfg.set_default("merge_mining_proxy.stibbons.wait_for_initial_sync_at_startup", true)
        .unwrap();
}

fn set_transport_defaults(cfg: &mut Config) {
    // Mainnet
    // Default transport for mainnet is tcp
    cfg.set_default("base_node.mainnet.transport", "tcp").unwrap();
    cfg.set_default("base_node.mainnet.tcp_listener_address", "/ip4/0.0.0.0/tcp/18089")
        .unwrap();

    cfg.set_default("base_node.mainnet.tor_control_address", "/ip4/127.0.0.1/tcp/9051")
        .unwrap();
    cfg.set_default("base_node.mainnet.tor_control_auth", "none").unwrap();
    cfg.set_default("base_node.mainnet.tor_forward_address", "/ip4/127.0.0.1/tcp/0")
        .unwrap();
    cfg.set_default("base_node.mainnet.tor_onion_port", "18141").unwrap();

    cfg.set_default("base_node.mainnet.socks5_proxy_address", "/ip4/0.0.0.0/tcp/9050")
        .unwrap();
    cfg.set_default("base_node.mainnet.socks5_listener_address", "/ip4/0.0.0.0/tcp/18099")
        .unwrap();
    cfg.set_default("base_node.mainnet.socks5_auth", "none").unwrap();

    // stibbons
    // Default transport for stibbons is tcp
    cfg.set_default("base_node.stibbons.transport", "tcp").unwrap();
    cfg.set_default("base_node.stibbons.tcp_listener_address", "/ip4/0.0.0.0/tcp/18189")
        .unwrap();

    cfg.set_default("base_node.stibbons.tor_control_address", "/ip4/127.0.0.1/tcp/9051")
        .unwrap();
    cfg.set_default("base_node.stibbons.tor_control_auth", "none").unwrap();
    cfg.set_default("base_node.stibbons.tor_forward_address", "/ip4/127.0.0.1/tcp/0")
        .unwrap();
    cfg.set_default("base_node.stibbons.tor_onion_port", "18141").unwrap();

    cfg.set_default("base_node.stibbons.socks5_proxy_address", "/ip4/0.0.0.0/tcp/9150")
        .unwrap();
    cfg.set_default("base_node.stibbons.socks5_listener_address", "/ip4/0.0.0.0/tcp/18199")
        .unwrap();
    cfg.set_default("base_node.stibbons.socks5_auth", "none").unwrap();
}

fn get_local_ip() -> Option<Multiaddr> {
    use std::net::IpAddr;

    get_if_addrs::get_if_addrs().ok().and_then(|if_addrs| {
        if_addrs
            .into_iter()
            .find(|if_addr| !if_addr.is_loopback())
            .map(|if_addr| {
                let mut addr = Multiaddr::empty();
                match if_addr.ip() {
                    IpAddr::V4(ip) => {
                        addr.push(Protocol::Ip4(ip));
                    },
                    IpAddr::V6(ip) => {
                        addr.push(Protocol::Ip6(ip));
                    },
                }
                addr
            })
    })
}
