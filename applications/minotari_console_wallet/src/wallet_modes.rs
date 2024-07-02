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

#![allow(dead_code, unused)]

use std::{fs, io::Stdout, path::PathBuf};

use clap::Parser;
use log::*;
use minotari_app_grpc::{authentication::ServerAuthenticationInterceptor, tls::identity::read_identity};
use minotari_wallet::{WalletConfig, WalletSqlite};
use rand::{rngs::OsRng, seq::SliceRandom};
use tari_common::exit_codes::{ExitCode, ExitError};
use tari_common_types::grpc_authentication::GrpcAuthentication;
use tari_comms::{multiaddr::Multiaddr, peer_manager::Peer, utils::multiaddr::multiaddr_to_socketaddr};
use tokio::{runtime::Handle, sync::broadcast};
use tonic::transport::{Identity, Server, ServerTlsConfig};
use tui::backend::CrosstermBackend;

use crate::{
    automation::commands::command_runner,
    cli::{Cli, CliCommands},
    grpc::WalletGrpcServer,
    notifier::Notifier,
    recovery::wallet_recovery,
    ui,
    ui::App,
    utils::db::get_custom_base_node_peer_from_db,
};

pub const LOG_TARGET: &str = "wallet::app::main";

#[derive(Debug, Clone)]
pub enum WalletMode {
    Tui,
    Grpc,
    Script(PathBuf),
    Command(Box<CliCommands>),
    RecoveryDaemon,
    RecoveryTui,
    Invalid,
}

