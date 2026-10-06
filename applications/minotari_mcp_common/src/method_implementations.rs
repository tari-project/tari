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
//! Method-Specific Parameter Converters
//!
//! This module provides concrete implementations of parameter converters for
//! specific gRPC methods, handling JSON to protobuf conversion with validation.

use async_trait::async_trait;
// Import gRPC types for parameter conversion
use minotari_node_grpc_client::grpc::{Empty, GetBlocksRequest};
use serde_json::Value;

use crate::parameter_converter::{ConversionError, JsonParameterExtractor, ParameterConverter};

/// Converter for GetTipInfo method (no parameters required)
pub struct GetTipInfoConverter;

#[async_trait]
impl ParameterConverter for GetTipInfoConverter {
    fn method_name(&self) -> &str {
        "GetTipInfo"
    }

    async fn convert(&self, _parameters: Value) -> Result<Box<dyn prost::Message + Send>, ConversionError> {
        // GetTipInfo requires no parameters, just return Empty
        Ok(Box::new(Empty {}))
    }

    fn validate(&self, _parameters: &Value) -> Result<(), ConversionError> {
        // No validation needed for Empty message
        Ok(())
    }
}

/// Converter for GetBlocks method (requires heights array)
pub struct GetBlocksConverter;

#[async_trait]
impl ParameterConverter for GetBlocksConverter {
    fn method_name(&self) -> &str {
        "GetBlocks"
    }

    async fn convert(&self, parameters: Value) -> Result<Box<dyn prost::Message + Send>, ConversionError> {
        let method_name = self.method_name();

        // Extract heights array - can be empty for latest blocks
        let heights = match parameters.get_optional_array("heights") {
            Some(heights_array) => {
                let mut heights = Vec::new();
                for (index, height_value) in heights_array.iter().enumerate() {
                    match height_value.as_u64() {
                        Some(height) => heights.push(height),
                        None => {
                            return Err(ConversionError::InvalidParameterType {
                                method: method_name.to_string(),
                                param: format!("heights[{index}]"),
                                expected: "unsigned integer".to_string(),
                                actual: format!("{height_value:?}"),
                            });
                        },
                    }
                }
                heights
            },
            None => Vec::new(), // Default to empty heights for latest blocks
        };

        Ok(Box::new(GetBlocksRequest { heights }))
    }

    fn validate(&self, parameters: &Value) -> Result<(), ConversionError> {
        let method_name = self.method_name();

        // Validate heights array if provided
        if let Some(heights_array) = parameters.get_optional_array("heights") {
            for (index, height_value) in heights_array.iter().enumerate() {
                if !height_value.is_u64() {
                    return Err(ConversionError::InvalidParameterType {
                        method: method_name.to_string(),
                        param: format!("heights[{index}]"),
                        expected: "unsigned integer".to_string(),
                        actual: format!("{height_value:?}"),
                    });
                }
            }
        }

        Ok(())
    }
}

/// Converter for GetVersion method (no parameters required)
pub struct GetVersionConverter;

#[async_trait]
impl ParameterConverter for GetVersionConverter {
    fn method_name(&self) -> &str {
        "GetVersion"
    }

    async fn convert(&self, _parameters: Value) -> Result<Box<dyn prost::Message + Send>, ConversionError> {
        Ok(Box::new(Empty {}))
    }

    fn validate(&self, _parameters: &Value) -> Result<(), ConversionError> {
        Ok(())
    }
}

/// Converter for GetPeers method (no parameters required)
pub struct GetPeersConverter;

#[async_trait]
impl ParameterConverter for GetPeersConverter {
    fn method_name(&self) -> &str {
        "GetPeers"
    }

    async fn convert(&self, _parameters: Value) -> Result<Box<dyn prost::Message + Send>, ConversionError> {
        Ok(Box::new(Empty {}))
    }

    fn validate(&self, _parameters: &Value) -> Result<(), ConversionError> {
        Ok(())
    }
}

/// Register all node method converters in the provided registry
pub fn register_node_converters(registry: &mut crate::parameter_converter::ConversionRegistry) {
    registry.register(GetTipInfoConverter);
    registry.register(GetBlocksConverter);
    registry.register(GetVersionConverter);
    registry.register(GetPeersConverter);
}

/// Factory function to create all node method converters (deprecated)
#[deprecated(note = "Use register_node_converters instead")]
pub fn create_node_converters() -> Vec<Box<dyn ParameterConverter>> {
    vec![
        Box::new(GetTipInfoConverter),
        Box::new(GetBlocksConverter),
        Box::new(GetVersionConverter),
        Box::new(GetPeersConverter),
    ]
}

// Wallet method converters (placeholder implementations for now)

/// Converter for GetBalance method (no parameters required)
pub struct GetBalanceConverter;

#[async_trait]
impl ParameterConverter for GetBalanceConverter {
    fn method_name(&self) -> &str {
        "GetBalance"
    }

    async fn convert(&self, _parameters: Value) -> Result<Box<dyn prost::Message + Send>, ConversionError> {
        Ok(Box::new(Empty {}))
    }

    fn validate(&self, _parameters: &Value) -> Result<(), ConversionError> {
        Ok(())
    }
}

/// Register all wallet method converters in the provided registry
pub fn register_wallet_converters(registry: &mut crate::parameter_converter::ConversionRegistry) {
    registry.register(GetBalanceConverter);
    // TODO: Add more wallet converters as needed
}

/// Factory function to create all wallet method converters (deprecated)
#[deprecated(note = "Use register_wallet_converters instead")]
pub fn create_wallet_converters() -> Vec<Box<dyn ParameterConverter>> {
    vec![
        Box::new(GetBalanceConverter),
        // TODO: Add more wallet converters as needed
    ]
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[tokio::test]
    async fn test_get_tip_info_converter() {
        let converter = GetTipInfoConverter;
        let result = converter.convert(json!({})).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_blocks_converter() {
        let converter = GetBlocksConverter;

        // Test with valid heights
        let params = json!({"heights": [100, 200, 300]});
        let result = converter.convert(params).await;
        assert!(result.is_ok());

        // Test with empty parameters (should default to empty heights)
        let result = converter.convert(json!({})).await;
        assert!(result.is_ok());

        // Test with invalid height type
        let params = json!({"heights": ["invalid"]});
        let result = converter.convert(params).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_get_blocks_validation() {
        let converter = GetBlocksConverter;

        // Valid parameters
        let params = json!({"heights": [100, 200]});
        assert!(converter.validate(&params).is_ok());

        // Invalid parameters
        let params = json!({"heights": ["invalid"]});
        assert!(converter.validate(&params).is_err());
    }
}
