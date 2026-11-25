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

use std::{path::PathBuf, time::Duration};

use minotari_mcp_common::{McpConfig, McpError, McpResult};
use minotari_node_grpc_client::{GrpcAuthentication, GrpcClientConfig, GrpcTlsConfig};
use serde::{Deserialize, Serialize};
use tari_utilities::SafePassword;

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

    /// Basic auth username; set together with `password`
    pub username: Option<String>,

    /// Basic auth password. Never serialized: `SafePassword` serializes the plain bytes.
    #[serde(skip_serializing)]
    pub password: Option<SafePassword>,

    /// Domain name to verify the node's TLS certificate against; set together with `ca_cert_path`
    pub tls_domain_name: Option<String>,

    /// PEM file of the CA that signed the node's TLS certificate
    pub ca_cert_path: Option<PathBuf>,
}

const PASSWORD_ENV: &str = "MINOTARI_NODE_GRPC_PASSWORD";

impl NodeMcpConfig {
    /// Load configuration from CLI arguments and the `MINOTARI_NODE_GRPC_PASSWORD` environment variable
    pub fn load(cli: &Cli) -> McpResult<Self> {
        Self::load_with_env(cli, |key| std::env::var(key).ok())
    }

    fn load_with_env(cli: &Cli, env: impl Fn(&str) -> Option<String>) -> McpResult<Self> {
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
            username: cli.node_grpc_username.clone(),
            password: load_password(env(PASSWORD_ENV), cli.node_grpc_password_file.as_ref())?,
            tls_domain_name: cli.node_grpc_tls_domain_name.clone(),
            ca_cert_path: cli.node_grpc_ca_cert.clone(),
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

        if self.node_grpc.username.is_some() != self.node_grpc.password.is_some() {
            return Err(McpError::config_error(
                "Node gRPC username and password must be set together",
            ));
        }

        if self.node_grpc.tls_domain_name.is_some() != self.node_grpc.ca_cert_path.is_some() {
            return Err(McpError::config_error(
                "Node gRPC TLS domain name and CA certificate must be set together",
            ));
        }

        if self.node_grpc.tls_domain_name.is_some() && self.node_grpc.address.starts_with("http://") {
            return Err(McpError::config_error(
                "Node gRPC TLS is configured but the address uses http://; use https://",
            ));
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
        } else if self.node_grpc.tls_domain_name.is_some() {
            format!("https://{}", self.node_grpc.address)
        } else {
            format!("http://{}", self.node_grpc.address)
        }
    }

    /// The client settings for the base node connection. Reads the CA certificate.
    pub fn grpc_client_config(&self) -> McpResult<GrpcClientConfig> {
        let node = &self.node_grpc;
        let auth = match (&node.username, &node.password) {
            (Some(username), Some(password)) => GrpcAuthentication::Basic {
                username: username.clone(),
                password: password.clone(),
            },
            _ => GrpcAuthentication::None,
        };
        let tls = match (&node.tls_domain_name, &node.ca_cert_path) {
            (Some(domain_name), Some(ca_cert_path)) => Some(GrpcTlsConfig {
                ca_cert_pem: std::fs::read(ca_cert_path).map_err(|e| {
                    McpError::config_error(format!(
                        "Could not read CA certificate '{}': {e}",
                        ca_cert_path.display()
                    ))
                })?,
                domain_name: domain_name.clone(),
            }),
            _ => None,
        };
        Ok(GrpcClientConfig {
            auth,
            tls,
            request_timeout: Some(Duration::from_secs(node.timeout_secs)),
            ..Default::default()
        })
    }
}

/// The password comes from the environment or a file, never a command line flag, so it is not visible in the process
/// list.
fn load_password(env_password: Option<String>, password_file: Option<&PathBuf>) -> McpResult<Option<SafePassword>> {
    match (env_password, password_file) {
        (Some(_), Some(_)) => Err(McpError::config_error(format!(
            "Set only one of {PASSWORD_ENV} and the node gRPC password file"
        ))),
        (Some(password), None) => Ok(Some(SafePassword::from(password))),
        (None, Some(path)) => {
            warn_if_readable_by_others(path);
            let password = std::fs::read_to_string(path).map_err(|e| {
                McpError::config_error(format!(
                    "Could not read node gRPC password file '{}': {e}",
                    path.display()
                ))
            })?;
            Ok(Some(SafePassword::from(password.trim_end_matches(['\r', '\n']))))
        },
        (None, None) => Ok(None),
    }
}

