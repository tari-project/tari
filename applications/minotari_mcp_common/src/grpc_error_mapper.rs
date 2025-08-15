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
//! gRPC Error Mapping and Enhanced Error Handling
//!
//! This module provides comprehensive error mapping from gRPC status codes to
//! meaningful MCP error responses with detailed context and recommendations.

use std::collections::HashMap;

use serde_json::{json, Value};
use tonic::{Code, Status};

use crate::McpError;

/// Enhanced error information with context and recommendations
#[derive(Debug, Clone)]
pub struct ErrorContext {
    /// Original error message
    pub message: String,
    /// Error category for classification
    pub category: ErrorCategory,
    /// Severity level
    pub severity: ErrorSeverity,
    /// User-friendly explanation
    pub user_message: String,
    /// Recommended actions
    pub recommendations: Vec<String>,
    /// Related documentation links
    pub documentation: Vec<String>,
    /// Error code for programmatic handling
    pub error_code: String,
    /// Additional context data
    pub context: HashMap<String, Value>,
}

/// Error categories for classification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCategory {
    Authentication,
    Authorization,
    Configuration,
    Network,
    Validation,
    Resource,
    State,
    Timeout,
    Internal,
    External,
    HealthCheck,
    CircuitBreaker,
    ParameterConversion,
    ResponseConversion,
}

impl ErrorCategory {
    pub fn description(&self) -> &'static str {
        match self {
            Self::Authentication => "Authentication or credential errors",
            Self::Authorization => "Permission or access control errors",
            Self::Configuration => "Configuration or setup errors",
            Self::Network => "Network connectivity or communication errors",
            Self::Validation => "Input validation or parameter errors",
            Self::Resource => "Resource availability or limit errors",
            Self::State => "System or service state errors",
            Self::Timeout => "Operation timeout errors",
            Self::Internal => "Internal service errors",
            Self::External => "External dependency errors",
            Self::HealthCheck => "Health check failure",
            Self::CircuitBreaker => "Circuit breaker protection",
            Self::ParameterConversion => "Parameter conversion error",
            Self::ResponseConversion => "Response conversion error",
        }
    }
}

/// Error severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorSeverity {
    Low,
    Medium,
    High,
    Critical,
}

impl ErrorSeverity {
    pub fn description(&self) -> &'static str {
        match self {
            Self::Low => "Minor issue that may not affect functionality",
            Self::Medium => "Moderate issue that may impact some operations",
            Self::High => "Significant issue that affects core functionality",
            Self::Critical => "Critical issue that prevents operation",
        }
    }

    pub fn color_code(&self) -> &'static str {
        match self {
            Self::Low => "#17a2b8",      // Blue
            Self::Medium => "#ffc107",   // Yellow
            Self::High => "#fd7e14",     // Orange
            Self::Critical => "#dc3545", // Red
        }
    }
}

/// gRPC error mapper
pub struct GrpcErrorMapper {
    /// Custom error mappings
    custom_mappings: HashMap<String, ErrorContext>,
}

impl GrpcErrorMapper {
    /// Create a new error mapper
    pub fn new() -> Self {
        Self {
            custom_mappings: HashMap::new(),
        }
    }

    /// Add custom error mapping
    pub fn add_mapping(&mut self, grpc_message: &str, context: ErrorContext) {
        self.custom_mappings.insert(grpc_message.to_string(), context);
    }

