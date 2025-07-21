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
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.//! Common MCP (Model Context Protocol) infrastructure for Tari applications
//! Integration module demonstrating ProtobufReflector usage with auto-discovery
//!
//! This module shows how to integrate the ProtobufReflector with the existing
//! auto-discovery system to provide enhanced runtime schema generation capabilities.

use crate::{
    ProtobufReflector, 
    AutoDiscoveryRegistry, 
    GrpcExecutor, 
    McpError, 
    McpResult,
    GrpcMethodInfo,
    ToolMetadata,
    ToolCategory,
    ToolRiskLevel
};
use schemars::schema::Schema;
use std::collections::HashMap;
use serde_json::Value;

/// Enhanced auto-discovery with runtime protobuf reflection
pub struct ReflectiveAutoDiscovery {
    registry: AutoDiscoveryRegistry,
    reflector: Option<ProtobufReflector>,
    method_schemas: HashMap<String, Schema>,
}

impl ReflectiveAutoDiscovery {
    /// Create new reflective auto-discovery
    pub fn new(registry: AutoDiscoveryRegistry) -> Self {
        Self {
            registry,
            reflector: None,
            method_schemas: HashMap::new(),
        }
    }

    /// Initialize with protobuf reflection capabilities
    pub fn with_reflection(mut self, descriptor_set: &[u8]) -> McpResult<Self> {
        let mut reflector = ProtobufReflector::new(descriptor_set)?;
        
        // Pre-generate schemas for all known methods
        let mut method_schemas = HashMap::new();
        
        for method_info in self.registry.get_all_methods() {
            if let Ok(schema) = reflector.generate_schema(&method_info.input_type) {
                method_schemas.insert(method_info.name.clone(), schema);
            }
        }

        self.reflector = Some(reflector);
        self.method_schemas = method_schemas;
        Ok(self)
    }

    /// Get enhanced tool metadata with runtime schema
    pub fn get_enhanced_tool(&self, method_name: &str) -> McpResult<EnhancedToolMetadata> {
        let method_info = self.registry.get_method_info(method_name)
            .ok_or_else(|| McpError::tool_not_found(method_name))?;

        let schema = self.method_schemas.get(method_name);
        
        Ok(EnhancedToolMetadata {
            basic_info: method_info.clone(),
            runtime_schema: schema.cloned(),
            validation_rules: self.extract_validation_rules(method_name),
            parameter_examples: self.generate_parameter_examples(method_name),
        })
    }

    /// Generate comprehensive tool documentation
    pub fn generate_tool_documentation(&self, method_name: &str) -> McpResult<ToolDocumentation> {
        let enhanced_tool = self.get_enhanced_tool(method_name)?;
        
        Ok(ToolDocumentation {
            name: enhanced_tool.basic_info.name.clone(),
            description: enhanced_tool.basic_info.description.clone(),
            category: enhanced_tool.basic_info.category.clone(),
            risk_level: self.assess_risk_level(&enhanced_tool.basic_info),
            parameters: self.document_parameters(&enhanced_tool)?,
            examples: enhanced_tool.parameter_examples.clone(),
            validation_schema: enhanced_tool.runtime_schema.clone(),
        })
    }

    /// Validate parameters against runtime schema
    pub fn validate_parameters(&self, method_name: &str, params: &Value) -> McpResult<()> {
        if let Some(schema) = self.method_schemas.get(method_name) {
            // Here you would integrate with a JSON schema validator
            // For example, using the `jsonschema` crate:
            // 
            // let validator = JSONSchema::compile(schema)?;
            // if let Err(errors) = validator.validate(params) {
            //     return Err(McpError::invalid_request(format!("Validation failed: {:?}", errors)));
            // }
            
            // For now, just basic validation
            self.basic_parameter_validation(params)
        } else {
            // Fallback to basic validation
            self.basic_parameter_validation(params)
        }
    }

    /// Get all available tools with enhanced metadata
    pub fn list_enhanced_tools(&self) -> Vec<EnhancedToolMetadata> {
        self.registry
            .get_all_methods()
            .into_iter()
            .filter_map(|method| self.get_enhanced_tool(&method.name).ok())
            .collect()
    }

    /// Generate OpenAPI-style documentation
    pub fn generate_openapi_spec(&self) -> McpResult<OpenApiSpec> {
        let mut spec = OpenApiSpec {
            openapi: "3.0.0".to_string(),
            info: OpenApiInfo {
                title: "Tari MCP gRPC Services".to_string(),
                version: "1.0.0".to_string(),
                description: "Auto-generated API documentation for Tari MCP services".to_string(),
            },
            paths: HashMap::new(),
            components: OpenApiComponents {
                schemas: HashMap::new(),
            },
        };

        // Generate schemas for all methods
        for method_info in self.registry.get_all_methods() {
            if let Some(schema) = self.method_schemas.get(&method_info.name) {
                // Convert JSON schema to OpenAPI schema format
                spec.components.schemas.insert(
                    format!("{}Request", method_info.name),
                    schema.clone()
                );

                // Add path entry
                spec.paths.insert(
                    format!("/{}", method_info.name),
                    OpenApiPath {
                        post: Some(OpenApiOperation {
                            summary: method_info.description.clone(),
                            request_body: Some(OpenApiRequestBody {
                                required: true,
                                content: {
                                    let mut content = HashMap::new();
                                    content.insert(
                                        "application/json".to_string(),
                                        OpenApiMediaType {
                                            schema: Some(format!("#/components/schemas/{}Request", method_info.name)),
                                        }
                                    );
                                    content
                                },
                            }),
                            responses: HashMap::new(),
                        }),
                    }
                );
            }
        }

        Ok(spec)
    }

