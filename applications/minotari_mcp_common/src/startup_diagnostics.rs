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
//! Startup diagnostics and user guidance for MCP servers
//!
//! Provides comprehensive diagnostic tools for troubleshooting MCP server startup
//! issues, including process detection, configuration validation, and user guidance
//! for common problems.

use std::{path::Path, process::Command, time::Duration};

use tokio::time::timeout;

use crate::{
    cli_integration::CliIntegrationUtils,
    executable_finder::TariExecutables,
    health_monitor::ServiceHealthMonitors,
};

/// Diagnostic result for startup issues
#[derive(Debug, Clone)]
pub struct DiagnosticResult {
    pub component: String,
    pub status: DiagnosticStatus,
    pub message: String,
    pub suggestions: Vec<String>,
    pub details: Option<String>,
}

/// Status of a diagnostic check
#[derive(Debug, Clone, PartialEq)]
pub enum DiagnosticStatus {
    /// Component is working correctly
    Healthy,
    /// Component has a warning but is functional
    Warning,
    /// Component has an error that needs attention
    Error,
    /// Component status could not be determined
    Unknown,
}

/// Comprehensive startup diagnostics system
pub struct StartupDiagnostics {
    network: Option<String>,
    base_path: String,
    config_path: String,
    node_grpc_address: Option<String>,
    wallet_grpc_address: Option<String>,
}

impl StartupDiagnostics {
    /// Create new startup diagnostics
    pub fn new() -> Self {
        Self {
            network: None,
            base_path: "/tmp/tari".to_string(),
            config_path: "config/config.toml".to_string(),
            node_grpc_address: None,
            wallet_grpc_address: None,
        }
    }

    /// Configure diagnostics with specific settings
    pub fn with_network(mut self, network: String) -> Self {
        self.network = Some(network);
        self
    }

    pub fn with_base_path(mut self, base_path: String) -> Self {
        self.base_path = base_path;
        self
    }

    pub fn with_config_path(mut self, config_path: String) -> Self {
        self.config_path = config_path;
        self
    }

    pub fn with_node_grpc_address(mut self, address: String) -> Self {
        self.node_grpc_address = Some(address);
        self
    }

    pub fn with_wallet_grpc_address(mut self, address: String) -> Self {
        self.wallet_grpc_address = Some(address);
        self
    }

    /// Run comprehensive startup diagnostics
    pub async fn run_diagnostics(&self) -> Vec<DiagnosticResult> {
        let mut results = Vec::new();

        // Check executable availability
        results.extend(self.check_executables().await);

        // Check configuration files
        results.extend(self.check_configuration());

        // Check directory structure
        results.extend(self.check_directories());

        // Check network configuration and ports
        results.extend(self.check_network_configuration().await);

        // Check running services
        results.extend(self.check_running_services().await);

        // Check system requirements
        results.extend(self.check_system_requirements());

        results
    }

    /// Check if required executables are available
    async fn check_executables(&self) -> Vec<DiagnosticResult> {
        let mut results = Vec::new();

        // Check node executable
        match TariExecutables::find_node() {
            Ok(path) => {
                results.push(DiagnosticResult {
                    component: "minotari_node".to_string(),
                    status: DiagnosticStatus::Healthy,
                    message: format!("Found at: {}", path.display()),
                    suggestions: vec![],
                    details: None,
                });
            },
            Err(e) => {
                results.push(DiagnosticResult {
                    component: "minotari_node".to_string(),
                    status: DiagnosticStatus::Error,
                    message: "Executable not found".to_string(),
                    suggestions: vec![
                        "Install with: cargo install minotari".to_string(),
                        "Build from source: cargo build --release".to_string(),
                        "Add to PATH or specify full path".to_string(),
                    ],
                    details: Some(e.to_string()),
                });
            },
        }

        // Check wallet executable
        match TariExecutables::find_wallet() {
            Ok(path) => {
                results.push(DiagnosticResult {
                    component: "minotari_console_wallet".to_string(),
                    status: DiagnosticStatus::Healthy,
                    message: format!("Found at: {}", path.display()),
                    suggestions: vec![],
                    details: None,
                });
            },
            Err(e) => {
                results.push(DiagnosticResult {
                    component: "minotari_console_wallet".to_string(),
                    status: DiagnosticStatus::Error,
                    message: "Executable not found".to_string(),
                    suggestions: vec![
                        "Install with: cargo install minotari".to_string(),
                        "Build from source: cargo build --release".to_string(),
                        "Add to PATH or specify full path".to_string(),
                    ],
                    details: Some(e.to_string()),
                });
            },
        }

        results
    }

