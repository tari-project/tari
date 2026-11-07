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
//! Command line interface for the Minotari Wallet MCP Server

use std::path::PathBuf;

use clap::Parser;
use minotari_app_utilities::common_cli_args::CommonCliArgs;
use minotari_mcp_common::{CliConfigBuilder, CliConfigExtractor, LaunchCliConfig, WalletArgumentBuilder};
use tari_common::configuration::{ConfigOverrideProvider, Network};
use tari_utilities::SafePassword;

#[derive(Parser, Debug)]
#[clap(name = "minotari_mcp_wallet")]
#[clap(about = "Minotari Wallet MCP (Model Context Protocol) Server")]
#[clap(version)]
#[allow(clippy::struct_excessive_bools)]
pub struct Cli {
    #[clap(flatten)]
    pub common: CommonCliArgs,

    /// Auto-launch console wallet if not already running
    #[clap(long, env = "MINOTARI_MCP_AUTO_LAUNCH_WALLET", default_value = "true")]
    pub auto_launch_wallet: bool,

    /// Enable MCP control operations (potentially dangerous)
    /// When disabled, only read-only operations are allowed
    #[clap(long, env = "MINOTARI_WALLET_MCP_CONTROL_ENABLED")]
    pub mcp_control_enabled: bool,

    /// Request timeout in seconds
    #[clap(long, env = "MINOTARI_WALLET_MCP_TIMEOUT", default_value = "60")]
    pub mcp_timeout: u64,

    /// Rate limit: maximum requests per minute per client
    #[clap(long, env = "MINOTARI_WALLET_MCP_RATE_LIMIT", default_value = "30")]
    pub mcp_rate_limit: u32,

    /// Enable audit logging of all MCP operations
    #[clap(long, env = "MINOTARI_WALLET_MCP_AUDIT_LOGGING")]
    pub mcp_audit_logging: bool,

    /// Path to audit log file
    #[clap(long, env = "MINOTARI_WALLET_MCP_AUDIT_LOG_PATH")]
    pub mcp_audit_log_path: Option<String>,

    /// Wallet gRPC endpoint
    #[clap(long, env = "MINOTARI_WALLET_GRPC_ADDRESS", default_value = "127.0.0.1:18143")]
    pub wallet_grpc_address: String,

    /// Wallet gRPC timeout in seconds
    #[clap(long, env = "MINOTARI_WALLET_GRPC_TIMEOUT", default_value = "10")]
    pub wallet_grpc_timeout: u64,

    /// Require user confirmation for all value transfers
    #[clap(long, env = "MINOTARI_WALLET_MCP_REQUIRE_CONFIRMATION")]
    pub require_confirmation: bool,

    /// Maximum transaction amount without confirmation in microTari (default: 1_000_000 µT / 1 Tari)
    #[clap(long, env = "MINOTARI_WALLET_MCP_MAX_AUTO_AMOUNT")]
    pub max_auto_amount: Option<u64>,

    // Wallet-specific flags for auto-launch (prefixed to avoid conflicts)
    /// Wallet: Password for the console wallet
    #[clap(long, env = "MINOTARI_MCP_WALLET_PASSWORD", hide_env_values = true)]
    pub wallet_password: Option<SafePassword>,

    /// Wallet: Force wallet recovery
    #[clap(long, env = "MINOTARI_MCP_WALLET_RECOVERY")]
    pub wallet_recovery: bool,

    /// Wallet: Run in non-interactive mode
    #[clap(long, env = "MINOTARI_MCP_WALLET_NON_INTERACTIVE")]
    pub wallet_non_interactive: bool,

    /// Wallet: Enable gRPC
    #[clap(long, env = "MINOTARI_MCP_WALLET_ENABLE_GRPC")]
    pub wallet_grpc_enabled: bool,

    /// Wallet: Alternative gRPC address for launched wallet
    #[clap(long, env = "MINOTARI_MCP_WALLET_ALT_GRPC_ADDRESS")]
    pub wallet_alt_grpc_address: Option<String>,

    /// Wallet: Path to libtor data directory
    #[clap(long, env = "MINOTARI_MCP_WALLET_LIBTOR_DATA_DIR")]
    pub wallet_libtor_data_dir: Option<PathBuf>,
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
        if self.require_confirmation {
            overrides.push(("mcp.require_confirmation".to_string(), "true".to_string()));
        }
        if let Some(max_auto) = self.max_auto_amount {
            overrides.push(("mcp.max_auto_amount".to_string(), max_auto.to_string()));
        }
        overrides.push(("mcp.request_timeout_secs".to_string(), self.mcp_timeout.to_string()));
        overrides.push(("mcp.rate_limit_per_minute".to_string(), self.mcp_rate_limit.to_string()));

        // Wallet gRPC settings
        overrides.push(("wallet.grpc_address".to_string(), self.wallet_grpc_address.clone()));

