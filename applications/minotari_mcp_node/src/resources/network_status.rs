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
//! Network status resource

use std::sync::Arc;

use async_trait::async_trait;
use minotari_mcp_common::{McpError, McpResource, McpResult};
use minotari_node_grpc_client::{grpc::Empty, BaseNodeGrpcClient};
use serde_json::Value;
use tonic::transport::Channel;

/// Resource providing current network status
pub struct NetworkStatusResource {
    grpc_client: Arc<BaseNodeGrpcClient<Channel>>,
}

impl NetworkStatusResource {
    pub fn new(grpc_client: Arc<BaseNodeGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait]
impl McpResource for NetworkStatusResource {
    fn uri(&self) -> &str {
        "network_status"
    }

    fn name(&self) -> &str {
        "Network Status"
    }

    fn description(&self) -> &str {
        "Current network connectivity status including peer connections and sync state"
    }

    fn mime_type(&self) -> &str {
        "application/json"
    }

    async fn read(&self) -> McpResult<Value> {
        let mut client = self.grpc_client.as_ref().clone();

        // Get version info to test connectivity
        let version_response = client
            .get_version(Empty {})
            .await
            .map_err(|e| McpError::resource_access_failed(format!("Failed to get version: {e}")))?;

        let version = version_response.into_inner();

        // TODO: Replace with actual network status calls when gRPC definitions are available
        Ok(serde_json::json!({
            "status": "online",
            "node_version": version.version,
            "network": "unknown",
            "message": "Network status information - placeholder implementation"
        }))
    }
}