    /// Check configuration files
    fn check_configuration(&self) -> Vec<DiagnosticResult> {
        let mut results = Vec::new();

        // Check if config file exists
        let config_path = Path::new(&self.config_path);
        if config_path.exists() {
            results.push(DiagnosticResult {
                component: "Configuration File".to_string(),
                status: DiagnosticStatus::Healthy,
                message: format!("Found at: {}", config_path.display()),
                suggestions: vec![],
                details: None,
            });
        } else {
            results.push(DiagnosticResult {
                component: "Configuration File".to_string(),
                status: DiagnosticStatus::Warning,
                message: format!("Not found at: {}", config_path.display()),
                suggestions: vec![
                    "Create default config with --init flag".to_string(),
                    "Copy sample config from docs".to_string(),
                    "Run minotari_node --init to generate config".to_string(),
                ],
                details: Some("Will use default settings if not provided".to_string()),
            });
        }

        // Check base path
        let base_path = Path::new(&self.base_path);
        if base_path.exists() {
            if base_path.is_dir() {
                results.push(DiagnosticResult {
                    component: "Base Directory".to_string(),
                    status: DiagnosticStatus::Healthy,
                    message: format!("Found at: {}", base_path.display()),
                    suggestions: vec![],
                    details: None,
                });
            } else {
                results.push(DiagnosticResult {
                    component: "Base Directory".to_string(),
                    status: DiagnosticStatus::Error,
                    message: format!("Path exists but is not a directory: {}", base_path.display()),
                    suggestions: vec![
                        "Remove the file and create directory".to_string(),
                        "Choose a different base path".to_string(),
                    ],
                    details: None,
                });
            }
        } else {
            results.push(DiagnosticResult {
                component: "Base Directory".to_string(),
                status: DiagnosticStatus::Warning,
                message: format!("Will be created at: {}", base_path.display()),
                suggestions: vec!["Ensure parent directory exists and is writable".to_string()],
                details: Some("Directory will be created automatically on first run".to_string()),
            });
        }

        results
    }

    /// Check directory structure and permissions
    fn check_directories(&self) -> Vec<DiagnosticResult> {
        let mut results = Vec::new();

        // Check if base path is writable
        let base_path = Path::new(&self.base_path);
        if let Some(parent) = base_path.parent() &&
            parent.exists()
        {
            // Try to create a test file to check write permissions
            let test_file = parent.join(".tari_mcp_test");
            match std::fs::write(&test_file, "test") {
                Ok(_) => {
                    drop(std::fs::remove_file(&test_file)); // Clean up
                    results.push(DiagnosticResult {
                        component: "Directory Permissions".to_string(),
                        status: DiagnosticStatus::Healthy,
                        message: "Base path is writable".to_string(),
                        suggestions: vec![],
                        details: None,
                    });
                },
                Err(e) => {
                    results.push(DiagnosticResult {
                        component: "Directory Permissions".to_string(),
                        status: DiagnosticStatus::Error,
                        message: "Base path is not writable".to_string(),
                        suggestions: vec![
                            format!("Check permissions on: {}", parent.display()),
                            "Choose a different base path".to_string(),
                            "Run with elevated permissions if needed".to_string(),
                        ],
                        details: Some(e.to_string()),
                    });
                },
            }
        }

        results
    }

