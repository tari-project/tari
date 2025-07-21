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
//! Command line interface for the Minotari Node MCP Server

use std::path::PathBuf;

use clap::Parser;
use minotari_app_utilities::common_cli_args::CommonCliArgs;
use minotari_mcp_common::{CliConfigBuilder, CliConfigExtractor, LaunchCliConfig, NodeArgumentBuilder};
use tari_common::configuration::{ConfigOverrideProvider, Network};

#[derive(Parser, Debug)]
#[clap(name = "minotari_mcp_node")]
#[clap(about = "Minotari Node MCP (Model Context Protocol) Server")]
#[clap(version)]
#[allow(clippy::struct_excessive_bools)]
pub struct Cli {
    #[clap(flatten)]
    pub common: CommonCliArgs,

    /// Auto-launch base node if not already running
    #[clap(long, env = "MINOTARI_MCP_AUTO_LAUNCH_NODE", default_value = "true")]
    pub auto_launch_node: bool,

    /// Enable MCP control operations (potentially dangerous)
    /// When disabled, only read-only operations are allowed
    #[clap(long, env = "MINOTARI_MCP_CONTROL_ENABLED")]
    pub mcp_control_enabled: bool,

    /// Request timeout in seconds
    #[clap(long, env = "MINOTARI_MCP_TIMEOUT", default_value = "30")]
    pub mcp_timeout: u64,

    /// Rate limit: maximum requests per minute per client
    #[clap(long, env = "MINOTARI_MCP_RATE_LIMIT", default_value = "60")]
    pub mcp_rate_limit: u32,

    /// Enable audit logging of all MCP operations
    #[clap(long, env = "MINOTARI_MCP_AUDIT_LOGGING")]
    pub mcp_audit_logging: bool,

    /// Path to audit log file
    #[clap(long, env = "MINOTARI_MCP_AUDIT_LOG_PATH")]
    pub mcp_audit_log_path: Option<String>,

    /// Base node gRPC endpoint
    #[clap(long, env = "MINOTARI_NODE_GRPC_ADDRESS", default_value = "127.0.0.1:18142")]
    pub node_grpc_address: String,

    /// Base node gRPC timeout in seconds
    #[clap(long, env = "MINOTARI_NODE_GRPC_TIMEOUT", default_value = "10")]
    pub node_grpc_timeout: u64,

    // Node-specific flags for auto-launch (prefixed to avoid conflicts)
    /// Base node: Create a default configuration file if it doesn't exist
    #[clap(long, env = "MINOTARI_MCP_NODE_INIT")]
    pub node_init: bool,

    /// Base node: Rebuild the database from scratch
    #[clap(long, env = "MINOTARI_MCP_NODE_REBUILD_DB")]
    pub node_rebuild_db: bool,

    /// Base node: Run in non-interactive mode
    #[clap(long, env = "MINOTARI_MCP_NODE_NON_INTERACTIVE")]
    pub node_non_interactive: bool,

    /// Base node: Enable gRPC
    #[clap(long, env = "MINOTARI_MCP_NODE_ENABLE_GRPC")]
    pub node_grpc_enabled: bool,

    /// Base node: Enable mining
    #[clap(long, env = "MINOTARI_MCP_NODE_ENABLE_MINING")]
    pub node_mining_enabled: bool,

    /// Base node: Enable second layer gRPC
    #[clap(long, env = "MINOTARI_MCP_NODE_SECOND_LAYER_GRPC")]
    pub node_second_layer_grpc_enabled: bool,

    /// Base node: Disable splash screen
    #[clap(long, env = "MINOTARI_MCP_NODE_DISABLE_SPLASH")]
    pub node_disable_splash_screen: bool,

    /// Base node: Path to libtor data directory
    #[clap(long, env = "MINOTARI_MCP_NODE_LIBTOR_DATA_DIR")]
    pub node_libtor_data_dir: Option<PathBuf>,
}

impl ConfigOverrideProvider for Cli {
    fn get_config_property_overrides(&self, network: &Network) -> Vec<(String, String)> {
        let mut overrides = self.common.get_config_property_overrides(network);

        // MCP-specific overrides
        overrides.push(("mcp.enabled".to_string(), "true".to_string()));
        if self.mcp_control_enabled {
            overrides.push(("mcp.control_enabled".to_string(), "true".to_string()));
        }
        if self.mcp_audit_logging {
            overrides.push(("mcp.audit_logging".to_string(), "true".to_string()));
        }
        if let Some(ref path) = self.mcp_audit_log_path {
            overrides.push(("mcp.audit_log_path".to_string(), path.clone()));
        }
        overrides.push(("mcp.request_timeout_secs".to_string(), self.mcp_timeout.to_string()));
        overrides.push(("mcp.rate_limit_per_minute".to_string(), self.mcp_rate_limit.to_string()));

        // Node gRPC settings
        overrides.push(("base_node.grpc_address".to_string(), self.node_grpc_address.clone()));

        // Node auto-launch settings (if enabled)
        if self.auto_launch_node {
            overrides.push(("mcp.auto_launch_node".to_string(), "true".to_string()));
            if self.node_grpc_enabled {
                overrides.push(("base_node.grpc_enabled".to_string(), "true".to_string()));
            }
            if self.node_mining_enabled {
                overrides.push(("base_node.mining_enabled".to_string(), "true".to_string()));
            }
            if self.node_second_layer_grpc_enabled {
                overrides.push(("base_node.second_layer_grpc_enabled".to_string(), "true".to_string()));
            }
            if self.node_non_interactive {
                overrides.push(("base_node.non_interactive_mode".to_string(), "true".to_string()));
            }
        }

        overrides
    }
}

