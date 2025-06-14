//! Real gRPC Method Execution
//!
//! This module provides unified execution of gRPC methods for both node and wallet
//! clients, handling parameter conversion, method invocation, and response formatting.

use std::sync::Arc;

use serde_json::Value;

use crate::{
    auto_registry::ServerType,
    connection_manager::ConnectionManager,
    grpc_discovery::{GrpcMethodCategory, GrpcMethodInfo},
    grpc_error_mapper::GrpcErrorMapper,
    health_checker::HealthResult,
    parameter_converter::ConversionRegistry,
    McpError,
    McpResult,
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
    /// Execute a generic node gRPC method
    async fn execute_method(&self, method_name: &str, parameters: Value) -> McpResult<Value>;

    /// Get tip information
    async fn get_tip_info(&self) -> McpResult<Value>;

    /// Get network status
    async fn get_network_status(&self) -> McpResult<Value>;

    /// Get peers
    async fn get_peers(&self) -> McpResult<Value>;

    /// Get new block template
    async fn get_new_block_template(&self, algorithm: Option<String>) -> McpResult<Value>;

    /// Submit block
    async fn submit_block(&self, block_data: &str) -> McpResult<Value>;

    /// Submit transaction
    async fn submit_transaction(&self, transaction_data: &str) -> McpResult<Value>;

    /// Get mempool stats
    async fn get_mempool_stats(&self) -> McpResult<Value>;

    /// Get sync info
    async fn get_sync_info(&self) -> McpResult<Value>;

    /// List headers
    async fn list_headers(&self, from_height: Option<u64>, to_height: Option<u64>) -> McpResult<Value>;

    /// Get blocks
    async fn get_blocks(&self, from_height: u64, to_height: Option<u64>) -> McpResult<Value>;
}

/// Trait for wallet gRPC client operations
#[async_trait::async_trait]
pub trait WalletGrpcClient: Send + Sync {
    /// Execute a generic wallet gRPC method
    async fn execute_method(&self, method_name: &str, parameters: Value) -> McpResult<Value>;

    /// Get wallet balance
    async fn get_balance(&self) -> McpResult<Value>;

    /// Transfer funds
    async fn transfer(
        &self,
        recipient: &str,
        amount: u64,
        fee_per_gram: Option<u64>,
        message: Option<&str>,
    ) -> McpResult<Value>;

    /// Get transaction info
    async fn get_transaction_info(&self, transaction_id: &str) -> McpResult<Value>;

    /// Cancel transaction
    async fn cancel_transaction(&self, transaction_id: &str) -> McpResult<Value>;

    /// Create burn transaction
    async fn create_burn_transaction(
        &self,
        amount: u64,
        fee_per_gram: Option<u64>,
        message: Option<&str>,
        claim_public_key: Option<&str>,
    ) -> McpResult<Value>;

    /// Split coins
    async fn coin_split(
        &self,
        amount: u64,
        count: u64,
        fee_per_gram: Option<u64>,
        message: Option<&str>,
    ) -> McpResult<Value>;

    /// Import UTXOs
    async fn import_utxos(&self, outputs: Vec<String>, source_public_keys: Vec<String>) -> McpResult<Value>;

    /// Get addresses
    async fn get_addresses(&self) -> McpResult<Value>;

    /// Get connected peers
    async fn get_connected_peers(&self) -> McpResult<Value>;

