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
//! Node MCP server implementation

use std::{sync::Arc, time::Duration};

use minotari_mcp_common::{
    AutoDiscoveryConfig,
    AutoDiscoveryRegistry,
    CircuitBreakerConfig,
    CliConfigExtractor,
    CliIntegrationUtils,
    ConnectionManager,
    ConnectionPoolConfig,
    ConversionRegistryFactory,
    GrpcErrorMapper,
    GrpcExecutor,
    HealthConfig,
    McpError,
    McpResult,
    McpServer,
    McpServerBuilder,
    NodeGrpcClientImpl,
    ProcessLaunchStatus,
    ProcessLauncher,
    SchemaGenerator,
    ServerType,
    ServiceDiscovery,
    ServiceHealthMonitors,
    StartupDiagnostics,
    TariProcessLauncher,
};
use minotari_node_grpc_client::{BaseNodeGrpcClient, grpc::base_node_client::BaseNodeClient};
use tonic::transport::{Channel, Endpoint};

use crate::{
    cli::Cli,
    config::NodeMcpConfig,
    prompts::NodePromptRegistry,
    resources::NodeResourceRegistry,
    tools::NodeToolRegistry,
};

/// Wrapper to convert Arc<dyn McpTool> to Box<dyn McpTool>
struct ArcToolWrapper {
    tool: Arc<dyn minotari_mcp_common::McpTool>,
}

impl ArcToolWrapper {
    fn new(tool: Arc<dyn minotari_mcp_common::McpTool>) -> Self {
        Self { tool }
    }
}

#[async_trait::async_trait]
impl minotari_mcp_common::McpTool for ArcToolWrapper {
    fn name(&self) -> &str {
        self.tool.name()
    }

    fn description(&self) -> &str {
        self.tool.description()
    }

    fn permission_level(&self) -> minotari_mcp_common::security::PermissionLevel {
        self.tool.permission_level()
    }

    fn input_schema(&self) -> serde_json::Value {
        self.tool.input_schema()
    }

    async fn execute(&self, args: serde_json::Value) -> minotari_mcp_common::McpResult<serde_json::Value> {
        self.tool.execute(args).await
    }
}

/// Minotari Node MCP Server
pub struct NodeMcpServer {
    inner: Box<dyn McpServer>,
    _grpc_client: Arc<BaseNodeGrpcClient<Channel>>,
    launched_process: Option<Arc<ProcessLauncher>>,
}

impl NodeMcpServer {
    /// Create a new Node MCP server
    pub async fn new(config: NodeMcpConfig, cli: &Cli) -> McpResult<Self> {
        // Auto-launch base node if configured and not running
        let launched_process = if config.should_auto_launch_node() {
            Self::ensure_base_node_running(&config, cli).await?
        } else {
            None
        };

        // Create gRPC client connection to base node
        let grpc_client = Self::create_grpc_client(&config).await?;
        let grpc_client = Arc::new(grpc_client);

        // Create connection manager with health monitoring
        let connection_manager = Self::create_connection_manager(&config).await?;

        // Create health-aware gRPC executor
        let conversion_registry = ConversionRegistryFactory::create_node_registry();
        let error_mapper = Arc::new(GrpcErrorMapper::new());
        let node_client_impl = Arc::new(NodeGrpcClientImpl::new(
            (*grpc_client).clone(),
            conversion_registry.clone(),
        ));
        let grpc_executor = Arc::new(GrpcExecutor::new_node_with_health(
            node_client_impl,
            error_mapper.clone(),
            conversion_registry,
            connection_manager.clone(),
        ));

        // Create auto-discovery registry with health monitoring
        let auto_discovery_config = AutoDiscoveryConfig {
            enabled: true,
            allowed_methods: config.allowed_methods(),
            control_enabled: config.mcp.control_enabled,
            server_type: ServerType::Node,
            rate_limits: std::collections::HashMap::new(),
            tool_overrides: std::collections::HashMap::new(),
        };

        let service_discovery = Arc::new(ServiceDiscovery::new());
        let schema_generator = Arc::new(SchemaGenerator::new(&service_discovery));

        let auto_discovery = AutoDiscoveryRegistry::new_with_executor(
            auto_discovery_config,
            service_discovery,
            schema_generator,
            error_mapper,
            grpc_executor,
        );

        // Initialize auto-discovery
        auto_discovery.initialize().await?;

        // Get tools from auto-discovery (use healthy tools only)
        let discovered_tools = auto_discovery.get_healthy_tools().await;

        // Create tool registry and populate with auto-discovered tools
        let mut tool_registry = minotari_mcp_common::ToolRegistry::new();

        // Convert Arc<dyn McpTool> to Box<dyn McpTool> for registration
        for (name, arc_tool) in discovered_tools {
            log::info!("Registering auto-discovered tool: {name}");

            // Create a wrapper that clones the Arc for each execution
            let arc_clone = arc_tool.clone();
            let boxed_tool = Box::new(ArcToolWrapper::new(arc_clone));
            tool_registry.register(boxed_tool);
        }

        // If no auto-discovered tools, fall back to manual tools
        if tool_registry.list_tools().is_empty() {
            log::warn!("No auto-discovered tools available, falling back to manual registration");
            tool_registry = NodeToolRegistry::new((*grpc_client).clone(), config.mcp.control_enabled);
        }

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
            launched_process,
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

        // First, stop any launched processes
        if let Some(ref process_launcher) = self.launched_process {
            log::info!("Stopping launched base node process...");
            if let Err(e) = process_launcher.stop().await {
                log::warn!("Failed to stop launched base node: {e}");
            }
        }

        // Then stop the MCP server
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

        log::info!("Connecting to base node at: {endpoint_url}");

        let endpoint = Endpoint::from_shared(endpoint_url)
            .map_err(|e| McpError::config_error(format!("Invalid gRPC endpoint: {e}")))?;

        let channel = endpoint
            .timeout(std::time::Duration::from_secs(config.node_grpc.timeout_secs))
            .connect()
            .await
            .map_err(|e| McpError::config_error(format!("Failed to connect to base node: {e}")))?;

        // Create client (simplified - no authentication for now)
        let client = BaseNodeClient::new(channel);

        // TODO: Test the connection with a simple call
        // For now, skip the test call due to tonic version compatibility issues

        log::info!("Base node client created (connection test skipped)");
        Ok(client)
    }

