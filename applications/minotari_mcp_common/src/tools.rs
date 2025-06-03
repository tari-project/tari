//! MCP tool definitions and registry

use crate::error::{McpError, McpResult};
use crate::security::PermissionLevel;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// MCP tool trait that all tools must implement
#[async_trait]
pub trait McpTool: Send + Sync {
    /// Get the tool name
    fn name(&self) -> &str;
    
    /// Get the tool description
    fn description(&self) -> &str;
    
    /// Get the permission level required to execute this tool
    fn permission_level(&self) -> PermissionLevel;
    
    /// Get the input schema for this tool
    fn input_schema(&self) -> Value;
    
    /// Execute the tool with the given parameters
    async fn execute(&self, params: Value) -> McpResult<Value>;
    
    /// Validate tool parameters before execution
    fn validate_params(&self, _params: &Value) -> McpResult<()> {
        // Default implementation - can be overridden
        Ok(())
    }
}

/// Tool information for MCP protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

/// Registry for managing MCP tools
#[derive(Default)]
pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn McpTool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a new tool
    pub fn register(&mut self, tool: Box<dyn McpTool>) {
        let name = tool.name().to_string();
        self.tools.insert(name, tool);
    }

    /// Get a tool by name
    pub fn get(&self, name: &str) -> Option<&dyn McpTool> {
        self.tools.get(name).map(|t| t.as_ref())
    }

    /// List all available tools
    pub fn list_tools(&self) -> Vec<ToolInfo> {
        self.tools
            .values()
            .map(|tool| ToolInfo {
                name: tool.name().to_string(),
                description: tool.description().to_string(),
                input_schema: tool.input_schema(),
            })
            .collect()
    }

    /// Execute a tool by name
    pub async fn execute_tool(&self, name: &str, params: Value) -> McpResult<Value> {
        let tool = self.get(name)
            .ok_or_else(|| McpError::tool_not_found(name))?;

        // Validate parameters
        tool.validate_params(&params)
            .map_err(|e| McpError::invalid_request(format!("Parameter validation failed: {}", e)))?;

        // Execute the tool
        tool.execute(params).await
            .map_err(|e| McpError::tool_execution_failed(format!("Tool '{}' execution failed: {}", name, e)))
    }

    /// Get the permission level required for a tool
    pub fn get_permission_level(&self, name: &str) -> McpResult<PermissionLevel> {
        let tool = self.get(name)
            .ok_or_else(|| McpError::tool_not_found(name))?;
        Ok(tool.permission_level())
    }
}

/// Macro to create JSON schema for tool input parameters
#[macro_export]
macro_rules! json_schema {
    ($($key:literal => $value:expr),* $(,)?) => {
        serde_json::json!({
            "type": "object",
            "properties": {
                $($key: $value),*
            }
        })
    };
}

/// Helper function to validate required string parameter
pub fn get_required_string_param(params: &Value, key: &str) -> McpResult<String> {
    params
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| McpError::invalid_request(format!("Missing required string parameter: {}", key)))
}

/// Helper function to validate optional string parameter
pub fn get_optional_string_param(params: &Value, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Helper function to validate required number parameter
pub fn get_required_number_param(params: &Value, key: &str) -> McpResult<f64> {
    params
        .get(key)
        .and_then(|v| v.as_f64())
        .ok_or_else(|| McpError::invalid_request(format!("Missing required number parameter: {}", key)))
}

/// Helper function to validate required boolean parameter
pub fn get_required_bool_param(params: &Value, key: &str) -> McpResult<bool> {
    params
        .get(key)
        .and_then(|v| v.as_bool())
        .ok_or_else(|| McpError::invalid_request(format!("Missing required boolean parameter: {}", key)))
}

/// Helper function to validate required u64 parameter
pub fn get_required_u64_param(params: &Value, key: &str) -> McpResult<u64> {
    params
        .get(key)
        .and_then(|v| v.as_u64())
        .ok_or_else(|| McpError::invalid_request(format!("Missing required u64 parameter: {}", key)))
}
