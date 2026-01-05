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
//! Block information resource

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use minotari_mcp_common::{McpError, McpResource, McpResult};
use minotari_node_grpc_client::BaseNodeGrpcClient;
use serde_json::Value;
use tonic::transport::Channel;

/// Resource providing block information by height
pub struct BlockInfoResource {
    #[allow(dead_code)]
    grpc_client: Arc<BaseNodeGrpcClient<Channel>>,
}

impl BlockInfoResource {
    pub fn new(grpc_client: Arc<BaseNodeGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait]
impl McpResource for BlockInfoResource {
    fn uri(&self) -> &str {
        "block/{height}"
    }

    fn name(&self) -> &str {
        "Block Information"
    }

    fn description(&self) -> &str {
        "Information about a specific block by height"
    }

    fn mime_type(&self) -> &str {
        "application/json"
    }

    fn supports_templating(&self) -> bool {
        true
    }

    fn resolve_template(&self, params: &HashMap<String, String>) -> McpResult<String> {
        let height = params
            .get("height")
            .ok_or_else(|| McpError::invalid_request("Missing height parameter"))?;

        // Validate height is a number
        height
            .parse::<u64>()
            .map_err(|_| McpError::invalid_request("Height must be a valid number"))?;

        Ok(format!("block/{height}"))
    }

    async fn read(&self) -> McpResult<Value> {
        // This will be called with the resolved template
        // For now, return placeholder data
        Ok(serde_json::json!({
            "message": "Block information - placeholder implementation"
        }))
    }
}
