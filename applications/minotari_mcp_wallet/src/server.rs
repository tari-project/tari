// Copyright 2025, The Tari Project
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
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.//! Common MCP (Model Context Protocol)
// infrastructure for Tari applications
//! Wallet MCP server implementation

use std::{collections::HashMap, sync::Arc, time::Duration};

use minotari_mcp_common::{
    CliConfigExtractor,
    CliIntegrationUtils,
    LaunchCliConfig,
    McpError,
    McpResult,
    McpServer,
    McpServerBuilder,
    ProcessLaunchStatus,
    ProcessLauncher,
    ServiceHealthMonitors,
    StartupDiagnostics,
    TariProcessLauncher,
};
use minotari_wallet_grpc_client::{GrpcAuthentication, WalletGrpcClient};
use tonic::transport::Channel;

use crate::{
    cli::Cli,
    config::{WalletMcpConfig, generate_per_run_grpc_auth},
    prompts::WalletPromptRegistry,
    resources::WalletResourceRegistry,
    tools::WalletToolRegistry,
};

/// Minotari Wallet MCP Server
pub struct WalletMcpServer {
    inner: Box<dyn McpServer>,
    _grpc_client: Arc<WalletGrpcClient<Channel>>,
    launched_process: Option<Arc<ProcessLauncher>>,
}

impl WalletMcpServer {
    /// Create a new Wallet MCP server
    pub async fn new(mut config: WalletMcpConfig, cli: &Cli) -> McpResult<Self> {
        // Auto-launch wallet if configured and not running
        let launched_process = if config.should_auto_launch_wallet() {
            Self::ensure_wallet_running(&mut config, cli).await?
        } else {
            None
        };

        // Create gRPC client connection to wallet
        let grpc_client = Self::create_grpc_client(&config).await?;
        let grpc_client = Arc::new(grpc_client);

        // Create tool registry with wallet-specific tools
        let tool_registry = WalletToolRegistry::new(grpc_client.clone(), config.mcp.control_enabled);

        // Create resource registry with wallet-specific resources
        let resource_registry = WalletResourceRegistry::new(grpc_client.clone());

        // Create prompt registry with wallet-specific prompts
        let prompt_registry = WalletPromptRegistry::new();

        // Build the MCP server
        let server = McpServerBuilder::new(config.mcp)
            .with_tool_registry(tool_registry)
            .with_resource_registry(resource_registry)
            .with_prompt_registry(prompt_registry)
            .build()?;

        Ok(Self {
            inner: Box::new(server),
            _grpc_client: grpc_client,
            launched_process,
        })
    }

    /// Start the MCP server
    pub async fn start(&self) -> McpResult<()> {
        log::info!("Starting Minotari Wallet MCP Server");
        self.inner.start().await
    }

    /// Stop the MCP server
    #[allow(dead_code)]
    pub async fn stop(&self) -> McpResult<()> {
        log::info!("Stopping Minotari Wallet MCP Server");

        // First, stop any launched processes
        if let Some(ref process_launcher) = self.launched_process {
            log::info!("Stopping launched wallet process...");
            if let Err(e) = process_launcher.stop().await {
                log::warn!("Failed to stop launched wallet: {e}");
            }
        }

        // Then stop the MCP server
        self.inner.stop().await
    }

    /// Create gRPC client connection to wallet
    async fn create_grpc_client(config: &WalletMcpConfig) -> McpResult<WalletGrpcClient<Channel>> {
        let wallet_grpc_url = config.wallet_grpc_url();

        log::info!("Connecting to wallet at: {wallet_grpc_url}");

        // Connect using configured (or auto-generated per-run) gRPC authentication credentials
        let client = WalletGrpcClient::connect_with_auth(&wallet_grpc_url, &config.wallet_grpc.authentication)
            .await
            .map_err(|e| McpError::config_error(format!("Failed to connect to wallet: {e}")))?;

        log::info!("Successfully connected to wallet");
        Ok(client)
    }

