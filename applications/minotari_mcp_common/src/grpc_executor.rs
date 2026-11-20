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
//! Real gRPC Method Execution
//!
//! This module provides unified execution of gRPC methods for both node and wallet
//! clients, handling parameter conversion, method invocation, and response formatting.

use std::sync::Arc;

use serde_json::Value;

use crate::{
    McpError,
    McpResult,
    auto_registry::ServerType,
    connection_manager::ConnectionManager,
    grpc_client_implementations::{NODE_METHODS, WALLET_METHODS},
    grpc_discovery::GrpcMethodInfo,
    grpc_error_mapper::GrpcErrorMapper,
    health_checker::HealthResult,
    parameter_converter::ConversionRegistry,
};

/// Unified gRPC method executor for node and wallet clients
#[derive(Clone)]
pub struct GrpcExecutor {
    /// Node gRPC client (optional)
    pub node_client: Option<Arc<dyn NodeGrpcClient>>,
    /// Wallet gRPC client (optional)  
    pub wallet_client: Option<Arc<dyn WalletGrpcClient>>,
    /// Error mapper for consistent error handling
    pub error_mapper: Arc<GrpcErrorMapper>,
    /// Server type this executor is configured for
    pub server_type: ServerType,
    /// Parameter conversion registry for JSON to protobuf conversion
    pub conversion_registry: Arc<ConversionRegistry>,
    /// Connection manager for health monitoring and circuit breakers (optional)
    pub connection_manager: Option<Arc<ConnectionManager>>,
}

/// Trait for node gRPC client operations
#[async_trait::async_trait]
pub trait NodeGrpcClient: Send + Sync {
    /// Execute a node gRPC method with its JSON tool arguments
    async fn execute_method(&self, method_name: &str, parameters: Value) -> McpResult<Value>;
}

/// Trait for wallet gRPC client operations
#[async_trait::async_trait]
pub trait WalletGrpcClient: Send + Sync {
    /// Execute a wallet gRPC method with its JSON tool arguments
    async fn execute_method(&self, method_name: &str, parameters: Value) -> McpResult<Value>;
}

impl GrpcExecutor {
    /// Create a new gRPC executor for node operations
    pub fn new_node(
        client: Arc<dyn NodeGrpcClient>,
        error_mapper: Arc<GrpcErrorMapper>,
        conversion_registry: Arc<ConversionRegistry>,
    ) -> Self {
        Self {
            node_client: Some(client),
            wallet_client: None,
            error_mapper,
            server_type: ServerType::Node,
            conversion_registry,
            connection_manager: None,
        }
    }

    /// Create a new gRPC executor for node operations with health monitoring
    pub fn new_node_with_health(
        client: Arc<dyn NodeGrpcClient>,
        error_mapper: Arc<GrpcErrorMapper>,
        conversion_registry: Arc<ConversionRegistry>,
        connection_manager: Arc<ConnectionManager>,
    ) -> Self {
        Self {
            node_client: Some(client),
            wallet_client: None,
            error_mapper,
            server_type: ServerType::Node,
            conversion_registry,
            connection_manager: Some(connection_manager),
        }
    }

    /// Create a new gRPC executor for wallet operations  
    pub fn new_wallet(
        client: Arc<dyn WalletGrpcClient>,
        error_mapper: Arc<GrpcErrorMapper>,
        conversion_registry: Arc<ConversionRegistry>,
    ) -> Self {
        Self {
            node_client: None,
            wallet_client: Some(client),
            error_mapper,
            server_type: ServerType::Wallet,
            conversion_registry,
            connection_manager: None,
        }
    }

    /// Create a new gRPC executor for wallet operations with health monitoring
    pub fn new_wallet_with_health(
        client: Arc<dyn WalletGrpcClient>,
        error_mapper: Arc<GrpcErrorMapper>,
        conversion_registry: Arc<ConversionRegistry>,
        connection_manager: Arc<ConnectionManager>,
    ) -> Self {
        Self {
            node_client: None,
            wallet_client: Some(client),
            error_mapper,
            server_type: ServerType::Wallet,
            conversion_registry,
            connection_manager: Some(connection_manager),
        }
    }

