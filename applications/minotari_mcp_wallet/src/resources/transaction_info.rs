//! Transaction info resource (templated)

use minotari_mcp_common::{McpResource, McpResult, McpError};
use minotari_wallet_grpc_client::{WalletGrpcClient, grpc::GetTransactionInfoRequest};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tonic::transport::Channel;

/// Resource providing detailed information about a specific transaction
pub struct TransactionInfoResource {
    grpc_client: Arc<WalletGrpcClient<Channel>>,
}

impl TransactionInfoResource {
    pub fn new(grpc_client: Arc<WalletGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }

    /// Extract transaction ID from URI template
    fn extract_transaction_id(uri: &str) -> McpResult<u64> {
        // Expected format: transaction/{id}
        let parts: Vec<&str> = uri.split('/').collect();
        if parts.len() != 2 || parts[0] != "transaction" {
            return Err(McpError::invalid_request("Invalid transaction URI format. Expected: transaction/{id}"));
        }

        let tx_id_str = parts[1];
        if tx_id_str.starts_with("0x") {
            u64::from_str_radix(&tx_id_str[2..], 16)
                .map_err(|_| McpError::invalid_request("Invalid hex transaction ID"))
        } else {
            tx_id_str.parse::<u64>()
                .map_err(|_| McpError::invalid_request("Invalid transaction ID format"))
        }
    }
}

#[async_trait]
impl McpResource for TransactionInfoResource {
    fn uri(&self) -> &str {
        "transaction/{id}"
    }

    fn name(&self) -> &str {
        "Transaction Information"
    }

    fn description(&self) -> &str {
        "Detailed information about a specific transaction by ID"
    }

    fn mime_type(&self) -> &str {
        "application/json"
    }

    async fn read_with_uri(&self, uri: &str) -> McpResult<Value> {
        let tx_id = Self::extract_transaction_id(uri)?;
        
        let mut client = self.grpc_client.as_ref().clone();
        
        let response = client
            .get_transaction_info(GetTransactionInfoRequest {
                transaction_ids: vec![tx_id],
            })
            .await
            .map_err(|e| McpError::resource_access_failed(format!("Failed to get transaction info: {}", e)))?;

        let transactions = response.into_inner().transactions;
        
        if transactions.is_empty() {
            return Err(McpError::resource_not_found(format!("Transaction {} not found", tx_id)));
        }

        let tx = &transactions[0];

        Ok(serde_json::json!({
            "transaction_id": tx.tx_id,
            "source_public_key": tx.source_public_key,
            "destination_public_key": tx.destination_public_key,
            "amount": tx.amount,
            "fee": tx.fee,
            "excess_sig": tx.excess_sig,
            "timestamp": tx.timestamp,
            "message": tx.message,
            "status": tx.status,
            "direction": if tx.direction == 0 { "inbound" } else { "outbound" },
            "cancelled": tx.cancelled,
            "cancellation_reason": tx.cancellation_reason,
            "confirmations": tx.confirmations,
            "mined_height": tx.mined_height,
            "mined_in_block": tx.mined_in_block,
            "mined_timestamp": tx.mined_timestamp
        }))
    }

    async fn read(&self) -> McpResult<Value> {
        Err(McpError::invalid_request("Transaction info requires a transaction ID. Use: transaction/{id}"))
    }
}
