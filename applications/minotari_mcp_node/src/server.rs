//! Node MCP server implementation

use crate::config::NodeMcpConfig;
use crate::tools::NodeToolRegistry;
use crate::resources::NodeResourceRegistry;
use crate::prompts::NodePromptRegistry;
use crate::cli::Cli;
use minotari_mcp_common::{
    McpServer, McpServerBuilder, McpResult, McpError,
    ServiceHealthMonitors, TariProcessLauncher,
    ProcessLaunchStatus, CliConfigExtractor, CliIntegrationUtils
};
use minotari_node_grpc_client::{BaseNodeGrpcClient, grpc::base_node_client::BaseNodeClient};
use std::sync::Arc;
use std::time::Duration;
use tonic::transport::{Channel, Endpoint};

/// Minotari Node MCP Server
pub struct NodeMcpServer {
    inner: Box<dyn McpServer>,
    _grpc_client: Arc<BaseNodeGrpcClient<Channel>>,
}

impl NodeMcpServer {
    /// Create a new Node MCP server
    pub async fn new(config: NodeMcpConfig, cli: &Cli) -> McpResult<Self> {
        // Auto-launch base node if configured and not running
        if config.should_auto_launch_node() {
            Self::ensure_base_node_running(&config, cli).await?;
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
    async fn ensure_base_node_running(config: &NodeMcpConfig, cli: &Cli) -> McpResult<()> {
        // Extract port from gRPC address
        let port = CliIntegrationUtils::extract_port_from_address(&config.node_grpc.address)
            .unwrap_or(18142);

        // Check if already running using proper health monitor
        let health_monitor = ServiceHealthMonitors::base_node(&config.node_grpc.address);
        if health_monitor.is_service_ready().await {
            log::info!("Base node already running and healthy on port {}", port);
            return Ok(());
        }

        log::info!("Base node not detected, auto-launching...");

        // Extract proper CLI configuration and arguments
        let launch_config = cli.extract_launch_config();
        let node_args = cli.extract_node_args();

        log::debug!("Launching base node with config: {:?}", launch_config);
        log::debug!("Node arguments: {:?}", node_args);

        // Create process launcher using TariProcessLauncher
        let (launcher, mut status_rx) = TariProcessLauncher::launch_node(
            launch_config.base_path,
            launch_config.config_path,
            launch_config.network,
            config.node_grpc.address.clone(),
            node_args,
        ).await?;

        // Start launcher in background
        let launcher_handle = tokio::spawn(async move {
            match launcher.launch().await {
                Ok(result) => {
                    log::info!("Base node launched successfully: {:?}", result);
                    // Keep the launcher alive to maintain the process
                    loop {
                        if !launcher.is_running().await {
                            log::warn!("Base node process has stopped");
                            break;
                        }
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                }
                Err(e) => {
                    log::error!("Failed to launch base node: {}", e);
                }
            }
        });

        // Wait for the node to become healthy with enhanced monitoring
        let mut attempts = 0;
        const MAX_ATTEMPTS: u32 = 60; // 5 minutes with 5-second intervals
        
        while attempts < MAX_ATTEMPTS {
            // Check if the service is ready using proper health check
            if health_monitor.is_service_ready().await {
                log::info!("Base node is now running and healthy");
                return Ok(());
            }
            
            // Check launcher status
            if let Ok(status) = status_rx.try_recv() {
                log::debug!("Launcher status: {:?}", status);
                match status {
                    ProcessLaunchStatus::Failed(err) => {
                        launcher_handle.abort();
                        return Err(McpError::server_error(format!("Failed to start base node: {}", err)));
                    }
                    ProcessLaunchStatus::Running => {
                        log::info!("Base node process is running, waiting for health checks...");
                    }
                    _ => {}
                }
            }
            
            tokio::time::sleep(Duration::from_secs(5)).await;
            attempts += 1;
        }

        launcher_handle.abort();
        Err(McpError::server_error("Base node failed to become healthy within timeout"))
    }


}
