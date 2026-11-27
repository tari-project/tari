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
    wallet_rpc::SpendLimits,
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

#[derive(Debug)]
enum IdentifyProbeError {
    Unauthenticated,
    Timeout,
    Failed(String),
}

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
        let limits = Arc::new(SpendLimits::new(
            config.security.spend_budget,
            config.security.max_fee_per_gram,
        ));
        let tool_registry = WalletToolRegistry::new(grpc_client.clone(), config.mcp.control_enabled, limits);

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

    /// Connect to `wallet_grpc_url` with `auth` and verify an authenticated `identify` call succeeds.
    async fn verify_wallet_identify(
        wallet_grpc_url: &str,
        auth: &GrpcAuthentication,
        timeout_duration: Duration,
    ) -> Result<(), IdentifyProbeError> {
        let probe = async {
            let mut test_client = WalletGrpcClient::connect_with_auth(wallet_grpc_url, auth)
                .await
                .map_err(|e| (tonic::Code::Unavailable, e.to_string()))?;
            test_client
                .identify(minotari_wallet_grpc_client::grpc::GetIdentityRequest {})
                .await
                .map_err(|status| (status.code(), status.to_string()))
        };

        match tokio::time::timeout(timeout_duration, probe).await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err((tonic::Code::Unauthenticated, _))) => Err(IdentifyProbeError::Unauthenticated),
            Ok(Err((_, err_str))) => Err(IdentifyProbeError::Failed(err_str)),
            Err(_) => Err(IdentifyProbeError::Timeout),
        }
    }

    /// Probe whether an existing wallet is running and authenticated on the configured port.
    /// Returns `Ok(false)` only when nothing is listening on the port. If a listener is present,
    /// it must pass an authenticated `identify` call; otherwise an error is returned and auto-launch
    /// does not run.
    async fn probe_existing_wallet(config: &WalletMcpConfig, port: u16) -> McpResult<bool> {
        let health_monitor = ServiceHealthMonitors::wallet(&config.wallet_grpc.address);
        if !health_monitor.is_service_ready().await {
            return Ok(false);
        }

        let wallet_grpc_url = config.wallet_grpc_url();
        let timeout_duration = Duration::from_secs(config.wallet_grpc.timeout_secs.max(1));
        let auth = &config.wallet_grpc.authentication;

        match Self::verify_wallet_identify(&wallet_grpc_url, auth, timeout_duration).await {
            Ok(()) => {
                log::info!("Wallet already running and authenticated on port {port}");
                Ok(true)
            },
            Err(IdentifyProbeError::Unauthenticated) => Err(McpError::service_unavailable(format!(
                "An orphaned or existing wallet is listening on port {port} with different authentication \
                 credentials. Please stop the existing wallet process or provide matching credentials via \
                 MINOTARI_WALLET_GRPC_USERNAME and MINOTARI_WALLET_GRPC_PASSWORD (or --wallet-grpc-password-file)."
            ))),
            Err(IdentifyProbeError::Timeout) => Err(McpError::service_unavailable(format!(
                "A wallet is listening on port {port} but did not answer the identify check within {}s; refusing to \
                 auto-launch a second wallet.",
                timeout_duration.as_secs()
            ))),
            Err(IdentifyProbeError::Failed(err_str)) => Err(McpError::service_unavailable(format!(
                "A service is listening on port {port} but failed the authenticated wallet identify check: {err_str}"
            ))),
        }
    }

    /// Extract the bound gRPC address reported by an auto-launched console wallet on stdout.
    pub(crate) fn extract_reported_grpc_address(output: &[String]) -> Option<String> {
        const MARKER: &str = "Minotari Console Wallet gRPC listening on ";
        output.iter().find_map(|line| {
            let addr = line.split_once(MARKER)?.1.trim();
            (!addr.is_empty()).then(|| addr.to_string())
        })
    }

    /// Ensure wallet is running, auto-launch if needed
    async fn ensure_wallet_running(config: &mut WalletMcpConfig, cli: &Cli) -> McpResult<Option<Arc<ProcessLauncher>>> {
        // Extract port from gRPC address
        let port = CliIntegrationUtils::extract_port_from_address(&config.wallet_grpc.address).unwrap_or(18143);

        if Self::probe_existing_wallet(config, port).await? {
            return Ok(None);
        }

        log::info!("Wallet not detected, auto-launching...");

        // Extract proper CLI configuration and arguments
        let launch_config = cli.extract_launch_config();
        let (wallet_args, env_vars) = Self::prepare_auto_launch_params(config, cli);

        // Have the child bind its own OS-assigned port (127.0.0.1:0) and report it back on stdout,
        // unless an explicit alternative gRPC address was configured.
        let grpc_address = cli
            .wallet_alt_grpc_address
            .clone()
            .unwrap_or_else(|| "127.0.0.1:0".to_string());

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

        // Wait for the launched wallet to report its bound port and pass an authenticated identify call
        let mut attempts = 0u32;
        const MAX_ATTEMPTS: u32 = 600; // 10 minutes with 1-second intervals
        let identify_timeout = Duration::from_secs(config.wallet_grpc.timeout_secs.max(1));

        while attempts < MAX_ATTEMPTS {
            // Check launcher status
            while let Ok(status) = status_rx.try_recv() {
                log::debug!("Launcher status: {status:?}");
                if let ProcessLaunchStatus::Failed(err) = status {
                    launcher_handle.abort();
                    return Err(McpError::server_error(format!("Failed to start wallet: {err}")));
                }
            }

            let output = launcher.get_captured_output().await;
            if let Some(bound_addr) = Self::extract_reported_grpc_address(&output) {
                let bound_url = if bound_addr.starts_with("http://") || bound_addr.starts_with("https://") {
                    bound_addr.clone()
                } else {
                    format!("http://{bound_addr}")
                };

                let auth = &config.wallet_grpc.authentication;
                match Self::verify_wallet_identify(&bound_url, auth, identify_timeout).await {
                    Ok(()) => {
                        log::info!("Wallet is now running and authenticated on {bound_addr}");
                        config.wallet_grpc.address = bound_addr;
                        return Ok(Some(launcher));
                    },
                    Err(IdentifyProbeError::Unauthenticated) => {
                        let _ = launcher.stop().await;
                        launcher_handle.abort();
                        return Err(McpError::service_unavailable(format!(
                            "Auto-launched wallet on {bound_addr} rejected gRPC authentication credentials"
                        )));
                    },
                    Err(IdentifyProbeError::Timeout | IdentifyProbeError::Failed(_)) => {},
                }
            }

            if launcher.get_pid().await.is_some() && !launcher.is_running().await {
                launcher_handle.abort();
                let output_summary = if output.is_empty() {
                    "No output captured".to_string()
                } else {
                    output.join("\n")
                };
                return Err(McpError::server_error(format!(
                    "Auto-launched wallet exited before passing identify check. Output:\n{output_summary}"
                )));
            }

            tokio::time::sleep(Duration::from_secs(1)).await;
            attempts = attempts.saturating_add(1);
        }

        let _ = launcher.stop().await;
        launcher_handle.abort();
        Err(McpError::server_error(
            "Wallet failed to report a bound gRPC port and pass identify within timeout",
        ))
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
        if let Some(ref password) = config.wallet_grpc.wallet_password {
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
    use serde::{Deserialize, Serialize};
    use tari_common::{
        DefaultConfigLoader,
        SubConfigPath,
        configuration::{ConfigOverrideProvider, Network},
        load_configuration,
    };
    use tari_common_types::grpc_authentication::GrpcAuthentication;
    use tokio::net::TcpListener;

    use super::*;
    use crate::config::{GRPC_PASSWORD_ENV, WALLET_PASSWORD_ENV};

    #[test]
    fn test_auto_launch_credentials_in_env_not_argv() {
        let cli = Cli::try_parse_from(["minotari_mcp_wallet"]).unwrap();
        let env_fn = |key: &str| (key == WALLET_PASSWORD_ENV).then(|| "test_wallet_auth".to_string());
        let mut config = WalletMcpConfig::load_with_env(&cli, env_fn).unwrap();
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
        let cli = Cli::try_parse_from(["minotari_mcp_wallet", "--wallet-grpc-username", "custom_admin"]).unwrap();
        let env_fn = |key: &str| (key == GRPC_PASSWORD_ENV).then(|| "test_grpc_auth".to_string());
        let mut config = WalletMcpConfig::load_with_env(&cli, env_fn).unwrap();
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

    #[test]
    fn test_extract_reported_grpc_address() {
        let output = vec![
            "STDOUT: Minotari Console Wallet running... (gRPC mode started)".to_string(),
            "STDOUT: Minotari Console Wallet gRPC listening on 127.0.0.1:49215".to_string(),
        ];
        assert_eq!(
            WalletMcpServer::extract_reported_grpc_address(&output),
            Some("127.0.0.1:49215".to_string())
        );
    }

    #[tokio::test]
    async fn probe_existing_wallet_errors_on_stalled_or_non_wallet_listener() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        // Hold accepted sockets open without speaking HTTP/2 so identify times out
        let accept_task = tokio::spawn(async move {
            let mut held = Vec::new();
            while let Ok((stream, _)) = listener.accept().await {
                held.push(stream);
            }
        });

        let cli = Cli::try_parse_from([
            "minotari_mcp_wallet",
            "--wallet-grpc-address",
            &addr.to_string(),
            "--wallet-grpc-timeout",
            "1",
        ])
        .unwrap();
        let config = WalletMcpConfig::load_with_env(&cli, |_| None).unwrap();

        let err = WalletMcpServer::probe_existing_wallet(&config, addr.port())
            .await
            .unwrap_err()
            .to_string();
        assert!(
            err.contains(&format!("listening on port {}", addr.port())),
            "Expected listening-port error, got: {err}"
        );
        accept_task.abort();
    }

    #[derive(Default, Serialize, Deserialize)]
    struct WalletAuthSection {
        #[serde(default)]
        grpc_authentication: GrpcAuthentication,
    }

    impl SubConfigPath for WalletAuthSection {
        fn main_key_prefix() -> &'static str {
            "wallet"
        }
    }

    struct NoOverrides;
    impl ConfigOverrideProvider for NoOverrides {
        fn get_config_property_overrides(&self, _network: &Network) -> Vec<(String, String)> {
            Vec::new()
        }
    }

    struct EnvGuard(&'static [&'static str]);
    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for key in self.0 {
                // SAFETY: Scoped test cleanup for process environment variables.
                unsafe {
                    std::env::remove_var(key);
                }
            }
        }
    }

    #[test]
    fn test_auto_launch_env_vars_deserialize_into_wallet_grpc_authentication() {
        const USER_ENV: &str = "TARI_WALLET__GRPC_AUTHENTICATION__USERNAME";
        const PASS_ENV: &str = "TARI_WALLET__GRPC_AUTHENTICATION__PASSWORD";
        let _guard = EnvGuard(&[USER_ENV, PASS_ENV]);

        let cli = Cli::try_parse_from(["minotari_mcp_wallet"]).unwrap();
        let mut config = WalletMcpConfig::load_with_env(&cli, |_| None).unwrap();
        let (_args, env_vars) = WalletMcpServer::prepare_auto_launch_params(&mut config, &cli);

        // SAFETY: Setting test-scoped environment variables produced by prepare_auto_launch_params.
        unsafe {
            std::env::set_var(USER_ENV, env_vars.get(USER_ENV).unwrap());
            std::env::set_var(PASS_ENV, env_vars.get(PASS_ENV).unwrap());
        }

        let dir = std::env::temp_dir().join(format!("tari_mcp_wallet_e2e_{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let config_path = dir.join("config.toml");
        let network = Network::get_current_or_user_setting_or_default();
        let cfg = load_configuration(&config_path, true, true, &NoOverrides, Some(network)).unwrap();
        let loaded = WalletAuthSection::load_from(&cfg).unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        match (&loaded.grpc_authentication, &config.wallet_grpc.authentication) {
            (
                GrpcAuthentication::Basic {
                    username: loaded_user,
                    password: loaded_pass,
                },
                GrpcAuthentication::Basic {
                    username: expected_user,
                    password: expected_pass,
                },
            ) => {
                assert_eq!(loaded_user, expected_user);
                assert_eq!(loaded_pass.reveal(), expected_pass.reveal());
            },
            other => panic!("Expected matching GrpcAuthentication::Basic, got {other:?}"),
        }
    }
}
