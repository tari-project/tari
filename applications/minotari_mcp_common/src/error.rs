//! Error types for MCP operations

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

    pub fn is_permission_denied(&self) -> bool {
        matches!(self, Self::PermissionDenied(_))
    }

    pub fn is_not_found(&self) -> bool {
        matches!(
            self,
            Self::ToolNotFound(_) | Self::ResourceNotFound(_) | Self::PromptNotFound(_)
        )
    }
}
