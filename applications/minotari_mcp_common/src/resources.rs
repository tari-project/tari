//! MCP resource definitions and registry

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{McpError, McpResult};

/// MCP resource trait that all resources must implement
#[async_trait]
pub trait McpResource: Send + Sync {
    /// Get the resource URI
    fn uri(&self) -> &str;

    /// Get the resource name
    fn name(&self) -> &str;

    /// Get the resource description
    fn description(&self) -> &str;

    /// Get the MIME type of the resource content
    fn mime_type(&self) -> &str;

    /// Read the resource content
    async fn read(&self) -> McpResult<Value>;

    /// Check if the resource supports templating (URI parameters)
    fn supports_templating(&self) -> bool {
        false
    }

    /// Resolve templated URI with parameters
    fn resolve_template(&self, _params: &HashMap<String, String>) -> McpResult<String> {
        if self.supports_templating() {
            Err(McpError::invalid_request("Template resolution not implemented"))
        } else {
            Ok(self.uri().to_string())
        }
    }
}

/// Resource information for MCP protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceInfo {
    pub uri: String,
    pub name: String,
    pub description: String,
    pub mime_type: String,
}

/// Registry for managing MCP resources
#[derive(Default)]
pub struct ResourceRegistry {
    resources: HashMap<String, Box<dyn McpResource>>,
    /// URI patterns for templated resources (e.g., "block/{height}")
    patterns: HashMap<String, String>, // pattern -> resource_key
}

impl ResourceRegistry {
    pub fn new() -> Self {
        Self {
            resources: HashMap::new(),
            patterns: HashMap::new(),
        }
    }

    /// Register a new resource
    pub fn register(&mut self, resource: Box<dyn McpResource>) {
        let uri = resource.uri().to_string();

        // Check if this is a templated resource
        if resource.supports_templating() {
            // Extract pattern from URI (e.g., "block/{height}" -> "block/*")
            let pattern = self.extract_pattern(&uri);
            self.patterns.insert(pattern, uri.clone());
        }

        self.resources.insert(uri, resource);
    }

    /// Get a resource by exact URI
    pub fn get(&self, uri: &str) -> Option<&dyn McpResource> {
        self.resources.get(uri).map(|r| r.as_ref())
    }

    /// Get a resource by URI, supporting templated URIs
    pub fn get_by_uri(&self, uri: &str) -> McpResult<&dyn McpResource> {
        // First try exact match
        if let Some(resource) = self.get(uri) {
            return Ok(resource);
        }

        // Try pattern matching for templated resources
        for (pattern, resource_uri) in &self.patterns {
            if self.matches_pattern(pattern, uri) {
                if let Some(resource) = self.get(resource_uri) {
                    return Ok(resource);
                }
            }
        }

        Err(McpError::resource_not_found(uri))
    }

    /// List all available resources
    pub fn list_resources(&self) -> Vec<ResourceInfo> {
        self.resources
            .values()
            .map(|resource| ResourceInfo {
                uri: resource.uri().to_string(),
                name: resource.name().to_string(),
                description: resource.description().to_string(),
                mime_type: resource.mime_type().to_string(),
            })
            .collect()
    }

    /// Read a resource by URI
    pub async fn read_resource(&self, uri: &str) -> McpResult<Value> {
        let resource = self.get_by_uri(uri)?;

        // If this is a templated resource, resolve the template
        if resource.supports_templating() {
            let params = self.extract_parameters(resource.uri(), uri)?;
            let _resolved_uri = resource.resolve_template(&params)?;
            // The resolved URI might be used for additional processing
        }

        resource
            .read()
            .await
            .map_err(|e| McpError::ResourceAccessFailed(format!("Failed to read resource '{}': {}", uri, e)))
    }

    /// Extract pattern from templated URI
    fn extract_pattern(&self, uri: &str) -> String {
        // Convert "block/{height}" to "block/*"
        // This is a simple implementation - could be made more sophisticated
        uri.replace('{', "*").replace('}', "")
    }

    /// Check if a URI matches a pattern
    fn matches_pattern(&self, pattern: &str, uri: &str) -> bool {
        // Simple glob-style matching
        // "block/*" matches "block/123" but not "block/123/tx"
        let pattern_parts: Vec<&str> = pattern.split('/').collect();
        let uri_parts: Vec<&str> = uri.split('/').collect();

        if pattern_parts.len() != uri_parts.len() {
            return false;
        }

        for (pattern_part, uri_part) in pattern_parts.iter().zip(uri_parts.iter()) {
            if *pattern_part != "*" && *pattern_part != *uri_part {
                return false;
            }
        }

        true
    }

    /// Extract parameters from templated URI
    fn extract_parameters(&self, template: &str, uri: &str) -> McpResult<HashMap<String, String>> {
        let mut params = HashMap::new();

        let template_parts: Vec<&str> = template.split('/').collect();
        let uri_parts: Vec<&str> = uri.split('/').collect();

        if template_parts.len() != uri_parts.len() {
            return Err(McpError::invalid_request("URI does not match template"));
        }

        for (template_part, uri_part) in template_parts.iter().zip(uri_parts.iter()) {
            if template_part.starts_with('{') && template_part.ends_with('}') {
                // Extract parameter name from {param_name}
                let param_name = &template_part[1..template_part.len() - 1];
                params.insert(param_name.to_string(), uri_part.to_string());
            } else if *template_part != *uri_part {
                return Err(McpError::invalid_request("URI does not match template"));
            } else {
                // Template part matches URI part exactly - continue processing
            }
        }

        Ok(params)
    }
}

/// Helper macro to create a simple static resource
#[macro_export]
macro_rules! static_resource {
    ($uri:expr, $name:expr, $description:expr, $content:expr) => {{
        use async_trait::async_trait;
        use $crate::resources::McpResource;

        struct StaticResource {
            uri: String,
            name: String,
            description: String,
            content: serde_json::Value,
        }

        #[async_trait]
        impl McpResource for StaticResource {
            fn uri(&self) -> &str {
                &self.uri
            }

            fn name(&self) -> &str {
                &self.name
            }

            fn description(&self) -> &str {
                &self.description
            }

            fn mime_type(&self) -> &str {
                "application/json"
            }

            async fn read(&self) -> McpResult<serde_json::Value> {
                Ok(self.content.clone())
            }
        }

        Box::new(StaticResource {
            uri: $uri.to_string(),
            name: $name.to_string(),
            description: $description.to_string(),
            content: $content,
        })
    }};
}
