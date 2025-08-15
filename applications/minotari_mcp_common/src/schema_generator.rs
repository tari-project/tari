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
//! JSON Schema Generation for gRPC Methods
//!
//! This module provides utilities for generating JSON schemas from gRPC method definitions
//! and validating parameters at runtime for MCP tool execution.

#![allow(clippy::indexing_slicing)]
use std::collections::HashMap;

use serde_json::{Map, Value};
use thiserror::Error;

use crate::grpc_discovery::{GrpcMethodCategory, GrpcMethodInfo, ServiceDiscovery};

/// Errors that can occur during schema generation or validation
#[derive(Debug, Error)]
pub enum SchemaError {
    #[error("Invalid JSON schema: {0}")]
    InvalidSchema(String),

    #[error("Schema validation failed: {0}")]
    ValidationFailed(String),

    #[error("Missing required parameter: {0}")]
    MissingParameter(String),

    #[error("Invalid parameter type for {field}: expected {expected}, got {actual}")]
    InvalidParameterType {
        field: String,
        expected: String,
        actual: String,
    },

    #[error("Parameter value out of range for {field}: {value}")]
    ValueOutOfRange { field: String, value: String },

    #[error("Invalid format for {field}: {reason}")]
    InvalidFormat { field: String, reason: String },
}

/// Schema generator for gRPC methods
#[derive(Debug, Clone)]
pub struct SchemaGenerator {
    /// Schemas for all available methods
    method_schemas: HashMap<String, GrpcMethodInfo>,
}

impl SchemaGenerator {
    /// Create a new schema generator from service discovery
    pub fn new(discovery: &ServiceDiscovery) -> Self {
        let mut method_schemas = HashMap::new();

        for method in &discovery.methods {
            method_schemas.insert(method.full_name.clone(), method.clone());
        }

        Self { method_schemas }
    }

    /// Get input schema for a specific method
    pub fn get_input_schema(&self, method_name: &str) -> Option<&Value> {
        self.method_schemas.get(method_name).map(|m| &m.input_schema)
    }

    /// Get output schema for a specific method
    pub fn get_output_schema(&self, method_name: &str) -> Option<&Value> {
        self.method_schemas.get(method_name).map(|m| &m.output_schema)
    }

    /// Get method information
    pub fn get_method_info(&self, method_name: &str) -> Option<&GrpcMethodInfo> {
        self.method_schemas.get(method_name)
    }

    /// Validate input parameters against schema
    pub fn validate_input(&self, method_name: &str, params: &Value) -> Result<(), SchemaError> {
        let schema = self
            .get_input_schema(method_name)
            .ok_or_else(|| SchemaError::InvalidSchema(format!("No schema found for method: {method_name}")))?;

        self.validate_value(params, schema, "root")
    }

    /// Validate output response against schema
    pub fn validate_output(&self, method_name: &str, response: &Value) -> Result<(), SchemaError> {
        let schema = self
            .get_output_schema(method_name)
            .ok_or_else(|| SchemaError::InvalidSchema(format!("No output schema found for method: {method_name}")))?;

        self.validate_value(response, schema, "root")
    }

    /// Generate MCP tool schema from gRPC method
    pub fn generate_mcp_tool_schema(&self, method_name: &str) -> Result<Value, SchemaError> {
        let method_info = self
            .get_method_info(method_name)
            .ok_or_else(|| SchemaError::InvalidSchema(format!("Method not found: {method_name}")))?;

        let mut tool_schema = serde_json::json!({
            "name": self.method_to_tool_name(&method_info.name),
            "description": method_info.description,
            "inputSchema": method_info.input_schema.clone()
        });

        // Add metadata
        if let Some(obj) = tool_schema.as_object_mut() {
            obj.insert(
                "category".to_string(),
                serde_json::json!(method_info.category.to_string()),
            );
            obj.insert(
                "isControlOperation".to_string(),
                serde_json::json!(method_info.is_control_operation),
            );
            obj.insert("isStreaming".to_string(), serde_json::json!(method_info.is_streaming));
            obj.insert("service".to_string(), serde_json::json!(method_info.service));
            obj.insert("grpcMethod".to_string(), serde_json::json!(method_info.full_name));
        }

        Ok(tool_schema)
    }

    /// Generate all MCP tool schemas for a category
    pub fn generate_category_schemas(&self, category: GrpcMethodCategory) -> Vec<Value> {
        self.method_schemas
            .values()
            .filter(|m| m.category == category)
            .filter_map(|m| self.generate_mcp_tool_schema(&m.full_name).ok())
            .collect()
    }

