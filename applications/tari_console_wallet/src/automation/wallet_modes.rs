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
    ui::{run, App},
};

use log::*;
use std::{fs, io::Stdout, net::SocketAddr, path::PathBuf};
use tari_app_utilities::utilities::ExitCodes;
use tari_common::GlobalConfig;
use tari_comms::NodeIdentity;

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
    Invalid,
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
    node_identity: NodeIdentity,
    wallet: WalletSqlite,
) -> Result<(), ExitCodes>
{
    let grpc = WalletGrpcServer::new(wallet.clone());
    handle.spawn(run_grpc(grpc, node_config.grpc_wallet_address));

    let app = App::<CrosstermBackend<Stdout>>::new(
        "Tari Console Wallet".into(),
        &node_identity,
        wallet,
        node_config.network,
    );
    handle.enter(|| run(app))?;

    info!(
        target: LOG_TARGET,
        "Termination signal received from user. Shutting wallet down."
    );

    Ok(())
}

pub fn grpc_mode(handle: Handle, wallet: WalletSqlite, node_config: GlobalConfig) -> Result<(), ExitCodes> {
    println!("Starting grpc server");
    let grpc = WalletGrpcServer::new(wallet);
    handle
        .block_on(run_grpc(grpc, node_config.grpc_wallet_address))
        .map_err(ExitCodes::GrpcError)?;
    println!("Shutting down");
    Ok(())
}

async fn run_grpc(grpc: WalletGrpcServer, grpc_wallet_address: SocketAddr) -> Result<(), String> {
    info!(target: LOG_TARGET, "Starting GRPC on {}", grpc_wallet_address);

    Server::builder()
        .add_service(tari_app_grpc::tari_rpc::wallet_server::WalletServer::new(grpc))
        .serve(grpc_wallet_address)
        .await
        .map_err(|e| format!("GRPC server returned error:{}", e))?;
    info!(target: LOG_TARGET, "Stopping GRPC");
    Ok(())
}