impl Cli {
    /// Get base path with default
    #[allow(dead_code)]
    pub fn get_base_path(&self) -> PathBuf {
        self.common.get_base_path()
    }

    /// Get log config path with application name
    #[allow(dead_code)]
    pub fn log_config_path(&self, app_name: &str) -> PathBuf {
        self.common.log_config_path(app_name)
    }

    /// Validate CLI arguments
    pub fn validate(&self) -> Result<(), String> {
        if self.mcp_timeout == 0 {
            return Err("MCP timeout must be greater than 0".into());
        }

        if self.mcp_rate_limit == 0 {
            return Err("MCP rate limit must be greater than 0".into());
        }

        if self.mcp_control_enabled {
            log::warn!("MCP control operations are ENABLED - this allows AI agents to modify blockchain state");
            log::warn!("Only enable control operations in trusted environments");
        }

        Ok(())
    }

    /// Generate command line arguments for launching the base node
    #[allow(dead_code)] // Will be used by auto-launch functionality in future versions
    pub fn generate_node_args(&self) -> Vec<String> {
        // Add common args
        let mut args = vec![
            "--base-path".to_string(),
            self.common.base_path.clone(),
            "--config".to_string(),
            self.common.config.clone(),
        ];

        if let Some(ref network) = self.common.network {
            args.push("--network".to_string());
            args.push(network.to_string());
        }

        if let Some(ref log_config) = self.common.log_config {
            args.push("--log-config".to_string());
            args.push(log_config.to_string_lossy().to_string());
        }

        // Add node-specific flags
        if self.node_init {
            args.push("--init".to_string());
        }
        if self.node_rebuild_db {
            args.push("--rebuild-db".to_string());
        }
        if self.node_non_interactive {
            args.push("--non-interactive-mode".to_string());
        }
        if self.node_grpc_enabled {
            args.push("--grpc-enabled".to_string());
        }
        if self.node_mining_enabled {
            args.push("--mining-enabled".to_string());
        }
        if self.node_second_layer_grpc_enabled {
            args.push("--second-layer-grpc-enabled".to_string());
        }
        if self.node_disable_splash_screen {
            args.push("--disable-splash-screen".to_string());
        }
        if let Some(ref libtor_dir) = self.node_libtor_data_dir {
            args.push("-z".to_string());
            args.push(libtor_dir.to_string_lossy().to_string());
        }

        // Add config property overrides
        for (key, value) in &self.common.config_property_overrides {
            args.push("-p".to_string());
            args.push(format!("{}={}", key, value));
        }

        args
    }
}

impl CliConfigExtractor for Cli {
    fn extract_launch_config(&self) -> LaunchCliConfig {
        let mut builder = CliConfigBuilder::new()
            .with_base_path(self.common.base_path.clone())
            .with_config_path(self.common.config.clone());

        if let Some(ref network) = self.common.network {
            builder = builder.with_network(network.to_string());
        }

        if let Some(ref log_config) = self.common.log_config {
            builder = builder.with_log_config(log_config.clone());
        }

        // Add property overrides
        for (key, value) in &self.common.config_property_overrides {
            builder = builder.with_property_override(key.clone(), value.clone());
        }

        builder.build()
    }

    fn extract_node_args(&self) -> Vec<String> {
        let config = self.extract_launch_config();
        let mut builder = NodeArgumentBuilder::node_args_only(config);

        // Add node-specific flags based on CLI arguments
        if self.node_grpc_enabled {
            builder = builder.enable_grpc();
        }

        if self.node_mining_enabled {
            builder = builder.enable_mining();
        }

        if self.node_second_layer_grpc_enabled {
            builder = builder.enable_second_layer_grpc();
        }

        if self.node_non_interactive {
            builder = builder.non_interactive();
        }

        if self.node_disable_splash_screen {
            builder = builder.disable_splash();
        }

        if let Some(ref libtor_dir) = self.node_libtor_data_dir {
            builder = builder.with_libtor_data_dir(libtor_dir.clone());
        }

        // Add init flag if specified
        if self.node_init {
            builder = builder.with_custom_arg("--init".to_string());
        }

        // Add rebuild-db flag if specified
        if self.node_rebuild_db {
            builder = builder.with_custom_arg("--rebuild-db".to_string());
        }

        builder.build()
    }

    fn extract_wallet_args(&self) -> Vec<String> {
        // Not applicable for node CLI, return empty
        Vec::new()
    }
}
