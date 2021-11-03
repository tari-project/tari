// Copyright 2020. The Tari Project
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
use crate::{
    automation::{command_parser::parse_command, commands::command_runner},
    grpc::WalletGrpcServer,
    notifier::Notifier,
    recovery::wallet_recovery,
    ui,
    ui::App,
    utils::db::get_custom_base_node_peer_from_db,
};
use log::*;
use rand::{rngs::OsRng, seq::SliceRandom};
use std::{fs, io::Stdout, net::SocketAddr, path::PathBuf};
use tari_common::{exit_codes::ExitCodes, ConfigBootstrap, GlobalConfig};
use tari_comms::peer_manager::Peer;
use tari_wallet::WalletSqlite;
use tokio::runtime::Handle;
use tonic::transport::Server;
use tui::backend::CrosstermBackend;

pub const LOG_TARGET: &str = "wallet::app::main";

#[derive(Debug, Clone)]
pub enum WalletMode {
    Tui,
    Grpc,
    Script(PathBuf),
    Command(String),
    RecoveryDaemon,
    RecoveryTui,
    Invalid,
}

#[derive(Debug, Clone)]
pub struct WalletModeConfig {
    pub base_node_config: PeerConfig,
    pub base_node_selected: Peer,
    pub bootstrap: ConfigBootstrap,
    pub global_config: GlobalConfig,
    pub handle: Handle,
    pub notify_script: Option<PathBuf>,
    pub wallet_mode: WalletMode,
}

#[derive(Debug, Clone)]
pub struct PeerConfig {
    pub base_node_custom: Option<Peer>,
    pub base_node_peers: Vec<Peer>,
    pub peer_seeds: Vec<Peer>,
}

impl PeerConfig {
    /// Create a new PeerConfig
    pub fn new(base_node_custom: Option<Peer>, base_node_peers: Vec<Peer>, peer_seeds: Vec<Peer>) -> Self {
        Self {
            base_node_custom,
            base_node_peers,
            peer_seeds,
        }
    }

    /// Get the prioritised base node peer from the PeerConfig.
    /// 1. Custom Base Node
    /// 2. First configured Base Node Peer
    /// 3. Random configured Peer Seed
    pub fn get_base_node_peer(&self) -> Result<Peer, ExitCodes> {
        if let Some(base_node) = self.base_node_custom.clone() {
            Ok(base_node)
        } else if !self.base_node_peers.is_empty() {
            Ok(self
                .base_node_peers
                .first()
                .ok_or_else(|| ExitCodes::ConfigError("Configured base node peer has no address!".to_string()))?
                .clone())
        } else if !self.peer_seeds.is_empty() {
            // pick a random peer seed
            Ok(self
                .peer_seeds
                .choose(&mut OsRng)
                .ok_or_else(|| ExitCodes::ConfigError("Peer seeds was empty.".to_string()))?
                .clone())
        } else {
            Err(ExitCodes::ConfigError(
                "No peer seeds or base node peer defined in config!".to_string(),
            ))
        }
    }

    /// Returns all the peers from the PeerConfig.
    /// In order: Custom base node, service peers, peer seeds.
    pub fn get_all_peers(&self) -> Vec<Peer> {
        let num_peers = self.base_node_peers.len();
        let num_seeds = self.peer_seeds.len();

        let mut peers = if let Some(peer) = self.base_node_custom.clone() {
            let mut peers = Vec::with_capacity(1 + num_peers + num_seeds);
            peers.push(peer);
            peers
        } else {
            Vec::with_capacity(num_peers + num_seeds)
        };

        peers.extend(self.base_node_peers.clone());
        peers.extend(self.peer_seeds.clone());

        peers
    }
}

pub fn command_mode(config: WalletModeConfig, wallet: WalletSqlite, command: String) -> Result<(), ExitCodes> {
    let WalletModeConfig {
        global_config, handle, ..
    } = config.clone();
    let commands = vec![parse_command(&command)?];
    info!(target: LOG_TARGET, "Starting wallet command mode");
    handle.block_on(command_runner(commands, wallet.clone(), global_config))?;

    info!(target: LOG_TARGET, "Completed wallet command mode");

    wallet_or_exit(config, wallet)
}

pub fn script_mode(config: WalletModeConfig, wallet: WalletSqlite, path: PathBuf) -> Result<(), ExitCodes> {
    let WalletModeConfig {
        global_config, handle, ..
    } = config.clone();
    info!(target: LOG_TARGET, "Starting wallet script mode");
    println!("Starting wallet script mode");
    let script = fs::read_to_string(path).map_err(|e| ExitCodes::InputError(e.to_string()))?;

    if script.is_empty() {
        return Err(ExitCodes::InputError("Input file is empty!".to_string()));
    };

    let mut commands = Vec::new();

    println!("Parsing commands...");
    for command in script.lines() {
        // skip empty lines and 'comments' starting with #
        if !command.is_empty() && !command.starts_with('#') {
            // parse the command
            commands.push(parse_command(command)?);
        }
    }
    println!("{} commands parsed successfully.", commands.len());

    println!("Starting the command runner!");
    handle.block_on(command_runner(commands, wallet.clone(), global_config))?;

    info!(target: LOG_TARGET, "Completed wallet script mode");

    wallet_or_exit(config, wallet)
}

