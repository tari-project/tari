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
//! This crate provides shared utilities, traits, and types for implementing
//! MCP servers in the Tari ecosystem, following security-first principles
//! with local-only binding and explicit control operation permissions.

pub mod auto_registry;
pub mod cli_integration;
pub mod config;
pub mod connection_manager;
pub mod conversion_registry_factory;
pub mod error;
pub mod executable_finder;
pub mod grpc_client_implementations;
pub mod grpc_discovery;
pub mod grpc_error_mapper;
pub mod grpc_executor;
pub mod health_checker;
pub mod health_monitor;
pub mod input_sanitizer;
pub mod method_implementations;
pub mod parameter_converter;
pub mod process_launcher;
pub mod process_manager;
pub mod prompts;
pub mod protobuf_reflector_simple;
pub mod resources;
pub mod response_converter;
pub mod schema_generator;
pub mod security;
pub mod server;
pub mod startup_diagnostics;
pub mod stdio_transport;
pub mod tool_macros;
pub mod tool_metadata;
pub mod tools;
pub mod transport;
// pub mod protobuf_integration; // Disabled pending API compatibility fixes

pub use auto_registry::{AutoDiscoveryConfig, AutoDiscoveryRegistry, RegistryStatistics, ServerType, ToolOverride};
pub use cli_integration::{
    CliConfigBuilder,
    CliConfigExtractor,
    CliIntegrationUtils,
    LaunchCliConfig,
    NodeArgumentBuilder,
    WalletArgumentBuilder,
};
pub use config::McpConfig;
pub use connection_manager::{
    CircuitBreaker,
    CircuitBreakerConfig,
    CircuitBreakerState,
    ConnectionManager,
    ConnectionPoolConfig,
    ManagedConnection,
};
pub use conversion_registry_factory::ConversionRegistryFactory;
pub use error::{McpError, McpResult};
pub use executable_finder::{ExecutableFinder, TariExecutables};
pub use grpc_client_implementations::{NodeGrpcClientImpl, WalletGrpcClientImpl};
pub use grpc_discovery::{GrpcMethodCategory, GrpcMethodInfo, ServiceDiscovery, base_node_methods, wallet_methods};
pub use grpc_error_mapper::{ErrorCategory, ErrorContext, ErrorSeverity, GrpcErrorMapper};
pub use grpc_executor::{ExecutorStatus, GrpcExecutor, NodeGrpcClient, WalletGrpcClient};
pub use health_checker::{
    GrpcHealthChecker,
    HealthChecker,
    HealthConfig,
    HealthResult,
    HealthStatus as GrpcHealthStatus,
};
pub use health_monitor::{HealthCheckResult, HealthMonitor, HealthStatus, ServiceHealthMonitors};
pub use input_sanitizer::{InputSanitizer, ValidationPatterns, sanitize_tool_input};
pub use method_implementations::{register_node_converters, register_wallet_converters};
pub use parameter_converter::{ConversionError, ConversionRegistry, JsonParameterExtractor, ParameterConverter};
pub use process_launcher::{
    HealthCheckConfig,
    LaunchConfig,
    LaunchConfigBuilder,
    LaunchResult,
    ProcessLaunchStatus,
    ProcessLauncher,
    TariProcessLauncher,
};
pub use process_manager::{ProcessStatus, ProcessSupervisor, ProcessType, ProcessUtils};
pub use prompts::{
    McpPrompt,
    MessageRole,
    PromptContent,
    PromptMessage,
    PromptRegistry,
    resource_message,
    text_message,
};
pub use protobuf_reflector_simple::ProtobufReflector;
pub use resources::{McpResource, ResourceRegistry};
pub use response_converter::{
    GenericJsonConverter,
    NodeResponseConverter,
    ResponseConverter,
    ResponseConverterFactory,
    ResponseConverterRegistry,
    WalletResponseConverter,
};
pub use schema_generator::{SchemaError, SchemaGenerator};
pub use security::{PermissionLevel, SecurityContext};
pub use server::{McpServer, McpServerBuilder};
pub use startup_diagnostics::{DiagnosticResult, DiagnosticStatus, StartupDiagnostics};
pub use stdio_transport::StdioTransport;
pub use tool_metadata::{
    DeprecationInfo,
    ParameterDoc,
    RateLimit,
    ToolCategory,
    ToolExample,
    ToolMetadata,
    ToolMetadataRegistry,
    ToolRiskLevel,
};
pub use tools::{
    McpTool,
    ToolRegistry,
    get_optional_string_param,
    get_required_bool_param,
    get_required_number_param,
    get_required_string_param,
    get_required_u64_param,
};
// Export all macros for public use
// pub use protobuf_integration::{ReflectiveAutoDiscovery, EnhancedToolMetadata, ToolDocumentation, OpenApiSpec}; //
// Disabled pending API compatibility fixes