    /// Check network configuration and port availability
    async fn check_network_configuration(&self) -> Vec<DiagnosticResult> {
        let mut results = Vec::new();

        // Check node gRPC port
        if let Some(ref address) = self.node_grpc_address {
            if CliIntegrationUtils::validate_grpc_address(address) {
                results.push(DiagnosticResult {
                    component: "Node gRPC Address".to_string(),
                    status: DiagnosticStatus::Healthy,
                    message: format!("Valid address: {address}"),
                    suggestions: vec![],
                    details: None,
                });

                // Check port availability
                if let Some(port) = CliIntegrationUtils::extract_port_from_address(address) {
                    if CliIntegrationUtils::find_available_port(port) == Some(port) {
                        results.push(DiagnosticResult {
                            component: "Node gRPC Port".to_string(),
                            status: DiagnosticStatus::Healthy,
                            message: format!("Port {port} is available"),
                            suggestions: vec![],
                            details: None,
                        });
                    } else {
                        results.push(DiagnosticResult {
                            component: "Node gRPC Port".to_string(),
                            status: DiagnosticStatus::Warning,
                            message: format!("Port {port} is in use"),
                            suggestions: vec![
                                "Check if node is already running".to_string(),
                                "Use a different port".to_string(),
                                "Stop the service using the port".to_string(),
                            ],
                            details: Some("Auto-launch will find an alternative port".to_string()),
                        });
                    }
                }
            } else {
                results.push(DiagnosticResult {
                    component: "Node gRPC Address".to_string(),
                    status: DiagnosticStatus::Error,
                    message: format!("Invalid address format: {address}"),
                    suggestions: vec![
                        "Use format: host:port".to_string(),
                        "Example: 127.0.0.1:18142".to_string(),
                    ],
                    details: None,
                });
            }
        }

        // Check wallet gRPC port
        if let Some(ref address) = self.wallet_grpc_address {
            if CliIntegrationUtils::validate_grpc_address(address) {
                results.push(DiagnosticResult {
                    component: "Wallet gRPC Address".to_string(),
                    status: DiagnosticStatus::Healthy,
                    message: format!("Valid address: {address}"),
                    suggestions: vec![],
                    details: None,
                });

                // Check port availability
                if let Some(port) = CliIntegrationUtils::extract_port_from_address(address) {
                    if CliIntegrationUtils::find_available_port(port) == Some(port) {
                        results.push(DiagnosticResult {
                            component: "Wallet gRPC Port".to_string(),
                            status: DiagnosticStatus::Healthy,
                            message: format!("Port {port} is available"),
                            suggestions: vec![],
                            details: None,
                        });
                    } else {
                        results.push(DiagnosticResult {
                            component: "Wallet gRPC Port".to_string(),
                            status: DiagnosticStatus::Warning,
                            message: format!("Port {port} is in use"),
                            suggestions: vec![
                                "Check if wallet is already running".to_string(),
                                "Use a different port".to_string(),
                                "Stop the service using the port".to_string(),
                            ],
                            details: Some("Auto-launch will find an alternative port".to_string()),
                        });
                    }
                }
            } else {
                results.push(DiagnosticResult {
                    component: "Wallet gRPC Address".to_string(),
                    status: DiagnosticStatus::Error,
                    message: format!("Invalid address format: {address}"),
                    suggestions: vec![
                        "Use format: host:port".to_string(),
                        "Example: 127.0.0.1:18143".to_string(),
                    ],
                    details: None,
                });
            }
        }

        results
    }