    /// Generate OpenAPI-style documentation for all methods
    pub fn generate_openapi_docs(&self) -> Value {
        let mut paths = Map::new();

        for method in self.method_schemas.values() {
            let path_name = format!(
                "/{}/{}",
                method.service.to_lowercase(),
                self.method_to_path_name(&method.name)
            );

            let operation = serde_json::json!({
                "summary": method.description,
                "operationId": self.method_to_tool_name(&method.name),
                "tags": [method.category.to_string()],
                "requestBody": {
                    "required": true,
                    "content": {
                        "application/json": {
                            "schema": method.input_schema
                        }
                    }
                },
                "responses": {
                    "200": {
                        "description": "Successful response",
                        "content": {
                            "application/json": {
                                "schema": method.output_schema
                            }
                        }
                    },
                    "400": {
                        "description": "Invalid request parameters"
                    },
                    "403": {
                        "description": "Control operation requires explicit consent"
                    },
                    "500": {
                        "description": "Internal server error"
                    }
                }
            });

            paths.insert(
                path_name,
                serde_json::json!({
                    "post": operation
                }),
            );
        }

        serde_json::json!({
            "openapi": "3.0.0",
            "info": {
                "title": "Tari gRPC API",
                "version": "1.0.0",
                "description": "Auto-generated API documentation for Tari gRPC services"
            },
            "paths": paths,
            "components": {
                "schemas": self.generate_common_schemas()
            }
        })
    }

    /// Convert gRPC method name to MCP tool name
    fn method_to_tool_name(&self, method_name: &str) -> String {
        // Convert PascalCase to snake_case
        let mut result = String::new();
        let chars = method_name.chars().peekable();

        for ch in chars {
            if ch.is_uppercase() && !result.is_empty() {
                result.push('_');
            }
            result.push(ch.to_lowercase().next().unwrap());
        }

        result
    }

    /// Convert gRPC method name to API path name
    fn method_to_path_name(&self, method_name: &str) -> String {
        self.method_to_tool_name(method_name).replace('_', "-")
    }

    /// Validate a JSON value against a schema
    fn validate_value(&self, value: &Value, schema: &Value, path: &str) -> Result<(), SchemaError> {
        let schema_obj = schema
            .as_object()
            .ok_or_else(|| SchemaError::InvalidSchema("Schema must be an object".to_string()))?;

        // Check type
        if let Some(schema_type) = schema_obj.get("type") {
            self.validate_type(value, schema_type, path)?;
        }

        // Check required properties
        if let Some(required) = schema_obj.get("required") {
            if let Some(required_array) = required.as_array() {
                if let Some(obj) = value.as_object() {
                    for req_prop in required_array {
                        if let Some(prop_name) = req_prop.as_str() {
                            if !obj.contains_key(prop_name) {
                                return Err(SchemaError::MissingParameter(format!("{path}.{prop_name}")));
                            }
                        }
                    }
                }
            }
        }

        // Validate properties
        if let Some(properties) = schema_obj.get("properties") {
            if let Some(props_obj) = properties.as_object() {
                if let Some(value_obj) = value.as_object() {
                    for (prop_name, prop_value) in value_obj {
                        if let Some(prop_schema) = props_obj.get(prop_name) {
                            let prop_path = format!("{path}.{prop_name}");
                            self.validate_value(prop_value, prop_schema, &prop_path)?;
                        }
                    }
                }
            }
        }

        // Validate array items
        if let (Some(items_schema), Some(array_value)) = (schema_obj.get("items"), value.as_array()) {
            for (i, item) in array_value.iter().enumerate() {
                let item_path = format!("{path}[{i}]");
                self.validate_value(item, items_schema, &item_path)?;
            }
        }

        // Check minimum/maximum for numbers
        if let Some(num_value) = value.as_f64() {
            if let Some(minimum) = schema_obj.get("minimum") {
                if let Some(min_val) = minimum.as_f64() {
                    if num_value < min_val {
                        return Err(SchemaError::ValueOutOfRange {
                            field: path.to_string(),
                            value: num_value.to_string(),
                        });
                    }
                }
            }

            if let Some(maximum) = schema_obj.get("maximum") {
                if let Some(max_val) = maximum.as_f64() {
                    if num_value > max_val {
                        return Err(SchemaError::ValueOutOfRange {
                            field: path.to_string(),
                            value: num_value.to_string(),
                        });
                    }
                }
            }
        }

        // Check string patterns and formats
        if let Some(str_value) = value.as_str() {
            if let Some(pattern) = schema_obj.get("pattern") {
                if let Some(pattern_str) = pattern.as_str() {
                    if let Ok(regex) = regex::Regex::new(pattern_str) {
                        if !regex.is_match(str_value) {
                            return Err(SchemaError::InvalidFormat {
                                field: path.to_string(),
                                reason: format!("does not match pattern: {pattern_str}"),
                            });
                        }
                    }
                }
            }

            if let Some(format) = schema_obj.get("format") {
                if let Some(format_str) = format.as_str() {
                    self.validate_string_format(str_value, format_str, path)?;
                }
            }
        }

        Ok(())
    }