    /// Map gRPC status to enhanced error context
    #[allow(clippy::too_many_lines)]
    pub fn map_status(&self, status: &Status) -> ErrorContext {
        // Check for custom mappings first
        if let Some(context) = self.custom_mappings.get(status.message()) {
            return context.clone();
        }

        // Map based on gRPC status code
        match status.code() {
            Code::Ok => ErrorContext {
                message: "Operation completed successfully".to_string(),
                category: ErrorCategory::Internal,
                severity: ErrorSeverity::Low,
                user_message: "The operation completed successfully".to_string(),
                recommendations: vec![],
                documentation: vec![],
                error_code: "SUCCESS".to_string(),
                context: HashMap::new(),
            },

            Code::Cancelled => ErrorContext {
                message: "Operation was cancelled".to_string(),
                category: ErrorCategory::State,
                severity: ErrorSeverity::Medium,
                user_message: "The operation was cancelled before completion".to_string(),
                recommendations: vec![
                    "Check if the cancellation was intentional".to_string(),
                    "Retry the operation if needed".to_string(),
                ],
                documentation: vec![],
                error_code: "OPERATION_CANCELLED".to_string(),
                context: self.extract_context(status),
            },

            Code::Unknown => ErrorContext {
                message: format!("Unknown error: {}", status.message()),
                category: ErrorCategory::Internal,
                severity: ErrorSeverity::High,
                user_message: "An unexpected error occurred".to_string(),
                recommendations: vec![
                    "Check the service logs for more details".to_string(),
                    "Verify that all services are running correctly".to_string(),
                    "Try the operation again after a brief delay".to_string(),
                ],
                documentation: vec!["https://docs.tari.com/troubleshooting".to_string()],
                error_code: "UNKNOWN_ERROR".to_string(),
                context: self.extract_context(status),
            },

            Code::InvalidArgument => ErrorContext {
                message: format!("Invalid argument: {}", status.message()),
                category: ErrorCategory::Validation,
                severity: ErrorSeverity::Medium,
                user_message: "The provided parameters are invalid".to_string(),
                recommendations: vec![
                    "Check parameter types and formats".to_string(),
                    "Verify required parameters are provided".to_string(),
                    "Review parameter constraints and limits".to_string(),
                ],
                documentation: vec!["https://docs.tari.com/api-reference".to_string()],
                error_code: "INVALID_PARAMETERS".to_string(),
                context: self.extract_context(status),
            },

            Code::DeadlineExceeded => ErrorContext {
                message: "Operation timed out".to_string(),
                category: ErrorCategory::Timeout,
                severity: ErrorSeverity::High,
                user_message: "The operation took too long to complete".to_string(),
                recommendations: vec![
                    "Check network connectivity".to_string(),
                    "Verify that the service is responding".to_string(),
                    "Try again with a longer timeout".to_string(),
                    "Consider breaking large operations into smaller parts".to_string(),
                ],
                documentation: vec!["https://docs.tari.com/performance".to_string()],
                error_code: "TIMEOUT".to_string(),
                context: self.extract_context(status),
            },

            Code::NotFound => ErrorContext {
                message: format!("Resource not found: {}", status.message()),
                category: ErrorCategory::Resource,
                severity: ErrorSeverity::Medium,
                user_message: "The requested resource could not be found".to_string(),
                recommendations: vec![
                    "Verify the resource identifier is correct".to_string(),
                    "Check if the resource exists or has been deleted".to_string(),
                    "Ensure you have access to the resource".to_string(),
                ],
                documentation: vec!["https://docs.tari.com/api-reference".to_string()],
                error_code: "RESOURCE_NOT_FOUND".to_string(),
                context: self.extract_context(status),
            },

            Code::AlreadyExists => ErrorContext {
                message: "Resource already exists".to_string(),
                category: ErrorCategory::State,
                severity: ErrorSeverity::Medium,
                user_message: "The resource you're trying to create already exists".to_string(),
                recommendations: vec![
                    "Use a different identifier or name".to_string(),
                    "Check if you want to update the existing resource instead".to_string(),
                    "Consider using an upsert operation if available".to_string(),
                ],
                documentation: vec![],
                error_code: "RESOURCE_EXISTS".to_string(),
                context: self.extract_context(status),
            },

            Code::PermissionDenied => ErrorContext {
                message: "Permission denied".to_string(),
                category: ErrorCategory::Authorization,
                severity: ErrorSeverity::High,
                user_message: "You don't have permission to perform this operation".to_string(),
                recommendations: vec![
                    "Check your authentication credentials".to_string(),
                    "Verify you have the required permissions".to_string(),
                    "Contact an administrator if you believe you should have access".to_string(),
                    "For control operations, ensure mcp_control_enabled is true".to_string(),
                ],
                documentation: vec!["https://docs.tari.com/security".to_string()],
                error_code: "PERMISSION_DENIED".to_string(),
                context: self.extract_context(status),
            },

            Code::ResourceExhausted => ErrorContext {
                message: "Resource exhausted".to_string(),
                category: ErrorCategory::Resource,
                severity: ErrorSeverity::High,
                user_message: "System resources are exhausted or rate limits exceeded".to_string(),
                recommendations: vec![
                    "Wait before retrying the operation".to_string(),
                    "Reduce the frequency of requests".to_string(),
                    "Check if you're within rate limits".to_string(),
                    "Consider using pagination for large requests".to_string(),
                ],
                documentation: vec!["https://docs.tari.com/rate-limits".to_string()],
                error_code: "RESOURCE_EXHAUSTED".to_string(),
                context: self.extract_context(status),
            },

            Code::FailedPrecondition => ErrorContext {
                message: format!("Failed precondition: {}", status.message()),
                category: ErrorCategory::State,
                severity: ErrorSeverity::High,
                user_message: "The system is not in the correct state for this operation".to_string(),
                recommendations: vec![
                    "Check the current system state".to_string(),
                    "Verify prerequisites are met".to_string(),
                    "Ensure proper operation sequence".to_string(),
                    "Wait for ongoing operations to complete".to_string(),
                ],
                documentation: vec!["https://docs.tari.com/state-management".to_string()],
                error_code: "PRECONDITION_FAILED".to_string(),
                context: self.extract_context(status),
            },

            Code::Aborted => ErrorContext {
                message: "Operation was aborted".to_string(),
                category: ErrorCategory::State,
                severity: ErrorSeverity::High,
                user_message: "The operation was aborted due to a conflict or system state".to_string(),
                recommendations: vec![
                    "Check for conflicting operations".to_string(),
                    "Verify system state and retry".to_string(),
                    "Ensure exclusive access if required".to_string(),
                ],
                documentation: vec![],
                error_code: "OPERATION_ABORTED".to_string(),
                context: self.extract_context(status),
            },

            Code::OutOfRange => ErrorContext {
                message: format!("Value out of range: {}", status.message()),
                category: ErrorCategory::Validation,
                severity: ErrorSeverity::Medium,
                user_message: "One or more values are outside the valid range".to_string(),
                recommendations: vec![
                    "Check parameter ranges and limits".to_string(),
                    "Verify numeric values are within bounds".to_string(),
                    "Review the API documentation for valid ranges".to_string(),
                ],
                documentation: vec!["https://docs.tari.com/api-reference".to_string()],
                error_code: "VALUE_OUT_OF_RANGE".to_string(),
                context: self.extract_context(status),
            },

            Code::Unimplemented => ErrorContext {
                message: "Operation not implemented".to_string(),
                category: ErrorCategory::Internal,
                severity: ErrorSeverity::Medium,
                user_message: "This operation is not yet implemented".to_string(),
                recommendations: vec![
                    "Check if there's an alternative method".to_string(),
                    "Verify you're using the correct API version".to_string(),
                    "Check the documentation for supported operations".to_string(),
                ],
                documentation: vec!["https://docs.tari.com/api-reference".to_string()],
                error_code: "NOT_IMPLEMENTED".to_string(),
                context: self.extract_context(status),
            },

            Code::Internal => ErrorContext {
                message: "Internal server error".to_string(),
                category: ErrorCategory::Internal,
                severity: ErrorSeverity::Critical,
                user_message: "An internal server error occurred".to_string(),
                recommendations: vec![
                    "Try the operation again after a brief delay".to_string(),
                    "Check service status and logs".to_string(),
                    "Contact support if the error persists".to_string(),
                    "Verify all services are running correctly".to_string(),
                ],
                documentation: vec!["https://docs.tari.com/troubleshooting".to_string()],
                error_code: "INTERNAL_ERROR".to_string(),
                context: self.extract_context(status),
            },

            Code::Unavailable => ErrorContext {
                message: "Service unavailable".to_string(),
                category: ErrorCategory::Network,
                severity: ErrorSeverity::Critical,
                user_message: "The service is currently unavailable".to_string(),
                recommendations: vec![
                    "Check network connectivity".to_string(),
                    "Verify the service is running".to_string(),
                    "Try again after a brief delay".to_string(),
                    "Check service status page".to_string(),
                ],
                documentation: vec!["https://docs.tari.com/troubleshooting".to_string()],
                error_code: "SERVICE_UNAVAILABLE".to_string(),
                context: self.extract_context(status),
            },

            Code::DataLoss => ErrorContext {
                message: "Data loss detected".to_string(),
                category: ErrorCategory::Internal,
                severity: ErrorSeverity::Critical,
                user_message: "Data corruption or loss has been detected".to_string(),
                recommendations: vec![
                    "Stop using the service immediately".to_string(),
                    "Contact support urgently".to_string(),
                    "Check data backups".to_string(),
                    "Do not attempt recovery without expert assistance".to_string(),
                ],
                documentation: vec!["https://docs.tari.com/data-recovery".to_string()],
                error_code: "DATA_LOSS".to_string(),
                context: self.extract_context(status),
            },

            Code::Unauthenticated => ErrorContext {
                message: "Authentication required".to_string(),
                category: ErrorCategory::Authentication,
                severity: ErrorSeverity::High,
                user_message: "You need to authenticate to access this resource".to_string(),
                recommendations: vec![
                    "Provide valid authentication credentials".to_string(),
                    "Check if your session has expired".to_string(),
                    "Verify the authentication method is correct".to_string(),
                ],
                documentation: vec!["https://docs.tari.com/authentication".to_string()],
                error_code: "AUTHENTICATION_REQUIRED".to_string(),
                context: self.extract_context(status),
            },
        }
    }