    /// Check for running services
    async fn check_running_services(&self) -> Vec<DiagnosticResult> {
        let mut results = Vec::new();

        // Check if node is running
        if let Some(ref address) = self.node_grpc_address {
            let health_monitor = ServiceHealthMonitors::base_node(address);

            match timeout(Duration::from_secs(5), health_monitor.is_service_ready()).await {
                Ok(true) => {
                    results.push(DiagnosticResult {
                        component: "Base Node Service".to_string(),
                        status: DiagnosticStatus::Healthy,
                        message: format!("Running and healthy at {address}"),
                        suggestions: vec!["No action needed - will use existing service".to_string()],
                        details: None,
                    });
                },
                Ok(false) => {
                    results.push(DiagnosticResult {
                        component: "Base Node Service".to_string(),
                        status: DiagnosticStatus::Warning,
                        message: "Not running".to_string(),
                        suggestions: vec![
                            "Will be auto-launched if configured".to_string(),
                            "Start manually with minotari_node".to_string(),
                        ],
                        details: None,
                    });
                },
                Err(_) => {
                    results.push(DiagnosticResult {
                        component: "Base Node Service".to_string(),
                        status: DiagnosticStatus::Unknown,
                        message: "Health check timed out".to_string(),
                        suggestions: vec![
                            "Check network connectivity".to_string(),
                            "Verify gRPC address is correct".to_string(),
                        ],
                        details: None,
                    });
                },
            }
        }

        // Check if wallet is running
        if let Some(ref address) = self.wallet_grpc_address {
            let health_monitor = ServiceHealthMonitors::wallet(address);

            match timeout(Duration::from_secs(5), health_monitor.is_service_ready()).await {
                Ok(true) => {
                    results.push(DiagnosticResult {
                        component: "Wallet Service".to_string(),
                        status: DiagnosticStatus::Healthy,
                        message: format!("Running and healthy at {address}"),
                        suggestions: vec!["No action needed - will use existing service".to_string()],
                        details: None,
                    });
                },
                Ok(false) => {
                    results.push(DiagnosticResult {
                        component: "Wallet Service".to_string(),
                        status: DiagnosticStatus::Warning,
                        message: "Not running".to_string(),
                        suggestions: vec![
                            "Will be auto-launched if configured".to_string(),
                            "Start manually with minotari_console_wallet".to_string(),
                        ],
                        details: None,
                    });
                },
                Err(_) => {
                    results.push(DiagnosticResult {
                        component: "Wallet Service".to_string(),
                        status: DiagnosticStatus::Unknown,
                        message: "Health check timed out".to_string(),
                        suggestions: vec![
                            "Check network connectivity".to_string(),
                            "Verify gRPC address is correct".to_string(),
                        ],
                        details: None,
                    });
                },
            }
        }

        results
    }

    /// Check system requirements
    fn check_system_requirements(&self) -> Vec<DiagnosticResult> {
        let mut results = Vec::new();

        // Check disk space (basic check)
        if let Ok(_metadata) = std::fs::metadata(&self.base_path).or_else(|_| {
            // If base path doesn't exist, check parent directory
            std::fs::metadata(Path::new(&self.base_path).parent().unwrap_or(Path::new("/")))
        }) {
            // This is a simplified check - in production you'd want to check actual available space
            results.push(DiagnosticResult {
                component: "Disk Space".to_string(),
                status: DiagnosticStatus::Healthy,
                message: "Base path is accessible".to_string(),
                suggestions: vec!["Ensure sufficient disk space (>10GB recommended)".to_string()],
                details: None,
            });
        } else {
            results.push(DiagnosticResult {
                component: "Disk Space".to_string(),
                status: DiagnosticStatus::Error,
                message: "Cannot access base path".to_string(),
                suggestions: vec![
                    "Check that the path exists and is accessible".to_string(),
                    "Verify file system permissions".to_string(),
                ],
                details: None,
            });
        }

        // Check if we can run basic commands
        match Command::new("which").arg("cargo").output() {
            Ok(_) => {
                results.push(DiagnosticResult {
                    component: "Build Environment".to_string(),
                    status: DiagnosticStatus::Healthy,
                    message: "Cargo is available".to_string(),
                    suggestions: vec![],
                    details: None,
                });
            },
            Err(_) => {
                results.push(DiagnosticResult {
                    component: "Build Environment".to_string(),
                    status: DiagnosticStatus::Warning,
                    message: "Cargo not found in PATH".to_string(),
                    suggestions: vec![
                        "Install Rust and Cargo".to_string(),
                        "Not required if binaries are pre-built".to_string(),
                    ],
                    details: None,
                });
            },
        }

        results
    }

