use crate::{
    configuration::bootstrap::ApplicationType,
    dir_utils::default_subdir,
    ConfigBootstrap,
    ConfigError,
    LOG_TARGET,
};
use config::Config;
use log::{debug, info};
use multiaddr::{Multiaddr, Protocol};
use std::{fs, fs::File, io::Write, path::Path};

//-------------------------------------           Main API functions         --------------------------------------//

pub fn load_configuration(bootstrap: &ConfigBootstrap) -> Result<Config, ConfigError> {
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
        .ok_or_else(|| ConfigError::new("Invalid config file path", None))?;
    let config_file = config::File::with_name(filename);

    cfg.merge(config_file)
        .map_err(|e| ConfigError::new("Failed to parse the configuration file", Some(e.to_string())))?;
    info!(target: LOG_TARGET, "Configuration file loaded.");

    Ok(cfg)
}

/// Installs a new configuration file template, copied from `tari_config_example.toml` to the given path.
pub fn install_default_config_file(path: &Path) -> Result<(), std::io::Error> {
    let source = include_str!("../../config/presets/tari_config_example.toml");
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
    cfg.set_default("common.rpc_max_simultaneous_sessions", 1000).unwrap();
    cfg.set_default("common.liveness_max_sessions", 0).unwrap();
    cfg.set_default("common.denylist_ban_period", 1440).unwrap();
    cfg.set_default("common.buffer_size_base_node", 1_500).unwrap();
    cfg.set_default("common.buffer_size_base_node_wallet", 50_000).unwrap();
    cfg.set_default("common.buffer_rate_limit_base_node", 1_000).unwrap();
    cfg.set_default("common.buffer_rate_limit_base_node_wallet", 1_000)
        .unwrap();
    cfg.set_default("common.dedup_cache_capacity", 2_500).unwrap();
    cfg.set_default("common.fetch_blocks_timeout", 150).unwrap();
    cfg.set_default("common.fetch_utxos_timeout", 600).unwrap();
    cfg.set_default("common.service_request_timeout", 180).unwrap();

    cfg.set_default("common.auto_update.dns_hosts", vec!["versions.tari.com"])
        .unwrap();
    // TODO: Change to a more permanent link
    cfg.set_default(
        "common.auto_update.hashes_url",
        "https://raw.githubusercontent.com/tari-project/tari/tari-script/meta/hashes.txt",
    )
    .unwrap();
    cfg.set_default(
        "common.auto_update.hashes_sig_url",
        "https://github.com/sdbondi/tari/raw/tari-script/meta/hashes.txt.sig",
    )
    .unwrap();

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
    // 60 sec * 60 minutes * 12 hours.
    cfg.set_default("wallet.scan_for_utxo_interval", 60 * 60 * 12).unwrap();
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

    cfg.set_default("common.network", "mainnet").unwrap();

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
        "base_node.mainnet.console_wallet_identity_file",
        default_subdir("config/console_wallet_id.json", Some(&bootstrap.base_path)),
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
    cfg.set_default("base_node.mainnet.enable_wallet", true).unwrap();
    cfg.set_default("base_node.mainnet.flood_ban_max_msg_count", 10000)
        .unwrap();

    //---------------------------------- Weatherwax Defaults --------------------------------------------//

    cfg.set_default("base_node.weatherwax.db_type", "lmdb").unwrap();
    cfg.set_default("base_node.weatherwax.orphan_storage_capacity", 720)
        .unwrap();
    cfg.set_default("base_node.weatherwax.orphan_db_clean_out_threshold", 0)
        .unwrap();
    cfg.set_default("base_node.weatherwax.pruning_horizon", 0).unwrap();
    cfg.set_default("base_node.weatherwax.pruned_mode_cleanup_interval", 50)
        .unwrap();
    cfg.set_default("base_node.weatherwax.flood_ban_max_msg_count", 1000)
        .unwrap();
    cfg.set_default("base_node.weatherwax.peer_seeds", Vec::<String>::new())
        .unwrap();
    cfg.set_default(
        "base_node.weatherwax.data_dir",
        default_subdir("stibbons/", Some(&bootstrap.base_path)),
    )
    .unwrap();
    cfg.set_default(
        "base_node.weatherwax.base_node_tor_identity_file",
        default_subdir("config/base_node_tor.json", Some(&bootstrap.base_path)),
    )
    .unwrap();
    cfg.set_default(
        "base_node.weatherwax.console_wallet_identity_file",
        default_subdir("config/console_wallet_id.json", Some(&bootstrap.base_path)),
    )
    .unwrap();
    cfg.set_default(
        "base_node.weatherwax.console_wallet_tor_identity_file",
        default_subdir("config/console_wallet_tor.json", Some(&bootstrap.base_path)),
    )
    .unwrap();
    cfg.set_default(
        "base_node.weatherwax.base_node_identity_file",
        default_subdir("config/base_node_id.json", Some(&bootstrap.base_path)),
    )
    .unwrap();
    cfg.set_default(
        "base_node.weatherwax.public_address",
        format!("{}/tcp/18141", local_ip_addr),
    )
    .unwrap();

    cfg.set_default("base_node.weatherwax.allow_test_addresses", false)
        .unwrap();
    cfg.set_default("base_node.weatherwax.grpc_enabled", false).unwrap();
    cfg.set_default("base_node.weatherwax.grpc_base_node_address", "127.0.0.1:18142")
        .unwrap();
    cfg.set_default("base_node.weatherwax.grpc_console_wallet_address", "127.0.0.1:18143")
        .unwrap();
    cfg.set_default("base_node.weatherwax.enable_wallet", true).unwrap();

    cfg.set_default("base_node.weatherwax.dns_seeds_name_server", "1.1.1.1:53")
        .unwrap();
    cfg.set_default("base_node.weatherwax.dns_seeds_use_dnssec", true)
        .unwrap();
    cfg.set_default("base_node.weatherwax.auto_ping_interval", 30).unwrap();

    cfg.set_default("wallet.base_node_service_peers", Vec::<String>::new())
        .unwrap();

    set_transport_defaults(&mut cfg).unwrap();
    set_merge_mining_defaults(&mut cfg);
    set_mining_node_defaults(&mut cfg);
    set_stratum_transcoder_defaults(&mut cfg);

    cfg
}

