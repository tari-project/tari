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
//! Transfer MCP tool for sending Tari

use minotari_mcp_common::{
    McpTool, McpResult, McpError, PermissionLevel,
    json_schema, get_required_string_param, get_required_u64_param
};
use minotari_wallet_grpc_client::{WalletGrpcClient, grpc::TransferRequest};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tonic::transport::Channel;

/// Tool for transferring Tari to another address
pub struct TransferTool {
    grpc_client: Arc<WalletGrpcClient<Channel>>,
}

impl TransferTool {
    pub fn new(grpc_client: Arc<WalletGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait]
impl McpTool for TransferTool {
    fn name(&self) -> &str {
        "transfer"
    }

    fn description(&self) -> &str {
        "Transfer Tari to another address. This is a control operation that will send funds."
    }

    fn permission_level(&self) -> PermissionLevel {
        PermissionLevel::Control
    }

    fn input_schema(&self) -> Value {
        json_schema! {
            "recipient" => {
                "type": "string", 
                "description": "Destination address or emoji ID to send Tari to"
            },
            "amount" => {
                "type": "number",
                "description": "Amount of microTari to send" 
            },
            "fee_per_gram" => {
                "type": "number",
                "description": "Fee per gram in microTari (optional, uses default if not specified)"
            },
            "message" => {
                "type": "string",
                "description": "Optional message to include with the transaction"
            }
        }
    }

    fn validate_params(&self, params: &Value) -> McpResult<()> {
        let recipient = get_required_string_param(params, "recipient")?;
        let amount = get_required_u64_param(params, "amount")?;
        
        if recipient.is_empty() {
            return Err(McpError::invalid_request("Recipient cannot be empty"));
        }

        if amount == 0 {
            return Err(McpError::invalid_request("Amount must be greater than 0"));
        }

        // Optional fee validation
        if let Some(fee) = params.get("fee_per_gram") {
            if let Some(fee_val) = fee.as_u64() {
                if fee_val == 0 {
                    return Err(McpError::invalid_request("Fee per gram must be greater than 0 if specified"));
                }
            }
        }

        Ok(())
    }

    async fn execute(&self, params: Value) -> McpResult<Value> {
        let recipient = get_required_string_param(&params, "recipient")?;
        let amount = get_required_u64_param(&params, "amount")?;
        let fee_per_gram = params.get("fee_per_gram").and_then(|v| v.as_u64());
        let message = params.get("message").and_then(|v| v.as_str()).unwrap_or("").to_string();

        // Create transfer request
        let request = TransferRequest {
            recipients: vec![minotari_wallet_grpc_client::grpc::PaymentRecipient {
                address: recipient,
                amount,
                fee_per_gram: fee_per_gram.unwrap_or(5), // Default fee
                message,
            }],
        };

        // Execute transfer
        let mut client = self.grpc_client.as_ref().clone();
        let response = client
            .transfer(request)
            .await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to transfer: {e}")))?;

        let response = response.into_inner();

        // Return transaction details
        Ok(serde_json::json!({
            "success": true,
            "transaction_id": response.transaction_id,
            "fee": response.fee,
            "amount_sent": amount,
            "recipient": recipient,
            "message": "Transfer initiated successfully"
        }))
    }
}
