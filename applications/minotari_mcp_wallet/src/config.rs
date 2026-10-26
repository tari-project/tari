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

use std::{collections::HashMap, path::Path};

use minotari_mcp_common::{CliIntegrationUtils, McpConfig, McpError, McpResult};
use minotari_wallet_grpc_client::GrpcAuthentication;
use serde::{Deserialize, Serialize};
use tari_common::{DefaultConfigLoader, SubConfigPath};
use tari_utilities::SafePassword;

use crate::cli::Cli;

#[derive(Debug, Default, Serialize, Deserialize)]
struct WalletAuthSection {
    #[serde(default)]
    grpc_authentication: GrpcAuthentication,
}

impl SubConfigPath for WalletAuthSection {
    fn main_key_prefix() -> &'static str {
        "wallet"
    }
}

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

    /// gRPC authentication configuration (read from env or config, never argv)
    #[serde(default)]
    pub authentication: GrpcAuthentication,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletSecurityConfig {
    /// Enable transaction preview before execution
    pub enable_preview: bool,
}

/// Generate a random per-run `GrpcAuthentication::Basic` credential for an auto-launched wallet.
pub fn generate_per_run_grpc_auth() -> GrpcAuthentication {
    let username = format!("mcp_{}", CliIntegrationUtils::generate_random_token());
    let password = format!(
        "{}{}",
        CliIntegrationUtils::generate_random_token(),
        CliIntegrationUtils::generate_random_token()
    );
    GrpcAuthentication::Basic {
        username,
        password: SafePassword::from(password),
    }
}

/// Resolve `GrpcAuthentication` from an environment variable map.
/// Checks `MINOTARI_WALLET_GRPC_USERNAME` / `MINOTARI_WALLET_GRPC_PASSWORD` first,
/// then `TARI_WALLET__GRPC_AUTHENTICATION__USERNAME` / `TARI_WALLET__GRPC_AUTHENTICATION__PASSWORD`.
pub fn resolve_grpc_authentication_from_vars(vars: &HashMap<String, String>) -> McpResult<Option<GrpcAuthentication>> {
    for (user_key, pass_key) in [
        ("MINOTARI_WALLET_GRPC_USERNAME", "MINOTARI_WALLET_GRPC_PASSWORD"),
        (
            "TARI_WALLET__GRPC_AUTHENTICATION__USERNAME",
            "TARI_WALLET__GRPC_AUTHENTICATION__PASSWORD",
        ),
    ] {
        let username = vars.get(user_key).map(String::as_str).filter(|s| !s.trim().is_empty());
        let password = vars.get(pass_key).map(String::as_str).filter(|s| !s.is_empty());
        match (username, password) {
            (Some(u), Some(p)) => {
                return Ok(Some(GrpcAuthentication::Basic {
                    username: u.to_string(),
                    password: SafePassword::from(p),
                }));
            },
            (Some(_), None) | (None, Some(_)) => {
                return Err(McpError::config_error(format!(
                    "Both {user_key} and {pass_key} must be set when configuring gRPC Basic authentication via \
                     environment variables"
                )));
            },
            (None, None) => {},
        }
    }
    Ok(None)
}

fn resolve_grpc_authentication_from_file(config_path: &Path, cli: &Cli) -> McpResult<GrpcAuthentication> {
    if !config_path.exists() {
        return Ok(GrpcAuthentication::None);
    }

    let cfg = tari_common::load_configuration(config_path, false, true, cli, cli.common.network).map_err(|e| {
        McpError::config_error(format!(
            "Failed to load configuration from {}: {e}",
            config_path.display()
        ))
    })?;

    let wallet_section = WalletAuthSection::load_from(&cfg).map_err(|e| {
        McpError::config_error(format!(
            "Failed to parse wallet gRPC authentication from {}: {e}",
            config_path.display()
        ))
    })?;

    Ok(wallet_section.grpc_authentication)
}

fn resolve_grpc_authentication(cli: &Cli) -> McpResult<GrpcAuthentication> {
    let env_vars: HashMap<String, String> = std::env::vars().collect();
    if let Some(auth) = resolve_grpc_authentication_from_vars(&env_vars)? {
        return Ok(auth);
    }

    let config_path = cli.common.config_path();
    resolve_grpc_authentication_from_file(&config_path, cli)
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

        let authentication = resolve_grpc_authentication(cli)?;

        let wallet_grpc_config = WalletGrpcConfig {
            address: cli.wallet_grpc_address.clone(),
            timeout_secs: cli.wallet_grpc_timeout,
            max_retries: 3,
            auto_launch: cli.auto_launch_wallet,
            authentication,
        };

        let security_config = WalletSecurityConfig { enable_preview: true };

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

        if let GrpcAuthentication::Basic { username, password } = &self.wallet_grpc.authentication &&
            (username.trim().is_empty() || password.reveal().is_empty())
        {
            return Err(McpError::config_error(
                "Wallet gRPC Basic authentication requires a non-empty username and password",
            ));
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
            authentication: GrpcAuthentication::None,
        }
    }
}

