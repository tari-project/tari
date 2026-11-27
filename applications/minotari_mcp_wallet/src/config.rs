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

use std::path::PathBuf;

use minotari_mcp_common::{
    McpConfig,
    McpError,
    McpResult,
    wallet_rpc::{DEFAULT_MAX_FEE_PER_GRAM, DEFAULT_SPEND_BUDGET},
};
use serde::{Deserialize, Serialize};
use tari_common_types::grpc_authentication::GrpcAuthentication;
use tari_utilities::SafePassword;

use crate::cli::Cli;

pub const GRPC_PASSWORD_ENV: &str = "MINOTARI_WALLET_GRPC_PASSWORD";
pub const WALLET_PASSWORD_ENV: &str = "MINOTARI_MCP_WALLET_PASSWORD";

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

    /// gRPC authentication configuration. Never serialized: `SafePassword` serializes plain bytes.
    #[serde(skip_serializing)]
    pub authentication: GrpcAuthentication,

    /// Console wallet password for auto-launch. Never serialized: `SafePassword` serializes plain bytes.
    #[serde(skip_serializing, default)]
    pub wallet_password: Option<SafePassword>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletSecurityConfig {
    /// Require user confirmation for all value transfers
    pub require_confirmation: bool,

    /// Maximum transaction amount without additional confirmation (in µT)
    pub max_auto_amount: u64,

    /// Most µT the server may move out of the wallet in total per process, fees included
    pub spend_budget: u64,

    /// Highest fee_per_gram, in µT/g, any fund-moving call may use
    pub max_fee_per_gram: u64,
}

impl WalletMcpConfig {
    /// Load configuration from CLI arguments and environment variables
    pub fn load(cli: &Cli) -> McpResult<Self> {
        Self::load_with_env(cli, |key| std::env::var(key).ok())
    }

