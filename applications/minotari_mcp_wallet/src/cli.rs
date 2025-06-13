//! Command line interface for the Minotari Wallet MCP Server

use clap::Parser;
use minotari_app_utilities::common_cli_args::CommonCliArgs;

use std::path::PathBuf;
use tari_common::configuration::{ConfigOverrideProvider, Network};
use tari_utilities::SafePassword;

#[derive(Parser, Debug)]
#[clap(name = "minotari_mcp_wallet")]
#[clap(about = "Minotari Wallet MCP (Model Context Protocol) Server")]
#[clap(version)]
pub struct Cli {
    #[clap(flatten)]
    pub common: CommonCliArgs,
    
    /// Auto-launch console wallet if not already running
    #[clap(long, env = "MINOTARI_MCP_AUTO_LAUNCH_WALLET")]
    pub auto_launch_wallet: bool,
    
    /// Enable MCP server
    #[clap(long, env = "MINOTARI_WALLET_MCP_ENABLED")]
    pub mcp_enabled: bool,
    
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
        if self.mcp_enabled {
            overrides.push(("mcp.enabled".to_string(), "true".to_string()));
        }
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
    pub fn get_base_path(&self) -> PathBuf {
        self.common.get_base_path()
    }
    
    /// Get log config path with application name
    pub fn log_config_path(&self, app_name: &str) -> PathBuf {
        self.common.log_config_path(app_name)
    }
    
    /// Validate CLI arguments
    pub fn validate(&self) -> Result<(), String> {
        if self.mcp_enabled {
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
        }

        Ok(())
    }
    
    /// Generate command line arguments for launching the console wallet
    #[allow(dead_code)]  // Will be used by auto-launch functionality in future versions
    pub fn generate_wallet_args(&self) -> Vec<String> {
        let mut args = Vec::new();
        
        // Add common args
        args.push("--base-path".to_string());
        args.push(self.common.base_path.clone());
        args.push("--config".to_string());
        args.push(self.common.config.clone());
        
        if let Some(ref network) = self.common.network {
            args.push("--network".to_string());
            args.push(network.to_string());
        }
        
        if let Some(ref log_config) = self.common.log_config {
            args.push("--log-config".to_string());
            args.push(log_config.to_string_lossy().to_string());
        }
        
        // Add wallet-specific flags
        if let Some(ref password) = self.wallet_password {
            args.push("--password".to_string());
            args.push(String::from_utf8_lossy(password.reveal()).to_string());
        }
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
            args.push(format!("{}={}", key, value));
        }
        
        args
    }
}