impl Default for WalletSecurityConfig {
    fn default() -> Self {
        Self { enable_preview: true }
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    #[test]
    fn test_resolve_grpc_auth_from_vars() {
        let mut vars = HashMap::new();
        assert!(resolve_grpc_authentication_from_vars(&vars).unwrap().is_none());

        // Only username set -> error
        vars.insert("MINOTARI_WALLET_GRPC_USERNAME".to_string(), "alice".to_string());
        assert!(resolve_grpc_authentication_from_vars(&vars).is_err());

        // Both username and password set -> Basic auth
        vars.insert("MINOTARI_WALLET_GRPC_PASSWORD".to_string(), "s3cret".to_string());
        let auth = resolve_grpc_authentication_from_vars(&vars).unwrap().unwrap();
        let (user, pass) = auth.username_password().expect("expected Basic auth");
        assert_eq!(user, "alice");
        assert_eq!(pass.reveal(), b"s3cret");

        // Leading/trailing spaces in username are preserved (blank-only check uses trim())
        vars.insert("MINOTARI_WALLET_GRPC_USERNAME".to_string(), " alice ".to_string());
        let auth_spaced = resolve_grpc_authentication_from_vars(&vars).unwrap().unwrap();
        let (user_spaced, _) = auth_spaced.username_password().expect("expected Basic auth");
        assert_eq!(user_spaced, " alice ");

        // Debug representation of WalletGrpcConfig never leaks the password
        let grpc_cfg = WalletGrpcConfig {
            authentication: auth,
            ..Default::default()
        };
        let debug_str = format!("{grpc_cfg:?}");
        assert!(!debug_str.contains("s3cret"));
    }

    #[test]
    fn test_resolve_grpc_auth_from_file_supports_scoped_credentials_and_reports_errors() {
        let cli = Cli::try_parse_from(["minotari_mcp_wallet"]).expect("default CLI should parse");
        let unique = CliIntegrationUtils::generate_random_token();
        let temp_dir = std::env::temp_dir().join(format!("tari_mcp_wallet_cfg_test_{unique}"));
        std::fs::create_dir_all(&temp_dir).unwrap();

        // 1. Scoped credentials via wallet.override_from = "mainnet" and [mainnet.wallet.grpc_authentication]
        let scoped_path = temp_dir.join("scoped.toml");
        std::fs::write(
            &scoped_path,
            r#"
[wallet]
override_from = "mainnet"

[mainnet.wallet.grpc_authentication]
username = "scoped_user"
password = "scoped_password"
"#,
        )
        .unwrap();
        let scoped_auth =
            resolve_grpc_authentication_from_file(&scoped_path, &cli).expect("scoped credentials should resolve");
        let (user, pass) = scoped_auth.username_password().expect("expected Basic auth");
        assert_eq!(user, "scoped_user");
        assert_eq!(pass.reveal(), b"scoped_password");

        // 2. Absent authentication setting falls back to GrpcAuthentication::None
        let absent_path = temp_dir.join("absent.toml");
        std::fs::write(
            &absent_path,
            r#"
[wallet]
grpc_enabled = true
"#,
        )
        .unwrap();
        let absent_auth =
            resolve_grpc_authentication_from_file(&absent_path, &cli).expect("absent auth should default to None");
        assert!(matches!(absent_auth, GrpcAuthentication::None));

        // 3. Malformed TOML or invalid grpc_authentication returns an error instead of silently becoming None
        let broken_path = temp_dir.join("broken.toml");
        std::fs::write(&broken_path, "[wallet\ngrpc_enabled = true").unwrap();
        assert!(resolve_grpc_authentication_from_file(&broken_path, &cli).is_err());

        let invalid_auth_path = temp_dir.join("invalid_auth.toml");
        std::fs::write(
            &invalid_auth_path,
            r#"
[wallet.grpc_authentication]
username = "only_username_no_password"
"#,
        )
        .unwrap();
        assert!(resolve_grpc_authentication_from_file(&invalid_auth_path, &cli).is_err());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_generate_per_run_grpc_auth_is_unique_and_valid() {
        let auth1 = generate_per_run_grpc_auth();
        let auth2 = generate_per_run_grpc_auth();
        let (u1, p1) = auth1.username_password().expect("expected Basic auth");
        let (u2, p2) = auth2.username_password().expect("expected Basic auth");
        assert!(!u1.is_empty());
        assert!(!p1.reveal().is_empty());
        assert_ne!(u1, u2);
        assert_ne!(p1.reveal(), p2.reveal());
    }
}
