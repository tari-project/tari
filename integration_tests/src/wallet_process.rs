//   Copyright 2022. The Tari Project
//
//   Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//   following conditions are met:
//
//   1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//   disclaimer.
//
//   2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//   following disclaimer in the documentation and/or other materials provided with the distribution.
//
//   3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//   products derived from this software without specific prior written permission.
//
//   THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//   INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//   DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//   SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//   SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//   WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//   USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{path::PathBuf, str::FromStr, thread, time::Duration};

use minotari_app_grpc::tari_rpc::SetBaseNodeRequest;
use minotari_app_utilities::common_cli_args::CommonCliArgs;
use minotari_console_wallet::{run_wallet_with_cli, Cli};
use minotari_wallet::{transaction_service::config::TransactionRoutingMechanism, WalletConfig};
use minotari_wallet_grpc_client::WalletGrpcClient;
use tari_common::{
    configuration::{CommonConfig, MultiaddrList},
    network_check::set_network_if_choice_valid,
};
use tari_comms::multiaddr::Multiaddr;
use tari_comms_dht::{DbConnectionUrl, DhtConfig};
use tari_p2p::{auto_update::AutoUpdateConfig, Network, PeerSeedsConfig, TransportType};
use tari_shutdown::Shutdown;
use tokio::runtime;
use tonic::transport::Channel;

use crate::{get_peer_addresses, get_port, wait_for_service, TariWorld};

#[derive(Clone, Debug)]
pub struct WalletProcess {
    pub config: WalletConfig,
    pub grpc_port: u64,
    pub kill_signal: Shutdown,
    pub name: String,
    pub port: u64,
    pub temp_dir_path: PathBuf,
}

impl Drop for WalletProcess {
    fn drop(&mut self) {
        self.kill();
    }
}

