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
//! gRPC Configuration Parser for Base Node
//!
//! This module handles parsing and filtering of gRPC methods based on the base node's
//! `grpc_server_allow_methods` configuration setting.

use std::{collections::HashSet, str::FromStr};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors that can occur during gRPC configuration parsing
#[derive(Debug, Error)]
pub enum GrpcConfigError {
    #[error("Invalid method name format: {0}")]
    InvalidMethodName(String),

    #[error("Unknown service: {0}")]
    UnknownService(String),

    #[error("Configuration parsing error: {0}")]
    #[allow(dead_code)]
    ParseError(String),

    #[error("Method not found: {0}")]
    #[allow(dead_code)]
    MethodNotFound(String),
}

/// Configuration for gRPC method restrictions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrpcMethodConfig {
    /// List of allowed gRPC methods
    /// Format: "service/method" or "service/*" for all methods in a service
    pub allowed_methods: Vec<String>,

    /// Whether to allow all methods by default (if allowed_methods is empty)
    pub allow_all_by_default: bool,

    /// Whether to allow control operations (methods that can modify state)
    pub allow_control_operations: bool,
}

impl Default for GrpcMethodConfig {
    fn default() -> Self {
        Self {
            allowed_methods: Vec::new(),
            allow_all_by_default: true,
            allow_control_operations: false,
        }
    }
}

/// gRPC configuration parser for base node methods
#[derive(Debug, Clone)]
pub struct GrpcConfigParser {
    config: GrpcMethodConfig,
    allowed_methods_set: HashSet<String>,
    allowed_services: HashSet<String>,
}

impl GrpcConfigParser {
    /// Create a new gRPC config parser
    pub fn new(config: GrpcMethodConfig) -> Result<Self, GrpcConfigError> {
        let mut parser = Self {
            config: config.clone(),
            allowed_methods_set: HashSet::new(),
            allowed_services: HashSet::new(),
        };

        parser.parse_allowed_methods(&config.allowed_methods)?;

        Ok(parser)
    }

    /// Create parser from a comma-separated string of allowed methods
    pub fn from_string(methods_str: &str, allow_control_operations: bool) -> Result<Self, GrpcConfigError> {
        let allowed_methods = if methods_str.trim().is_empty() {
            Vec::new()
        } else {
            methods_str
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        };

        let config = GrpcMethodConfig {
            allowed_methods,
            allow_all_by_default: methods_str.trim().is_empty(),
            allow_control_operations,
        };

        Self::new(config)
    }

    /// Check if a specific method is allowed
    #[allow(dead_code)]
    pub fn is_method_allowed(&self, method_name: &str) -> bool {
        // If no restrictions configured, allow all methods
        if self.config.allow_all_by_default && self.config.allowed_methods.is_empty() {
            return true;
        }

        // Check exact method match
        if self.allowed_methods_set.contains(method_name) {
            return true;
        }

        // Check service wildcard match
        if let Some(service_name) = self.extract_service_name(method_name) {
            if self.allowed_services.contains(&service_name) {
                return true;
            }
        }

        false
    }

    /// Check if control operations are allowed
    #[allow(dead_code)]
    pub fn are_control_operations_allowed(&self) -> bool {
        self.config.allow_control_operations
    }

    /// Get all allowed methods
    #[allow(dead_code)]
    pub fn get_allowed_methods(&self) -> &HashSet<String> {
        &self.allowed_methods_set
    }

    /// Get all allowed services (with wildcard)
    #[allow(dead_code)]
    pub fn get_allowed_services(&self) -> &HashSet<String> {
        &self.allowed_services
    }

    /// Filter a list of method names based on configuration
    #[allow(dead_code)]
    pub fn filter_methods(&self, methods: &[String]) -> Vec<String> {
        methods
            .iter()
            .filter(|method| self.is_method_allowed(method))
            .cloned()
            .collect()
    }

    /// Get configuration summary for debugging
    #[allow(dead_code)]
    pub fn get_config_summary(&self) -> String {
        let allowed_count = self.allowed_methods_set.len();
        let services_count = self.allowed_services.len();

        if self.config.allow_all_by_default && self.config.allowed_methods.is_empty() {
            "All methods allowed (no restrictions configured)".to_string()
        } else {
            format!(
                "Restricted mode: {} specific methods, {} wildcard services, control operations {}",
                allowed_count,
                services_count,
                if self.config.allow_control_operations {
                    "enabled"
                } else {
                    "disabled"
                }
            )
        }
    }

    /// Parse the allowed methods configuration
    fn parse_allowed_methods(&mut self, methods: &[String]) -> Result<(), GrpcConfigError> {
        for method_spec in methods {
            self.parse_method_spec(method_spec.trim())?;
        }

        Ok(())
    }

