//! Node MCP server implementation

use crate::config::NodeMcpConfig;
use crate::tools::NodeToolRegistry;
use crate::resources::NodeResourceRegistry;
use crate::prompts::NodePromptRegistry;
use minotari_mcp_common::{
    McpServer, McpServerBuilder, McpResult, McpError,
    ProcessSupervisor, ProcessType, ProcessUtils, ProcessStatus
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
        // Auto-launch base node if configured and not running
        if config.should_auto_launch_node() {
            Self::ensure_base_node_running(&config).await?;
        }

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

    /// Ensure base node is running, auto-launch if needed
    async fn ensure_base_node_running(config: &NodeMcpConfig) -> McpResult<()> {
        // Extract port from gRPC address
        let port = config.node_grpc.address
            .split(':')
            .next_back()
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(18142);

        // Check if already running
        if ProcessUtils::is_service_running(port).await {
            log::info!("Base node already running on port {}", port);
            return Ok(());
        }

        log::info!("Base node not detected, auto-launching...");

        // Find the minotari_node executable
        let node_executable = Self::find_node_executable()?;
        
        // Build command arguments (this would use CLI args from the config)
        let (executable, args) = ProcessUtils::build_node_command(
            &node_executable,
            "/tmp/tari", // This should come from CLI
            "config/config.toml", // This should come from CLI  
            Some("mainnet"), // This should come from CLI
            true, // Enable gRPC
            true, // Non-interactive
            &[], // Additional args
        );

        // Create and start process supervisor
        let (supervisor, mut status_rx) = ProcessSupervisor::new(
            ProcessType::BaseNode,
            executable,
            args,
            port,
        )?;

        // Start supervisor in background
        tokio::spawn(async move {
            if let Err(e) = supervisor.start().await {
                log::error!("Process supervisor failed: {}", e);
            }
        });

        // Wait for the node to become healthy
        let mut attempts = 0;
        while attempts < 30 {
            if ProcessUtils::is_service_running(port).await {
                log::info!("Base node is now running and healthy");
                return Ok(());
            }
            
            // Check supervisor status
            if let Ok(status) = status_rx.try_recv() {
                log::debug!("Supervisor status: {:?}", status);
                if let ProcessStatus::Failed(err) = status {
                    return Err(McpError::server_error(format!("Failed to start base node: {}", err)));
                }
            }
            
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            attempts += 1;
        }

        Err(McpError::server_error("Base node failed to start within timeout"))
    }

    /// Find the minotari_node executable
    fn find_node_executable() -> McpResult<String> {
        // Try to find the executable in common locations
        let possible_paths = [
            "minotari_node",
            "./minotari_node", 
            "../minotari_node/target/release/minotari_node",
            "../minotari_node/target/debug/minotari_node",
        ];

        for path in &possible_paths {
            if std::path::Path::new(path).exists() {
                return Ok(path.to_string());
            }
        }

        // Try to find in PATH
        if which::which("minotari_node").is_ok() {
            return Ok("minotari_node".to_string());
        }

        Err(McpError::config_error("Could not find minotari_node executable. Please ensure it's in PATH or specify the full path."))
    }
}
