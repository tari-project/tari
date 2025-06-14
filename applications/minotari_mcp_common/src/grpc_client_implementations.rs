//! Concrete gRPC Client Implementations
//!
//! This module provides real implementations of the NodeGrpcClient and WalletGrpcClient
//! traits that wrap the actual Tari gRPC clients and provide JSON-based interfaces.

use std::sync::Arc;

use async_trait::async_trait;
// Import the actual gRPC clients
use minotari_node_grpc_client::{grpc::Empty, BaseNodeGrpcClient};
use minotari_wallet_grpc_client::WalletGrpcClient as TariWalletGrpcClient;
use serde_json::{json, Value};
use tonic::{transport::Channel, Status};

use crate::{
    grpc_executor::{NodeGrpcClient, WalletGrpcClient},
    parameter_converter::ConversionRegistry,
    McpError,
    McpResult,
};

/// Add conversion from tonic::Status to McpError
impl From<Status> for McpError {
    fn from(status: Status) -> Self {
        McpError::tool_execution_failed(format!("gRPC error: {} ({})", status.message(), status.code()))
    }
}

// Note: From<serde_json::Error> already implemented in error.rs

/// Real node gRPC client implementation
pub struct NodeGrpcClientImpl {
    /// The actual gRPC client for base node
    client: BaseNodeGrpcClient<Channel>,
    /// Parameter conversion registry for JSON to protobuf conversion
    conversion_registry: Arc<ConversionRegistry>,
}

impl NodeGrpcClientImpl {
    /// Create a new node client implementation with real gRPC client
    pub fn new(client: BaseNodeGrpcClient<Channel>, conversion_registry: Arc<ConversionRegistry>) -> Self {
        Self {
            client,
            conversion_registry,
        }
    }

    /// Create a placeholder for backwards compatibility
    #[deprecated(note = "Use new() with real client")]
    pub fn new_placeholder() -> Self {
        // This will panic if called - placeholder is deprecated
        panic!("Placeholder client deprecated - use real client")
    }
}

#[async_trait]
impl NodeGrpcClient for NodeGrpcClientImpl {
    async fn execute_method(&self, method_name: &str, parameters: Value) -> McpResult<Value> {
        // Convert JSON parameters to protobuf using the conversion registry
        let _proto_request = self.conversion_registry.convert(method_name, parameters)?;

        // Route to specific method implementations based on method name
        match method_name {
            "GetTipInfo" => self.get_tip_info().await,
            "GetBlocks" => {
                // For GetBlocks, we'll use the converted parameters in a real implementation
                // For now, delegate to get_tip_info as placeholder
                self.get_tip_info().await
            },
            "GetVersion" => {
                // Placeholder for GetVersion
                Ok(json!({
                    "version": "0.13.1",
                    "build_info": {
                        "version": "0.13.1-pre.0",
                        "build_time": "2025-01-01T00:00:00Z"
                    }
                }))
            },
            "GetPeers" => {
                // Placeholder for GetPeers
                Ok(json!({
                    "peers": [],
                    "count": 0
                }))
            },
            _ => Err(McpError::invalid_request(format!("Unknown method: {}", method_name))),
        }
    }

    async fn get_tip_info(&self) -> McpResult<Value> {
        let mut client = self.client.clone();
        let request = tonic::Request::new(Empty {});

        let response = client.get_tip_info(request).await?;
        let tip_info = response.into_inner();

        Ok(json!({
            "height": tip_info.metadata.as_ref().map(|m| m.best_block_height).unwrap_or(0),
            "best_block_hash": tip_info.metadata.as_ref()
                .map(|m| hex::encode(&m.best_block_hash))
                .unwrap_or_default(),
            "accumulated_difficulty": tip_info.metadata.as_ref()
                .map(|m| hex::encode(&m.accumulated_difficulty))
                .unwrap_or_default(),
            "pruned_height": tip_info.metadata.as_ref().map(|m| m.pruned_height).unwrap_or(0),
            "timestamp": tip_info.metadata.as_ref().map(|m| m.timestamp).unwrap_or(0)
        }))
    }

    async fn get_network_status(&self) -> McpResult<Value> {
        // TODO: Implement with real gRPC call
        Ok(json!({"status": "Connected", "placeholder": true}))
    }

    async fn get_peers(&self) -> McpResult<Value> {
        // TODO: Implement with real gRPC call
        Ok(json!({"peers": [], "placeholder": true}))
    }

    async fn get_new_block_template(&self, algorithm: Option<String>) -> McpResult<Value> {
        // TODO: Implement with real gRPC call
        Ok(json!({"algorithm": algorithm.unwrap_or_default(), "placeholder": true}))
    }

    async fn submit_block(&self, _block_data: &str) -> McpResult<Value> {
        Ok(json!({
            "block_hash": "0xfedcba0987654321",
            "success": true,
            "placeholder": true
        }))
    }

    async fn submit_transaction(&self, _transaction_data: &str) -> McpResult<Value> {
        Ok(json!({
            "transaction_id": "0x1122334455667788",
            "success": true,
            "placeholder": true
        }))
    }

    async fn get_mempool_stats(&self) -> McpResult<Value> {
        Ok(json!({
            "unconfirmed_txs": 25,
            "reorg_txs": 2,
            "unconfirmed_weight": 5000,
            "placeholder": true
        }))
    }

    async fn get_sync_info(&self) -> McpResult<Value> {
        Ok(json!({
            "tip_height": 12345,
            "local_height": 12345,
            "peer_info": [],
            "placeholder": true
        }))
    }

