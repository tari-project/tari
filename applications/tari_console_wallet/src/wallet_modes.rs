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
};
use log::*;
use rand::{rngs::OsRng, seq::SliceRandom};
use std::{fs, io::Stdout, net::SocketAddr, path::PathBuf};
use tari_app_utilities::utilities::ExitCodes;
use tari_common::GlobalConfig;
use tari_comms::{peer_manager::Peer, types::CommsPublicKey};
use tari_wallet::WalletSqlite;
use tokio::runtime::Handle;
use tonic::transport::Server;
use tui::backend::CrosstermBackend;

pub const LOG_TARGET: &str = "wallet::app::main";

#[derive(Debug)]
pub enum WalletMode {
    Tui,
    Grpc,
    Script(PathBuf),
    Command(String),
    Recovery,
    Invalid,
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
}

pub fn command_mode(
    handle: Handle,
    command: String,
    wallet: WalletSqlite,
    config: GlobalConfig,
) -> Result<(), ExitCodes>
{
    let commands = vec![parse_command(&command)?];
    info!("Starting wallet command mode");
    handle.block_on(command_runner(handle.clone(), commands, wallet, config))?;
    info!("Shutting down wallet command mode");

    Ok(())
}

pub fn script_mode(handle: Handle, path: PathBuf, wallet: WalletSqlite, config: GlobalConfig) -> Result<(), ExitCodes> {
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
    handle.block_on(command_runner(handle.clone(), commands, wallet, config))?;

    info!(target: LOG_TARGET, "Completed wallet script mode");
    Ok(())
}

pub fn tui_mode(
    handle: Handle,
    node_config: GlobalConfig,
    wallet: WalletSqlite,
    base_node_selected: Peer,
    base_node_config: PeerConfig,
    notify_script: Option<PathBuf>,
) -> Result<(), ExitCodes>
{
    let grpc = WalletGrpcServer::new(wallet.clone());
    handle.spawn(run_grpc(grpc, node_config.grpc_console_wallet_address));

    let notifier = Notifier::new(notify_script, handle.clone(), wallet.clone());

    let app = App::<CrosstermBackend<Stdout>>::new(
        "Tari Console Wallet".into(),
        wallet,
        node_config.network,
        base_node_selected,
        base_node_config,
        node_config,
        notifier,
    );

    info!(target: LOG_TARGET, "Starting app");

    handle.enter(|| ui::run(app))?;

    info!(
        target: LOG_TARGET,
        "Termination signal received from user. Shutting wallet down."
    );

    Ok(())
}

pub fn recovery_mode(
    handle: Handle,
    config: GlobalConfig,
    wallet: WalletSqlite,
    base_node_selected: Peer,
    base_node_config: PeerConfig,
    notify_script: Option<PathBuf>,
) -> Result<(), ExitCodes>
{
    let peer_seed_public_keys: Vec<CommsPublicKey> = base_node_config
        .peer_seeds
        .iter()
        .map(|f| f.public_key.clone())
        .collect();
    println!("Starting recovery...");
    match handle.block_on(wallet_recovery(wallet.clone(), peer_seed_public_keys)) {
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
    tui_mode(
        handle,
        config,
        wallet,
        base_node_selected,
        base_node_config,
        notify_script,
    )
}

pub fn grpc_mode(handle: Handle, wallet: WalletSqlite, node_config: GlobalConfig) -> Result<(), ExitCodes> {
    println!("Starting grpc server");
    let grpc = WalletGrpcServer::new(wallet);
    handle
        .block_on(run_grpc(grpc, node_config.grpc_console_wallet_address))
        .map_err(ExitCodes::GrpcError)?;
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
