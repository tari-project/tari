//! Configuration for the Minotari Node MCP Server

use crate::cli::Cli;
use minotari_mcp_common::{McpConfig, McpResult, McpError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeMcpConfig {
    /// MCP server configuration
    pub mcp: McpConfig,
    
    /// Base node gRPC configuration
    pub node_grpc: NodeGrpcConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeGrpcConfig {
    /// Base node gRPC endpoint address
    pub address: String,
    
    /// Connection timeout in seconds
    pub timeout_secs: u64,
    
    /// Maximum number of retries for failed requests
    pub max_retries: u32,
}

impl NodeMcpConfig {
    /// Load configuration from CLI arguments
    pub fn load(cli: &Cli) -> McpResult<Self> {
        // Validate CLI arguments first
        cli.validate()
            .map_err(|e| McpError::config_error(e))?;

        let mcp_config = McpConfig {
            enabled: cli.mcp_enabled,
            control_enabled: cli.mcp_control_enabled,
            bind_address: cli.mcp_bind_address,
            port: cli.mcp_port,
            max_connections: cli.mcp_max_connections,
            request_timeout_secs: cli.mcp_timeout,
            rate_limit_per_minute: cli.mcp_rate_limit,
            audit_logging: cli.mcp_audit_logging,
            audit_log_path: cli.mcp_audit_log_path.clone(),
        };

        let node_grpc_config = NodeGrpcConfig {
            address: cli.node_grpc_address.clone(),
            timeout_secs: 30,
            max_retries: 3,
        };

        let config = Self {
            mcp: mcp_config,
            node_grpc: node_grpc_config,
        };

        // Validate the complete configuration
        config.validate()?;

        Ok(config)
    }

    /// Validate the configuration
    pub fn validate(&self) -> McpResult<()> {
        // Validate MCP configuration
        self.mcp.validate()
            .map_err(|e| McpError::config_error(e))?;

        // Validate node gRPC configuration
        if self.node_grpc.address.is_empty() {
            return Err(McpError::config_error("Node gRPC address must be specified"));
        }

        if self.node_grpc.timeout_secs == 0 {
            return Err(McpError::config_error("Node gRPC timeout must be greater than 0"));
        }

        Ok(())
    }

    /// Get the node gRPC endpoint URL
    pub fn node_grpc_url(&self) -> String {
        if self.node_grpc.address.starts_with("http://") || self.node_grpc.address.starts_with("https://") {
            self.node_grpc.address.clone()
        } else {
            format!("http://{}", self.node_grpc.address)
        }
    }
}

impl Default for NodeGrpcConfig {
    fn default() -> Self {
        Self {
            address: "127.0.0.1:18142".to_string(),
            timeout_secs: 30,
            max_retries: 3,
        }
    }
}
