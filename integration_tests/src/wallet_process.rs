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

// Overflow in test code panics, which is the desired failure mode for a test.
#![allow(clippy::arithmetic_side_effects)]
use std::{path::PathBuf, str::FromStr, thread, time::Duration};

use minotari_app_utilities::common_cli_args::CommonCliArgs;
use minotari_console_wallet::{Cli, run_wallet_with_cli};
use minotari_wallet::{WalletConfig, transaction_service::config::TransactionRoutingMechanism};
use minotari_wallet_grpc_client::WalletGrpcClient;
use tari_common::{configuration::CommonConfig, network_check::set_network_if_choice_valid};
use tari_common_types::tari_address::TariAddress;
use tari_comms::multiaddr::Multiaddr;
use tari_p2p::{Network, PeerSeedsConfig, auto_update::AutoUpdateConfig};
use tari_shutdown::Shutdown;
use tokio::{runtime, sync::watch};
use tonic::transport::Channel;

use crate::{TariWorld, get_peer_addresses, wait_for_service};

#[derive(Clone, Debug)]
pub struct WalletProcess {
    pub config: WalletConfig,
    pub grpc_port: u16,
    pub kill_signal: Shutdown,
    pub name: String,
    pub temp_dir_path: PathBuf,
    pub base_node_name: Option<String>,
    pub peer_seeds: Vec<String>,
    pub http_port: u16,
    is_running: bool,
    /// Flips to `true` once the wallet's runtime thread has fully exited — i.e. its tokio runtime
    /// (and therefore its SQLite pool and comms stack) has been dropped. `kill()` waits on this so
    /// a respawn of the same wallet does not race the previous incarnation's teardown.
    finished: watch::Receiver<bool>,
}

