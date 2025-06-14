//! CLI integration utilities for enhanced argument passthrough
//!
//! Provides utilities for converting MCP server CLI arguments into proper
//! arguments for launched Tari applications, ensuring configuration consistency
//! and eliminating hardcoded values in auto-launch functionality.

#![allow(clippy::vec_init_then_push)]

use std::{collections::HashMap, path::PathBuf};

/// Configuration extracted from CLI arguments for process launching
#[derive(Debug, Clone)]
pub struct LaunchCliConfig {
    /// Base path for Tari applications
    pub base_path: String,
    /// Configuration file path
    pub config_path: String,
    /// Network to use (mainnet, esmeralda, nextnet, etc.)
    pub network: Option<String>,
    /// Log configuration path
    pub log_config: Option<PathBuf>,
    /// Additional property overrides
    pub property_overrides: Vec<(String, String)>,
    /// Environment variables to set
    pub environment_vars: HashMap<String, String>,
}

/// Builder for creating CLI configurations from various sources
pub struct CliConfigBuilder {
    base_path: Option<String>,
    config_path: Option<String>,
    network: Option<String>,
    log_config: Option<PathBuf>,
    property_overrides: Vec<(String, String)>,
    environment_vars: HashMap<String, String>,
}

impl CliConfigBuilder {
    pub fn new() -> Self {
        Self {
            base_path: None,
            config_path: None,
            network: None,
            log_config: None,
            property_overrides: Vec::new(),
            environment_vars: HashMap::new(),
        }
    }

    pub fn with_base_path(mut self, base_path: String) -> Self {
        self.base_path = Some(base_path);
        self
    }

    pub fn with_config_path(mut self, config_path: String) -> Self {
        self.config_path = Some(config_path);
        self
    }

    pub fn with_network(mut self, network: String) -> Self {
        self.network = Some(network);
        self
    }

    pub fn with_log_config(mut self, log_config: PathBuf) -> Self {
        self.log_config = Some(log_config);
        self
    }

    pub fn with_property_override(mut self, key: String, value: String) -> Self {
        self.property_overrides.push((key, value));
        self
    }

    pub fn with_environment_var(mut self, key: String, value: String) -> Self {
        self.environment_vars.insert(key, value);
        self
    }

    pub fn build(self) -> LaunchCliConfig {
        LaunchCliConfig {
            base_path: self.base_path.unwrap_or_else(|| "/tmp/tari".to_string()),
            config_path: self.config_path.unwrap_or_else(|| "config/config.toml".to_string()),
            network: self.network,
            log_config: self.log_config,
            property_overrides: self.property_overrides,
            environment_vars: self.environment_vars,
        }
    }
}

/// Trait for extracting CLI configuration from different CLI types
pub trait CliConfigExtractor {
    /// Extract configuration for launching processes
    fn extract_launch_config(&self) -> LaunchCliConfig;

    /// Extract base node specific arguments
    fn extract_node_args(&self) -> Vec<String>;

    /// Extract wallet specific arguments
    fn extract_wallet_args(&self) -> Vec<String>;
}

/// Node-specific argument builder
pub struct NodeArgumentBuilder {
    config: LaunchCliConfig,
    node_specific_args: Vec<String>,
    include_base_args: bool,
}

impl NodeArgumentBuilder {
    pub fn new(config: LaunchCliConfig) -> Self {
        Self {
            config,
            node_specific_args: Vec::new(),
            include_base_args: true,
        }
    }

    /// Create builder that only includes node-specific args (no base args)
    pub fn node_args_only(config: LaunchCliConfig) -> Self {
        Self {
            config,
            node_specific_args: Vec::new(),
            include_base_args: false,
        }
    }

    pub fn enable_grpc(mut self) -> Self {
        self.node_specific_args.push("--grpc-enabled".to_string());
        self
    }

    pub fn enable_mining(mut self) -> Self {
        self.node_specific_args.push("--mining-enabled".to_string());
        self
    }

    pub fn enable_second_layer_grpc(mut self) -> Self {
        self.node_specific_args.push("--second-layer-grpc-enabled".to_string());
        self
    }

    pub fn non_interactive(mut self) -> Self {
        self.node_specific_args.push("--non-interactive-mode".to_string());
        self
    }

