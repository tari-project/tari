use std::{fs, fs::File, io::Write, path::Path};

use config::Config;
use log::{debug, info};
use multiaddr::{Multiaddr, Protocol};

use crate::{
    configuration::bootstrap::ApplicationType,
    dir_utils::default_subdir,
    ConfigBootstrap,
    ConfigError,
    LOG_TARGET,
};

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

/// Installs a new configuration file template, copied from the application type's preset and written to the given path.
/// Also includes the common configuration defined in `config/presets/common.toml`.
pub fn config_installer(_app_type: ApplicationType, path: &Path) -> Result<(), std::io::Error> {
    // Use the same config file so that all the settings are easier to find, and easier to
    // support users over chat channels
    let common = include_str!("../../config/presets/common.toml");
    let source = [
        common,
        include_str!("../../config/presets/base_node.toml"),
        include_str!("../../config/presets/console_wallet.toml"),
        include_str!("../../config/presets/mining_node.toml"),
        include_str!("../../config/presets/merge_mining_proxy.toml"),
        include_str!("../../config/presets/stratum_transcoder.toml"),
        include_str!("../../config/presets/validator_node.toml"),
        include_str!("../../config/presets/collectibles.toml"),
    ]
    .join("\n");

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

    // Common settings
    cfg.set_default("common.message_cache_size", 10).unwrap();
    cfg.set_default("common.message_cache_ttl", 1440).unwrap();
    cfg.set_default("common.peer_allowlist", Vec::<String>::new()).unwrap();
    cfg.set_default("common.rpc_max_simultaneous_sessions", 1000).unwrap();
    cfg.set_default("common.liveness_max_sessions", 0).unwrap();
    cfg.set_default("common.denylist_ban_period", 1440).unwrap();
    cfg.set_default("common.buffer_size_base_node", 1_500).unwrap();
    cfg.set_default("common.buffer_size_console_wallet", 50_000).unwrap();
    cfg.set_default("common.buffer_rate_limit_base_node", 1_000).unwrap();
    cfg.set_default("common.buffer_rate_limit_console_wallet", 1_000)
        .unwrap();
    cfg.set_default("common.dedup_cache_capacity", 2_500).unwrap();
    cfg.set_default("common.dht_minimum_desired_tcpv4_node_ratio", 0.0f64)
        .unwrap();
    cfg.set_default("common.fetch_blocks_timeout", 150).unwrap();
    cfg.set_default("common.fetch_utxos_timeout", 600).unwrap();
    cfg.set_default("common.service_request_timeout", 180).unwrap();

    // Wallet settings
    cfg.set_default("wallet.grpc_enabled", false).unwrap();
    cfg.set_default("wallet.grpc_address", "127.0.0.1:18043").unwrap();
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
    cfg.set_default("wallet.base_node_service_refresh_interval", 5).unwrap();
    cfg.set_default("wallet.base_node_service_request_max_age", 60).unwrap();
    cfg.set_default("wallet.balance_enquiry_cooldown_period", 1).unwrap();
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
    cfg.set_default("base_node.mainnet.grpc_enabled", false).unwrap();
    cfg.set_default("base_node.mainnet.allow_test_addresses", false)
        .unwrap();
    cfg.set_default("base_node.mainnet.grpc_base_node_address", "127.0.0.1:18142")
        .unwrap();
    cfg.set_default("wallet.grpc_address", "127.0.0.1:18143").unwrap();
    cfg.set_default("base_node.mainnet.flood_ban_max_msg_count", 100_000)
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
    cfg.set_default("base_node.weatherwax.flood_ban_max_msg_count", 100_000)
        .unwrap();
    cfg.set_default("base_node.weatherwax.peer_seeds", Vec::<String>::new())
        .unwrap();
    cfg.set_default(
        "base_node.weatherwax.data_dir",
        default_subdir("weatherwax/", Some(&bootstrap.base_path)),
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

    cfg.set_default("base_node.weatherwax.allow_test_addresses", false)
        .unwrap();
    cfg.set_default("base_node.weatherwax.grpc_enabled", false).unwrap();
    cfg.set_default("base_node.weatherwax.grpc_base_node_address", "127.0.0.1:18142")
        .unwrap();

    //---------------------------------- Igor Defaults --------------------------------------------//

    cfg.set_default("base_node.igor.db_type", "lmdb").unwrap();
    cfg.set_default("base_node.igor.orphan_storage_capacity", 720).unwrap();
    cfg.set_default("base_node.igor.orphan_db_clean_out_threshold", 0)
        .unwrap();
    cfg.set_default("base_node.igor.pruning_horizon", 0).unwrap();
    cfg.set_default("base_node.igor.pruned_mode_cleanup_interval", 50)
        .unwrap();
    cfg.set_default("base_node.igor.flood_ban_max_msg_count", 100_000)
        .unwrap();
    cfg.set_default("base_node.igor.grpc_enabled", false).unwrap();
    cfg.set_default("base_node.igor.grpc_base_node_address", "127.0.0.1:18142")
        .unwrap();

    set_common_network_defaults(&mut cfg);
    set_transport_defaults(&mut cfg).unwrap();
    set_merge_mining_defaults(&mut cfg);
    set_mining_node_defaults(&mut cfg);
    set_stratum_transcoder_defaults(&mut cfg);

    cfg
}

fn set_common_network_defaults(cfg: &mut Config) {
    for network in ["mainnet", "dibbler", "igor", "localnet"] {
        let key = format!("base_node.{}.dns_seeds_name_server", network);
        cfg.set_default(&key, "1.1.1.1:853/cloudflare-dns.com").unwrap();

        let key = format!("base_node.{}.dns_seeds_use_dnssec", network);
        cfg.set_default(&key, true).unwrap();

        let key = format!("base_node.{}.metadata_auto_ping_interval", network);
        cfg.set_default(&key, 30).unwrap();

        let key = format!("wallet.{}.contacts_auto_ping_interval", network);
        cfg.set_default(&key, 20).unwrap();

        let key = format!("wallet.{}.contacts_online_ping_window", network);
        cfg.set_default(&key, 2).unwrap();

        let key = format!("common.{}.peer_seeds", network);
        cfg.set_default(&key, Vec::<String>::new()).unwrap();

        let key = format!("common.{}.dns_seeds", network);
        cfg.set_default(&key, Vec::<String>::new()).unwrap();

        let key = format!("common.{}.dns_seeds_name_server", network);
        cfg.set_default(&key, "1.1.1.1:853/cloudflare-dns.com").unwrap();

        let key = format!("common.{}.dns_seeds_use_dnssec", network);
        cfg.set_default(&key, true).unwrap();

        let key = format!("common.{}.auto_update.dns_hosts", network);
        cfg.set_default(&key, vec!["versions.tari.com"]).unwrap();

        let key = format!("common.{}.auto_update.hashes_url", network);
        cfg.set_default(
            &key,
            "https://raw.githubusercontent.com/tari-project/tari/development/meta/hashes.txt",
        )
        .unwrap();

        let key = format!("common.{}.auto_update.hashes_sig_url", network);
        cfg.set_default(
            &key,
            "https://raw.githubusercontent.com/tari-project/tari/development/meta/hashes.txt.sig",
        )
        .unwrap();
    }
}

fn set_stratum_transcoder_defaults(cfg: &mut Config) {
    cfg.set_default("stratum_transcoder.mainnet.transcoder_host_address", "127.0.0.1:7879")
        .unwrap();
    cfg.set_default(
        "stratum_transcoder.weatherwax.transcoder_host_address",
        "127.0.0.1:7879",
    )
    .unwrap();
    cfg.set_default("stratum_transcoder.igor.transcoder_host_address", "127.0.0.1:7879")
        .unwrap();
    cfg.set_default("stratum_transcoder.dibbler.transcoder_host_address", "127.0.0.1:7879")
        .unwrap();
}

fn set_merge_mining_defaults(cfg: &mut Config) {
    //---------------------------------- common defaults --------------------------------------------//
    cfg.set_default("merge_mining_proxy.proxy_host_address", "/ip4/127.0.0.1/tcp/7878")
        .unwrap();
    cfg.set_default("merge_mining_proxy.base_node_grpc_address", "/ip4/127.0.0.1/tcp/18142")
        .unwrap();
    cfg.set_default("merge_mining_proxy.wallet_grpc_address", "/ip4/127.0.0.1/tcp/18143")
        .unwrap();
    cfg.set_default("merge_mining_proxy.proxy_submit_to_origin", true)
        .unwrap();
    cfg.set_default("merge_mining_proxy.wait_for_initial_sync_at_startup", true)
        .unwrap();

    //---------------------------------- mainnet defaults --------------------------------------------//
    cfg.set_default("merge_mining_proxy.mainnet.monerod_url", "http://xmr.support:18081")
        .unwrap();
    cfg.set_default("merge_mining_proxy.mainnet.monerod_use_auth", false)
        .unwrap();
    cfg.set_default("merge_mining_proxy.mainnet.monerod_username", "")
        .unwrap();
    cfg.set_default("merge_mining_proxy.mainnet.monerod_password", "")
        .unwrap();

    //---------------------------------- igor defaults --------------------------------------------//
    cfg.set_default(
        "merge_mining_proxy.igor.monerod_url",
        "http://monero-stagenet.exan.tech:38081",
    )
    .unwrap();
    cfg.set_default("merge_mining_proxy.igor.monerod_use_auth", false)
        .unwrap();
    cfg.set_default("merge_mining_proxy.igor.monerod_username", "").unwrap();
    cfg.set_default("merge_mining_proxy.igor.monerod_password", "").unwrap();

    //---------------------------------- dibbler defaults --------------------------------------------//
    cfg.set_default(
        "merge_mining_proxy.dibbler.monerod_url",
        "http://monero-stagenet.exan.tech:38081",
    )
    .unwrap();
    cfg.set_default("merge_mining_proxy.dibbler.monerod_use_auth", false)
        .unwrap();
    cfg.set_default("merge_mining_proxy.dibbler.monerod_username", "")
        .unwrap();
    cfg.set_default("merge_mining_proxy.dibbler.monerod_password", "")
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

        // igor
        cfg.set_default(&format!("{}.igor.transport", app), "tor")?;

        cfg.set_default(&format!("{}.igor.tor_control_address", app), "/ip4/127.0.0.1/tcp/9051")?;
        cfg.set_default(&format!("{}.igor.tor_control_auth", app), "none")?;
        cfg.set_default(&format!("{}.igor.tor_forward_address", app), "/ip4/127.0.0.1/tcp/0")?;
        cfg.set_default(&format!("{}.igor.tor_onion_port", app), "18141")?;

        cfg.set_default(&format!("{}.igor.socks5_proxy_address", app), "/ip4/0.0.0.0/tcp/9150")?;

        cfg.set_default(&format!("{}.igor.socks5_auth", app), "none")?;

        // dibbler
        cfg.set_default(&format!("{}.dibbler.transport", app), "tor")?;

        cfg.set_default(
            &format!("{}.dibbler.tor_control_address", app),
            "/ip4/127.0.0.1/tcp/9051",
        )?;
        cfg.set_default(&format!("{}.dibbler.tor_control_auth", app), "none")?;
        cfg.set_default(&format!("{}.dibbler.tor_forward_address", app), "/ip4/127.0.0.1/tcp/0")?;
        cfg.set_default(&format!("{}.dibbler.tor_onion_port", app), "18141")?;

        cfg.set_default(
            &format!("{}.dibbler.socks5_proxy_address", app),
            "/ip4/0.0.0.0/tcp/9150",
        )?;

        cfg.set_default(&format!("{}.dibbler.socks5_auth", app), "none")?;
    }
    Ok(())
}

pub fn get_local_ip() -> Option<Multiaddr> {
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