    /// Convert gRPC status to MCP error with enhanced context
    pub fn to_mcp_error(&self, status: &Status, operation: &str) -> McpError {
        let context = self.map_status(status);

        McpError::tool_execution_failed(format!(
            "{} failed: {} ({})",
            operation, context.user_message, context.error_code
        ))
    }

    /// Create a detailed error response for MCP
    pub fn create_error_response(&self, status: &Status, operation: &str) -> Value {
        let context = self.map_status(status);

        json!({
            "error": {
                "code": context.error_code,
                "message": context.user_message,
                "category": format!("{:?}", context.category),
                "severity": format!("{:?}", context.severity),
                "operation": operation,
                "details": {
                    "grpc_code": format!("{:?}", status.code()),
                    "grpc_message": status.message(),
                    "category_description": context.category.description(),
                    "severity_description": context.severity.description(),
                }
            },
            "recommendations": context.recommendations,
            "documentation": context.documentation,
            "context": context.context,
            "support": {
                "message": "If this error persists, please contact support with the error code and operation details",
                "error_id": self.generate_error_id(),
                "timestamp": std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            }
        })
    }

    /// Extract additional context from gRPC status
    fn extract_context(&self, status: &Status) -> HashMap<String, Value> {
        let mut context = HashMap::new();

        context.insert("grpc_code".to_string(), json!(format!("{:?}", status.code())));
        context.insert("grpc_message".to_string(), json!(status.message()));

        // Extract metadata if available
        if !status.metadata().is_empty() {
            context.insert("has_metadata".to_string(), json!(true));
        }

        context
    }