/// Prompts the user to continue to the wallet, or exit.
fn wallet_or_exit(config: WalletModeConfig, wallet: WalletSqlite) -> Result<(), ExitCodes> {
    if config.bootstrap.command_mode_auto_exit {
        info!(target: LOG_TARGET, "Auto exit argument supplied - exiting.");
        return Ok(());
    }

    if config.bootstrap.non_interactive_mode {
        info!(target: LOG_TARGET, "Starting GRPC server.");
        grpc_mode(config, wallet)
    } else {
        debug!(target: LOG_TARGET, "Prompting for run or exit key.");
        println!("\nPress Enter to continue to the wallet, or type q (or quit) followed by Enter.");
        let mut buf = String::new();
        std::io::stdin()
            .read_line(&mut buf)
            .map_err(|e| ExitCodes::IOError(e.to_string()))?;

        match buf.as_str().trim() {
            "quit" | "q" | "exit" => {
                info!(target: LOG_TARGET, "Exiting.");
                Ok(())
            },
            _ => {
                info!(target: LOG_TARGET, "Starting TUI.");
                tui_mode(config, wallet)
            },
        }
    }
}

pub fn tui_mode(config: WalletModeConfig, mut wallet: WalletSqlite) -> Result<(), ExitCodes> {
    let WalletModeConfig {
        mut base_node_config,
        mut base_node_selected,
        global_config,
        handle,
        notify_script,
        ..
    } = config;
    if let Some(grpc_address) = global_config
        .wallet_config
        .as_ref()
        .and_then(|c| c.grpc_address.as_ref())
    {
        let grpc = WalletGrpcServer::new(wallet.clone());
        handle.spawn(run_grpc(grpc, *grpc_address));
    }

    let notifier = Notifier::new(notify_script, handle.clone(), wallet.clone());

    // update the selected/custom base node since it may have been changed by script/command mode
    let base_node_custom = handle.block_on(get_custom_base_node_peer_from_db(&mut wallet));
    base_node_config.base_node_custom = base_node_custom.clone();
    if let Some(peer) = base_node_custom {
        base_node_selected = peer;
    } else if let Some(peer) = handle.block_on(wallet.get_base_node_peer()) {
        base_node_selected = peer;
    }

    let app = App::<CrosstermBackend<Stdout>>::new(
        "Tari Console Wallet".into(),
        wallet,
        global_config.network,
        base_node_selected,
        base_node_config,
        global_config,
        notifier,
    );

    info!(target: LOG_TARGET, "Starting app");

    {
        let _enter = handle.enter();
        ui::run(app)?;
    }

    info!(
        target: LOG_TARGET,
        "Termination signal received from user. Shutting wallet down."
    );

    Ok(())
}

pub fn recovery_mode(config: WalletModeConfig, wallet: WalletSqlite) -> Result<(), ExitCodes> {
    let WalletModeConfig {
        base_node_config,
        handle,
        wallet_mode,
        ..
    } = config.clone();
    println!("Starting recovery...");
    match handle.block_on(wallet_recovery(&wallet, &base_node_config)) {
        Ok(_) => println!("Wallet recovered!"),
        Err(e) => {
            error!(target: LOG_TARGET, "Recovery failed: {}", e);
            println!(
                "Recovery failed. Restarting the console wallet will restart the recovery process from where you left \
                 off. If you want to start with a fresh wallet then delete the wallet data file"
            );

            return Err(e);
        },
    }

    println!("Starting TUI.");

    match wallet_mode {
        WalletMode::RecoveryDaemon => grpc_mode(config, wallet),
        WalletMode::RecoveryTui => tui_mode(config, wallet),
        _ => Err(ExitCodes::RecoveryError("Unsupported post recovery mode".to_string())),
    }
}

pub fn grpc_mode(config: WalletModeConfig, wallet: WalletSqlite) -> Result<(), ExitCodes> {
    let WalletModeConfig {
        global_config, handle, ..
    } = config;
    println!("Starting grpc server");
    if let Some(grpc_address) = global_config.wallet_config.and_then(|c| c.grpc_address) {
        let grpc = WalletGrpcServer::new(wallet);
        handle
            .block_on(run_grpc(grpc, grpc_address))
            .map_err(ExitCodes::GrpcError)?;
    } else {
        println!("No grpc address specified");
    }
    println!("Shutting down");
    Ok(())
}

async fn run_grpc(grpc: WalletGrpcServer, grpc_console_wallet_address: SocketAddr) -> Result<(), String> {
    info!(target: LOG_TARGET, "Starting GRPC on {}", grpc_console_wallet_address);

    Server::builder()
        .add_service(tari_app_grpc::tari_rpc::wallet_server::WalletServer::new(grpc))
        .serve(grpc_console_wallet_address)
        .await
        .map_err(|e| format!("GRPC server returned error:{}", e))?;
    info!(target: LOG_TARGET, "Stopping GRPC");
    Ok(())
}