        // Wallet auto-launch settings (if enabled)
        if self.auto_launch_wallet {
            overrides.push(("mcp.auto_launch_wallet".to_string(), "true".to_string()));
            if self.wallet_grpc_enabled {
                overrides.push(("wallet.grpc_enabled".to_string(), "true".to_string()));
            }
            if let Some(ref alt_addr) = self.wallet_alt_grpc_address {
                overrides.push(("wallet.grpc_address".to_string(), alt_addr.clone()));
            }
            if self.wallet_non_interactive {
                overrides.push(("wallet.non_interactive_mode".to_string(), "true".to_string()));
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
            log::warn!("WALLET MCP control operations are ENABLED - this allows AI agents to spend funds");
            log::warn!("Only enable control operations in fully trusted environments");
            log::warn!("Consider using --require-confirmation for additional safety");
        }

        if self.require_confirmation && self.mcp_control_enabled {
            log::info!("User confirmation required for all value transfers");
        }

        Ok(())
    }

    /// Generate command line arguments for launching the console wallet
    #[allow(dead_code)] // Will be used by auto-launch functionality in future versions
    pub fn generate_wallet_args(&self) -> Vec<String> {
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

        // Note: wallet password is passed via MINOTARI_WALLET_PASSWORD environment variable
        // rather than on argv to prevent process table / procfs leaks.
        if self.wallet_recovery {
            args.push("--recovery".to_string());
        }
        if self.wallet_non_interactive {
            args.push("--non-interactive-mode".to_string());
        }
        if self.wallet_grpc_enabled {
            args.push("--grpc-enabled".to_string());
        }
        if let Some(ref alt_addr) = self.wallet_alt_grpc_address {
            args.push("--grpc-address".to_string());
            args.push(alt_addr.clone());
        }
        if let Some(ref libtor_dir) = self.wallet_libtor_data_dir {
            args.push("-z".to_string());
            args.push(libtor_dir.to_string_lossy().to_string());
        }

        // Add config property overrides
        for (key, value) in &self.common.config_property_overrides {
            args.push("-p".to_string());
            args.push(format!("{key}={value}"));
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

        if let Some(ref password) = self.wallet_password {
            let password_str = String::from_utf8_lossy(password.reveal()).to_string();
            builder = builder.with_environment_var("MINOTARI_WALLET_PASSWORD".to_string(), password_str);
        }

        // Add property overrides
        for (key, value) in &self.common.config_property_overrides {
            builder = builder.with_property_override(key.clone(), value.clone());
        }

        builder.build()
    }

    fn extract_node_args(&self) -> Vec<String> {
        // Not applicable for wallet CLI, return empty
        Vec::new()
    }

    fn extract_wallet_args(&self) -> Vec<String> {
        let config = self.extract_launch_config();
        let mut builder = WalletArgumentBuilder::wallet_args_only(config);

        // Add wallet-specific flags based on CLI arguments
        if self.wallet_grpc_enabled {
            builder = builder.enable_grpc();
        }

        if let Some(ref alt_addr) = self.wallet_alt_grpc_address {
            builder = builder.with_grpc_address(alt_addr.clone());
        }

        if self.wallet_non_interactive {
            builder = builder.non_interactive();
        }

        // Note: wallet_password is passed via MINOTARI_WALLET_PASSWORD env var rather than argv

        if self.wallet_recovery {
            builder = builder.force_recovery();
        }

        if let Some(ref libtor_dir) = self.wallet_libtor_data_dir {
            builder = builder.with_libtor_data_dir(libtor_dir.clone());
        }

        builder.build()
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    #[test]
    fn test_wallet_password_not_on_argv_but_in_env() {
        let cli = Cli::parse_from([
            "minotari_mcp_wallet",
            "--wallet-password",
            "super_secret_wallet_password",
        ]);

        let wallet_args = cli.extract_wallet_args();
        assert!(
            !wallet_args
                .iter()
                .any(|arg| arg.contains("super_secret_wallet_password"))
        );
        assert!(!wallet_args.contains(&"--password".to_string()));

        let gen_args = cli.generate_wallet_args();
        assert!(!gen_args.iter().any(|arg| arg.contains("super_secret_wallet_password")));
        assert!(!gen_args.contains(&"--password".to_string()));

        let launch_config = cli.extract_launch_config();
        assert_eq!(
            launch_config.environment_vars.get("MINOTARI_WALLET_PASSWORD"),
            Some(&"super_secret_wallet_password".to_string())
        );
    }

    #[test]
    fn test_max_auto_amount_cli_parsing() {
        let cli = Cli::parse_from([
            "minotari_mcp_wallet",
            "--require-confirmation",
            "--max-auto-amount",
            "5000000",
        ]);

        assert!(cli.require_confirmation);
        assert_eq!(cli.max_auto_amount, Some(5_000_000));
    }
}