    /// Execute a gRPC method with the provided parameters
    pub async fn execute_method(&self, method_info: &GrpcMethodInfo, parameters: Value) -> McpResult<Value> {
        log::debug!(
            "Executing gRPC method: {} with parameters: {}",
            method_info.name,
            parameters
        );

        // Check health status if connection manager is available
        if let Some(ref conn_manager) = self.connection_manager {
            let service_name = match self.server_type {
                ServerType::Node => "base_node",
                ServerType::Wallet => "wallet",
                _ => "unknown",
            };

            // Get health status and check if service is healthy
            let health_status = conn_manager.get_all_health_status();
            if let Some(health) = health_status.get(service_name) &&
                !health.is_healthy()
            {
                log::warn!(
                    "Service {} is not healthy (status: {}), but proceeding with request",
                    service_name,
                    health.status
                );
                // Note: We log the warning but don't fail the request immediately
                // The circuit breaker in the connection manager will handle failures
            }
        }

        if !self.can_execute(method_info) {
            return Err(McpError::not_implemented(&method_info.name));
        }

        let result = match (self.server_type, &self.node_client, &self.wallet_client) {
            (ServerType::Node, Some(client), _) => client.execute_method(&method_info.name, parameters).await,
            (ServerType::Wallet, _, Some(client)) => client.execute_method(&method_info.name, parameters).await,
            _ => Err(McpError::server_error("Client not available")),
        };

        self.handle_result(result, method_info).await
    }

    /// Handle the result from gRPC execution, applying error mapping
    async fn handle_result(&self, result: McpResult<Value>, method_info: &GrpcMethodInfo) -> McpResult<Value> {
        match result {
            Ok(response) => {
                log::debug!("Successfully executed method: {}", method_info.name);
                Ok(response)
            },
            Err(error) => {
                log::warn!("gRPC method {} failed: {}", method_info.name, error);
                // Error is already McpError, just return it
                // TODO: Enhance with additional context from method_info if needed
                Err(error)
            },
        }
    }

    /// Whether the executor has a real implementation for the given method
    pub fn can_execute(&self, method_info: &GrpcMethodInfo) -> bool {
        let name = method_info.name.as_str();
        match self.server_type {
            ServerType::Node => self.node_client.is_some() && NODE_METHODS.contains(&name),
            ServerType::Wallet => self.wallet_client.is_some() && WALLET_METHODS.contains(&name),
            _ => false,
        }
    }

    /// Get a status summary of the executor
    pub fn get_status(&self) -> ExecutorStatus {
        let health_status = if let Some(ref conn_manager) = self.connection_manager {
            let service_name = match self.server_type {
                ServerType::Node => "base_node",
                ServerType::Wallet => "wallet",
                _ => "unknown",
            };
            conn_manager.get_all_health_status().get(service_name).cloned()
        } else {
            None
        };

        ExecutorStatus {
            server_type: self.server_type,
            node_client_available: self.node_client.is_some(),
            wallet_client_available: self.wallet_client.is_some(),
            health_monitoring_enabled: self.connection_manager.is_some(),
            health_status,
        }
    }

    /// Get detailed health and circuit breaker status if available
    pub fn get_detailed_status(&self) -> DetailedExecutorStatus {
        let health_status = self
            .connection_manager
            .as_ref()
            .map(|cm| cm.get_all_health_status())
            .unwrap_or_default();

        let circuit_breaker_status = self
            .connection_manager
            .as_ref()
            .map(|cm| cm.get_all_circuit_breaker_status())
            .unwrap_or_default();

        DetailedExecutorStatus {
            basic_status: self.get_status(),
            all_health_status: health_status,
            circuit_breaker_status,
        }
    }
}

/// Status information about the executor
#[derive(Debug, Clone)]
pub struct ExecutorStatus {
    pub server_type: ServerType,
    pub node_client_available: bool,
    pub wallet_client_available: bool,
    pub health_monitoring_enabled: bool,
    pub health_status: Option<HealthResult>,
}

