//! Configuration for MCP servers

use serde::{Deserialize, Serialize};
use std::net::IpAddr;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConfig {
    /// Whether the MCP server is enabled
    pub enabled: bool,
    
    /// Whether control operations are enabled (potentially dangerous)
    pub control_enabled: bool,
    
    /// Bind address for the MCP server (default: 127.0.0.1 for security)
    pub bind_address: IpAddr,
    
    /// Port to bind the MCP server to
    pub port: u16,
    
    /// Maximum number of concurrent connections
    pub max_connections: usize,
    
    /// Request timeout in seconds
    pub request_timeout_secs: u64,
    
    /// Rate limit: maximum requests per minute per client
    pub rate_limit_per_minute: u32,
    
    /// Enable audit logging of all MCP operations
    pub audit_logging: bool,
    
    /// Path to audit log file (if audit_logging is enabled)
    pub audit_log_path: Option<String>,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            control_enabled: false,
            bind_address: "127.0.0.1".parse().unwrap(), // Local only for security
            port: 8080,
            max_connections: 10,
            request_timeout_secs: 30,
            rate_limit_per_minute: 60,
            audit_logging: true,
            audit_log_path: None,
        }
    }
}

impl McpConfig {
    /// Validate the configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.enabled {
            // Enforce local-only binding for security
            if !self.bind_address.is_loopback() {
                return Err("MCP server must bind to loopback address only (127.0.0.1 or ::1) for security".into());
            }

            if self.port == 0 {
                return Err("Port must be specified".into());
            }

            if self.max_connections == 0 {
                return Err("Max connections must be greater than 0".into());
            }

            if self.request_timeout_secs == 0 {
                return Err("Request timeout must be greater than 0".into());
            }
        }

        Ok(())
    }

    /// Check if the server should accept connections
    pub fn should_accept_connections(&self) -> bool {
        self.enabled
    }

    /// Check if control operations are permitted
    pub fn are_control_operations_enabled(&self) -> bool {
        self.enabled && self.control_enabled
    }

    /// Get the full bind address
    pub fn bind_address(&self) -> String {
        format!("{}:{}", self.bind_address, self.port)
    }
}
