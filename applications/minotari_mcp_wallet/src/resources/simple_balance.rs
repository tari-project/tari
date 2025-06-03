//! Simple balance resource for testing

use minotari_mcp_common::{McpResource, McpResult, McpError};
use minotari_wallet_grpc_client::WalletGrpcClient;
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tonic::transport::Channel;

/// Simple resource providing wallet balance information
pub struct SimpleBalanceResource {
    grpc_client: Arc<WalletGrpcClient<Channel>>,
}

impl SimpleBalanceResource {
    pub fn new(grpc_client: Arc<WalletGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait]
impl McpResource for SimpleBalanceResource {
    fn uri(&self) -> &str {
        "simple_balance"
    }

    fn name(&self) -> &str {
        "Simple Wallet Balance"
    }

    fn description(&self) -> &str {
        "Simple wallet balance information (placeholder implementation)"
    }

    fn mime_type(&self) -> &str {
        "application/json"
    }

    async fn read(&self) -> McpResult<Value> {
        // For now, return placeholder data
        // In a real implementation, this would call the wallet gRPC service
        Ok(serde_json::json!({
            "available_balance": 0,
            "time_locked_balance": 0,
            "pending_incoming_balance": 0,
            "pending_outgoing_balance": 0,
            "total_balance": 0,
            "message": "Balance functionality not yet implemented",
            "note": "This is a placeholder - actual wallet integration pending"
        }))
    }
}