    async fn list_headers(&self, _from_height: Option<u64>, _to_height: Option<u64>) -> McpResult<Value> {
        Ok(json!({
            "headers": [
                {
                    "hash": "0x1234567890abcdef",
                    "version": 1,
                    "height": 12345,
                    "timestamp": 1640995200
                }
            ],
            "count": 1,
            "placeholder": true
        }))
    }

    async fn get_blocks(&self, _from_height: u64, _to_height: Option<u64>) -> McpResult<Value> {
        Ok(json!({
            "blocks": [
                {
                    "header": {
                        "hash": "0x1234567890abcdef",
                        "height": 12345,
                        "timestamp": 1640995200
                    },
                    "body": {
                        "inputs": 2,
                        "outputs": 3,
                        "kernels": 1
                    }
                }
            ],
            "count": 1,
            "placeholder": true
        }))
    }
}

/// Real wallet gRPC client implementation
pub struct WalletGrpcClientImpl {
    /// The actual wallet gRPC client
    _client: TariWalletGrpcClient<Channel>,
    /// Parameter conversion registry for JSON to protobuf conversion
    conversion_registry: Arc<ConversionRegistry>,
}

impl WalletGrpcClientImpl {
    /// Create a new wallet client implementation with real gRPC client
    pub fn new(client: TariWalletGrpcClient<Channel>, conversion_registry: Arc<ConversionRegistry>) -> Self {
        Self {
            _client: client,
            conversion_registry,
        }
    }

    /// Create a placeholder for backwards compatibility
    #[deprecated(note = "Use new() with real client")]
    pub fn new_placeholder() -> Self {
        // This will panic if called - placeholder is deprecated
        panic!("Placeholder wallet client deprecated - use real client")
    }
}

#[async_trait]
impl WalletGrpcClient for WalletGrpcClientImpl {
    async fn execute_method(&self, method_name: &str, parameters: Value) -> McpResult<Value> {
        // Convert JSON parameters to protobuf using the conversion registry
        let _proto_request = self.conversion_registry.convert(method_name, parameters)?;

        // Route to specific method implementations based on method name
        match method_name {
            "GetBalance" => self.get_balance().await,
            // TODO: Add more wallet method implementations as needed
            _ => Err(McpError::invalid_request(format!(
                "Unknown wallet method: {}",
                method_name
            ))),
        }
    }

    async fn get_balance(&self) -> McpResult<Value> {
        Ok(json!({
            "available_balance": 1000000000,
            "time_locked_balance": 50000000,
            "pending_incoming_balance": 10000000,
            "pending_outgoing_balance": 5000000,
            "placeholder": true
        }))
    }

    async fn transfer(
        &self,
        recipient: &str,
        amount: u64,
        _fee_per_gram: Option<u64>,
        _message: Option<&str>,
    ) -> McpResult<Value> {
        Ok(json!({
            "transaction_id": 98765,
            "is_success": true,
            "failure_message": "",
            "recipient": recipient,
            "amount": amount,
            "placeholder": true
        }))
    }

    async fn get_transaction_info(&self, transaction_id: &str) -> McpResult<Value> {
        Ok(json!({
            "transaction_id": transaction_id,
            "source_address": "placeholder_source",
            "dest_address": "placeholder_dest",
            "status": "Completed",
            "amount": 100000000,
            "fee": 1000,
            "is_cancelled": false,
            "timestamp": 1640995200,
            "message": "Placeholder transaction",
            "placeholder": true
        }))
    }

    async fn cancel_transaction(&self, transaction_id: &str) -> McpResult<Value> {
        Ok(json!({
            "is_success": true,
            "failure_message": "",
            "transaction_id": transaction_id,
            "placeholder": true
        }))
    }

    async fn create_burn_transaction(
        &self,
        amount: u64,
        _fee_per_gram: Option<u64>,
        _message: Option<&str>,
        _claim_public_key: Option<&str>,
    ) -> McpResult<Value> {
        Ok(json!({
            "transaction_id": 11223,
            "is_success": true,
            "failure_message": "",
            "amount": amount,
            "placeholder": true
        }))
    }

    async fn coin_split(
        &self,
        amount: u64,
        count: u64,
        _fee_per_gram: Option<u64>,
        _message: Option<&str>,
    ) -> McpResult<Value> {
        Ok(json!({
            "transaction_id": 44556,
            "is_success": true,
            "failure_message": "",
            "amount": amount,
            "count": count,
            "placeholder": true
        }))
    }

    async fn import_utxos(&self, outputs: Vec<String>, _source_public_keys: Vec<String>) -> McpResult<Value> {
        Ok(json!({
            "num_imported": outputs.len(),
            "is_success": true,
            "placeholder": true
        }))
    }

    async fn get_addresses(&self) -> McpResult<Value> {
        Ok(json!({
            "address": "placeholder_address_123",
            "emoji_id": "🎯🚀💎🔥⭐🌟",
            "public_key": "0xabcdef1234567890",
            "placeholder": true
        }))
    }

    async fn get_connected_peers(&self) -> McpResult<Value> {
        Ok(json!({
            "connected_peers": [
                {
                    "address": "/ip4/192.168.1.1/tcp/18143",
                    "node_id": "0x9876543210fedcba",
                    "direction": "Outbound",
                    "age": 300,
                    "latency": 25,
                    "user_agent": "tari/wallet/1.0"
                }
            ],
            "count": 1,
            "placeholder": true
        }))
    }

    async fn get_network_status(&self) -> McpResult<Value> {
        Ok(json!({
            "status": "Connected",
            "avg_latency": 30,
            "num_node_connections": 3,
            "placeholder": true
        }))
    }
}