    pub(crate) fn load_with_env(cli: &Cli, env: impl Fn(&str) -> Option<String>) -> McpResult<Self> {
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

        let grpc_password = load_password(
            env(GRPC_PASSWORD_ENV),
            cli.wallet_grpc_password_file.as_ref(),
            GRPC_PASSWORD_ENV,
            "wallet gRPC password file",
        )?;
        let wallet_password = load_password(
            env(WALLET_PASSWORD_ENV),
            cli.wallet_password_file.as_ref(),
            WALLET_PASSWORD_ENV,
            "wallet password file",
        )?;

        let grpc_authentication = match (&cli.wallet_grpc_username, grpc_password) {
            (Some(username), Some(password)) => GrpcAuthentication::Basic {
                username: username.clone(),
                password,
            },
            (None, None) => GrpcAuthentication::None,
            (Some(_), None) => {
                return Err(McpError::config_error(
                    "Wallet gRPC username and password must be set together",
                ));
            },
            (None, Some(_)) => {
                return Err(McpError::config_error(
                    "Wallet gRPC username and password must be set together",
                ));
            },
        };

        let wallet_grpc_config = WalletGrpcConfig {
            address: cli.wallet_grpc_address.clone(),
            timeout_secs: cli.wallet_grpc_timeout,
            max_retries: 3,
            auto_launch: cli.auto_launch_wallet,
            authentication: grpc_authentication,
            wallet_password,
        };

        let security_config = WalletSecurityConfig {
            require_confirmation: cli.require_confirmation,
            max_auto_amount: 1_000_000, // 1 Tari in µT
            spend_budget: cli.mcp_spend_budget,
            max_fee_per_gram: cli.mcp_max_fee_per_gram,
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

        // There is no channel over stdio MCP to ask a person to confirm a call, so the setting cannot be honoured
        if self.security.require_confirmation {
            if self.mcp.control_enabled {
                return Err(McpError::config_error(
                    "require_confirmation is not supported by the MCP wallet server; Control tools cannot be \
                     confirmed per call",
                ));
            }
            log::debug!("require_confirmation has no effect without control operations");
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

/// The password comes from the environment or a file, never a command line flag, so it is not visible in the process
/// list.
fn load_password(
    env_password: Option<String>,
    password_file: Option<&PathBuf>,
    env_name: &str,
    label: &str,
) -> McpResult<Option<SafePassword>> {
    match (env_password, password_file) {
        (Some(_), Some(_)) => Err(McpError::config_error(format!(
            "Set only one of {env_name} and the {label}"
        ))),
        (Some(password), None) => Ok(Some(SafePassword::from(password))),
        (None, Some(path)) => {
            warn_if_readable_by_others(path, label);
            let mut password = std::fs::read_to_string(path).map_err(|e| {
                McpError::config_error(format!("Could not read {label} '{}': {e}", path.display()))
            })?;
            password.truncate(password.trim_end_matches(['\r', '\n']).len());
            // Moved, not copied, into the zeroizing SafePassword.
            Ok(Some(SafePassword::from(password)))
        },
        (None, None) => Ok(None),
    }
}

#[cfg(unix)]
fn warn_if_readable_by_others(path: &PathBuf, label: &str) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(metadata) = std::fs::metadata(path) &&
        metadata.permissions().mode() & 0o044 != 0
    {
        log::warn!(
            "The {label} '{}' is readable by group or others; restrict it with chmod 600",
            path.display()
        );
    }
}

#[cfg(not(unix))]
fn warn_if_readable_by_others(_path: &PathBuf, _label: &str) {}

impl Default for WalletGrpcConfig {
    fn default() -> Self {
        Self {
            address: "127.0.0.1:18143".to_string(),
            timeout_secs: 30,
            max_retries: 3,
            auto_launch: true,
            authentication: GrpcAuthentication::None,
            wallet_password: None,
        }
    }
}

impl Default for WalletSecurityConfig {
    fn default() -> Self {
        Self {
            require_confirmation: false,
            max_auto_amount: 1_000_000, // 1 Tari
            spend_budget: DEFAULT_SPEND_BUDGET,
            max_fee_per_gram: DEFAULT_MAX_FEE_PER_GRAM,
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;
    use uuid::Uuid;

    use super::*;

    fn config(control_enabled: bool, require_confirmation: bool) -> WalletMcpConfig {
        WalletMcpConfig {
            mcp: McpConfig {
                enabled: true,
                control_enabled,
                ..Default::default()
            },
            wallet_grpc: WalletGrpcConfig::default(),
            security: WalletSecurityConfig {
                require_confirmation,
                ..Default::default()
            },
        }
    }

    fn cli(args: &[&str]) -> Cli {
        Cli::try_parse_from([&["minotari_mcp_wallet"], args].concat()).unwrap()
    }

    #[test]
    fn require_confirmation_is_refused_with_control_operations() {
        let err = config(true, true).validate().unwrap_err().to_string();
        assert!(err.contains("require_confirmation is not supported"), "{err}");
        assert!(config(false, true).validate().is_ok());
        assert!(config(true, false).validate().is_ok());
    }

    #[test]
    fn grpc_password_from_env_yields_basic_auth() {
        let env_fn = |key: &str| (key == GRPC_PASSWORD_ENV).then(|| "secret".to_string());
        let cfg = WalletMcpConfig::load_with_env(&cli(&["--wallet-grpc-username", "admin"]), env_fn).unwrap();
        match cfg.wallet_grpc.authentication {
            GrpcAuthentication::Basic { username, password } => {
                assert_eq!(username, "admin");
                assert_eq!(password.reveal().as_slice(), b"secret");
            },
            other => panic!("Expected GrpcAuthentication::Basic, got {other:?}"),
        }
    }

    #[test]
    fn grpc_and_wallet_passwords_from_files() {
        let dir = std::env::temp_dir().join(format!("tari_mcp_wallet_cfg_{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let grpc_pass_path = dir.join("grpc_pass");
        let wallet_pass_path = dir.join("wallet_pass");
        std::fs::write(&grpc_pass_path, "grpc_from_file\n").unwrap();
        std::fs::write(&wallet_pass_path, "wallet_from_file\r\n").unwrap();

        let cfg = WalletMcpConfig::load_with_env(
            &cli(&[
                "--wallet-grpc-username",
                "admin",
                "--wallet-grpc-password-file",
                grpc_pass_path.to_str().unwrap(),
                "--wallet-password-file",
                wallet_pass_path.to_str().unwrap(),
            ]),
            |_| None,
        )
        .unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        match cfg.wallet_grpc.authentication {
            GrpcAuthentication::Basic { username, password } => {
                assert_eq!(username, "admin");
                assert_eq!(password.reveal().as_slice(), b"grpc_from_file");
            },
            other => panic!("Expected GrpcAuthentication::Basic, got {other:?}"),
        }
        assert_eq!(
            cfg.wallet_grpc.wallet_password.unwrap().reveal().as_slice(),
            b"wallet_from_file"
        );
    }

    #[test]
    fn username_and_password_must_be_set_together() {
        let env_fn = |key: &str| (key == GRPC_PASSWORD_ENV).then(|| "secret".to_string());
        assert!(WalletMcpConfig::load_with_env(&cli(&["--wallet-grpc-username", "admin"]), |_| None).is_err());
        assert!(WalletMcpConfig::load_with_env(&cli(&[]), env_fn).is_err());
    }
}
