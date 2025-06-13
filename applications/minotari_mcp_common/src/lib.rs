//! Common MCP (Model Context Protocol) infrastructure for Tari applications
//!
//! This crate provides shared utilities, traits, and types for implementing
//! MCP servers in the Tari ecosystem, following security-first principles
//! with local-only binding and explicit control operation permissions.

pub mod config;
pub mod error;
pub mod server;
pub mod tools;
pub mod resources;
pub mod prompts;
pub mod security;
pub mod transport;
pub mod stdio_transport;
pub mod process_manager;
pub mod input_sanitizer;
pub mod health_monitor;
pub mod executable_finder;
pub mod process_launcher;
pub mod cli_integration;
pub mod startup_diagnostics;
pub mod grpc_discovery;
pub mod schema_generator;
pub mod tool_metadata;
pub mod grpc_error_mapper;
pub mod auto_registry;

pub use error::{McpError, McpResult};
pub use server::{McpServer, McpServerBuilder};
pub use tools::{McpTool, ToolRegistry, get_required_string_param, get_optional_string_param, get_required_number_param, get_required_bool_param, get_required_u64_param};
pub use resources::{McpResource, ResourceRegistry};
pub use prompts::{McpPrompt, PromptRegistry, MessageRole, PromptMessage, PromptContent, text_message, resource_message};
pub use security::{SecurityContext, PermissionLevel};
pub use config::McpConfig;
pub use stdio_transport::StdioTransport;
pub use process_manager::{ProcessSupervisor, ProcessType, ProcessStatus, ProcessUtils};
pub use input_sanitizer::{InputSanitizer, ValidationPatterns, sanitize_tool_input};
pub use health_monitor::{HealthMonitor, HealthStatus, HealthCheckResult, ServiceHealthMonitors};
pub use executable_finder::{ExecutableFinder, TariExecutables};
pub use process_launcher::{ProcessLauncher, LaunchConfig, LaunchConfigBuilder, LaunchResult, ProcessLaunchStatus, HealthCheckConfig, TariProcessLauncher};
pub use cli_integration::{LaunchCliConfig, CliConfigBuilder, CliConfigExtractor, NodeArgumentBuilder, WalletArgumentBuilder, CliIntegrationUtils};
pub use startup_diagnostics::{StartupDiagnostics, DiagnosticResult, DiagnosticStatus};
pub use grpc_discovery::{ServiceDiscovery, GrpcMethodInfo, GrpcMethodCategory, base_node_methods, wallet_methods};
pub use schema_generator::{SchemaGenerator, SchemaError};
pub use tool_metadata::{ToolMetadata, ToolMetadataRegistry, ToolCategory, ToolRiskLevel, ToolExample, ParameterDoc, RateLimit, DeprecationInfo};
pub use grpc_error_mapper::{GrpcErrorMapper, ErrorContext, ErrorCategory, ErrorSeverity};
pub use auto_registry::{AutoDiscoveryRegistry, AutoDiscoveryConfig, ServerType, ToolOverride, RegistryStatistics};