    /// Generate a unique error ID for tracking
    fn generate_error_id(&self) -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        format!("err_{timestamp}")
    }
}

impl Default for GrpcErrorMapper {
    fn default() -> Self {
        Self::new()
    }
}

/// Utility functions for common error scenarios
pub mod error_utils {
    use super::*;

    /// Create error context for wallet-specific errors
    pub fn wallet_error_context(message: &str, is_balance_issue: bool) -> ErrorContext {
        let (recommendations, severity) = if is_balance_issue {
            (
                vec![
                    "Check wallet balance".to_string(),
                    "Ensure sufficient funds for transaction and fees".to_string(),
                    "Wait for pending transactions to confirm".to_string(),
                ],
                ErrorSeverity::Medium,
            )
        } else {
            (
                vec![
                    "Verify wallet is synchronized".to_string(),
                    "Check wallet connection status".to_string(),
                    "Restart wallet if necessary".to_string(),
                ],
                ErrorSeverity::High,
            )
        };

        ErrorContext {
            message: message.to_string(),
            category: ErrorCategory::State,
            severity,
            user_message: "Wallet operation failed".to_string(),
            recommendations,
            documentation: vec!["https://docs.tari.com/wallet-troubleshooting".to_string()],
            error_code: "WALLET_ERROR".to_string(),
            context: HashMap::new(),
        }
    }

    /// Create error context for node-specific errors
    pub fn node_error_context(message: &str, is_sync_issue: bool) -> ErrorContext {
        let (recommendations, severity) = if is_sync_issue {
            (
                vec![
                    "Wait for node to complete synchronization".to_string(),
                    "Check network connectivity".to_string(),
                    "Verify peers are connected".to_string(),
                ],
                ErrorSeverity::Medium,
            )
        } else {
            (
                vec![
                    "Check node status and logs".to_string(),
                    "Verify node configuration".to_string(),
                    "Restart node if necessary".to_string(),
                ],
                ErrorSeverity::High,
            )
        };

        ErrorContext {
            message: message.to_string(),
            category: ErrorCategory::State,
            severity,
            user_message: "Node operation failed".to_string(),
            recommendations,
            documentation: vec!["https://docs.tari.com/node-troubleshooting".to_string()],
            error_code: "NODE_ERROR".to_string(),
            context: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)]
    use super::*;

    #[test]
    fn test_error_mapping() {
        let mapper = GrpcErrorMapper::new();
        let status = Status::new(Code::NotFound, "Transaction not found");

        let context = mapper.map_status(&status);
        assert_eq!(context.category, ErrorCategory::Resource);
        assert_eq!(context.error_code, "RESOURCE_NOT_FOUND");
        assert!(!context.recommendations.is_empty());
    }

    #[test]
    fn test_mcp_error_conversion() {
        let mapper = GrpcErrorMapper::new();
        let status = Status::new(Code::InvalidArgument, "Invalid parameter");

        let mcp_error = mapper.to_mcp_error(&status, "get_balance");
        // The error should contain the operation name and error code
        let error_msg = format!("{mcp_error:?}");
        assert!(error_msg.contains("get_balance"));
        assert!(error_msg.contains("INVALID_PARAMETERS"));
    }

    #[test]
    fn test_error_response_generation() {
        let mapper = GrpcErrorMapper::new();
        let status = Status::new(Code::PermissionDenied, "Access denied");

        let response = mapper.create_error_response(&status, "transfer");
        assert!(response["error"]["code"].as_str().unwrap() == "PERMISSION_DENIED");
        assert!(!response["recommendations"].as_array().unwrap().is_empty());
        assert!(response["support"]["error_id"].is_string());
    }
}