#[allow(clippy::too_many_lines)]
pub async fn spawn_wallet(
    world: &mut TariWorld,
    wallet_name: String,
    base_node_name: Option<String>,
    peer_seeds: Vec<String>,
    routing_mechanism: Option<TransactionRoutingMechanism>,
    cli: Option<Cli>,
) {
    std::env::set_var("TARI_NETWORK", "localnet");
    set_network_if_choice_valid(Network::LocalNet).unwrap();

    let port: u64;
    let grpc_port: u64;
    let temp_dir_path: PathBuf;
    let wallet_config: WalletConfig;

    if let Some(wallet_ps) = world.wallets.get(&wallet_name) {
        port = wallet_ps.port;
        grpc_port = wallet_ps.grpc_port;
        temp_dir_path = wallet_ps.temp_dir_path.clone();
        wallet_config = wallet_ps.config.clone();
    } else {
        // each spawned wallet will use different ports
        port = get_port(18000..18499).unwrap();
        grpc_port = get_port(18500..18999).unwrap();

        temp_dir_path = world
            .current_base_dir
            .as_ref()
            .expect("Base dir on world")
            .join("wallets")
            .join(format!("{}_grpc_port_{}", wallet_name.clone(), grpc_port));

        wallet_config = WalletConfig::default();
    };

    let base_node = base_node_name.map(|name| {
        let pubkey = world.base_nodes.get(&name).unwrap().identity.public_key().clone();
        let port = world.base_nodes.get(&name).unwrap().port;
        let set_base_node_request = SetBaseNodeRequest {
            net_address: format! {"/ip4/127.0.0.1/tcp/{}", port},
            public_key_hex: pubkey.to_string(),
        };

        (pubkey, port, set_base_node_request)
    });

    let peer_addresses = get_peer_addresses(world, &peer_seeds).await;

    let base_node_cloned = base_node.clone();
    let shutdown = Shutdown::new();
    let mut send_to_thread_shutdown = shutdown.clone();

    let temp_dir = temp_dir_path.clone();

    let mut common_config = CommonConfig::default();
    common_config.base_path = temp_dir_path.clone();
    let wallet_cfg = wallet_config.clone();
    thread::spawn(move || {
        let mut wallet_app_config = minotari_console_wallet::ApplicationConfig {
            common: common_config,
            auto_update: AutoUpdateConfig::default(),
            wallet: wallet_cfg,
            peer_seeds: PeerSeedsConfig {
                peer_seeds: peer_addresses.into(),
                ..Default::default()
            },
        };

        eprintln!("Using wallet temp_dir: {}", temp_dir_path.clone().display());

        wallet_app_config.wallet.identity_file = Some(temp_dir_path.clone().join("wallet_id.json"));
        wallet_app_config.wallet.network = Network::LocalNet;
        wallet_app_config.wallet.password = Some("test".into());
        wallet_app_config.wallet.grpc_enabled = true;
        wallet_app_config.wallet.grpc_address =
            Some(Multiaddr::from_str(&format!("/ip4/127.0.0.1/tcp/{}", grpc_port)).unwrap());
        wallet_app_config.wallet.db_file = PathBuf::from("console_wallet.db");
        wallet_app_config.wallet.contacts_auto_ping_interval = Duration::from_secs(2);
        wallet_app_config
            .wallet
            .base_node_service_config
            .base_node_monitor_max_refresh_interval = Duration::from_secs(15);
        wallet_app_config.wallet.p2p.transport.transport_type = TransportType::Tcp;
        wallet_app_config.wallet.p2p.transport.tcp.listener_address =
            Multiaddr::from_str(&format!("/ip4/127.0.0.1/tcp/{}", port)).unwrap();
        wallet_app_config.wallet.p2p.public_addresses = MultiaddrList::from(vec![wallet_app_config
            .wallet
            .p2p
            .transport
            .tcp
            .listener_address
            .clone()]);
        wallet_app_config.wallet.p2p.dht = DhtConfig::default_local_test();
        wallet_app_config.wallet.p2p.dht.database_url = DbConnectionUrl::file(format!("{}-dht.sqlite", port));
        wallet_app_config.wallet.p2p.allow_test_addresses = true;
        if let Some(mech) = routing_mechanism {
            wallet_app_config
                .wallet
                .transaction_service_config
                .transaction_routing_mechanism = mech;
        }

        // FIXME: wallet doesn't pick up the custom base node for some reason atm
        wallet_app_config.wallet.custom_base_node =
            base_node_cloned.map(|(pubkey, port, _)| format!("{}::/ip4/127.0.0.1/tcp/{}", pubkey, port));

        wallet_app_config.wallet.set_base_path(temp_dir_path.clone());

        let rt = runtime::Builder::new_multi_thread().enable_all().build().unwrap();

        let mut cli = cli.unwrap_or_else(get_default_cli);
        // We expect only file_name to be passed from cucumber.rs, now we put it in the right directory.
        if let Some(file_name) = cli.seed_words_file_name {
            cli.seed_words_file_name = Some(temp_dir_path.join(file_name));
        }

        if let Err(e) = run_wallet_with_cli(&mut send_to_thread_shutdown, rt, &mut wallet_app_config, cli) {
            panic!("{:?}", e);
        }
    });

    // make the new wallet able to be referenced by other processes
    world.wallets.insert(wallet_name.clone(), WalletProcess {
        config: wallet_config,
        name: wallet_name.clone(),
        port,
        grpc_port,
        temp_dir_path: temp_dir,
        kill_signal: shutdown,
    });

    tokio::time::sleep(Duration::from_secs(5)).await;

    wait_for_service(port).await;
    wait_for_service(grpc_port).await;

    if let Some((_, _, hacky_request)) = base_node {
        let mut wallet_client = create_wallet_client(world, wallet_name.clone())
            .await
            .expect("wallet grpc client");

        let _resp = wallet_client.set_base_node(hacky_request).await.unwrap();
    }

    tokio::time::sleep(Duration::from_secs(2)).await;
}

pub fn get_default_cli() -> Cli {
    Cli {
        // CommonCliArgs are ignored in test, it's used only to override the config in the main.rs of the wallet.
        common: CommonCliArgs {
            base_path: Default::default(),
            config: Default::default(),
            log_config: None,
            log_level: None,
            network: None,
            config_property_overrides: vec![],
        },
        password: None,
        change_password: false,
        recovery: false,
        seed_words: None,
        seed_words_file_name: None,
        non_interactive_mode: true,
        input_file: None,
        command: None,
        wallet_notify: None,
        command_mode_auto_exit: false,
        grpc_enabled: true,
        grpc_address: None,
        command2: None,
        profile_with_tokio_console: false,
        view_private_key: None,
        spend_key: None,
    }
}

pub async fn create_wallet_client(world: &TariWorld, wallet_name: String) -> anyhow::Result<WalletGrpcClient<Channel>> {
    let wallet_grpc_port = world.wallets.get(&wallet_name).unwrap().grpc_port;
    let wallet_addr = format!("http://127.0.0.1:{}", wallet_grpc_port);

    eprintln!("Wallet GRPC at {}", wallet_addr);

    Ok(WalletGrpcClient::connect(wallet_addr.as_str()).await?)
}

impl WalletProcess {
    #[allow(dead_code)]
    pub async fn get_grpc_client(&self) -> anyhow::Result<WalletGrpcClient<Channel>> {
        let wallet_addr = format!("http://127.0.0.1:{}", self.grpc_port);
        Ok(WalletGrpcClient::connect(wallet_addr.as_str()).await?)
    }

    pub fn kill(&mut self) {
        self.kill_signal.trigger();
    }
}