#[derive(Debug, Clone)]
pub struct ConsoleWalletConfig {
    pub base_node_config: PeerConfig,
    pub base_node_selected: Peer,
    pub notify_script: Option<PathBuf>,
    pub wallet_mode: WalletMode,
    pub grpc_address: Option<Multiaddr>,
    pub recovery_retry_limit: usize,
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
    pub fn get_base_node_peer(&self) -> Result<Peer, ExitError> {
        if let Some(base_node) = self.base_node_custom.clone() {
            Ok(base_node)
        } else if !self.base_node_peers.is_empty() {
            Ok(self
                .base_node_peers
                .first()
                .ok_or_else(|| ExitError::new(ExitCode::ConfigError, "Configured base node peer has no address!"))?
                .clone())
        } else if !self.peer_seeds.is_empty() {
            // pick a random peer seed
            Ok(self
                .peer_seeds
                .choose(&mut OsRng)
                .ok_or_else(|| ExitError::new(ExitCode::ConfigError, "Peer seeds was empty."))?
                .clone())
        } else {
            Err(ExitError::new(
                ExitCode::ConfigError,
                "No peer seeds or base node peer defined in config!",
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

pub(crate) fn command_mode(
    handle: Handle,
    cli: &Cli,
    config: &WalletConfig,
    base_node_config: &PeerConfig,
    wallet: WalletSqlite,
    command: CliCommands,
) -> Result<(), ExitError> {
    let commands = vec![command];

    // Do not remove this println!
    const CUCUMBER_TEST_MARKER_A: &str = "Minotari Console Wallet running... (Command mode started)";
    println!("{}", CUCUMBER_TEST_MARKER_A);

    info!(target: LOG_TARGET, "Starting wallet command mode");
    handle.block_on(command_runner(config, commands, wallet.clone()))?;

    // Do not remove this println!
    const CUCUMBER_TEST_MARKER_B: &str = "Minotari Console Wallet running... (Command mode completed)";
    println!("{}", CUCUMBER_TEST_MARKER_B);

    info!(target: LOG_TARGET, "Completed wallet command mode");

    wallet_or_exit(handle, cli, config, base_node_config, wallet)
}

pub(crate) fn parse_command_file(script: String) -> Result<Vec<CliCommands>, ExitError> {
    let mut commands: Vec<CliCommands> = Vec::new();
    let cli_parse_prefix = "minotari_console_wallet --command n/a".to_string();

    for command in script.lines() {
        // skip empty lines and 'comments' starting with #
        if !command.trim().is_empty() && !command.trim().starts_with('#') {
            let command_trimmed = cli_parse_prefix.to_owned() + " " + command.trim();
            let parse_vec: Vec<&str> = command_trimmed.split(' ').collect();
            let cli_parsed = Cli::try_parse_from(parse_vec);
            match cli_parsed {
                Ok(result) => {
                    if let Some(sub_command) = result.command2 {
                        commands.push(sub_command);
                    }
                },
                Err(e) => {
                    println!("\nError! parsing '{}' ({})\n", command, e);
                    return Err(ExitError::new(ExitCode::CommandError, e.to_string()));
                },
            }
        }
    }
    Ok(commands)
}

pub(crate) fn script_mode(
    handle: Handle,
    cli: &Cli,
    config: &WalletConfig,
    base_node_config: &PeerConfig,
    wallet: WalletSqlite,
    path: PathBuf,
) -> Result<(), ExitError> {
    info!(target: LOG_TARGET, "Starting wallet script mode");
    println!("Starting wallet script mode");
    let script = fs::read_to_string(path).map_err(|e| ExitError::new(ExitCode::InputError, e))?;

    if script.is_empty() {
        return Err(ExitError::new(ExitCode::InputError, "Input file is empty!"));
    };

    println!("Parsing commands...");
    let commands = parse_command_file(script)?;
    println!("{} commands parsed successfully.", commands.len());

    // Do not remove this println!
    const CUCUMBER_TEST_MARKER_A: &str = "Minotari Console Wallet running... (Script mode started)";
    println!("{}", CUCUMBER_TEST_MARKER_A);

    println!("Starting the command runner!");
    handle.block_on(command_runner(config, commands, wallet.clone()))?;

    // Do not remove this println!
    const CUCUMBER_TEST_MARKER_B: &str = "Minotari Console Wallet running... (Script mode completed)";
    println!("{}", CUCUMBER_TEST_MARKER_B);

    info!(target: LOG_TARGET, "Completed wallet script mode");

    wallet_or_exit(handle, cli, config, base_node_config, wallet)
}

/// Prompts the user to continue to the wallet, or exit.
fn wallet_or_exit(
    handle: Handle,
    cli: &Cli,
    config: &WalletConfig,
    base_node_config: &PeerConfig,
    wallet: WalletSqlite,
) -> Result<(), ExitError> {
    if cli.command_mode_auto_exit {
        info!(target: LOG_TARGET, "Auto exit argument supplied - exiting.");
        return Ok(());
    }

    if cli.non_interactive_mode {
        info!(target: LOG_TARGET, "Starting GRPC server.");
        grpc_mode(handle, config, wallet)
    } else {
        debug!(target: LOG_TARGET, "Prompting for run or exit key.");
        println!("\nPress Enter to continue to the wallet, or type q (or quit) followed by Enter.");
        let mut buf = String::new();
        std::io::stdin()
            .read_line(&mut buf)
            .map_err(|e| ExitError::new(ExitCode::IOError, e))?;

        match buf.as_str().trim() {
            "quit" | "q" | "exit" => {
                info!(target: LOG_TARGET, "Exiting.");
                Ok(())
            },
            _ => {
                info!(target: LOG_TARGET, "Starting TUI.");
                tui_mode(handle, config, base_node_config, wallet)
            },
        }
    }
}

pub fn tui_mode(
    handle: Handle,
    config: &WalletConfig,
    base_node_config: &PeerConfig,
    mut wallet: WalletSqlite,
) -> Result<(), ExitError> {
    let (events_broadcaster, _events_listener) = broadcast::channel(100);

    if config.grpc_enabled {
        #[cfg(feature = "grpc")]
        if let Some(address) = config.grpc_address.clone() {
            let grpc = WalletGrpcServer::new(wallet.clone()).map_err(|e| ExitError {
                exit_code: ExitCode::UnknownError,
                details: Some(e.to_string()),
            })?;

            let mut tls_identity = None;
            if config.grpc_tls_enabled {
                match handle
                    .block_on(read_identity(config.config_dir.clone()))
                    .map(Some)
                    .map_err(|e| ExitError::new(ExitCode::TlsConfigurationError, e.to_string()))
                {
                    Ok(identity) => tls_identity = identity,
                    Err(e) => return Err(e),
                }
            }

            handle.spawn(run_grpc(
                grpc,
                address,
                config.grpc_authentication.clone(),
                tls_identity,
                wallet.clone(),
            ));
        }
        #[cfg(not(feature = "grpc"))]
        return Err(ExitError::new(
            ExitCode::GrpcError,
            "gRPC server is enabled but not supported in this build",
        ));
    }

    let notifier = Notifier::new(
        config.notify_file.clone(),
        handle.clone(),
        wallet.clone(),
        events_broadcaster,
    );

    let base_node_selected;
    if let Some(peer) = base_node_config.base_node_custom.clone() {
        base_node_selected = peer;
    } else if let Some(peer) = get_custom_base_node_peer_from_db(&mut wallet) {
        base_node_selected = peer;
    } else if let Some(peer) = handle.block_on(wallet.get_base_node_peer()) {
        base_node_selected = peer;
    } else {
        return Err(ExitError::new(ExitCode::WalletError, "Could not select a base node"));
    }

    let app = handle.block_on(App::<CrosstermBackend<Stdout>>::new(
        "Minotari Wallet".into(),
        wallet,
        config.clone(),
        base_node_selected,
        base_node_config.clone(),
        notifier,
    ))?;

    info!(target: LOG_TARGET, "Starting app");

    // Do not remove this println!
    const CUCUMBER_TEST_MARKER: &str = "Minotari Console Wallet running... (TUI mode started)";
    println!("{}", CUCUMBER_TEST_MARKER);

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

pub fn recovery_mode(
    handle: Handle,
    base_node_config: &PeerConfig,
    wallet_config: &WalletConfig,
    wallet_mode: WalletMode,
    wallet: WalletSqlite,
) -> Result<(), ExitError> {
    // Do not remove this println!
    const CUCUMBER_TEST_MARKER_A: &str = "Minotari Console Wallet running... (Recovery mode started)";
    println!("{}", CUCUMBER_TEST_MARKER_A);

    println!("Starting recovery...");
    match handle.block_on(wallet_recovery(
        &wallet,
        base_node_config,
        wallet_config.recovery_retry_limit,
    )) {
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

    // Do not remove this println!
    const CUCUMBER_TEST_MARKER_B: &str = "Minotari Console Wallet running... (Recovery mode completed)";
    println!("{}", CUCUMBER_TEST_MARKER_B);

    println!("Starting TUI.");

    match wallet_mode {
        WalletMode::RecoveryDaemon => grpc_mode(handle, wallet_config, wallet),
        WalletMode::RecoveryTui => tui_mode(handle, wallet_config, base_node_config, wallet),
        _ => Err(ExitError::new(
            ExitCode::RecoveryError,
            "Unsupported post recovery mode",
        )),
    }
}

pub fn grpc_mode(handle: Handle, config: &WalletConfig, wallet: WalletSqlite) -> Result<(), ExitError> {
    info!(target: LOG_TARGET, "Starting grpc server");
    if let Some(address) = config.grpc_address.as_ref().filter(|_| config.grpc_enabled).cloned() {
        #[cfg(feature = "grpc")]
        {
            let grpc = WalletGrpcServer::new(wallet.clone()).map_err(|e| ExitError {
                exit_code: ExitCode::UnknownError,
                details: Some(e.to_string()),
            })?;
            let auth = config.grpc_authentication.clone();

            let mut tls_identity = None;
            if config.grpc_tls_enabled {
                match handle
                    .block_on(read_identity(config.config_dir.clone()))
                    .map(Some)
                    .map_err(|e| ExitError::new(ExitCode::TlsConfigurationError, e.to_string()))
                {
                    Ok(identity) => tls_identity = identity,
                    Err(e) => return Err(e),
                }
            }

            handle
                .block_on(run_grpc(grpc, address, auth, tls_identity, wallet))
                .map_err(|e| ExitError::new(ExitCode::GrpcError, e))?;
        }
        #[cfg(not(feature = "grpc"))]
        return Err(ExitError::new(
            ExitCode::GrpcError,
            "gRPC server is enabled but not supported in this build",
        ));
    } else {
        println!("GRPC server is disabled");
    }
    info!(target: LOG_TARGET, "Shutting down");
    Ok(())
}

async fn run_grpc(
    grpc: WalletGrpcServer,
    grpc_listener_addr: Multiaddr,
    auth_config: GrpcAuthentication,
    tls_identity: Option<Identity>,
    wallet: WalletSqlite,
) -> Result<(), String> {
    // Do not remove this println!
    const CUCUMBER_TEST_MARKER_A: &str = "Minotari Console Wallet running... (gRPC mode started)";
    println!("{}", CUCUMBER_TEST_MARKER_A);

    info!(target: LOG_TARGET, "Starting GRPC on {}", grpc_listener_addr);
    let address = multiaddr_to_socketaddr(&grpc_listener_addr).map_err(|e| e.to_string())?;
    let auth = ServerAuthenticationInterceptor::new(auth_config)
        .ok_or("Unable to prepare server gRPC authentication".to_string())?;
    let service = minotari_app_grpc::tari_rpc::wallet_server::WalletServer::with_interceptor(grpc, auth);

    let mut server_builder = if let Some(identity) = tls_identity {
        Server::builder()
            .tls_config(ServerTlsConfig::new().identity(identity))
            .map_err(|e| e.to_string())?
    } else {
        Server::builder()
    };

    server_builder
        .add_service(service)
        .serve_with_shutdown(address, wallet.wait_until_shutdown())
        .await
        .map_err(|e| format!("GRPC server returned error:{}", e))?;

    // Do not remove this println!
    const CUCUMBER_TEST_MARKER_B: &str = "Minotari Console Wallet running... (gRPC mode completed)";
    println!("{}", CUCUMBER_TEST_MARKER_B);

    info!(target: LOG_TARGET, "Stopping GRPC");
    Ok(())
}

#[cfg(test)]
mod test {
    use std::path::Path;

    use crate::{cli::CliCommands, wallet_modes::parse_command_file};

    #[test]
    #[allow(clippy::too_many_lines)]
    fn clap_parses_user_defined_commands_as_expected() {
        let script = "
            # Beginning of script file

            get-balance

            whois 5c4f2a4b3f3f84e047333218a84fd24f581a9d7e4f23b78e3714e9d174427d61

            discover-peer f6b2ca781342a3ebe30ee1643655c96f1d7c14f4d49f077695395de98ae73665

            send-minotari --message Our_secret! 125T \
                      f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb
            
            burn-minotari --message Ups_these_funds_will_be_burned! 100T

            create-key-pair --key-branch pie

            create-aggregate-signature-utxo \
                --amount 125T \
                --fee-per-gram 1 \
                --n 3 \
                --m 2 \
                --message ff \
                --maturity 0 \
                --public-keys=5c4f2a4b3f3f84e047333218a84fd24f581a9d7e4f23b78e3714e9d174427d61 \
                --public-keys=f6b2ca781342a3ebe30ee1643655c96f1d7c14f4d49f077695395de98ae73665

            sign-message \
                --private-key-id imported.96159b07298a453c9f514f5307f70659c7561dd6d9ed376854c5cb573cb2e311 \
                --challenge f6b2ca781342a3ebe30ee1643655c96f1d7c14f4d49f077695395de98ae73665

            encumber-aggregate-utxo \
                --fee-per-gram 1 \
                --output-hash f6b2ca781342a3ebe30ee1643655c96f1d7c14f4d49f077695395de98ae73665 \
                --script-input-shares=3ddde10d0775c20fb25015546c6a8068812044e7ca4ee1057e84ec9ab6705d03,8a55d1cb503be36875d38f2dc6abac7b23445bbd7253684a1506f5ee1855cd58 \
                --script-input-shares=3edf1ed103b0ac0bbad6a6de8369808d14dfdaaf294fe660646875d749a1f908,50a26c646db951720c919f59cd7a34600a7fc3ee978c64fbcce0ad184c46844c \
                --script-public-key-shares=5c4f2a4b3f3f84e047333218a84fd24f581a9d7e4f23b78e3714e9d174427d61 \
                --script-public-key-shares=f6b2ca781342a3ebe30ee1643655c96f1d7c14f4d49f077695395de98ae73665 \
                --script-signature-public-nonces=8a55d1cb503be36875d38f2dc6abac7b23445bbd7253684a1506f5ee1855cd58 \
                --script-signature-public-nonces=50a26c646db951720c919f59cd7a34600a7fc3ee978c64fbcce0ad184c46844c \
                --sender-offset-public-key-shares=5c4f2a4b3f3f84e047333218a84fd24f581a9d7e4f23b78e3714e9d174427d61 \
                --sender-offset-public-key-shares=f6b2ca781342a3ebe30ee1643655c96f1d7c14f4d49f077695395de98ae73665 \
                --metadata-ephemeral-public-key-shares=5c4f2a4b3f3f84e047333218a84fd24f581a9d7e4f23b78e3714e9d174427d61 \
                --metadata-ephemeral-public-key-shares=f6b2ca781342a3ebe30ee1643655c96f1d7c14f4d49f077695395de98ae73665 \
                --dh-shared-secret-shares=5c4f2a4b3f3f84e047333218a84fd24f581a9d7e4f23b78e3714e9d174427d61 \
                --dh-shared-secret-shares=f6b2ca781342a3ebe30ee1643655c96f1d7c14f4d49f077695395de98ae73665 \
                --recipient-address f4LR9f6WwwcPiKJjK5ciTkU1ocNhANa3FPw1wkyVUwbuKpgiihawCXy6PFszunUWQ4Te8KVFnyWVHHwsk9x5Cg7ZQiA

            spend-aggregate-utxo \
                --tx-id 12345678 \
                --meta-signatures=3ddde10d0775c20fb25015546c6a8068812044e7ca4ee1057e84ec9ab6705d03,8a55d1cb503be36875d38f2dc6abac7b23445bbd7253684a1506f5ee1855cd58 \
                --meta-signatures=3edf1ed103b0ac0bbad6a6de8369808d14dfdaaf294fe660646875d749a1f908,50a26c646db951720c919f59cd7a34600a7fc3ee978c64fbcce0ad184c46844c \
                --script-signatures=3ddde10d0775c20fb25015546c6a8068812044e7ca4ee1057e84ec9ab6705d03,8a55d1cb503be36875d38f2dc6abac7b23445bbd7253684a1506f5ee1855cd58 \
                --script-signatures=3edf1ed103b0ac0bbad6a6de8369808d14dfdaaf294fe660646875d749a1f908,50a26c646db951720c919f59cd7a34600a7fc3ee978c64fbcce0ad184c46844c \
                --script-offset-keys=5c4f2a4b3f3f84e047333218a84fd24f581a9d7e4f23b78e3714e9d174427d61 \
                --script-offset-keys=f6b2ca781342a3ebe30ee1643655c96f1d7c14f4d49f077695395de98ae73665

            create-script-sig \
                --private-key-id imported.96159b07298a453c9f514f5307f70659c7561dd6d9ed376854c5cb573cb2e311 \
                --secret-nonce imported.96159b07298a453c9f514f5307f70659c7561dd6d9ed376854c5cb573cb2e311 \
                --input-script ae010268593ed2d36a2d95f0ffe0f41649b97cc36fc4ef0c8ecd6bd28f9d56c76b793b08691435a5c813578f8a7f4973166dc1c6c15f37aec2a7d65b1583c8b2129364c916d5986a0c1b3dac7d6efb94bed688ba52fa8b962cf27c0446e2fea6d66a04 \
                --input-stack 050857c14f72cf885aac9f08c9484cb7cb06b6cc20eab68c9bee1e8d5a85649b0a6d31c5cc49afc1e03ebbcf55c82f47e8cbc796c33e96c17a31eab027ee821f00 \
                --ephemeral-commitment f6b2ca781342a3ebe30ee1643655c96f1d7c14f4d49f077695395de98ae73665 \
                --ephemeral-pubkey 8a55d1cb503be36875d38f2dc6abac7b23445bbd7253684a1506f5ee1855cd58 \
                --total-script-key 5c4f2a4b3f3f84e047333218a84fd24f581a9d7e4f23b78e3714e9d174427d61 \
                --commitment 94966b4f1b5dc050df1109cf07a516ae85912c82503b1a8c1625986a569fae67

            create-meta-sig \
                --secret-script-key imported.96159b07298a453c9f514f5307f70659c7561dd6d9ed376854c5cb573cb2e311 \
                --secret-sender-offset-key imported.96159b07298a453c9f514f5307f70659c7561dd6d9ed376854c5cb573cb2e311 \
                --secret-nonce imported.96159b07298a453c9f514f5307f70659c7561dd6d9ed376854c5cb573cb2e311 \
                --ephemeral-commitment f6b2ca781342a3ebe30ee1643655c96f1d7c14f4d49f077695395de98ae73665 \
                --ephemeral-pubkey 8a55d1cb503be36875d38f2dc6abac7b23445bbd7253684a1506f5ee1855cd58 \
                --total-meta-key 5c4f2a4b3f3f84e047333218a84fd24f581a9d7e4f23b78e3714e9d174427d61 \
                --commitment 94966b4f1b5dc050df1109cf07a516ae85912c82503b1a8c1625986a569fae67

            coin-split --message Make_many_dust_UTXOs! --fee-per-gram 2 0.001T 499

            make-it-rain --duration 100 --transactions-per-second 10 --start-amount 0.009200T --increase-amount 0T \
                      --start-time now --message Stressing_it_a_bit...!_(from_Feeling-a-bit-Generous) \
                      f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb

            export-tx 123456789 --output-file pie.txt

            import-tx --input-file pie_this_message.txt

            # End of script file
            "
        .to_string();

        let commands = parse_command_file(script).unwrap();

        let mut get_balance = false;
        let mut send_tari = false;
        let mut burn_tari = false;
        let mut create_key_pair = false;
        let mut create_aggregate_signature_utxo = false;
        let mut encumber_aggregate_utxo = false;
        let mut spend_aggregate_utxo = false;
        let mut sign_message = false;
        let mut create_script_sig = false;
        let mut create_meta_sig = false;
        let mut make_it_rain = false;
        let mut coin_split = false;
        let mut discover_peer = false;
        let mut export_tx = false;
        let mut import_tx = false;
        let mut whois = false;
        for command in commands {
            match command {
                CliCommands::GetBalance => get_balance = true,
                CliCommands::SendMinotari(_) => send_tari = true,
                CliCommands::BurnMinotari(_) => burn_tari = true,
                CliCommands::CreateKeyPair(_) => create_key_pair = true,
                CliCommands::CreateAggregateSignatureUtxo(_) => create_aggregate_signature_utxo = true,
                CliCommands::EncumberAggregateUtxo(_) => encumber_aggregate_utxo = true,
                CliCommands::SpendAggregateUtxo(_) => spend_aggregate_utxo = true,
                CliCommands::SignMessage(_) => sign_message = true,
                CliCommands::CreateScriptSig(_) => create_script_sig = true,
                CliCommands::CreateMetaSig(_) => create_meta_sig = true,
                CliCommands::SendOneSidedToStealthAddress(_) => {},
                CliCommands::MakeItRain(_) => make_it_rain = true,
                CliCommands::CoinSplit(_) => coin_split = true,
                CliCommands::DiscoverPeer(_) => discover_peer = true,
                CliCommands::Whois(_) => whois = true,
                CliCommands::ExportUtxos(_) => {},
                CliCommands::ExportTx(args) => {
                    if args.tx_id == 123456789 && args.output_file == Some("pie.txt".into()) {
                        export_tx = true
                    }
                },
                CliCommands::ImportTx(args) => {
                    if args.input_file == Path::new("pie_this_message.txt") {
                        import_tx = true
                    }
                },
                CliCommands::ExportSpentUtxos(_) => {},
                CliCommands::CountUtxos => {},
                CliCommands::SetBaseNode(_) => {},
                CliCommands::SetCustomBaseNode(_) => {},
                CliCommands::ClearCustomBaseNode => {},
                CliCommands::InitShaAtomicSwap(_) => {},
                CliCommands::FinaliseShaAtomicSwap(_) => {},
                CliCommands::ClaimShaAtomicSwapRefund(_) => {},
                CliCommands::RevalidateWalletDb => {},
                CliCommands::RegisterValidatorNode(_) => {},
                CliCommands::CreateTlsCerts => {},
                CliCommands::FaucetCreatePartyDetails(_) => {},
            }
        }
        assert!(
            get_balance &&
                send_tari &&
                burn_tari &&
                create_key_pair &&
                create_aggregate_signature_utxo &&
                encumber_aggregate_utxo &&
                spend_aggregate_utxo &&
                sign_message &&
                create_script_sig &&
                create_meta_sig &&
                make_it_rain &&
                coin_split &&
                discover_peer &&
                whois &&
                export_tx &&
                import_tx
        );
    }
}