/// Detailed status including health and circuit breaker metrics
#[derive(Debug, Clone)]
pub struct DetailedExecutorStatus {
    pub basic_status: ExecutorStatus,
    pub all_health_status: std::collections::HashMap<String, HealthResult>,
    pub circuit_breaker_status: std::collections::HashMap<String, crate::connection_manager::CircuitBreakerMetrics>,
}

impl ExecutorStatus {
    /// Check if the executor is ready for operations
    pub fn is_ready(&self) -> bool {
        let client_available = match self.server_type {
            ServerType::Node => self.node_client_available,
            ServerType::Wallet => self.wallet_client_available,
            _ => false,
        };

        // If health monitoring is enabled, also check health status
        if self.health_monitoring_enabled {
            if let Some(ref health) = self.health_status {
                client_available && health.is_healthy()
            } else {
                // Health monitoring enabled but no status available - not ready
                false
            }
        } else {
            // No health monitoring, just check client availability
            client_available
        }
    }

    /// Check if health monitoring is active and service is healthy
    pub fn is_healthy(&self) -> Option<bool> {
        self.health_status.as_ref().map(|h| h.is_healthy())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)]
    use std::sync::Arc;

    use serde_json::json;

    use super::*;
    use crate::grpc_discovery::{base_node_methods, wallet_methods};

    struct MockNodeClient;

    #[async_trait::async_trait]
    impl NodeGrpcClient for MockNodeClient {
        async fn execute_method(&self, method_name: &str, _parameters: Value) -> McpResult<Value> {
            match method_name {
                "GetTipInfo" => Ok(json!({"height": 12345, "hash": "abc123"})),
                _ => Err(McpError::not_implemented(method_name)),
            }
        }
    }

    fn node_executor() -> GrpcExecutor {
        GrpcExecutor::new_node(
            Arc::new(MockNodeClient),
            Arc::new(GrpcErrorMapper::new()),
            Arc::new(ConversionRegistry::new()),
        )
    }

    fn method(name: &str) -> GrpcMethodInfo {
        base_node_methods().into_iter().find(|m| m.name == name).unwrap()
    }

    #[tokio::test]
    async fn test_node_executor_creation() {
        let executor = node_executor();

        assert!(executor.node_client.is_some());
        assert!(executor.wallet_client.is_none());
        assert_eq!(executor.server_type, ServerType::Node);
    }

    #[tokio::test]
    async fn test_executor_status() {
        let status = node_executor().get_status();

        assert!(status.is_ready());
        assert!(status.node_client_available);
        assert!(!status.wallet_client_available);
    }

    #[tokio::test]
    async fn test_execute_routes_to_client() {
        let response = node_executor()
            .execute_method(&method("GetTipInfo"), json!({}))
            .await
            .unwrap();
        assert_eq!(response["height"], 12345);
        assert_eq!(response["hash"], "abc123");
    }

    #[tokio::test]
    async fn unimplemented_methods_are_unsupported_and_error() {
        let executor = node_executor();
        let identify = method("Identify");
        assert!(!executor.can_execute(&identify));
        assert!(matches!(
            executor.execute_method(&identify, json!({})).await,
            Err(McpError::NotImplemented(_))
        ));
        assert!(executor.can_execute(&method("SubmitTransaction")));
    }

    #[test]
    fn every_supported_method_is_a_discovered_method() {
        let node: Vec<String> = base_node_methods().into_iter().map(|m| m.name).collect();
        let wallet: Vec<String> = wallet_methods().into_iter().map(|m| m.name).collect();
        assert!(NODE_METHODS.iter().all(|m| node.iter().any(|n| n == m)));
        assert!(WALLET_METHODS.iter().all(|m| wallet.iter().any(|n| n == m)));
    }

    #[test]
    fn no_placeholder_responses_remain() {
        let needle = concat!("\"place", "holder\"");
        for source in [
            include_str!("grpc_client_implementations.rs"),
            include_str!("node_rpc.rs"),
            include_str!("wallet_rpc.rs"),
            include_str!("auto_registry.rs"),
        ] {
            assert!(!source.contains(needle));
        }
    }
}