fn set_stratum_transcoder_defaults(cfg: &mut Config) {
    cfg.set_default("stratum_transcoder.mainnet.transcoder_host_address", "127.0.0.1:7879")
        .unwrap();
    cfg.set_default(
        "stratum_transcoder.weatherwax.transcoder_host_address",
        "127.0.0.1:7879",
    )
    .unwrap();
}

fn set_merge_mining_defaults(cfg: &mut Config) {
    cfg.set_default(
        "merge_mining_proxy.mainnet.monerod_url",
        "http://monero-stagenet.exan.tech:38081",
    )
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
    cfg.set_default(
        "merge_mining_proxy.weatherwax.monerod_url",
        "http://monero-stagenet.exan.tech:38081",
    )
    .unwrap();
    cfg.set_default("merge_mining_proxy.weatherwax.proxy_host_address", "127.0.0.1:7878")
        .unwrap();
    cfg.set_default("merge_mining_proxy.weatherwax.proxy_submit_to_origin", true)
        .unwrap();
    cfg.set_default("merge_mining_proxy.weatherwax.monerod_use_auth", "false")
        .unwrap();
    cfg.set_default("merge_mining_proxy.weatherwax.monerod_username", "")
        .unwrap();
    cfg.set_default("merge_mining_proxy.weatherwax.monerod_password", "")
        .unwrap();
    cfg.set_default("merge_mining_proxy.weatherwax.wait_for_initial_sync_at_startup", true)
        .unwrap();
}