    pub fn disable_splash(mut self) -> Self {
        self.node_specific_args.push("--disable-splash-screen".to_string());
        self
    }

    pub fn with_libtor_data_dir(mut self, dir: PathBuf) -> Self {
        self.node_specific_args.push("-z".to_string());
        self.node_specific_args.push(dir.to_string_lossy().to_string());
        self
    }

    pub fn with_custom_arg(mut self, arg: String) -> Self {
        self.node_specific_args.push(arg);
        self
    }

    pub fn build(self) -> Vec<String> {
        let mut args = Vec::new();

        // Add base arguments only if requested
        if self.include_base_args {
            args.push("--base-path".to_string());
            args.push(self.config.base_path);
            args.push("--config".to_string());
            args.push(self.config.config_path);

            // Add network if specified
            if let Some(network) = self.config.network {
                args.push("--network".to_string());
                args.push(network);
            }
        }

        // Add log config if specified
        if self.include_base_args {
            if let Some(log_config) = self.config.log_config {
                args.push("--log-config".to_string());
                args.push(log_config.to_string_lossy().to_string());
            }
        }

        // Add node-specific arguments
        args.extend(self.node_specific_args);

        // Add property overrides
        if self.include_base_args {
            for (key, value) in self.config.property_overrides {
                args.push("-p".to_string());
                args.push(format!("{}={}", key, value));
            }
        }

        args
    }
}

/// Wallet-specific argument builder
pub struct WalletArgumentBuilder {
    config: LaunchCliConfig,
    wallet_specific_args: Vec<String>,
    include_base_args: bool,
}

impl WalletArgumentBuilder {
    pub fn new(config: LaunchCliConfig) -> Self {
        Self {
            config,
            wallet_specific_args: Vec::new(),
            include_base_args: true,
        }
    }

    /// Create builder that only includes wallet-specific args (no base args)
    pub fn wallet_args_only(config: LaunchCliConfig) -> Self {
        Self {
            config,
            wallet_specific_args: Vec::new(),
            include_base_args: false,
        }
    }

    pub fn enable_grpc(mut self) -> Self {
        self.wallet_specific_args.push("--grpc-enabled".to_string());
        self
    }

    pub fn with_grpc_address(mut self, address: String) -> Self {
        self.wallet_specific_args.push("--grpc-address".to_string());
        self.wallet_specific_args.push(address);
        self
    }

    pub fn non_interactive(mut self) -> Self {
        self.wallet_specific_args.push("--non-interactive-mode".to_string());
        self
    }

    pub fn with_password(mut self, password: String) -> Self {
        self.wallet_specific_args.push("--password".to_string());
        self.wallet_specific_args.push(password);
        self
    }

    pub fn force_recovery(mut self) -> Self {
        self.wallet_specific_args.push("--recovery".to_string());
        self
    }

    pub fn with_libtor_data_dir(mut self, dir: PathBuf) -> Self {
        self.wallet_specific_args.push("-z".to_string());
        self.wallet_specific_args.push(dir.to_string_lossy().to_string());
        self
    }

    pub fn with_custom_arg(mut self, arg: String) -> Self {
        self.wallet_specific_args.push(arg);
        self
    }

    pub fn build(self) -> Vec<String> {
        let mut args = Vec::new();

        // Add base arguments only if requested
        if self.include_base_args {
            args.push("--base-path".to_string());
            args.push(self.config.base_path);
            args.push("--config".to_string());
            args.push(self.config.config_path);

            // Add network if specified
            if let Some(network) = self.config.network {
                args.push("--network".to_string());
                args.push(network);
            }

            // Add log config if specified
            if let Some(log_config) = self.config.log_config {
                args.push("--log-config".to_string());
                args.push(log_config.to_string_lossy().to_string());
            }
        }

        // Add wallet-specific arguments
        args.extend(self.wallet_specific_args);

        // Add property overrides
        if self.include_base_args {
            for (key, value) in self.config.property_overrides {
                args.push("-p".to_string());
                args.push(format!("{}={}", key, value));
            }
        }

        args
    }
}

/// Utility functions for CLI integration
pub struct CliIntegrationUtils;

impl CliIntegrationUtils {
    /// Extract port from gRPC address string
    pub fn extract_port_from_address(address: &str) -> Option<u16> {
        address.split(':').last().and_then(|port_str| port_str.parse().ok())
    }