    /// Parse a single method specification
    fn parse_method_spec(&mut self, spec: &str) -> Result<(), GrpcConfigError> {
        if spec.is_empty() {
            return Ok(());
        }

        // Handle service wildcard (e.g., "BaseNode/*")
        if spec.ends_with("/*") {
            let service_name = spec.trim_end_matches("/*");
            self.validate_service_name(service_name)?;
            self.allowed_services.insert(service_name.to_string());
            return Ok(());
        }

        // Handle full method name (e.g., "BaseNode/GetTipInfo" or "tari.rpc.BaseNode/GetTipInfo")
        if spec.contains('/') {
            self.validate_method_name(spec)?;

            // Normalize to full name format
            let full_name = if spec.starts_with("tari.rpc.") {
                spec.to_string()
            } else {
                format!("tari.rpc.{spec}")
            };

            self.allowed_methods_set.insert(full_name);
            return Ok(());
        }

        // Handle service name only (equivalent to service/*)
        if !spec.contains('/') {
            self.validate_service_name(spec)?;
            self.allowed_services.insert(spec.to_string());
            return Ok(());
        }

        Err(GrpcConfigError::InvalidMethodName(format!(
            "Invalid method specification format: {spec}. Expected formats: 'Service/*', 'Service/Method', or \
             'tari.rpc.Service/Method'"
        )))
    }

    /// Validate service name
    fn validate_service_name(&self, service: &str) -> Result<(), GrpcConfigError> {
        const VALID_SERVICES: &[&str] = &["BaseNode", "Wallet"];

        if !VALID_SERVICES.contains(&service) {
            return Err(GrpcConfigError::UnknownService(format!(
                "Unknown service: {}. Valid services: {}",
                service,
                VALID_SERVICES.join(", ")
            )));
        }

        Ok(())
    }

    /// Validate method name format
    fn validate_method_name(&self, method: &str) -> Result<(), GrpcConfigError> {
        if !method.contains('/') {
            return Err(GrpcConfigError::InvalidMethodName(
                "Method name must contain service/method separator".to_string(),
            ));
        }

        let parts: Vec<&str> = method.split('/').collect();
        if parts.len() != 2 {
            return Err(GrpcConfigError::InvalidMethodName(
                "Method name must have exactly one '/' separator".to_string(),
            ));
        }

        let (service_part, method_part) = (
            *parts.first().expect("Already checked"),
            *parts.get(1).expect("Already checked"),
        );

        // Extract service name from full qualified name if present
        let service_name = if service_part.starts_with("tari.rpc.") {
            service_part.trim_start_matches("tari.rpc.")
        } else {
            service_part
        };

        self.validate_service_name(service_name)?;

        if method_part.is_empty() {
            return Err(GrpcConfigError::InvalidMethodName(
                "Method name cannot be empty".to_string(),
            ));
        }

        Ok(())
    }

    /// Extract service name from full method name
    #[allow(dead_code)]
    fn extract_service_name(&self, method_name: &str) -> Option<String> {
        if let Some(slash_pos) = method_name.find('/') {
            let service_part = &method_name[..slash_pos];

            // Handle full qualified names
            if service_part.starts_with("tari.rpc.") {
                Some(service_part.trim_start_matches("tari.rpc.").to_string())
            } else {
                Some(service_part.to_string())
            }
        } else {
            None
        }
    }
}

/// Predefined method groups for common configurations
pub mod method_groups {
    /// Read-only base node methods (safe for most use cases)
    pub const BASE_NODE_READONLY: &[&str] = &[
        "BaseNode/GetTipInfo",
        "BaseNode/GetSyncInfo",
        "BaseNode/GetNetworkStatus",
        "BaseNode/ListHeaders",
        "BaseNode/GetHeaderByHash",
        "BaseNode/GetBlocks",
        "BaseNode/GetPeers",
        "BaseNode/ListConnectedPeers",
        "BaseNode/GetMempoolStats",
        "BaseNode/GetMempoolTransactions",
        "BaseNode/GetNetworkDifficulty",
        "BaseNode/GetTokensInCirculation",
        "BaseNode/GetVersion",
        "BaseNode/Identify",
        "BaseNode/SearchKernels",
        "BaseNode/SearchUtxos",
        "BaseNode/FetchMatchingUtxos",
    ];

    /// Mining-related methods
    pub const BASE_NODE_MINING: &[&str] = &[
        "BaseNode/GetNewBlockTemplate",
        "BaseNode/GetNewBlock",
        "BaseNode/GetNewBlockWithCoinbases",
        "BaseNode/GetNewBlockTemplateWithCoinbases",
        "BaseNode/GetNewBlockBlob",
    ];

    /// Control operations (require careful consideration)
    #[allow(dead_code)]
    pub const BASE_NODE_CONTROL: &[&str] = &["BaseNode/SubmitBlock", "BaseNode/SubmitTransaction"];

    /// All base node methods
    pub const BASE_NODE_ALL: &[&str] = &["BaseNode/*"];

