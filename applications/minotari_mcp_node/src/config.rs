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
//! Configuration for the Minotari Node MCP Server

use minotari_mcp_common::{McpConfig, McpError, McpResult};
use serde::{Deserialize, Serialize};

use crate::cli::Cli;

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

    /// Whether to auto-launch the base node
    pub auto_launch: bool,
}

impl NodeMcpConfig {
    /// Load configuration from CLI arguments
    pub fn load(cli: &Cli) -> McpResult<Self> {
        // Validate CLI arguments first
        cli.validate().map_err(McpError::config_error)?;

        let mcp_config = McpConfig {
            enabled: true,
            control_enabled: cli.mcp_control_enabled,
            request_timeout_secs: cli.mcp_timeout,
            rate_limit_per_minute: cli.mcp_rate_limit,
            audit_logging: cli.mcp_audit_logging,
            audit_log_path: cli.mcp_audit_log_path.clone().map(|s| s.into()),
            auto_launch: minotari_mcp_common::config::AutoLaunchConfig::default(),
            input_sanitization: minotari_mcp_common::config::InputSanitizationConfig::default(),
        };

        let node_grpc_config = NodeGrpcConfig {
            address: cli.node_grpc_address.clone(),
            timeout_secs: cli.node_grpc_timeout,
            max_retries: 3,
            auto_launch: cli.auto_launch_node,
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
        self.mcp.validate().map_err(McpError::config_error)?;

        // Validate node gRPC configuration
        if self.node_grpc.address.is_empty() {
            return Err(McpError::config_error("Node gRPC address must be specified"));
        }

        if self.node_grpc.timeout_secs == 0 {
            return Err(McpError::config_error("Node gRPC timeout must be greater than 0"));
        }

        Ok(())
    }

    /// Check if auto-launch is enabled
    pub fn should_auto_launch_node(&self) -> bool {
        self.node_grpc.auto_launch
    }

    /// Get allowed gRPC methods for auto-discovery
    pub fn allowed_methods(&self) -> std::collections::HashSet<String> {
        // For now, allow all methods (empty set means all allowed)
        // In the future, this could be configurable via CLI or config file
        std::collections::HashSet::new()
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
            auto_launch: true,
        }
    }
}