    /// Build gRPC endpoint URL from address
    pub fn build_grpc_url(address: &str) -> String {
        if address.starts_with("http://") || address.starts_with("https://") {
            address.to_string()
        } else {
            format!("http://{}", address)
        }
    }

    /// Validate gRPC address format
    pub fn validate_grpc_address(address: &str) -> bool {
        // Basic validation: should contain host:port or be a valid URL
        if address.starts_with("http://") || address.starts_with("https://") {
            return true;
        }

        // Check for host:port format
        address.split(':').count() == 2 && Self::extract_port_from_address(address).is_some()
    }

    /// Find available port starting from base
    pub fn find_available_port(base_port: u16) -> Option<u16> {
        (base_port..=u16::MAX).find(|&port| std::net::TcpListener::bind(("127.0.0.1", port)).is_ok())
    }

    /// Create environment variables map for launched processes
    pub fn create_environment_vars(config: &LaunchCliConfig) -> HashMap<String, String> {
        let mut env_vars = config.environment_vars.clone();

        // Add common Tari environment variables
        env_vars.insert("TARI_BASE_PATH".to_string(), config.base_path.clone());
        env_vars.insert("TARI_CONFIG_PATH".to_string(), config.config_path.clone());

        if let Some(ref network) = config.network {
            env_vars.insert("TARI_NETWORK".to_string(), network.clone());
        }

        env_vars
    }
}

impl Default for CliConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_config_builder() {
        let config = CliConfigBuilder::new()
            .with_base_path("/custom/path".to_string())
            .with_network("esmeralda".to_string())
            .with_property_override("test.key".to_string(), "test_value".to_string())
            .build();

        assert_eq!(config.base_path, "/custom/path");
        assert_eq!(config.network, Some("esmeralda".to_string()));
        assert_eq!(config.property_overrides.len(), 1);
    }

    #[test]
    fn test_node_argument_builder() {
        let config = CliConfigBuilder::new()
            .with_base_path("/test".to_string())
            .with_config_path("config.toml".to_string())
            .with_network("mainnet".to_string())
            .build();

        let args = NodeArgumentBuilder::new(config).enable_grpc().non_interactive().build();

        assert!(args.contains(&"--base-path".to_string()));
        assert!(args.contains(&"/test".to_string()));
        assert!(args.contains(&"--grpc-enabled".to_string()));
        assert!(args.contains(&"--non-interactive-mode".to_string()));
    }

    #[test]
    fn test_wallet_argument_builder() {
        let config = CliConfigBuilder::new().with_base_path("/test".to_string()).build();

        let args = WalletArgumentBuilder::new(config)
            .enable_grpc()
            .with_grpc_address("127.0.0.1:18143".to_string())
            .non_interactive()
            .build();

        assert!(args.contains(&"--grpc-enabled".to_string()));
        assert!(args.contains(&"--grpc-address".to_string()));
        assert!(args.contains(&"127.0.0.1:18143".to_string()));
    }

    #[test]
    fn test_port_extraction() {
        assert_eq!(
            CliIntegrationUtils::extract_port_from_address("127.0.0.1:18142"),
            Some(18142)
        );
        assert_eq!(
            CliIntegrationUtils::extract_port_from_address("localhost:8080"),
            Some(8080)
        );
        assert_eq!(CliIntegrationUtils::extract_port_from_address("invalid"), None);
    }

    #[test]
    fn test_grpc_url_building() {
        assert_eq!(
            CliIntegrationUtils::build_grpc_url("127.0.0.1:18142"),
            "http://127.0.0.1:18142"
        );
        assert_eq!(
            CliIntegrationUtils::build_grpc_url("http://127.0.0.1:18142"),
            "http://127.0.0.1:18142"
        );
    }

    #[test]
    fn test_address_validation() {
        assert!(CliIntegrationUtils::validate_grpc_address("127.0.0.1:18142"));
        assert!(CliIntegrationUtils::validate_grpc_address("http://127.0.0.1:18142"));
        assert!(!CliIntegrationUtils::validate_grpc_address("invalid"));
        assert!(!CliIntegrationUtils::validate_grpc_address("127.0.0.1"));
    }
}