impl Drop for WalletProcess {
    fn drop(&mut self) {
        // Signal only — see the note on `BaseNodeProcess::drop`. Blocking here would occupy a
        // cucumber runtime worker thread. Callers needing the gRPC port back use `kill().await`.
        self.kill_signal.trigger();
        self.is_running = false;
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
    set_network_if_choice_valid(Network::LocalNet).unwrap();

    let grpc_port: u16;
    let temp_dir_path: PathBuf;
    let mut wallet_config: WalletConfig;

    if let Some(wallet_ps) = world.wallets.get(&wallet_name) {
        if wallet_ps.is_running() {
            panic!("Wallet {wallet_name} is already running");
        }
        grpc_port = wallet_ps.grpc_port;
        temp_dir_path = wallet_ps.temp_dir_path.clone();
        wallet_config = wallet_ps.config.clone();
    } else {
        // Allocate port from the global pool (pre-scanned at startup)
        let wallet_ports = crate::port_pool::global_port_pool()
            .allocate_wallet_ports()
            .expect("Port pool exhausted — too many concurrent wallets");
        grpc_port = wallet_ports.grpc;
        // NOTE: the wallet does not own an HTTP port — `http_port` below is the *base node's*
        // HTTP query service that this wallet is configured to talk to.
        world.assigned_ports.insert(grpc_port, grpc_port);

        temp_dir_path = world
            .current_base_dir
            .as_ref()
            .expect("Base dir on world")
            .join("wallets")
            .join(format!("{}_grpc_port_{}", wallet_name.clone(), grpc_port));

        wallet_config = WalletConfig::default();
    };
    wallet_config.scanning_interval = 1; // set scanning interval to 1 second for faster tests
    let peer_addresses = get_peer_addresses(world, &peer_seeds).await;

    let shutdown = Shutdown::new();
    let mut send_to_thread_shutdown = shutdown.clone();

    let temp_dir = temp_dir_path.clone();
    let http_port = world
        .base_nodes
        .get(base_node_name.as_ref().unwrap())
        .unwrap()
        .http_port;

    let mut common_config = CommonConfig::default();
    common_config.base_path = temp_dir_path.clone();
    let wallet_cfg = wallet_config.clone();
    // Signalled once the wallet's runtime thread returns (runtime dropped → DB/comms released).
    let (finished_tx, finished_rx) = watch::channel(false);
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

        wallet_app_config.wallet.network = Network::LocalNet;
        wallet_app_config.wallet.password = Some("test".into());
        wallet_app_config.wallet.grpc_enabled = true;
        wallet_app_config.wallet.grpc_address =
            Some(Multiaddr::from_str(&format!("/ip4/127.0.0.1/tcp/{grpc_port}")).unwrap());
        wallet_app_config.wallet.db_file = PathBuf::from("console_wallet.db");
        wallet_app_config.wallet.http_server_url = format!("http://127.0.0.1:{http_port}");
        wallet_app_config.wallet.fallback_http_server_url = format!("http://127.0.0.1:{http_port}");
        // Tune transaction service timing for faster test execution
        let tx_cfg = &mut wallet_app_config.wallet.transaction_service_config;
        tx_cfg.broadcast_monitoring_timeout = Duration::from_secs(5); // prod: 30s
        tx_cfg.chain_monitoring_timeout = Duration::from_secs(10); // prod: 60s
        tx_cfg.direct_send_timeout = Duration::from_secs(5); // prod: 20s
        tx_cfg.broadcast_send_timeout = Duration::from_secs(10); // prod: 60s
        tx_cfg.transaction_resend_period = Duration::from_secs(30); // prod: 600s
        tx_cfg.resend_response_cooldown = Duration::from_secs(10); // prod: 300s
        tx_cfg.transaction_mempool_resubmission_window = Duration::from_secs(30); // prod: 600s

        if let Some(mech) = routing_mechanism {
            wallet_app_config
                .wallet
                .transaction_service_config
                .transaction_routing_mechanism = mech;
        }

        // Tune wallet base node monitoring for faster chain detection
        wallet_app_config
            .wallet
            .base_node_service_config
            .base_node_monitor_max_refresh_interval = Duration::from_secs(5); // prod: 30s

        // Tune balance/broadcast responsiveness
        wallet_app_config.wallet.balance_enquiry_cooldown_period = Duration::from_secs(1); // prod: 5s
        wallet_app_config.wallet.grpc_broadcast_confirmation = 1000; // 1s, prod: 5s

        wallet_app_config.wallet.set_base_path(temp_dir_path.clone());

        // Cap the worker count. The default is `available_parallelism()`, and with several
        // scenarios in flight there are ~10 of these runtimes alive at once on top of the main
        // cucumber runtime and every base node task — on a high-core CI runner that is hundreds of
        // 4 MB-stack threads competing for the same cores, which shows up as scheduling jitter and
        // timeout flakes. A full console wallet still runs comms + tx/output services + gRPC
        // concurrently, so 4 (not 2) is the floor that keeps it from starving under that load.
        let rt = runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .thread_stack_size(4 * 1024 * 1024)// 4 MB stack size per thread (4 * 1024 * 1024 = 4,194,304 bytes)
            .enable_all()
            .build()
            .unwrap();

        let mut cli = cli.unwrap_or_else(get_default_cli);
        // We expect only file_name to be passed from cucumber.rs, now we put it in the right directory.
        if let Some(file_name) = cli.seed_words_file_name {
            cli.seed_words_file_name = Some(temp_dir_path.join(file_name));
        }

        let run_result = run_wallet_with_cli(&mut send_to_thread_shutdown, rt, &mut wallet_app_config, cli);
        // `run_wallet_with_cli` owns the runtime, so by the time it returns the runtime (and with it
        // the SQLite pool and comms) has been dropped and the DB file is released. Signal before
        // surfacing any error so `kill()` is always released.
        let _ = finished_tx.send(true);
        if let Err(e) = run_result {
            panic!("{e:?}");
        }
    });

    wait_for_service(grpc_port).await;

    let wallet_addr = format!("http://127.0.0.1:{grpc_port}");
    let tari_address = {
        let mut wallet_client = WalletGrpcClient::connect(wallet_addr.as_str()).await.unwrap();
        let wallet_address_bytes = wallet_client
            .get_address(minotari_wallet_grpc_client::grpc::Empty {})
            .await
            .unwrap()
            .into_inner()
            .interactive_address;
        TariAddress::from_bytes(&wallet_address_bytes).unwrap()
    }; // wallet_client is automatically dropped here
    world
        .wallet_addresses
        .insert(wallet_name.clone(), tari_address.to_base58());

    // make the new wallet able to be referenced by other processes
    world.wallets.insert(wallet_name.clone(), WalletProcess {
        config: wallet_config,
        name: wallet_name.clone(),
        grpc_port,
        temp_dir_path: temp_dir,
        base_node_name,
        kill_signal: shutdown,
        peer_seeds,
        is_running: true,
        http_port,
        finished: finished_rx,
    });
}