    /// Extract validation rules from schema
    fn extract_validation_rules(&self, method_name: &str) -> Vec<ValidationRule> {
        // Extract validation rules from the runtime schema
        // This would analyze the schema and extract constraints
        vec![] // Placeholder
    }

    /// Generate parameter examples from schema
    fn generate_parameter_examples(&self, method_name: &str) -> Vec<ParameterExample> {
        // Generate realistic examples based on the schema
        // This would analyze field types and generate appropriate test data
        vec![] // Placeholder
    }

    /// Assess risk level based on method characteristics
    fn assess_risk_level(&self, method_info: &GrpcMethodInfo) -> ToolRiskLevel {
        // Analyze the method to determine risk level
        match method_info.category {
            ToolCategory::Control => ToolRiskLevel::High,
            ToolCategory::Transaction => ToolRiskLevel::Medium,
            ToolCategory::Mining => ToolRiskLevel::Medium,
            _ => ToolRiskLevel::Low,
        }
    }

    /// Document parameters with enhanced information
    fn document_parameters(&self, enhanced_tool: &EnhancedToolMetadata) -> McpResult<Vec<ParameterDocumentation>> {
        // Extract parameter documentation from runtime schema
        // This would walk the schema and generate comprehensive parameter docs
        Ok(vec![]) // Placeholder
    }

    /// Basic parameter validation fallback
    fn basic_parameter_validation(&self, params: &Value) -> McpResult<()> {
        // Basic validation: ensure it's an object
        if !params.is_object() {
            return Err(McpError::invalid_request("Parameters must be an object"));
        }
        Ok(())
    }
}

/// Enhanced tool metadata with runtime reflection
#[derive(Debug, Clone)]
pub struct EnhancedToolMetadata {
    pub basic_info: GrpcMethodInfo,
    pub runtime_schema: Option<Schema>,
    pub validation_rules: Vec<ValidationRule>,
    pub parameter_examples: Vec<ParameterExample>,
}

/// Comprehensive tool documentation
#[derive(Debug, Clone)]
pub struct ToolDocumentation {
    pub name: String,
    pub description: String,
    pub category: ToolCategory,
    pub risk_level: ToolRiskLevel,
    pub parameters: Vec<ParameterDocumentation>,
    pub examples: Vec<ParameterExample>,
    pub validation_schema: Option<Schema>,
}

/// Parameter documentation with type information
#[derive(Debug, Clone)]
pub struct ParameterDocumentation {
    pub name: String,
    pub type_info: String,
    pub description: String,
    pub required: bool,
    pub constraints: Vec<String>,
    pub example_values: Vec<Value>,
}

/// Validation rule extracted from schema
#[derive(Debug, Clone)]
pub struct ValidationRule {
    pub field_path: String,
    pub rule_type: String,
    pub constraint: Value,
    pub error_message: String,
}

/// Parameter example with context
#[derive(Debug, Clone)]
pub struct ParameterExample {
    pub name: String,
    pub description: String,
    pub value: Value,
    pub context: String,
}

/// OpenAPI specification structures
#[derive(Debug, Clone)]
pub struct OpenApiSpec {
    pub openapi: String,
    pub info: OpenApiInfo,
    pub paths: HashMap<String, OpenApiPath>,
    pub components: OpenApiComponents,
}

#[derive(Debug, Clone)]
pub struct OpenApiInfo {
    pub title: String,
    pub version: String,
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct OpenApiPath {
    pub post: Option<OpenApiOperation>,
}

#[derive(Debug, Clone)]
pub struct OpenApiOperation {
    pub summary: String,
    pub request_body: Option<OpenApiRequestBody>,
    pub responses: HashMap<String, OpenApiResponse>,
}

#[derive(Debug, Clone)]
pub struct OpenApiRequestBody {
    pub required: bool,
    pub content: HashMap<String, OpenApiMediaType>,
}

#[derive(Debug, Clone)]
pub struct OpenApiMediaType {
    pub schema: Option<String>,
}

#[derive(Debug, Clone)]
pub struct OpenApiResponse {
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct OpenApiComponents {
    pub schemas: HashMap<String, Schema>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AutoDiscoveryConfig;

    #[test]
    fn test_reflective_discovery_creation() {
        let config = AutoDiscoveryConfig {
            enabled: true,
            health_check_interval: std::time::Duration::from_secs(30),
            method_filter: None,
            tool_overrides: HashMap::new(),
        };

        let registry = AutoDiscoveryRegistry::new(config);
        let discovery = ReflectiveAutoDiscovery::new(registry);
        
        assert!(discovery.reflector.is_none());
        assert!(discovery.method_schemas.is_empty());
    }

    #[test]
    fn test_parameter_validation_basic() {
        let config = AutoDiscoveryConfig {
            enabled: true,
            health_check_interval: std::time::Duration::from_secs(30),
            method_filter: None,
            tool_overrides: HashMap::new(),
        };

        let registry = AutoDiscoveryRegistry::new(config);
        let discovery = ReflectiveAutoDiscovery::new(registry);
        
        // Test with valid object
        let valid_params = serde_json::json!({"test": "value"});
        assert!(discovery.validate_parameters("test_method", &valid_params).is_ok());
        
        // Test with invalid non-object
        let invalid_params = serde_json::json!("not an object");
        assert!(discovery.validate_parameters("test_method", &invalid_params).is_err());
    }
}
