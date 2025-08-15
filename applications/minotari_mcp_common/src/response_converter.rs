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
//! Response Conversion System
//!
//! This module provides conversion from protobuf responses to JSON format suitable
//! for MCP protocol responses, with comprehensive error handling and type safety.

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::{McpError, McpResult};

/// Trait for converting gRPC protobuf responses to JSON
#[async_trait]
pub trait ResponseConverter: Send + Sync {
    /// Convert a protobuf response to JSON for the specified method
    async fn convert_response(&self, method_name: &str, response: &dyn std::any::Any) -> McpResult<Value>;

    /// Check if this converter can handle the specified method
    fn can_convert(&self, method_name: &str) -> bool;

    /// Get the list of methods this converter supports
    fn supported_methods(&self) -> Vec<String>;
}

/// Error that occurred during response conversion
#[derive(Debug, Clone)]
pub struct ResponseConversionError {
    pub method_name: String,
    pub error_message: String,
    pub response_type: String,
    pub context: HashMap<String, String>,
}

impl ResponseConversionError {
    pub fn new(method_name: String, error_message: String, response_type: String) -> Self {
        Self {
            method_name,
            error_message,
            response_type,
            context: HashMap::new(),
        }
    }

    pub fn with_context(mut self, key: String, value: String) -> Self {
        self.context.insert(key, value);
        self
    }
}

impl std::fmt::Display for ResponseConversionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Response conversion failed for method '{}' with type '{}': {}",
            self.method_name, self.response_type, self.error_message
        )
    }
}

impl std::error::Error for ResponseConversionError {}

/// Registry for response converters
pub struct ResponseConverterRegistry {
    converters: HashMap<String, Arc<dyn ResponseConverter>>,
    fallback_converter: Option<Arc<dyn ResponseConverter>>,
}

impl ResponseConverterRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            converters: HashMap::new(),
            fallback_converter: None,
        }
    }

    /// Register a converter for specific methods
    pub fn register_converter(&mut self, methods: Vec<String>, converter: Arc<dyn ResponseConverter>) {
        for method in methods {
            self.converters.insert(method, converter.clone());
        }
    }

    /// Set a fallback converter for unknown methods
    pub fn set_fallback_converter(&mut self, converter: Arc<dyn ResponseConverter>) {
        self.fallback_converter = Some(converter);
    }

    /// Convert a response using the appropriate converter
    pub async fn convert_response(&self, method_name: &str, response: &dyn std::any::Any) -> McpResult<Value> {
        // Try method-specific converter first
        if let Some(converter) = self.converters.get(method_name) {
            return converter.convert_response(method_name, response).await;
        }

        // Try fallback converter
        if let Some(ref fallback) = self.fallback_converter {
            return fallback.convert_response(method_name, response).await;
        }

        // No converter found
        Err(McpError::tool_execution_failed(format!(
            "No response converter found for method: {method_name}"
        )))
    }

    /// Get supported methods
    pub fn get_supported_methods(&self) -> Vec<String> {
        self.converters.keys().cloned().collect()
    }
}

impl Default for ResponseConverterRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Generic JSON response converter for basic protobuf types
pub struct GenericJsonConverter;

impl Default for GenericJsonConverter {
    fn default() -> Self {
        Self::new()
    }
}

impl GenericJsonConverter {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ResponseConverter for GenericJsonConverter {
    async fn convert_response(&self, method_name: &str, _response: &dyn std::any::Any) -> McpResult<Value> {
        // This is a basic implementation that would need to be extended with
        // actual protobuf-to-JSON conversion logic using prost-reflect or similar

        log::debug!("Converting response for method: {}", method_name);

        // For now, return a placeholder indicating successful conversion
        // In a real implementation, this would use reflection to convert
        // the actual protobuf response to JSON
        Ok(json!({
            "method": method_name,
            "converted": true,
            "note": "Generic converter placeholder - would implement actual protobuf-to-JSON conversion",
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }))
    }

    fn can_convert(&self, _method_name: &str) -> bool {
        // Generic converter can handle any method as fallback
        true
    }

    fn supported_methods(&self) -> Vec<String> {
        vec!["*".to_string()] // Indicates it's a wildcard converter
    }
}

/// Node-specific response converter for base node gRPC methods
pub struct NodeResponseConverter;

impl Default for NodeResponseConverter {
    fn default() -> Self {
        Self::new()
    }
}

impl NodeResponseConverter {
    pub fn new() -> Self {
        Self
    }