    /// Validate JSON value type
    fn validate_type(&self, value: &Value, schema_type: &Value, path: &str) -> Result<(), SchemaError> {
        let expected_type = schema_type
            .as_str()
            .ok_or_else(|| SchemaError::InvalidSchema("Type must be a string".to_string()))?;

        let actual_type = match value {
            Value::Null => "null",
            Value::Bool(_) => "boolean",
            Value::Number(_) => {
                if value.as_i64().is_some() || value.as_u64().is_some() {
                    "integer"
                } else {
                    "number"
                }
            },
            Value::String(_) => "string",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
        };

        if expected_type != actual_type && !(expected_type == "number" && actual_type == "integer") {
            return Err(SchemaError::InvalidParameterType {
                field: path.to_string(),
                expected: expected_type.to_string(),
                actual: actual_type.to_string(),
            });
        }

        Ok(())
    }

    /// Validate string format
    fn validate_string_format(&self, value: &str, format: &str, path: &str) -> Result<(), SchemaError> {
        match format {
            "byte" => {
                // Validate base64 or hex string
                if !value.chars().all(|c| c.is_alphanumeric() || "=+/".contains(c)) {
                    return Err(SchemaError::InvalidFormat {
                        field: path.to_string(),
                        reason: "invalid byte string format".to_string(),
                    });
                }
            },
            "uint64" => {
                if value.parse::<u64>().is_err() {
                    return Err(SchemaError::InvalidFormat {
                        field: path.to_string(),
                        reason: "invalid uint64 format".to_string(),
                    });
                }
            },
            "uint32" => {
                if value.parse::<u32>().is_err() {
                    return Err(SchemaError::InvalidFormat {
                        field: path.to_string(),
                        reason: "invalid uint32 format".to_string(),
                    });
                }
            },
            _ => {
                // Unknown format, skip validation
            },
        }

        Ok(())
    }

    /// Generate common schemas used across methods
    fn generate_common_schemas(&self) -> Value {
        serde_json::json!({
            "Error": {
                "type": "object",
                "properties": {
                    "code": {
                        "type": "integer",
                        "description": "Error code"
                    },
                    "message": {
                        "type": "string",
                        "description": "Error message"
                    },
                    "details": {
                        "type": "string",
                        "description": "Additional error details"
                    }
                },
                "required": ["code", "message"]
            },
            "TransactionOutput": {
                "type": "object",
                "properties": {
                    "commitment": {
                        "type": "string",
                        "format": "byte",
                        "description": "Output commitment"
                    },
                    "features": {
                        "type": "object",
                        "description": "Output features"
                    },
                    "script": {
                        "type": "string",
                        "format": "byte",
                        "description": "TariScript"
                    }
                }
            },
            "Transaction": {
                "type": "object",
                "properties": {
                    "offset": {
                        "type": "string",
                        "format": "byte",
                        "description": "Transaction offset"
                    },
                    "body": {
                        "type": "object",
                        "description": "Transaction body"
                    }
                }
            },
            "BlockHeader": {
                "type": "object",
                "properties": {
                    "version": {
                        "type": "integer",
                        "format": "uint32",
                        "description": "Block version"
                    },
                    "height": {
                        "type": "integer",
                        "format": "uint64",
                        "description": "Block height"
                    },
                    "prev_hash": {
                        "type": "string",
                        "format": "byte",
                        "description": "Previous block hash"
                    },
                    "timestamp": {
                        "type": "integer",
                        "format": "uint64",
                        "description": "Block timestamp"
                    },
                    "merkle_roots_hash": {
                        "type": "string",
                        "format": "byte",
                        "description": "Merkle roots hash"
                    },
                    "pow": {
                        "type": "object",
                        "description": "Proof of work"
                    }
                }
            }
        })
    }
}

impl Default for SchemaGenerator {
    fn default() -> Self {
        let discovery = ServiceDiscovery::default();
        Self::new(&discovery)
    }
}

/// Utility functions for schema generation
pub mod utils {
    use super::*;

