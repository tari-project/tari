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
//! Configuration for the Minotari Wallet MCP Server

use minotari_mcp_common::{McpConfig, McpError, McpResult};
use serde::{Deserialize, Serialize};

use crate::cli::Cli;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletMcpConfig {
    /// MCP server configuration
    pub mcp: McpConfig,

    /// Wallet gRPC configuration
    pub wallet_grpc: WalletGrpcConfig,

    /// Wallet-specific security settings
    pub security: WalletSecurityConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletGrpcConfig {
    /// Wallet gRPC endpoint address
    pub address: String,

    /// Connection timeout in seconds
    pub timeout_secs: u64,

    /// Maximum number of retries for failed requests
    pub max_retries: u32,

    /// Whether to auto-launch the wallet
    pub auto_launch: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletSecurityConfig {
    /// Require user confirmation for all value transfers
    pub require_confirmation: bool,

    /// Maximum transaction amount without additional confirmation (in µT)
    pub max_auto_amount: u64,

    /// Enable transaction preview before execution
    pub enable_preview: bool,
}

impl WalletMcpConfig {
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

        let wallet_grpc_config = WalletGrpcConfig {
            address: cli.wallet_grpc_address.clone(),
            timeout_secs: cli.wallet_grpc_timeout,
            max_retries: 3,
            auto_launch: cli.auto_launch_wallet,
        };

        let security_config = WalletSecurityConfig {
            require_confirmation: cli.require_confirmation,
            max_auto_amount: 1_000_000, // 1 Tari in µT
            enable_preview: true,
        };

        let config = Self {
            mcp: mcp_config,
            wallet_grpc: wallet_grpc_config,
            security: security_config,
        };

        // Validate the complete configuration
        config.validate()?;

        Ok(config)
    }

    /// Validate the configuration
    pub fn validate(&self) -> McpResult<()> {
        // Validate MCP configuration
        self.mcp.validate().map_err(McpError::config_error)?;

        // Validate wallet gRPC configuration
        if self.wallet_grpc.address.is_empty() {
            return Err(McpError::config_error("Wallet gRPC address must be specified"));
        }

        if self.wallet_grpc.timeout_secs == 0 {
            return Err(McpError::config_error("Wallet gRPC timeout must be greater than 0"));
        }

        // Validate security configuration
        if self.mcp.control_enabled && !self.security.require_confirmation {
            log::warn!("Control operations enabled without required confirmation - this is potentially dangerous");
        }

        Ok(())
    }

    /// Check if auto-launch is enabled
    pub fn should_auto_launch_wallet(&self) -> bool {
        self.wallet_grpc.auto_launch
    }

    /// Get the wallet gRPC endpoint URL
    pub fn wallet_grpc_url(&self) -> String {
        if self.wallet_grpc.address.starts_with("http://") || self.wallet_grpc.address.starts_with("https://") {
            self.wallet_grpc.address.clone()
        } else {
            format!("http://{}", self.wallet_grpc.address)
        }
    }
}

impl Default for WalletGrpcConfig {
    fn default() -> Self {
        Self {
            address: "127.0.0.1:18143".to_string(),
            timeout_secs: 30,
            max_retries: 3,
            auto_launch: true,
        }
    }
}

impl Default for WalletSecurityConfig {
    fn default() -> Self {
        Self {
            require_confirmation: false,
            max_auto_amount: 1_000_000, // 1 Tari
            enable_preview: true,
        }
    }
}
