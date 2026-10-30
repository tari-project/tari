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
use minotari_wallet_grpc_client::WalletGrpcClient;
use tari_common_types::grpc_authentication::GrpcAuthentication;
use tari_utilities::SafePassword;
use tonic::transport::Channel;
use uuid::Uuid;

use crate::{
    cli::Cli,
    config::WalletMcpConfig,
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

        // Use the wallet client's connect_with_auth method
        let client = WalletGrpcClient::connect_with_auth(&wallet_grpc_url, &config.wallet_grpc.authentication)
            .await
            .map_err(|e| McpError::config_error(format!("Failed to connect to wallet: {e}")))?;

        log::info!("Successfully connected to wallet");
        Ok(client)
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

        // Extract proper CLI configuration and arguments
        let launch_config = cli.extract_launch_config();
        let (wallet_args, env_vars) = Self::prepare_auto_launch_params(config, cli);

        log::debug!("Launching wallet on port {available_port}");

        // Create process launcher using TariProcessLauncher
        let grpc_address = format!("127.0.0.1:{available_port}");
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

    /// Prepare launch args and environment variables for the spawned wallet process.
    /// Credentials are intentionally placed into environment variables (never argv / args)
    /// to prevent credential leakage via /proc/<pid>/cmdline and avoid CLI override parsing issues.
    pub(crate) fn prepare_auto_launch_params(
        config: &mut WalletMcpConfig,
        cli: &Cli,
    ) -> (Vec<String>, HashMap<String, String>) {
        let wallet_args = cli.extract_wallet_args();
        let mut env_vars = HashMap::new();

        // Pass wallet password via MINOTARI_WALLET_PASSWORD environment variable
        if let Some(ref password) = cli.wallet_password {
            env_vars.insert(
                "MINOTARI_WALLET_PASSWORD".to_string(),
                String::from_utf8_lossy(password.reveal()).to_string(),
            );
        }

        // Setup gRPC authentication for auto-launched wallet via environment variables
        // to avoid leaking credentials in argv / ps or failing string deserialization of untagged enums.
        match &config.wallet_grpc.authentication {
            GrpcAuthentication::Basic { username, password } => {
                env_vars.insert(
                    "TARI_WALLET__GRPC_AUTHENTICATION__USERNAME".to_string(),
                    username.clone(),
                );
                env_vars.insert(
                    "TARI_WALLET__GRPC_AUTHENTICATION__PASSWORD".to_string(),
                    String::from_utf8_lossy(password.reveal()).to_string(),
                );
            },
            GrpcAuthentication::None => {
                let random_user = "mcp_wallet";
                let random_password = Uuid::new_v4().to_string();
                env_vars.insert(
                    "TARI_WALLET__GRPC_AUTHENTICATION__USERNAME".to_string(),
                    random_user.to_string(),
                );
                env_vars.insert(
                    "TARI_WALLET__GRPC_AUTHENTICATION__PASSWORD".to_string(),
                    random_password.clone(),
                );
                config.wallet_grpc.authentication = GrpcAuthentication::Basic {
                    username: random_user.to_string(),
                    password: SafePassword::from(random_password),
                };
            },
        }

        (wallet_args, env_vars)
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
    use tari_common_types::grpc_authentication::GrpcAuthentication;

    use super::*;

    #[test]
    fn test_auto_launch_credentials_in_env_not_argv() {
        let cli = Cli::try_parse_from(["minotari_mcp_wallet", "--wallet-password", "test_wallet_auth"]).unwrap();

        let mut config = WalletMcpConfig::load(&cli).unwrap();
        let (args, env_vars) = WalletMcpServer::prepare_auto_launch_params(&mut config, &cli);

        // Args must NOT contain any passwords or credentials
        for arg in &args {
            assert!(!arg.contains("test_wallet_auth"));
            assert!(!arg.contains("mcp_wallet"));
            assert_ne!(arg, "-p");
            assert_ne!(arg, "--password");
        }

        // Env vars must contain the wallet password
        assert_eq!(
            env_vars.get("MINOTARI_WALLET_PASSWORD"),
            Some(&"test_wallet_auth".to_string())
        );

        // Env vars must contain generated gRPC authentication
        assert_eq!(
            env_vars.get("TARI_WALLET__GRPC_AUTHENTICATION__USERNAME"),
            Some(&"mcp_wallet".to_string())
        );
        assert!(env_vars.contains_key("TARI_WALLET__GRPC_AUTHENTICATION__PASSWORD"));

        // Config should have been updated to GrpcAuthentication::Basic
        assert!(matches!(
            config.wallet_grpc.authentication,
            GrpcAuthentication::Basic { .. }
        ));
    }

    #[test]
    fn test_auto_launch_configured_grpc_auth_in_env() {
        let cli = Cli::try_parse_from([
            "minotari_mcp_wallet",
            "--wallet-grpc-username",
            "custom_admin",
            "--wallet-grpc-password",
            "test_grpc_auth",
        ])
        .unwrap();

        let mut config = WalletMcpConfig::load(&cli).unwrap();
        let (args, env_vars) = WalletMcpServer::prepare_auto_launch_params(&mut config, &cli);

        for arg in &args {
            assert!(!arg.contains("test_grpc_auth"));
            assert!(!arg.contains("custom_admin"));
            assert_ne!(arg, "-p");
        }

        assert_eq!(
            env_vars.get("TARI_WALLET__GRPC_AUTHENTICATION__USERNAME"),
            Some(&"custom_admin".to_string())
        );
        assert_eq!(
            env_vars.get("TARI_WALLET__GRPC_AUTHENTICATION__PASSWORD"),
            Some(&"test_grpc_auth".to_string())
        );
    }
}
