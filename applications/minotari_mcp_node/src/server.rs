//! Node MCP server implementation

use crate::config::NodeMcpConfig;
use crate::tools::NodeToolRegistry;
use crate::resources::NodeResourceRegistry;
use crate::prompts::NodePromptRegistry;
use minotari_mcp_common::{
    McpServer, McpServerBuilder, McpResult, McpError
};
use minotari_node_grpc_client::{BaseNodeGrpcClient, grpc::base_node_client::BaseNodeClient};
use std::sync::Arc;
use tonic::transport::{Channel, Endpoint};

/// Minotari Node MCP Server
pub struct NodeMcpServer {
    inner: Box<dyn McpServer>,
    _grpc_client: Arc<BaseNodeGrpcClient<Channel>>,
}

impl NodeMcpServer {
    /// Create a new Node MCP server
    pub async fn new(config: NodeMcpConfig) -> McpResult<Self> {
        // Create gRPC client connection to base node
        let grpc_client = Self::create_grpc_client(&config).await?;
        let grpc_client = Arc::new(grpc_client);

        // Create tool registry with node-specific tools
        let tool_registry = NodeToolRegistry::new(grpc_client.clone(), config.mcp.control_enabled);

        // Create resource registry with node-specific resources
        let resource_registry = NodeResourceRegistry::new(grpc_client.clone());

        // Create prompt registry with node-specific prompts
        let prompt_registry = NodePromptRegistry::new();

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
    pub async fn start(&self) -> McpResult<()> {
        log::info!("Starting Minotari Node MCP Server");
        self.inner.start().await
    }

    /// Stop the MCP server
    #[allow(dead_code)]
    pub async fn stop(&self) -> McpResult<()> {
        log::info!("Stopping Minotari Node MCP Server");
        self.inner.stop().await
    }

    /// Check if the server is running
    #[allow(dead_code)]
    pub fn is_running(&self) -> bool {
        self.inner.is_running()
    }

    /// Create gRPC client connection to base node
    async fn create_grpc_client(config: &NodeMcpConfig) -> McpResult<BaseNodeGrpcClient<Channel>> {
        let endpoint_url = config.node_grpc_url();
        
        log::info!("Connecting to base node at: {}", endpoint_url);
        
        let endpoint = Endpoint::from_shared(endpoint_url)
            .map_err(|e| McpError::config_error(format!("Invalid gRPC endpoint: {}", e)))?;

        let channel = endpoint
            .timeout(std::time::Duration::from_secs(config.node_grpc.timeout_secs))
            .connect()
            .await
            .map_err(|e| McpError::config_error(format!("Failed to connect to base node: {}", e)))?;

        // Create client (simplified - no authentication for now)
        let client = BaseNodeClient::new(channel);
        
        // TODO: Test the connection with a simple call
        // For now, skip the test call due to tonic version compatibility issues
        
        log::info!("Base node client created (connection test skipped)");
        Ok(client)
    }
}