    /// Create connection manager with health monitoring
    async fn create_connection_manager(config: &NodeMcpConfig) -> McpResult<Arc<ConnectionManager>> {
        let pool_config = ConnectionPoolConfig::default();
        let circuit_config = CircuitBreakerConfig::default();
        let health_config = HealthConfig::default();

        let conn_manager = ConnectionManager::new(pool_config, circuit_config, health_config);

        // Add the base node service endpoint
        let endpoint_url = config.node_grpc_url();
        let endpoint = Endpoint::from_shared(endpoint_url)
            .map_err(|e| McpError::config_error(format!("Invalid gRPC endpoint: {e}")))?
            .timeout(Duration::from_secs(config.node_grpc.timeout_secs))
            .connect_timeout(Duration::from_secs(5));

        conn_manager.add_service("base_node".to_string(), endpoint).await?;
        conn_manager.start_maintenance().await?;

        Ok(Arc::new(conn_manager))
    }

    /// Ensure base node is running, auto-launch if needed
    async fn ensure_base_node_running(config: &NodeMcpConfig, cli: &Cli) -> McpResult<Option<Arc<ProcessLauncher>>> {
        // Extract port from gRPC address
        let port = CliIntegrationUtils::extract_port_from_address(&config.node_grpc.address).unwrap_or(18142);

        // Check if already running using proper health monitor
        let health_monitor = ServiceHealthMonitors::base_node(&config.node_grpc.address);
        if health_monitor.is_service_ready().await {
            log::info!("Base node already running and healthy on port {port}");
            return Ok(None); // No process launched by us
        }

        log::info!("Base node not detected, auto-launching...");

        // Extract proper CLI configuration and arguments
        let launch_config = cli.extract_launch_config();
        let node_args = cli.extract_node_args();

        log::info!("Launching base node with config: {launch_config:?}");
        log::info!("Node arguments: {node_args:?}");

        // Create process launcher using TariProcessLauncher
        let (launcher, mut status_rx) = TariProcessLauncher::launch_node(
            launch_config.base_path,
            launch_config.config_path,
            launch_config.network,
            config.node_grpc.address.clone(),
            node_args,
        )
        .await?;

        let launcher = Arc::new(launcher);
        let launcher_for_background = launcher.clone();

        // Start launcher in background
        let launcher_handle = tokio::spawn(async move {
            match launcher_for_background.launch().await {
                Ok(result) => {
                    log::info!("Base node launched successfully: {result:?}");
                    // Keep the launcher alive to maintain the process
                    loop {
                        if !launcher_for_background.is_running().await {
                            log::warn!("Base node process has stopped");
                            break;
                        }
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                },
                Err(e) => {
                    log::error!("Failed to launch base node: {e}");
                },
            }
        });

        // Wait for the node to become healthy with enhanced monitoring
        let mut attempts = 0;
        const MAX_ATTEMPTS: u32 = 60; // 5 minutes with 5-second intervals

        while attempts < MAX_ATTEMPTS {
            // Check if the service is ready using proper health check
            if health_monitor.is_service_ready().await {
                log::info!("Base node is now running and healthy");
                return Ok(Some(launcher));
            }

            // Check launcher status
            if let Ok(status) = status_rx.try_recv() {
                log::debug!("Launcher status: {status:?}");
                match status {
                    ProcessLaunchStatus::Failed(err) => {
                        launcher_handle.abort();
                        return Err(McpError::server_error(format!("Failed to start base node: {err}")));
                    },
                    ProcessLaunchStatus::Running => {
                        log::info!("Base node process is running, waiting for health checks...");
                    },
                    _ => {},
                }
            }

            tokio::time::sleep(Duration::from_secs(5)).await;
            attempts = attempts.saturating_add(1);
        }

        launcher_handle.abort();
        Err(McpError::server_error(
            "Base node failed to become healthy within timeout",
        ))
    }

    /// Run startup diagnostics for troubleshooting
    #[allow(dead_code)]
    pub async fn run_diagnostics(config: &NodeMcpConfig, cli: &Cli) -> String {
        let launch_config = cli.extract_launch_config();

        let diagnostics = StartupDiagnostics::new()
            .with_base_path(launch_config.base_path)
            .with_config_path(launch_config.config_path)
            .with_node_grpc_address(config.node_grpc.address.clone());

        let results = diagnostics.run_diagnostics().await;
        diagnostics.format_diagnostic_report(&results)
    }
}