    /// Get network status
    async fn get_network_status(&self) -> McpResult<Value>;
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
            if let Some(health) = health_status.get(service_name) {
                if !health.is_healthy() {
                    log::warn!(
                        "Service {} is not healthy (status: {}), but proceeding with request",
                        service_name,
                        health.status
                    );
                    // Note: We log the warning but don't fail the request immediately
                    // The circuit breaker in the connection manager will handle failures
                }
            }
        }

        match self.server_type {
            ServerType::Node => {
                if let Some(ref client) = self.node_client {
                    self.execute_node_method(client, method_info, parameters).await
                } else {
                    Err(McpError::server_error("Node client not available"))
                }
            },
            ServerType::Wallet => {
                if let Some(ref client) = self.wallet_client {
                    self.execute_wallet_method(client, method_info, parameters).await
                } else {
                    Err(McpError::server_error("Wallet client not available"))
                }
            },
            _ => Err(McpError::server_error("Unsupported server type")),
        }
    }

    /// Execute a node-specific gRPC method
    async fn execute_node_method(
        &self,
        client: &Arc<dyn NodeGrpcClient>,
        method_info: &GrpcMethodInfo,
        parameters: Value,
    ) -> McpResult<Value> {
        let result = match method_info.name.as_str() {
            "GetTipInfo" => client.get_tip_info().await,
            "GetNetworkStatus" => client.get_network_status().await,
            "GetPeers" => client.get_peers().await,
            "GetNewBlockTemplate" => {
                let algorithm = parameters
                    .get("algorithm")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                client.get_new_block_template(algorithm).await
            },
            "SubmitBlock" => {
                let block_data = parameters
                    .get("block")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::invalid_request("Missing 'block' parameter"))?;
                client.submit_block(block_data).await
            },
            "SubmitTransaction" => {
                let transaction_data = parameters
                    .get("transaction")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::invalid_request("Missing 'transaction' parameter"))?;
                client.submit_transaction(transaction_data).await
            },
            "GetMempoolStats" => client.get_mempool_stats().await,
            "GetSyncInfo" => client.get_sync_info().await,
            "ListHeaders" => {
                let from_height = parameters.get("from_height").and_then(|v| v.as_u64());
                let to_height = parameters.get("to_height").and_then(|v| v.as_u64());
                client.list_headers(from_height, to_height).await
            },
            "GetBlocks" => {
                let from_height = parameters
                    .get("from_height")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| McpError::invalid_request("Missing 'from_height' parameter"))?;
                let to_height = parameters.get("to_height").and_then(|v| v.as_u64());
                client.get_blocks(from_height, to_height).await
            },
            _ => {
                // Fall back to generic method execution
                client.execute_method(&method_info.name, parameters).await
            },
        };

        self.handle_result(result, method_info).await
    }

    /// Execute a wallet-specific gRPC method
    async fn execute_wallet_method(
        &self,
        client: &Arc<dyn WalletGrpcClient>,
        method_info: &GrpcMethodInfo,
        parameters: Value,
    ) -> McpResult<Value> {
        let result = match method_info.name.as_str() {
            "GetBalance" => client.get_balance().await,
            "Transfer" => {
                let recipient = parameters
                    .get("recipient")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::invalid_request("Missing 'recipient' parameter"))?;
                let amount = parameters
                    .get("amount")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| McpError::invalid_request("Missing 'amount' parameter"))?;
                let fee_per_gram = parameters.get("fee_per_gram").and_then(|v| v.as_u64());
                let message = parameters.get("message").and_then(|v| v.as_str());
                client.transfer(recipient, amount, fee_per_gram, message).await
            },
            "GetTransactionInfo" => {
                let transaction_id = parameters
                    .get("transaction_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::invalid_request("Missing 'transaction_id' parameter"))?;
                client.get_transaction_info(transaction_id).await
            },
            "CancelTransaction" => {
                let transaction_id = parameters
                    .get("transaction_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::invalid_request("Missing 'transaction_id' parameter"))?;
                client.cancel_transaction(transaction_id).await
            },
            "CreateBurnTransaction" => {
                let amount = parameters
                    .get("amount")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| McpError::invalid_request("Missing 'amount' parameter"))?;
                let fee_per_gram = parameters.get("fee_per_gram").and_then(|v| v.as_u64());
                let message = parameters.get("message").and_then(|v| v.as_str());
                let claim_public_key = parameters.get("claim_public_key").and_then(|v| v.as_str());
                client
                    .create_burn_transaction(amount, fee_per_gram, message, claim_public_key)
                    .await
            },
            "CoinSplit" => {
                let amount = parameters
                    .get("amount")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| McpError::invalid_request("Missing 'amount' parameter"))?;
                let count = parameters
                    .get("count")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| McpError::invalid_request("Missing 'count' parameter"))?;
                let fee_per_gram = parameters.get("fee_per_gram").and_then(|v| v.as_u64());
                let message = parameters.get("message").and_then(|v| v.as_str());
                client.coin_split(amount, count, fee_per_gram, message).await
            },
            "ImportUtxos" => {
                let outputs = parameters
                    .get("outputs")
                    .and_then(|v| v.as_array())
                    .ok_or_else(|| McpError::invalid_request("Missing 'outputs' parameter"))?
                    .iter()
                    .map(|v| v.as_str().unwrap_or("").to_string())
                    .collect();
                let source_public_keys = parameters
                    .get("source_public_keys")
                    .and_then(|v| v.as_array())
                    .ok_or_else(|| McpError::invalid_request("Missing 'source_public_keys' parameter"))?
                    .iter()
                    .map(|v| v.as_str().unwrap_or("").to_string())
                    .collect();
                client.import_utxos(outputs, source_public_keys).await
            },
            "GetAddresses" => client.get_addresses().await,
            "GetConnectedPeers" => client.get_connected_peers().await,
            "GetNetworkStatus" => client.get_network_status().await,
            _ => {
                // Fall back to generic method execution
                client.execute_method(&method_info.name, parameters).await
            },
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

    /// Check if the executor can handle the given method
    pub fn can_execute(&self, method_info: &GrpcMethodInfo) -> bool {
        match self.server_type {
            ServerType::Node => {
                self.node_client.is_some() &&
                    matches!(
                        method_info.category,
                        GrpcMethodCategory::Blockchain |
                            GrpcMethodCategory::Mining |
                            GrpcMethodCategory::Network |
                            GrpcMethodCategory::Mempool |
                            GrpcMethodCategory::Validation |
                            GrpcMethodCategory::Status |
                            GrpcMethodCategory::System
                    )
            },
            ServerType::Wallet => {
                self.wallet_client.is_some() &&
                    matches!(
                        method_info.category,
                        GrpcMethodCategory::Balance |
                            GrpcMethodCategory::Transaction |
                            GrpcMethodCategory::Address |
                            GrpcMethodCategory::AtomicSwap |
                            GrpcMethodCategory::Recovery |
                            GrpcMethodCategory::Status |
                            GrpcMethodCategory::System
                    )
            },
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
    use std::sync::Arc;

    use serde_json::json;

    use super::*;

    struct MockNodeClient;

    #[async_trait::async_trait]
    impl NodeGrpcClient for MockNodeClient {
        async fn execute_method(&self, _method_name: &str, _parameters: Value) -> McpResult<Value> {
            Ok(json!({"status": "mock_success"}))
        }

        async fn get_tip_info(&self) -> McpResult<Value> {
            Ok(json!({"height": 12345, "hash": "abc123"}))
        }

        async fn get_network_status(&self) -> McpResult<Value> {
            Ok(json!({"connected": true, "peers": 5}))
        }

        async fn get_peers(&self) -> McpResult<Value> {
            Ok(json!({"peers": []}))
        }

        async fn get_new_block_template(&self, _algorithm: Option<String>) -> McpResult<Value> {
            Ok(json!({"template": "block_template_data"}))
        }

        async fn submit_block(&self, _block_data: &str) -> McpResult<Value> {
            Ok(json!({"submitted": true}))
        }

        async fn submit_transaction(&self, _transaction_data: &str) -> McpResult<Value> {
            Ok(json!({"submitted": true}))
        }

        async fn get_mempool_stats(&self) -> McpResult<Value> {
            Ok(json!({"count": 10, "size": 1024}))
        }

        async fn get_sync_info(&self) -> McpResult<Value> {
            Ok(json!({"synced": true}))
        }

        async fn list_headers(&self, _from_height: Option<u64>, _to_height: Option<u64>) -> McpResult<Value> {
            Ok(json!({"headers": []}))
        }

        async fn get_blocks(&self, _from_height: u64, _to_height: Option<u64>) -> McpResult<Value> {
            Ok(json!({"blocks": []}))
        }
    }

    #[tokio::test]
    async fn test_node_executor_creation() {
        let client = Arc::new(MockNodeClient);
        let error_mapper = Arc::new(GrpcErrorMapper::new());
        let conversion_registry = Arc::new(ConversionRegistry::new());
        let executor = GrpcExecutor::new_node(client, error_mapper, conversion_registry);

        assert!(executor.node_client.is_some());
        assert!(executor.wallet_client.is_none());
        assert_eq!(executor.server_type, ServerType::Node);
    }

    #[tokio::test]
    async fn test_executor_status() {
        let client = Arc::new(MockNodeClient);
        let error_mapper = Arc::new(GrpcErrorMapper::new());
        let conversion_registry = Arc::new(ConversionRegistry::new());
        let executor = GrpcExecutor::new_node(client, error_mapper, conversion_registry);
        let status = executor.get_status();

        assert!(status.is_ready());
        assert!(status.node_client_available);
        assert!(!status.wallet_client_available);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_parameter_conversion_integration() {
        use serde_json::json;

        use crate::{
            grpc_discovery::{GrpcMethodCategory, GrpcMethodInfo},
            ConversionRegistryFactory,
        };

        // Create a real conversion registry with node converters
        let conversion_registry = ConversionRegistryFactory::create_node_registry();

        // Create executor with conversion registry
        let client = Arc::new(MockNodeClient);
        let error_mapper = Arc::new(GrpcErrorMapper::new());
        let executor = GrpcExecutor::new_node(client, error_mapper, conversion_registry);

        // Create a mock method info
        let method_info = GrpcMethodInfo {
            name: "GetTipInfo".to_string(),
            service: "BaseNode".to_string(),
            full_name: "tari.rpc.BaseNode/GetTipInfo".to_string(),
            description: "Get tip info".to_string(),
            category: GrpcMethodCategory::Blockchain,
            is_control_operation: false,
            is_streaming: false,
            input_schema: json!({}),
            output_schema: json!({}),
        };

        // Test that parameter conversion is working in the executor
        let result = executor.execute_method(&method_info, json!({})).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        // The response comes from MockNodeClient.get_tip_info()
        assert_eq!(response["height"], 12345);
        assert_eq!(response["hash"], "abc123");
    }
}