    /// Generate a basic JSON schema for a simple type
    pub fn simple_type_schema(type_name: &str, description: &str) -> Value {
        serde_json::json!({
            "type": type_name,
            "description": description
        })
    }

    /// Generate an array schema
    pub fn array_schema(item_schema: Value, description: &str) -> Value {
        serde_json::json!({
            "type": "array",
            "items": item_schema,
            "description": description
        })
    }

    /// Generate an object schema with properties
    pub fn object_schema(properties: Map<String, Value>, required: Vec<String>, description: &str) -> Value {
        let mut schema = serde_json::json!({
            "type": "object",
            "properties": properties,
            "description": description,
            "additionalProperties": false
        });

        if !required.is_empty() {
            schema["required"] = serde_json::json!(required);
        }

        schema
    }

    /// Generate an enum schema
    pub fn enum_schema(values: Vec<Value>, description: &str) -> Value {
        serde_json::json!({
            "enum": values,
            "description": description
        })
    }

    /// Generate a string schema with format and pattern
    pub fn string_schema(format: Option<&str>, pattern: Option<&str>, description: &str) -> Value {
        let mut schema = serde_json::json!({
            "type": "string",
            "description": description
        });

        if let Some(fmt) = format {
            schema["format"] = serde_json::json!(fmt);
        }

        if let Some(pat) = pattern {
            schema["pattern"] = serde_json::json!(pat);
        }

        schema
    }

    /// Generate an integer schema with range constraints
    pub fn integer_schema(
        format: Option<&str>,
        minimum: Option<i64>,
        maximum: Option<i64>,
        description: &str,
    ) -> Value {
        let mut schema = serde_json::json!({
            "type": "integer",
            "description": description
        });

        if let Some(fmt) = format {
            schema["format"] = serde_json::json!(fmt);
        }

        if let Some(min) = minimum {
            schema["minimum"] = serde_json::json!(min);
        }

        if let Some(max) = maximum {
            schema["maximum"] = serde_json::json!(max);
        }

        schema
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grpc_discovery::{base_node_methods, wallet_methods};

    #[test]
    fn test_schema_generator_creation() {
        let mut discovery = ServiceDiscovery::new();
        for method in base_node_methods() {
            discovery.add_method(method);
        }
        for method in wallet_methods() {
            discovery.add_method(method);
        }

        let generator = SchemaGenerator::new(&discovery);
        assert!(!generator.method_schemas.is_empty());
    }

    #[test]
    fn test_method_name_conversion() {
        let discovery = ServiceDiscovery::new();
        let generator = SchemaGenerator::new(&discovery);

        assert_eq!(generator.method_to_tool_name("GetBalance"), "get_balance");
        assert_eq!(generator.method_to_tool_name("ListHeaders"), "list_headers");
        assert_eq!(generator.method_to_tool_name("SubmitTransaction"), "submit_transaction");
    }

    #[test]
    fn test_input_validation() {
        let mut discovery = ServiceDiscovery::new();
        discovery.add_method(base_node_methods()[0].clone()); // ListHeaders

        let generator = SchemaGenerator::new(&discovery);

        // Valid input
        let valid_input = serde_json::json!({
            "from_height": 100,
            "num_headers": 10,
            "sorting": 0
        });

        let result = generator.validate_input("tari.rpc.BaseNode/ListHeaders", &valid_input);
        assert!(result.is_ok());

        // Invalid input - missing type
        let invalid_input = serde_json::json!({
            "from_height": "not_a_number",
            "num_headers": 10
        });

        let result = generator.validate_input("tari.rpc.BaseNode/ListHeaders", &invalid_input);
        assert!(result.is_err());
    }

    #[test]
    fn test_mcp_tool_schema_generation() {
        let mut discovery = ServiceDiscovery::new();
        discovery.add_method(base_node_methods()[0].clone()); // ListHeaders

        let generator = SchemaGenerator::new(&discovery);

        let schema = generator
            .generate_mcp_tool_schema("tari.rpc.BaseNode/ListHeaders")
            .unwrap();

        assert_eq!(schema["name"], "list_headers");
        assert_eq!(schema["category"], "blockchain");
        assert_eq!(schema["isControlOperation"], false);
    }

    #[test]
    fn test_openapi_docs_generation() {
        let mut discovery = ServiceDiscovery::new();
        discovery.add_method(base_node_methods()[0].clone()); // ListHeaders

        let generator = SchemaGenerator::new(&discovery);
        let docs = generator.generate_openapi_docs();

        assert_eq!(docs["openapi"], "3.0.0");
        assert!(docs["paths"].is_object());
        assert!(docs["components"]["schemas"].is_object());
    }
}
