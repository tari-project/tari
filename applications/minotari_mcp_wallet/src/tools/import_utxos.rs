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
//! Import UTXOs MCP tool

use minotari_mcp_common::{
    McpTool, McpResult, McpError, PermissionLevel,
    json_schema, get_required_string_param
};
use minotari_wallet_grpc_client::{WalletGrpcClient, grpc::ImportUtxosRequest};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tonic::transport::Channel;

/// Tool for importing external UTXOs
pub struct ImportUtxosTool {
    grpc_client: Arc<WalletGrpcClient<Channel>>,
}

impl ImportUtxosTool {
    pub fn new(grpc_client: Arc<WalletGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait]
impl McpTool for ImportUtxosTool {
    fn name(&self) -> &str {
        "import_utxos"
    }

    fn description(&self) -> &str {
        "Import external UTXOs into the wallet. This is a control operation that can modify wallet state."
    }

    fn permission_level(&self) -> PermissionLevel {
        PermissionLevel::Control
    }

    fn input_schema(&self) -> Value {
        json_schema! {
            "outputs" => {
                "type": "array",
                "description": "Array of UTXO output hex strings to import",
                "items": {
                    "type": "string",
                    "description": "Hex-encoded UTXO output"
                }
            },
            "source_public_key" => {
                "type": "string",
                "description": "Public key of the source wallet (hex encoded)"
            }
        }
    }

    fn validate_params(&self, params: &Value) -> McpResult<()> {
        // Validate outputs array
        let outputs = params.get("outputs")
            .ok_or_else(|| McpError::invalid_request("outputs parameter is required"))?
            .as_array()
            .ok_or_else(|| McpError::invalid_request("outputs must be an array"))?;

        if outputs.is_empty() {
            return Err(McpError::invalid_request("At least one output must be provided"));
        }

        if outputs.len() > 100 {
            return Err(McpError::invalid_request("Cannot import more than 100 outputs at once"));
        }

        // Validate each output is a hex string
        for (i, output) in outputs.iter().enumerate() {
            let output_str = output.as_str()
                .ok_or_else(|| McpError::invalid_request(format!("Output {} must be a string", i)))?;
            
            if output_str.is_empty() {
                return Err(McpError::invalid_request(format!("Output {} cannot be empty", i)));
            }

            if !output_str.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(McpError::invalid_request(format!("Output {} contains invalid hex characters", i)));
            }

            if output_str.len() % 2 != 0 {
                return Err(McpError::invalid_request(format!("Output {} must have even length", i)));
            }
        }

        // Validate source public key
        let source_key = get_required_string_param(params, "source_public_key")?;
        if !source_key.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(McpError::invalid_request("Source public key must be valid hex"));
        }
        if source_key.len() != 64 {
            return Err(McpError::invalid_request("Source public key must be 32 bytes (64 hex chars)"));
        }

        Ok(())
    }

    async fn execute(&self, params: Value) -> McpResult<Value> {
        let outputs = params.get("outputs").unwrap().as_array().unwrap();
        let source_public_key = get_required_string_param(&params, "source_public_key")?;

        // Convert output hex strings to bytes
        let mut output_bytes = Vec::new();
        for output in outputs {
            let output_str = output.as_str().unwrap();
            let bytes = hex::decode(output_str)
                .map_err(|e| McpError::tool_execution_failed(format!("Invalid hex encoding: {}", e)))?;
            output_bytes.push(bytes);
        }

        // Create import request
        let request = ImportUtxosRequest {
            outputs: output_bytes,
            source_public_key,
        };

        // Execute import
        let mut client = self.grpc_client.as_ref().clone();
        let response = client
            .import_utxos(request)
            .await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to import UTXOs: {}", e)))?;

        let response = response.into_inner();

        Ok(serde_json::json!({
            "success": true,
            "imported_count": response.num_imported,
            "total_value": response.total_value,
            "message": format!("Successfully imported {} UTXOs with total value {} microTari", 
                             response.num_imported, response.total_value)
        }))
    }
}
