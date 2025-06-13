//! Wallet MCP server implementation

use crate::config::WalletMcpConfig;
use crate::tools::WalletToolRegistry;
use crate::resources::WalletResourceRegistry;
use crate::prompts::WalletPromptRegistry;
use crate::cli::Cli;
use minotari_mcp_common::{
    McpServer, McpServerBuilder, McpResult, McpError,
    ServiceHealthMonitors, TariProcessLauncher,
    ProcessLaunchStatus, CliConfigExtractor, CliIntegrationUtils,
    StartupDiagnostics
};
use minotari_wallet_grpc_client::WalletGrpcClient;
use std::sync::Arc;
use std::time::Duration;
use tonic::transport::Channel;

/// Minotari Wallet MCP Server
pub struct WalletMcpServer {
    inner: Box<dyn McpServer>,
    _grpc_client: Arc<WalletGrpcClient<Channel>>,
}

impl WalletMcpServer {
    /// Create a new Wallet MCP server
    pub async fn new(config: WalletMcpConfig, cli: &Cli) -> McpResult<Self> {
        // Auto-launch wallet if configured and not running
        log::info!("Auto-launch enabled: {}", config.should_auto_launch_wallet());
        if config.should_auto_launch_wallet() {
            log::info!("Auto-launching wallet...");
            Self::ensure_wallet_running(&config, cli).await?;
        }

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
        })
    }

    /// Start the MCP server
    pub async fn start(self) -> McpResult<()> {
        log::info!("Starting Minotari Wallet MCP Server");
        self.inner.start().await
    }

    /// Create gRPC client connection to wallet
    async fn create_grpc_client(config: &WalletMcpConfig) -> McpResult<WalletGrpcClient<Channel>> {
        let wallet_grpc_url = config.wallet_grpc_url();
        
        log::info!("Connecting to wallet at: {}", wallet_grpc_url);
        
        // Use the wallet client's connect method
        let client = WalletGrpcClient::connect(&wallet_grpc_url)
            .await
            .map_err(|e| McpError::config_error(format!("Failed to connect to wallet: {}", e)))?;

        log::info!("Successfully connected to wallet");
        Ok(client)
    }

    /// Ensure wallet is running, auto-launch if needed
    async fn ensure_wallet_running(config: &WalletMcpConfig, cli: &Cli) -> McpResult<()> {
        // Extract port from gRPC address
        let port = CliIntegrationUtils::extract_port_from_address(&config.wallet_grpc.address)
            .unwrap_or(18143);

        // Check if already running using proper health monitor
        let health_monitor = ServiceHealthMonitors::wallet(&config.wallet_grpc.address);
        if health_monitor.is_service_ready().await {
            log::info!("Wallet already running and healthy on port {}", port);
            return Ok(());
        }

        log::info!("Wallet not detected, auto-launching...");

        // Find an available port if the default is in use
        let available_port = CliIntegrationUtils::find_available_port(port).unwrap_or(port);
        if available_port != port {
            log::info!("Port {} in use, using port {} instead", port, available_port);
        }

        // Extract proper CLI configuration and arguments
        let launch_config = cli.extract_launch_config();
        let wallet_args = cli.extract_wallet_args();

        log::debug!("Launching wallet with config: {:?}", launch_config);
        log::debug!("Wallet arguments: {:?}", wallet_args);

        // Create process launcher using TariProcessLauncher
        let grpc_address = format!("127.0.0.1:{}", available_port);
        let (launcher, mut status_rx) = TariProcessLauncher::launch_wallet(
            launch_config.base_path,
            launch_config.config_path,
            launch_config.network,
            grpc_address,
            wallet_args,
        ).await?;

        // Start launcher in background
        let launcher_handle = tokio::spawn(async move {
            match launcher.launch().await {
                Ok(result) => {
                    log::info!("Wallet launched successfully: {:?}", result);
                    // Keep the launcher alive to maintain the process
                    loop {
                        if !launcher.is_running().await {
                            log::warn!("Wallet process has stopped");
                            break;
                        }
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                }
                Err(e) => {
                    log::error!("Failed to launch wallet: {}", e);
                }
            }
        });

        // Wait for the wallet to become healthy with enhanced monitoring
        let mut attempts = 0;
        const MAX_ATTEMPTS: u32 = 120; // 10 minutes with 5-second intervals (wallets take longer)
        
        // Create health monitor for the actual launched port
        let launched_health_monitor = ServiceHealthMonitors::wallet(&format!("127.0.0.1:{}", available_port));
        
        while attempts < MAX_ATTEMPTS {
            // Check if the service is ready using proper health check
            if launched_health_monitor.is_service_ready().await {
                log::info!("Wallet is now running and healthy on port {}", available_port);
                return Ok(());
            }
            
            // Check launcher status
            if let Ok(status) = status_rx.try_recv() {
                log::debug!("Launcher status: {:?}", status);
                match status {
                    ProcessLaunchStatus::Failed(err) => {
                        launcher_handle.abort();
                        return Err(McpError::server_error(format!("Failed to start wallet: {}", err)));
                    }
                    ProcessLaunchStatus::Running => {
                        log::info!("Wallet process is running, waiting for health checks...");
                    }
                    _ => {}
                }
            }
            
            tokio::time::sleep(Duration::from_secs(5)).await;
            attempts += 1;
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
