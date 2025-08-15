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
//! Parameter Conversion Layer
//!
//! This module provides JSON to protobuf parameter conversion with method-specific handlers
//! and error handling for gRPC method execution.

use std::collections::HashMap;

use async_trait::async_trait;
use serde_json::Value;

use crate::McpError;

/// Error types for parameter conversion
#[derive(Debug, thiserror::Error)]
pub enum ConversionError {
    #[error("JSON deserialization failed for method '{method}': {source}")]
    JsonDeserialization {
        method: String,
        #[source]
        source: serde_json::Error,
    },

    #[error("Type conversion failed for method '{method}': {details}")]
    ProtoConversion { method: String, details: String },

    #[error("Unsupported method: {method}")]
    MethodNotFound { method: String },

    #[error("Missing required parameter '{param}' for method '{method}'")]
    MissingParameter { method: String, param: String },

    #[error("Invalid parameter type for '{param}' in method '{method}': expected {expected}, got {actual}")]
    InvalidParameterType {
        method: String,
        param: String,
        expected: String,
        actual: String,
    },
}

impl From<ConversionError> for McpError {
    fn from(err: ConversionError) -> Self {
        McpError::tool_execution_failed(format!("Parameter conversion error: {err}"))
    }
}

/// Trait for converting JSON parameters to protobuf messages
#[async_trait]
pub trait ParameterConverter: Send + Sync {
    /// The name of the method this converter handles
    fn method_name(&self) -> &str;

    /// Convert JSON parameters to a typed protobuf message
    async fn convert(&self, parameters: Value) -> Result<Box<dyn prost::Message + Send>, ConversionError>;

    /// Validate parameters without performing conversion (useful for schema validation)
    fn validate(&self, parameters: &Value) -> Result<(), ConversionError>;
}

/// Dynamic converter function type for trait object storage
pub type DynConverter = dyn Fn(Value) -> Result<Box<dyn prost::Message + Send>, ConversionError> + Send + Sync;

/// Registry for managing parameter converters by method name
pub struct ConversionRegistry {
    converters: HashMap<String, Box<DynConverter>>,
}

impl ConversionRegistry {
    /// Create a new empty conversion registry
    pub fn new() -> Self {
        Self {
            converters: HashMap::new(),
        }
    }

    /// Register a converter for a specific method name
    pub fn register<C>(&mut self, converter: C)
    where C: ParameterConverter + 'static {
        let method_name = converter.method_name().to_string();

        // Create a boxed closure that captures the converter
        let boxed_converter = Box::new(move |parameters: Value| {
            // Use tokio::task::block_in_place for sync conversion in async context
            tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(converter.convert(parameters)))
        });

        self.converters.insert(method_name, boxed_converter);
    }

    /// Convert parameters for a specific method
    pub fn convert(
        &self,
        method_name: &str,
        parameters: Value,
    ) -> Result<Box<dyn prost::Message + Send>, ConversionError> {
        match self.converters.get(method_name) {
            Some(converter) => converter(parameters),
            None => Err(ConversionError::MethodNotFound {
                method: method_name.to_string(),
            }),
        }
    }

    /// Check if a method has a registered converter
    pub fn has_converter(&self, method_name: &str) -> bool {
        self.converters.contains_key(method_name)
    }

    /// Get list of all registered method names
    pub fn registered_methods(&self) -> Vec<&String> {
        self.converters.keys().collect()
    }
}

impl Default for ConversionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper trait for extracting typed parameters from JSON
pub trait JsonParameterExtractor {
    /// Extract a required string parameter
    fn get_required_string(&self, method: &str, key: &str) -> Result<String, ConversionError>;

    /// Extract an optional string parameter
    fn get_optional_string(&self, key: &str) -> Option<String>;

    /// Extract a required u64 parameter
    fn get_required_u64(&self, method: &str, key: &str) -> Result<u64, ConversionError>;

    /// Extract an optional u64 parameter
    fn get_optional_u64(&self, key: &str) -> Option<u64>;

