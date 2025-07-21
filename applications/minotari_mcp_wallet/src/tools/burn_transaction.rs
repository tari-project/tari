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
//! Burn transaction MCP tool

use minotari_mcp_common::{
    McpTool, McpResult, McpError, PermissionLevel,
    json_schema, get_required_u64_param
};
use minotari_wallet_grpc_client::{WalletGrpcClient, grpc::CreateBurnTransactionRequest};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tonic::transport::Channel;

/// Tool for creating burn transactions
pub struct BurnTransactionTool {
    grpc_client: Arc<WalletGrpcClient<Channel>>,
}

impl BurnTransactionTool {
    pub fn new(grpc_client: Arc<WalletGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait]
impl McpTool for BurnTransactionTool {
    fn name(&self) -> &str {
        "burn_transaction"
    }

    fn description(&self) -> &str {
        "Create a burn transaction to permanently destroy Tari. This is a control operation that permanently destroys funds."
    }

    fn permission_level(&self) -> PermissionLevel {
        PermissionLevel::Control
    }

    fn input_schema(&self) -> Value {
        json_schema! {
            "amount" => {
                "type": "number",
                "description": "Amount of microTari to burn (permanently destroy)"
            },
            "fee_per_gram" => {
                "type": "number",
                "description": "Fee per gram in microTari (optional)"
            },
            "message" => {
                "type": "string", 
                "description": "Optional message for the burn transaction"
            },
            "claim_public_key" => {
                "type": "string",
                "description": "Optional public key to claim burned funds (hex encoded)"
            }
        }
    }

    fn validate_params(&self, params: &Value) -> McpResult<()> {
        let amount = get_required_u64_param(params, "amount")?;

        if amount == 0 {
            return Err(McpError::invalid_request("Burn amount must be greater than 0"));
        }

        // Validate claim public key if provided
        if let Some(claim_key) = params.get("claim_public_key") {
            if let Some(key_str) = claim_key.as_str() {
                if !key_str.is_empty() {
                    // Basic hex validation
                    if !key_str.chars().all(|c| c.is_ascii_hexdigit()) {
                        return Err(McpError::invalid_request("Claim public key must be valid hex"));
                    }
                    if key_str.len() != 64 {
                        return Err(McpError::invalid_request("Claim public key must be 32 bytes (64 hex chars)"));
                    }
                }
            }
        }

        Ok(())
    }

    async fn execute(&self, params: Value) -> McpResult<Value> {
        let amount = get_required_u64_param(&params, "amount")?;
        let fee_per_gram = params.get("fee_per_gram").and_then(|v| v.as_u64()).unwrap_or(5);
        let message = params.get("message").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let claim_public_key = params.get("claim_public_key").and_then(|v| v.as_str()).map(|s| s.to_string());

        // Create burn request
        let request = CreateBurnTransactionRequest {
            amount,
            fee_per_gram,
            message,
            claim_public_key,
        };

        // Execute burn transaction
        let mut client = self.grpc_client.as_ref().clone();
        let response = client
            .create_burn_transaction(request)
            .await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to create burn transaction: {}", e)))?;

        let response = response.into_inner();

        Ok(serde_json::json!({
            "success": true,
            "transaction_id": response.transaction_id,
            "burned_amount": amount,
            "fee": response.fee,
            "commitment": response.commitment,
            "proof": response.ownership_proof,
            "message": "Burn transaction created successfully - funds permanently destroyed"
        }))
    }
}
