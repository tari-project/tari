//! Wallet MCP server implementation

use crate::config::WalletMcpConfig;
use crate::tools::WalletToolRegistry;
use crate::resources::WalletResourceRegistry;
use crate::prompts::WalletPromptRegistry;
use minotari_mcp_common::{
    McpServer, McpServerBuilder, McpResult, McpError
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
}
