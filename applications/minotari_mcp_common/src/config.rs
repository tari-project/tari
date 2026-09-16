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
//! Configuration for MCP servers

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConfig {
    /// Whether the MCP server is enabled
    pub enabled: bool,

    /// Whether control operations are enabled (potentially dangerous)
    pub control_enabled: bool,

    /// Request timeout in seconds
    pub request_timeout_secs: u64,

    /// Rate limit: maximum requests per minute per client
    pub rate_limit_per_minute: u32,

    /// Enable audit logging of all MCP operations
    pub audit_logging: bool,

    /// Path to audit log file (if audit_logging is enabled)
    pub audit_log_path: Option<PathBuf>,

    /// Auto-launch configuration
    pub auto_launch: AutoLaunchConfig,

    /// Input sanitization configuration
    pub input_sanitization: InputSanitizationConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoLaunchConfig {
    /// Whether to auto-launch processes if not running
    pub enabled: bool,

    /// Maximum time to wait for process startup (seconds)
    pub startup_timeout_secs: u64,

    /// Retry attempts for process launch
    pub max_retry_attempts: u32,

    /// Delay between retry attempts (seconds)
    pub retry_delay_secs: u64,

    /// Whether to use unique ports for wallet instances
    pub use_unique_ports: bool,

    /// Base port for auto-assigned ports
    pub base_port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputSanitizationConfig {
    /// Maximum string length for inputs
    pub max_string_length: usize,

    /// Maximum array length for inputs
    pub max_array_length: usize,

    /// Maximum object depth for inputs
    pub max_object_depth: usize,

    /// Whether to enable HTML entity cleaning
    pub clean_html_entities: bool,

    /// Whether to validate paths for security
    pub validate_paths: bool,

    /// Whether to enforce Unicode normalization
    pub unicode_normalization: bool,
}

impl Default for AutoLaunchConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            startup_timeout_secs: 90,
            max_retry_attempts: 3,
            retry_delay_secs: 5,
            use_unique_ports: true,
            base_port: 18142, // Default Tari wallet gRPC port + 1
        }
    }
}

impl Default for InputSanitizationConfig {
    fn default() -> Self {
        Self {
            max_string_length: 1024 * 1024, // 1MB
            max_array_length: 10000,
            max_object_depth: 20,
            clean_html_entities: true,
            validate_paths: true,
            unicode_normalization: true,
        }
    }
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            control_enabled: false,
            request_timeout_secs: 30,
            rate_limit_per_minute: 60,
            audit_logging: true,
            audit_log_path: None,
            auto_launch: AutoLaunchConfig::default(),
            input_sanitization: InputSanitizationConfig::default(),
        }
    }
}

impl McpConfig {
    /// Validate the configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.enabled {
            // Basic validation for stdio transport
            if self.request_timeout_secs == 0 {
                return Err("Request timeout must be greater than 0".into());
            }

            if self.rate_limit_per_minute == 0 {
                return Err("Rate limit must be greater than 0".into());
            }

            // Validate auto-launch configuration
            if self.auto_launch.enabled {
                if self.auto_launch.startup_timeout_secs == 0 {
                    return Err("Auto-launch startup timeout must be greater than 0".into());
                }
                if self.auto_launch.max_retry_attempts == 0 {
                    return Err("Auto-launch max retry attempts must be greater than 0".into());
                }
            }

            // Validate input sanitization configuration
            if self.input_sanitization.max_string_length == 0 {
                return Err("Input sanitization max string length must be greater than 0".into());
            }
            if self.input_sanitization.max_array_length == 0 {
                return Err("Input sanitization max array length must be greater than 0".into());
            }
            if self.input_sanitization.max_object_depth == 0 {
                return Err("Input sanitization max object depth must be greater than 0".into());
            }
        }

        Ok(())
    }

    /// Check if the server should start
    pub fn should_start_server(&self) -> bool {
        self.enabled
    }

    /// Check if control operations are permitted
    pub fn are_control_operations_enabled(&self) -> bool {
        self.enabled && self.control_enabled
    }

    /// Check if auto-launch is enabled
    pub fn is_auto_launch_enabled(&self) -> bool {
        self.enabled && self.auto_launch.enabled
    }

    /// Get audit log path with defaults
    pub fn get_audit_log_path(&self) -> Option<PathBuf> {
        if self.audit_logging {
            self.audit_log_path.clone().or_else(|| {
                // Default audit log path
                Some(PathBuf::from("logs/mcp_audit.log"))
            })
        } else {
            None
        }
    }
}
