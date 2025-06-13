//! Wallet MCP server implementation

use crate::config::WalletMcpConfig;
use crate::tools::WalletToolRegistry;
use crate::resources::WalletResourceRegistry;
use crate::prompts::WalletPromptRegistry;
use minotari_mcp_common::{
    McpServer, McpServerBuilder, McpResult, McpError,
    ProcessSupervisor, ProcessType, ProcessUtils, ProcessStatus
};
use minotari_wallet_grpc_client::WalletGrpcClient;
use std::sync::Arc;
use tonic::transport::Channel;

/// Minotari Wallet MCP Server
pub struct WalletMcpServer {
    inner: Box<dyn McpServer>,
    _grpc_client: Arc<WalletGrpcClient<Channel>>,
}

impl WalletMcpServer {
    /// Create a new Wallet MCP server
    pub async fn new(config: WalletMcpConfig) -> McpResult<Self> {
        // Auto-launch wallet if configured and not running
        if config.should_auto_launch_wallet() {
            Self::ensure_wallet_running(&config).await?;
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
    async fn ensure_wallet_running(config: &WalletMcpConfig) -> McpResult<()> {
        // Extract port from gRPC address
        let port = config.wallet_grpc.address
            .split(':')
            .last()
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(18143);

        // Check if already running
        if ProcessUtils::is_service_running(port).await {
            log::info!("Wallet already running on port {}", port);
            return Ok(());
        }

        log::info!("Wallet not detected, auto-launching...");

        // Find the minotari_console_wallet executable
        let wallet_executable = Self::find_wallet_executable()?;
        
        // Find an available port if the default is in use
        let available_port = ProcessUtils::find_available_port(port).unwrap_or(port);
        if available_port != port {
            log::info!("Port {} in use, using port {} instead", port, available_port);
        }
        
        // Build command arguments
        let (executable, args) = ProcessUtils::build_wallet_command(
            &wallet_executable,
            "/tmp/tari", // This should come from CLI
            "config/config.toml", // This should come from CLI  
            Some("mainnet"), // This should come from CLI
            true, // Enable gRPC
            Some(&format!("127.0.0.1:{}", available_port)), // gRPC address
            true, // Non-interactive
            &[], // Additional args
        );

        // Create and start process supervisor
        let (supervisor, mut status_rx) = ProcessSupervisor::new(
            ProcessType::Wallet,
            executable,
            args,
            available_port,
        )?;

        // Start supervisor in background
        tokio::spawn(async move {
            if let Err(e) = supervisor.start().await {
                log::error!("Wallet supervisor failed: {}", e);
            }
        });

        // Wait for the wallet to become healthy
        let mut attempts = 0;
        while attempts < 30 {
            if ProcessUtils::is_service_running(available_port).await {
                log::info!("Wallet is now running and healthy on port {}", available_port);
                return Ok(());
            }
            
            // Check supervisor status
            if let Ok(status) = status_rx.try_recv() {
                log::debug!("Supervisor status: {:?}", status);
                match status {
                    ProcessStatus::Failed(err) => {
                        return Err(McpError::server_error(format!("Failed to start wallet: {}", err)));
                    }
                    _ => {}
                }
            }
            
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            attempts += 1;
        }

        Err(McpError::server_error("Wallet failed to start within timeout"))
    }

    /// Find the minotari_console_wallet executable
    fn find_wallet_executable() -> McpResult<String> {
        // Try to find the executable in common locations
        let possible_paths = [
            "minotari_console_wallet",
            "./minotari_console_wallet", 
            "../minotari_console_wallet/target/release/minotari_console_wallet",
            "../minotari_console_wallet/target/debug/minotari_console_wallet",
        ];

        for path in &possible_paths {
            if std::path::Path::new(path).exists() {
                return Ok(path.to_string());
            }
        }

        // Try to find in PATH
        if which::which("minotari_console_wallet").is_ok() {
            return Ok("minotari_console_wallet".to_string());
        }

        Err(McpError::config_error("Could not find minotari_console_wallet executable. Please ensure it's in PATH or specify the full path."))
    }
}
