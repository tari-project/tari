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
//! Submit block MCP tool

use async_trait::async_trait;
use minotari_mcp_common::{
    MAX_ENCODED_BLOCK_HEX_LEN,
    McpError,
    McpResult,
    McpTool,
    get_required_string_param,
    node_rpc,
};
use minotari_node_grpc_client::BaseNodeGrpcClient;
use serde_json::Value;
use tonic::transport::Channel;

/// Tool for submitting blocks to the base node
pub struct SubmitBlockTool {
    grpc_client: BaseNodeGrpcClient<Channel>,
}

impl SubmitBlockTool {
    pub fn new(grpc_client: BaseNodeGrpcClient<Channel>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait]
impl McpTool for SubmitBlockTool {
    fn name(&self) -> &str {
        "submit_block"
    }

    fn description(&self) -> &str {
        "Submit a new block to the Tari blockchain. This is a control operation that modifies blockchain state."
    }

    fn permission_level(&self) -> minotari_mcp_common::security::PermissionLevel {
        minotari_mcp_common::security::PermissionLevel::Control
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "block_hex": {
                    "type": "string",
                    "description": "Hex of the protobuf-encoded tari.rpc.Block to submit",
                    "maxLength": MAX_ENCODED_BLOCK_HEX_LEN
                }
            },
            "required": ["block_hex"],
            "additionalProperties": false
        })
    }

    fn validate_params(&self, params: &Value) -> McpResult<()> {
        if params.as_object().is_some_and(|o| o.keys().any(|k| k != "block_hex")) {
            return Err(McpError::invalid_request("Only block_hex is accepted"));
        }
        let block_hex = get_required_string_param(params, "block_hex")?;

        if block_hex.is_empty() {
            return Err(McpError::invalid_request("Block hex cannot be empty"));
        }

        // Basic hex validation
        if !block_hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(McpError::invalid_request("Block hex contains invalid characters"));
        }

        if block_hex.len() % 2 != 0 {
            return Err(McpError::invalid_request("Block hex must have even length"));
        }

        Ok(())
    }

    async fn execute(&self, params: Value) -> McpResult<Value> {
        node_rpc::submit_block(self.grpc_client.clone(), &params).await
    }
}
