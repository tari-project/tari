//! Command line interface for the Minotari Node MCP Server

use clap::Parser;
use std::net::IpAddr;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[clap(name = "minotari_mcp_node")]
#[clap(about = "Minotari Node MCP (Model Context Protocol) Server")]
#[clap(version)]
pub struct Cli {
    /// Base directory path
    #[clap(short, long, env = "TARI_BASE_DIR")]
    pub base_path: Option<PathBuf>,
    
    /// Config file path
    #[clap(short, long, env = "TARI_CONFIG")]
    pub config: Option<PathBuf>,
    
    /// Log config file path
    #[clap(long, env = "TARI_LOG_CONFIG")]
    pub log_config: Option<PathBuf>,
    
    /// Enable MCP server
    #[clap(long, env = "MINOTARI_MCP_ENABLED")]
    pub mcp_enabled: bool,
    
    /// Enable MCP control operations (potentially dangerous)
    /// When disabled, only read-only operations are allowed
    #[clap(long, env = "MINOTARI_MCP_CONTROL_ENABLED")]
    pub mcp_control_enabled: bool,
    
    /// MCP server bind address (default: 127.0.0.1 for security)
    /// Only loopback addresses are permitted for security
    #[clap(long, env = "MINOTARI_MCP_BIND_ADDRESS", default_value = "127.0.0.1")]
    pub mcp_bind_address: IpAddr,
    
    /// MCP server port
    #[clap(long, env = "MINOTARI_MCP_PORT", default_value = "8080")]
    pub mcp_port: u16,
    
    /// Maximum number of concurrent MCP connections
    #[clap(long, env = "MINOTARI_MCP_MAX_CONNECTIONS", default_value = "10")]
    pub mcp_max_connections: usize,
    
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
}

impl Cli {
    /// Get base path with default
    pub fn get_base_path(&self) -> PathBuf {
        self.base_path.clone().unwrap_or_else(|| {
            dirs::data_dir()
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
                .join("tari")
        })
    }
    
    /// Get log config path with application name
    pub fn log_config_path(&self, app_name: &str) -> PathBuf {
        if let Some(ref path) = self.log_config {
            path.clone()
        } else {
            self.get_base_path().join("config").join("log4rs").join(format!("{}.yml", app_name))
        }
    }
    
    /// Validate CLI arguments
    pub fn validate(&self) -> Result<(), String> {
        if self.mcp_enabled {
            // Enforce security constraints
            if !self.mcp_bind_address.is_loopback() {
                return Err("MCP server must bind to loopback address only (127.0.0.1 or ::1) for security".into());
            }

            if self.mcp_port == 0 {
                return Err("MCP port must be specified".into());
            }

            if self.mcp_max_connections == 0 {
                return Err("Max connections must be greater than 0".into());
            }

            if self.mcp_timeout == 0 {
                return Err("Timeout must be greater than 0".into());
            }

            if self.mcp_control_enabled {
                log::warn!("MCP control operations are ENABLED - this allows AI agents to modify blockchain state");
                log::warn!("Only enable control operations in trusted environments");
            }
        }

        Ok(())
    }
}
