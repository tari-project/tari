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
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.//! Common MCP (Model Context Protocol) infrastructure for Tari applications
//! Transaction history resource

use minotari_mcp_common::{McpResource, McpResult, McpError};
use minotari_wallet_grpc_client::{WalletGrpcClient, grpc::GetTransactionInfoRequest};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tonic::transport::Channel;

/// Resource providing wallet transaction history
pub struct TransactionHistoryResource {
    grpc_client: Arc<WalletGrpcClient<Channel>>,
}

impl TransactionHistoryResource {
    pub fn new(grpc_client: Arc<WalletGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait]
impl McpResource for TransactionHistoryResource {
    fn uri(&self) -> &str {
        "transaction_history"
    }

    fn name(&self) -> &str {
        "Transaction History"
    }

    fn description(&self) -> &str {
        "Recent wallet transaction history including sent, received, and pending transactions"
    }

    fn mime_type(&self) -> &str {
        "application/json"
    }

    async fn read(&self) -> McpResult<Value> {
        let mut client = self.grpc_client.as_ref().clone();
        
        // Get completed transactions
        let completed_response = client
            .get_completed_transactions(grpc::GetCompletedTransactionsRequest {
                limit: Some(50), // Limit to recent 50 transactions
                offset: Some(0),
            })
            .await
            .map_err(|e| McpError::resource_access_failed(format!("Failed to get completed transactions: {e}")))?;

        // Get pending transactions
        let pending_response = client
            .get_pending_inbound_transactions(grpc::GetPendingInboundTransactionsRequest {})
            .await
            .map_err(|e| McpError::resource_access_failed(format!("Failed to get pending inbound transactions: {e}")))?;

        let outbound_pending_response = client
            .get_pending_outbound_transactions(grpc::GetPendingOutboundTransactionsRequest {})
            .await
            .map_err(|e| McpError::resource_access_failed(format!("Failed to get pending outbound transactions: {e}")))?;

        let completed_txs = completed_response.into_inner().transactions;
        let pending_inbound = pending_response.into_inner().transactions;
        let pending_outbound = outbound_pending_response.into_inner().transactions;

        // Process completed transactions
        let mut completed_transactions = Vec::new();
        for tx in completed_txs {
            completed_transactions.push(serde_json::json!({
                "transaction_id": tx.tx_id,
                "source_public_key": tx.source_public_key,
                "destination_public_key": tx.destination_public_key,
                "amount": tx.amount,
                "fee": tx.fee,
                "timestamp": tx.timestamp,
                "message": tx.message,
                "status": "completed",
                "direction": if tx.direction == 0 { "inbound" } else { "outbound" },
                "confirmations": tx.confirmations,
                "mined_height": tx.mined_height,
                "mined_in_block": tx.mined_in_block
            }));
        }

        // Process pending transactions
        let mut pending_transactions = Vec::new();
        
        for tx in pending_inbound {
            pending_transactions.push(serde_json::json!({
                "transaction_id": tx.tx_id,
                "source_public_key": tx.source_public_key,
                "amount": tx.amount,
                "timestamp": tx.timestamp,
                "message": tx.message,
                "status": "pending_inbound",
                "direction": "inbound"
            }));
        }

        for tx in pending_outbound {
            pending_transactions.push(serde_json::json!({
                "transaction_id": tx.tx_id,
                "destination_public_key": tx.destination_public_key,
                "amount": tx.amount,
                "fee": tx.fee,
                "timestamp": tx.timestamp,
                "message": tx.message,
                "status": "pending_outbound",
                "direction": "outbound"
            }));
        }

        Ok(serde_json::json!({
            "completed_transactions": completed_transactions,
            "pending_transactions": pending_transactions,
            "summary": {
                "total_completed": completed_transactions.len(),
                "total_pending": pending_transactions.len(),
                "last_updated": chrono::Utc::now().timestamp()
            }
        }))
    }
}