pub fn get_default_cli() -> Cli {
    Cli {
        // CommonCliArgs are ignored in test, it's used only to override the config in the main.rs of the wallet.
        common: CommonCliArgs {
            base_path: Default::default(),
            config: Default::default(),
            log_config: None,
            log_path: None,
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
        birthday: None,
        spend_key: None,
        burn_proof_out: None,
        libtor_data_dir: None,
        skip_recovery: false,
        print_env: false,
    }
}

pub async fn create_wallet_client(world: &TariWorld, wallet_name: String) -> anyhow::Result<WalletGrpcClient<Channel>> {
    let wallet_grpc_port = world
        .wallets
        .get(&wallet_name)
        .ok_or_else(|| anyhow::anyhow!("Wallet process '{wallet_name}' not found in world"))?
        .grpc_port;
    let wallet_addr = format!("http://127.0.0.1:{wallet_grpc_port}");

    eprintln!("Wallet GRPC at {wallet_addr}");

    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        WalletGrpcClient::connect(wallet_addr.as_str()),
    )
    .await
    .map_err(|_| anyhow::anyhow!("Timed out connecting to wallet '{wallet_name}' gRPC at {wallet_addr}"))?
    .map_err(|e| anyhow::anyhow!("Failed to connect to wallet '{wallet_name}' gRPC at {wallet_addr}: {e}"))
}

impl WalletProcess {
    #[allow(dead_code)]
    pub async fn get_grpc_client(&self) -> anyhow::Result<WalletGrpcClient<Channel>> {
        let wallet_addr = format!("http://127.0.0.1:{}", self.grpc_port);
        tokio::time::timeout(
            std::time::Duration::from_secs(10),
            WalletGrpcClient::connect(wallet_addr.as_str()),
        )
        .await
        .map_err(|_| anyhow::anyhow!("Timed out connecting to wallet '{}' gRPC", self.name))?
        .map_err(|e| anyhow::anyhow!("Failed to connect to wallet '{}' gRPC: {e}", self.name))
    }

    pub fn is_running(&self) -> bool {
        self.is_running
    }

    /// Shut the wallet down and wait for its runtime thread to fully exit before returning.
    ///
    /// Waiting only for the gRPC port to free (as this used to) is not enough: the gRPC server
    /// stops early in the shutdown sequence while the wallet's SQLite database and comms stack are
    /// still being torn down. The export / sign / import steps kill a wallet and immediately
    /// respawn it on the *same* temp dir and port; if the respawn starts before the previous
    /// incarnation released the database, it blocks opening the still-locked DB and never binds its
    /// gRPC — surfacing as a "service on port N to start" timeout. Blocking on full completion
    /// (runtime dropped → DB + comms released) makes those respawns safe.
    ///
    /// `async` with Tokio primitives rather than `std::thread::sleep`: this is called from step
    /// definitions and from the scenario teardown hook, both of which run on the shared cucumber
    /// runtime that every base node task is also scheduled on.
    pub async fn kill(&mut self) {
        self.kill_signal.trigger();
        self.is_running = false;

        let mut finished = self.finished.clone();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
        while !*finished.borrow() {
            match tokio::time::timeout_at(deadline, finished.changed()).await {
                Ok(Ok(())) => {},     // value changed — re-check the loop condition
                Ok(Err(_)) => return, // sender dropped: the thread has ended, so teardown is done
                Err(_) => {
                    eprintln!(
                        "WARNING: wallet '{}' (grpc port {}) did not finish shutting down within 30s",
                        self.name, self.grpc_port
                    );
                    return;
                },
            }
        }
    }
}