    /// Generate a comprehensive diagnostic report
    pub fn format_diagnostic_report(&self, results: &[DiagnosticResult]) -> String {
        let mut report = String::new();

        report.push_str("=== Tari MCP Server Startup Diagnostics ===\n\n");

        let healthy_count = results.iter().filter(|r| r.status == DiagnosticStatus::Healthy).count();
        let warning_count = results.iter().filter(|r| r.status == DiagnosticStatus::Warning).count();
        let error_count = results.iter().filter(|r| r.status == DiagnosticStatus::Error).count();
        let unknown_count = results.iter().filter(|r| r.status == DiagnosticStatus::Unknown).count();

        report.push_str(&format!(
            "Summary: {healthy_count} healthy, {warning_count} warnings, {error_count} errors, {unknown_count} \
             unknown\n\n"
        ));

        // Group results by status
        for status in [
            DiagnosticStatus::Error,
            DiagnosticStatus::Warning,
            DiagnosticStatus::Healthy,
            DiagnosticStatus::Unknown,
        ] {
            let status_results: Vec<_> = results.iter().filter(|r| r.status == status).collect();
            if status_results.is_empty() {
                continue;
            }

            let status_name = match status {
                DiagnosticStatus::Error => "🔴 ERRORS",
                DiagnosticStatus::Warning => "🟡 WARNINGS",
                DiagnosticStatus::Healthy => "🟢 HEALTHY",
                DiagnosticStatus::Unknown => "🔶 UNKNOWN",
            };

            report.push_str(&format!("{status_name}\n"));
            report.push_str(&format!("{}\n", "=".repeat(status_name.len())));

            for result in status_results {
                report.push_str(&format!("• {}: {}\n", result.component, result.message));

                if !result.suggestions.is_empty() {
                    report.push_str("  Suggestions:\n");
                    for suggestion in &result.suggestions {
                        report.push_str(&format!("    - {suggestion}\n"));
                    }
                }

                if let Some(ref details) = result.details {
                    report.push_str(&format!("  Details: {details}\n"));
                }

                report.push('\n');
            }
        }

        if error_count > 0 {
            report.push_str("❌ Action required to resolve errors before startup.\n");
        } else if warning_count > 0 {
            report.push_str("⚠️  Startup should work, but warnings should be addressed.\n");
        } else {
            report.push_str("✅ All checks passed - ready for startup!\n");
        }

        report
    }
}

impl Default for StartupDiagnostics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnostic_result_creation() {
        let result = DiagnosticResult {
            component: "test".to_string(),
            status: DiagnosticStatus::Healthy,
            message: "test message".to_string(),
            suggestions: vec!["suggestion".to_string()],
            details: None,
        };

        assert_eq!(result.component, "test");
        assert_eq!(result.status, DiagnosticStatus::Healthy);
    }

    #[test]
    fn test_diagnostics_builder() {
        let diagnostics = StartupDiagnostics::new()
            .with_network("esmeralda".to_string())
            .with_base_path("/custom/path".to_string());

        assert_eq!(diagnostics.network, Some("esmeralda".to_string()));
        assert_eq!(diagnostics.base_path, "/custom/path");
    }

    #[test]
    fn test_diagnostic_status_equality() {
        assert_eq!(DiagnosticStatus::Healthy, DiagnosticStatus::Healthy);
        assert_ne!(DiagnosticStatus::Healthy, DiagnosticStatus::Error);
    }
}
