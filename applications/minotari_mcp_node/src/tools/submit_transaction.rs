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
//! Submit transaction MCP tool

use async_trait::async_trait;
use minotari_mcp_common::{McpError, McpResult, McpTool, get_required_string_param, node_rpc};
use minotari_node_grpc_client::BaseNodeGrpcClient;
use serde_json::Value;
use tonic::transport::Channel;

/// Tool for submitting transactions to the mempool
pub struct SubmitTransactionTool {
    grpc_client: BaseNodeGrpcClient<Channel>,
}

impl SubmitTransactionTool {
    pub fn new(grpc_client: BaseNodeGrpcClient<Channel>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait]
impl McpTool for SubmitTransactionTool {
    fn name(&self) -> &str {
        "submit_transaction"
    }

    fn description(&self) -> &str {
        "Submit a pre-signed transaction to the Tari mempool for inclusion in the blockchain. This tool accepts the \
         hex of a protobuf-encoded tari.rpc.Transaction and broadcasts it to the network. The transaction must be \
         properly formatted, signed, and have valid inputs. Once submitted, the transaction will be validated by nodes \
         and miners for inclusion in the next block. Use this for broadcasting transactions created offline or by \
         other applications."
    }

    fn permission_level(&self) -> minotari_mcp_common::security::PermissionLevel {
        minotari_mcp_common::security::PermissionLevel::Control
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "transaction_hex": {
                    "type": "string",
                    "description": "Hex of the protobuf-encoded tari.rpc.Transaction to submit (fully formed and signed).",
                    "pattern": "^[0-9a-fA-F]+$",
                    "minLength": 100
                }
            },
            "required": ["transaction_hex"]
        })
    }

    fn validate_params(&self, params: &Value) -> McpResult<()> {
        let tx_hex = get_required_string_param(params, "transaction_hex")?;

        if tx_hex.is_empty() {
            return Err(McpError::invalid_request("Transaction hex cannot be empty"));
        }

        // Basic hex validation
        if !tx_hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(McpError::invalid_request("Transaction hex contains invalid characters"));
        }

        if tx_hex.len() % 2 != 0 {
            return Err(McpError::invalid_request("Transaction hex must have even length"));
        }

        Ok(())
    }

    async fn execute(&self, params: Value) -> McpResult<Value> {
        node_rpc::submit_transaction(self.grpc_client.clone(), &params).await
    }
}