    /// Prepare launch configuration, CLI arguments, and child environment variables for an auto-launched wallet.
    /// Generates a per-run `GrpcAuthentication::Basic` credential if none is configured.
    fn prepare_auto_launch_wallet(
        config: &mut WalletMcpConfig,
        cli: &Cli,
    ) -> (LaunchCliConfig, Vec<String>, HashMap<String, String>) {
        if matches!(config.wallet_grpc.authentication, GrpcAuthentication::None) {
            config.wallet_grpc.authentication = generate_per_run_grpc_auth();
        }

        let launch_config = cli.extract_launch_config();
        let wallet_args = cli.extract_wallet_args();
        let mut env_vars = launch_config.environment_vars.clone();

        if let GrpcAuthentication::Basic {
            ref username,
            ref password,
        } = config.wallet_grpc.authentication
        {
            env_vars.insert(
                "TARI_WALLET__GRPC_AUTHENTICATION__USERNAME".to_string(),
                username.clone(),
            );
            env_vars.insert(
                "TARI_WALLET__GRPC_AUTHENTICATION__PASSWORD".to_string(),
                String::from_utf8_lossy(password.reveal()).to_string(),
            );
        }

        (launch_config, wallet_args, env_vars)
    }

    /// Ensure wallet is running, auto-launch if needed
    async fn ensure_wallet_running(config: &mut WalletMcpConfig, cli: &Cli) -> McpResult<Option<Arc<ProcessLauncher>>> {
        // Extract port from gRPC address
        let port = CliIntegrationUtils::extract_port_from_address(&config.wallet_grpc.address).unwrap_or(18143);

        // Check if already running using proper health monitor
        let health_monitor = ServiceHealthMonitors::wallet(&config.wallet_grpc.address);
        if health_monitor.is_service_ready().await {
            log::info!("Wallet already running and healthy on port {port}");
            return Ok(None); // No process launched by us
        }

        log::info!("Wallet not detected, auto-launching...");

        // Find an available port if the default is in use
        let available_port = CliIntegrationUtils::find_available_port(port).unwrap_or(port);
        if available_port != port {
            log::info!("Port {port} in use, using port {available_port} instead");
        }

        // Prepare CLI configuration, arguments, and per-child environment variables (never logging secrets)
        let (launch_config, wallet_args, env_vars) = Self::prepare_auto_launch_wallet(config, cli);

        // Create process launcher using TariProcessLauncher
        let grpc_address = format!("127.0.0.1:{available_port}");
        config.wallet_grpc.address = grpc_address.clone();
        let (launcher, mut status_rx) = TariProcessLauncher::launch_wallet(
            launch_config.base_path,
            launch_config.config_path,
            launch_config.network,
            grpc_address,
            wallet_args,
            env_vars,
        )
        .await?;

        let launcher = Arc::new(launcher);
        let launcher_for_background = launcher.clone();

        // Start launcher in background
        let launcher_handle = tokio::spawn(async move {
            match launcher_for_background.launch().await {
                Ok(result) => {
                    log::info!("Wallet launched successfully: {result:?}");
                    // Keep the launcher alive to maintain the process
                    loop {
                        if !launcher_for_background.is_running().await {
                            log::warn!("Wallet process has stopped");
                            break;
                        }
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                },
                Err(e) => {
                    log::error!("Failed to launch wallet: {e}");
                },
            }
        });

        // Wait for the wallet to become healthy with enhanced monitoring
        let mut attempts = 0;
        const MAX_ATTEMPTS: u32 = 120; // 10 minutes with 5-second intervals (wallets take longer)

        // Create health monitor for the actual launched port
        let launched_health_monitor = ServiceHealthMonitors::wallet(&format!("127.0.0.1:{available_port}"));

        while attempts < MAX_ATTEMPTS {
            // Check if the service is ready using proper health check
            if launched_health_monitor.is_service_ready().await {
                log::info!("Wallet is now running and healthy on port {available_port}");
                return Ok(Some(launcher));
            }

            // Check launcher status
            if let Ok(status) = status_rx.try_recv() {
                log::debug!("Launcher status: {status:?}");
                match status {
                    ProcessLaunchStatus::Failed(err) => {
                        launcher_handle.abort();
                        return Err(McpError::server_error(format!("Failed to start wallet: {err}")));
                    },
                    ProcessLaunchStatus::Running => {
                        log::info!("Wallet process is running, waiting for health checks...");
                    },
                    _ => {},
                }
            }

            tokio::time::sleep(Duration::from_secs(5)).await;
            attempts = attempts.saturating_add(1);
        }

        launcher_handle.abort();
        Err(McpError::server_error("Wallet failed to become healthy within timeout"))
    }

    /// Run startup diagnostics for troubleshooting
    #[allow(dead_code)]
    pub async fn run_diagnostics(config: &WalletMcpConfig, cli: &Cli) -> String {
        let launch_config = cli.extract_launch_config();

        let diagnostics = StartupDiagnostics::new()
            .with_base_path(launch_config.base_path)
            .with_config_path(launch_config.config_path)
            .with_wallet_grpc_address(config.wallet_grpc.address.clone());

        let results = diagnostics.run_diagnostics().await;
        diagnostics.format_diagnostic_report(&results)
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;
    use tari_utilities::SafePassword;

    use super::*;

    #[test]
    fn test_prepare_auto_launch_generates_per_run_grpc_auth_and_uses_env() {
        let mut cli = Cli::try_parse_from(["minotari_mcp_wallet"]).unwrap();
        cli.wallet_password = Some(SafePassword::from("wallet-secret-123"));

        let mut config = WalletMcpConfig::load(&cli).unwrap();
        config.wallet_grpc.authentication = GrpcAuthentication::None;

        let (_launch_config, wallet_args, env_vars) = WalletMcpServer::prepare_auto_launch_wallet(&mut config, &cli);

        // Auto-launch must generate a Basic credential when none was configured
        let (username, password) = config
            .wallet_grpc
            .authentication
            .username_password()
            .expect("expected generated Basic auth");
        assert!(username.starts_with("mcp_"));
        assert!(!password.reveal().is_empty());

        // Child environment receives wallet password and generated gRPC Basic auth credentials
        assert_eq!(
            env_vars.get("MINOTARI_WALLET_PASSWORD").map(String::as_str),
            Some("wallet-secret-123")
        );
        assert_eq!(
            env_vars
                .get("TARI_WALLET__GRPC_AUTHENTICATION__USERNAME")
                .map(String::as_str),
            Some(username)
        );
        assert_eq!(
            env_vars
                .get("TARI_WALLET__GRPC_AUTHENTICATION__PASSWORD")
                .map(String::as_bytes),
            Some(password.reveal().as_slice())
        );

        // Neither secret appears in child argv
        let password_str = String::from_utf8_lossy(password.reveal());
        assert!(!wallet_args.iter().any(|a| a.contains("wallet-secret-123")));
        assert!(!wallet_args.iter().any(|a| a.contains(password_str.as_ref())));
    }

    #[test]
    fn test_prepare_auto_launch_preserves_configured_grpc_auth() {
        let cli = Cli::try_parse_from(["minotari_mcp_wallet"]).unwrap();
        let mut config = WalletMcpConfig::load(&cli).unwrap();
        config.wallet_grpc.authentication = GrpcAuthentication::Basic {
            username: "configured_user".to_string(),
            password: SafePassword::from("configured_pass"),
        };

        let (_launch_config, _wallet_args, env_vars) = WalletMcpServer::prepare_auto_launch_wallet(&mut config, &cli);

        let (username, password) = config
            .wallet_grpc
            .authentication
            .username_password()
            .expect("expected Basic auth");
        assert_eq!(username, "configured_user");
        assert_eq!(password.reveal(), b"configured_pass");
        assert_eq!(
            env_vars
                .get("TARI_WALLET__GRPC_AUTHENTICATION__USERNAME")
                .map(String::as_str),
            Some("configured_user")
        );
        assert_eq!(
            env_vars
                .get("TARI_WALLET__GRPC_AUTHENTICATION__PASSWORD")
                .map(String::as_str),
            Some("configured_pass")
        );
    }
}