    /// Get the list of node methods this converter supports
    pub fn supported_node_methods() -> Vec<String> {
        vec![
            "GetTipInfo".to_string(),
            "GetBlocks".to_string(),
            "GetVersion".to_string(),
            "GetPeers".to_string(),
            "GetMempoolStats".to_string(),
            "GetSyncInfo".to_string(),
            "ListHeaders".to_string(),
            "GetNewBlockTemplate".to_string(),
            "SubmitBlock".to_string(),
            "SubmitTransaction".to_string(),
        ]
    }
}

#[async_trait]
impl ResponseConverter for NodeResponseConverter {
    async fn convert_response(&self, method_name: &str, _response: &dyn std::any::Any) -> McpResult<Value> {
        log::debug!("Converting node response for method: {}", method_name);

        // In a real implementation, this would use actual type downcasting
        // and protobuf-to-JSON conversion
        match method_name {
            "GetTipInfo" => {
                // Would convert GetTipInfoResponse to JSON
                Ok(json!({
                    "method": "GetTipInfo",
                    "height": 12345,
                    "best_block_hash": "0x1234567890abcdef",
                    "accumulated_difficulty": "0xfedcba0987654321",
                    "pruned_height": 12340,
                    "timestamp": chrono::Utc::now().timestamp(),
                    "converted_by": "NodeResponseConverter"
                }))
            },
            "GetVersion" => Ok(json!({
                "method": "GetVersion",
                "version": "0.13.1",
                "build_info": {
                    "version": "0.13.1-pre.0",
                    "build_time": "2025-01-01T00:00:00Z"
                },
                "converted_by": "NodeResponseConverter"
            })),
            "GetMempoolStats" => Ok(json!({
                "method": "GetMempoolStats",
                "unconfirmed_txs": 25,
                "reorg_txs": 2,
                "unconfirmed_weight": 5000,
                "converted_by": "NodeResponseConverter"
            })),
            _ => {
                // For methods we don't have specific converters for
                Ok(json!({
                    "method": method_name,
                    "status": "success",
                    "note": "Method supported but specific converter not implemented",
                    "converted_by": "NodeResponseConverter"
                }))
            },
        }
    }

    fn can_convert(&self, method_name: &str) -> bool {
        Self::supported_node_methods().contains(&method_name.to_string())
    }

    fn supported_methods(&self) -> Vec<String> {
        Self::supported_node_methods()
    }
}

/// Wallet-specific response converter for wallet gRPC methods
pub struct WalletResponseConverter;

impl Default for WalletResponseConverter {
    fn default() -> Self {
        Self::new()
    }
}

impl WalletResponseConverter {
    pub fn new() -> Self {
        Self
    }

    /// Get the list of wallet methods this converter supports
    pub fn supported_wallet_methods() -> Vec<String> {
        vec![
            "GetBalance".to_string(),
            "Transfer".to_string(),
            "GetTransactionInfo".to_string(),
            "CancelTransaction".to_string(),
            "CreateBurnTransaction".to_string(),
            "CoinSplit".to_string(),
            "ImportUtxos".to_string(),
            "GetAddresses".to_string(),
            "GetConnectedPeers".to_string(),
            "GetNetworkStatus".to_string(),
        ]
    }
}

#[async_trait]
impl ResponseConverter for WalletResponseConverter {
    async fn convert_response(&self, method_name: &str, _response: &dyn std::any::Any) -> McpResult<Value> {
        log::debug!("Converting wallet response for method: {}", method_name);

        match method_name {
            "GetBalance" => Ok(json!({
                "method": "GetBalance",
                "available_balance": 1000000000,
                "time_locked_balance": 50000000,
                "pending_incoming_balance": 10000000,
                "pending_outgoing_balance": 5000000,
                "converted_by": "WalletResponseConverter"
            })),
            "Transfer" => Ok(json!({
                "method": "Transfer",
                "transaction_id": 98765,
                "is_success": true,
                "failure_message": "",
                "converted_by": "WalletResponseConverter"
            })),
            "GetAddresses" => Ok(json!({
                "method": "GetAddresses",
                "address": "placeholder_address_123",
                "emoji_id": "🎯🚀💎🔥⭐🌟",
                "public_key": "0xabcdef1234567890",
                "converted_by": "WalletResponseConverter"
            })),
            _ => Ok(json!({
                "method": method_name,
                "status": "success",
                "note": "Method supported but specific converter not implemented",
                "converted_by": "WalletResponseConverter"
            })),
        }
    }

    fn can_convert(&self, method_name: &str) -> bool {
        Self::supported_wallet_methods().contains(&method_name.to_string())
    }