fn set_mining_node_defaults(cfg: &mut Config) {
    cfg.set_default("mining_node.num_mining_threads", 1).unwrap();
    cfg.set_default("mining_node.mine_on_tip_only", true).unwrap();
    cfg.set_default("mining_node.validate_tip_timeout_sec", 0).unwrap();
}

fn set_transport_defaults(cfg: &mut Config) -> Result<(), config::ConfigError> {
    // Defaults that should not conflict across apps
    cfg.set_default(
        &format!("{}.mainnet.tcp_listener_address", ApplicationType::BaseNode),
        "/ip4/0.0.0.0/tcp/18089",
    )?;
    cfg.set_default(
        &format!("{}.mainnet.tcp_listener_address", ApplicationType::ConsoleWallet),
        "/ip4/0.0.0.0/tcp/18088",
    )?;
    cfg.set_default(
        &format!("{}.weatherwax.tcp_listener_address", ApplicationType::BaseNode),
        "/ip4/0.0.0.0/tcp/18199",
    )?;
    cfg.set_default(
        &format!("{}.weatherwax.tcp_listener_address", ApplicationType::ConsoleWallet),
        "/ip4/0.0.0.0/tcp/18198",
    )?;
    cfg.set_default(
        &format!("{}.mainnet.socks5_listener_address", ApplicationType::BaseNode),
        "/ip4/0.0.0.0/tcp/18099",
    )?;
    cfg.set_default(
        &format!("{}.mainnet.socks5_listener_address", ApplicationType::ConsoleWallet),
        "/ip4/0.0.0.0/tcp/18098",
    )?;
    cfg.set_default(
        &format!("{}.weatherwax.socks5_listener_address", ApplicationType::BaseNode),
        "/ip4/0.0.0.0/tcp/18199",
    )?;
    cfg.set_default(
        &format!("{}.weatherwax.socks5_listener_address", ApplicationType::ConsoleWallet),
        "/ip4/0.0.0.0/tcp/18198",
    )?;

    let apps = &[ApplicationType::BaseNode, ApplicationType::ConsoleWallet];
    for app in apps {
        let app = app.as_config_str();

        // Mainnet
        cfg.set_default(&format!("{}.mainnet.transport", app), "tor")?;
        cfg.set_default(
            &format!("{}.mainnet.tor_control_address", app),
            "/ip4/127.0.0.1/tcp/9051",
        )?;
        cfg.set_default(&format!("{}.mainnet.tor_control_auth", app), "none")?;
        cfg.set_default(&format!("{}.mainnet.tor_forward_address", app), "/ip4/127.0.0.1/tcp/0")?;
        cfg.set_default(&format!("{}.mainnet.tor_onion_port", app), "18141")?;

        cfg.set_default(
            &format!("{}.mainnet.socks5_proxy_address", app),
            "/ip4/0.0.0.0/tcp/9050",
        )?;
        cfg.set_default(&format!("{}.mainnet.socks5_auth", app), "none")?;

        // weatherwax
        cfg.set_default(&format!("{}.weatherwax.transport", app), "tor")?;

        cfg.set_default(
            &format!("{}.weatherwax.tor_control_address", app),
            "/ip4/127.0.0.1/tcp/9051",
        )?;
        cfg.set_default(&format!("{}.weatherwax.tor_control_auth", app), "none")?;
        cfg.set_default(
            &format!("{}.weatherwax.tor_forward_address", app),
            "/ip4/127.0.0.1/tcp/0",
        )?;
        cfg.set_default(&format!("{}.weatherwax.tor_onion_port", app), "18141")?;

        cfg.set_default(
            &format!("{}.weatherwax.socks5_proxy_address", app),
            "/ip4/0.0.0.0/tcp/9150",
        )?;

        cfg.set_default(&format!("{}.weatherwax.socks5_auth", app), "none")?;
    }
    Ok(())
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