    /// Extract a required boolean parameter
    fn get_required_bool(&self, method: &str, key: &str) -> Result<bool, ConversionError>;

    /// Extract an optional boolean parameter
    fn get_optional_bool(&self, key: &str) -> Option<bool>;

    /// Extract a required array parameter
    fn get_required_array(&self, method: &str, key: &str) -> Result<&Vec<Value>, ConversionError>;

    /// Extract an optional array parameter
    fn get_optional_array(&self, key: &str) -> Option<&Vec<Value>>;
}

impl JsonParameterExtractor for Value {
    fn get_required_string(&self, method: &str, key: &str) -> Result<String, ConversionError> {
        match self.get(key) {
            Some(Value::String(s)) => Ok(s.clone()),
            Some(other) => Err(ConversionError::InvalidParameterType {
                method: method.to_string(),
                param: key.to_string(),
                expected: "string".to_string(),
                actual: format!("{other:?}"),
            }),
            None => Err(ConversionError::MissingParameter {
                method: method.to_string(),
                param: key.to_string(),
            }),
        }
    }

    fn get_optional_string(&self, key: &str) -> Option<String> {
        self.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
    }

    fn get_required_u64(&self, method: &str, key: &str) -> Result<u64, ConversionError> {
        match self.get(key) {
            Some(Value::Number(n)) => n.as_u64().ok_or_else(|| ConversionError::InvalidParameterType {
                method: method.to_string(),
                param: key.to_string(),
                expected: "unsigned integer".to_string(),
                actual: format!("{n}"),
            }),
            Some(other) => Err(ConversionError::InvalidParameterType {
                method: method.to_string(),
                param: key.to_string(),
                expected: "number".to_string(),
                actual: format!("{other:?}"),
            }),
            None => Err(ConversionError::MissingParameter {
                method: method.to_string(),
                param: key.to_string(),
            }),
        }
    }

    fn get_optional_u64(&self, key: &str) -> Option<u64> {
        self.get(key).and_then(|v| v.as_u64())
    }

    fn get_required_bool(&self, method: &str, key: &str) -> Result<bool, ConversionError> {
        match self.get(key) {
            Some(Value::Bool(b)) => Ok(*b),
            Some(other) => Err(ConversionError::InvalidParameterType {
                method: method.to_string(),
                param: key.to_string(),
                expected: "boolean".to_string(),
                actual: format!("{other:?}"),
            }),
            None => Err(ConversionError::MissingParameter {
                method: method.to_string(),
                param: key.to_string(),
            }),
        }
    }

    fn get_optional_bool(&self, key: &str) -> Option<bool> {
        self.get(key).and_then(|v| v.as_bool())
    }

    fn get_required_array(&self, method: &str, key: &str) -> Result<&Vec<Value>, ConversionError> {
        match self.get(key) {
            Some(Value::Array(arr)) => Ok(arr),
            Some(other) => Err(ConversionError::InvalidParameterType {
                method: method.to_string(),
                param: key.to_string(),
                expected: "array".to_string(),
                actual: format!("{other:?}"),
            }),
            None => Err(ConversionError::MissingParameter {
                method: method.to_string(),
                param: key.to_string(),
            }),
        }
    }

    fn get_optional_array(&self, key: &str) -> Option<&Vec<Value>> {
        self.get(key).and_then(|v| v.as_array())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn test_json_parameter_extractor() {
        let params = json!({
            "name": "test",
            "count": 42,
            "active": true,
            "items": [1, 2, 3]
        });

        assert_eq!(params.get_required_string("test_method", "name").unwrap(), "test");
        assert_eq!(params.get_required_u64("test_method", "count").unwrap(), 42);
        assert!(params.get_required_bool("test_method", "active").unwrap());
        assert_eq!(params.get_required_array("test_method", "items").unwrap().len(), 3);

        assert!(params.get_required_string("test_method", "missing").is_err());
        assert!(params.get_optional_string("missing").is_none());
    }
}