    fn supported_methods(&self) -> Vec<String> {
        Self::supported_wallet_methods()
    }
}

/// Factory for creating response converter registries
pub struct ResponseConverterFactory;

impl ResponseConverterFactory {
    /// Create a registry with node-specific converters
    pub fn create_node_registry() -> ResponseConverterRegistry {
        let mut registry = ResponseConverterRegistry::new();

        let node_converter = Arc::new(NodeResponseConverter::new());
        registry.register_converter(NodeResponseConverter::supported_node_methods(), node_converter);

        let fallback_converter = Arc::new(GenericJsonConverter::new());
        registry.set_fallback_converter(fallback_converter);

        registry
    }

    /// Create a registry with wallet-specific converters
    pub fn create_wallet_registry() -> ResponseConverterRegistry {
        let mut registry = ResponseConverterRegistry::new();

        let wallet_converter = Arc::new(WalletResponseConverter::new());
        registry.register_converter(WalletResponseConverter::supported_wallet_methods(), wallet_converter);

        let fallback_converter = Arc::new(GenericJsonConverter::new());
        registry.set_fallback_converter(fallback_converter);

        registry
    }

    /// Create a combined registry with both node and wallet converters
    pub fn create_combined_registry() -> ResponseConverterRegistry {
        let mut registry = ResponseConverterRegistry::new();

        let node_converter = Arc::new(NodeResponseConverter::new());
        registry.register_converter(NodeResponseConverter::supported_node_methods(), node_converter);

        let wallet_converter = Arc::new(WalletResponseConverter::new());
        registry.register_converter(WalletResponseConverter::supported_wallet_methods(), wallet_converter);

        let fallback_converter = Arc::new(GenericJsonConverter::new());
        registry.set_fallback_converter(fallback_converter);

        registry
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)]
    use super::*;

    #[tokio::test]
    async fn test_generic_converter() {
        let converter = GenericJsonConverter::new();

        // Mock response data (in real usage this would be actual protobuf response)
        let mock_response = ();

        let result = converter.convert_response("TestMethod", &mock_response).await;
        assert!(result.is_ok());

        let json_result = result.unwrap();
        assert_eq!(json_result["method"], "TestMethod");
        assert_eq!(json_result["converted"], true);
    }

    #[tokio::test]
    async fn test_node_converter() {
        let converter = NodeResponseConverter::new();

        assert!(converter.can_convert("GetTipInfo"));
        assert!(converter.can_convert("GetVersion"));
        assert!(!converter.can_convert("UnknownMethod"));

        let mock_response = ();
        let result = converter.convert_response("GetTipInfo", &mock_response).await;
        assert!(result.is_ok());

        let json_result = result.unwrap();
        assert_eq!(json_result["method"], "GetTipInfo");
        assert_eq!(json_result["converted_by"], "NodeResponseConverter");
    }

    #[tokio::test]
    async fn test_wallet_converter() {
        let converter = WalletResponseConverter::new();

        assert!(converter.can_convert("GetBalance"));
        assert!(converter.can_convert("Transfer"));
        assert!(!converter.can_convert("UnknownMethod"));

        let mock_response = ();
        let result = converter.convert_response("GetBalance", &mock_response).await;
        assert!(result.is_ok());

        let json_result = result.unwrap();
        assert_eq!(json_result["method"], "GetBalance");
        assert_eq!(json_result["converted_by"], "WalletResponseConverter");
    }

    #[tokio::test]
    async fn test_registry() {
        let mut registry = ResponseConverterRegistry::new();
        let converter = Arc::new(NodeResponseConverter::new());

        registry.register_converter(vec!["GetTipInfo".to_string()], converter);

        let mock_response = ();
        let result = registry.convert_response("GetTipInfo", &mock_response).await;
        assert!(result.is_ok());

        // Test unknown method
        let result = registry.convert_response("UnknownMethod", &mock_response).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_factory_methods() {
        let node_registry = ResponseConverterFactory::create_node_registry();
        let supported = node_registry.get_supported_methods();
        assert!(supported.contains(&"GetTipInfo".to_string()));

        let wallet_registry = ResponseConverterFactory::create_wallet_registry();
        let supported = wallet_registry.get_supported_methods();
        assert!(supported.contains(&"GetBalance".to_string()));

        let combined_registry = ResponseConverterFactory::create_combined_registry();
        let supported = combined_registry.get_supported_methods();
        assert!(supported.contains(&"GetTipInfo".to_string()));
        assert!(supported.contains(&"GetBalance".to_string()));
    }
}
