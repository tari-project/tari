//! Wallet balance resource

use minotari_mcp_common::{McpResource, McpResult, McpError};
use minotari_wallet_grpc_client::{WalletGrpcClient, grpc::GetBalanceRequest};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tonic::transport::Channel;

/// Resource providing current wallet balance information
pub struct BalanceResource {
    grpc_client: Arc<WalletGrpcClient<Channel>>,
}

impl BalanceResource {
    pub fn new(grpc_client: Arc<WalletGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait]
impl McpResource for BalanceResource {
    fn uri(&self) -> &str {
        "balance"
    }

    fn name(&self) -> &str {
        "Wallet Balance"
    }

    fn description(&self) -> &str {
        "Current wallet balance breakdown including available, pending, and timelocked funds"
    }

    fn mime_type(&self) -> &str {
        "application/json"
    }

    async fn read(&self) -> McpResult<Value> {
        let mut client = self.grpc_client.as_ref().clone();
        
        let response = client
            .get_balance(GetBalanceRequest {})
            .await
            .map_err(|e| McpError::resource_access_failed(format!("Failed to get balance: {}", e)))?;

        let balance = response.into_inner();

        Ok(serde_json::json!({
            "available_balance": balance.available_balance,
            "time_locked_balance": balance.time_locked_balance,
            "pending_incoming_balance": balance.pending_incoming_balance,
            "pending_outgoing_balance": balance.pending_outgoing_balance,
            "total_balance": balance.available_balance + balance.time_locked_balance,
            "pending_balance": balance.pending_incoming_balance + balance.pending_outgoing_balance,
            "spendable_balance": balance.available_balance,
            "balance_breakdown": {
                "mature_utxos": balance.available_balance,
                "immature_utxos": balance.time_locked_balance,
                "incoming_pending": balance.pending_incoming_balance,
                "outgoing_pending": balance.pending_outgoing_balance
            }
        }))
    }
}