    /// Configuration presets
    pub fn readonly_config() -> super::GrpcMethodConfig {
        super::GrpcMethodConfig {
            allowed_methods: BASE_NODE_READONLY.iter().map(|s| s.to_string()).collect(),
            allow_all_by_default: false,
            allow_control_operations: false,
        }
    }

    pub fn mining_config() -> super::GrpcMethodConfig {
        let mut methods = BASE_NODE_READONLY.to_vec();
        methods.extend_from_slice(BASE_NODE_MINING);

        super::GrpcMethodConfig {
            allowed_methods: methods.iter().map(|s| s.to_string()).collect(),
            allow_all_by_default: false,
            allow_control_operations: false,
        }
    }

    pub fn full_access_config() -> super::GrpcMethodConfig {
        super::GrpcMethodConfig {
            allowed_methods: BASE_NODE_ALL.iter().map(|s| s.to_string()).collect(),
            allow_all_by_default: true,
            allow_control_operations: true,
        }
    }
}

/// Integration with existing base node configuration
impl FromStr for GrpcMethodConfig {
    type Err = GrpcConfigError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Handle predefined presets
        match s.trim().to_lowercase().as_str() {
            "readonly" => Ok(method_groups::readonly_config()),
            "mining" => Ok(method_groups::mining_config()),
            "full" | "all" => Ok(method_groups::full_access_config()),
            "none" | "disabled" => Ok(GrpcMethodConfig {
                allowed_methods: Vec::new(),
                allow_all_by_default: false,
                allow_control_operations: false,
            }),
            _ => {
                // Parse as comma-separated list
                let parser = GrpcConfigParser::from_string(s, false)?;
                Ok(parser.config)
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parser_creation() {
        let config = GrpcMethodConfig::default();
        let parser = GrpcConfigParser::new(config);
        assert!(parser.is_ok());
    }

    #[test]
    fn test_method_filtering() {
        let parser = GrpcConfigParser::from_string("BaseNode/GetTipInfo,BaseNode/GetSyncInfo", false).unwrap();

        assert!(parser.is_method_allowed("tari.rpc.BaseNode/GetTipInfo"));
        assert!(parser.is_method_allowed("tari.rpc.BaseNode/GetSyncInfo"));
        assert!(!parser.is_method_allowed("tari.rpc.BaseNode/SubmitBlock"));
    }

    #[test]
    fn test_service_wildcard() {
        let parser = GrpcConfigParser::from_string("BaseNode/*", false).unwrap();

        assert!(parser.is_method_allowed("tari.rpc.BaseNode/GetTipInfo"));
        assert!(parser.is_method_allowed("tari.rpc.BaseNode/SubmitBlock"));
        assert!(!parser.is_method_allowed("tari.rpc.Wallet/GetBalance"));
    }

    #[test]
    fn test_allow_all_default() {
        let parser = GrpcConfigParser::from_string("", false).unwrap();

        assert!(parser.is_method_allowed("tari.rpc.BaseNode/GetTipInfo"));
        assert!(parser.is_method_allowed("tari.rpc.BaseNode/SubmitBlock"));
        assert!(parser.is_method_allowed("tari.rpc.Wallet/GetBalance"));
    }

    #[test]
    fn test_control_operations_flag() {
        let parser = GrpcConfigParser::from_string("BaseNode/*", true).unwrap();
        assert!(parser.are_control_operations_allowed());

        let parser = GrpcConfigParser::from_string("BaseNode/*", false).unwrap();
        assert!(!parser.are_control_operations_allowed());
    }

    #[test]
    fn test_invalid_service() {
        let result = GrpcConfigParser::from_string("InvalidService/*", false);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), GrpcConfigError::UnknownService(_)));
    }

    #[test]
    fn test_invalid_method_format() {
        let result = GrpcConfigParser::from_string("BaseNode/Invalid/Extra/Parts", false);
        assert!(result.is_err());
    }

    #[test]
    fn test_method_groups() {
        let readonly_config = method_groups::readonly_config();
        assert!(!readonly_config.allow_control_operations);
        assert!(!readonly_config.allowed_methods.is_empty());

        let full_config = method_groups::full_access_config();
        assert!(full_config.allow_control_operations);
    }

    #[test]
    fn test_config_from_str() {
        let config: GrpcMethodConfig = "readonly".parse().unwrap();
        assert!(!config.allow_control_operations);

        let config: GrpcMethodConfig = "full".parse().unwrap();
        assert!(config.allow_control_operations);

        let config: GrpcMethodConfig = "none".parse().unwrap();
        assert!(!config.allow_all_by_default);
        assert!(config.allowed_methods.is_empty());
    }

    #[test]
    fn test_config_summary() {
        let parser = GrpcConfigParser::from_string("BaseNode/GetTipInfo,Wallet/*", false).unwrap();
        let summary = parser.get_config_summary();
        assert!(summary.contains("1 specific methods"));
        assert!(summary.contains("1 wildcard services"));
        assert!(summary.contains("control operations disabled"));
    }
}