#[cfg(unix)]
fn warn_if_readable_by_others(path: &PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(metadata) = std::fs::metadata(path) &&
        metadata.permissions().mode() & 0o044 != 0
    {
        log::warn!(
            "Node gRPC password file '{}' is readable by group or others; restrict it with chmod 600",
            path.display()
        );
    }
}

#[cfg(not(unix))]
fn warn_if_readable_by_others(_path: &PathBuf) {}

impl Default for NodeGrpcConfig {
    fn default() -> Self {
        Self {
            address: "127.0.0.1:18142".to_string(),
            timeout_secs: 30,
            max_retries: 3,
            auto_launch: true,
            username: None,
            password: None,
            tls_domain_name: None,
            ca_cert_path: None,
        }
    }
}

#[cfg(test)]
mod test {
    use clap::Parser;

    use super::*;

    fn cli(args: &[&str]) -> Cli {
        Cli::try_parse_from([&["minotari_mcp_node"], args].concat()).unwrap()
    }

    fn env_with_password(key: &str) -> Option<String> {
        (key == PASSWORD_ENV).then(|| "secret".to_string())
    }

    #[test]
    fn password_from_env_yields_basic_auth() {
        let config = NodeMcpConfig::load_with_env(&cli(&["--node-grpc-username", "admin"]), env_with_password).unwrap();
        let cfg = config.grpc_client_config().unwrap();
        let (username, password) = cfg.auth.username_password().unwrap();
        assert_eq!(username, "admin");
        assert_eq!(password.reveal(), b"secret");
    }

    #[test]
    fn password_from_file_yields_basic_auth() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("password");
        std::fs::write(&path, "from_file\n").unwrap();
        let config = NodeMcpConfig::load_with_env(
            &cli(&[
                "--node-grpc-username",
                "admin",
                "--node-grpc-password-file",
                path.to_str().unwrap(),
            ]),
            |_| None,
        )
        .unwrap();
        assert_eq!(config.node_grpc.password.unwrap().reveal(), b"from_file");
    }

    #[test]
    fn password_env_and_file_together_is_an_error() {
        let cli = cli(&[
            "--node-grpc-username",
            "admin",
            "--node-grpc-password-file",
            "/nonexistent",
        ]);
        assert!(matches!(
            NodeMcpConfig::load_with_env(&cli, env_with_password),
            Err(McpError::ConfigError(_))
        ));
    }

    #[test]
    fn username_and_password_must_be_set_together() {
        assert!(NodeMcpConfig::load_with_env(&cli(&["--node-grpc-username", "admin"]), |_| None).is_err());
        assert!(NodeMcpConfig::load_with_env(&cli(&[]), env_with_password).is_err());
    }

    #[test]
    fn tls_domain_requires_ca_cert() {
        let cli = cli(&["--node-grpc-tls-domain-name", "localhost"]);
        assert!(NodeMcpConfig::load_with_env(&cli, |_| None).is_err());
    }

    #[test]
    fn tls_with_explicit_http_address_is_an_error() {
        let cli = cli(&[
            "--node-grpc-address",
            "http://10.0.0.1:18142",
            "--node-grpc-tls-domain-name",
            "localhost",
            "--node-grpc-ca-cert",
            "/tmp/node_ca.pem",
        ]);
        assert!(NodeMcpConfig::load_with_env(&cli, |_| None).is_err());
    }

    #[test]
    fn node_grpc_url_uses_https_when_tls_is_set() {
        let cli = cli(&[
            "--node-grpc-address",
            "10.0.0.1:18142",
            "--node-grpc-tls-domain-name",
            "localhost",
            "--node-grpc-ca-cert",
            "/tmp/node_ca.pem",
        ]);
        let config = NodeMcpConfig::load_with_env(&cli, |_| None).unwrap();
        assert_eq!(config.node_grpc_url(), "https://10.0.0.1:18142");

        let config =
            NodeMcpConfig::load_with_env(&self::cli(&["--node-grpc-address", "10.0.0.1:18142"]), |_| None).unwrap();
        assert_eq!(config.node_grpc_url(), "http://10.0.0.1:18142");
    }
}
