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
//! Transaction information resource

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use minotari_mcp_common::{McpError, McpResource, McpResult};
use minotari_node_grpc_client::BaseNodeGrpcClient;
use serde_json::Value;
use tonic::transport::Channel;

/// Resource providing transaction information by hash
pub struct TransactionInfoResource {
    #[allow(dead_code)]
    grpc_client: Arc<BaseNodeGrpcClient<Channel>>,
}

impl TransactionInfoResource {
    pub fn new(grpc_client: Arc<BaseNodeGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait]
impl McpResource for TransactionInfoResource {
    fn uri(&self) -> &str {
        "transaction/{hash}"
    }

    fn name(&self) -> &str {
        "Transaction Information"
    }

    fn description(&self) -> &str {
        "Information about a specific transaction by hash"
    }

    fn mime_type(&self) -> &str {
        "application/json"
    }

    fn supports_templating(&self) -> bool {
        true
    }

    fn resolve_template(&self, params: &HashMap<String, String>) -> McpResult<String> {
        let hash = params
            .get("hash")
            .ok_or_else(|| McpError::invalid_request("Missing hash parameter"))?;

        // Validate hash is hex
        if !hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(McpError::invalid_request("Hash must be hexadecimal"));
        }

        if hash.is_empty() {
            return Err(McpError::invalid_request("Hash cannot be empty"));
        }

        Ok(format!("transaction/{}", hash))
    }

    async fn read(&self) -> McpResult<Value> {
        // This will be called with the resolved template
        // For now, return placeholder data
        Ok(serde_json::json!({
            "message": "Transaction information - placeholder implementation"
        }))
    }
}
