//! Payment info resource (templated)

use minotari_mcp_common::{McpResource, McpResult, McpError};
use minotari_wallet_grpc_client::{WalletGrpcClient, grpc::GetTransactionInfoRequest};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tonic::transport::Channel;

/// Resource providing payment information by payment ID
pub struct PaymentInfoResource {
    grpc_client: Arc<WalletGrpcClient<Channel>>,
}

impl PaymentInfoResource {
    pub fn new(grpc_client: Arc<WalletGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }

    /// Extract payment ID from URI template
    fn extract_payment_id(uri: &str) -> McpResult<String> {
        // Expected format: payment_id/{id}
        let parts: Vec<&str> = uri.split('/').collect();
        if parts.len() != 2 || parts[0] != "payment_id" {
            return Err(McpError::invalid_request("Invalid payment ID URI format. Expected: payment_id/{id}"));
        }

        let payment_id = parts[1].to_string();
        if payment_id.is_empty() {
            return Err(McpError::invalid_request("Payment ID cannot be empty"));
        }

        Ok(payment_id)
    }
}

#[async_trait]
impl McpResource for PaymentInfoResource {
    fn uri(&self) -> &str {
        "payment_id/{id}"
    }

    fn name(&self) -> &str {
        "Payment Information"
    }

    fn description(&self) -> &str {
        "Transaction information by payment ID or message"
    }

    fn mime_type(&self) -> &str {
        "application/json"
    }

    async fn read_with_uri(&self, uri: &str) -> McpResult<Value> {
        let payment_id = Self::extract_payment_id(uri)?;
        
        let mut client = self.grpc_client.as_ref().clone();
        
        // Search for transactions with matching payment ID/message
        // First try completed transactions
        let completed_response = client
            .get_completed_transactions(grpc::GetCompletedTransactionsRequest {
                limit: Some(1000), // Search more broadly
                offset: Some(0),
            })
            .await
            .map_err(|e| McpError::resource_access_failed(format!("Failed to get completed transactions: {}", e)))?;

        let completed_txs = completed_response.into_inner().transactions;
        
        // Look for transactions with matching message or containing the payment ID
        let mut matching_transactions = Vec::new();
        for tx in completed_txs {
            if tx.message.contains(&payment_id) || tx.payment_id == payment_id {
                matching_transactions.push(serde_json::json!({
                    "transaction_id": tx.tx_id,
                    "source_public_key": tx.source_public_key,
                    "destination_public_key": tx.destination_public_key,
                    "amount": tx.amount,
                    "fee": tx.fee,
                    "timestamp": tx.timestamp,
                    "message": tx.message,
                    "payment_id": tx.payment_id,
                    "status": "completed",
                    "direction": if tx.direction == 0 { "inbound" } else { "outbound" },
                    "confirmations": tx.confirmations,
                    "mined_height": tx.mined_height,
                    "mined_in_block": tx.mined_in_block
                }));
            }
        }

        // Also check pending transactions
        let pending_response = client
            .get_pending_inbound_transactions(grpc::GetPendingInboundTransactionsRequest {})
            .await
            .map_err(|e| McpError::resource_access_failed(format!("Failed to get pending transactions: {}", e)))?;

        let pending_txs = pending_response.into_inner().transactions;
        for tx in pending_txs {
            if tx.message.contains(&payment_id) {
                matching_transactions.push(serde_json::json!({
                    "transaction_id": tx.tx_id,
                    "source_public_key": tx.source_public_key,
                    "amount": tx.amount,
                    "timestamp": tx.timestamp,
                    "message": tx.message,
                    "status": "pending_inbound",
                    "direction": "inbound"
                }));
            }
        }

        if matching_transactions.is_empty() {
            return Err(McpError::resource_not_found(format!("No transactions found with payment ID: {}", payment_id)));
        }

        Ok(serde_json::json!({
            "payment_id": payment_id,
            "matching_transactions": matching_transactions,
            "transaction_count": matching_transactions.len(),
            "search_criteria": "Payment ID or message content",
            "last_updated": chrono::Utc::now().timestamp()
        }))
    }

    async fn read(&self) -> McpResult<Value> {
        Err(McpError::invalid_request("Payment info requires a payment ID. Use: payment_id/{id}"))
    }
}
