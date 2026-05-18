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
//! Error types for MCP operations

#![allow(clippy::indexing_slicing)]
use serde_json::{json, Value};
use thiserror::Error;

pub type McpResult<T> = Result<T, McpError>;

#[derive(Error, Debug)]
pub enum McpError {
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    #[error("Tool not found: {0}")]
    ToolNotFound(String),

    #[error("Resource not found: {0}")]
    ResourceNotFound(String),

    #[error("Prompt not found: {0}")]
    PromptNotFound(String),

    #[error("Tool execution failed: {0}")]
    ToolExecutionFailed(String),

    #[error("Resource access failed: {0}")]
    ResourceAccessFailed(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Transport error: {0}")]
    TransportError(String),

    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    #[error("Server error: {0}")]
    ServerError(String),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Other error: {0}")]
    Other(#[from] anyhow::Error),
}

impl McpError {
    pub fn permission_denied(msg: impl Into<String>) -> Self {
        Self::PermissionDenied(msg.into())
    }

    pub fn invalid_request(msg: impl Into<String>) -> Self {
        Self::InvalidRequest(msg.into())
    }

    pub fn tool_not_found(tool_name: impl Into<String>) -> Self {
        Self::ToolNotFound(tool_name.into())
    }

    pub fn resource_not_found(resource_uri: impl Into<String>) -> Self {
        Self::ResourceNotFound(resource_uri.into())
    }

    pub fn tool_execution_failed(msg: impl Into<String>) -> Self {
        Self::ToolExecutionFailed(msg.into())
    }

    pub fn config_error(msg: impl Into<String>) -> Self {
        Self::ConfigError(msg.into())
    }

    pub fn resource_access_failed(msg: impl Into<String>) -> Self {
        Self::ResourceAccessFailed(msg.into())
    }

    pub fn server_error(msg: impl Into<String>) -> Self {
        Self::ServerError(msg.into())
    }

    pub fn connection_failed(msg: impl Into<String>) -> Self {
        Self::ToolExecutionFailed(format!("Connection failed: {}", msg.into()))
    }

    pub fn service_unavailable(msg: impl Into<String>) -> Self {
        Self::ToolExecutionFailed(format!("Service unavailable: {}", msg.into()))
    }

    pub fn service_error(msg: impl Into<String>) -> Self {
        Self::ToolExecutionFailed(format!("Service error: {}", msg.into()))
    }

    pub fn is_permission_denied(&self) -> bool {
        matches!(self, Self::PermissionDenied(_))
    }

    pub fn is_not_found(&self) -> bool {
        matches!(
            self,
            Self::ToolNotFound(_) | Self::ResourceNotFound(_) | Self::PromptNotFound(_)
        )
    }

    /// Get JSON-RPC 2.0 error code for this error
    pub fn error_code(&self) -> i32 {
        match self {
            Self::SerializationError(_) => -32700, // Parse error
            Self::InvalidRequest(_) => -32600,     // Invalid Request
            Self::ToolNotFound(_) | Self::ResourceNotFound(_) | Self::PromptNotFound(_) => -32601, // Method not found
            Self::PermissionDenied(_) | Self::AuthenticationFailed(_) => -32603, // Internal error (authorization)
            Self::RateLimitExceeded => -32603,     // Internal error (rate limit)
            Self::ServerError(_) | Self::TransportError(_) => -32603, // Internal error
            Self::ToolExecutionFailed(_) | Self::ResourceAccessFailed(_) => -32603, // Internal error
            Self::ConfigError(_) => -32603,        // Internal error
            Self::IoError(_) => -32603,            // Internal error
            Self::Other(_) => -32603,              // Internal error
        }
    }

    /// Convert error to JSON-RPC 2.0 error object
    pub fn to_json_rpc_error(&self, id: Option<Value>) -> Value {
        let code = self.error_code();
        let message = self.to_string();

        // Create detailed error data
        let mut data = json!({
            "error_type": self.error_type_name(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });

        // Add context-specific data
        match self {
            Self::PermissionDenied(msg) => {
                data["details"] = json!({
                    "permission_required": msg,
                    "help": "Check that you have the required permissions for this operation"
                });
            },
            Self::ToolNotFound(tool) => {
                data["details"] = json!({
                    "tool_name": tool,
                    "help": "Use 'tools/list' to see available tools"
                });
            },
            Self::ResourceNotFound(resource) => {
                data["details"] = json!({
                    "resource_uri": resource,
                    "help": "Use 'resources/list' to see available resources"
                });
            },
            Self::RateLimitExceeded => {
                data["details"] = json!({
                    "retry_after": 60,
                    "help": "Wait before making more requests"
                });
            },
            Self::ToolExecutionFailed(msg) => {
                data["details"] = json!({
                    "execution_error": msg,
                    "help": "Check tool parameters and try again"
                });
            },
            _ => {},
        }

        json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": code,
                "message": message,
                "data": data
            }
        })
    }

    /// Get a human-readable error type name
    pub fn error_type_name(&self) -> &'static str {
        match self {
            Self::PermissionDenied(_) => "permission_denied",
            Self::InvalidRequest(_) => "invalid_request",
            Self::ToolNotFound(_) => "tool_not_found",
            Self::ResourceNotFound(_) => "resource_not_found",
            Self::PromptNotFound(_) => "prompt_not_found",
            Self::ToolExecutionFailed(_) => "tool_execution_failed",
            Self::ResourceAccessFailed(_) => "resource_access_failed",
            Self::ConfigError(_) => "config_error",
            Self::TransportError(_) => "transport_error",
            Self::AuthenticationFailed(_) => "authentication_failed",
            Self::RateLimitExceeded => "rate_limit_exceeded",
            Self::ServerError(_) => "server_error",
            Self::SerializationError(_) => "serialization_error",
            Self::IoError(_) => "io_error",
            Self::Other(_) => "other_error",
        }
    }
}
